//! Conectores que o próprio Claude Code carrega para uma conta: os da conta
//! claude.ai (web), os configurados no terminal e os de plugins.

use super::{run_cli, run_cli_login};
use crate::application::{ports::AccountConnectors, AppError, AppResult};
use crate::domain::extension::{parse_mcp_list, AccountConnector, ConnectorStatus};
use serde_json::{Map, Value};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const LIST_TIMEOUT: Duration = Duration::from_secs(90);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// Onde os conectores da conta claude.ai são vinculados e desvinculados.
const CLAUDE_AI_CONNECTORS_URL: &str = "https://claude.ai/customize/connectors";
/// Arquivo em que o CLI guarda, por 15 minutos, os conectores que pediram login; as
/// conversas pulam esses conectores mesmo depois que o usuário os vincula.
const NEEDS_AUTH_CACHE: &str = "mcp-needs-auth-cache.json";

pub struct ClaudeCliConnectors {
    cli: PathBuf,
}

impl ClaudeCliConnectors {
    pub fn new(cli: PathBuf) -> Self {
        ClaudeCliConnectors { cli }
    }
}

impl AccountConnectors for ClaudeCliConnectors {
    fn list(&self, config_dir: Option<&str>) -> AppResult<Vec<AccountConnector>> {
        let text = run_cli(&self.cli, config_dir, &["mcp", "list"], LIST_TIMEOUT, "A verificação dos conectores demorou demais.")?;
        let list = parse_mcp_list(&text);
        // `mcp list` testa cada conector de verdade; as conversas confiam no cache. Um conector
        // conectado que ficou no cache não chegaria ao assistente até o cache vencer.
        let connected: Vec<&str> = list.iter().filter(|c| c.status == ConnectorStatus::Connected).map(|c| c.name.as_str()).collect();
        if let Some(dir) = config_root(config_dir) {
            forget_needs_auth(&dir.join(NEEDS_AUTH_CACHE), &connected);
        }
        Ok(list)
    }

    /// Abre o login do conector no navegador e espera a conclusão.
    fn login(&self, config_dir: Option<&str>, name: &str) -> AppResult<()> {
        run_cli_login(&self.cli, config_dir, &["mcp", "login", name], LOGIN_TIMEOUT, "O login não foi concluído em 5 minutos.").map(|_| ())
    }

    /// Conectores da conta claude.ai só se desvinculam no próprio claude.ai (o CLI não
    /// mexe neles): abre a página de conectores no navegador.
    fn logout(&self, config_dir: Option<&str>, name: &str) -> AppResult<()> {
        if is_claude_ai(name) {
            return tauri_plugin_opener::open_url(CLAUDE_AI_CONNECTORS_URL, None::<&str>)
                .map_err(|e| AppError(format!("Não foi possível abrir {CLAUDE_AI_CONNECTORS_URL}: {e}")));
        }
        run_cli(&self.cli, config_dir, &["mcp", "logout", name], LIST_TIMEOUT, "A operação demorou demais.").map(|_| ())
    }
}

fn is_claude_ai(name: &str) -> bool {
    name.starts_with("claude.ai ")
}

/// Pasta de configuração do CLI para a conta (`None` = a do computador, `~/.claude`).
fn config_root(config_dir: Option<&str>) -> Option<PathBuf> {
    match config_dir {
        Some(dir) => Some(PathBuf::from(dir)),
        None => dirs::home_dir().map(|home| home.join(".claude")),
    }
}

/// Tira do cache de "precisa de login" os conectores que já estão conectados. O arquivo
/// é só um cache do CLI: qualquer falha ao lê-lo ou gravá-lo é ignorada.
fn forget_needs_auth(path: &Path, connected: &[&str]) {
    let Ok(text) = std::fs::read_to_string(path) else { return };
    let Ok(Value::Object(mut cache)) = serde_json::from_str::<Value>(&text) else { return };
    if !remove_keys(&mut cache, connected) {
        return;
    }
    let Ok(json) = serde_json::to_string(&Value::Object(cache)) else { return };
    // Grava ao lado e troca de uma vez: o CLI nunca lê um arquivo pela metade.
    let tmp = path.with_extension("json.lume-tmp");
    if std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Remove as chaves; diz se alguma saiu.
fn remove_keys(cache: &mut Map<String, Value>, keys: &[&str]) -> bool {
    keys.iter().fold(false, |changed, key| cache.remove(*key).is_some() || changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_connectors_leave_the_needs_auth_cache() {
        let dir = std::env::temp_dir().join(format!("lume-needs-auth-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(NEEDS_AUTH_CACHE);
        std::fs::write(&path, r#"{"claude.ai Atlassian":{"timestamp":1,"id":"a"},"claude.ai Notion":{"timestamp":2,"id":"n"}}"#).unwrap();
        forget_needs_auth(&path, &["claude.ai Atlassian", "claude.ai Gmail"]);
        let cache: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(cache.get("claude.ai Atlassian").is_none());
        assert_eq!(cache["claude.ai Notion"]["id"], "n", "quem ainda pede login continua no cache");
        // Sem nada a tirar, o arquivo não é regravado; sem arquivo, nada acontece.
        forget_needs_auth(&path, &["claude.ai Gmail"]);
        forget_needs_auth(&dir.join("nao-existe.json"), &["x"]);
        assert!(!dir.join("nao-existe.json").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_claude_ai_connectors_unlink_on_the_site() {
        assert!(is_claude_ai("claude.ai Atlassian"));
        assert!(!is_claude_ai("plugin:context7:context7"));
        assert!(!is_claude_ai("rea"));
    }
}
