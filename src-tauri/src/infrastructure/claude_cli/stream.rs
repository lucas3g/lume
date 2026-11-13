//! Protocolo stream-json do Claude Code: leitura dos eventos e desfecho do turno.

use crate::domain::chat::{RateInfo, SubagentStep, TurnEnd, TurnEvent};
use serde_json::Value;

/// Eventos contidos em uma linha do stream (a de inicialização gera dois).
pub fn parse_line(line: &str) -> Vec<TurnEvent> {
    let Ok(v) = serde_json::from_str::<Value>(line) else { return vec![] };
    let single = |event: Option<TurnEvent>| event.into_iter().collect();
    match v["type"].as_str().unwrap_or("") {
        "system" if v["subtype"] == "init" => parse_init(&v),
        "system" => single(parse_task(&v)),
        "stream_event" if !v["parent_tool_use_id"].is_null() => single(parse_subagent_event(&v["event"])),
        "stream_event" => single(parse_stream_event(&v["event"])),
        "rate_limit_event" => vec![TurnEvent::Rate(parse_rate(&v["rate_limit_info"]))],
        // Mensagens de subagentes (com `parent_tool_use_id`) não são passos desta conversa:
        // viram passos do subagente, para acompanhar o trabalho dele.
        "assistant" | "user" if !v["parent_tool_use_id"].is_null() => parse_subagent_message(&v),
        "assistant" => parse_assistant(&v),
        // Eco (`--replay-user-messages`) de uma mensagem do usuário no momento em que é lida.
        "user" if v["isReplay"] == true => v["uuid"].as_str().map(|id| TurnEvent::UserRead(id.into())).into_iter().collect(),
        "user" => parse_tool_results(&v["message"]["content"]),
        "control_request" => single(parse_permission(&v)),
        "result" => parse_context(&v)
            .into_iter()
            .chain([TurnEvent::Result {
                is_error: v["is_error"].as_bool().unwrap_or(false),
                text: result_text(&v),
                session_id: v["session_id"].as_str().map(Into::into),
            }])
            .collect(),
        _ => vec![],
    }
}

/// Erro de API, se houver, e as chamadas de ferramenta completas da mensagem.
fn parse_assistant(v: &Value) -> Vec<TurnEvent> {
    let mut events: Vec<TurnEvent> = v["error"].as_str().map(|e| TurnEvent::ApiError(e.into())).into_iter().collect();
    let blocks = v["message"]["content"].as_array().map(Vec::as_slice).unwrap_or_default();
    events.extend(blocks.iter().filter(|b| b["type"] == "tool_use").filter_map(|b| {
        Some(TurnEvent::ToolCall {
            id: b["id"].as_str()?.to_owned(),
            name: b["name"].as_str()?.to_owned(),
            input: b["input"].to_string(),
        })
    }));
    events
}

