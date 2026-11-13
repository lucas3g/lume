//! Gestão de projetos e de seus arquivos de conhecimento.

use super::{
    ports::{ConversationRepository, FileLoader, IdGenerator, MemoryStore, ProjectRepository},
    AppError, AppResult,
};
use crate::domain::{
    chat::{AttachmentKind, Conversation, KIND_AGENT},
    project::{folder_name, normalize_folder, MemoryFile, Project, ProjectFile, MAX_KNOWLEDGE_CHARS},
};
use std::sync::Arc;

const NEW_SESSION_TITLE: &str = "Nova sessão";

pub struct ManageProjects {
    pub projects: Arc<dyn ProjectRepository>,
    pub conversations: Arc<dyn ConversationRepository>,
    pub files: Arc<dyn FileLoader>,
    pub ids: Arc<dyn IdGenerator>,
    pub memory: Arc<dyn MemoryStore>,
}

impl ManageProjects {
    /// Pasta do projeto, exigida pelas operações de memória.
    fn folder_of(&self, project_id: &str) -> AppResult<String> {
        self.projects
            .get(project_id)?
            .and_then(|p| p.folder)
            .ok_or_else(|| AppError("A memória existe só em projetos com pasta.".into()))
    }

    pub fn memory_files(&self, project_id: &str) -> AppResult<Vec<MemoryFile>> {
        match self.projects.get(project_id)?.and_then(|p| p.folder) {
            Some(folder) => self.memory.list(&folder),
            None => Ok(vec![]),
        }
    }

    pub fn memory_read(&self, project_id: &str, name: &str) -> AppResult<String> {
        self.memory.read(&self.folder_of(project_id)?, name)
    }

    pub fn memory_write(&self, project_id: &str, name: &str, content: &str) -> AppResult<()> {
        self.memory.write(&self.folder_of(project_id)?, name, content)
    }

    pub fn memory_remove(&self, project_id: &str, name: &str) -> AppResult<()> {
        self.memory.remove(&self.folder_of(project_id)?, name)
    }

    pub fn list(&self) -> AppResult<Vec<Project>> {
        self.projects.list()
    }

