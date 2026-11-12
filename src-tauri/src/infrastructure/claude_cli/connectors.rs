//! Conectores que o próprio Claude Code carrega para uma conta: os da conta
//! claude.ai (web), os configurados no terminal e os de plugins.

use super::command;
use crate::application::{ports::AccountConnectors, AppError, AppResult};
use crate::domain::extension::{parse_mcp_list, AccountConnector};
use std::{path::PathBuf, process::Stdio, time::Duration};

const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

pub struct ClaudeCliConnectors {
    cli: PathBuf,
}

impl ClaudeCliConnectors {
    pub fn new(cli: PathBuf) -> Self {
        ClaudeCliConnectors { cli }
    }

    fn run(&self, config_dir: Option<&str>, args: &[&str], timeout: Duration, slow: &str) -> AppResult<String> {
        let output = command(&self.cli, config_dir).args(args).stdin(Stdio::null()).output();
        let output = tauri::async_runtime::block_on(async { tokio::time::timeout(timeout, output).await })
            .map_err(|_| AppError(slow.into()))?
            .map_err(|e| AppError(format!("Não foi possível executar o Claude Code: {e}")))?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if output.status.success() {
            return Ok(stdout);
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if stderr.trim().is_empty() { stdout.trim().to_owned() } else { stderr.trim().to_owned() };
        Err(AppError(if detail.is_empty() { "O Claude Code recusou a operação.".into() } else { detail }))
    }
}

impl AccountConnectors for ClaudeCliConnectors {
    fn list(&self, config_dir: Option<&str>) -> AppResult<Vec<AccountConnector>> {
        let text = self.run(config_dir, &["mcp", "list"], LIST_TIMEOUT, "A verificação dos conectores demorou demais.")?;
        Ok(parse_mcp_list(&text))
    }

    /// Abre o login do conector no navegador e espera a conclusão.
    fn login(&self, config_dir: Option<&str>, name: &str) -> AppResult<()> {
        self.run(config_dir, &["mcp", "login", name], LOGIN_TIMEOUT, "O login não foi concluído em 5 minutos.").map(|_| ())
    }

    fn logout(&self, config_dir: Option<&str>, name: &str) -> AppResult<()> {
        self.run(config_dir, &["mcp", "logout", name], LIST_TIMEOUT, "A operação demorou demais.").map(|_| ())
    }
}
