//! Protocolo stream-json do Claude Code: leitura dos eventos e desfecho do turno.

use crate::domain::chat::{RateInfo, SubagentStep, TurnEnd, TurnEvent};
use chrono::{DateTime, Datelike, TimeZone};
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
        // Tarefas de outros tipos (ex.: comandos do terminal) não são subagentes, mas já podem ir para segundo plano.
        "task_started" if !v["task_type"].as_str().is_some_and(|t| t.contains("agent")) => return Some(TurnEvent::ToolTask(parent)),
        "task_started" => SubagentStep::Started {
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

    /// O resultado recebido diz que a conta bateu no limite ou perdeu o login: o turno acaba aí,
    /// mesmo com agentes em segundo plano (eles usam a mesma conta e também parariam), para a
    /// próxima conta assumir. Mais estrito que `into_end`: só o texto do resultado conta (um 429
    /// passageiro antes não basta).
    pub fn hit_wall(&self) -> bool {
        matches!(&self.result, Some((true, text)) if looks_like_limit(text) || looks_like_auth(text))
    }

    pub fn into_end(self, stderr: &str) -> TurnEnd {
        self.end_at(stderr, chrono::Local::now())
    }

    /// `now` serve para ler o horário de reset escrito no texto ("resets 1pm"), que o CLI dá no
    /// fuso local quando o evento do limite não traz o horário.
    fn end_at<Tz: TimeZone>(self, stderr: &str, now: DateTime<Tz>) -> TurnEnd {
        let stderr = stderr.trim();
        match self.result {
            Some((false, text)) => TurnEnd::Done {
                session_id: self.session_id,
                text: if self.streamed.trim().is_empty() { text } else { self.streamed },
            },
            Some((true, text)) if self.limited || looks_like_limit(&text) => {
                TurnEnd::Limited { resets_at: self.resets_at.or_else(|| reset_from_text(&text, &now)) }
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
    // Formatos do CLI: "You've hit your 5-hour limit", "You've reached your Fable limit", "You're out of extra usage".
    ["usage limit", "rate limit", "limit reached", "out of extra usage"].iter().any(|p| t.contains(p))
        || (t.contains("limit") && (t.contains("hit your") || t.contains("reached your")) && (t.contains("resets") || t.contains("usage")))
}

/// Horário do reset escrito no aviso do CLI: "resets 1pm", "resets 1:40pm (America/Sao_Paulo)" ou
/// "resets Nov 18 at 7am". Sem data, é a próxima vez que esse horário chega; com data, a próxima
/// vez que essa data chega.
fn reset_from_text<Tz: TimeZone>(text: &str, now: &DateTime<Tz>) -> Option<i64> {
    let lower = text.to_lowercase();
    let rest = &lower[lower.find("resets ")? + "resets ".len()..];
    let words: Vec<&str> = rest.split_whitespace().collect();
    let (date, clock) = match words.as_slice() {
        [month, day, "at", clock, ..] => (Some((month_number(month)?, day.trim_matches(',').parse::<u32>().ok()?)), *clock),
        [clock, ..] => (None, *clock),
        [] => return None,
    };
    let (hour, minute) = parse_clock(clock)?;
    let tz = now.timezone();
    let at = |y: i32, m: u32, d: u32| tz.with_ymd_and_hms(y, m, d, hour, minute, 0).earliest();
    let today = now.date_naive();
    let found = match date {
        None => {
            let t = at(today.year(), today.month(), today.day())?;
            if t > *now { t } else { t + chrono::Duration::days(1) }
        }
        Some((month, day)) => match at(today.year(), month, day)? {
            t if t > *now => t,
            _ => at(today.year() + 1, month, day)?,
        },
    };
    Some(found.timestamp())
}

fn month_number(word: &str) -> Option<u32> {
    let months = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    months.iter().position(|m| word.starts_with(m)).map(|i| i as u32 + 1)
}

/// "7am", "1:40pm", "12pm" → (hora, minuto) em 24 h.
fn parse_clock(word: &str) -> Option<(u32, u32)> {
    let word = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    let (digits, pm) = if let Some(d) = word.strip_suffix("pm") { (d, true) } else { (word.strip_suffix("am")?, false) };
    let (h, m) = digits.split_once(':').unwrap_or((digits, "0"));
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    if !(1..=12).contains(&h) || m > 59 {
        return None;
    }
    Some((h % 12 + if pm { 12 } else { 0 }, m))
}


/// Login da própria conta do Claude vencido ou inválido. Não vale para o login de um conector
/// ("Atlassian authentication required"): a conta segue boa e não deve ser marcada sem login.
fn looks_like_auth(text: &str) -> bool {
    let t = text.to_lowercase();
    ["/login", "not logged in", "authentication_error", "oauth token has expired", "invalid api key"].iter().any(|p| t.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn end_of(lines: &[&str], stderr: &str) -> TurnEnd {
        let mut seen = Observed::default();
        lines.iter().flat_map(|l| parse_line(l)).for_each(|e| seen.record(&e));
        seen.end_at(stderr, fixed_now())
    }

    /// 17/11/2026 11:00 em São Paulo (UTC-3).
    fn fixed_now() -> DateTime<chrono::FixedOffset> {
        chrono::FixedOffset::west_opt(3 * 3600).unwrap().with_ymd_and_hms(2026, 11, 17, 11, 0, 0).unwrap()
    }

    #[test]
    fn reads_the_reset_time_from_the_limit_text() {
        let now = fixed_now();
        let at = |m, d, h, min| now.timezone().with_ymd_and_hms(2026, m, d, h, min, 0).unwrap().timestamp();
        assert_eq!(reset_from_text("You've hit your 5-hour limit · resets 1pm (America/Sao_Paulo)", &now), Some(at(11, 17, 13, 0)));
        assert_eq!(reset_from_text("resets 1:40pm", &now), Some(at(11, 17, 13, 40)));
        assert_eq!(reset_from_text("resets 8:10am", &now), Some(at(11, 18, 8, 10)), "já passou hoje: amanhã");
        assert_eq!(reset_from_text("resets 12am", &now), Some(at(11, 18, 0, 0)));
        assert_eq!(reset_from_text("You've hit your Fable limit · resets Nov 22 at 10pm (America/Sao_Paulo)", &now), Some(at(11, 22, 22, 0)));
        assert_eq!(reset_from_text("You've hit your limit", &now), None);
        assert_eq!(reset_from_text("resets soon", &now), None);
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
        assert_eq!(parse_line(shell), [TurnEvent::ToolTask("t8".into())], "comandos não são subagentes, mas podem ir para segundo plano");
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
        let six_pm = fixed_now().timezone().with_ymd_and_hms(2026, 11, 17, 18, 0, 0).unwrap().timestamp();
        assert_eq!(end, TurnEnd::Limited { resets_at: Some(six_pm) }, "o horário vem do texto");
    }

    #[test]
    fn current_limit_messages_are_limited() {
        // Textos reais do CLI, copiados das transcrições.
        for text in [
            "You've hit your 5-hour limit · resets 1pm (America/Sao_Paulo)",
            "You've hit your Fable limit · resets Nov 18 at 7am (America/Sao_Paulo)",
            "You've reached your Fable limit. Run /usage-credits to continue",
            "You're out of extra usage · resets 3pm",
        ] {
            let line = serde_json::json!({"type": "result", "is_error": true, "result": text}).to_string();
            assert!(matches!(end_of(&[&line], ""), TurnEnd::Limited { .. }), "{text}");
        }
    }

    #[test]
    fn limited_result_ends_the_turn_even_with_background_agents() {
        let started = r#"{"type":"system","subtype":"task_started","tool_use_id":"t1","task_type":"local_agent","is_backgrounded":true}"#;
        let error = r#"{"type":"assistant","error":"rate_limit","message":{"content":[{"type":"text","text":"You've hit your 5-hour limit · resets 1pm"}]}}"#;
        let result = r#"{"type":"result","is_error":true,"result":"You've hit your 5-hour limit · resets 1pm"}"#;
        let mut seen = Observed::default();
        for line in [started, error, result] {
            parse_line(line).iter().for_each(|e| seen.record(e));
        }
        assert!(seen.hit_wall(), "o limite não espera os agentes em segundo plano");
        assert!(matches!(seen.into_end(""), TurnEnd::Limited { resets_at: Some(_) }));

        let mut ok = Observed::default();
        parse_line(r#"{"type":"result","is_error":false,"result":"pronto"}"#).iter().for_each(|e| ok.record(e));
        assert!(!ok.hit_wall());
        let mut login = Observed::default();
        parse_line(r#"{"type":"result","is_error":true,"result":"Please run /login"}"#).iter().for_each(|e| login.record(e));
        assert!(login.hit_wall());

        // Um 429 passageiro antes não faz de qualquer erro posterior um limite; nem o login de um conector.
        let mut retried = Observed::default();
        for line in [r#"{"type":"assistant","error":"rate_limit","message":{"content":[]}}"#, r#"{"type":"result","is_error":true,"result":"Prompt is too long"}"#] {
            parse_line(line).iter().for_each(|e| retried.record(e));
        }
        assert!(!retried.hit_wall());
        let mut connector = Observed::default();
        parse_line(r#"{"type":"result","is_error":true,"result":"Atlassian authentication required"}"#).iter().for_each(|e| connector.record(e));
        assert!(!connector.hit_wall());
    }

    #[test]
    fn other_caps_are_not_account_limits() {
        assert!(!looks_like_limit("You've reached your maximum turns limit"));
        assert!(!looks_like_limit("You hit your file size limit"));
        assert!(looks_like_limit("You've hit your session limit · resets 8:10am"));
    }

    #[test]
    fn auth_and_generic_failures() {
        assert!(matches!(
            end_of(&[r#"{"type":"result","is_error":true,"result":"Please run /login"}"#], ""),
            TurnEnd::AuthError(_)
        ));
        let expired = r#"{"type":"result","is_error":true,"result":"API Error: 401 {\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"OAuth token has expired.\"}}"}"#;
        assert!(matches!(end_of(&[expired], ""), TurnEnd::AuthError(_)));
        // O login de um conector não é o da conta.
        let connector = r#"{"type":"result","is_error":true,"result":"Atlassian authentication required"}"#;
        assert!(matches!(end_of(&[connector], ""), TurnEnd::Failed(_)));
        assert_eq!(end_of(&[], "boom"), TurnEnd::Failed("boom".into()));
        assert!(matches!(end_of(&[], ""), TurnEnd::Failed(_)));
    }
}
