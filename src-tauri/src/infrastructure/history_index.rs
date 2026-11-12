//! Índice de busca (SQLite FTS5) das conversas do Claude Code e do Claude Desktop
//! guardadas neste computador. Os arquivos originais são apenas lidos.

use super::{sqlite::SqliteStore, transcript::parse_transcript};
use crate::application::ports::HistoryIndex;
use crate::domain::history::{HistSession, HistoryStatus, ImportedSession, Snippet};
use rusqlite::{params, Row};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    io::BufReader,
    path::{Path, PathBuf},
    sync::Arc,
    time::UNIX_EPOCH,
};

const SOURCE_CODE: &str = "code";
const SOURCE_DESKTOP: &str = "desktop";
const MIN_WORD_CHARS: usize = 4;
const MAX_QUERY_WORDS: usize = 10;
const SEARCH_CANDIDATES: i64 = 40;
const PROGRESS_EVERY: usize = 20;
const DESKTOP_SCAN_DEPTH: u8 = 3;
const SESSION_COLUMNS: &str = "s.id,s.source,s.cwd,s.title,s.mtime,s.msg_count";

pub struct SqliteHistoryIndex {
    store: Arc<SqliteStore>,
    /// `~/.claude/projects`
    transcripts_dir: PathBuf,
    /// Metadados das sessões de código do Claude Desktop.
    desktop_dir: PathBuf,
}

impl SqliteHistoryIndex {
    pub fn new(store: Arc<SqliteStore>, transcripts_dir: PathBuf, desktop_dir: PathBuf) -> Self {
        SqliteHistoryIndex { store, transcripts_dir, desktop_dir }
    }

    fn is_current(&self, id: &str, mtime: i64) -> bool {
        self.store
            .conn()
            .query_row("SELECT mtime FROM hist_sessions WHERE id=?", [id], |r| r.get::<_, i64>(0))
            .is_ok_and(|known| known == mtime)
    }

    fn store_session(&self, id: &str, path: &Path, mtime: i64, session: &ImportedSession) -> rusqlite::Result<()> {
        let mut conn = self.store.conn();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM hist_fts WHERE session_id=?", [id])?;
        for turn in &session.turns {
            tx.execute(
                "INSERT INTO hist_fts(session_id,role,body) VALUES(?,?,?)",
                params![id, turn.role, turn.text],
            )?;
        }
        tx.execute(
            "INSERT INTO hist_sessions(id,source,cwd,title,path,mtime,msg_count) VALUES(?,?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET cwd=excluded.cwd, title=excluded.title, path=excluded.path,
               mtime=excluded.mtime, msg_count=excluded.msg_count",
            params![id, SOURCE_CODE, session.cwd, session.title, path.to_string_lossy(), mtime, session.turns.len() as i64],
        )?;
        tx.commit()
    }

    fn apply_desktop_titles(&self) {
        let conn = self.store.conn();
        for (id, title) in desktop_titles(&self.desktop_dir) {
            let _ = conn.execute(
                "UPDATE hist_sessions SET source=?, title=? WHERE id=?",
                params![SOURCE_DESKTOP, title, id],
            );
        }
    }
}

impl HistoryIndex for SqliteHistoryIndex {
    fn reindex(&self, progress: &mut dyn FnMut(usize, usize)) -> usize {
        let files = transcript_files(&self.transcripts_dir);
        let total = files.len();
        let mut changed = 0;
        for (i, path) in files.iter().enumerate() {
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            let mtime = modified_at(path);
            if self.is_current(id, mtime) {
                continue;
            }
            let Ok(file) = fs::File::open(path) else { continue };
            let session = parse_transcript(BufReader::new(file));
            if self.store_session(id, path, mtime, &session).is_ok() {
                changed += 1;
            }
            if i % PROGRESS_EVERY == 0 {
                progress(i + 1, total);
            }
        }
        self.apply_desktop_titles();
        progress(total, total);
        changed
    }

