//! Camada de apresentação (Tauri) e raiz de composição: é o único lugar que
//! conhece as implementações concretas e as liga aos casos de uso.

pub mod commands;
pub mod events;
pub mod windows;

use crate::application::{
    accounts::ManageAccounts,
    conversations::ManageConversations,
    draft_agent::DraftAgent,
    extensions::{ManageAgents, ManageConnectors, ManageSkills},
    history::ManageHistory,
    pipeline::{final_answer, ManagePipelines},
    ports::{
        AccountGateway, ChatEngine, Clock, EventSink, FileLoader, FileSaver, IdGenerator, Settings,
        StageRunner, SwitchLog, UsageProbe, VersionControl,
    },
    projects::ManageProjects,
    send_message::SendMessage,
    store::BrowseStore,
    tasks::ManageTasks,
    terminal::ManageTerminals,
    turn_setup::PrepareTurn,
    workspace::ManageWorkspace,
    AppError, AppResult, RunningTurns,
};
use crate::infrastructure::{
    antigravity_cli::{AntigravityAccounts, AntigravityEngine, AntigravityUsage},
    codex_cli::{CodexAccounts, CodexEngine, CodexUsage},
    providers::ByProvider,
    claude_cli::{
        accounts::ClaudeCliAccounts, connectors::ClaudeCliConnectors, engine::ClaudeCliEngine, find_cli,
        plugins::ClaudeCliPlugins, usage::OauthUsage,
    },
    files::FsFileLoader,
    git_cli::GitCli,
    history_index::SqliteHistoryIndex,
    icons::FaviconFetcher,
    knowledge_fs::FsKnowledge,
    mcp_registry::McpRegistry,
    memory_fs::FsMemory,
    pty::PtyTerminals,
    skills::{DesktopConnectors, SqliteSkills},
    sqlite::SqliteStore,
    workspace_fs::FsWorkspace,
    SystemClock, UuidGenerator,
};
use events::TauriEvents;
use std::{path::PathBuf, sync::Arc};
use tauri::{AppHandle, Manager};

const DESKTOP_SESSIONS_DIR: &str = "Claude/claude-code-sessions";
const DESKTOP_CONFIG_FILE: &str = "Claude/claude_desktop_config.json";

pub struct Container {
    pub accounts: ManageAccounts,
    pub conversations: ManageConversations,
    pub history: ManageHistory,
    pub skills: ManageSkills,
    pub agents: ManageAgents,
    pub draft_agent: DraftAgent,
    pub connectors: Arc<ManageConnectors>,
    pub store: BrowseStore,
    pub projects: ManageProjects,
    pub workspace: ManageWorkspace,
    pub tasks: ManageTasks,
    pub pipelines: ManagePipelines,
    pub terminals: ManageTerminals,
    pub send_message: SendMessage,
    pub switches: Arc<dyn SwitchLog>,
    pub settings: Arc<dyn Settings>,
    pub events: Arc<dyn EventSink>,
    pub notifier: Arc<TauriEvents>,
    pub exports: Arc<dyn FileSaver>,
    pub running: RunningTurns,
    pub cli_found: bool,
}

impl Container {
    pub fn build(app: &AppHandle) -> AppResult<Container> {
        let io = |e: std::io::Error| AppError(format!("Não foi possível preparar a pasta de dados: {e}"));
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| AppError(format!("Pasta de dados indisponível: {e}")))?;
        let chat_dir = data_dir.join("chat");
        let knowledge = Arc::new(FsKnowledge::new(data_dir.join("knowledge")));
        let profiles_dir = data_dir.join("profiles");
        std::fs::create_dir_all(&chat_dir).map_err(io)?;
        std::fs::create_dir_all(&profiles_dir).map_err(io)?;

