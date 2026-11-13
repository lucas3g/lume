//! Armazenamento local em SQLite: implementa os repositórios da aplicação.

use crate::application::{
    ports::{
        AccountRepository, AgentRepository, Clock, ConnectorRepository, ConversationRepository, ProjectRepository,
        Settings, SwitchLog,
    },
    AppError, AppResult,
};
use crate::domain::{
    account::{Account, Identity},
    chat::{Conversation, Message, Switch, DEFAULT_PERMISSION_MODE},
    extension::{connector_key, Agent, Connector},
    project::{Project, ProjectFile},
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
  limited_until INTEGER, util_5h REAL, util_7d REAL, resets_5h INTEGER, login_expires_at INTEGER);
CREATE TABLE IF NOT EXISTS conversations(
  id TEXT PRIMARY KEY, title TEXT NOT NULL, cli_session_id TEXT, model TEXT NOT NULL,
  cwd TEXT NOT NULL, agent_id TEXT, kind TEXT NOT NULL DEFAULT 'chat', allowed_tools TEXT NOT NULL DEFAULT '',
  project_id TEXT, context_percent INTEGER,
  effort TEXT NOT NULL DEFAULT 'medium', permission_mode TEXT NOT NULL DEFAULT 'manual',
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS messages(
  id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT NOT NULL, role TEXT NOT NULL,
  content TEXT NOT NULL, account_id TEXT, meta TEXT, created_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conversation_id, id);
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
  id TEXT PRIMARY KEY, name TEXT NOT NULL, instructions TEXT NOT NULL, folder TEXT);
CREATE TABLE IF NOT EXISTS project_files(
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL, name TEXT NOT NULL, content TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS hist_sessions(
  id TEXT PRIMARY KEY, source TEXT NOT NULL, cwd TEXT, title TEXT, path TEXT NOT NULL,
  mtime INTEGER NOT NULL, msg_count INTEGER NOT NULL);
CREATE VIRTUAL TABLE IF NOT EXISTS hist_fts USING fts5(
  session_id UNINDEXED, role UNINDEXED, body, tokenize='unicode61 remove_diacritics 2');
";

const ACCOUNT_COLUMNS: &str =
    "id,name,email,plan,config_dir,priority,enabled,auth_ok,limited_until,util_5h,util_7d,resets_5h,login_expires_at";
const CONVERSATION_COLUMNS: &str = "id,title,cli_session_id,model,cwd,agent_id,kind,allowed_tools,project_id,context_percent,updated_at,effort,permission_mode";

/// Colunas acrescentadas depois da primeira versão; falham em silêncio se já existem.
const MIGRATIONS: [&str; 9] = [
    "ALTER TABLE accounts ADD COLUMN login_expires_at INTEGER",
    "ALTER TABLE projects ADD COLUMN folder TEXT",
    "ALTER TABLE conversations ADD COLUMN effort TEXT NOT NULL DEFAULT 'medium'",
    // Conversas antigas foram criadas no modo que sempre pergunta; continuam assim.
    "ALTER TABLE conversations ADD COLUMN permission_mode TEXT NOT NULL DEFAULT 'manual'",
    "ALTER TABLE conversations ADD COLUMN context_percent INTEGER",
    "ALTER TABLE conversations ADD COLUMN project_id TEXT",
    "ALTER TABLE conversations ADD COLUMN agent_id TEXT",
    "ALTER TABLE conversations ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat'",
    "ALTER TABLE conversations ADD COLUMN allowed_tools TEXT NOT NULL DEFAULT ''",
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

    fn insert(&self, id: &str, name: &str, identity: &Identity, config_dir: Option<&str>) -> AppResult<()> {
        let c = self.conn();
        let next: i64 = c.query_row("SELECT COALESCE(MAX(priority),0)+1 FROM accounts", [], |r| r.get(0))?;
        c.execute(
            "INSERT INTO accounts(id,name,email,plan,config_dir,priority,login_expires_at) VALUES(?,?,?,?,?,?,?)",
            params![id, name, identity.email, identity.plan, config_dir, next, identity.login_expires_at],
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

    fn set_limited(&self, id: &str, until: Option<i64>) -> AppResult<()> {
        self.conn()
            .execute("UPDATE accounts SET limited_until=? WHERE id=?", params![until, id])?;
        Ok(())
    }

    fn set_usage(&self, id: &str, util_5h: Option<f64>, util_7d: Option<f64>, resets_5h: Option<i64>) -> AppResult<()> {
        self.conn().execute(
            "UPDATE accounts SET util_5h=COALESCE(?,util_5h), util_7d=COALESCE(?,util_7d),
             resets_5h=COALESCE(?,resets_5h) WHERE id=?",
            params![util_5h, util_7d, resets_5h, id],
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
        let mut st = c.prepare("SELECT id,name,instructions,folder FROM projects ORDER BY name")?;
        let rows = st.query_map([], project_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn get(&self, id: &str) -> AppResult<Option<Project>> {
        Ok(self
            .conn()
            .query_row("SELECT id,name,instructions,folder FROM projects WHERE id=?", [id], project_row)
            .optional()?)
    }

    fn save(&self, project: &Project) -> AppResult<()> {
        self.conn().execute(
            "INSERT INTO projects(id,name,instructions,folder) VALUES(?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, instructions=excluded.instructions, folder=excluded.folder",
            params![project.id, project.name, project.instructions, project.folder],
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
    Ok(Project { id: r.get(0)?, name: r.get(1)?, instructions: r.get(2)?, folder: r.get(3)? })
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
}
