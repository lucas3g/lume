//! Conta Google (Gemini) pelo Antigravity CLI (`agy`), no modo sem terminal:
//! `agy -p … --output-format stream-json`.
//!
//! O Antigravity guarda um único login por computador e só o faz pela própria tela:
//! não há perfil isolado, comando de login nem consulta de conta ou de uso.

use super::providers::{cli_command, failure, find_executable, now_seconds, Prompt};
use crate::application::{
    ports::{AccountGateway, ChatEngine, TurnListener, UsageProbe},
    AppError, AppResult, TurnControl,
};
use crate::domain::{
    account::{Identity, Provider},
    chat::{RateInfo, TurnEnd, TurnEvent, TurnRequest},
};
use async_trait::async_trait;
use serde_json::Value;
use std::{collections::HashSet, path::PathBuf, process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

/// Tempo para o turno mínimo que confere o login.
const CHECK_TIMEOUT: Duration = Duration::from_secs(120);
const CHECK_PROMPT: &str = "Responda apenas: ok";
const SINGLE_LOGIN: &str = "O Antigravity guarda um único login por computador. Para usar esta conta, abra o terminal, \
rode `agy`, entre com a conta Google e adicione a conta de novo.";

pub fn find_cli() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["agy.exe", "agy.cmd"] } else { &["agy"] };
    find_executable(names, &[])
}

fn missing_cli() -> AppError {
    AppError("O Antigravity (agy) não está instalado neste computador.".into())
}

pub struct AntigravityAccounts;

#[async_trait]
impl AccountGateway for AntigravityAccounts {
    fn installed(&self, _provider: Provider) -> bool {
        find_cli().is_some()
    }

    fn prepare_profile(&self, _provider: Provider, _account_id: &str) -> AppResult<String> {
        Err(AppError(SINGLE_LOGIN.into()))
    }

    fn discard_profile(&self, _provider: Provider, _config_dir: &str) {}

    /// O CLI não diz quem está logado: um turno mínimo mostra se o login vale.
    async fn identity(&self, _provider: Provider, _config_dir: Option<&str>) -> Identity {
        let Some(cli) = find_cli() else { return Identity::default() };
        let run = cli_command(&cli).args(["-p", CHECK_PROMPT, "--output-format", "json"]).stdin(Stdio::null()).output();
        let Ok(Ok(output)) = tokio::time::timeout(CHECK_TIMEOUT, run).await else { return Identity::default() };
        let said = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        // Só o erro de autenticação desmente o login; limite e outras falhas aparecem no turno.
        Identity { logged_in: !matches!(failure(&said, 0), TurnEnd::AuthError(_)) || output.status.success(), ..Identity::default() }
    }

    async fn login(&self, _provider: Provider, _config_dir: &str) -> AppResult<Identity> {
        Err(AppError(SINGLE_LOGIN.into()))
    }
}

/// Argumentos de `agy`; `prompt` é a mensagem já com instruções e anexos.
fn print_args(request: &TurnRequest, prompt: &str) -> Vec<String> {
    let mut args: Vec<String> = ["-p", prompt, "--output-format", "stream-json"].map(String::from).to_vec();
    if let Some(conversation) = &request.resume {
        args.extend(["--conversation".to_owned(), conversation.clone()]);
    }
    // Fora do terminal o Antigravity não pergunta: o que precisaria de aprovação é recusado.
    match request.permission_mode.as_str() {
        "plan" => args.extend(["--mode", "plan"].map(String::from)),
        "acceptEdits" => args.extend(["--mode", "accept-edits"].map(String::from)),
        "bypassPermissions" => args.push("--dangerously-skip-permissions".into()),
        _ => {}
    }
    args
}

/// Acompanha os eventos de `--output-format stream-json` (`init`, `step_update`, `result`).
#[derive(Default)]
pub(crate) struct AntigravityTurn {
    session_id: Option<String>,
    streamed: String,
    /// `(status, resposta, erro)` do evento final.
    result: Option<(String, String, String)>,
    announced: HashSet<i64>,
}

