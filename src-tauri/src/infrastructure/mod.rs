//! Adaptadores: implementações concretas das portas da aplicação.

pub mod antigravity_cli;
pub mod claude_cli;
pub mod cli_output;
pub mod clipboard;
pub mod codex_cli;
pub mod providers;
pub mod files;
pub mod git_cli;
pub mod history_index;
pub mod icons;
pub mod knowledge_fs;
pub mod mcp_registry;
pub mod memory_fs;
pub mod model_catalog;
pub mod pty;
pub mod skills;
pub mod sqlite;
pub mod transcript;
pub mod workspace_fs;

/// No Windows, um processo de console (claude, codex, git) iniciado por um app de
/// janela abre um terminal próprio; esta flag mantém o processo sem janela.
#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Esconde a janela de console do comando no Windows; nos demais sistemas não faz nada.
pub(crate) fn hide_console(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Versão de `hide_console` para comandos assíncronos do tokio.
pub(crate) fn hide_console_async(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

use crate::application::ports::{Clock, IdGenerator};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

pub struct UuidGenerator;

impl IdGenerator for UuidGenerator {
    fn new_id(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }
}
