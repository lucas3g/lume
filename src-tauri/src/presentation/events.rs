//! Entrega os eventos da aplicação à interface pelo canal de eventos do Tauri.

use crate::application::{ports::EventSink, ChatEvent};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub const CHAT_EVENT: &str = "chat";
pub const ACCOUNTS_CHANGED: &str = "accounts-changed";
pub const HISTORY_PROGRESS: &str = "history-progress";

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
}

impl TauriEvents {
    pub fn new(app: AppHandle) -> Self {
        TauriEvents { app }
    }
}

impl EventSink for TauriEvents {
    fn chat(&self, conversation_id: &str, event: ChatEvent) {
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
