//! Explorador de arquivos e alterações da pasta de uma sessão de agente.

use super::{
    ports::{ConversationRepository, VersionControl, Workspace},
    AppError, AppResult,
};
use crate::domain::workspace::{FileEntry, FileView, WorkspaceStatus};
use std::sync::Arc;

pub struct ManageWorkspace {
    pub conversations: Arc<dyn ConversationRepository>,
    pub workspace: Arc<dyn Workspace>,
    pub vcs: Arc<dyn VersionControl>,
}

impl ManageWorkspace {
    /// Conteúdo de uma pasta da sessão (`dir` vazio é a raiz).
    pub fn entries(&self, conversation_id: &str, dir: &str) -> AppResult<Vec<FileEntry>> {
        self.workspace.entries(&self.root(conversation_id)?, dir)
    }

    pub fn read(&self, conversation_id: &str, path: &str) -> AppResult<FileView> {
        self.workspace.read(&self.root(conversation_id)?, path)
    }

    pub fn status(&self, conversation_id: &str) -> AppResult<WorkspaceStatus> {
        Ok(self.vcs.status(&self.root(conversation_id)?))
    }

    pub fn diff(&self, conversation_id: &str, path: &str) -> AppResult<String> {
        self.vcs.diff(&self.root(conversation_id)?, path)
    }

    /// Registra todas as alterações da pasta em um commit.
    pub fn commit(&self, conversation_id: &str, message: &str) -> AppResult<()> {
        let message = message.trim();
        if message.is_empty() {
            return Err(AppError("Escreva a mensagem do commit.".into()));
        }
        self.vcs.commit(&self.root(conversation_id)?, message)
    }

    /// Só sessões de agente têm pasta própria; conversas comuns rodam em uma pasta neutra.
    fn root(&self, conversation_id: &str) -> AppResult<String> {
        let conversation = self
            .conversations
            .get(conversation_id)?
            .ok_or_else(|| AppError("Conversa não encontrada.".into()))?;
        if conversation.is_agent() {
            Ok(conversation.cwd)
        } else {
            Err(AppError("Esta conversa não está ligada a uma pasta.".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;
    use crate::domain::chat::KIND_AGENT;

    fn use_case(world: &World, vcs: Arc<FakeVcs>) -> ManageWorkspace {
        ManageWorkspace { conversations: world.store.clone(), workspace: Arc::new(FakeWorkspace), vcs }
    }

    #[test]
    fn only_agent_sessions_have_a_folder() {
        let world = World::with_accounts(vec![]);
        ConversationRepository::insert(&*world.store, "s1", "Sessão", "opus", "/tmp/loja", KIND_AGENT, None).unwrap();
        let vcs = Arc::new(FakeVcs::default());
        let uc = use_case(&world, vcs.clone());

        assert!(uc.entries("c1", "").is_err(), "conversa comum não tem pasta");
        assert_eq!(uc.entries("s1", "src").unwrap()[0].path, "/tmp/loja:src");
        assert_eq!(uc.read("s1", "a.txt").unwrap().path, "/tmp/loja:a.txt");
        assert!(uc.status("s1").unwrap().is_repo);

        assert!(uc.commit("s1", "  ").is_err(), "mensagem vazia não vira commit");
        uc.commit("s1", " Corrige filtro ").unwrap();
        assert_eq!(*vcs.commits.lock().unwrap(), [("/tmp/loja".to_owned(), "Corrige filtro".to_owned())]);
    }
}
