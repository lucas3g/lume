//! Tarefas agendadas: cadastro, cadência e a conversa em que cada execução roda.

use super::{
    ports::{Clock, ConversationRepository, IdGenerator, ProjectRepository, TaskRepository},
    AppError, AppResult,
};
use crate::domain::{
    chat::{Conversation, KIND_AGENT, KIND_CHAT},
    schedule::{next_run, validate_repeat, Repeat, Task},
};
use std::sync::Arc;

pub struct ManageTasks {
    pub tasks: Arc<dyn TaskRepository>,
    pub projects: Arc<dyn ProjectRepository>,
    pub conversations: Arc<dyn ConversationRepository>,
    pub ids: Arc<dyn IdGenerator>,
    pub clock: Arc<dyn Clock>,
    /// Pasta neutra das conversas sem projeto.
    pub chat_dir: String,
}

/// Dados de uma tarefa vindos da interface.
pub struct TaskInput {
    pub id: Option<String>,
    pub name: String,
    pub prompt: String,
    pub project_id: Option<String>,
    pub model: String,
    pub repeat: Repeat,
    pub tz_offset_minutes: i64,
}

impl ManageTasks {
    pub fn list(&self) -> AppResult<Vec<Task>> {
        self.tasks.list()
    }

    pub fn save(&self, input: TaskInput) -> AppResult<Task> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err(AppError("Dê um nome para a tarefa.".into()));
        }
        if input.prompt.trim().is_empty() {
            return Err(AppError("Escreva o que a tarefa deve fazer.".into()));
        }
        validate_repeat(&input.repeat).map_err(AppError)?;
        let now = self.clock.now();
        let existing = match &input.id {
            Some(id) => self.tasks.get(id)?,
            None => None,
        };
        // Mudar nome, prompt ou modelo não reinicia a contagem até a próxima rodada.
        let same_cadence = existing.as_ref().filter(|t| t.repeat == input.repeat && t.tz_offset_minutes == input.tz_offset_minutes);
        let next_run_at = match same_cadence {
            Some(t) => t.next_run_at,
            None => next_run(&input.repeat, now, input.tz_offset_minutes),
        };
        let task = Task {
            id: input.id.unwrap_or_else(|| self.ids.new_id()),
            name: name.to_owned(),
            prompt: input.prompt.trim().to_owned(),
            project_id: input.project_id,
            model: input.model,
            next_run_at,
            repeat: input.repeat,
            tz_offset_minutes: input.tz_offset_minutes,
            enabled: existing.as_ref().is_none_or(|t| t.enabled),
            last_run_at: existing.and_then(|t| t.last_run_at),
        };
        self.tasks.save(&task)?;
        Ok(task)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> AppResult<()> {
        let mut task = self.get(id)?;
        task.enabled = enabled;
        if enabled {
            task.next_run_at = next_run(&task.repeat, self.clock.now(), task.tz_offset_minutes);
        }
        self.tasks.save(&task)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        self.tasks.delete(id)
    }

    /// Tarefas cuja hora chegou.
    pub fn due(&self) -> AppResult<Vec<Task>> {
        let now = self.clock.now();
        Ok(self.tasks.list()?.into_iter().filter(|t| t.is_due(now)).collect())
    }

    /// Prepara uma execução: cria a conversa da rodada e agenda a próxima.
    /// Devolve a conversa e o prompt a enviar nela.
    pub fn begin_run(&self, id: &str) -> AppResult<(Conversation, String)> {
        let mut task = self.get(id)?;
        let now = self.clock.now();
        task.last_run_at = Some(now);
        task.next_run_at = next_run(&task.repeat, now, task.tz_offset_minutes);
        self.tasks.save(&task)?;

        let project = match &task.project_id {
            Some(pid) => self.projects.get(pid)?,
            None => None,
        };
        let (cwd, kind) = match project.as_ref().and_then(|p| p.folder.clone()) {
            Some(folder) => (folder, KIND_AGENT),
            None => (self.chat_dir.clone(), KIND_CHAT),
        };
        let conversation_id = self.ids.new_id();
        let title = format!("⏱ {}", task.name);
        self.conversations.insert(&conversation_id, &title, &task.model, &cwd, kind, None)?;
        if let Some(p) = &project {
            self.conversations.set_project(&conversation_id, Some(&p.id))?;
        }
        // Tarefas rodam sem ninguém olhando: edições de arquivo seguem sem perguntar.
        if kind == KIND_AGENT {
            self.conversations.set_permission_mode(&conversation_id, "acceptEdits")?;
        }
        let conversation = self
            .conversations
            .get(&conversation_id)?
            .ok_or_else(|| AppError("Conversa não encontrada.".into()))?;
        Ok((conversation, task.prompt))
    }

    fn get(&self, id: &str) -> AppResult<Task> {
        self.tasks.get(id)?.ok_or_else(|| AppError("Tarefa não encontrada.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;
    use crate::domain::project::Project;

    fn use_case(world: &World) -> ManageTasks {
        ManageTasks {
            tasks: world.store.clone(),
            projects: world.store.clone(),
            conversations: world.store.clone(),
            ids: Arc::new(SequentialIds::default()),
            clock: Arc::new(FixedClock(1000)),
            chat_dir: "/chat".into(),
        }
    }

    fn input(project_id: Option<&str>) -> TaskInput {
        TaskInput {
            id: None,
            name: "Resumo".into(),
            prompt: "Resuma o dia.".into(),
            project_id: project_id.map(Into::into),
            model: "sonnet".into(),
            repeat: Repeat::EveryMinutes { minutes: 30 },
            tz_offset_minutes: -180,
        }
    }

    #[test]
    fn saves_schedules_and_runs_in_a_fresh_conversation() {
        let world = World::with_accounts(vec![]);
        let uc = use_case(&world);
        assert!(uc.save(TaskInput { name: " ".into(), ..input(None) }).is_err());
        assert!(uc.save(TaskInput { repeat: Repeat::EveryMinutes { minutes: 1 }, ..input(None) }).is_err());
        let task = uc.save(input(None)).unwrap();
        assert_eq!((task.enabled, task.next_run_at), (true, 1000 + 1800));
        assert!(uc.due().unwrap().is_empty());

        uc.set_enabled(&task.id, false).unwrap();
        assert!(!uc.list().unwrap()[0].enabled);
        uc.set_enabled(&task.id, true).unwrap();

        let (conversation, prompt) = uc.begin_run(&task.id).unwrap();
        assert_eq!((conversation.title.as_str(), conversation.kind.as_str(), conversation.cwd.as_str()), ("⏱ Resumo", "chat", "/chat"));
        assert_eq!(prompt, "Resuma o dia.");
        let saved = uc.list().unwrap().remove(0);
        assert_eq!((saved.last_run_at, saved.next_run_at), (Some(1000), 2800));

        let renamed = uc.save(TaskInput { id: Some(task.id.clone()), name: "Resumo do dia".into(), ..input(None) }).unwrap();
        assert_eq!((renamed.next_run_at, renamed.last_run_at), (2800, Some(1000)), "editar sem mudar a cadência mantém a próxima rodada");
        let hourly = uc.save(TaskInput { id: Some(task.id.clone()), repeat: Repeat::EveryMinutes { minutes: 60 }, ..input(None) }).unwrap();
        assert_eq!(hourly.next_run_at, 1000 + 3600, "mudar a cadência reagenda");

        uc.delete(&task.id).unwrap();
        assert!(uc.list().unwrap().is_empty());
    }

    #[test]
    fn project_with_folder_runs_as_an_agent_session_that_accepts_edits() {
        let world = World::with_accounts(vec![]);
        let project = Project { id: "p1".into(), name: "Loja".into(), instructions: String::new(), folder: Some("/tmp/loja".into()) };
        ProjectRepository::save(&*world.store, &project).unwrap();
        let uc = use_case(&world);
        let task = uc.save(input(Some("p1"))).unwrap();
        let (conversation, _) = uc.begin_run(&task.id).unwrap();
        assert_eq!((conversation.kind.as_str(), conversation.cwd.as_str()), ("agent", "/tmp/loja"));
        assert_eq!((conversation.project_id.as_deref(), conversation.permission_mode.as_str()), (Some("p1"), "acceptEdits"));
    }
}
