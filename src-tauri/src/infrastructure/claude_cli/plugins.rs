//! Catálogo de plugins do Claude Code: marketplaces já configurados na máquina,
//! plugins instalados e instalação pelo CLI oficial.

use super::command;
use crate::application::{ports::PluginCatalog, AppError, AppResult};
use crate::domain::store::PluginListing;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

const MARKETPLACE_FILE: &str = ".claude-plugin/marketplace.json";
const INSTALLED_FILE: &str = "installed_plugins.json";
const SETTINGS_FILE: &str = "settings.json";
const INSTALL_TIMEOUT_SECS: u64 = 300;

/// Plugin instalado, conforme o registro do Claude Code.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledPlugin {
    /// `nome@marketplace`
    pub id: String,
    pub name: String,
    pub install_path: PathBuf,
}

/// Lê `installed_plugins.json` (`plugins`: id → lista de instalações).
pub fn installed_plugins(plugins_dir: &Path) -> Vec<InstalledPlugin> {
    let Some(v) = read_json(&plugins_dir.join(INSTALLED_FILE)) else { return vec![] };
    let Some(map) = v["plugins"].as_object() else { return vec![] };
    map.iter()
        .filter_map(|(id, installs)| {
            let path = installs.as_array()?.iter().find_map(|i| i["installPath"].as_str())?;
            let name = id.split('@').next().unwrap_or(id).to_owned();
            Some(InstalledPlugin { id: id.clone(), name, install_path: PathBuf::from(path) })
        })
        .collect()
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

pub struct ClaudeCliPlugins {
    cli: PathBuf,
    /// `~/.claude/plugins`
    plugins_dir: PathBuf,
    /// `~/.claude`
    config_dir: PathBuf,
}

impl ClaudeCliPlugins {
    pub fn new(cli: PathBuf, config_dir: PathBuf) -> Self {
        ClaudeCliPlugins { plugins_dir: config_dir.join("plugins"), cli, config_dir }
    }

    /// Plugins desativados pelo usuário (`enabledPlugins` nas configurações).
    fn disabled(&self) -> HashSet<String> {
        read_json(&self.config_dir.join(SETTINGS_FILE))
            .and_then(|s| s["enabledPlugins"].as_object().cloned())
            .map(|m| m.into_iter().filter(|(_, v)| v.as_bool() == Some(false)).map(|(k, _)| k).collect())
            .unwrap_or_default()
    }

    fn marketplace_files(&self) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(self.plugins_dir.join("marketplaces")) else { return vec![] };
        entries.flatten().map(|e| e.path().join(MARKETPLACE_FILE)).filter(|p| p.is_file()).collect()
    }

    fn run(&self, args: &[&str]) -> AppResult<String> {
        let output = command(&self.cli, None)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output();
        let output = tauri::async_runtime::block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(INSTALL_TIMEOUT_SECS), output).await
        })
        .map_err(|_| AppError("O Claude Code demorou demais para responder.".into()))?
        .map_err(|e| AppError(format!("Não foi possível executar o Claude Code: {e}")))?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if output.status.success() {
            return Ok(stdout);
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = cli_message(&stdout).unwrap_or_else(|| stderr.trim().to_owned());
        Err(AppError(if detail.is_empty() { "O Claude Code recusou a operação.".into() } else { detail }))
    }
}

/// Mensagem de erro da última linha JSON impressa pelo CLI com `--json`.
fn cli_message(stdout: &str) -> Option<String> {
    let line = stdout.lines().rev().find(|l| l.trim_start().starts_with('{'))?;
    let v: Value = serde_json::from_str(line).ok()?;
    v["error"].as_str().or(v["message"].as_str()).map(str::to_owned)
}

