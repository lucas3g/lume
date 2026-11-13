//! Criação, listagem e exclusão de conversas.

use super::{
    ports::{ConversationRepository, IdGenerator},
    AppError, AppResult,
};
use crate::domain::chat::{Conversation, Message, EFFORTS, KIND_CHAT, PERMISSION_MODES};
use std::sync::Arc;

pub const NEW_CONVERSATION_TITLE: &str = "Nova conversa";
const BRANCH_SUFFIX: &str = " · ramo";

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

    /// Nova conversa com as mesmas configurações e as mensagens anteriores a
    /// `before_message`; o diálogo copiado vira contexto do primeiro turno.
    pub fn branch(&self, id: &str, before_message: i64) -> AppResult<Conversation> {
        let source = self.get(id)?;
        let new_id = self.ids.new_id();
        let title = match source.title.strip_suffix(BRANCH_SUFFIX) {
            Some(base) => format!("{base}{BRANCH_SUFFIX}"),
            None => format!("{}{BRANCH_SUFFIX}", source.title),
        };
        self.conversations.copy(id, &new_id, &title, before_message)?;
        self.get(&new_id)
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

    /// Conteúdo (base64) da imagem colada de posição `index` numa mensagem enviada.
    pub fn message_image(&self, message_id: i64, index: usize) -> AppResult<String> {
        let missing = || AppError("A imagem não foi encontrada.".into());
        let meta = self.conversations.message_meta(message_id)?.ok_or_else(missing)?;
        let meta: serde_json::Value = serde_json::from_str(&meta).map_err(|_| missing())?;
        meta["images"][index]["data"].as_str().map(str::to_owned).ok_or_else(missing)
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

    #[test]
    fn message_image_reads_the_pasted_image_stored_with_the_message() {
        let world = World::with_accounts(vec![]);
        let uc = ManageConversations { conversations: world.store.clone(), ids: Arc::new(SequentialIds::default()), chat_dir: "/tmp".into() };
        let meta = r#"{"attachments":["a.png","b.png"],"images":[{"name":"a.png","data":"QQ=="},{"name":"b.png","data":"Qg=="}]}"#;
        world.store.add_message("c1", "user", "veja", None, Some(meta)).unwrap();
        world.store.add_message("c1", "assistant", "ok", None, None).unwrap();
        let [user, reply] = [0, 1].map(|i| world.store.messages("c1").unwrap()[i].id);

        assert_eq!(uc.message_image(user, 1).unwrap(), "Qg==");
        assert!(uc.message_image(user, 2).is_err());
        assert!(uc.message_image(reply, 0).is_err());
        assert!(uc.message_image(9999, 0).is_err());
    }

    #[test]
    fn branch_copies_settings_and_earlier_messages_without_the_session() {
        let world = World::with_accounts(vec![]);
        let uc = ManageConversations { conversations: world.store.clone(), ids: Arc::new(SequentialIds::default()), chat_dir: "/tmp".into() };
        world.store.update("c1", Some("Índices"), Some("sessao-1"), None).unwrap();
        world.store.set_effort("c1", "high").unwrap();
        for (role, text) in [("user", "oi"), ("assistant", "olá"), ("user", "tchau"), ("assistant", "até")] {
            world.store.add_message("c1", role, text, None, None).unwrap();
        }
        let third = world.store.messages("c1").unwrap()[2].id;

        let branch = uc.branch("c1", third).unwrap();
        assert_eq!((branch.title.as_str(), branch.effort.as_str(), branch.cli_session_id), ("Índices · ramo", "high", None));
        let copied: Vec<String> = world.store.messages(&branch.id).unwrap().into_iter().map(|m| m.content).collect();
        assert_eq!(copied, ["oi", "olá"]);
        assert_eq!(world.store.messages("c1").unwrap().len(), 4, "a original fica intacta");
        assert_eq!(uc.branch(&branch.id, i64::MAX).unwrap().title, "Índices · ramo", "o sufixo não se acumula");
    }
}
