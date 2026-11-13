//! Portas: o que os casos de uso exigem do mundo externo.
//! A infraestrutura implementa; a aplicação só conhece estes contratos.

use super::{AppResult, ChatEvent, TurnControl};
use crate::domain::{
    account::{Account, Identity, Provider, UsageLimit},
    chat::{Attachment, Conversation, Message, RateInfo, Switch, TurnEnd, TurnEvent, TurnRequest},
    extension::{AccountConnector, Agent, Connector, Skill, SkillRules},
    history::{HistSession, HistoryStatus, ImportedSession, Snippet},
    project::{MemoryFile, Project, ProjectFile},
    schedule::Task,
    store::{PluginListing, ServerListing},
    workspace::{FileEntry, FileView, WorkspaceStatus},
};
use async_trait::async_trait;

pub trait AccountRepository: Send + Sync {
    fn all(&self) -> AppResult<Vec<Account>>;
    fn insert(&self, id: &str, name: &str, provider: Provider, identity: &Identity, config_dir: Option<&str>) -> AppResult<()>;
    fn delete(&self, id: &str) -> AppResult<()>;
    fn set_enabled(&self, id: &str, enabled: bool) -> AppResult<()>;
    fn set_name(&self, id: &str, name: &str) -> AppResult<()>;
    fn set_identity(&self, id: &str, identity: &Identity) -> AppResult<()>;
    fn set_auth(&self, id: &str, ok: bool) -> AppResult<()>;
    /// Liga a conta a um perfil próprio (`None` = login do computador).
    fn set_config_dir(&self, id: &str, config_dir: Option<&str>) -> AppResult<()>;
    fn set_limited(&self, id: &str, until: Option<i64>) -> AppResult<()>;
    fn set_usage(&self, id: &str, rate: &RateInfo) -> AppResult<()>;
    fn reorder(&self, ids: &[String]) -> AppResult<()>;
}

pub trait ConversationRepository: Send + Sync {
    fn list(&self) -> AppResult<Vec<Conversation>>;
    fn get(&self, id: &str) -> AppResult<Option<Conversation>>;
    fn insert(&self, id: &str, title: &str, model: &str, cwd: &str, kind: &str, session: Option<&str>) -> AppResult<()>;
    /// Libera de vez uma ferramenta nesta conversa.
    fn allow_tool(&self, id: &str, tool: &str) -> AppResult<()>;
    fn update(&self, id: &str, title: Option<&str>, session: Option<&str>, model: Option<&str>) -> AppResult<()>;
    /// Registra em qual serviço a sessão da conversa existe.
    fn set_session_provider(&self, id: &str, provider: Provider) -> AppResult<()>;
    /// Cria `new_id` com as configurações de `source_id` (sem a sessão do provedor)
    /// e as mensagens de id menor que `before_message`.
    fn copy(&self, source_id: &str, new_id: &str, title: &str, before_message: i64) -> AppResult<()>;
    fn set_agent(&self, id: &str, agent_id: Option<&str>) -> AppResult<()>;
    fn set_project(&self, id: &str, project_id: Option<&str>) -> AppResult<()>;
    fn set_context_percent(&self, id: &str, percent: i64) -> AppResult<()>;
    fn set_effort(&self, id: &str, effort: &str) -> AppResult<()>;
    fn set_permission_mode(&self, id: &str, mode: &str) -> AppResult<()>;
    fn delete(&self, id: &str) -> AppResult<()>;
    fn messages(&self, conversation_id: &str) -> AppResult<Vec<Message>>;
    fn add_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        account_id: Option<&str>,
        meta: Option<&str>,
    ) -> AppResult<()>;
    fn message_count(&self, conversation_id: &str) -> AppResult<i64>;
    /// Metadados guardados com a mensagem (`None` se ela não existe ou não tem).
    fn message_meta(&self, message_id: i64) -> AppResult<Option<String>>;
    /// Grava um passo (JSON) do subagente criado pela chamada `parent`.
    fn add_subagent_step(&self, conversation_id: &str, parent: &str, step: &str) -> AppResult<()>;
    /// Passos de todos os subagentes da conversa, em ordem: (chamada, passo em JSON).
    fn subagent_steps(&self, conversation_id: &str) -> AppResult<Vec<(String, String)>>;
}

pub trait ProjectRepository: Send + Sync {
    fn list(&self) -> AppResult<Vec<Project>>;
    fn get(&self, id: &str) -> AppResult<Option<Project>>;
    fn save(&self, project: &Project) -> AppResult<()>;
    /// Remove o projeto e seus arquivos; as conversas ficam sem projeto.
    fn delete(&self, id: &str) -> AppResult<()>;
    fn files(&self, project_id: &str) -> AppResult<Vec<ProjectFile>>;
    /// Arquivos de conhecimento com o conteúdo: (nome, texto).
    fn knowledge(&self, project_id: &str) -> AppResult<Vec<(String, String)>>;
    fn add_file(&self, id: &str, project_id: &str, name: &str, content: &str) -> AppResult<()>;
    fn remove_file(&self, file_id: &str) -> AppResult<()>;
}

