//! Contas do ChatGPT pelo Codex CLI: login, perfis isolados (`CODEX_HOME`), turnos
//! (`codex exec --json`) e o uso que o próprio Codex grava nas sessões.

use super::{
    claude_cli::accounts::link_dir,
    providers::{cli_command, failure, find_executable, now_seconds, Prompt},
};
use crate::application::{
    ports::{AccountGateway, ChatEngine, TurnListener, UsageProbe},
    AppError, AppResult, TurnControl,
};
use crate::domain::{
    account::{Identity, Provider},
    chat::{RateInfo, TurnEnd, TurnEvent, TurnRequest},
};
use super::cli_output::{failure_detail, run_login};
use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
const AUTH_FILE: &str = "auth.json";
const CONFIG_FILE: &str = "config.toml";
/// Pasta das sessões, compartilhada com o perfil principal: qualquer conta do ChatGPT
/// retoma a conversa que outra começou.
const SESSIONS_DIR: &str = "sessions";
/// Janela de uso de até um dia conta como a "sessão"; acima disso, como a semana.
const SESSION_WINDOW_MINUTES: i64 = 1440;
/// Esforços que todo modelo do Codex aceita; os demais ficam com o padrão do CLI.
const EFFORTS: [&str; 3] = ["low", "medium", "high"];

/// Executável do Codex: o do PATH ou o que acompanha o aplicativo Codex.
pub fn find_cli() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["codex.exe", "codex.cmd"] } else { &["codex"] };
    find_executable(names, &[PathBuf::from("/Applications/Codex.app/Contents/Resources/codex")])
}

fn missing_cli() -> AppError {
    AppError("O Codex não está instalado neste computador.".into())
}

/// Pasta de configuração principal do Codex (`CODEX_HOME` ou `~/.codex`).
fn main_home() -> PathBuf {
    std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".codex"))
}

fn home_of(config_dir: Option<&str>) -> PathBuf {
    config_dir.map(PathBuf::from).unwrap_or_else(main_home)
}

/// Comando do Codex apontando para o perfil de uma conta (`None` = login do computador).
fn command(cli: &Path, config_dir: Option<&str>) -> Command {
    let mut cmd = cli_command(cli);
    if let Some(dir) = config_dir {
        cmd.env("CODEX_HOME", dir);
    }
    cmd
}

/// Identidade gravada pelo Codex em `auth.json`: o e-mail e o plano vêm no token de identidade.
pub(crate) fn parse_auth(text: &str) -> Identity {
    let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let filled = |value: &Value| value.as_str().is_some_and(|t| !t.is_empty());
    let claims: Value = v["tokens"]["id_token"]
        .as_str()
        .and_then(|token| token.split('.').nth(1))
        .and_then(|payload| URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null);
    let with_key = filled(&v["OPENAI_API_KEY"]);
    let plan = claims["https://api.openai.com/auth"]["chatgpt_plan_type"].as_str().map(str::to_owned);
    Identity {
        logged_in: filled(&v["tokens"]["access_token"]) || with_key,
        email: claims["email"].as_str().map(Into::into),
        plan: plan.or_else(|| with_key.then(|| "api".to_owned())),
        login_expires_at: None,
    }
}

pub struct CodexAccounts {
    profiles_dir: PathBuf,
}

impl CodexAccounts {
    pub fn new(profiles_dir: PathBuf) -> Self {
        CodexAccounts { profiles_dir }
    }
}

#[async_trait]
impl AccountGateway for CodexAccounts {
    fn installed(&self, _provider: Provider) -> bool {
        find_cli().is_some()
    }

