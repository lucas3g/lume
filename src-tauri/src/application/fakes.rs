//! Dublês de teste das portas. O armazenamento usa o SQLite real em memória.

use super::{
    ports::*,
    AppError, AppResult, ChatEvent, TurnControl,
};
use crate::domain::{
    account::{Account, Identity},
    chat::{Attachment, Conversation, PermissionDecision, TurnEnd, TurnEvent, TurnRequest, KIND_CHAT},
    history::{HistSession, HistoryStatus, ImportedSession, Snippet},
    store::{InputPlace, PluginListing, ServerInput, ServerInstall, ServerListing},
    workspace::{FileEntry, FileView, WorkspaceStatus},
};
use crate::infrastructure::{skills::SqliteSkills, sqlite::SqliteStore};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

pub struct World {
    pub store: Arc<SqliteStore>,
    pub skills: Arc<SqliteSkills>,
    pub history: Arc<FakeHistory>,
    pub events: Arc<RecordingEvents>,
}

impl World {
    pub fn conversation(&self, id: &str) -> Conversation {
        ConversationRepository::get(&*self.store, id).unwrap().unwrap()
    }

    /// Cria as contas na ordem dada e uma conversa vazia "c1".
    pub fn with_accounts(accounts: Vec<Account>) -> World {
        let store = Arc::new(SqliteStore::memory());
        for a in &accounts {
            AccountRepository::insert(&*store, &a.id, &a.name, &Identity::default(), None).unwrap();
        }
        ConversationRepository::insert(&*store, "c1", "Nova conversa", "opus", "/tmp", KIND_CHAT, None).unwrap();
        let skills = Arc::new(SqliteSkills::new(store.clone(), "/pasta/inexistente".into(), "/pasta/inexistente".into()));
        World { store, skills, history: Arc::default(), events: Arc::default() }
    }
}

type Script = dyn Fn(&TurnRequest, usize) -> (Vec<TurnEvent>, TurnEnd) + Send + Sync;

/// Motor que responde conforme um roteiro; recebe o pedido e o número da chamada.
pub struct ScriptedEngine {
    script: Box<Script>,
    pub calls: Arc<Mutex<usize>>,
    /// Respostas a pedidos de autorização que chegaram ao motor.
    pub decisions: Arc<Mutex<Vec<PermissionDecision>>>,
}

impl ScriptedEngine {
    pub fn new(script: impl Fn(&TurnRequest, usize) -> (Vec<TurnEvent>, TurnEnd) + Send + Sync + 'static) -> Self {
        ScriptedEngine { script: Box::new(script), calls: Arc::default(), decisions: Arc::default() }
    }
}

#[async_trait]
impl ChatEngine for ScriptedEngine {
    async fn run_turn(
        &self,
        request: &TurnRequest,
        control: &mut TurnControl,
        on_event: TurnListener<'_>,
    ) -> TurnEnd {
        let call = {
            let mut n = self.calls.lock().unwrap();
            *n += 1;
            *n
        };
        let (events, end) = (self.script)(request, call);
        for event in &events {
            on_event(event);
        }
        while let Ok(decision) = control.decisions.try_recv() {
            self.decisions.lock().unwrap().push(decision);
        }
        end
    }
}

#[derive(Default)]
pub struct FakeHistory {
    snippets: Mutex<Vec<Snippet>>,
    session: Mutex<Option<ImportedSession>>,
}

impl FakeHistory {
    pub fn set(&self, snippets: Vec<Snippet>) {
        *self.snippets.lock().unwrap() = snippets;
    }

    pub fn set_session(&self, session: ImportedSession) {
        *self.session.lock().unwrap() = Some(session);
    }
}

impl HistoryIndex for FakeHistory {
    fn reindex(&self, _progress: &mut dyn FnMut(usize, usize)) -> usize {
        0
    }
    fn status(&self) -> HistoryStatus {
        HistoryStatus::default()
    }
    fn search(&self, _text: &str, _limit: usize, _exclude: Option<&str>) -> Vec<Snippet> {
        self.snippets.lock().unwrap().clone()
    }
    fn list(&self, _query: &str, _limit: i64) -> Vec<HistSession> {
        vec![]
    }
    fn load(&self, _session_id: &str) -> Option<ImportedSession> {
        self.session.lock().unwrap().clone()
    }
}

#[derive(Default)]
pub struct RecordingEvents(Mutex<Vec<ChatEvent>>);

impl RecordingEvents {
    pub fn chat_events(&self) -> Vec<ChatEvent> {
        self.0.lock().unwrap().clone()
    }
}

impl EventSink for RecordingEvents {
    fn chat(&self, _conversation_id: &str, event: ChatEvent) {
        self.0.lock().unwrap().push(event);
    }
    fn accounts_changed(&self) {}
    fn history_progress(&self, _done: usize, _total: usize) {}
}

pub struct FixedClock(pub i64);

impl Clock for FixedClock {
    fn now(&self) -> i64 {
        self.0
    }
}

#[derive(Default)]
pub struct SequentialIds(Mutex<usize>);

impl IdGenerator for SequentialIds {
    fn new_id(&self) -> String {
        let mut n = self.0.lock().unwrap();
        *n += 1;
        format!("id-{n}")
    }
}

pub struct FakeGateway {
    login: Result<Identity, String>,
    pub discarded: Arc<Mutex<Vec<String>>>,
}