    /// Cria (sem `id`) ou atualiza um projeto.
    pub fn save(&self, id: Option<String>, name: &str, instructions: &str) -> AppResult<Project> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError("Dê um nome para o projeto.".into()));
        }
        // A pasta de um projeto existente não muda ao editar nome e instruções.
        let folder = match &id {
            Some(id) => self.projects.get(id)?.and_then(|p| p.folder),
            None => None,
        };
        let project = Project {
            id: id.unwrap_or_else(|| self.ids.new_id()),
            name: name.to_owned(),
            instructions: instructions.trim().to_owned(),
            folder,
        };
        self.projects.save(&project)?;
        Ok(project)
    }

    /// Projeto da pasta: o que já aponta para ela ou um novo com o nome da pasta.
    /// Sessões de agente dessa pasta que estavam sem projeto passam a pertencer a ele.
    pub fn open_folder(&self, folder: &str) -> AppResult<Project> {
        let folder = normalize_folder(folder);
        if folder.is_empty() {
            return Err(AppError("Escolha a pasta do projeto.".into()));
        }
        let existing = self.projects.list()?.into_iter().find(|p| p.folder.as_deref() == Some(folder.as_str()));
        let project = match existing {
            Some(project) => project,
            None => {
                let project = Project {
                    id: self.ids.new_id(),
                    name: folder_name(&folder),
                    instructions: String::new(),
                    folder: Some(folder.clone()),
                };
                self.projects.save(&project)?;
                project
            }
        };
        for orphan in self.conversations.list()? {
            if orphan.is_agent() && orphan.project_id.is_none() && normalize_folder(&orphan.cwd) == folder {
                self.conversations.set_project(&orphan.id, Some(&project.id))?;
            }
        }
        Ok(project)
    }

    /// Coloca cada sessão de agente sem projeto no projeto da sua pasta, criando-o se preciso.
    pub fn adopt_sessions(&self) -> AppResult<()> {
        let orphans = self.conversations.list()?.into_iter().filter(|c| c.is_agent() && c.project_id.is_none());
        for orphan in orphans {
            self.open_folder(&orphan.cwd)?;
        }
        Ok(())
    }

    /// Nova sessão de agente na pasta, já dentro do projeto dela.
    pub fn start_session(&self, folder: &str, model: &str) -> AppResult<Conversation> {
        let project = self.open_folder(folder)?;
        let folder = project.folder.clone().unwrap_or_default();
        let id = self.ids.new_id();
        self.conversations.insert(&id, NEW_SESSION_TITLE, model, &folder, KIND_AGENT, None)?;
        self.conversations.set_project(&id, Some(&project.id))?;
        self.conversations
            .get(&id)?
            .ok_or_else(|| AppError("Conversa não encontrada.".into()))
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        self.projects.delete(id)
    }

    pub fn files(&self, project_id: &str) -> AppResult<Vec<ProjectFile>> {
        self.projects.files(project_id)
    }

    /// Acrescenta arquivos de texto ao conhecimento do projeto.
    pub fn add_files(&self, project_id: &str, paths: &[String]) -> AppResult<()> {
        let mut used: usize = self.projects.files(project_id)?.iter().map(|f| f.size).sum();
        for path in paths {
            let file = self.files.load(path)?;
            if file.kind != AttachmentKind::Text {
                return Err(AppError(format!("{}: o conhecimento do projeto aceita só arquivos de texto.", file.name)));
            }
            used += file.data.chars().count();
            if used > MAX_KNOWLEDGE_CHARS {
                return Err(AppError(format!("{}: o projeto passaria do limite de conhecimento.", file.name)));
            }
            self.projects.add_file(&self.ids.new_id(), project_id, &file.name, &file.data)?;
        }
        Ok(())
    }

    pub fn remove_file(&self, file_id: &str) -> AppResult<()> {
        self.projects.remove_file(file_id)
    }

    pub fn assign(&self, conversation_id: &str, project_id: Option<&str>) -> AppResult<()> {
        self.conversations.set_project(conversation_id, project_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;
    use crate::application::ports::ConversationRepository;
    use crate::domain::chat::Attachment;

    fn use_case(world: &World, files: FakeFiles) -> ManageProjects {
        ManageProjects {
            projects: world.store.clone(),
            conversations: world.store.clone(),
            files: Arc::new(files),
            ids: Arc::new(SequentialIds::default()),
            memory: Arc::new(FakeMemory::default()),
        }
    }

    #[test]
    fn memory_is_tied_to_the_project_folder() {
        let world = World::with_accounts(vec![]);
        let uc = use_case(&world, FakeFiles(vec![]));
        let loose = uc.save(None, "Sem pasta", "").unwrap();
        assert!(uc.memory_files(&loose.id).unwrap().is_empty());
        assert!(uc.memory_write(&loose.id, "a.md", "x").is_err());

        uc.start_session("/tmp/loja", "opus").unwrap();
        let project = uc.list().unwrap().into_iter().find(|p| p.folder.is_some()).unwrap();
        uc.memory_write(&project.id, "notas.md", "lembrar").unwrap();
        assert_eq!(uc.memory_files(&project.id).unwrap()[0].name, "notas.md");
        assert_eq!(uc.memory_read(&project.id, "notas.md").unwrap(), "lembrar");
        uc.memory_remove(&project.id, "notas.md").unwrap();
        assert!(uc.memory_files(&project.id).unwrap().is_empty());
    }

    #[test]
    fn saves_project_and_keeps_only_text_knowledge_within_limit() {
        let world = World::with_accounts(vec![]);
        let files = FakeFiles(vec![
            Attachment::text("regras.md", "Use REST."),
            Attachment { name: "foto.png".into(), kind: AttachmentKind::Image, media_type: "image/png".into(), data: "AAAA".into() },
            Attachment::text("enorme.txt", &"x".repeat(MAX_KNOWLEDGE_CHARS)),
        ]);
        let uc = use_case(&world, files);
        assert!(uc.save(None, " ", "").is_err());
        let project = uc.save(None, "Portal", "Responda em português.").unwrap();

        uc.add_files(&project.id, &["regras.md".into()]).unwrap();
        assert!(uc.add_files(&project.id, &["foto.png".into()]).unwrap_err().0.contains("só arquivos de texto"));
        assert!(uc.add_files(&project.id, &["enorme.txt".into()]).unwrap_err().0.contains("limite"));
        let stored = uc.files(&project.id).unwrap();
        assert_eq!((stored.len(), stored[0].name.as_str(), stored[0].size), (1, "regras.md", 9));

        uc.assign("c1", Some(&project.id)).unwrap();
        assert_eq!(world.conversation("c1").project_id, Some(project.id.clone()));
        uc.delete(&project.id).unwrap();
        assert_eq!(world.conversation("c1").project_id, None, "a conversa sai do projeto excluído");
        assert!(uc.files(&project.id).unwrap().is_empty());
    }

    #[test]
    fn folder_becomes_a_project_shared_by_its_sessions() {
        let world = World::with_accounts(vec![]);
        ConversationRepository::insert(&*world.store, "antiga", "Sessão antiga", "opus", "/tmp/loja", KIND_AGENT, None).unwrap();
        let uc = use_case(&world, FakeFiles(vec![]));

        let first = uc.start_session("/tmp/loja/", "opus").unwrap();
        let project = uc.list().unwrap().pop().unwrap();
        assert_eq!((project.name.as_str(), project.folder.as_deref()), ("loja", Some("/tmp/loja")));
        assert_eq!((first.cwd.as_str(), first.kind.as_str(), first.project_id.as_deref()), ("/tmp/loja", "agent", Some(project.id.as_str())));
        assert_eq!(world.conversation("antiga").project_id, Some(project.id.clone()), "sessão sem projeto é adotada");
        assert_eq!(world.conversation("c1").project_id, None, "conversa comum não é adotada");

        let second = uc.start_session("/tmp/loja", "opus").unwrap();
        assert_eq!(second.project_id, first.project_id);
        assert_eq!(uc.list().unwrap().len(), 1, "a mesma pasta não cria outro projeto");

        uc.save(Some(project.id.clone()), "Loja virtual", "Use REST.").unwrap();
        assert_eq!(uc.list().unwrap()[0].folder.as_deref(), Some("/tmp/loja"), "editar não perde a pasta");
        assert!(uc.start_session("  ", "opus").is_err());

        ConversationRepository::insert(&*world.store, "outra", "Outra pasta", "opus", "/tmp/portal", KIND_AGENT, None).unwrap();
        uc.adopt_sessions().unwrap();
        let names: Vec<String> = uc.list().unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Loja virtual", "portal"], "sessões soltas ganham o projeto da pasta");
        assert!(world.conversation("outra").project_id.is_some());
    }
}