    fn prepare_profile(&self, _provider: Provider, account_id: &str) -> AppResult<String> {
        let dir = self.profiles_dir.join(account_id);
        let io = |e: std::io::Error| AppError(format!("Não foi possível criar o perfil da conta: {e}"));
        let main = main_home();
        std::fs::create_dir_all(&dir).map_err(io)?;
        std::fs::create_dir_all(main.join(SESSIONS_DIR)).map_err(io)?;
        link_dir(&main.join(SESSIONS_DIR), &dir.join(SESSIONS_DIR));
        // A conta nova trabalha com o modelo e as preferências que o Codex já tem neste computador.
        if main.join(CONFIG_FILE).is_file() {
            let _ = std::fs::copy(main.join(CONFIG_FILE), dir.join(CONFIG_FILE));
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
        if let Ok(text) = std::fs::read_to_string(home_of(config_dir).join(AUTH_FILE)) {
            return parse_auth(&text);
        }
        // Sem o arquivo, a credencial pode estar no chaveiro do sistema: só o CLI sabe dizer.
        let Some(cli) = find_cli() else { return Identity::default() };
        let status = command(&cli, config_dir).args(["login", "status"]).stdin(Stdio::null()).output().await;
        Identity { logged_in: status.is_ok_and(|o| o.status.success()), ..Identity::default() }
    }

    /// Abre o login do ChatGPT (no navegador) para o perfil indicado.
    async fn login(&self, provider: Provider, config_dir: &str) -> AppResult<Identity> {
        let cli = find_cli().ok_or_else(missing_cli)?;
        let mut cmd = command(&cli, Some(config_dir));
        cmd.arg("login");
        // O Codex abre o navegador por conta própria mesmo sem console (abrir aqui duplicaria a aba)
        // e não pede código: a entrada fica vazia, como antes.
        let run = run_login(cmd, false, None);
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

/// O que o Codex pode fazer sozinho no turno. Fora do terminal ele não pergunta nada:
/// o que passa do permitido é recusado pelo próprio CLI.
fn sandbox(request: &TurnRequest) -> &'static str {
    // Conversa comum não mexe em arquivos; "default" é a sessão de agente em uma pasta.
    if request.tools != "default" {
        return "read-only";
    }
    match request.permission_mode.as_str() {
        "plan" => "read-only",
        "bypassPermissions" => "danger-full-access",
        _ => "workspace-write",
    }
}

/// Argumentos de `codex exec`; a mensagem vai pela entrada padrão.
fn exec_args(request: &TurnRequest, images: &[PathBuf]) -> Vec<String> {
    let mut args: Vec<String> = vec!["exec".into()];
    if !images.is_empty() {
        let list: Vec<String> = images.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        args.push(format!("--image={}", list.join(",")));
    }
    args.extend(["--json", "--skip-git-repo-check", "--color", "never", "--sandbox", sandbox(request)].map(String::from));
    args.extend(["--cd".to_owned(), request.cwd.clone()]);
    // Vazio: o modelo configurado no próprio Codex.
    if !request.model.is_empty() {
        args.extend(["--model".to_owned(), request.model.clone()]);
    }
    if EFFORTS.contains(&request.effort.as_str()) {
        args.extend(["--config".to_owned(), format!("model_reasoning_effort=\"{}\"", request.effort)]);
    }
    if let Some(session) = &request.resume {
        args.extend(["resume".to_owned(), session.clone()]);
    }
    args.push("-".into());
    args
}

/// Acompanha os eventos de `codex exec --json` e os traduz para os do turno.
#[derive(Default)]
pub(crate) struct CodexTurn {
    session_id: Option<String>,
    text: String,
    error: Option<String>,
    completed: bool,
    /// Itens cuja chamada de ferramenta já foi anunciada.
    announced: HashSet<String>,
}

impl CodexTurn {
    pub fn feed(&mut self, line: &str) -> Vec<TurnEvent> {
        let Ok(v) = serde_json::from_str::<Value>(line) else { return vec![] };
        match v["type"].as_str().unwrap_or_default() {
            "thread.started" => match v["thread_id"].as_str() {
                Some(id) => {
                    self.session_id = Some(id.to_owned());
                    vec![TurnEvent::Session(id.to_owned())]
                }
                None => vec![],
            },
            "item.started" | "item.updated" => self.item(&v["item"], false),
            "item.completed" => self.item(&v["item"], true),
            "turn.completed" => {
                self.completed = true;
                vec![]
            }
            "turn.failed" => {
                self.error = v["error"]["message"].as_str().map(str::to_owned).or(self.error.take());
                vec![]
            }
            "error" => {
                self.error = v["message"].as_str().map(str::to_owned);
                vec![]
            }
            _ => vec![],
        }
    }

    fn item(&mut self, item: &Value, done: bool) -> Vec<TurnEvent> {
        let id = item["id"].as_str().unwrap_or_default().to_owned();
        let text = |key: &str| item[key].as_str().unwrap_or_default().to_owned();
        let (name, input, output, is_error) = match item["type"].as_str().unwrap_or_default() {
            "agent_message" if done => {
                // Cada mensagem chega inteira; uma linha em branco separa as do mesmo turno.
                let piece = if self.text.is_empty() { text("text") } else { format!("\n\n{}", text("text")) };
                self.text.push_str(&piece);
                return vec![TurnEvent::Delta(piece)];
            }
            "reasoning" if done => return vec![TurnEvent::Thinking(text("text"))],
            "error" if done => {
                self.error = Some(text("message"));
                return vec![];
            }
            "command_execution" => {
                let failed = item["exit_code"].as_i64().is_some_and(|code| code != 0) || item["status"] == "failed";
                ("Bash".to_owned(), json!({ "command": text("command") }), text("aggregated_output"), failed)
            }
            "file_change" => {
                let paths: Vec<&str> = item["changes"].as_array().into_iter().flatten().filter_map(|c| c["path"].as_str()).collect();
                let input = json!({ "file_path": paths.join(", "), "changes": item["changes"] });
                ("Edit".to_owned(), input, String::new(), item["status"] == "failed")
            }
            "mcp_tool_call" => {
                let name = format!("mcp__{}__{}", text("server"), text("tool"));
                let failed = !item["error"].is_null() || item["status"] == "failed";
                let output = if failed { item["error"]["message"].as_str().unwrap_or_default().to_owned() } else { item["result"].to_string() };
                (name, item["arguments"].clone(), output, failed)
            }
            "web_search" => ("WebSearch".to_owned(), json!({ "query": text("query") }), String::new(), false),
            _ => return vec![],
        };
        let mut events = vec![];
        if self.announced.insert(id.clone()) {
            events.push(TurnEvent::ToolCall { id: id.clone(), name, input: input.to_string() });
        }
        if done {
            events.push(TurnEvent::ToolResult { id, output, is_error });
        }
        events
    }

    pub fn into_end(self, stderr: &str, now: i64) -> TurnEnd {
        if self.completed {
            return TurnEnd::Done { session_id: self.session_id, text: self.text };
        }
        failure(self.error.as_deref().unwrap_or(stderr), now)
    }
}

pub struct CodexEngine;

#[async_trait]
impl ChatEngine for CodexEngine {
    async fn run_turn(&self, request: &TurnRequest, control: &mut TurnControl, on_event: TurnListener<'_>) -> TurnEnd {
        let Some(cli) = find_cli() else { return TurnEnd::Failed(missing_cli().0) };
        let prompt = Prompt::compose(request);
        let mut child = match command(&cli, request.config_dir.as_deref())
            .args(exec_args(request, &prompt.images))
            .current_dir(&request.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => return TurnEnd::Failed(format!("Não foi possível iniciar o Codex: {e}")),
        };
        // A mensagem inteira vai pela entrada padrão; fechá-la avisa que acabou.
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(prompt.text.as_bytes()).await;
        }
        let stderr = child.stderr.take().map(|mut pipe| {
            tokio::spawn(async move {
                let mut text = String::new();
                let _ = pipe.read_to_string(&mut text).await;
                text
            })
        });
        let Some(stdout) = child.stdout.take() else {
            return TurnEnd::Failed("Não foi possível ler a saída do Codex.".into());
        };

        let mut lines = BufReader::new(stdout).lines();
        let mut turn = CodexTurn::default();
        loop {
            tokio::select! {
                _ = control.cancel.notified() => {
                    let _ = child.kill().await;
                    return TurnEnd::Cancelled;
                }
                line = lines.next_line() => {
                    let Ok(Some(line)) = line else { break };
                    for event in turn.feed(&line) {
                        on_event(&event);
                    }
                }
            }
        }
        let _ = child.wait().await;
        let stderr = match stderr {
            Some(task) => task.await.unwrap_or_default(),
            None => String::new(),
        };

        // O Codex grava o uso da conta no arquivo da sessão, não na saída do turno.
        let now = now_seconds();
        let home = home_of(request.config_dir.as_deref());
        let session = turn.session_id.clone();
        let rate = tokio::task::spawn_blocking(move || recorded_rate(&home, session.as_deref(), now)).await.ok().flatten();
        if let Some(rate) = &rate {
            on_event(&TurnEvent::Rate(rate.clone()));
        }
        match turn.into_end(&stderr, now) {
            TurnEnd::Limited { resets_at } => {
                TurnEnd::Limited { resets_at: resets_at.or(rate.and_then(|r| r.resets_at)) }
            }
            end => end,
        }
    }
}

/// Janelas de uso de um evento `token_count` (`rate_limits.primary`/`secondary`, com
/// `used_percent`, `window_minutes` e `resets_at`) no formato interno. Janela que já zerou conta como vazia.
pub(crate) fn parse_rate(rate_limits: &Value, now: i64) -> Option<RateInfo> {
    let mut rate = RateInfo { status: "allowed".into(), resets_at: None, util_5h: None, util_7d: None, resets_5h: None, resets_7d: None };
    for window in [&rate_limits["primary"], &rate_limits["secondary"]] {
        let Some(percent) = window["used_percent"].as_f64() else { continue };
        let resets = window["resets_at"].as_i64().or_else(|| window["resets_in_seconds"].as_i64().map(|s| now + s));
        let used = if resets.is_some_and(|t| t <= now) { 0.0 } else { percent / 100.0 };
        if used >= 1.0 {
            rate.status = "rejected".into();
            rate.resets_at = rate.resets_at.max(resets);
        }
        if window["window_minutes"].as_i64().unwrap_or(0) <= SESSION_WINDOW_MINUTES {
            (rate.util_5h, rate.resets_5h) = (Some(used), resets);
        } else {
            (rate.util_7d, rate.resets_7d) = (Some(used), resets);
        }
    }
    (rate.util_5h.is_some() || rate.util_7d.is_some()).then_some(rate)
}

/// Arquivos de sessão do perfil (`sessions/ano/mês/dia/rollout-…-<id>.jsonl`), do mais novo ao mais antigo.
fn session_files(home: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() && depth > 0 {
                walk(&path, depth - 1, found);
            } else if path.extension().is_some_and(|e| e == "jsonl") {
                found.push(path);
            }
        }
    }
    let mut found = vec![];
    walk(&home.join(SESSIONS_DIR), 3, &mut found);
    // O caminho começa pela data, então a ordem alfabética é a cronológica.
    found.sort();
    found.reverse();
    found
}

