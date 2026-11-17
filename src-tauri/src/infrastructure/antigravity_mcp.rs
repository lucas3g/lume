//! Conectores do Lume no Antigravity (Gemini). O `agy` não aceita servidores por execução: lê
//! `~/.gemini/config/mcp_config.json` e as permissões de `~/.gemini/antigravity-cli/settings.json`.
//! O Lume mescla ali só o que é dele (servidores `lume-*` e as regras deles); o resto fica intacto.

use super::antigravity_cli::find_cli;
use crate::application::{ports::AntigravityMcp, AppError, AppResult};
use crate::domain::mcp::{antigravity_config, antigravity_settings, McpServer, McpTransport};
use std::path::{Path, PathBuf};

pub struct AntigravityFiles {
    /// Pasta `~/.gemini`.
    root: PathBuf,
}

impl AntigravityFiles {
    pub fn new(root: PathBuf) -> Self {
        AntigravityFiles { root }
    }

    pub fn from_home() -> Self {
        AntigravityFiles::new(dirs::home_dir().unwrap_or_default().join(".gemini"))
    }

    fn config_path(&self) -> PathBuf {
        self.root.join("config").join("mcp_config.json")
    }

    fn settings_path(&self) -> PathBuf {
        self.root.join("antigravity-cli").join("settings.json")
    }

    fn tokens_path(&self) -> PathBuf {
        self.root.join("antigravity").join("mcp_oauth_tokens.json")
    }
}

/// Grava ao lado e troca de uma vez: o `agy` nunca lê um arquivo pela metade. Não regrava o que não mudou.
fn write_atomic(path: &Path, text: &str) -> AppResult<()> {
    if std::fs::read_to_string(path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    let io = |e: std::io::Error| AppError(format!("Não foi possível gravar {}: {e}", path.display()));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    // Nome próprio por gravação: duas sincronizações ao mesmo tempo (a da abertura do app e a da
    // tela de Conectores) não escrevem no mesmo arquivo temporário.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("json.lume-tmp-{}-{seq}", std::process::id()));
    std::fs::write(&tmp, text).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io(e)
    })
}

fn unreadable(path: &Path) -> AppError {
    AppError(format!("{} não é um JSON válido; o Lume não mexeu nele. Corrija o arquivo para os conectores chegarem ao Gemini.", path.display()))
}

impl AntigravityMcp for AntigravityFiles {
    fn installed(&self) -> bool {
        find_cli().is_some()
    }

    fn sync(&self, servers: &[McpServer], allow: &[String]) -> AppResult<()> {
        // Sem o Antigravity neste computador não há o que configurar (e não se cria a pasta dele).
        if find_cli().is_none() && !self.root.is_dir() {
            return Ok(());
        }
        let config = self.config_path();
        let existing = std::fs::read_to_string(&config).ok();
        // Nada do Lume e nenhum arquivo: não cria um arquivo vazio à toa.
        if existing.is_none() && servers.is_empty() {
            return Ok(());
        }
        let merged = antigravity_config(existing.as_deref(), servers).ok_or_else(|| unreadable(&config))?;
        write_atomic(&config, &merged)?;

        let settings = self.settings_path();
        let existing = std::fs::read_to_string(&settings).ok();
        if existing.is_none() && allow.is_empty() {
            return Ok(());
        }
        let merged = antigravity_settings(existing.as_deref(), allow).ok_or_else(|| unreadable(&settings))?;
        write_atomic(&settings, &merged)
    }

    /// O Antigravity guarda os tokens de login dos servidores remotos num arquivo só; o servidor
    /// conta como vinculado quando o nome ou o endereço aparecem nele.
    fn linked(&self, server: &McpServer) -> Option<bool> {
        let McpTransport::Http { url, .. } = &server.transport else { return Some(true) };
        let text = match std::fs::read_to_string(self.tokens_path()) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(false),
            Err(_) => return None,
        };
        Some(text.contains(&format!("\"{}\"", server.name)) || text.contains(url.trim_end_matches('/')))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::mcp::Approval;
    use std::collections::BTreeMap;

    fn server(name: &str) -> McpServer {
        McpServer { name: name.into(), transport: McpTransport::Http { url: "https://mcp.atlassian.com/v1/mcp".into(), headers: BTreeMap::new() }, approval: Approval::ReadOnly }
    }

    #[test]
    fn merges_into_the_user_files_and_reads_the_login() {
        let root = std::env::temp_dir().join(format!("lume-agy-{}", std::process::id()));
        let files = AntigravityFiles::new(root.clone());
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(files.config_path(), r#"{"mcpServers":{"meu":{"command":"x"}}}"#).unwrap();
        files.sync(&[server("lume-atlassian")], &["mcp(lume-atlassian/getJiraIssue)".into()]).unwrap();
        let config = std::fs::read_to_string(files.config_path()).unwrap();
        assert!(config.contains("\"meu\"") && config.contains("lume-atlassian"));
        let settings = std::fs::read_to_string(files.settings_path()).unwrap();
        assert!(settings.contains("mcp(lume-atlassian/getJiraIssue)"));

        assert_eq!(files.linked(&server("lume-atlassian")), Some(false), "sem arquivo de tokens");
        std::fs::create_dir_all(root.join("antigravity")).unwrap();
        std::fs::write(files.tokens_path(), r#"{"lume-atlassian":{"access_token":"x"}}"#).unwrap();
        assert_eq!(files.linked(&server("lume-atlassian")), Some(true));

        std::fs::write(files.config_path(), "{quebrado").unwrap();
        assert!(files.sync(&[], &[]).is_err(), "arquivo ilegível não é sobrescrito");
        assert_eq!(std::fs::read_to_string(files.config_path()).unwrap(), "{quebrado");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