    fn status(&self) -> HistoryStatus {
        let conn = self.store.conn();
        let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0);
        HistoryStatus {
            sessions: count("SELECT COUNT(*) FROM hist_sessions WHERE msg_count>0"),
            projects: count("SELECT COUNT(DISTINCT cwd) FROM hist_sessions WHERE msg_count>0"),
            desktop_sessions: count("SELECT COUNT(*) FROM hist_sessions WHERE source='desktop' AND msg_count>0"),
        }
    }

    /// Trechos mais relevantes, no máximo um por conversa.
    fn search(&self, text: &str, limit: usize, exclude_session: Option<&str>) -> Vec<Snippet> {
        let Some(query) = fts_query(text) else { return vec![] };
        let conn = self.store.conn();
        let Ok(mut st) = conn.prepare(
            "SELECT f.session_id, COALESCE(s.title,'Conversa sem título'), snippet(hist_fts, 2, '', '', ' … ', 48)
             FROM hist_fts f JOIN hist_sessions s ON s.id = f.session_id
             WHERE hist_fts MATCH ?1 ORDER BY rank LIMIT ?2",
        ) else {
            return vec![];
        };
        let Ok(rows) = st.query_map(params![query, SEARCH_CANDIDATES], |r| {
            Ok(Snippet { session_id: r.get(0)?, title: r.get(1)?, text: r.get(2)? })
        }) else {
            return vec![];
        };
        let mut seen = HashSet::new();
        rows.flatten()
            .filter(|s| Some(s.session_id.as_str()) != exclude_session && seen.insert(s.session_id.clone()))
            .take(limit)
            .collect()
    }

    fn list(&self, query: &str, limit: i64) -> Vec<HistSession> {
        let conn = self.store.conn();
        let result = match fts_query(query) {
            Some(q) => conn
                .prepare(&format!(
                    "SELECT {SESSION_COLUMNS} FROM hist_sessions s WHERE s.id IN
                     (SELECT session_id FROM hist_fts WHERE hist_fts MATCH ?1) ORDER BY s.mtime DESC LIMIT ?2"
                ))
                .and_then(|mut st| st.query_map(params![q, limit], session_row)?.collect::<Result<Vec<_>, _>>()),
            None => conn
                .prepare(&format!(
                    "SELECT {SESSION_COLUMNS} FROM hist_sessions s WHERE s.msg_count>0 ORDER BY s.mtime DESC LIMIT ?1"
                ))
                .and_then(|mut st| st.query_map([limit], session_row)?.collect::<Result<Vec<_>, _>>()),
        };
        result.unwrap_or_default()
    }

    fn load(&self, session_id: &str) -> Option<ImportedSession> {
        let (path, title): (String, Option<String>) = self
            .store
            .conn()
            .query_row("SELECT path,title FROM hist_sessions WHERE id=?", [session_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .ok()?;
        let mut session = parse_transcript(BufReader::new(fs::File::open(path).ok()?));
        session.title = title.or(session.title);
        Some(session)
    }
}

fn session_row(r: &Row<'_>) -> rusqlite::Result<HistSession> {
    Ok(HistSession {
        id: r.get(0)?,
        source: r.get(1)?,
        cwd: r.get(2)?,
        title: r.get(3)?,
        mtime: r.get(4)?,
        msg_count: r.get(5)?,
    })
}

fn modified_at(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Transcrições de primeiro nível de cada projeto (subagentes ficam em subpastas).
fn transcript_files(root: &Path) -> Vec<PathBuf> {
    let Ok(projects) = fs::read_dir(root) else { return vec![] };
    projects
        .flatten()
        .filter_map(|project| fs::read_dir(project.path()).ok())
        .flat_map(|entries| entries.flatten().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect()
}

/// Títulos que o Claude Desktop deu às sessões de código (chave: id da sessão do CLI).
fn desktop_titles(root: &Path) -> Vec<(String, String)> {
    fn walk(dir: &Path, depth: u8, out: &mut Vec<(String, String)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for path in entries.flatten().map(|e| e.path()) {
            if path.is_dir() {
                if depth < DESKTOP_SCAN_DEPTH {
                    walk(&path, depth + 1, out);
                }
                continue;
            }
            if path.extension().is_none_or(|x| x != "json") {
                continue;
            }
            let Some(v) = fs::read(&path).ok().and_then(|raw| serde_json::from_slice::<Value>(&raw).ok()) else {
                continue;
            };
            if let (Some(id), Some(title)) = (v["cliSessionId"].as_str(), v["title"].as_str()) {
                out.push((id.to_owned(), title.to_owned()));
            }
        }
    }
    let mut out = vec![];
    walk(root, 0, &mut out);
    out
}

/// Monta uma consulta FTS a partir de texto livre: palavras relevantes unidas por OR.
fn fts_query(text: &str) -> Option<String> {
    let mut seen = HashSet::new();
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.chars().count() >= MIN_WORD_CHARS)
        .map(str::to_lowercase)
        .filter(|w| seen.insert(w.clone()))
        .take(MAX_QUERY_WORDS)
        .map(|w| format!("\"{w}\""))
        .collect();
    (!words.is_empty()).then(|| words.join(" OR "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::transcript::SAMPLE;

    #[test]
    fn indexes_finds_and_loads_sessions() {
        let dir = std::env::temp_dir().join(format!("relay-test-{}", uuid::Uuid::new_v4()));
        let project = dir.join("projects/-tmp-loja");
        let desktop = dir.join("desktop/a/b");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&desktop).unwrap();
        fs::write(project.join("abc.jsonl"), SAMPLE).unwrap();
        fs::write(desktop.join("local_1.json"), r#"{"cliSessionId":"abc","title":"Título do Desktop"}"#).unwrap();

        let index = SqliteHistoryIndex::new(Arc::new(SqliteStore::memory()), dir.join("projects"), dir.join("desktop"));
        assert_eq!(index.reindex(&mut |_, _| {}), 1);
        assert_eq!(index.reindex(&mut |_, _| {}), 0, "sem alteração, não reindexa");

        let hits = index.search("qual índice usar na tabela de pedidos", 3, None);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Título do Desktop");
        assert!(index.search("pedidos", 3, Some("abc")).is_empty());

        let status = index.status();
        assert_eq!((status.sessions, status.desktop_sessions), (1, 1));
        assert_eq!(index.list("", 10).len(), 1);
        assert_eq!(index.list("inexistente", 10).len(), 0);

        let loaded = index.load("abc").unwrap();
        assert_eq!(loaded.turns.len(), 2);
        assert_eq!(loaded.title.as_deref(), Some("Título do Desktop"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn fts_query_ignores_short_words_and_symbols() {
        assert_eq!(fts_query("o que é isso?"), Some("\"isso\"".into()));
        assert_eq!(fts_query("a b c"), None);
        assert_eq!(fts_query("teste \"aspas\" teste"), Some("\"teste\" OR \"aspas\"".into()));
    }
}