fn parse_tool_results(content: &Value) -> Vec<TurnEvent> {
    let blocks = content.as_array().map(Vec::as_slice).unwrap_or_default();
    blocks
        .iter()
        .filter(|b| b["type"] == "tool_result")
        .filter_map(|b| {
            Some(TurnEvent::ToolResult {
                id: b["tool_use_id"].as_str()?.to_owned(),
                output: block_text(&b["content"]),
                is_error: b["is_error"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

/// Texto de um conteúdo que pode ser uma string ou uma lista de blocos de texto.
fn block_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

fn parse_permission(v: &Value) -> Option<TurnEvent> {
    let request = &v["request"];
    if request["subtype"] != "can_use_tool" {
        return None;
    }
    Some(TurnEvent::Permission {
        request_id: v["request_id"].as_str()?.to_owned(),
        tool: request["tool_name"].as_str()?.to_owned(),
        input: request["input"].to_string(),
        description: request["description"].as_str().unwrap_or_default().to_owned(),
    })
}

/// Ocupação do contexto: tokens da última chamada ao modelo sobre a janela dele.
fn parse_context(v: &Value) -> Option<TurnEvent> {
    let last = v["usage"]["iterations"].as_array()?.last()?;
    let used_tokens = ["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens", "output_tokens"]
        .iter()
        .filter_map(|key| last[*key].as_u64())
        .sum();
    let window_tokens = v["modelUsage"].as_object()?.values().filter_map(|m| m["contextWindow"].as_u64()).max()?;
    Some(TurnEvent::Context { used_tokens, window_tokens })
}

fn parse_init(v: &Value) -> Vec<TurnEvent> {
    let names = |key: &str| -> Vec<String> {
        v[key].as_array().map(|list| list.iter().filter_map(Value::as_str).map(Into::into).collect()).unwrap_or_default()
    };
    let connectors = v["mcp_servers"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|s| Some((s["name"].as_str()?.to_owned(), s["status"].as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    let mut events: Vec<TurnEvent> = v["session_id"].as_str().map(|s| TurnEvent::Session(s.into())).into_iter().collect();
    events.push(TurnEvent::Environment { skills: names("skills"), connectors });
    events
}

fn parse_stream_event(e: &Value) -> Option<TurnEvent> {
    match e["type"].as_str()? {
        "content_block_delta" => match e["delta"]["type"].as_str()? {
            "text_delta" => e["delta"]["text"].as_str().map(|t| TurnEvent::Delta(t.into())),
            "thinking_delta" => e["delta"]["thinking"].as_str().map(|t| TurnEvent::Thinking(t.into())),
            _ => None,
        },
        "content_block_start" => {
            let block = &e["content_block"];
            if block["type"].as_str()?.ends_with("tool_use") {
                block["name"].as_str().map(|n| TurnEvent::Tool(n.into()))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Ciclo de vida de um subagente (`task_*`), ligado à chamada que o criou.
fn parse_task(v: &Value) -> Option<TurnEvent> {
    let parent = v["tool_use_id"].as_str()?.to_owned();
    let text = |key: &str| v[key].as_str().unwrap_or_default().to_owned();
    let step = match v["subtype"].as_str()? {
        // Tarefas de outros tipos (ex.: comandos em segundo plano) não são subagentes.
        "task_started" if v["task_type"].as_str().is_some_and(|t| t.contains("agent")) => SubagentStep::Started {
            description: text("description"),
            agent_type: text("subagent_type"),
            prompt: text("prompt"),
            background: v["is_backgrounded"].as_bool().unwrap_or(false),
        },
        "task_progress" => SubagentStep::Progress { description: text("description") },
        "task_notification" => SubagentStep::Finished { status: text("status"), summary: text("summary") },
        _ => return None,
    };
    Some(TurnEvent::Subagent { parent, step })
}

/// Texto, chamadas de ferramenta e resultados de uma mensagem do subagente.
fn parse_subagent_message(v: &Value) -> Vec<TurnEvent> {
    let Some(parent) = v["parent_tool_use_id"].as_str() else { return vec![] };
    let blocks = v["message"]["content"].as_array().map(Vec::as_slice).unwrap_or_default();
    let is_assistant = v["type"] == "assistant";
    blocks
        .iter()
        .filter_map(|b| {
            let step = match b["type"].as_str()? {
                "text" if is_assistant => {
                    let text = b["text"].as_str()?.trim();
                    if text.is_empty() {
                        return None;
                    }
                    SubagentStep::Text { text: text.to_owned() }
                }
                "tool_use" if is_assistant => SubagentStep::ToolCall {
                    id: b["id"].as_str()?.to_owned(),
                    name: b["name"].as_str()?.to_owned(),
                    input: b["input"].to_string(),
                },
                "tool_result" => SubagentStep::ToolResult {
                    id: b["tool_use_id"].as_str()?.to_owned(),
                    output: block_text(&b["content"]),
                    is_error: b["is_error"].as_bool().unwrap_or(false),
                },
                _ => return None,
            };
            Some(TurnEvent::Subagent { parent: parent.to_owned(), step })
        })
        .collect()
}

/// De um subagente só interessa o texto, para mostrar o andamento.
fn parse_subagent_event(e: &Value) -> Option<TurnEvent> {
    match parse_stream_event(e)? {
        TurnEvent::Delta(text) => Some(TurnEvent::SubagentDelta(text)),
        _ => None,
    }
}

fn parse_rate(info: &Value) -> RateInfo {
    let windows = &info["unifiedWindows"];
    RateInfo {
        status: info["status"].as_str().unwrap_or("allowed").into(),
        resets_at: info["resetsAt"].as_i64(),
        util_5h: windows["five_hour"]["utilization"].as_f64(),
        util_7d: windows["seven_day"]["utilization"].as_f64(),
        resets_5h: windows["five_hour"]["resetsAt"].as_i64(),
        resets_7d: windows["seven_day"]["resetsAt"].as_i64(),
    }
}

fn result_text(v: &Value) -> String {
    if let Some(text) = v["result"].as_str() {
        return text.to_owned();
    }
    v["errors"]
        .as_array()
        .map(|list| list.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n"))
        .unwrap_or_default()
}

/// O que foi observado no stream de um turno.
#[derive(Default)]
pub struct Observed {
    pub limited: bool,
    pub resets_at: Option<i64>,
    pub session_id: Option<String>,
    /// `(is_error, texto)` do evento final, se houve.
    pub result: Option<(bool, String)>,
    pub streamed: String,
}

impl Observed {
    pub fn record(&mut self, event: &TurnEvent) {
        match event {
            TurnEvent::Session(id) => self.session_id = Some(id.clone()),
            TurnEvent::Delta(text) => self.streamed.push_str(text),
            TurnEvent::Rate(rate) if rate.is_limited() => {
                self.limited = true;
                self.resets_at = rate.resets_at.or(rate.resets_5h);
            }
            TurnEvent::ApiError(e) if e.contains("rate_limit") => self.limited = true,
            TurnEvent::Result { is_error, text, session_id } => {
                if session_id.is_some() {
                    self.session_id = session_id.clone();
                }
                self.result = Some((*is_error, text.clone()));
            }
            _ => {}
        }
    }

    pub fn into_end(self, stderr: &str) -> TurnEnd {
        let stderr = stderr.trim();
        match self.result {
            Some((false, text)) => TurnEnd::Done {
                session_id: self.session_id,
                text: if self.streamed.trim().is_empty() { text } else { self.streamed },
            },
            Some((true, text)) if self.limited || looks_like_limit(&text) => {
                TurnEnd::Limited { resets_at: self.resets_at }
            }
            Some((true, text)) if looks_like_auth(&text) => TurnEnd::AuthError(text),
            Some((true, text)) => TurnEnd::Failed(if text.is_empty() { stderr.to_owned() } else { text }),
            None if self.limited => TurnEnd::Limited { resets_at: self.resets_at },
            None if looks_like_auth(stderr) => TurnEnd::AuthError(stderr.to_owned()),
            None if stderr.is_empty() => TurnEnd::Failed("O processo terminou sem resposta.".into()),
            None => TurnEnd::Failed(stderr.to_owned()),
        }
    }
}

fn looks_like_limit(text: &str) -> bool {
    let t = text.to_lowercase();
    ["usage limit", "hit your limit", "rate limit", "limit reached"].iter().any(|p| t.contains(p))
}

fn looks_like_auth(text: &str) -> bool {
    let t = text.to_lowercase();
    ["/login", "authentication", "oauth", "not logged in"].iter().any(|p| t.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn end_of(lines: &[&str], stderr: &str) -> TurnEnd {
        let mut seen = Observed::default();
        lines.iter().flat_map(|l| parse_line(l)).for_each(|e| seen.record(&e));
        seen.into_end(stderr)
    }

    const DELTA: &str = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ok"}}}"#;
    // Capturado do CLI 2.1.322.
    const RATE_ALLOWED: &str = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1794521400,"rateLimitType":"five_hour","unifiedWindows":{"five_hour":{"utilization":0.41,"resetsAt":1794521400,"status":"allowed"},"seven_day":{"utilization":0.46,"resetsAt":1794906000,"status":"allowed"}}},"session_id":"x"}"#;
    const RATE_REJECTED: &str = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":99}}"#;

    #[test]
    fn replayed_user_message_reports_its_id() {
        let line = r#"{"type":"user","isReplay":true,"uuid":"aaaa-1","parent_tool_use_id":null,"message":{"role":"user","content":"diga BANANA"}}"#;
        assert_eq!(parse_line(line), [TurnEvent::UserRead("aaaa-1".into())]);
    }

    #[test]
    fn parses_text_delta() {
        assert_eq!(parse_line(DELTA), [TurnEvent::Delta("ok".into())]);
        assert!(parse_line("não é json").is_empty());
    }

    #[test]
    fn subagent_text_is_kept_apart_from_the_answer() {
        let line = r#"{"type":"stream_event","parent_tool_use_id":"t9","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lendo"}}}"#;
        assert_eq!(parse_line(line), [TurnEvent::SubagentDelta("lendo".into())]);
        let tool = r#"{"type":"stream_event","parent_tool_use_id":"t9","event":{"type":"content_block_start","content_block":{"type":"tool_use","name":"Read"}}}"#;
        assert!(parse_line(tool).is_empty(), "ferramentas do subagente não viram passos da conversa");
    }

    #[test]
    fn subagent_messages_become_its_steps() {
        let sub = |step| TurnEvent::Subagent { parent: "t9".into(), step };
        let call = r#"{"type":"assistant","parent_tool_use_id":"t9","message":{"content":[{"type":"text","text":"vou ler"},{"type":"tool_use","id":"u1","name":"Read","input":{}}]}}"#;
        assert_eq!(
            parse_line(call),
            [
                sub(SubagentStep::Text { text: "vou ler".into() }),
                sub(SubagentStep::ToolCall { id: "u1".into(), name: "Read".into(), input: "{}".into() }),
            ]
        );
        let result = r#"{"type":"user","parent_tool_use_id":"t9","message":{"content":[{"type":"tool_result","tool_use_id":"u1","content":"x"}]}}"#;
        assert_eq!(parse_line(result), [sub(SubagentStep::ToolResult { id: "u1".into(), output: "x".into(), is_error: false })]);
        // O pedido que abre o subagente chega como texto do usuário e não é um passo.
        let prompt = r#"{"type":"user","parent_tool_use_id":"t9","message":{"content":"leia a.txt"}}"#;
        assert!(parse_line(prompt).is_empty());
    }

    #[test]
    fn task_events_follow_the_subagent_lifecycle() {
        // Capturados do CLI 2.1.323.
        let started = r#"{"type":"system","subtype":"task_started","task_id":"a1","tool_use_id":"t9","description":"Ler a.txt","subagent_type":"general-purpose","is_backgrounded":true,"task_type":"local_agent","prompt":"leia"}"#;
        let progress = r#"{"type":"system","subtype":"task_progress","task_id":"a1","tool_use_id":"t9","description":"Reading a.txt","last_tool_name":"Read"}"#;
        let done = r#"{"type":"system","subtype":"task_notification","task_id":"a1","tool_use_id":"t9","status":"completed","summary":"hello"}"#;
        let sub = |step| TurnEvent::Subagent { parent: "t9".into(), step };
        assert_eq!(
            parse_line(started),
            [sub(SubagentStep::Started {
                description: "Ler a.txt".into(),
                agent_type: "general-purpose".into(),
                prompt: "leia".into(),
                background: true,
            })]
        );
        assert_eq!(parse_line(progress), [sub(SubagentStep::Progress { description: "Reading a.txt".into() })]);
        assert_eq!(parse_line(done), [sub(SubagentStep::Finished { status: "completed".into(), summary: "hello".into() })]);
        let shell = r#"{"type":"system","subtype":"task_started","task_id":"b1","tool_use_id":"t8","task_type":"local_bash"}"#;
        assert!(parse_line(shell).is_empty(), "comandos em segundo plano não são subagentes");
        assert!(parse_line(r#"{"type":"system","subtype":"task_updated","task_id":"a1"}"#).is_empty());
    }

    #[test]
    fn init_reports_session_skills_and_connectors() {
        let init = r#"{"type":"system","subtype":"init","session_id":"s1","skills":["pdf","revisar"],"mcp_servers":[{"name":"fs","status":"connected"}]}"#;
        assert_eq!(
            parse_line(init),
            [
                TurnEvent::Session("s1".into()),
                TurnEvent::Environment {
                    skills: vec!["pdf".into(), "revisar".into()],
                    connectors: vec![("fs".into(), "connected".into())],
                },
            ]
        );
    }

    #[test]
    fn parses_tool_calls_results_and_permission_requests() {
        let call = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"vou editar"},{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"a.txt"}}]}}"#;
        assert_eq!(
            parse_line(call),
            [TurnEvent::ToolCall { id: "t1".into(), name: "Edit".into(), input: r#"{"file_path":"a.txt"}"#.into() }]
        );

        let result = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"feito"}],"is_error":false}]}}"#;
        assert_eq!(parse_line(result), [TurnEvent::ToolResult { id: "t1".into(), output: "feito".into(), is_error: false }]);

        // Capturado do CLI 2.1.322.
        let ask = r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Edit","display_name":"Edit","input":{"file_path":"a.txt"},"description":"a.txt","tool_use_id":"t1"}}"#;
        assert_eq!(
            parse_line(ask),
            [TurnEvent::Permission {
                request_id: "r1".into(),
                tool: "Edit".into(),
                input: r#"{"file_path":"a.txt"}"#.into(),
                description: "a.txt".into(),
            }]
        );
        assert!(parse_line(r#"{"type":"control_request","request_id":"r2","request":{"subtype":"outro"}}"#).is_empty());
    }

    #[test]
    fn result_reports_context_usage() {
        // Campos como emitidos pelo CLI 2.1.322.
        let result = r#"{"type":"result","is_error":false,"result":"ok","usage":{"iterations":[{"input_tokens":2,"output_tokens":4,"cache_read_input_tokens":11875,"cache_creation_input_tokens":6472}]},"modelUsage":{"claude-haiku-5-5":{"contextWindow":1000000}}}"#;
        let events = parse_line(result);
        assert_eq!(events[0], TurnEvent::Context { used_tokens: 18353, window_tokens: 1_000_000 });
        assert!(matches!(events[1], TurnEvent::Result { is_error: false, .. }));
    }

    #[test]
    fn parses_real_rate_limit_event() {
        let [TurnEvent::Rate(r)] = &parse_line(RATE_ALLOWED)[..] else { panic!() };
        assert!(!r.is_limited());
        assert_eq!((r.util_5h, r.util_7d, r.resets_5h, r.resets_7d), (Some(0.41), Some(0.46), Some(1794521400), Some(1794906000)));
    }

    #[test]
    fn successful_turn_keeps_streamed_text_and_session() {
        let end = end_of(
            &[
                r#"{"type":"system","subtype":"init","session_id":"s1"}"#,
                DELTA,
                RATE_ALLOWED,
                r#"{"type":"result","subtype":"success","is_error":false,"result":"ok","session_id":"s1"}"#,
            ],
            "",
        );
        assert_eq!(end, TurnEnd::Done { session_id: Some("s1".into()), text: "ok".into() });
    }

    #[test]
    fn rejected_rate_limit_ends_as_limited() {
        let end = end_of(&[RATE_REJECTED, r#"{"type":"result","is_error":true,"result":"x"}"#], "");
        assert_eq!(end, TurnEnd::Limited { resets_at: Some(99) });
        assert_eq!(end_of(&[RATE_REJECTED], ""), TurnEnd::Limited { resets_at: Some(99) });
    }

    #[test]
    fn limit_message_without_event_is_still_limited() {
        let end = end_of(&[r#"{"type":"result","is_error":true,"result":"You've hit your limit · resets 6pm"}"#], "");
        assert_eq!(end, TurnEnd::Limited { resets_at: None });
    }

    #[test]
    fn auth_and_generic_failures() {
        assert!(matches!(
            end_of(&[r#"{"type":"result","is_error":true,"result":"Please run /login"}"#], ""),
            TurnEnd::AuthError(_)
        ));
        assert_eq!(end_of(&[], "boom"), TurnEnd::Failed("boom".into()));
        assert!(matches!(end_of(&[], ""), TurnEnd::Failed(_)));
    }
}
