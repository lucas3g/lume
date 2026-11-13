//! Adaptadores: implementações concretas das portas da aplicação.

pub mod antigravity_cli;
pub mod claude_cli;
pub mod clipboard;
pub mod codex_cli;
pub mod providers;
pub mod files;
pub mod git_cli;
pub mod history_index;
pub mod icons;
pub mod mcp_registry;
pub mod memory_fs;
pub mod pty;
pub mod skills;
pub mod sqlite;
pub mod transcript;
pub mod workspace_fs;

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
