//! Login, identidade e perfis isolados das contas, via Claude Code CLI.

use super::command;
use crate::application::{ports::AccountGateway, AppError, AppResult};
use crate::domain::account::Identity;
use async_trait::async_trait;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
/// Pastas compartilhadas com o perfil principal: qualquer conta retoma qualquer
/// conversa e enxerga as mesmas skills e agentes. Só as credenciais ficam separadas.
const SHARED_DIRS: [&str; 3] = ["projects", "skills", "agents"];

pub struct ClaudeCliAccounts {
    cli: PathBuf,
    profiles_dir: PathBuf,
    /// Diretório de configuração principal do Claude Code (`~/.claude`).
    main_config: PathBuf,
}

impl ClaudeCliAccounts {
    pub fn new(cli: PathBuf, profiles_dir: PathBuf, main_config: PathBuf) -> Self {
        ClaudeCliAccounts { cli, profiles_dir, main_config }
    }
}

fn link_dir(target: &Path, link: &Path) {
    if link.exists() || !target.exists() {
        return;
    }
    #[cfg(unix)]
    let _ = std::os::unix::fs::symlink(target, link);
    #[cfg(windows)]
    let _ = std::os::windows::fs::symlink_dir(target, link);
}

#[async_trait]
impl AccountGateway for ClaudeCliAccounts {
    fn prepare_profile(&self, account_id: &str) -> AppResult<String> {
        let dir = self.profiles_dir.join(account_id);
        let io = |e: std::io::Error| AppError(format!("Não foi possível criar o perfil da conta: {e}"));
        std::fs::create_dir_all(&dir).map_err(io)?;
        std::fs::create_dir_all(self.main_config.join("projects")).map_err(io)?;
        for shared in SHARED_DIRS {
            link_dir(&self.main_config.join(shared), &dir.join(shared));
        }
        Ok(dir.to_string_lossy().into_owned())
    }

    fn discard_profile(&self, config_dir: &str) {
        let dir = Path::new(config_dir);
        // Só remove perfis criados pelo app; os links são apagados sem seguir o destino.
        if dir.starts_with(&self.profiles_dir) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    async fn identity(&self, config_dir: Option<&str>) -> Identity {
        let output = command(&self.cli, config_dir).args(["auth", "status"]).output().await;
        let v: Value = output
            .ok()
            .and_then(|o| serde_json::from_slice(&o.stdout).ok())
            .unwrap_or(Value::Null);
        Identity {
            logged_in: v["loggedIn"].as_bool().unwrap_or(false),
            email: v["email"].as_str().map(Into::into),
            plan: v["subscriptionType"].as_str().map(Into::into),
        }
    }

    /// Abre o login oficial (no navegador) para o perfil indicado.
    async fn login(&self, config_dir: &str) -> AppResult<Identity> {
        let run = command(&self.cli, Some(config_dir))
            .args(["auth", "login", "--claudeai"])
            .stdin(Stdio::null())
            .output();
        let output = tokio::time::timeout(LOGIN_TIMEOUT, run)
            .await
            .map_err(|_| AppError("O login não foi concluído em 5 minutos.".into()))?
            .map_err(|e| AppError(format!("Não foi possível abrir o login: {e}")))?;
        let identity = self.identity(Some(config_dir)).await;
        if identity.logged_in {
            return Ok(identity);
        }
        let detail = String::from_utf8_lossy(&output.stderr);
        Err(AppError(format!("O login não foi concluído. {}", detail.trim())))
    }
}
