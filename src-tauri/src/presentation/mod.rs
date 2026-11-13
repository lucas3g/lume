//! Camada de apresentação (Tauri) e raiz de composição: é o único lugar que
//! conhece as implementações concretas e as liga aos casos de uso.

pub mod commands;
pub mod events;
pub mod windows;

use crate::application::{
    accounts::ManageAccounts,
    conversations::ManageConversations,
    extensions::{ManageAgents, ManageConnectors, ManageSkills},
    history::ManageHistory,
    ports::{Clock, EventSink, FileLoader, FileSaver, IdGenerator, Settings, SwitchLog},
    projects::ManageProjects,
    send_message::SendMessage,
    store::BrowseStore,
    turn_setup::PrepareTurn,
    workspace::ManageWorkspace,
    AppError, AppResult, RunningTurns,
};
use crate::infrastructure::{
    claude_cli::{
        accounts::ClaudeCliAccounts, connectors::ClaudeCliConnectors, engine::ClaudeCliEngine, find_cli,
        plugins::ClaudeCliPlugins,
    },
    files::FsFileLoader,
    git_cli::GitCli,
    history_index::SqliteHistoryIndex,
    icons::FaviconFetcher,
    mcp_registry::McpRegistry,
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
    pub connectors: Arc<ManageConnectors>,
    pub store: BrowseStore,
    pub projects: ManageProjects,
    pub workspace: ManageWorkspace,
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

        Ok(Container {
            accounts: ManageAccounts {
                accounts: store.clone(),
                gateway: Arc::new(ClaudeCliAccounts::new(cli.clone(), profiles_dir, main_config.clone())),
                events: events.clone(),
                ids: ids.clone(),
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
            projects: ManageProjects {
                projects: store.clone(),
                conversations: store.clone(),
                files: files.clone(),
                ids: ids.clone(),
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
                vcs: Arc::new(GitCli),
            },
            send_message: SendMessage {
                accounts: store.clone(),
                conversations: store.clone(),
                switches: store.clone(),
                settings: store.clone(),
                engine: Arc::new(ClaudeCliEngine::new(cli)),
                prepare: PrepareTurn {
                    settings: store.clone(),
                    history: index,
                    skills: skills.clone(),
                    agents: store.clone(),
                    projects: store.clone(),
                    connectors: store.clone(),
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

/// Tarefas de inicialização que não devem atrasar a abertura da janela.
pub fn start_background_work(container: Arc<Container>) {
    let c = container.clone();
    tauri::async_runtime::spawn(async move {
        let _ = c.accounts.ensure_default().await;
    });
    tauri::async_runtime::spawn_blocking(move || container.history.reindex());
}
