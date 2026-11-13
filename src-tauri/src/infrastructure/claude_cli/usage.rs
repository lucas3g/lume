//! Uso atual de uma conta: um turno mínimo do CLI, só para receber o evento de limite
//! que o provedor manda no início de toda chamada.

use super::{command, stream::parse_line};
use crate::application::{ports::UsageProbe, AppError, AppResult};
use crate::domain::chat::{RateInfo, TurnEvent};
use async_trait::async_trait;
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, BufReader};

const TIMEOUT: Duration = Duration::from_secs(60);
/// O menor turno possível: modelo barato, sem ferramentas, sem raciocínio.
const PROBE_PROMPT: &str = "Responda apenas: ok";

pub struct ClaudeCliUsage {
    cli: PathBuf,
}

impl ClaudeCliUsage {
    pub fn new(cli: PathBuf) -> Self {
        ClaudeCliUsage { cli }
    }
}

#[async_trait]
impl UsageProbe for ClaudeCliUsage {
    async fn usage(&self, config_dir: Option<&str>) -> AppResult<RateInfo> {
        let mut child = command(&self.cli, config_dir)
            .args(["-p", "--output-format", "stream-json", "--verbose", "--model", "haiku", "--effort", "low", "--tools", "", "--max-turns", "1", PROBE_PROMPT])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| AppError(format!("Não foi possível consultar o uso: {e}")))?;
        let stdout = child.stdout.take().ok_or_else(|| AppError("Sem saída do Claude Code.".into()))?;
        let mut lines = BufReader::new(stdout).lines();
        let read = async {
            while let Ok(Some(line)) = lines.next_line().await {
                for event in parse_line(&line) {
                    if let TurnEvent::Rate(rate) = event {
                        return Some(rate);
                    }
                }
            }
            None
        };
        let found = tokio::time::timeout(TIMEOUT, read).await;
        let _ = child.kill().await;
        match found {
            Ok(Some(rate)) => Ok(rate),
            Ok(None) => Err(AppError("O provedor não informou o uso desta conta.".into())),
            Err(_) => Err(AppError("A consulta de uso demorou demais.".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::claude_cli::find_cli;

    #[tokio::test]
    #[ignore = "consome uso da conta real"]
    async fn real_cli_reports_usage_windows() {
        let probe = ClaudeCliUsage::new(find_cli().expect("Claude Code instalado"));
        let rate = probe.usage(None).await.unwrap();
        assert!(rate.util_5h.is_some() && rate.resets_5h.is_some());
    }
}