impl AntigravityTurn {
    pub fn feed(&mut self, line: &str) -> Vec<TurnEvent> {
        let Ok(v) = serde_json::from_str::<Value>(line) else { return vec![] };
        match v["event"].as_str().unwrap_or_default() {
            "step_update" => self.step(&v["step_update"]),
            "result" => {
                let result = &v["result"];
                let text = |value: &Value| match value {
                    Value::String(s) => s.clone(),
                    Value::Null => String::new(),
                    other => other["message"].as_str().map(str::to_owned).unwrap_or_else(|| other.to_string()),
                };
                if let Some(id) = result["conversation_id"].as_str() {
                    self.session_id = Some(id.to_owned());
                }
                self.result = Some((text(&result["status"]), text(&result["response"]), text(&result["error"])));
                vec![]
            }
            _ => vec![],
        }
    }

    fn step(&mut self, step: &Value) -> Vec<TurnEvent> {
        let mut events = vec![];
        if let Some(id) = step["conversation_id"].as_str().filter(|_| self.session_id.is_none()) {
            self.session_id = Some(id.to_owned());
            events.push(TurnEvent::Session(id.to_owned()));
        }
        match step["step_type"].as_str().unwrap_or_default() {
            "agent_response" => {
                if let Some(delta) = step["text_delta"].as_str().filter(|d| !d.is_empty()) {
                    self.streamed.push_str(delta);
                    events.push(TurnEvent::Delta(delta.to_owned()));
                }
            }
            "tool" => {
                let index = step["step_index"].as_i64().unwrap_or(-1);
                let id = format!("step-{index}");
                let info = &step["tool_info"];
                if self.announced.insert(index) {
                    let name = info["name"].as_str().or(step["tool_name"].as_str()).unwrap_or("Ferramenta").to_owned();
                    events.push(TurnEvent::ToolCall { id: id.clone(), name, input: info["parameters"].to_string() });
                }
                if step["state"] == "DONE" {
                    let failed = !info["error"].is_null();
                    let output = if failed { info["error"]["message"].as_str() } else { info["output"].as_str() };
                    events.push(TurnEvent::ToolResult { id, output: output.unwrap_or_default().to_owned(), is_error: failed });
                }
            }
            _ => {}
        }
        events
    }

    pub fn into_end(self, stderr: &str, now: i64) -> TurnEnd {
        match self.result {
            Some((status, response, _)) if status == "SUCCESS" => TurnEnd::Done {
                session_id: self.session_id,
                text: if self.streamed.trim().is_empty() { response } else { self.streamed },
            },
            Some((_, _, error)) if !error.is_empty() => failure(&error, now),
            _ => failure(stderr, now),
        }
    }
}

pub struct AntigravityEngine;

#[async_trait]
impl ChatEngine for AntigravityEngine {
    async fn run_turn(&self, request: &TurnRequest, control: &mut TurnControl, on_event: TurnListener<'_>) -> TurnEnd {
        let Some(cli) = find_cli() else { return TurnEnd::Failed(missing_cli().0) };
        let prompt = Prompt::compose(request);
        let mut child = match cli_command(&cli)
            .args(print_args(request, &prompt.text))
            .current_dir(&request.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => return TurnEnd::Failed(format!("Não foi possível iniciar o Antigravity: {e}")),
        };
        let stderr = child.stderr.take().map(|mut pipe| {
            tokio::spawn(async move {
                let mut text = String::new();
                let _ = pipe.read_to_string(&mut text).await;
                text
            })
        });
        let Some(stdout) = child.stdout.take() else {
            return TurnEnd::Failed("Não foi possível ler a saída do Antigravity.".into());
        };

        let mut lines = BufReader::new(stdout).lines();
        let mut turn = AntigravityTurn::default();
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
        turn.into_end(&stderr, now_seconds())
    }
}

/// O Antigravity só mostra a cota dentro da própria tela (`/usage`).
pub struct AntigravityUsage;

