//! Login, identidade e perfis isolados das contas, via Claude Code CLI.

use super::command;
use crate::application::{ports::AccountGateway, AppError, AppResult};
use crate::domain::account::{Identity, Provider};
use crate::infrastructure::cli_output::{failure_detail, run_login, CodeScope};
use async_trait::async_trait;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
/// Pastas compartilhadas com o perfil principal: qualquer conta retoma qualquer
/// conversa e enxerga as mesmas skills e agentes. Só as credenciais ficam separadas.
const SHARED_DIRS: [&str; 3] = ["projects", "skills", "agents"];

pub struct ClaudeCliAccounts {
    cli: PathBuf,
    profiles_dir: PathBuf,
    /// Diretório de configuração principal do Claude Code (`~/.claude`).
    main_config: PathBuf,
}

impl ClaudeCliAccounts {
    pub fn new(cli: PathBuf, profiles_dir: PathBuf, main_config: PathBuf) -> Self {
        ClaudeCliAccounts { cli, profiles_dir, main_config }
    }
}

/// Liga a pasta do perfil à compartilhada. Se o CLI já criou ali uma pasta comum (o link tinha
/// falhado), o conteúdo dela vai para a compartilhada antes de virar link; o que não puder ser
/// movido fica onde está, e a pasta não vira link.
pub(crate) fn link_dir(target: &Path, link: &Path) {
    if !target.exists() || link.read_link().is_ok() {
        return;
    }
    if link.is_dir() {
        merge_into(link, target);
        if std::fs::remove_dir(link).is_err() {
            return;
        }
    } else if link.exists() {
        return;
    }
    #[cfg(unix)]
    let _ = std::os::unix::fs::symlink(target, link);
    // O symlink no Windows exige administrador ou o modo de desenvolvedor; a junção, não.
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_dir(target, link).is_err() {
            let mut cmd = std::process::Command::new("cmd");
            cmd.arg("/C").arg("mklink").arg("/J").arg(link).arg(target);
            crate::infrastructure::hide_console(&mut cmd);
            let _ = cmd.output();
        }
    }
}

/// Move para `to` o que há em `from`, sem sobrescrever: pastas com o mesmo nome são mescladas.
fn merge_into(from: &Path, to: &Path) {
    let Ok(entries) = std::fs::read_dir(from) else { return };
    for entry in entries.flatten() {
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if !dst.exists() {
            let _ = std::fs::rename(&src, &dst);
        } else if src.is_dir() && dst.is_dir() {
            merge_into(&src, &dst);
            let _ = std::fs::remove_dir(&src);
        }
    }
}

/// Instante Unix de uma data ISO 8601 em UTC ("2026-12-12T03:41:12.461Z" ou "...+00:00").
pub(crate) fn parse_iso_utc(text: &str) -> Option<i64> {
    let (date, time) = text.split_once('T')?;
    let time = time.split_once('+').map_or(time, |(t, _)| t);
    let mut date = date.split('-').map(|n| n.parse::<i64>().ok());
    let (year, month, day) = (date.next()??, date.next()??, date.next()??);
    let time = time.trim_end_matches('Z');
    let mut time = time.split(':').map(|n| n.split('.').next()?.parse::<i64>().ok());
    let (hour, minute, second) = (time.next()??, time.next()??, time.next().flatten().unwrap_or(0));
    // Dias desde 1970-01-01 (algoritmo "days from civil").
    let (y, m) = if month <= 2 { (year - 1, month + 9) } else { (year, month - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}

#[async_trait]
impl AccountGateway for ClaudeCliAccounts {
    fn installed(&self, _provider: Provider) -> bool {
        self.cli.is_file()
    }

    fn prepare_profile(&self, _provider: Provider, account_id: &str) -> AppResult<String> {
        let dir = self.profiles_dir.join(account_id);
        let io = |e: std::io::Error| AppError(format!("Não foi possível criar o perfil da conta: {e}"));
        std::fs::create_dir_all(&dir).map_err(io)?;
        std::fs::create_dir_all(self.main_config.join("projects")).map_err(io)?;
        for shared in SHARED_DIRS {
            link_dir(&self.main_config.join(shared), &dir.join(shared));
        }
        Ok(dir.to_string_lossy().into_owned())
    }

    fn discard_profile(&self, _provider: Provider, config_dir: &str) {
        let dir = Path::new(config_dir);
        // Só remove perfis criados pelo app; os links são apagados sem seguir o destino.
        if dir.starts_with(&self.profiles_dir) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    async fn identity(&self, _provider: Provider, config_dir: Option<&str>) -> Identity {
        let output = command(&self.cli, config_dir).args(["auth", "status"]).output().await;
        let v: Value = output
            .ok()
            .and_then(|o| serde_json::from_slice(&o.stdout).ok())
            .unwrap_or(Value::Null);
        Identity {
            logged_in: v["loggedIn"].as_bool().unwrap_or(false),
            email: v["email"].as_str().map(Into::into),
            plan: v["subscriptionType"].as_str().map(Into::into),
            login_expires_at: v["loginExpiresAt"].as_str().and_then(parse_iso_utc),
        }
    }

    /// Abre o login oficial (no navegador) para o perfil indicado.
    async fn login(&self, provider: Provider, config_dir: &str) -> AppResult<Identity> {
        let mut cmd = command(&self.cli, Some(config_dir));
        cmd.args(["auth", "login", "--claudeai"]);
        // Sem console, o Claude Code no Windows não abre o navegador: o Lume abre.
        let run = run_login(cmd, true, Some(CodeScope::Account));
        let output = tokio::time::timeout(LOGIN_TIMEOUT, run)
            .await
            .map_err(|_| AppError("O login não foi concluído em 5 minutos.".into()))?
            .map_err(|e| AppError(format!("Não foi possível abrir o login: {e}")))?;
        let identity = self.identity(provider, Some(config_dir)).await;
        if identity.logged_in {
            return Ok(identity);
        }
        Err(AppError(format!("O login não foi concluído. {}", failure_detail(&output))))
    }
}

#[cfg(test)]
mod tests {
    use super::parse_iso_utc;

    #[test]
    fn parses_iso_dates_from_auth_status() {
        assert_eq!(parse_iso_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso_utc("2026-12-12T03:41:12.461Z"), Some(1_797_046_872));
        assert_eq!(parse_iso_utc("2026-11-13T01:27:24Z"), Some(1_794_533_244));
        assert_eq!(parse_iso_utc("ontem"), None);
    }
}
