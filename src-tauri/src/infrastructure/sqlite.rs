//! Armazenamento local em SQLite: implementa os repositórios da aplicação.

use crate::application::{
    ports::{
        AccountRepository, AgentRepository, Clock, ConnectorRepository, ConversationRepository, PipelineRepository,
        ProjectRepository, Settings, SwitchLog, TaskRepository,
    },
    AppError, AppResult,
};
use crate::domain::{
    account::{Account, Identity, Provider},
    chat::{Attachment, AttachmentKind, Conversation, Message, RateInfo, Switch, TokenUsage, DEFAULT_PERMISSION_MODE},
    extension::{connector_key, Agent, Connector},
    pipeline::{PipelineRun, Role, RunStatus, StageRun, StageStatus, WorkMode},
    project::{Project, ProjectFile},
    schedule::{Repeat, Task},
};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

const SCHEMA: &str = "
PRAGMA journal_mode=WAL;
CREATE TABLE IF NOT EXISTS accounts(
  id TEXT PRIMARY KEY, name TEXT NOT NULL, email TEXT, plan TEXT, config_dir TEXT,
  priority INTEGER NOT NULL, enabled INTEGER NOT NULL DEFAULT 1, auth_ok INTEGER NOT NULL DEFAULT 1,
  limited_until INTEGER, util_5h REAL, util_7d REAL, resets_5h INTEGER, login_expires_at INTEGER, resets_7d INTEGER);
