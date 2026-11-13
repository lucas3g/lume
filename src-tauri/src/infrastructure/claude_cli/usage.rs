//! Uso atual de uma conta. O caminho principal consulta o provedor com o token OAuth
//! que o Claude Code guardou (sem gastar cota); quando não há token válido, cai no
//! turno mínimo do CLI, que renova o token e traz o evento de limite.

use super::{accounts::parse_iso_utc, command, credentials::read_token, stream::parse_line};
use crate::application::{ports::UsageProbe, AppError, AppResult};
use crate::domain::{
    account::{Provider, UsageLimit},
    chat::{RateInfo, TurnEvent},
};
use async_trait::async_trait;
use serde_json::Value;
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, BufReader};

const TIMEOUT: Duration = Duration::from_secs(60);
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
/// O menor turno possível: modelo barato, sem ferramentas, sem raciocínio.
const PROBE_PROMPT: &str = "Responda apenas: ok";
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";

/// Consulta pelo token OAuth gravado pelo Claude Code, com o CLI como reserva.
pub struct OauthUsage {
    default_config: PathBuf,
    fallback: ClaudeCliUsage,
}

impl OauthUsage {
    pub fn new(cli: PathBuf, default_config: PathBuf) -> Self {
        OauthUsage { default_config, fallback: ClaudeCliUsage::new(cli) }
    }
}

impl OauthUsage {
    /// Resposta do provedor para a conta; `None` quando não há token aceito e resta o CLI.
    async fn body(&self, config_dir: Option<&str>) -> AppResult<Option<Value>> {
        let dir = config_dir.map(str::to_string);
        let default_config = self.default_config.clone();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let token = tokio::task::spawn_blocking(move || read_token(dir.as_deref(), &default_config))
            .await
            .ok()
            .flatten()
            .filter(|t| t.is_valid_at(now));
        let Some(token) = token else {
            return Ok(None);
        };
        match tokio::task::spawn_blocking(move || fetch_usage(&token.access_token)).await {
            Ok(Ok(body)) => Ok(Some(body)),
            // Token recusado: o CLI renova as credenciais e ainda informa o uso.
            Ok(Err(Fetch::Unauthorized)) => Ok(None),
            Ok(Err(Fetch::Failed(msg))) => Err(AppError(msg)),
            Err(e) => Err(AppError(format!("Não foi possível consultar o uso: {e}"))),
        }
    }
}

#[async_trait]
impl UsageProbe for OauthUsage {
    async fn usage(&self, provider: Provider, config_dir: Option<&str>) -> AppResult<RateInfo> {
        match self.body(config_dir).await? {
            Some(body) => parse_usage(&body).ok_or_else(no_answer),
            None => self.fallback.usage(provider, config_dir).await,
        }
    }

    async fn limits(&self, provider: Provider, config_dir: Option<&str>) -> AppResult<Vec<UsageLimit>> {
        match self.body(config_dir).await? {
            Some(body) => Some(parse_limits(&body)).filter(|l| !l.is_empty()).ok_or_else(no_answer),
            None => self.fallback.limits(provider, config_dir).await,
        }
    }
}

fn no_answer() -> AppError {
    AppError("O provedor não informou o uso desta conta.".into())
}

enum Fetch {
    Unauthorized,
    Failed(String),
}

fn fetch_usage(access_token: &str) -> Result<Value, Fetch> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(HTTP_TIMEOUT)).build().into();
    let mut response = agent
        .get(USAGE_URL)
        .header("Authorization", &format!("Bearer {access_token}"))
        .header("anthropic-beta", OAUTH_BETA)
        .header("Accept", "application/json")
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(401 | 403) => Fetch::Unauthorized,
            ureq::Error::StatusCode(429) => Fetch::Failed("O provedor pediu para aguardar antes de consultar o uso de novo.".into()),
            other => Fetch::Failed(format!("Não foi possível consultar o uso: {other}")),
        })?;
    response.body_mut().read_json().map_err(|e| Fetch::Failed(format!("Resposta de uso inválida: {e}")))
}

/// Lista `limits` da resposta (sessão, semana e semana por modelo, com `percent` de 0 a 100).
/// Sem ela, monta sessão e semana a partir das janelas `five_hour`/`seven_day`.
pub(crate) fn parse_limits(body: &Value) -> Vec<UsageLimit> {
    let listed: Vec<UsageLimit> = body["limits"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| {
            Some(UsageLimit {
                kind: l["kind"].as_str()?.to_string(),
                model: l["scope"]["model"]["display_name"].as_str().map(str::to_string),
                used: l["percent"].as_f64()? / 100.0,
                resets_at: l["resets_at"].as_str().and_then(parse_iso_utc),
            })
        })
        .collect();
    if !listed.is_empty() {
        return listed;
    }
    parse_usage(body).map(|r| UsageLimit::from_windows(r.util_5h, r.resets_5h, r.util_7d, r.resets_7d)).unwrap_or_default()
}

/// Converte a resposta do provedor (`five_hour`/`seven_day` com `utilization` em
/// porcentagem e `resets_at` ISO) no formato interno (fração 0–1 e instante Unix).
pub(crate) fn parse_usage(body: &Value) -> Option<RateInfo> {
    let window = |name: &str| {
        let w = &body[name];
        (w["utilization"].as_f64().map(|p| p / 100.0), w["resets_at"].as_str().and_then(parse_iso_utc))
    };
    let (util_5h, resets_5h) = window("five_hour");
    let (util_7d, resets_7d) = window("seven_day");
    util_5h?;
    let limited = util_5h >= Some(1.0) || util_7d >= Some(1.0);
    Some(RateInfo {
        status: if limited { "rejected".into() } else { "allowed".into() },
        resets_at: if limited { resets_5h.or(resets_7d) } else { None },
        util_5h,
        util_7d,
        resets_5h,
        resets_7d,
    })
}