pub trait TaskRepository: Send + Sync {
    fn list(&self) -> AppResult<Vec<Task>>;
    fn get(&self, id: &str) -> AppResult<Option<Task>>;
    /// Cria ou substitui a tarefa.
    fn save(&self, task: &Task) -> AppResult<()>;
    fn delete(&self, id: &str) -> AppResult<()>;
}

/// Memória que o Claude Code mantém por pasta de projeto (arquivos Markdown).
pub trait MemoryStore: Send + Sync {
    fn list(&self, folder: &str) -> AppResult<Vec<MemoryFile>>;
    fn read(&self, folder: &str, name: &str) -> AppResult<String>;
    fn write(&self, folder: &str, name: &str, content: &str) -> AppResult<()>;
    fn remove(&self, folder: &str, name: &str) -> AppResult<()>;
}

/// Arquivos da pasta de uma sessão. Os caminhos são relativos a `root`.
pub trait Workspace: Send + Sync {
    fn entries(&self, root: &str, dir: &str) -> AppResult<Vec<FileEntry>>;
    fn read(&self, root: &str, path: &str) -> AppResult<FileView>;
}

pub trait VersionControl: Send + Sync {
    /// Situação da pasta; `is_repo` falso quando ela não é um repositório.
    fn status(&self, root: &str) -> WorkspaceStatus;
    fn diff(&self, root: &str, path: &str) -> AppResult<String>;
    /// Registra todas as alterações em um commit.
    fn commit(&self, root: &str, message: &str) -> AppResult<()>;
    /// Cria uma cópia de trabalho isolada do repositório em `path`, no ramo novo `branch`.
    fn add_worktree(&self, root: &str, path: &str, branch: &str) -> AppResult<()>;
    /// Caminhos das cópias de trabalho que o repositório já tem (inclusive a principal).
    fn worktrees(&self, root: &str) -> AppResult<Vec<String>>;
}

/// Lê um arquivo escolhido pelo usuário para uso como anexo.
pub trait FileLoader: Send + Sync {
    fn load(&self, path: &str) -> AppResult<Attachment>;
}

/// Grava um texto no local escolhido pelo usuário (ex.: exportar um artifact).
pub trait FileSaver: Send + Sync {
    fn save(&self, path: &str, content: &str) -> AppResult<()>;
}

pub trait SkillRepository: Send + Sync {
    /// Skills conhecidas: as pessoais deste computador e as vistas em uso.
    fn catalog(&self) -> AppResult<Vec<Skill>>;
    /// Registra os nomes de skills informados pelo provedor.
    fn remember(&self, names: &[String]) -> AppResult<()>;
    fn rules(&self, scope: &str) -> AppResult<SkillRules>;
    fn set_rule(&self, scope: &str, skill: &str, enabled: bool) -> AppResult<()>;
}

pub trait AgentRepository: Send + Sync {
    fn list(&self) -> AppResult<Vec<Agent>>;
    fn get(&self, id: &str) -> AppResult<Option<Agent>>;
    fn save(&self, agent: &Agent) -> AppResult<()>;
    fn delete(&self, id: &str) -> AppResult<()>;
}

pub trait ConnectorRepository: Send + Sync {
    fn list(&self) -> AppResult<Vec<Connector>>;
    fn insert(&self, id: &str, name: &str, config: &str) -> AppResult<()>;
    fn set_enabled(&self, id: &str, enabled: bool) -> AppResult<()>;
    /// Situação informada pelo provedor, pela chave MCP do conector.
    fn set_status(&self, key: &str, status: &str) -> AppResult<()>;
    fn delete(&self, id: &str) -> AppResult<()>;
}

/// Catálogo de plugins (skills) dos marketplaces configurados no Claude Code.
pub trait PluginCatalog: Send + Sync {
    fn listings(&self) -> AppResult<Vec<PluginListing>>;
    fn install(&self, id: &str) -> AppResult<()>;
    fn uninstall(&self, id: &str) -> AppResult<()>;
    /// Atualiza os marketplaces a partir das fontes.
    fn refresh(&self) -> AppResult<()>;
}

/// Diretório público de servidores MCP, paginado por cursor.
pub trait ServerDirectory: Send + Sync {
    fn search(&self, query: &str, cursor: Option<&str>) -> AppResult<(Vec<ServerListing>, Option<String>)>;
}

/// Ícone (favicon) do serviço de um domínio; `None` quando não há.
pub trait IconSource: Send + Sync {
    fn icon(&self, domain: &str) -> AppResult<Option<Vec<u8>>>;
}

/// Conectores que o Claude Code carrega para uma conta (`config_dir` é o perfil dela).
pub trait AccountConnectors: Send + Sync {
    fn list(&self, config_dir: Option<&str>) -> AppResult<Vec<AccountConnector>>;
    fn login(&self, config_dir: Option<&str>, name: &str) -> AppResult<()>;
    fn logout(&self, config_dir: Option<&str>, name: &str) -> AppResult<()>;
}

