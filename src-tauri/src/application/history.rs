//! Histórico importado: indexação, consulta e continuação de conversas antigas.

use super::{
    ports::{ConversationRepository, EventSink, HistoryIndex, IdGenerator},
    AppError, AppResult,
};
use crate::domain::{
    chat::{Conversation, KIND_AGENT},
    history::{HistSession, HistoryStatus},
};
use std::sync::Arc;

const LIST_LIMIT: i64 = 100;
const UNTITLED: &str = "Conversa importada";

pub struct ManageHistory {
    pub index: Arc<dyn HistoryIndex>,
    pub conversations: Arc<dyn ConversationRepository>,
    pub events: Arc<dyn EventSink>,
    pub ids: Arc<dyn IdGenerator>,
}

impl ManageHistory {
    /// Operação demorada: deve ser chamada fora da thread da interface.
    pub fn reindex(&self) -> usize {
        self.index.reindex(&mut |done, total| self.events.history_progress(done, total))
    }

    pub fn status(&self) -> HistoryStatus {
        self.index.status()
    }

    pub fn list(&self, query: &str) -> Vec<HistSession> {
        self.index.list(query, LIST_LIMIT)
    }

    /// Traz uma conversa antiga para o app, pronta para ser continuada.
    pub fn open(&self, session_id: &str, model: &str) -> AppResult<Conversation> {
        let existing = self
            .conversations
            .list()?
            .into_iter()
            .find(|c| c.cli_session_id.as_deref() == Some(session_id));
        if let Some(conversation) = existing {
            return Ok(conversation);
        }
        let session = self
            .index
            .load(session_id)
            .ok_or_else(|| AppError("Não foi possível ler esta conversa.".into()))?;
        let cwd = session
            .cwd
            .ok_or_else(|| AppError("Esta conversa não informa a pasta de origem.".into()))?;
        let id = self.ids.new_id();
        let title = session.title.unwrap_or_else(|| UNTITLED.into());
        self.conversations.insert(&id, &title, model, &cwd, KIND_AGENT, Some(session_id))?;
        for turn in &session.turns {
            self.conversations.add_message(&id, &turn.role, &turn.text, None, None)?;
        }
        self.conversations
            .get(&id)?
            .ok_or_else(|| AppError("Conversa não encontrada.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;
    use crate::domain::history::{DialogueTurn, ImportedSession};

    #[test]
    fn opens_imported_session_once() {
        let world = World::with_accounts(vec![]);
        world.history.set_session(ImportedSession {
            cwd: Some("/tmp/loja".into()),
            title: Some("Frete".into()),
            turns: vec![
                DialogueTurn { role: "user".into(), text: "oi".into() },
                DialogueTurn { role: "assistant".into(), text: "olá".into() },
            ],
        });
        let uc = ManageHistory {
            index: world.history.clone(),
            conversations: world.store.clone(),
            events: world.events.clone(),
            ids: Arc::new(SequentialIds::default()),
        };
        let first = uc.open("sessao", "opus").unwrap();
        assert_eq!(first.title, "Frete");
        assert_eq!(first.cwd, "/tmp/loja");
        assert_eq!(world.store.messages(&first.id).unwrap().len(), 2);

        let again = uc.open("sessao", "opus").unwrap();
        assert_eq!(again.id, first.id, "não duplica a conversa");
    }
}
