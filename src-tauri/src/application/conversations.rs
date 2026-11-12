//! Criação, listagem e exclusão de conversas.

use super::{
    ports::{ConversationRepository, IdGenerator},
    AppError, AppResult,
};
use crate::domain::chat::{Conversation, Message, EFFORTS, KIND_CHAT, PERMISSION_MODES};
use std::sync::Arc;

pub const NEW_CONVERSATION_TITLE: &str = "Nova conversa";

pub struct ManageConversations {
    pub conversations: Arc<dyn ConversationRepository>,
    pub ids: Arc<dyn IdGenerator>,
    /// Pasta neutra onde as conversas comuns são executadas.
    pub chat_dir: String,
}

impl ManageConversations {
    pub fn list(&self) -> AppResult<Vec<Conversation>> {
        self.conversations.list()
    }

    pub fn create(&self, model: &str) -> AppResult<Conversation> {
        let id = self.ids.new_id();
        self.conversations.insert(&id, NEW_CONVERSATION_TITLE, model, &self.chat_dir, KIND_CHAT, None)?;
        self.get(&id)
    }

    pub fn set_effort(&self, id: &str, effort: &str) -> AppResult<()> {
        if !EFFORTS.contains(&effort) {
            return Err(AppError("Nível de esforço desconhecido.".into()));
        }
        self.conversations.set_effort(id, effort)
    }

    pub fn set_permission_mode(&self, id: &str, mode: &str) -> AppResult<()> {
        if !PERMISSION_MODES.contains(&mode) {
            return Err(AppError("Modo de permissão desconhecido.".into()));
        }
        self.conversations.set_permission_mode(id, mode)
    }

    /// O usuário liberou de vez uma ferramenta nesta conversa.
    pub fn allow_tool(&self, id: &str, tool: &str) -> AppResult<()> {
        self.conversations.allow_tool(id, tool)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        self.conversations.delete(id)
    }

    pub fn messages(&self, id: &str) -> AppResult<Vec<Message>> {
        self.conversations.messages(id)
    }

    pub fn set_model(&self, id: &str, model: &str) -> AppResult<()> {
        self.conversations.update(id, None, None, Some(model))
    }

    fn get(&self, id: &str) -> AppResult<Conversation> {
        self.conversations
            .get(id)?
            .ok_or_else(|| AppError("Conversa não encontrada.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;

    #[test]
    fn new_conversations_start_automatic_and_accept_only_known_settings() {
        let world = World::with_accounts(vec![]);
        let uc = ManageConversations { conversations: world.store.clone(), ids: Arc::new(SequentialIds::default()), chat_dir: "/tmp".into() };
        let session = uc.create("opus").unwrap();
        assert_eq!((session.effort.as_str(), session.permission_mode.as_str()), ("medium", "auto"));

        uc.set_effort(&session.id, "max").unwrap();
        uc.set_permission_mode(&session.id, "plan").unwrap();
        assert!(uc.set_effort(&session.id, "infinito").is_err());
        assert!(uc.set_permission_mode(&session.id, "qualquer").is_err());
        let saved = world.conversation(&session.id);
        assert_eq!((saved.effort.as_str(), saved.permission_mode.as_str()), ("max", "plan"));
    }
}
