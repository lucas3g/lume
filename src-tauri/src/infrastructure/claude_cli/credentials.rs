//! Credenciais OAuth que o Claude Code guarda para cada perfil: no macOS, um item do
//! Keychain ("Claude Code-credentials", com sufixo derivado do `CLAUDE_CONFIG_DIR`);
//! nos demais sistemas, o arquivo `.credentials.json` dentro do perfil.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const CREDENTIALS_FILE: &str = ".credentials.json";

/// Token de acesso de uma conta e quando vence (instante Unix, em segundos).
#[derive(Debug, Clone, PartialEq)]
pub struct OauthToken {
    pub access_token: String,
    pub expires_at: i64,
}

impl OauthToken {
    pub fn is_valid_at(&self, now: i64) -> bool {
        self.expires_at > now
    }
}

/// Nome do item do Keychain para um perfil: sem sufixo para o perfil padrão,
/// senão os 8 primeiros hexadecimais do SHA-256 do caminho do perfil.
pub fn keychain_service(config_dir: Option<&str>) -> String {
    match config_dir {
        None => KEYCHAIN_SERVICE.to_string(),
        Some(dir) => {
            let digest = Sha256::digest(dir.as_bytes());
            let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
            format!("{KEYCHAIN_SERVICE}-{}", &hex[..8])
        }
    }
}

/// Interpreta o JSON que o Claude Code grava (`{"claudeAiOauth":{"accessToken":..,"expiresAt":ms}}`).
pub fn parse_credentials(text: &str) -> Option<OauthToken> {
    let v: Value = serde_json::from_str(text).ok()?;
    let oauth = &v["claudeAiOauth"];
    Some(OauthToken {
        access_token: oauth["accessToken"].as_str().filter(|t| !t.is_empty())?.to_string(),
        expires_at: oauth["expiresAt"].as_i64().map(|ms| ms / 1000).unwrap_or(i64::MAX),
    })
}

/// Lê o token do perfil (`None` = perfil padrão em `default_dir`). Devolve `None`
/// quando não há credencial legível; o chamador decide a alternativa.
pub fn read_token(config_dir: Option<&str>, default_dir: &Path) -> Option<OauthToken> {
    read_from_keychain(config_dir)
        .or_else(|| read_from_file(&config_dir.map(PathBuf::from).unwrap_or_else(|| default_dir.to_path_buf())))
}

fn read_from_file(dir: &Path) -> Option<OauthToken> {
    let text = std::fs::read_to_string(dir.join(CREDENTIALS_FILE)).ok()?;
    parse_credentials(&text)
}

#[cfg(target_os = "macos")]
fn read_from_keychain(config_dir: Option<&str>) -> Option<OauthToken> {
    let output = std::process::Command::new("security")
        .args(["find-generic-password", "-s", &keychain_service(config_dir), "-w"])
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_credentials(String::from_utf8_lossy(&output.stdout).trim())
}

#[cfg(not(target_os = "macos"))]
fn read_from_keychain(_config_dir: Option<&str>) -> Option<OauthToken> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_has_no_suffix() {
        assert_eq!(keychain_service(None), "Claude Code-credentials");
    }

    #[test]
    fn custom_profile_gets_eight_hex_chars_of_sha256() {
        // sha256("/tmp/x") = 0f3b2e0c7a8f...; só os 8 primeiros entram no nome.
        let name = keychain_service(Some("/tmp/x"));
        assert!(name.starts_with("Claude Code-credentials-"));
        assert_eq!(name.len(), "Claude Code-credentials-".len() + 8);
        assert_ne!(name, keychain_service(Some("/tmp/y")));
    }

    #[test]
    fn parses_claude_code_credentials_json() {
        let token = parse_credentials(r#"{"claudeAiOauth":{"accessToken":"sk-ant-oat01-abc","refreshToken":"r","expiresAt":1794591219035}}"#).unwrap();
        assert_eq!(token, OauthToken { access_token: "sk-ant-oat01-abc".into(), expires_at: 1794591219 });
        assert!(token.is_valid_at(1794591218));
        assert!(!token.is_valid_at(1794591219));
    }

    #[test]
    fn rejects_missing_or_empty_token() {
        assert!(parse_credentials(r#"{"claudeAiOauth":{"accessToken":""}}"#).is_none());
        assert!(parse_credentials(r#"{"other":1}"#).is_none());
        assert!(parse_credentials("not json").is_none());
    }

    #[test]
    fn reads_credentials_file_from_profile() {
        let dir = std::env::temp_dir().join(format!("lume-cred-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(CREDENTIALS_FILE), r#"{"claudeAiOauth":{"accessToken":"tok","expiresAt":2000}}"#).unwrap();
        let token = read_from_file(&dir).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(token.access_token, "tok");
        assert_eq!(token.expires_at, 2);
    }
}