CREATE TABLE IF NOT EXISTS conversations(
  id TEXT PRIMARY KEY, title TEXT NOT NULL, cli_session_id TEXT, model TEXT NOT NULL,
  cwd TEXT NOT NULL, agent_id TEXT, kind TEXT NOT NULL DEFAULT 'chat', allowed_tools TEXT NOT NULL DEFAULT '',
  project_id TEXT, context_percent INTEGER,
  effort TEXT NOT NULL DEFAULT 'medium', permission_mode TEXT NOT NULL DEFAULT 'manual', provider_models TEXT NOT NULL DEFAULT '{}',
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS messages(
  id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT NOT NULL, role TEXT NOT NULL,
  content TEXT NOT NULL, account_id TEXT, meta TEXT, created_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conversation_id, id);
CREATE TABLE IF NOT EXISTS subagent_steps(
  id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT NOT NULL, parent_id TEXT NOT NULL, step TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_subagent_steps_conv ON subagent_steps(conversation_id, id);
CREATE TABLE IF NOT EXISTS switches(
  id INTEGER PRIMARY KEY AUTOINCREMENT, at INTEGER NOT NULL, from_name TEXT NOT NULL,
  to_name TEXT NOT NULL, conversation_title TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS agents(id TEXT PRIMARY KEY, name TEXT NOT NULL, instructions TEXT NOT NULL, model TEXT);
CREATE TABLE IF NOT EXISTS connectors(
  id TEXT PRIMARY KEY, name TEXT NOT NULL, config TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 0, status TEXT);
CREATE TABLE IF NOT EXISTS skill_rules(
  scope TEXT NOT NULL, skill TEXT NOT NULL, enabled INTEGER NOT NULL, PRIMARY KEY(scope, skill));
CREATE TABLE IF NOT EXISTS skill_catalog(name TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS projects(
  id TEXT PRIMARY KEY, name TEXT NOT NULL, instructions TEXT NOT NULL, folder TEXT, versioned INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS pipeline_runs(
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL, jira_key TEXT, title TEXT NOT NULL, request TEXT NOT NULL,
  status TEXT NOT NULL, mode TEXT NOT NULL, workdir TEXT NOT NULL, branch TEXT, rounds INTEGER NOT NULL DEFAULT 0,
  skipped TEXT NOT NULL DEFAULT '', agents TEXT NOT NULL DEFAULT '{}', image_count INTEGER NOT NULL DEFAULT 0,
  jira_template TEXT, jira_url TEXT, efforts TEXT NOT NULL DEFAULT '{}', design INTEGER NOT NULL DEFAULT 0,
  budget_minutes INTEGER NOT NULL, note TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS pipeline_stages(
  id TEXT PRIMARY KEY, run_id TEXT NOT NULL, role TEXT NOT NULL, round INTEGER NOT NULL, status TEXT NOT NULL,
  conversation_id TEXT NOT NULL, input TEXT NOT NULL, output TEXT, started_at INTEGER NOT NULL, finished_at INTEGER);
CREATE INDEX IF NOT EXISTS idx_pipeline_stages_run ON pipeline_stages(run_id, started_at);
CREATE TABLE IF NOT EXISTS pipeline_images(
  run_id TEXT NOT NULL, position INTEGER NOT NULL, name TEXT NOT NULL, media_type TEXT NOT NULL, data TEXT NOT NULL,
  PRIMARY KEY(run_id, position));
CREATE TABLE IF NOT EXISTS project_files(
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL, name TEXT NOT NULL, content TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS tasks(
  id TEXT PRIMARY KEY, name TEXT NOT NULL, prompt TEXT NOT NULL, project_id TEXT, model TEXT NOT NULL,
  repeat TEXT NOT NULL, tz_offset INTEGER NOT NULL DEFAULT 0, enabled INTEGER NOT NULL DEFAULT 1,
  last_run_at INTEGER, next_run_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS hist_sessions(
  id TEXT PRIMARY KEY, source TEXT NOT NULL, cwd TEXT, title TEXT, path TEXT NOT NULL,
  mtime INTEGER NOT NULL, msg_count INTEGER NOT NULL);
CREATE VIRTUAL TABLE IF NOT EXISTS hist_fts USING fts5(
  session_id UNINDEXED, role UNINDEXED, body, tokenize='unicode61 remove_diacritics 2');
";

const ACCOUNT_COLUMNS: &str =
    "id,name,email,plan,config_dir,priority,enabled,auth_ok,limited_until,util_5h,util_7d,resets_5h,login_expires_at,resets_7d,provider";
const CONVERSATION_COLUMNS: &str = "id,title,cli_session_id,model,cwd,agent_id,kind,allowed_tools,project_id,context_percent,updated_at,effort,permission_mode,session_provider,provider_models";

/// Colunas acrescentadas depois da primeira versão; falham em silêncio se já existem.
const MIGRATIONS: [&str; 23] = [
    "ALTER TABLE accounts ADD COLUMN provider TEXT NOT NULL DEFAULT 'claude'",
    "ALTER TABLE conversations ADD COLUMN session_provider TEXT NOT NULL DEFAULT 'claude'",
    "ALTER TABLE accounts ADD COLUMN login_expires_at INTEGER",
    "ALTER TABLE accounts ADD COLUMN resets_7d INTEGER",
    "ALTER TABLE projects ADD COLUMN folder TEXT",
    "ALTER TABLE conversations ADD COLUMN effort TEXT NOT NULL DEFAULT 'medium'",
    // Conversas antigas foram criadas no modo que sempre pergunta; continuam assim.
    "ALTER TABLE conversations ADD COLUMN permission_mode TEXT NOT NULL DEFAULT 'manual'",
    "ALTER TABLE conversations ADD COLUMN context_percent INTEGER",
    "ALTER TABLE conversations ADD COLUMN project_id TEXT",
    "ALTER TABLE conversations ADD COLUMN agent_id TEXT",
    "ALTER TABLE conversations ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat'",
    "ALTER TABLE conversations ADD COLUMN allowed_tools TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE projects ADD COLUMN versioned INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE pipeline_runs ADD COLUMN skipped TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE pipeline_runs ADD COLUMN agents TEXT NOT NULL DEFAULT '{}'",
    "ALTER TABLE pipeline_runs ADD COLUMN image_count INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE pipeline_runs ADD COLUMN jira_template TEXT",
    "ALTER TABLE pipeline_runs ADD COLUMN jira_url TEXT",
    "ALTER TABLE conversations ADD COLUMN provider_models TEXT NOT NULL DEFAULT '{}'",
    "ALTER TABLE pipeline_runs ADD COLUMN efforts TEXT NOT NULL DEFAULT '{}'",
    "ALTER TABLE pipeline_runs ADD COLUMN design INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE conversations ADD COLUMN usage TEXT",
    "ALTER TABLE pipeline_stages ADD COLUMN usage TEXT",
];

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError(format!("Erro no banco de dados: {e}"))
    }
}

pub struct SqliteStore {
    conn: Mutex<Connection>,
    clock: Arc<dyn Clock>,
}

impl SqliteStore {
    pub fn open(path: &Path, clock: Arc<dyn Clock>) -> AppResult<SqliteStore> {
        Self::init(Connection::open(path)?, clock)
    }

    #[cfg(test)]
    pub fn memory() -> SqliteStore {
        let clock = Arc::new(crate::application::fakes::FixedClock(1000));
        Self::init(Connection::open_in_memory().unwrap(), clock).unwrap()
    }

    fn init(conn: Connection, clock: Arc<dyn Clock>) -> AppResult<SqliteStore> {
        conn.execute_batch(SCHEMA)?;
        for migration in MIGRATIONS {
            let _ = conn.execute(migration, []);
        }
        Ok(SqliteStore { conn: Mutex::new(conn), clock })
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn account_row(r: &Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: r.get(0)?,
        name: r.get(1)?,
        email: r.get(2)?,
        plan: r.get(3)?,
        config_dir: r.get(4)?,
        priority: r.get(5)?,
        enabled: r.get::<_, i64>(6)? != 0,
        auth_ok: r.get::<_, i64>(7)? != 0,
        limited_until: r.get(8)?,
        util_5h: r.get(9)?,
        util_7d: r.get(10)?,
        resets_5h: r.get(11)?,
        login_expires_at: r.get(12)?,
        resets_7d: r.get(13)?,
        provider: Provider::parse(&r.get::<_, String>(14)?),
    })
}

fn conversation_row(r: &Row<'_>) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: r.get(0)?,
        title: r.get(1)?,
        cli_session_id: r.get(2)?,
        model: r.get(3)?,
        cwd: r.get(4)?,
        agent_id: r.get(5)?,
        kind: r.get(6)?,
        allowed_tools: split_tools(&r.get::<_, String>(7)?),
        project_id: r.get(8)?,
        context_percent: r.get(9)?,
        updated_at: r.get(10)?,
        effort: r.get(11)?,
        permission_mode: r.get(12)?,
        session_provider: Provider::parse(&r.get::<_, String>(13)?),
        provider_models: serde_json::from_str(&r.get::<_, String>(14)?).unwrap_or_default(),
    })
}

fn split_tools(list: &str) -> Vec<String> {
    list.split(',').filter(|t| !t.is_empty()).map(Into::into).collect()
}

impl AccountRepository for SqliteStore {
    fn all(&self) -> AppResult<Vec<Account>> {
        let c = self.conn();
        let mut st = c.prepare(&format!("SELECT {ACCOUNT_COLUMNS} FROM accounts ORDER BY priority"))?;
        let rows = st.query_map([], account_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn insert(&self, id: &str, name: &str, provider: Provider, identity: &Identity, config_dir: Option<&str>) -> AppResult<()> {
        let c = self.conn();
        let next: i64 = c.query_row("SELECT COALESCE(MAX(priority),0)+1 FROM accounts", [], |r| r.get(0))?;
        c.execute(
            "INSERT INTO accounts(id,name,email,plan,config_dir,priority,login_expires_at,provider) VALUES(?,?,?,?,?,?,?,?)",
            params![id, name, identity.email, identity.plan, config_dir, next, identity.login_expires_at, provider.as_str()],
        )?;
        Ok(())
    }

    fn delete(&self, id: &str) -> AppResult<()> {
        self.conn().execute("DELETE FROM accounts WHERE id=?", [id])?;
        Ok(())
    }

    fn set_enabled(&self, id: &str, enabled: bool) -> AppResult<()> {
        self.conn()
            .execute("UPDATE accounts SET enabled=? WHERE id=?", params![enabled as i64, id])?;
        Ok(())
    }

    fn set_name(&self, id: &str, name: &str) -> AppResult<()> {
        self.conn().execute("UPDATE accounts SET name=? WHERE id=?", params![name, id])?;
        Ok(())
    }

    fn set_identity(&self, id: &str, identity: &Identity) -> AppResult<()> {
        self.conn().execute(
            "UPDATE accounts SET email=COALESCE(?,email), plan=COALESCE(?,plan), auth_ok=?, login_expires_at=? WHERE id=?",
            params![identity.email, identity.plan, identity.logged_in as i64, identity.login_expires_at, id],
        )?;
        Ok(())
    }

    fn set_auth(&self, id: &str, ok: bool) -> AppResult<()> {
        self.conn()
            .execute("UPDATE accounts SET auth_ok=? WHERE id=?", params![ok as i64, id])?;
        Ok(())
    }

    fn set_config_dir(&self, id: &str, config_dir: Option<&str>) -> AppResult<()> {
        self.conn()
            .execute("UPDATE accounts SET config_dir=? WHERE id=?", params![config_dir, id])?;
        Ok(())
    }

    fn set_limited(&self, id: &str, until: Option<i64>) -> AppResult<()> {
        self.conn()
            .execute("UPDATE accounts SET limited_until=? WHERE id=?", params![until, id])?;
        Ok(())
    }

    fn set_usage(&self, id: &str, rate: &RateInfo) -> AppResult<()> {
        self.conn().execute(
            "UPDATE accounts SET util_5h=COALESCE(?,util_5h), util_7d=COALESCE(?,util_7d),
             resets_5h=COALESCE(?,resets_5h), resets_7d=COALESCE(?,resets_7d) WHERE id=?",
            params![rate.util_5h, rate.util_7d, rate.resets_5h, rate.resets_7d, id],
        )?;
        Ok(())
    }

    fn reorder(&self, ids: &[String]) -> AppResult<()> {
        let c = self.conn();
        for (i, id) in ids.iter().enumerate() {
            c.execute("UPDATE accounts SET priority=? WHERE id=?", params![i as i64 + 1, id])?;
        }
        Ok(())
    }
}

impl ConversationRepository for SqliteStore {
    fn list(&self) -> AppResult<Vec<Conversation>> {
        let c = self.conn();
        let mut st = c.prepare(&format!(
            "SELECT {CONVERSATION_COLUMNS} FROM conversations ORDER BY updated_at DESC, rowid DESC LIMIT 200"
        ))?;
        let rows = st.query_map([], conversation_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn get(&self, id: &str) -> AppResult<Option<Conversation>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {CONVERSATION_COLUMNS} FROM conversations WHERE id=?"),
                [id],
                conversation_row,
            )
            .optional()?)
    }

    fn insert(&self, id: &str, title: &str, model: &str, cwd: &str, kind: &str, session: Option<&str>) -> AppResult<()> {
        let now = self.clock.now();
        self.conn().execute(
            "INSERT INTO conversations(id,title,cli_session_id,model,cwd,kind,permission_mode,created_at,updated_at)
             VALUES(?,?,?,?,?,?,?,?,?)",
            params![id, title, session, model, cwd, kind, DEFAULT_PERMISSION_MODE, now, now],
        )?;
        Ok(())
    }

    fn allow_tool(&self, id: &str, tool: &str) -> AppResult<()> {
        let Some(conversation) = ConversationRepository::get(self, id)? else { return Ok(()) };
        let mut tools = conversation.allowed_tools;
        if tools.iter().any(|t| t == tool) {
            return Ok(());
        }
        tools.push(tool.to_owned());
        self.conn()
            .execute("UPDATE conversations SET allowed_tools=? WHERE id=?", params![tools.join(","), id])?;
        Ok(())
    }

    fn update(&self, id: &str, title: Option<&str>, session: Option<&str>, model: Option<&str>) -> AppResult<()> {
        self.conn().execute(
            "UPDATE conversations SET title=COALESCE(?,title), cli_session_id=COALESCE(?,cli_session_id),
             model=COALESCE(?,model), updated_at=? WHERE id=?",
            params![title, session, model, self.clock.now(), id],
        )?;
        Ok(())
    }

    fn set_session_provider(&self, id: &str, provider: Provider) -> AppResult<()> {
        self.conn()
            .execute("UPDATE conversations SET session_provider=? WHERE id=?", params![provider.as_str(), id])?;
        Ok(())
    }

    fn copy(&self, source_id: &str, new_id: &str, title: &str, before_message: i64) -> AppResult<()> {
        let now = self.clock.now();
        let c = self.conn();
        c.execute(
            "INSERT INTO conversations(id,title,cli_session_id,model,cwd,agent_id,kind,allowed_tools,project_id,
             context_percent,effort,permission_mode,provider_models,created_at,updated_at)
             SELECT ?,?,NULL,model,cwd,agent_id,kind,allowed_tools,project_id,NULL,effort,permission_mode,provider_models,?,?
             FROM conversations WHERE id=?",
            params![new_id, title, now, now, source_id],
        )?;
        c.execute(
            "INSERT INTO messages(conversation_id,role,content,account_id,meta,created_at)
             SELECT ?,role,content,account_id,meta,created_at FROM messages
             WHERE conversation_id=? AND id<? ORDER BY id",
            params![new_id, source_id, before_message],
        )?;
        // Os subagentes das chamadas copiadas continuam abríveis no ramo; os de depois, não.
        c.execute(
            "INSERT INTO subagent_steps(conversation_id,parent_id,step)
             SELECT ?1,parent_id,step FROM subagent_steps
             WHERE conversation_id=?2 AND parent_id IN (
               SELECT json_extract(content,'$.id') FROM messages
               WHERE conversation_id=?2 AND role='tool' AND id<?3 AND json_valid(content))
             ORDER BY id",
            params![new_id, source_id, before_message],
        )?;
        Ok(())
    }

    fn set_agent(&self, id: &str, agent_id: Option<&str>) -> AppResult<()> {
        self.conn()
            .execute("UPDATE conversations SET agent_id=? WHERE id=?", params![agent_id, id])?;
        Ok(())
    }

    fn set_context_percent(&self, id: &str, percent: i64) -> AppResult<()> {
        self.conn()
            .execute("UPDATE conversations SET context_percent=? WHERE id=?", params![percent, id])?;
        Ok(())
    }

    fn add_usage(&self, id: &str, usage: &TokenUsage) -> AppResult<()> {
        // A conexão fica presa até o fim: ler e gravar não se misturam com outra soma.
        let conn = self.conn();
        let mut total = conversation_usage(&conn, id)?;
        total.add(usage);
        conn.execute("UPDATE conversations SET usage=? WHERE id=?", params![usage_json(&total)?, id])?;
        Ok(())
    }

    fn usage(&self, id: &str) -> AppResult<TokenUsage> {
        conversation_usage(&self.conn(), id)
    }

    fn set_provider_models(&self, id: &str, models: &std::collections::BTreeMap<String, String>) -> AppResult<()> {
        let text = serde_json::to_string(models).unwrap_or_else(|_| "{}".into());
        self.conn().execute("UPDATE conversations SET provider_models=? WHERE id=?", params![text, id])?;
        Ok(())
    }

    fn set_effort(&self, id: &str, effort: &str) -> AppResult<()> {
        self.conn().execute("UPDATE conversations SET effort=? WHERE id=?", params![effort, id])?;
        Ok(())
    }

    fn set_permission_mode(&self, id: &str, mode: &str) -> AppResult<()> {
        self.conn()
            .execute("UPDATE conversations SET permission_mode=? WHERE id=?", params![mode, id])?;
        Ok(())
    }

    fn set_project(&self, id: &str, project_id: Option<&str>) -> AppResult<()> {
        self.conn()
            .execute("UPDATE conversations SET project_id=? WHERE id=?", params![project_id, id])?;
        Ok(())
    }

    fn delete(&self, id: &str) -> AppResult<()> {
        let c = self.conn();
        c.execute("DELETE FROM messages WHERE conversation_id=?", [id])?;
        c.execute("DELETE FROM subagent_steps WHERE conversation_id=?", [id])?;
        c.execute("DELETE FROM conversations WHERE id=?", [id])?;
        Ok(())
    }

    fn messages(&self, conversation_id: &str) -> AppResult<Vec<Message>> {
        let c = self.conn();
        let mut st = c.prepare(
            "SELECT id,role,content,account_id,meta,created_at FROM messages WHERE conversation_id=? ORDER BY id",
        )?;
        let rows = st.query_map([conversation_id], |r| {
            Ok(Message {
                id: r.get(0)?,
                role: r.get(1)?,
                content: r.get(2)?,
                account_id: r.get(3)?,
                meta: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn add_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        account_id: Option<&str>,
        meta: Option<&str>,
    ) -> AppResult<()> {
        self.conn().execute(
            "INSERT INTO messages(conversation_id,role,content,account_id,meta,created_at) VALUES(?,?,?,?,?,?)",
            params![conversation_id, role, content, account_id, meta, self.clock.now()],
        )?;
        Ok(())
    }

    fn message_count(&self, conversation_id: &str) -> AppResult<i64> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*) FROM messages WHERE conversation_id=?",
            [conversation_id],
            |r| r.get(0),
        )?)
    }

    fn message_meta(&self, message_id: i64) -> AppResult<Option<String>> {
        Ok(self
            .conn()
            .query_row("SELECT meta FROM messages WHERE id=?", [message_id], |r| r.get(0))
            .optional()?
            .flatten())
    }

    fn add_subagent_step(&self, conversation_id: &str, parent: &str, step: &str) -> AppResult<()> {
        self.conn().execute(
            "INSERT INTO subagent_steps(conversation_id,parent_id,step) VALUES(?,?,?)",
            params![conversation_id, parent, step],
        )?;
        Ok(())
    }

    fn subagent_steps(&self, conversation_id: &str) -> AppResult<Vec<(String, String)>> {
        let c = self.conn();
        let mut st = c.prepare("SELECT parent_id,step FROM subagent_steps WHERE conversation_id=? ORDER BY id")?;
        let rows = st.query_map([conversation_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

impl AgentRepository for SqliteStore {
    fn list(&self) -> AppResult<Vec<Agent>> {
        let c = self.conn();
        let mut st = c.prepare("SELECT id,name,instructions,model FROM agents ORDER BY name")?;
        let rows = st.query_map([], agent_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn get(&self, id: &str) -> AppResult<Option<Agent>> {
        Ok(self
            .conn()
            .query_row("SELECT id,name,instructions,model FROM agents WHERE id=?", [id], agent_row)
            .optional()?)
    }

    fn save(&self, agent: &Agent) -> AppResult<()> {
        self.conn().execute(
            "INSERT INTO agents(id,name,instructions,model) VALUES(?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, instructions=excluded.instructions, model=excluded.model",
            params![agent.id, agent.name, agent.instructions, agent.model],
        )?;
        Ok(())
    }

    /// Remove o agente, suas regras de skills e o vínculo com as conversas.
    fn delete(&self, id: &str) -> AppResult<()> {
        let c = self.conn();
        c.execute("UPDATE conversations SET agent_id=NULL WHERE agent_id=?", [id])?;
        c.execute("DELETE FROM skill_rules WHERE scope=?", [id])?;
        c.execute("DELETE FROM agents WHERE id=?", [id])?;
        Ok(())
    }
}

fn agent_row(r: &Row<'_>) -> rusqlite::Result<Agent> {
    Ok(Agent { id: r.get(0)?, name: r.get(1)?, instructions: r.get(2)?, model: r.get(3)? })
}

impl ProjectRepository for SqliteStore {
    fn list(&self) -> AppResult<Vec<Project>> {
        let c = self.conn();
        let mut st = c.prepare("SELECT id,name,instructions,folder,versioned FROM projects ORDER BY name")?;
        let rows = st.query_map([], project_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn get(&self, id: &str) -> AppResult<Option<Project>> {
        Ok(self
            .conn()
            .query_row("SELECT id,name,instructions,folder,versioned FROM projects WHERE id=?", [id], project_row)
            .optional()?)
    }

    fn save(&self, project: &Project) -> AppResult<()> {
        self.conn().execute(
            "INSERT INTO projects(id,name,instructions,folder,versioned) VALUES(?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, instructions=excluded.instructions, folder=excluded.folder, versioned=excluded.versioned",
            params![project.id, project.name, project.instructions, project.folder, project.versioned as i64],
        )?;
        Ok(())
    }

    fn delete(&self, id: &str) -> AppResult<()> {
        let c = self.conn();
        c.execute("UPDATE conversations SET project_id=NULL WHERE project_id=?", [id])?;
        c.execute("DELETE FROM project_files WHERE project_id=?", [id])?;
        c.execute("DELETE FROM projects WHERE id=?", [id])?;
        Ok(())
    }

    fn files(&self, project_id: &str) -> AppResult<Vec<ProjectFile>> {
        let c = self.conn();
        let mut st = c.prepare("SELECT id,name,LENGTH(content) FROM project_files WHERE project_id=? ORDER BY name")?;
        let rows = st.query_map([project_id], |r| {
            Ok(ProjectFile { id: r.get(0)?, name: r.get(1)?, size: r.get::<_, i64>(2)? as usize })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn knowledge(&self, project_id: &str) -> AppResult<Vec<(String, String)>> {
        let c = self.conn();
        let mut st = c.prepare("SELECT name,content FROM project_files WHERE project_id=? ORDER BY name")?;
        let rows = st.query_map([project_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn add_file(&self, id: &str, project_id: &str, name: &str, content: &str) -> AppResult<()> {
        self.conn().execute(
            "INSERT INTO project_files(id,project_id,name,content) VALUES(?,?,?,?)",
            params![id, project_id, name, content],
        )?;
        Ok(())
    }

    fn remove_file(&self, file_id: &str) -> AppResult<()> {
        self.conn().execute("DELETE FROM project_files WHERE id=?", [file_id])?;
        Ok(())
    }
}

fn project_row(r: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project { id: r.get(0)?, name: r.get(1)?, instructions: r.get(2)?, folder: r.get(3)?, versioned: r.get::<_, i64>(4)? != 0 })
}

impl ConnectorRepository for SqliteStore {
    fn list(&self) -> AppResult<Vec<Connector>> {
        let c = self.conn();
        let mut st = c.prepare("SELECT id,name,config,enabled,status FROM connectors ORDER BY name")?;
        let rows = st.query_map([], |r| {
            Ok(Connector {
                id: r.get(0)?,
                name: r.get(1)?,
                config: r.get(2)?,
                enabled: r.get::<_, i64>(3)? != 0,
                status: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn insert(&self, id: &str, name: &str, config: &str) -> AppResult<()> {
        self.conn()
            .execute("INSERT INTO connectors(id,name,config) VALUES(?,?,?)", params![id, name, config])?;
        Ok(())
    }

    fn set_enabled(&self, id: &str, enabled: bool) -> AppResult<()> {
        self.conn().execute(
            "UPDATE connectors SET enabled=?1, status=CASE WHEN ?1 THEN status ELSE NULL END WHERE id=?2",
            params![enabled as i64, id],
        )?;
        Ok(())
    }

    fn set_status(&self, key: &str, status: &str) -> AppResult<()> {
        let matching: Vec<String> = ConnectorRepository::list(self)?
            .into_iter()
            .filter(|c| connector_key(&c.name) == key)
            .map(|c| c.id)
            .collect();
        let c = self.conn();
        for id in matching {
            c.execute("UPDATE connectors SET status=? WHERE id=?", params![status, id])?;
        }
        Ok(())
    }

    fn delete(&self, id: &str) -> AppResult<()> {
        self.conn().execute("DELETE FROM connectors WHERE id=?", [id])?;
        Ok(())
    }
}

const TASK_COLUMNS: &str = "id,name,prompt,project_id,model,repeat,tz_offset,enabled,last_run_at,next_run_at";

fn task_row(r: &Row<'_>) -> rusqlite::Result<Task> {
    let repeat: String = r.get(5)?;
    let repeat: Repeat = serde_json::from_str(&repeat).unwrap_or(Repeat::EveryMinutes { minutes: 60 });
    Ok(Task {
        id: r.get(0)?,
        name: r.get(1)?,
        prompt: r.get(2)?,
        project_id: r.get(3)?,
        model: r.get(4)?,
        repeat,
        tz_offset_minutes: r.get(6)?,
        enabled: r.get::<_, i64>(7)? != 0,
        last_run_at: r.get(8)?,
        next_run_at: r.get(9)?,
    })
}

impl TaskRepository for SqliteStore {
    fn list(&self) -> AppResult<Vec<Task>> {
        let c = self.conn();
        let mut st = c.prepare(&format!("SELECT {TASK_COLUMNS} FROM tasks ORDER BY name"))?;
        let rows = st.query_map([], task_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn get(&self, id: &str) -> AppResult<Option<Task>> {
        Ok(self.conn().query_row(&format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"), [id], task_row).optional()?)
    }

    fn save(&self, task: &Task) -> AppResult<()> {
        let repeat = serde_json::to_string(&task.repeat).unwrap_or_default();
        self.conn().execute(
            "INSERT OR REPLACE INTO tasks(id,name,prompt,project_id,model,repeat,tz_offset,enabled,last_run_at,next_run_at)
             VALUES(?,?,?,?,?,?,?,?,?,?)",
            params![task.id, task.name, task.prompt, task.project_id, task.model, repeat, task.tz_offset_minutes,
                task.enabled as i64, task.last_run_at, task.next_run_at],
        )?;
        Ok(())
    }

    fn delete(&self, id: &str) -> AppResult<()> {
        self.conn().execute("DELETE FROM tasks WHERE id=?", [id])?;
        Ok(())
    }
}

const RUN_COLUMNS: &str = "id,project_id,jira_key,title,request,status,mode,workdir,branch,rounds,budget_minutes,note,created_at,updated_at,skipped,agents,image_count,jira_template,jira_url,efforts,design";
const STAGE_COLUMNS: &str = "id,run_id,role,round,status,conversation_id,input,output,started_at,finished_at";

fn run_row(r: &Row<'_>) -> rusqlite::Result<PipelineRun> {
    let design: bool = r.get(20)?;
    let mut skipped: Vec<Role> = r.get::<_, String>(14)?.split(',').filter_map(Role::parse).collect();
    // Sem design pedido (inclusive as execuções de antes dele), o Designer e a aprovação dele não rodam.
    if !design {
        for role in Role::OPT_IN {
            if !skipped.contains(&role) {
                skipped.push(role);
            }
        }
    }
    Ok(PipelineRun {
        id: r.get(0)?,
        project_id: r.get(1)?,
        jira_key: r.get(2)?,
        title: r.get(3)?,
        request: r.get(4)?,
        status: RunStatus::parse(&r.get::<_, String>(5)?),
        mode: WorkMode::parse(&r.get::<_, String>(6)?),
        workdir: r.get(7)?,
        branch: r.get(8)?,
        rounds: r.get(9)?,
        budget_minutes: r.get(10)?,
        note: r.get(11)?,
        created_at: r.get(12)?,
        updated_at: r.get(13)?,
        skipped,
        design,
        agents: serde_json::from_str(&r.get::<_, String>(15)?).unwrap_or_default(),
        image_count: r.get(16)?,
        jira_template: r.get(17)?,
        jira_url: r.get(18)?,
        efforts: serde_json::from_str(&r.get::<_, String>(19)?).unwrap_or_default(),
    })
}

/// Gasto gravado em JSON. Um registro ilegível é erro: somar por cima dele apagaria o acumulado.
fn parse_usage(text: &str) -> AppResult<TokenUsage> {
    serde_json::from_str(text).map_err(|e| AppError(format!("Uso de tokens gravado ilegível: {e}")))
}

fn usage_json(usage: &TokenUsage) -> AppResult<String> {
    serde_json::to_string(usage).map_err(|e| AppError(format!("Não foi possível gravar o uso de tokens: {e}")))
}

/// Tokens que a conversa gastou (vazio se ainda nenhum turno informou).
fn conversation_usage(conn: &Connection, id: &str) -> AppResult<TokenUsage> {
    let stored: Option<String> = conn
        .query_row("SELECT usage FROM conversations WHERE id=?", params![id], |r| r.get(0))
        .optional()?
        .flatten();
    stored.as_deref().map_or(Ok(TokenUsage::default()), parse_usage)
}

fn stage_row(r: &Row<'_>) -> rusqlite::Result<Option<StageRun>> {
    let Some(role) = Role::parse(&r.get::<_, String>(2)?) else { return Ok(None) };
    Ok(Some(StageRun {
        id: r.get(0)?,
        run_id: r.get(1)?,
        role,
        round: r.get(3)?,
        status: StageStatus::parse(&r.get::<_, String>(4)?),
        conversation_id: r.get(5)?,
        input: r.get(6)?,
        output: r.get(7)?,
        started_at: r.get(8)?,
        finished_at: r.get(9)?,
    }))
}

impl PipelineRepository for SqliteStore {
    fn runs(&self) -> AppResult<Vec<PipelineRun>> {
        let c = self.conn();
        let mut st = c.prepare(&format!("SELECT {RUN_COLUMNS} FROM pipeline_runs ORDER BY created_at DESC, rowid DESC"))?;
        let rows = st.query_map([], run_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn run(&self, id: &str) -> AppResult<Option<PipelineRun>> {
        Ok(self.conn().query_row(&format!("SELECT {RUN_COLUMNS} FROM pipeline_runs WHERE id=?"), [id], run_row).optional()?)
    }

    fn save_run(&self, run: &PipelineRun) -> AppResult<()> {
        self.conn().execute(
            &format!("INSERT OR REPLACE INTO pipeline_runs({RUN_COLUMNS}) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"),
            params![run.id, run.project_id, run.jira_key, run.title, run.request, run.status.key(), run.mode.key(),
                run.workdir, run.branch, run.rounds, run.budget_minutes, run.note, run.created_at, run.updated_at,
                run.skipped.iter().map(|r| r.key()).collect::<Vec<_>>().join(","),
                serde_json::to_string(&run.agents).unwrap_or_else(|_| "{}".into()), run.image_count,
                run.jira_template, run.jira_url, serde_json::to_string(&run.efforts).unwrap_or_else(|_| "{}".into()), run.design],
        )?;
        Ok(())
    }

    fn stages(&self, run_id: &str) -> AppResult<Vec<StageRun>> {
        let c = self.conn();
        let mut st = c.prepare(&format!("SELECT {STAGE_COLUMNS} FROM pipeline_stages WHERE run_id=? ORDER BY started_at, rowid"))?;
        let rows = st.query_map([run_id], stage_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?.into_iter().flatten().collect())
    }

    fn save_stage(&self, stage: &StageRun) -> AppResult<()> {
        // REPLACE apagaria a linha e mudaria o rowid (a ordem): atualiza no lugar.
        self.conn().execute(
            &format!(
                "INSERT INTO pipeline_stages({STAGE_COLUMNS}) VALUES(?,?,?,?,?,?,?,?,?,?)
                 ON CONFLICT(id) DO UPDATE SET status=excluded.status, output=excluded.output, finished_at=excluded.finished_at"
            ),
            params![stage.id, stage.run_id, stage.role.key(), stage.round, stage.status.key(), stage.conversation_id,
                stage.input, stage.output, stage.started_at, stage.finished_at],
        )?;
        Ok(())
    }

    fn delete_run(&self, id: &str) -> AppResult<()> {
        let c = self.conn();
        c.execute("DELETE FROM pipeline_stages WHERE run_id=?", [id])?;
        // As imagens dos ajustes ficam em "<execução>/<ajuste>".
        c.execute("DELETE FROM pipeline_images WHERE run_id=? OR substr(run_id, 1, length(?) + 1) = ? || '/'", [id, id, id])?;
        c.execute("DELETE FROM pipeline_runs WHERE id=?", [id])?;
        Ok(())
    }

    fn save_images(&self, run_id: &str, images: &[Attachment]) -> AppResult<()> {
        let c = self.conn();
        for (position, image) in images.iter().enumerate() {
            c.execute(
                "INSERT OR REPLACE INTO pipeline_images(run_id,position,name,media_type,data) VALUES(?,?,?,?,?)",
                params![run_id, position as i64, image.name, image.media_type, image.data],
            )?;
        }
        Ok(())
    }

    fn set_stage_usage(&self, stage_id: &str, usage: &TokenUsage) -> AppResult<()> {
        self.conn().execute("UPDATE pipeline_stages SET usage=? WHERE id=?", params![usage_json(usage)?, stage_id])?;
        Ok(())
    }

    fn stage_usage(&self, run_id: &str) -> AppResult<std::collections::BTreeMap<String, TokenUsage>> {
        let c = self.conn();
        let mut st = c.prepare("SELECT id,usage FROM pipeline_stages WHERE run_id=? AND usage IS NOT NULL")?;
        let rows = st.query_map([run_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.map(|row| {
            let (id, text) = row?;
            Ok((id, parse_usage(&text)?))
        })
        .collect()
    }

    fn images(&self, run_id: &str) -> AppResult<Vec<Attachment>> {
        let c = self.conn();
        let mut st = c.prepare("SELECT name,media_type,data FROM pipeline_images WHERE run_id=? ORDER BY position")?;
        let rows = st.query_map([run_id], |r| {
            Ok(Attachment { name: r.get(0)?, kind: AttachmentKind::Image, media_type: r.get(1)?, data: r.get(2)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

impl SwitchLog for SqliteStore {
    fn record(&self, from_name: &str, to_name: &str, conversation_title: &str) -> AppResult<()> {
        self.conn().execute(
            "INSERT INTO switches(at,from_name,to_name,conversation_title) VALUES(?,?,?,?)",
            params![self.clock.now(), from_name, to_name, conversation_title],
        )?;
        Ok(())
    }

    fn recent(&self) -> AppResult<Vec<Switch>> {
        let c = self.conn();
        let mut st =
            c.prepare("SELECT at,from_name,to_name,conversation_title FROM switches ORDER BY id DESC LIMIT 20")?;
        let rows = st.query_map([], |r| {
            Ok(Switch { at: r.get(0)?, from_name: r.get(1)?, to_name: r.get(2)?, conversation_title: r.get(3)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

impl Settings for SqliteStore {
    fn flag(&self, key: &str, default: bool) -> bool {
        self.conn()
            .query_row("SELECT value FROM settings WHERE key=?", [key], |r| r.get::<_, String>(0))
            .optional()
            .ok()
            .flatten()
            .map(|v| v == "1")
            .unwrap_or(default)
    }

    fn flags_with_prefix(&self, prefix: &str) -> AppResult<Vec<(String, bool)>> {
        let c = self.conn();
        let mut st = c.prepare("SELECT key, value FROM settings WHERE key LIKE ? || '%'")?;
        let rows = st.query_map([prefix], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)? == "1")))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn set_flag(&self, key: &str, value: bool) -> AppResult<()> {
        self.conn().execute(
            "INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, if value { "1" } else { "0" }],
        )?;
        Ok(())
    }

    fn text(&self, key: &str) -> Option<String> {
        self.conn()
            .query_row("SELECT value FROM settings WHERE key=?", [key], |r| r.get::<_, String>(0))
            .optional()
            .ok()
            .flatten()
    }

    fn set_text(&self, key: &str, value: &str) -> AppResult<()> {
        self.conn().execute(
            "INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chat::{tool_message, KIND_CHAT};

    #[test]
    fn branch_keeps_only_subagents_of_copied_calls() {
        let store = SqliteStore::memory();
        ConversationRepository::insert(&store, "c1", "Conversa", "opus", "/tmp", KIND_CHAT, None).unwrap();
        store.add_message("c1", "tool", &tool_message("antes", "Agent", "{}", Some("ok"), false), None, None).unwrap();
        store.add_message("c1", "user", "corte", None, None).unwrap();
        store.add_message("c1", "tool", &tool_message("depois", "Agent", "{}", Some("ok"), false), None, None).unwrap();
        store.add_subagent_step("c1", "antes", r#"{"kind":"text","text":"a"}"#).unwrap();
        store.add_subagent_step("c1", "depois", r#"{"kind":"text","text":"b"}"#).unwrap();
        let cut = store.messages("c1").unwrap()[1].id;

        store.copy("c1", "c2", "Ramo", cut).unwrap();

        let parents: Vec<String> = store.subagent_steps("c2").unwrap().into_iter().map(|(p, _)| p).collect();
        assert_eq!(parents, ["antes"]);
    }
}