        let home = dirs::home_dir().unwrap_or_default();
        let main_config = home.join(".claude");
        let app_support = dirs::config_dir().unwrap_or_default();
        let desktop_dir = app_support.join(DESKTOP_SESSIONS_DIR);
        let cli = find_cli();
        let cli_found = cli.is_some();
        let cli = cli.unwrap_or_else(|| PathBuf::from("claude"));

        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(UuidGenerator);
        let store = Arc::new(SqliteStore::open(&data_dir.join("lume.db"), clock.clone())?);
        let notifier = Arc::new(TauriEvents::new(app.clone(), store.clone()));
        let vcs: Arc<dyn VersionControl> = Arc::new(GitCli);
        let events: Arc<dyn EventSink> = notifier.clone();
        let index = Arc::new(SqliteHistoryIndex::new(store.clone(), main_config.join("projects"), desktop_dir));
        let skills = Arc::new(SqliteSkills::new(store.clone(), main_config.join("skills"), main_config.join("plugins")));
        let connectors = Arc::new(ManageConnectors {
            connectors: store.clone(),
            desktop: Arc::new(DesktopConnectors::new(app_support.join(DESKTOP_CONFIG_FILE))),
            ids: ids.clone(),
            account_connectors: Arc::new(ClaudeCliConnectors::new(cli.clone())),
            accounts: store.clone(),
            settings: store.clone(),
            clock: clock.clone(),
            icons: Arc::new(FaviconFetcher::new(data_dir.join("icons"))),
        });
        let files: Arc<dyn FileLoader> = Arc::new(FsFileLoader);
        // Cada tipo de conta é atendido pelo CLI do seu serviço.
        let gateway: Arc<dyn AccountGateway> = Arc::new(ByProvider::<dyn AccountGateway> {
            claude: Arc::new(ClaudeCliAccounts::new(cli.clone(), profiles_dir.clone(), main_config.clone())),
            chatgpt: Arc::new(CodexAccounts::new(profiles_dir)),
            gemini: Arc::new(AntigravityAccounts),
        });
        let usage: Arc<dyn UsageProbe> = Arc::new(ByProvider::<dyn UsageProbe> {
            claude: Arc::new(OauthUsage::new(cli.clone(), main_config.clone())),
            chatgpt: Arc::new(CodexUsage),
            gemini: Arc::new(AntigravityUsage),
        });
        let engine: Arc<dyn ChatEngine> = Arc::new(ByProvider::<dyn ChatEngine> {
            claude: Arc::new(ClaudeCliEngine::new(cli.clone())),
            chatgpt: Arc::new(CodexEngine),
            gemini: Arc::new(AntigravityEngine),
        });

        Ok(Container {
            accounts: ManageAccounts {
                accounts: store.clone(),
                gateway,
                events: events.clone(),
                ids: ids.clone(),
                usage,
            },
            conversations: ManageConversations {
                conversations: store.clone(),
                ids: ids.clone(),
                chat_dir: chat_dir.to_string_lossy().into_owned(),
            },
            history: ManageHistory {
                index: index.clone(),
                conversations: store.clone(),
                events: events.clone(),
                ids: ids.clone(),
            },
            skills: ManageSkills { skills: skills.clone() },
            agents: ManageAgents { agents: store.clone(), conversations: store.clone(), ids: ids.clone() },
            draft_agent: DraftAgent {
                engine: engine.clone(),
                accounts: store.clone(),
                skills: skills.clone(),
                clock: clock.clone(),
                chat_dir: chat_dir.to_string_lossy().into_owned(),
            },
            projects: ManageProjects {
                projects: store.clone(),
                conversations: store.clone(),
                files: files.clone(),
                ids: ids.clone(),
                memory: Arc::new(FsMemory::new(main_config.join("projects"))),
                vcs: vcs.clone(),
                knowledge: knowledge.clone(),
            },
            connectors: connectors.clone(),
            store: BrowseStore {
                plugins: Arc::new(ClaudeCliPlugins::new(cli.clone(), main_config.clone())),
                directory: Arc::new(McpRegistry),
                connectors,
            },
            workspace: ManageWorkspace {
                conversations: store.clone(),
                workspace: Arc::new(FsWorkspace),
                vcs: vcs.clone(),
            },
            tasks: ManageTasks {
                tasks: store.clone(),
                projects: store.clone(),
                conversations: store.clone(),
                ids: ids.clone(),
                clock: clock.clone(),
                chat_dir: chat_dir.to_string_lossy().into_owned(),
            },
            pipelines: ManagePipelines {
                pipelines: store.clone(),
                agents: store.clone(),
                settings: store.clone(),
                catalog: Arc::new(crate::infrastructure::model_catalog::CliModels),
                projects: store.clone(),
                conversations: store.clone(),
                vcs: vcs.clone(),
                events: events.clone(),
                ids: ids.clone(),
                clock: clock.clone(),
                active: Default::default(),
                cancelled: Default::default(),
            },
            terminals: ManageTerminals {
                conversations: store.clone(),
                terminals: Arc::new(PtyTerminals::default()),
                events: events.clone(),
            },
            send_message: SendMessage {
                accounts: store.clone(),
                conversations: store.clone(),
                switches: store.clone(),
                settings: store.clone(),
                engine,
                prepare: PrepareTurn {
                    settings: store.clone(),
                    history: index,
                    skills: skills.clone(),
                    agents: store.clone(),
                    projects: store.clone(),
                    connectors: store.clone(),
                    conversations: store.clone(),
                    knowledge,
                },
                skills,
                connectors: store.clone(),
                files,
                events: events.clone(),
                clock,
            },
            switches: store.clone(),
            settings: store,
            events,
            notifier,
            exports: Arc::new(FsFileLoader),
            running: RunningTurns::default(),
            cli_found,
        })
    }
}