/// Conectores já configurados em outro aplicativo deste computador: (nome, configuração JSON).
pub trait ConnectorSource: Send + Sync {
    fn servers(&self) -> Vec<(String, String)>;
}

pub trait SwitchLog: Send + Sync {
    fn record(&self, from_name: &str, to_name: &str, conversation_title: &str) -> AppResult<()>;
    fn recent(&self) -> AppResult<Vec<Switch>>;
}

pub trait Settings: Send + Sync {
    /// Configuração booleana; ausente vale `default`.
    fn flag(&self, key: &str, default: bool) -> bool;
    fn set_flag(&self, key: &str, value: bool) -> AppResult<()>;
    /// Configurações cujo nome começa com o prefixo: (nome, valor).
    fn flags_with_prefix(&self, prefix: &str) -> AppResult<Vec<(String, bool)>>;
    /// Configuração de texto; `None` quando nunca foi gravada.
    fn text(&self, key: &str) -> Option<String>;
    fn set_text(&self, key: &str, value: &str) -> AppResult<()>;
}

/// Ouvinte dos eventos emitidos durante um turno.
pub type TurnListener<'a> = &'a mut (dyn for<'e> FnMut(&'e TurnEvent) + Send);

/// Executa um turno de conversa em nome de uma conta.
#[async_trait]
pub trait ChatEngine: Send + Sync {
    async fn run_turn(
        &self,
        request: &TurnRequest,
        control: &mut TurnControl,
        on_event: TurnListener<'_>,
    ) -> TurnEnd;
}

/// Consulta o uso atual de uma conta junto ao provedor, sem uma conversa.
#[async_trait]
pub trait UsageProbe: Send + Sync {
    async fn usage(&self, provider: Provider, config_dir: Option<&str>) -> AppResult<RateInfo>;

    /// Todos os limites da conta, inclusive os semanais por modelo. Quem só conhece
    /// as duas janelas do turno devolve apenas sessão e semana.
    async fn limits(&self, provider: Provider, config_dir: Option<&str>) -> AppResult<Vec<UsageLimit>> {
        let rate = self.usage(provider, config_dir).await?;
        Ok(UsageLimit::from_windows(rate.util_5h, rate.resets_5h, rate.util_7d, rate.resets_7d))
    }
}

/// Login e identidade das contas junto ao provedor.
#[async_trait]
pub trait AccountGateway: Send + Sync {
    /// O CLI que atende este tipo de conta está instalado neste computador.
    fn installed(&self, provider: Provider) -> bool;
    /// Cria o perfil isolado de uma conta e devolve o diretório de configuração.
    /// Falha quando o CLI do tipo só aceita o login do computador.
    fn prepare_profile(&self, provider: Provider, account_id: &str) -> AppResult<String>;
    fn discard_profile(&self, provider: Provider, config_dir: &str);
    async fn identity(&self, provider: Provider, config_dir: Option<&str>) -> Identity;
    async fn login(&self, provider: Provider, config_dir: &str) -> AppResult<Identity>;
}

pub trait HistoryIndex: Send + Sync {
    /// Indexa o que mudou e devolve quantas sessões foram atualizadas.
    fn reindex(&self, progress: &mut dyn FnMut(usize, usize)) -> usize;
    fn status(&self) -> HistoryStatus;
    fn search(&self, text: &str, limit: usize, exclude_session: Option<&str>) -> Vec<Snippet>;
    fn list(&self, query: &str, limit: i64) -> Vec<HistSession>;
    fn load(&self, session_id: &str) -> Option<ImportedSession>;
}

/// Saída de eventos para a interface.
pub trait EventSink: Send + Sync {
    fn chat(&self, conversation_id: &str, event: ChatEvent);
    fn accounts_changed(&self);
    fn history_progress(&self, done: usize, total: usize);
    /// Bytes que um terminal produziu.
    fn terminal_output(&self, terminal_id: &str, data: &[u8]);
}

/// Recebe os bytes que um terminal produz.
pub type TerminalOutput = Box<dyn Fn(&[u8]) + Send>;

/// Terminais interativos (pseudo-TTY) com o shell do usuário.
pub trait TerminalPort: Send + Sync {
    /// Abre um terminal na pasta; `on_output` recebe o que ele produz até fechar.
    fn open(&self, id: &str, cwd: &str, cols: u16, rows: u16, on_output: TerminalOutput) -> AppResult<()>;
    fn write(&self, id: &str, data: &[u8]) -> AppResult<()>;
    fn resize(&self, id: &str, cols: u16, rows: u16) -> AppResult<()>;
    fn close(&self, id: &str);
}

pub trait Clock: Send + Sync {
    fn now(&self) -> i64;
}

pub trait IdGenerator: Send + Sync {
    fn new_id(&self) -> String;
}