/// Converte um `marketplace.json` em anúncios, marcando os instalados.
pub fn parse_marketplace(json: &Value, installed: &HashMap<String, bool>) -> Vec<PluginListing> {
    let marketplace = json["name"].as_str().unwrap_or("marketplace").to_owned();
    let Some(plugins) = json["plugins"].as_array() else { return vec![] };
    plugins
        .iter()
        .filter_map(|p| {
            let name = p["name"].as_str()?.to_owned();
            let id = format!("{name}@{marketplace}");
            let enabled = installed.get(&id).copied();
            Some(PluginListing {
                display_name: p["displayName"].as_str().unwrap_or(&name).to_owned(),
                description: p["description"].as_str().unwrap_or_default().to_owned(),
                author: p["author"]["name"].as_str().map(Into::into),
                category: p["category"].as_str().map(Into::into),
                homepage: p["homepage"].as_str().map(Into::into),
                skill_count: p["skills"].as_array().map(Vec::len),
                installed: enabled.is_some(),
                enabled: enabled.unwrap_or(false),
                marketplace: marketplace.clone(),
                id,
                name,
            })
        })
        .collect()
}

impl PluginCatalog for ClaudeCliPlugins {
    fn listings(&self) -> AppResult<Vec<PluginListing>> {
        let disabled = self.disabled();
        let installed: HashMap<String, bool> =
            installed_plugins(&self.plugins_dir).into_iter().map(|p| (p.id.clone(), !disabled.contains(&p.id))).collect();
        let mut all: Vec<PluginListing> = self
            .marketplace_files()
            .iter()
            .filter_map(|path| read_json(path))
            .flat_map(|json| parse_marketplace(&json, &installed))
            .collect();
        // O mesmo plugin pode estar em mais de um marketplace: fica a primeira ocorrência.
        let mut seen = HashSet::new();
        all.retain(|p| seen.insert(p.id.clone()));
        Ok(all)
    }

    fn install(&self, id: &str) -> AppResult<()> {
        self.run(&["plugin", "install", id, "--scope", "user", "--json", "--yes"]).map(|_| ())
    }

    fn uninstall(&self, id: &str) -> AppResult<()> {
        self.run(&["plugin", "uninstall", id, "--json"]).map(|_| ())
    }

    fn refresh(&self) -> AppResult<()> {
        self.run(&["plugin", "marketplace", "update"]).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_marketplace_entries_and_installed_state() {
        let json: Value = serde_json::json!({
            "name": "oficial",
            "plugins": [
                { "name": "amd-skills", "description": "Skills da AMD", "author": { "name": "AMD" }, "category": "development",
                  "homepage": "https://amd.com", "skills": ["./a", "./b"] },
                { "name": "swift-lsp", "description": "LSP", "displayName": "Swift LSP" },
                { "description": "sem nome" }
            ]
        });
        let installed = HashMap::from([("swift-lsp@oficial".to_owned(), false)]);
        let list = parse_marketplace(&json, &installed);
        assert_eq!(list.len(), 2);
        let amd = &list[0];
        assert_eq!((amd.id.as_str(), amd.author.as_deref(), amd.category.as_deref(), amd.skill_count), ("amd-skills@oficial", Some("AMD"), Some("development"), Some(2)));
        assert!(!amd.installed);
        let swift = &list[1];
        assert_eq!((swift.display_name.as_str(), swift.installed, swift.enabled), ("Swift LSP", true, false));
    }

    #[test]
    fn reads_installed_plugins_file() {
        let dir = std::env::temp_dir().join(format!("lume-plugins-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(INSTALLED_FILE),
            r#"{"version":2,"plugins":{"a@m":[{"scope":"user","installPath":"/x/a/1.0"}],"b@m":[]}}"#,
        )
        .unwrap();
        let list = installed_plugins(&dir);
        assert_eq!(list, [InstalledPlugin { id: "a@m".into(), name: "a".into(), install_path: "/x/a/1.0".into() }]);
        assert!(installed_plugins(&dir.join("nao-existe")).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn extracts_cli_error_message() {
        assert_eq!(cli_message("aviso\n{\"ok\":false,\"error\":\"Plugin não encontrado\"}\n"), Some("Plugin não encontrado".into()));
        assert_eq!(cli_message("sem json"), None);
    }
}
