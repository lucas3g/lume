//! Catálogo de skills (pasta pessoal do Claude Code + nomes vistos em uso) e
//! conectores já configurados no Claude Desktop.

use super::sqlite::SqliteStore;
use crate::application::{
    ports::{ConnectorSource, SkillRepository},
    AppResult,
};
use crate::domain::extension::{Skill, SkillRules, ORIGIN_PERSONAL, ORIGIN_PLUGIN};
use rusqlite::params;
use serde_json::Value;
use std::{collections::HashSet, fs, path::PathBuf, sync::Arc};

const SKILL_FILE: &str = "SKILL.md";

pub struct SqliteSkills {
    store: Arc<SqliteStore>,
    /// `~/.claude/skills`
    skills_dir: PathBuf,
}

impl SqliteSkills {
    pub fn new(store: Arc<SqliteStore>, skills_dir: PathBuf) -> Self {
        SqliteSkills { store, skills_dir }
    }

    fn personal(&self) -> Vec<Skill> {
        let Ok(entries) = fs::read_dir(&self.skills_dir) else { return vec![] };
        entries
            .flatten()
            .filter_map(|e| {
                let text = fs::read_to_string(e.path().join(SKILL_FILE)).ok()?;
                let folder = e.file_name().to_string_lossy().into_owned();
                let (name, description) = front_matter(&text);
                Some(Skill { name: name.unwrap_or(folder), description, origin: ORIGIN_PERSONAL.into() })
            })
            .collect()
    }

    fn remembered(&self) -> AppResult<Vec<String>> {
        let conn = self.store.conn();
        let mut st = conn.prepare("SELECT name FROM skill_catalog ORDER BY name")?;
        let rows = st.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

/// Lê `name` e `description` do cabeçalho de um SKILL.md.
fn front_matter(text: &str) -> (Option<String>, Option<String>) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }
    let field = |line: &str, key: &str| {
        line.strip_prefix(key)
            .map(|v| v.trim().trim_matches(['"', '\'']).to_owned())
            .filter(|v| !v.is_empty())
    };
    let (mut name, mut description) = (None, None);
    for line in lines.take_while(|l| l.trim() != "---") {
        name = name.or(field(line, "name:"));
        description = description.or(field(line, "description:"));
    }
    (name, description)
}

impl SkillRepository for SqliteSkills {
    fn catalog(&self) -> AppResult<Vec<Skill>> {
        let mut skills = self.personal();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        let known: HashSet<String> = skills.iter().map(|s| s.name.clone()).collect();
        let plugins = self
            .remembered()?
            .into_iter()
            .filter(|name| !known.contains(name))
            .map(|name| Skill { name, description: None, origin: ORIGIN_PLUGIN.into() });
        skills.extend(plugins);
        Ok(skills)
    }

    fn remember(&self, names: &[String]) -> AppResult<()> {
        let conn = self.store.conn();
        for name in names {
            conn.execute("INSERT OR IGNORE INTO skill_catalog(name) VALUES(?)", [name])?;
        }
        Ok(())
    }

    fn rules(&self, scope: &str) -> AppResult<SkillRules> {
        let conn = self.store.conn();
        let mut st = conn.prepare("SELECT skill, enabled FROM skill_rules WHERE scope=?")?;
        let rows = st.query_map([scope], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn set_rule(&self, scope: &str, skill: &str, enabled: bool) -> AppResult<()> {
        self.store.conn().execute(
            "INSERT INTO skill_rules(scope,skill,enabled) VALUES(?,?,?)
             ON CONFLICT(scope,skill) DO UPDATE SET enabled=excluded.enabled",
            params![scope, skill, enabled as i64],
        )?;
        Ok(())
    }
}

/// Lê os servidores MCP do `claude_desktop_config.json`.
pub struct DesktopConnectors {
    config_file: PathBuf,
}

impl DesktopConnectors {
    pub fn new(config_file: PathBuf) -> Self {
        DesktopConnectors { config_file }
    }
}

impl ConnectorSource for DesktopConnectors {
    fn servers(&self) -> Vec<(String, String)> {
        let Some(config) = fs::read(&self.config_file).ok().and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
        else {
            return vec![];
        };
        config["mcpServers"]
            .as_object()
            .map(|servers| servers.iter().map(|(name, server)| (name.clone(), server.to_string())).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_joins_personal_skills_with_remembered_names() {
        let dir = std::env::temp_dir().join(format!("relay-skills-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join("revisar")).unwrap();
        fs::create_dir_all(dir.join("sem-arquivo")).unwrap();
        fs::write(dir.join("revisar").join(SKILL_FILE), "---\nname: revisar\ndescription: \"Revisa o código.\"\n---\n# x").unwrap();

        let skills = SqliteSkills::new(Arc::new(SqliteStore::memory()), dir.clone());
        skills.remember(&["revisar".into(), "pdf".into(), "pdf".into()]).unwrap();
        let catalog = skills.catalog().unwrap();
        assert_eq!(catalog.len(), 2);
        assert_eq!((catalog[0].name.as_str(), catalog[0].description.as_deref(), catalog[0].origin.as_str()), ("revisar", Some("Revisa o código."), "personal"));
        assert_eq!((catalog[1].name.as_str(), catalog[1].origin.as_str()), ("pdf", "plugin"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_desktop_mcp_servers() {
        let file = std::env::temp_dir().join(format!("relay-desktop-{}.json", uuid::Uuid::new_v4()));
        fs::write(&file, r#"{"mcpServers":{"rea":{"command":"rea","args":["serve"]}},"preferences":{}}"#).unwrap();
        let servers = DesktopConnectors::new(file.clone()).servers();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].0, "rea");
        assert!(servers[0].1.contains("\"command\":\"rea\""));
        assert!(DesktopConnectors::new("/nao/existe.json".into()).servers().is_empty());
        fs::remove_file(file).unwrap();
    }
}
