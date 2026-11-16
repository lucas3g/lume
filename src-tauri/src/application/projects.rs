//! Gestão de projetos e de seus arquivos de conhecimento.

use super::{
    ports::{ConversationRepository, FileLoader, IdGenerator, KnowledgeFolder, MemoryStore, ProjectRepository, VersionControl},
    AppError, AppResult,
};
use crate::domain::{
    chat::{AttachmentKind, Conversation, KIND_AGENT},
    project::{
        folder_name, knowledge_file_names, normalize_folder, project_knowledge, project_knowledge_inline, MemoryFile, Project,
        ProjectFile, MAX_KNOWLEDGE_CHARS,
    },
};
use std::sync::Arc;

pub const NEW_SESSION_TITLE: &str = "Nova sessão";

/// Como o conhecimento do projeto chega ao assistente.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectKnowledge {
    None,
    /// Arquivos copiados para `dir`; o prompt leva só `listing`. `key` também muda
    /// quando muda o conteúdo, para a sessão retomada saber que precisa reler.
    OnDisk { dir: String, listing: String, key: String },
    /// A cópia em disco falhou: o conteúdo vai no prompt, como antes.
    Inline(String),
}

/// Deixa a pasta de conhecimento do projeto igual ao que está guardado e diz como entregá-lo.
pub fn sync_knowledge(projects: &dyn ProjectRepository, knowledge: &dyn KnowledgeFolder, project_id: &str) -> AppResult<ProjectKnowledge> {
    use sha2::{Digest, Sha256};
    let stored = projects.knowledge(project_id)?;
    if stored.is_empty() {
        let _ = knowledge.remove(project_id);
        return Ok(ProjectKnowledge::None);
    }
    let names = knowledge_file_names(&stored.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>());
    let files: Vec<(String, String)> = names.into_iter().zip(stored.iter().map(|(_, c)| c.clone())).collect();
    let Ok(dir) = knowledge.sync(project_id, &files) else {
        return Ok(project_knowledge_inline(&stored).map_or(ProjectKnowledge::None, ProjectKnowledge::Inline));
    };
    let listed: Vec<(String, usize)> = files.iter().map(|(n, c)| (n.clone(), c.chars().count())).collect();
    let listing = project_knowledge(&dir, &listed).unwrap_or_default();
    let mut digest = Sha256::new();
    for (name, content) in &files {
        digest.update(name.as_bytes());
        digest.update([0]);
        digest.update(content.as_bytes());
        digest.update([0]);
    }
    let key = format!("{listing}{:x}", digest.finalize());
    Ok(ProjectKnowledge::OnDisk { dir, listing, key })
}

/// Cria uma cópia isolada (git worktree) ao lado da pasta do projeto, com o primeiro nome
/// livre: nem pasta em uso (por outra cópia, por uma sessão em `taken` ou em disco) nem
/// ramo que já exista (o ramo fica quando a cópia é removida). `candidate(n)` dá o nome
/// da pasta e o ramo da n-ésima tentativa. Devolve (caminho, ramo).
pub fn add_free_worktree(
    vcs: &dyn VersionControl,
    folder: &str,
    taken: &[String],
    candidate: impl Fn(u32) -> (String, String),
) -> AppResult<(String, String)> {
    let mut used: Vec<String> = taken.iter().map(|t| normalize_folder(t)).collect();
    used.extend(vcs.worktrees(folder)?.iter().map(|w| normalize_folder(w)));
    let parent = normalize_folder(folder).rsplit_once(['/', '\\']).map(|(p, _)| p.to_owned()).unwrap_or_default();
    let mut n = 1;
    let (path, branch) = loop {
        let (name, branch) = candidate(n);
        let path = format!("{parent}/{name}");
        if !used.contains(&path) && !std::path::Path::new(&path).exists() && !vcs.has_branch(folder, &branch) {
            break (path, branch);
        }
        n += 1;
    };
    vcs.add_worktree(folder, &path, &branch)?;
    Ok((path, branch))
}

pub struct ManageProjects {
    pub projects: Arc<dyn ProjectRepository>,
    pub conversations: Arc<dyn ConversationRepository>,
    pub files: Arc<dyn FileLoader>,
    pub ids: Arc<dyn IdGenerator>,
    pub memory: Arc<dyn MemoryStore>,
    pub vcs: Arc<dyn VersionControl>,
    pub knowledge: Arc<dyn KnowledgeFolder>,
}