#[async_trait]
impl UsageProbe for AntigravityUsage {
    async fn usage(&self, _provider: Provider, _config_dir: Option<&str>) -> AppResult<RateInfo> {
        Err(AppError("O Antigravity não informa o uso da conta fora do terminal. Use /usage dentro do agy.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::providers::sample_request;

    fn run(lines: &[&str]) -> (Vec<TurnEvent>, AntigravityTurn) {
        let mut turn = AntigravityTurn::default();
        let events = lines.iter().flat_map(|l| turn.feed(l)).collect();
        (events, turn)
    }

    /// Formato da documentação do modo sem terminal (antigravity.google/docs/cli/headless).
    #[test]
    fn translates_text_tools_and_the_final_result() {
        let (events, turn) = run(&[
            r#"{"event":"init","init":{"cwd":"/tmp","tools":[],"permission_mode":"request-review"}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"c-1","step_index":0,"state":"DONE","step_type":"user_input"}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"c-1","step_index":1,"state":"ACTIVE","step_type":"tool","tool_name":"read_file","tool_info":{"name":"read_file","parameters":{"path":"a.txt"}}}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"c-1","step_index":1,"state":"DONE","step_type":"tool","tool_info":{"name":"read_file","parameters":{"path":"a.txt"},"output":"olá"}}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"c-1","step_index":2,"state":"ACTIVE","step_type":"agent_response","text_delta":"O arquivo "}}"#,
            r#"{"event":"step_update","step_update":{"conversation_id":"c-1","step_index":2,"state":"ACTIVE","step_type":"agent_response","text_delta":"diz olá."}}"#,
            r#"{"event":"result","result":{"conversation_id":"c-1","status":"SUCCESS","response":"O arquivo diz olá.","num_turns":1}}"#,
        ]);
        assert_eq!(events[0], TurnEvent::Session("c-1".into()));
        assert_eq!(events[1], TurnEvent::ToolCall { id: "step-1".into(), name: "read_file".into(), input: r#"{"path":"a.txt"}"#.into() });
        assert_eq!(events[2], TurnEvent::ToolResult { id: "step-1".into(), output: "olá".into(), is_error: false });
        assert_eq!(events[3..], [TurnEvent::Delta("O arquivo ".into()), TurnEvent::Delta("diz olá.".into())]);
        assert_eq!(turn.into_end("", 0), TurnEnd::Done { session_id: Some("c-1".into()), text: "O arquivo diz olá.".into() });
    }

    #[test]
    fn failed_tool_and_failed_run_are_reported() {
        let (events, turn) = run(&[
            r#"{"event":"step_update","step_update":{"conversation_id":"c-2","step_index":3,"state":"DONE","step_type":"tool","tool_info":{"name":"run_command","parameters":{},"error":{"type":"denied","message":"não autorizado"}}}}"#,
            r#"{"event":"result","result":{"conversation_id":"c-2","status":"ERROR","error":"quota exceeded for this model"}}"#,
        ]);
        assert_eq!(events[2], TurnEvent::ToolResult { id: "step-3".into(), output: "não autorizado".into(), is_error: true });
        assert_eq!(turn.into_end("", 0), TurnEnd::Limited { resets_at: None });

        let unanswered = run(&[]).1.into_end("ERROR: authentication required", 0);
        assert!(matches!(unanswered, TurnEnd::AuthError(_)));
        let no_stream = run(&[r#"{"event":"result","result":{"conversation_id":"c-3","status":"SUCCESS","response":"ok"}}"#]).1;
        assert_eq!(no_stream.into_end("", 0), TurnEnd::Done { session_id: Some("c-3".into()), text: "ok".into() });
    }

    #[test]
    fn arguments_resume_the_conversation_and_follow_the_mode() {
        let mut request = sample_request(Provider::Gemini);
        assert_eq!(print_args(&request, "olá"), ["-p", "olá", "--output-format", "stream-json"]);
        request.resume = Some("c-1".into());
        request.permission_mode = "acceptEdits".into();
        assert_eq!(print_args(&request, "olá")[4..], ["--conversation", "c-1", "--mode", "accept-edits"]);
        request.permission_mode = "bypassPermissions".into();
        assert_eq!(print_args(&request, "olá").last().map(String::as_str), Some("--dangerously-skip-permissions"));
    }

    #[tokio::test]
    #[ignore = "consome uso da conta real do Google"]
    async fn real_antigravity_answers() {
        let mut request = sample_request(Provider::Gemini);
        request.prompt = "Responda apenas: ok".into();
        let end = AntigravityEngine.run_turn(&request, &mut TurnControl::new().0, &mut |_| {}).await;
        assert!(matches!(end, TurnEnd::Done { .. }), "turno falhou: {end:?}");
    }
}
