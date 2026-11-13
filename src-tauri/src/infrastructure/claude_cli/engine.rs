//! Motor de conversa que executa o Claude Code CLI oficial como processo filho.

use super::{command, stream::{parse_line, Observed}};
use crate::application::{ports::{ChatEngine, TurnListener}, TurnControl};
use crate::domain::chat::{Attachment, AttachmentKind, Injection, PermissionDecision, TurnEnd, TurnEvent, TurnRequest};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet}, path::PathBuf, process::Stdio};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, Command},
};

const DENIED_MESSAGE: &str = "O usuário não autorizou esta ação.";

pub struct ClaudeCliEngine {
    cli: PathBuf,
}

impl ClaudeCliEngine {
    pub fn new(cli: PathBuf) -> Self {
        ClaudeCliEngine { cli }
    }

    fn command(&self, request: &TurnRequest) -> Command {
        let mut cmd = command(&self.cli, request.config_dir.as_deref());
        cmd.args([
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            // Ecoa cada mensagem do usuário quando é lida: é como se sabe que uma
            // mensagem mandada no meio do turno entrou na conversa.
            "--replay-user-messages",
            "--include-partial-messages",
            "--forward-subagent-text",
            "--model",
            &request.model,
            "--tools",
            &request.tools,
            "--effort",
            &request.effort,
        ]);
        if request.interactive {
            // O que não estiver liberado é perguntado ao app, que repassa ao usuário.
            cmd.args(["--permission-mode", &request.permission_mode, "--permission-prompt-tool", "stdio"]);
        }
        if !request.allowed_tools.is_empty() {
            cmd.arg("--allowedTools").arg(request.allowed_tools.join(","));
        }
        let denied: Vec<String> = request
            .denied_skills
            .iter()
            .map(|s| format!("Skill({s})"))
            .chain(request.denied_tools.iter().cloned())
            .collect();
        if !denied.is_empty() {
            cmd.arg("--disallowedTools").arg(denied.join(","));
        }
        if let Some(config) = &request.mcp_config {
            cmd.args(["--mcp-config", config]);
        }
        if let Some(session) = &request.resume {
            cmd.args(["--resume", session]);
        }
        if let Some(system) = &request.extra_system {
            cmd.args(["--append-system-prompt", system]);
        }
        cmd.current_dir(&request.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }
}

/// Bloco de conteúdo de um anexo, no formato da API de mensagens.
fn attachment_block(attachment: &Attachment) -> Value {
    let source = json!({ "type": "base64", "media_type": attachment.media_type, "data": attachment.data });
    match attachment.kind {
        AttachmentKind::Text => json!({ "type": "text", "text": format!("Arquivo anexado — {}:\n{}", attachment.name, attachment.data) }),
        AttachmentKind::Image => json!({ "type": "image", "source": source }),
        AttachmentKind::Pdf => json!({ "type": "document", "source": source }),
    }
}

/// Mensagem do usuário: só o texto, ou os anexos seguidos do texto.
fn user_message(request: &TurnRequest) -> Value {
    json!({ "type": "user", "message": { "role": "user", "content": user_content(&request.prompt, &request.attachments) } })
}

/// Mensagem mandada durante o turno; o `uuid` volta no eco quando o CLI a lê.
fn injected_message(injection: &Injection) -> Value {
    json!({
        "type": "user",
        "uuid": injection.id,
        "message": { "role": "user", "content": user_content(&injection.prompt, &injection.attachments) }
    })
}

fn user_content(prompt: &str, attachments: &[Attachment]) -> Value {
    if attachments.is_empty() {
        return json!(prompt);
    }
    let mut blocks: Vec<Value> = attachments.iter().map(attachment_block).collect();
    blocks.push(json!({ "type": "text", "text": prompt }));
    Value::Array(blocks)
}

async fn write_line(stdin: &mut Option<ChildStdin>, message: Value) {
    if let Some(pipe) = stdin {
        let _ = pipe.write_all(format!("{message}\n").as_bytes()).await;
        let _ = pipe.flush().await;
    }
}

/// Resposta a um pedido de autorização, no formato do protocolo de controle.
/// Respostas a perguntas entram nos argumentos (`answers`); a aprovação de um
/// plano pode trocar o modo da sessão (`updatedPermissions`).
fn permission_response(decision: &PermissionDecision, input: &str) -> Value {
    let response = if decision.allow {
        let mut input: Value = serde_json::from_str(input).unwrap_or(Value::Null);
        if !decision.answers.is_empty() {
            let answers: serde_json::Map<String, Value> =
                decision.answers.iter().map(|(q, a)| (q.clone(), Value::String(a.clone()))).collect();
            input["answers"] = Value::Object(answers);
        }
        let mut response = json!({ "behavior": "allow", "updatedInput": input });
        if let Some(mode) = &decision.next_mode {
            response["updatedPermissions"] = json!([{ "type": "setMode", "mode": mode, "destination": "session" }]);
        }
        response
    } else {
        json!({ "behavior": "deny", "message": DENIED_MESSAGE })
    };
    json!({
        "type": "control_response",
        "response": { "subtype": "success", "request_id": decision.request_id, "response": response }
    })
}

#[async_trait]
impl ChatEngine for ClaudeCliEngine {
    async fn run_turn(
        &self,
        request: &TurnRequest,
        control: &mut TurnControl,
        on_event: TurnListener<'_>,
    ) -> TurnEnd {
        let mut child = match self.command(request).spawn() {
            Ok(child) => child,
            Err(e) => return TurnEnd::Failed(format!("Não foi possível iniciar o Claude Code: {e}")),
        };

        // O stdin fica aberto durante o turno para responder autorizações;
        // fechá-lo, ao receber o resultado, encerra o processo.
        let mut stdin = child.stdin.take();
        write_line(&mut stdin, user_message(request)).await;

        let stderr = child.stderr.take().map(|mut pipe| {
            tokio::spawn(async move {
                let mut text = String::new();
                let _ = pipe.read_to_string(&mut text).await;
                text
            })
        });
        let Some(stdout) = child.stdout.take() else {
            return TurnEnd::Failed("Não foi possível ler a saída do Claude Code.".into());
        };

        let mut lines = BufReader::new(stdout).lines();
        let mut observed = Observed::default();
        // Pedidos de autorização em aberto: id → argumentos originais da ferramenta.
        let mut pending: HashMap<String, String> = HashMap::new();
        // Mensagens mandadas durante o turno que o CLI ainda não leu.
        let mut unread: HashSet<String> = HashSet::new();
        loop {
            tokio::select! {
                _ = control.cancel.notified() => {
                    let _ = child.kill().await;
                    return TurnEnd::Cancelled;
                }
                Some(decision) = control.decisions.recv() => {
                    if let Some(input) = pending.remove(&decision.request_id) {
                        write_line(&mut stdin, permission_response(&decision, &input)).await;
                    }
                }
                Some((injection, ack)) = control.injections.recv() => {
                    // Depois do resultado sem nada por ler, o stdin já fechou e o processo está saindo.
                    let open = stdin.is_some();
                    if open {
                        write_line(&mut stdin, injected_message(&injection)).await;
                        unread.insert(injection.id);
                    }
                    let _ = ack.send(open);
                }
                line = lines.next_line() => {
                    let Ok(Some(line)) = line else { break };
                    for event in parse_line(&line) {
                        match &event {
                            TurnEvent::Permission { request_id, input, .. } => {
                                pending.insert(request_id.clone(), input.clone());
                            }
                            // Só interessa o eco das mensagens mandadas no meio do turno.
                            TurnEvent::UserRead(id) if !unread.remove(id) => continue,
                            // Uma mensagem que chegou no fim vira mais uma resposta no mesmo processo.
                            TurnEvent::Result { .. } if unread.is_empty() => stdin = None,
                            _ => {}
                        }
                        observed.record(&event);
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
        observed.into_end(&stderr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::claude_cli::find_cli;

    fn request(prompt: &str, resume: Option<String>) -> TurnRequest {
        TurnRequest {
            provider: crate::domain::account::Provider::Claude,
            config_dir: None,
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            model: "haiku".into(),
            resume,
            prompt: prompt.into(),
            attachments: vec![],
            extra_system: None,
            tools: "WebSearch,WebFetch,Skill".into(),
            effort: "low".into(),
            permission_mode: "manual".into(),
            interactive: false,
            denied_skills: vec![],
            denied_tools: vec![],
            mcp_config: None,
            allowed_tools: vec![],
        }
    }

    fn engine() -> ClaudeCliEngine {
        ClaudeCliEngine::new(find_cli().expect("Claude Code instalado"))
    }

    #[test]
    fn permission_response_echoes_input_when_allowed() {
        let allow = PermissionDecision::new("r1", true);
        let v = permission_response(&allow, r#"{"file_path":"a.txt"}"#);
        assert_eq!(v["response"]["request_id"], "r1");
        assert_eq!(v["response"]["response"]["behavior"], "allow");
        assert_eq!(v["response"]["response"]["updatedInput"]["file_path"], "a.txt");
        assert!(v["response"]["response"].get("updatedPermissions").is_none());

        let deny = PermissionDecision::new("r2", false);
        assert_eq!(permission_response(&deny, "{}")["response"]["response"]["behavior"], "deny");
    }

    /// Formatos conferidos com o CLI 2.1.323: respostas em `answers`; plano aprovado troca o modo.
    #[test]
    fn answers_and_plan_approval_follow_the_control_protocol() {
        let mut answer = PermissionDecision::new("r1", true);
        answer.answers = vec![("Qual cor?".into(), "Azul".into())];
        let v = permission_response(&answer, r#"{"questions":[{"question":"Qual cor?"}]}"#);
        let updated = &v["response"]["response"]["updatedInput"];
        assert_eq!(updated["questions"][0]["question"], "Qual cor?");
        assert_eq!(updated["answers"]["Qual cor?"], "Azul");

        let mut plan = PermissionDecision::new("r2", true);
        plan.next_mode = Some("acceptEdits".into());
        let v = permission_response(&plan, r##"{"plan":"# Plano"}"##);
        assert_eq!(v["response"]["response"]["updatedInput"]["plan"], "# Plano");
        assert_eq!(v["response"]["response"]["updatedPermissions"][0]["mode"], "acceptEdits");
    }

    #[test]
    fn user_message_puts_attachments_before_the_text() {
        let mut req = request("resuma", None);
        assert_eq!(user_message(&req)["message"]["content"], "resuma");

        req.attachments = vec![
            Attachment::text("nota.md", "olá"),
            Attachment { name: "foto.png".into(), kind: AttachmentKind::Image, media_type: "image/png".into(), data: "AAAA".into() },
        ];
        let content = &user_message(&req)["message"]["content"];
        assert_eq!(content[0]["text"], "Arquivo anexado — nota.md:\nolá");
        assert_eq!((content[1]["type"].as_str(), content[1]["source"]["media_type"].as_str()), (Some("image"), Some("image/png")));
        assert_eq!(content[2]["text"], "resuma");
    }

    #[tokio::test]
    #[ignore = "consome uso da conta real"]
    async fn real_cli_reads_attached_text_and_image() {
        // PNG 1x1 vermelho.
        let pixel = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==";
        let mut req = request("Qual é a senha do arquivo anexado e qual a cor predominante da imagem? Responda em uma linha.", None);
        req.attachments = vec![
            Attachment::text("segredo.txt", "A senha é abacaxi-72."),
            Attachment { name: "cor.png".into(), kind: AttachmentKind::Image, media_type: "image/png".into(), data: pixel.into() },
        ];
        let end = engine().run_turn(&req, &mut TurnControl::new().0, &mut |_| {}).await;
        let TurnEnd::Done { text, .. } = end else { panic!("turno falhou: {end:?}") };
        assert!(text.contains("abacaxi-72"), "leu o texto anexado: {text}");
        assert!(text.to_lowercase().contains("vermelh"), "viu a imagem: {text}");
    }

    #[tokio::test]
    #[ignore = "consome uso da conta real"]
    async fn real_cli_reads_messages_sent_during_the_turn() {
        let mut req = request("Use a ferramenta Bash para rodar: sleep 6; echo pronto. Depois responda em uma frase.", None);
        req.tools = "Bash".into();
        req.allowed_tools = vec!["Bash".into()];
        let (mut control, _handle) = TurnControl::new();
        let (inject, injections) = tokio::sync::mpsc::unbounded_channel();
        control.injections = injections;
        let sender = tokio::spawn(async move {
            let mut accepted = vec![];
            for (delay, id, text) in [(3, "11111111-1111-4111-8111-111111111111", "Ao final, diga também a palavra BANANA.")] {
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                let (ack, rx) = tokio::sync::oneshot::channel();
                let injection = Injection { id: id.into(), prompt: text.into(), attachments: vec![] };
                inject.send((injection, ack)).unwrap();
                accepted.push(rx.await.unwrap_or(false));
            }
            accepted
        });
        let mut read = vec![];
        let end = engine().run_turn(&req, &mut control, &mut |e| if let TurnEvent::UserRead(id) = e { read.push(id.clone()) }).await;
        assert_eq!(sender.await.unwrap(), [true]);
        assert_eq!(read, ["11111111-1111-4111-8111-111111111111"], "só o eco da mensagem mandada no meio do turno");
        let TurnEnd::Done { text, .. } = end else { panic!("turno falhou: {end:?}") };
        assert!(text.contains("BANANA"), "leu a mensagem: {text}");
    }

    #[tokio::test]
    #[ignore = "consome uso da conta real"]
    async fn real_cli_answers_a_message_that_arrives_at_the_end() {
        let req = request("Escreva um parágrafo de 4 frases sobre o mar.", None);
        let (mut control, _handle) = TurnControl::new();
        let (inject, injections) = tokio::sync::mpsc::unbounded_channel();
        control.injections = injections;
        let mut sent = false;
        let mut read = vec![];
        let mut inject = Some(inject);
        let end = engine()
            .run_turn(&req, &mut control, &mut |e| {
                // Manda assim que o texto começa a chegar: o CLI pode já ter terminado de responder.
                if let (TurnEvent::Delta(_), false) = (e, sent) {
                    sent = true;
                    let (ack, _rx) = tokio::sync::oneshot::channel();
                    let injection = Injection { id: "22222222-2222-4222-8222-222222222222".into(), prompt: "Agora diga apenas: BANANA".into(), attachments: vec![] };
                    let _ = inject.take().unwrap().send((injection, ack));
                }
                if let TurnEvent::UserRead(id) = e { read.push(id.clone()) }
            })
            .await;
        let TurnEnd::Done { text, .. } = end else { panic!("turno falhou: {end:?}") };
        assert_eq!(read.len(), 1, "a mensagem foi lida");
        assert!(text.contains("BANANA"), "respondeu à mensagem: {text}");
    }

    /// Usa o Claude Code real e a conta deste computador: `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "consome uso da conta real"]
    async fn real_cli_answers_reports_usage_and_resumes() {
        let engine = engine();
        let mut events = vec![];
        let first = engine
            .run_turn(&request("Guarde o número 417. Responda apenas: ok", None), &mut TurnControl::new().0, &mut |e| {
                events.push(e.clone())
            })
            .await;
        let TurnEnd::Done { session_id: Some(session), .. } = first else { panic!("turno falhou: {first:?}") };
        assert!(events.iter().any(|e| matches!(e, TurnEvent::Delta(_))), "o texto chega em streaming");
        assert!(
            events.iter().any(|e| matches!(e, TurnEvent::Rate(r) if r.util_5h.is_some())),
            "o uso da conta é informado"
        );

        let second = engine
            .run_turn(
                &request("Qual número pedi para guardar? Responda só o número.", Some(session)),
                &mut TurnControl::new().0,
                &mut |_| {},
            )
            .await;
        let TurnEnd::Done { text, .. } = second else { panic!("retomada falhou: {second:?}") };
        assert!(text.contains("417"), "a conversa retomada mantém o contexto: {text}");
    }

    #[tokio::test]
    #[ignore = "consome uso da conta real"]
    async fn real_cli_loads_connectors_and_accepts_skill_rules() {
        let mut req = request("Responda apenas: ok", None);
        req.denied_skills = vec!["hyperframes-cli".into(), "pdf".into()];
        req.mcp_config = Some(
            r#"{"mcpServers":{"arquivos":{"command":"npx","args":["-y","@modelcontextprotocol/server-filesystem","/tmp"]}}}"#.into(),
        );
        req.allowed_tools.push("mcp__arquivos".into());
        let mut connectors = vec![];
        let end = engine()
            .run_turn(&req, &mut TurnControl::new().0, &mut |e| {
                if let TurnEvent::Environment { connectors: found, skills } = e {
                    assert!(!skills.is_empty(), "o catálogo de skills é informado");
                    connectors = found.clone();
                }
            })
            .await;
        assert!(matches!(end, TurnEnd::Done { .. }), "turno falhou: {end:?}");
        assert_eq!(connectors, [("arquivos".to_owned(), "connected".to_owned())]);
    }

    /// Sessão de agente em uma pasta temporária: a edição só acontece conforme a resposta ao pedido.
    async fn edit_note(allow: bool) -> (String, Vec<TurnEvent>) {
        let dir = std::env::temp_dir().join(format!("lume-agent-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("nota.txt"), "conteudo original\n").unwrap();
        let mut req = request("Edite o arquivo nota.txt trocando 'original' por 'novo'. Seja breve.", None);
        req.cwd = dir.to_string_lossy().into_owned();
        req.tools = "default".into();
        req.interactive = true;

        let (mut control, _handle) = TurnControl::new();
        let answer = control.answer.clone();
        let mut events = vec![];
        let end = engine()
            .run_turn(&req, &mut control, &mut |e| {
                if let TurnEvent::Permission { request_id, .. } = e {
                    let _ = answer.send(PermissionDecision::new(request_id, allow));
                }
                events.push(e.clone());
            })
            .await;
        assert!(matches!(end, TurnEnd::Done { .. }), "turno falhou: {end:?}");
        let content = std::fs::read_to_string(dir.join("nota.txt")).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
        (content, events)
    }

    #[tokio::test]
    #[ignore = "consome uso da conta real"]
    async fn real_cli_edits_file_only_after_approval() {
        let (content, events) = edit_note(true).await;
        assert_eq!(content.trim(), "conteudo novo");
        assert!(events.iter().any(|e| matches!(e, TurnEvent::Permission { tool, .. } if tool == "Edit")));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::ToolCall { name, .. } if name == "Edit")));
        assert!(events.iter().any(|e| matches!(e, TurnEvent::ToolResult { is_error: false, .. })));
    }

    #[tokio::test]
    #[ignore = "consome uso da conta real"]
    async fn real_cli_leaves_file_untouched_when_denied() {
        let (content, events) = edit_note(false).await;
        assert_eq!(content.trim(), "conteudo original");
        assert!(events.iter().any(|e| matches!(e, TurnEvent::Permission { .. })));
    }
}