impl ManageProjects {
    /// Sessão em uma cópia isolada do repositório (git worktree), ao lado da pasta do
    /// projeto, em um ramo novo; a sessão continua pertencendo ao projeto.
    pub fn start_isolated_session(&self, project_id: &str, model: &str) -> AppResult<Conversation> {
        let project = self.projects.get(project_id)?.ok_or_else(|| AppError("Projeto não encontrado.".into()))?;
        let folder = project.folder.ok_or_else(|| AppError("Só projetos com pasta têm sessões isoladas.".into()))?;
        // Pastas de outras sessões também contam como usadas (a cópia pode ter sido removida
        // do git sem a conversa ter sido apagada).
        let sessions: Vec<String> = self.conversations.list()?.iter().map(|c| normalize_folder(&c.cwd)).collect();
        let base = folder_name(&folder);
        let (path, _) = add_free_worktree(&*self.vcs, &folder, &sessions, |n| {
            let name = format!("{base}-ramo-{n}");
            (name.clone(), format!("lume/{name}"))
        })?;
        let name = folder_name(&path);
        let id = self.ids.new_id();
        self.conversations.insert(&id, &format!("Sessão isolada · {name}"), model, &path, KIND_AGENT, None)?;
        self.conversations.set_project(&id, Some(project_id))?;
        self.conversations.get(&id)?.ok_or_else(|| AppError("Conversa não encontrada.".into()))
    }

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
        // A pasta e o versionamento de um projeto existente não mudam ao editar nome e instruções.
        let existing = match &id {
            Some(id) => self.projects.get(id)?,
            None => None,
        };
        let (folder, versioned) = existing.map_or((None, false), |p| (p.folder, p.versioned));
        let project = Project {
            id: id.unwrap_or_else(|| self.ids.new_id()),
            name: name.to_owned(),
            instructions: instructions.trim().to_owned(),
            folder,
            versioned,
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
                    versioned: false,
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

    /// Diz se o projeto tem versionamento; decide onde a esteira de bots trabalha.
    pub fn set_versioned(&self, id: &str, versioned: bool) -> AppResult<Project> {
        let mut project = self.projects.get(id)?.ok_or_else(|| AppError("Projeto não encontrado.".into()))?;
        project.versioned = versioned;
        self.projects.save(&project)?;
        Ok(project)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        self.projects.delete(id)?;
        // A cópia em disco é refeita a cada turno; sobrar não quebra nada, então a falha não impede a exclusão.
        let _ = self.knowledge.remove(id);
        Ok(())
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
        self.refresh_knowledge(project_id);
        Ok(())
    }

    pub fn remove_file(&self, project_id: &str, file_id: &str) -> AppResult<()> {
        self.projects.remove_file(file_id)?;
        // O arquivo removido sai do disco já, sem esperar a próxima mensagem do projeto.
        self.refresh_knowledge(project_id);
        Ok(())
    }

    /// A cópia em disco também é refeita a cada turno: falhar aqui não impede a edição.
    fn refresh_knowledge(&self, project_id: &str) {
        let _ = sync_knowledge(&*self.projects, &*self.knowledge, project_id);
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
            vcs: Arc::new(FakeVcs::default()),
            knowledge: world.knowledge.clone(),
        }
    }

    #[test]
    fn isolated_session_runs_in_a_worktree_beside_the_project_folder() {
        let world = World::with_accounts(vec![]);
        let vcs = Arc::new(FakeVcs::default());
        let uc = ManageProjects { vcs: vcs.clone(), ..use_case(&world, FakeFiles(vec![])) };
        let loose = uc.save(None, "Sem pasta", "").unwrap();
        assert!(uc.start_isolated_session(&loose.id, "opus").is_err());

        uc.start_session("/tmp/loja", "opus").unwrap();
        let project = uc.list().unwrap().into_iter().find(|p| p.folder.is_some()).unwrap();
        let session = uc.start_isolated_session(&project.id, "opus").unwrap();
        assert_eq!((session.cwd.as_str(), session.kind.as_str(), session.project_id.as_deref()), ("/tmp/loja-ramo-1", "agent", Some(project.id.as_str())));
        let second = uc.start_isolated_session(&project.id, "opus").unwrap();
        assert_eq!(second.cwd, "/tmp/loja-ramo-2");
        let asked = vcs.commits.lock().unwrap().clone();
        assert_eq!(asked[0], ("/tmp/loja".to_owned(), "worktree /tmp/loja-ramo-1 lume/loja-ramo-1".to_owned()));

        // Apagar a conversa não apaga a cópia em disco: o nome dela não pode ser reutilizado.
        ConversationRepository::delete(&*world.store, &session.id).unwrap();
        assert_eq!(uc.start_isolated_session(&project.id, "opus").unwrap().cwd, "/tmp/loja-ramo-3");
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
        assert_eq!(world.knowledge.0.lock().unwrap().len(), 1, "o arquivo já vai para o disco ao ser adicionado");
        uc.remove_file(&project.id, &stored[0].id).unwrap();
        assert!(world.knowledge.0.lock().unwrap().is_empty(), "e sai do disco ao ser removido");
        uc.add_files(&project.id, &["regras.md".into()]).unwrap();

        uc.assign("c1", Some(&project.id)).unwrap();
        assert_eq!(world.conversation("c1").project_id, Some(project.id.clone()));
        uc.delete(&project.id).unwrap();
        assert_eq!(world.conversation("c1").project_id, None, "a conversa sai do projeto excluído");
        assert!(uc.files(&project.id).unwrap().is_empty());
        assert!(world.knowledge.0.lock().unwrap().is_empty(), "a pasta do projeto excluído é apagada");
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