/// Intervalo entre verificações das tarefas agendadas.
const SCHEDULER_TICK: std::time::Duration = std::time::Duration::from_secs(30);

impl Container {
    /// Envia a mensagem em segundo plano; o andamento chega pelos eventos de conversa.
    /// Devolve `false` se a conversa já tem um turno em andamento.
    pub fn spawn_turn(self: &Arc<Self>, conversation_id: String, text: String, attachments: Vec<String>, pasted: Vec<crate::domain::chat::Attachment>) -> bool {
        let Some(control) = self.running.start(&conversation_id) else { return false };
        let container = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            if let Err(e) = container.send_message.execute(&conversation_id, &text, &attachments, pasted, control).await {
                container.events.chat(&conversation_id, crate::application::ChatEvent::Error { message: e.0 });
            }
            container.running.finish(&conversation_id);
        });
        true
    }

    /// Roda uma tarefa agora: cria a conversa da rodada e dispara o turno.
    pub fn run_task(self: &Arc<Self>, task_id: &str) -> AppResult<crate::domain::chat::Conversation> {
        let (conversation, prompt) = self.tasks.begin_run(task_id)?;
        self.spawn_turn(conversation.id.clone(), prompt, vec![], vec![]);
        Ok(conversation)
    }
}

/// Executor das etapas da esteira: cada etapa é um turno comum na conversa dela,
/// registrado entre os turnos em andamento para a interface poder autorizar e cancelar.
pub struct Bots(pub Arc<Container>);

#[async_trait::async_trait]
impl StageRunner for Bots {
    async fn run(&self, conversation_id: &str, prompt: &str, images: Vec<crate::domain::chat::Attachment>) -> AppResult<String> {
        let c = &self.0;
        let control = c.running.start(conversation_id).ok_or_else(|| AppError("A conversa da etapa já está em uso.".into()))?;
        let result = c.send_message.execute_stage(conversation_id, prompt, images, control).await;
        // Como em `spawn_turn`: o erro encerra o turno também na tela da conversa.
        if let Err(e) = &result {
            c.events.chat(conversation_id, crate::application::ChatEvent::Error { message: e.0.clone() });
        }
        c.running.finish(conversation_id);
        result?;
        final_answer(&c.pipelines.conversations.messages(conversation_id)?)
    }

    fn cancel(&self, conversation_id: &str) {
        self.0.running.cancel(conversation_id);
    }
}

impl Container {
    /// Conduz a execução da esteira em segundo plano até a próxima parada.
    pub fn spawn_pipeline(self: &Arc<Self>, run_id: String) {
        let container = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let bots = Bots(container.clone());
            let _ = container.pipelines.advance(&run_id, &bots).await;
        });
    }
}

/// Tarefas de inicialização que não devem atrasar a abertura da janela.
pub fn start_background_work(container: Arc<Container>) {
    let _ = container.pipelines.recover();
    let c = container.clone();
    tauri::async_runtime::spawn(async move {
        let _ = c.accounts.ensure_default().await;
        let _ = c.accounts.verify_machine_logins().await;
    });
    let c = container.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(SCHEDULER_TICK).await;
            for task in c.tasks.due().unwrap_or_default() {
                let _ = c.run_task(&task.id);
            }
        }
    });
    tauri::async_runtime::spawn_blocking(move || container.history.reindex());
}
