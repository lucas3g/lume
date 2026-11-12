//! Conectores que o próprio Claude Code carrega para uma conta: os da conta
//! claude.ai (web), os configurados no terminal e os de plugins.

use super::run_cli;
use crate::application::{ports::AccountConnectors, AppResult};
use crate::domain::extension::{parse_mcp_list, AccountConnector};
use std::{path::PathBuf, time::Duration};

const LIST_TIMEOUT: Duration = Duration::from_secs(90);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

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
        Ok(parse_mcp_list(&text))
    }

    /// Abre o login do conector no navegador e espera a conclusão.
    fn login(&self, config_dir: Option<&str>, name: &str) -> AppResult<()> {
        run_cli(&self.cli, config_dir, &["mcp", "login", name], LOGIN_TIMEOUT, "O login não foi concluído em 5 minutos.").map(|_| ())
    }

    fn logout(&self, config_dir: Option<&str>, name: &str) -> AppResult<()> {
        run_cli(&self.cli, config_dir, &["mcp", "logout", name], LIST_TIMEOUT, "A operação demorou demais.").map(|_| ())
    }
}