impl FakeGateway {
    pub fn logged_in(identity: Identity) -> Self {
        FakeGateway { login: Ok(identity), discarded: Arc::default() }
    }

    pub fn failing(message: &str) -> Self {
        FakeGateway { login: Err(message.into()), discarded: Arc::default() }
    }
}

#[async_trait]
impl AccountGateway for FakeGateway {
    fn prepare_profile(&self, account_id: &str) -> AppResult<String> {
        Ok(format!("/perfis/{account_id}"))
    }
    fn discard_profile(&self, config_dir: &str) {
        self.discarded.lock().unwrap().push(config_dir.to_owned());
    }
    async fn identity(&self, _config_dir: Option<&str>) -> Identity {
        self.login.clone().unwrap_or_default()
    }
    async fn login(&self, _config_dir: &str) -> AppResult<Identity> {
        self.login.clone().map_err(AppError)
    }
}

/// Conectores "já configurados em outro aplicativo".
pub struct FixedConnectors(pub Vec<(String, String)>);

impl ConnectorSource for FixedConnectors {
    fn servers(&self) -> Vec<(String, String)> {
        self.0.clone()
    }
}

impl SqliteStore {
    /// Atalho de teste: cria um conector já ligado ou desligado.
    pub fn insert_connector(&self, id: &str, name: &str, config: &str, enabled: bool) {
        ConnectorRepository::insert(self, id, name, config).unwrap();
        ConnectorRepository::set_enabled(self, id, enabled).unwrap();
    }
}

/// Arquivos "em disco": carrega pelo nome; o que não está na lista não existe.
pub struct FakeFiles(pub Vec<Attachment>);

impl FileLoader for FakeFiles {
    fn load(&self, path: &str) -> AppResult<Attachment> {
        self.0
            .iter()
            .find(|a| a.name == path)
            .cloned()
            .ok_or_else(|| AppError(format!("{path}: não foi possível abrir o arquivo.")))
    }
}

/// Pasta de trabalho que só devolve o que lhe foi pedido, para conferir a raiz usada.
pub struct FakeWorkspace;

impl Workspace for FakeWorkspace {
    fn entries(&self, root: &str, dir: &str) -> AppResult<Vec<FileEntry>> {
        Ok(vec![FileEntry { name: dir.to_owned(), path: format!("{root}:{dir}"), is_dir: false }])
    }
    fn read(&self, root: &str, path: &str) -> AppResult<FileView> {
        Ok(FileView { path: format!("{root}:{path}"), size: 0, text: None })
    }
}

#[derive(Default)]
pub struct FakeVcs {
    /// Commits pedidos: (pasta, mensagem).
    pub commits: Mutex<Vec<(String, String)>>,
}

impl VersionControl for FakeVcs {
    fn status(&self, _root: &str) -> WorkspaceStatus {
        WorkspaceStatus::with_files(Some("main".into()), vec![])
    }
    fn diff(&self, _root: &str, _path: &str) -> AppResult<String> {
        Ok(String::new())
    }
    fn commit(&self, root: &str, message: &str) -> AppResult<()> {
        self.commits.lock().unwrap().push((root.to_owned(), message.to_owned()));
        Ok(())
    }
}

/// Catálogo de plugins em memória; instalar e remover só mudam o estado e ficam no registro.
pub struct FakePluginCatalog {
    listings: Mutex<Vec<PluginListing>>,
    pub log: Mutex<Vec<String>>,
}

impl FakePluginCatalog {
    pub fn new(listings: Vec<PluginListing>) -> Self {
        FakePluginCatalog { listings: Mutex::new(listings), log: Mutex::default() }
    }

    fn mark(&self, id: &str, installed: bool, action: &str) -> AppResult<()> {
        self.log.lock().unwrap().push(format!("{action} {id}"));
        let mut list = self.listings.lock().unwrap();
        let plugin = list.iter_mut().find(|p| p.id == id).ok_or_else(|| AppError("Plugin não encontrado.".into()))?;
        plugin.installed = installed;
        plugin.enabled = installed;
        Ok(())
    }
}

impl PluginCatalog for FakePluginCatalog {
    fn listings(&self) -> AppResult<Vec<PluginListing>> {
        Ok(self.listings.lock().unwrap().clone())
    }
    fn install(&self, id: &str) -> AppResult<()> {
        self.mark(id, true, "install")
    }
    fn uninstall(&self, id: &str) -> AppResult<()> {
        self.mark(id, false, "uninstall")
    }
    fn refresh(&self) -> AppResult<()> {
        Ok(())
    }
}

/// Diretório que responde sempre com um servidor remoto chamado como a busca.
pub struct FakeServerDirectory;

impl ServerDirectory for FakeServerDirectory {
    fn search(&self, query: &str, _cursor: Option<&str>) -> AppResult<(Vec<ServerListing>, Option<String>)> {
        let listing = ServerListing {
            name: format!("io.github.x/{query}"),
            title: query.to_owned(),
            description: String::new(),
            website: None,
            install: ServerInstall::Remote { url: format!("https://{query}.exemplo.com/mcp"), sse: false },
            inputs: vec![ServerInput { name: "Authorization".into(), description: String::new(), secret: true, required: false, place: InputPlace::Header }],
        };
        Ok((vec![listing], None))
    }
}