/// Reserva: um turno mínimo do CLI, só para receber o evento de limite que o
/// provedor manda no início de toda chamada.
pub struct ClaudeCliUsage {
    cli: PathBuf,
}

impl ClaudeCliUsage {
    pub fn new(cli: PathBuf) -> Self {
        ClaudeCliUsage { cli }
    }
}

#[async_trait]
impl UsageProbe for ClaudeCliUsage {
    async fn usage(&self, _provider: Provider, config_dir: Option<&str>) -> AppResult<RateInfo> {
        let mut child = command(&self.cli, config_dir)
            .args(["-p", "--output-format", "stream-json", "--verbose", "--model", "haiku", "--effort", "low", "--tools", "", "--max-turns", "1", PROBE_PROMPT])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| AppError(format!("Não foi possível consultar o uso: {e}")))?;
        let stdout = child.stdout.take().ok_or_else(|| AppError("Sem saída do Claude Code.".into()))?;
        let mut lines = BufReader::new(stdout).lines();
        let read = async {
            while let Ok(Some(line)) = lines.next_line().await {
                for event in parse_line(&line) {
                    if let TurnEvent::Rate(rate) = event {
                        return Some(rate);
                    }
                }
            }
            None
        };
        let found = tokio::time::timeout(TIMEOUT, read).await;
        let _ = child.kill().await;
        match found {
            Ok(Some(rate)) => Ok(rate),
            Ok(None) => Err(AppError("O provedor não informou o uso desta conta.".into())),
            Err(_) => Err(AppError("A consulta de uso demorou demais.".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::claude_cli::find_cli;
    use serde_json::json;

    #[test]
    fn parses_provider_windows_into_fractions_and_instants() {
        let body = json!({
            "five_hour": {"utilization": 43.0, "resets_at": "2026-11-13T11:29:59.635819+00:00"},
            "seven_day": {"utilization": 40.0, "resets_at": "2026-11-17T08:59:59.635839+00:00"}
        });
        let rate = parse_usage(&body).unwrap();
        assert_eq!(rate.status, "allowed");
        assert_eq!((rate.util_5h, rate.util_7d), (Some(0.43), Some(0.40)));
        assert_eq!((rate.resets_5h, rate.resets_7d), (Some(1794569399), Some(1794905999)));
        assert_eq!(rate.resets_at, None);
    }

    #[test]
    fn full_window_counts_as_limited() {
        let body = json!({"five_hour": {"utilization": 100.0, "resets_at": "2026-11-13T11:29:59+00:00"}, "seven_day": {"utilization": 12.0}});
        let rate = parse_usage(&body).unwrap();
        assert!(rate.is_limited());
        assert_eq!(rate.resets_at, Some(1794569399));
    }

    #[test]
    fn parses_listed_limits_with_the_model_they_apply_to() {
        let body = json!({"limits": [
            {"kind": "session", "percent": 14, "resets_at": "2026-11-13T11:29:59+00:00", "scope": null},
            {"kind": "weekly_all", "percent": 40, "resets_at": null, "scope": null},
            {"kind": "weekly_scoped", "percent": 71, "resets_at": "2026-11-17T08:59:59.5+00:00", "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null}}
        ]});
        let limits = parse_limits(&body);
        assert_eq!(limits.len(), 3);
        assert_eq!((limits[0].kind.as_str(), limits[0].used, limits[0].resets_at), ("session", 0.14, Some(1794569399)));
        assert_eq!((limits[1].model.as_deref(), limits[1].resets_at), (None, None));
        assert_eq!((limits[2].model.as_deref(), limits[2].used, limits[2].resets_at), (Some("Fable"), 0.71, Some(1794905999)));
    }

    #[test]
    fn limits_fall_back_to_the_two_windows() {
        let body = json!({"five_hour": {"utilization": 43.0, "resets_at": "2026-11-13T11:29:59+00:00"}, "seven_day": {"utilization": 40.0}});
        let limits = parse_limits(&body);
        assert_eq!(limits.iter().map(|l| (l.kind.as_str(), l.used)).collect::<Vec<_>>(), [("session", 0.43), ("weekly_all", 0.40)]);
        assert!(parse_limits(&json!({})).is_empty());
    }

    #[test]
    fn missing_five_hour_window_is_no_answer() {
        assert!(parse_usage(&json!({"seven_day": {"utilization": 1.0}})).is_none());
    }

    #[tokio::test]
    #[ignore = "depende do login real do computador"]
    async fn real_oauth_probe_answers_without_a_cli_turn() {
        let probe = OauthUsage::new(find_cli().expect("Claude Code instalado"), dirs::home_dir().unwrap().join(".claude"));
        let started = std::time::Instant::now();
        let rate = probe.usage(Provider::Claude, None).await.unwrap();
        assert!(rate.util_5h.is_some() && rate.resets_5h.is_some() && rate.util_7d.is_some());
        assert!(started.elapsed() < Duration::from_secs(3), "caiu no turno do CLI: {:?}", started.elapsed());
    }

    #[tokio::test]
    #[ignore = "consome uso da conta real"]
    async fn real_cli_reports_usage_windows() {
        let probe = ClaudeCliUsage::new(find_cli().expect("Claude Code instalado"));
        let rate = probe.usage(Provider::Claude, None).await.unwrap();
        assert!(rate.util_5h.is_some() && rate.resets_5h.is_some());
    }
}