/// Uso mais recente gravado em um arquivo de sessão.
fn rate_in_file(path: &Path, now: i64) -> Option<RateInfo> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().rev().find_map(|line| {
        let v: Value = serde_json::from_str(line).ok()?;
        (v["payload"]["type"] == "token_count").then(|| parse_rate(&v["payload"]["rate_limits"], now)).flatten()
    })
}

/// Uso da conta segundo o que o Codex gravou: na sessão indicada ou, sem ela, nas mais recentes.
fn recorded_rate(home: &Path, session: Option<&str>, now: i64) -> Option<RateInfo> {
    const RECENT: usize = 5;
    let files = session_files(home);
    match session {
        Some(id) => {
            let suffix = format!("{id}.jsonl");
            let file = files.iter().find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(&suffix)))?;
            rate_in_file(file, now)
        }
        None => files.iter().take(RECENT).find_map(|p| rate_in_file(p, now)),
    }
}

/// Uso da conta do ChatGPT. O Codex só o informa durante um turno, então vale o último que ele gravou.
pub struct CodexUsage;

#[async_trait]
impl UsageProbe for CodexUsage {
    async fn usage(&self, _provider: Provider, config_dir: Option<&str>) -> AppResult<RateInfo> {
        let home = home_of(config_dir);
        tokio::task::spawn_blocking(move || recorded_rate(&home, None, now_seconds()))
            .await
            .ok()
            .flatten()
            .ok_or_else(|| AppError("O Codex ainda não registrou o uso desta conta. Ele aparece depois da primeira mensagem.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::providers::sample_request;

    fn events(lines: &[&str]) -> (Vec<TurnEvent>, CodexTurn) {
        let mut turn = CodexTurn::default();
        let events = lines.iter().flat_map(|l| turn.feed(l)).collect();
        (events, turn)
    }

    #[test]
    fn reads_email_and_plan_from_the_identity_token() {
        let claims = json!({"email": "a@exemplo.com", "https://api.openai.com/auth": {"chatgpt_plan_type": "plus"}});
        let token = format!("cabecalho.{}.assinatura", URL_SAFE_NO_PAD.encode(claims.to_string()));
        let auth = json!({"OPENAI_API_KEY": null, "tokens": {"id_token": token, "access_token": "abc"}});
        let identity = parse_auth(&auth.to_string());
        assert!(identity.logged_in);
        assert_eq!((identity.email.as_deref(), identity.plan.as_deref()), (Some("a@exemplo.com"), Some("plus")));

        let with_key = parse_auth(r#"{"OPENAI_API_KEY":"sk-x","tokens":null}"#);
        assert_eq!((with_key.logged_in, with_key.plan.as_deref()), (true, Some("api")));
        assert!(!parse_auth(r#"{"tokens":{"access_token":""}}"#).logged_in);
        assert!(!parse_auth("lixo").logged_in);
    }

    #[test]
    fn translates_a_turn_with_command_and_answer() {
        let (events, turn) = events(&[
            r#"{"type":"thread.started","thread_id":"0199-abc"}"#,
            r#"{"type":"turn.started"}"#,
            r#"{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"pensando"}}"#,
            r#"{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"bash -lc ls","aggregated_output":"","exit_code":null,"status":"in_progress"}}"#,
            r#"{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"bash -lc ls","aggregated_output":"a.txt\n","exit_code":0,"status":"completed"}}"#,
            r#"{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"Há um arquivo."}}"#,
            r#"{"type":"item.completed","item":{"id":"item_3","type":"agent_message","text":"Pronto."}}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":2}}"#,
        ]);
        assert_eq!(events[0], TurnEvent::Session("0199-abc".into()));
        assert_eq!(events[1], TurnEvent::Thinking("pensando".into()));
        assert_eq!(events[2], TurnEvent::ToolCall { id: "item_1".into(), name: "Bash".into(), input: r#"{"command":"bash -lc ls"}"#.into() });
        assert_eq!(events[3], TurnEvent::ToolResult { id: "item_1".into(), output: "a.txt\n".into(), is_error: false });
        assert_eq!(events.len(), 6, "a chamada é anunciada uma vez só");
        let end = turn.into_end("", 0);
        assert_eq!(end, TurnEnd::Done { session_id: Some("0199-abc".into()), text: "Há um arquivo.\n\nPronto.".into() });
    }

    #[test]
    fn file_changes_and_connector_calls_become_tool_uses() {
        let (events, _) = events(&[
            r#"{"type":"item.completed","item":{"id":"i1","type":"file_change","changes":[{"path":"src/a.rs","kind":"update"}],"status":"completed"}}"#,
            r#"{"type":"item.completed","item":{"id":"i2","type":"mcp_tool_call","server":"linear","tool":"list","arguments":{"q":"x"},"result":null,"error":{"message":"sem acesso"},"status":"failed"}}"#,
            r#"{"type":"item.completed","item":{"id":"i3","type":"command_execution","command":"false","aggregated_output":"","exit_code":1,"status":"failed"}}"#,
        ]);
        assert!(matches!(&events[0], TurnEvent::ToolCall { name, input, .. } if name == "Edit" && input.contains(r#""file_path":"src/a.rs""#)));
        assert!(matches!(&events[2], TurnEvent::ToolCall { name, .. } if name == "mcp__linear__list"));
        assert_eq!(events[3], TurnEvent::ToolResult { id: "i2".into(), output: "sem acesso".into(), is_error: true });
        assert!(matches!(&events[5], TurnEvent::ToolResult { is_error: true, .. }));
    }

    #[test]
    fn failed_turn_is_limit_login_or_error() {
        let limit = r#"{"type":"turn.failed","error":{"message":"You've hit your usage limit. Try again in 3 hours."}}"#;
        assert_eq!(events(&[limit]).1.into_end("", 100), TurnEnd::Limited { resets_at: Some(100 + 3 * 3600) });
        let other = [r#"{"type":"error","message":"stream disconnected"}"#, r#"{"type":"turn.failed","error":{"message":"modelo inexistente"}}"#];
        assert_eq!(events(&other).1.into_end("", 0), TurnEnd::Failed("modelo inexistente".into()));
        assert!(matches!(events(&[]).1.into_end("Not logged in. Run codex login.", 0), TurnEnd::AuthError(_)));
    }

    #[test]
    fn exec_arguments_follow_the_conversation() {
        let mut request = sample_request(Provider::ChatGpt);
        request.cwd = "/tmp/pasta".into();
        let args = exec_args(&request, &[]);
        assert_eq!(args[..2], ["exec", "--json"]);
        assert!(args.windows(2).any(|w| w == ["--sandbox", "read-only"]), "conversa comum não altera arquivos");
        assert!(args.windows(2).any(|w| w == ["--cd", "/tmp/pasta"]));
        assert!(args.contains(&r#"model_reasoning_effort="medium""#.to_owned()));
        assert_eq!(args.last().map(String::as_str), Some("-"));
        assert!(!args.contains(&"resume".to_owned()) && !args.iter().any(|a| a == "--model"), "sem escolha, o modelo é o do Codex");
        request.model = "gpt-forte".into();
        assert!(exec_args(&request, &[]).windows(2).any(|w| w == ["--model", "gpt-forte"]));
        request.model.clear();

        request.tools = "default".into();
        request.effort = "max".into();
        request.resume = Some("0199-abc".into());
        let args = exec_args(&request, &[PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")]);
        assert_eq!(args[1], "--image=/tmp/a.png,/tmp/b.png");
        assert!(args.windows(2).any(|w| w == ["--sandbox", "workspace-write"]));
        assert!(!args.iter().any(|a| a.starts_with("model_reasoning_effort")), "esforço fora da lista fica com o padrão do CLI");
        assert_eq!(args[args.len() - 3..], ["resume", "0199-abc", "-"]);

        request.permission_mode = "plan".into();
        assert_eq!(sandbox(&request), "read-only");
        request.permission_mode = "bypassPermissions".into();
        assert_eq!(sandbox(&request), "danger-full-access");
    }

    /// Formato conferido em uma sessão gravada pelo Codex 0.95.
    #[test]
    fn usage_windows_are_sorted_by_their_length() {
        let weekly_only = json!({"primary": {"used_percent": 8.0, "window_minutes": 10080, "resets_at": 5000}, "secondary": null});
        let rate = parse_rate(&weekly_only, 1000).unwrap();
        assert_eq!((rate.util_5h, rate.util_7d, rate.resets_7d), (None, Some(0.08), Some(5000)));

        let both = json!({"primary": {"used_percent": 100.0, "window_minutes": 300, "resets_in_seconds": 60}, "secondary": {"used_percent": 40.0, "window_minutes": 10080, "resets_at": 9000}});
        let rate = parse_rate(&both, 1000).unwrap();
        assert!(rate.is_limited());
        assert_eq!((rate.util_5h, rate.resets_5h, rate.resets_at, rate.util_7d), (Some(1.0), Some(1060), Some(1060), Some(0.4)));

        assert_eq!(parse_rate(&weekly_only, 6000).unwrap().util_7d, Some(0.0), "janela que já zerou conta como vazia");
        assert!(parse_rate(&json!({"primary": null}), 0).is_none());
    }

    #[test]
    fn finds_the_usage_recorded_for_a_session() {
        let home = std::env::temp_dir().join(format!("lume-codex-{}", uuid::Uuid::new_v4()));
        let day = home.join("sessions/2026/11/13");
        std::fs::create_dir_all(&day).unwrap();
        let line = |percent: f64| {
            json!({"type": "event_msg", "payload": {"type": "token_count", "rate_limits": {"primary": {"used_percent": percent, "window_minutes": 300, "resets_at": 9000}}}}).to_string()
        };
        std::fs::write(day.join("rollout-2026-11-13T09-00-00-aaa.jsonl"), format!("{}\n{}\n", line(10.0), line(20.0))).unwrap();
        std::fs::write(day.join("rollout-2026-11-13T10-00-00-bbb.jsonl"), "{\"type\":\"session_meta\"}\n").unwrap();

        assert_eq!(recorded_rate(&home, Some("aaa"), 0).unwrap().util_5h, Some(0.2), "vale o último registro da sessão");
        assert!(recorded_rate(&home, Some("bbb"), 0).is_none());
        assert_eq!(recorded_rate(&home, None, 0).unwrap().util_5h, Some(0.2), "sem sessão, procura nas mais recentes");
        std::fs::remove_dir_all(home).unwrap();
    }

    #[tokio::test]
    #[ignore = "consome uso da conta real do ChatGPT"]
    async fn real_codex_answers_and_resumes() {
        let engine = CodexEngine;
        let mut request = sample_request(Provider::ChatGpt);
        request.prompt = "Guarde o número 417. Responda apenas: ok".into();
        let first = engine.run_turn(&request, &mut TurnControl::new().0, &mut |_| {}).await;
        let TurnEnd::Done { session_id: Some(session), .. } = first else { panic!("turno falhou: {first:?}") };
        request.resume = Some(session);
        request.prompt = "Qual número pedi para guardar? Responda só o número.".into();
        let second = engine.run_turn(&request, &mut TurnControl::new().0, &mut |_| {}).await;
        let TurnEnd::Done { text, .. } = second else { panic!("retomada falhou: {second:?}") };
        assert!(text.contains("417"), "a conversa retomada mantém o contexto: {text}");
    }
}
