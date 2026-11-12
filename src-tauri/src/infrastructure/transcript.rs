//! Leitura das transcrições (.jsonl) do Claude Code: extrai só o diálogo.

use crate::domain::history::{DialogueTurn, ImportedSession};
use serde_json::Value;
use std::io::BufRead;

const MAX_TURN_CHARS: usize = 4000;
const TITLE_CHARS: usize = 70;

fn truncate(s: &str, max_chars: usize) -> String {
    match s.char_indices().nth(max_chars) {
        Some((i, _)) => s[..i].to_owned(),
        None => s.to_owned(),
    }
}

fn message_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn title_of(record: &Value) -> Option<&str> {
    record["aiTitle"].as_str().or(record["title"].as_str()).or(record["summary"].as_str())
}

fn is_dialogue(kind: &str, record: &Value) -> bool {
    (kind == "user" || kind == "assistant")
        && !record["isSidechain"].as_bool().unwrap_or(false)
        && !record["isMeta"].as_bool().unwrap_or(false)
}

pub fn parse_transcript(reader: impl BufRead) -> ImportedSession {
    let mut session = ImportedSession::default();
    for line in reader.lines().map_while(Result::ok) {
        let Ok(record) = serde_json::from_str::<Value>(&line) else { continue };
        if session.cwd.is_none() {
            session.cwd = record["cwd"].as_str().map(Into::into);
        }
        let kind = record["type"].as_str().unwrap_or("");
        if kind == "ai-title" || kind == "summary" {
            if let Some(title) = title_of(&record) {
                session.title = Some(title.to_owned());
            }
            continue;
        }
        if !is_dialogue(kind, &record) {
            continue;
        }
        let text = message_text(&record["message"]["content"]);
        let text = text.trim();
        // Entradas do usuário que começam com "<" são avisos do sistema e comandos.
        if text.is_empty() || (kind == "user" && text.starts_with('<')) {
            continue;
        }
        session.turns.push(DialogueTurn { role: kind.into(), text: truncate(text, MAX_TURN_CHARS) });
    }
    if session.title.is_none() {
        session.title = session
            .turns
            .iter()
            .find(|t| t.role == "user")
            .map(|t| truncate(t.text.lines().next().unwrap_or(""), TITLE_CHARS));
    }
    session
}

#[cfg(test)]
pub(crate) const SAMPLE: &str = r#"{"type":"user","cwd":"/tmp/loja","message":{"role":"user","content":"<system-reminder>ignorar</system-reminder>"}}
{"type":"user","cwd":"/tmp/loja","message":{"role":"user","content":"Como criar um índice composto para pedidos?"}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"..."},{"type":"text","text":"Use CREATE INDEX em cliente_id e criado_em."},{"type":"tool_use","name":"Bash","input":{}}]}}
{"type":"assistant","isSidechain":true,"message":{"role":"assistant","content":"subagente"}}
{"type":"ai-title","aiTitle":"Índice de pedidos"}
linha inválida
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_dialogue() {
        let s = parse_transcript(SAMPLE.as_bytes());
        assert_eq!(s.cwd.as_deref(), Some("/tmp/loja"));
        assert_eq!(s.title.as_deref(), Some("Índice de pedidos"));
        assert_eq!(s.turns.len(), 2);
        assert_eq!(s.turns[0].role, "user");
        assert_eq!(s.turns[1].text, "Use CREATE INDEX em cliente_id e criado_em.");
    }

    #[test]
    fn falls_back_to_first_user_line_as_title() {
        let s = parse_transcript(r#"{"type":"user","message":{"content":"Primeira linha\nsegunda"}}"#.as_bytes());
        assert_eq!(s.title.as_deref(), Some("Primeira linha"));
    }
}
