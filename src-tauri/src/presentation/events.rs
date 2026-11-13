//! Entrega os eventos da aplicação à interface pelo canal de eventos do Tauri e,
//! com a janela em segundo plano, avisa pelo sistema e marca o ícone do app.

use super::windows::MAIN_WINDOW;
use crate::application::{flags, ports::{EventSink, Settings}, ChatEvent};
use serde::Serialize;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

pub const CHAT_EVENT: &str = "chat";
pub const ACCOUNTS_CHANGED: &str = "accounts-changed";
pub const HISTORY_PROGRESS: &str = "history-progress";
const NOTICE_MAX_CHARS: usize = 140;

#[derive(Serialize, Clone)]
struct ChatPayload {
    conversation_id: String,
    #[serde(flatten)]
    event: ChatEvent,
}

#[derive(Serialize, Clone)]
struct ProgressPayload {
    done: usize,
    total: usize,
}

pub struct TauriEvents {
    app: AppHandle,
    settings: Arc<dyn Settings>,
    /// Conversas que precisam do usuário enquanto a janela está em segundo plano.
    waiting: Mutex<HashSet<String>>,
}

/// Título e texto do aviso do sistema para um evento, se ele merece um.
fn notice_for(event: &ChatEvent) -> Option<(&'static str, String)> {
    let short = |text: &str| -> String {
        let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
        match line.char_indices().nth(NOTICE_MAX_CHARS) {
            Some((cut, _)) => format!("{}…", &line[..cut]),
            None => line.to_owned(),
        }
    };
    match event {
        ChatEvent::Done { text } => Some(("Resposta pronta", short(text))),
        ChatEvent::Permission { tool, description, .. } if tool == "AskUserQuestion" => {
            Some(("O agente tem uma pergunta", short(description)))
        }
        ChatEvent::Permission { tool, description, .. } if tool == "ExitPlanMode" => {
            Some(("Plano pronto para aprovação", short(description)))
        }
        ChatEvent::Permission { tool, description, .. } => Some(("O agente pede sua aprovação", format!("{tool} · {}", short(description)))),
        ChatEvent::Exhausted { .. } => Some(("Todas as contas atingiram o limite", String::new())),
        ChatEvent::Error { message } => Some(("Não foi possível responder", short(message))),
        _ => None,
    }
}

impl TauriEvents {
    pub fn new(app: AppHandle, settings: Arc<dyn Settings>) -> Self {
        TauriEvents { app, settings, waiting: Mutex::default() }
    }

    fn main_focused(&self) -> bool {
        self.app.get_webview_window(MAIN_WINDOW).and_then(|w| w.is_focused().ok()).unwrap_or(false)
    }

    fn set_badge(&self, count: usize) {
        if let Some(window) = self.app.get_webview_window(MAIN_WINDOW) {
            let _ = window.set_badge_count((count > 0).then_some(count as i64));
        }
    }

    /// A janela voltou ao primeiro plano: nada mais está pendente de atenção.
    pub fn clear_waiting(&self) {
        self.waiting.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.set_badge(0);
    }

    fn notify(&self, conversation_id: &str, event: &ChatEvent) {
        let Some((title, body)) = notice_for(event) else { return };
        if self.main_focused() || !self.settings.flag(flags::NOTIFY_DESKTOP, true) {
            return;
        }
        let count = {
            let mut waiting = self.waiting.lock().unwrap_or_else(|e| e.into_inner());
            waiting.insert(conversation_id.to_owned());
            waiting.len()
        };
        self.set_badge(count);
        let _ = self.app.notification().builder().title(title).body(body).show();
    }
}

impl EventSink for TauriEvents {
    fn chat(&self, conversation_id: &str, event: ChatEvent) {
        self.notify(conversation_id, &event);
        let payload = ChatPayload { conversation_id: conversation_id.to_owned(), event };
        let _ = self.app.emit(CHAT_EVENT, payload);
    }

    fn accounts_changed(&self) {
        let _ = self.app.emit(ACCOUNTS_CHANGED, ());
    }

    fn history_progress(&self, done: usize, total: usize) {
        let _ = self.app.emit(HISTORY_PROGRESS, ProgressPayload { done, total });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_events_that_need_the_user_become_notices() {
        assert_eq!(notice_for(&ChatEvent::Done { text: "\n Pronto. \nmais".into() }).unwrap().1, "Pronto.");
        let ask = ChatEvent::Permission { request_id: "r".into(), tool: "AskUserQuestion".into(), input: "{}".into(), description: "Qual cor?".into() };
        assert_eq!(notice_for(&ask).unwrap().0, "O agente tem uma pergunta");
        assert!(notice_for(&ChatEvent::Delta { text: "x".into() }).is_none());
        let long = "a".repeat(200);
        assert_eq!(notice_for(&ChatEvent::Error { message: long }).unwrap().1.chars().count(), NOTICE_MAX_CHARS + 1);
    }
}
