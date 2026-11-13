//! Adaptadores do Claude Code CLI oficial.

pub mod accounts;
pub mod connectors;
pub mod credentials;
pub mod engine;
pub mod plugins;
pub mod stream;
pub mod usage;

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::OnceLock,
};

use crate::application::{AppError, AppResult};
use std::time::{Duration, Instant};
use tokio::process::Command;

/// Variáveis herdadas que fariam o CLI se comportar como sessão aninhada.
const INHERITED_ENV: [&str; 4] = ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SSE_PORT", "CLAUDE_CONFIG_DIR"];

/// Comando do CLI apontando para o perfil de uma conta (`None` = login do computador).
fn command(cli: &Path, config_dir: Option<&str>) -> Command {
    let mut cmd = Command::new(cli);
    for key in INHERITED_ENV {
        cmd.env_remove(key);
    }
    if let Some(dir) = config_dir {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }
    if let Some(path) = shell_path() {
        cmd.env("PATH", path);
    }
    cmd.kill_on_drop(true);
    super::hide_console_async(&mut cmd);
    cmd
}

/// Versão bloqueante de `command`, para chamadas curtas feitas fora do runtime assíncrono.
fn blocking_command(cli: &Path, config_dir: Option<&str>) -> std::process::Command {
    let mut cmd = std::process::Command::new(cli);
    for key in INHERITED_ENV {
        cmd.env_remove(key);
    }
    if let Some(dir) = config_dir {
        cmd.env("CLAUDE_CONFIG_DIR", dir);
    }
    if let Some(path) = shell_path() {
        cmd.env("PATH", path);
    }
    super::hide_console(&mut cmd);
    cmd
}

/// Executa um comando do CLI e devolve a saída padrão; falha com a mensagem do CLI
/// ou com `slow` se estourar o tempo (o processo é encerrado).
pub(crate) fn run_cli(cli: &Path, config_dir: Option<&str>, args: &[&str], timeout: Duration, slow: &str) -> AppResult<String> {
    run_cli_with(cli, config_dir, args, timeout, slow, false)
}

/// Como `run_cli`, para logins: no Windows abre no navegador padrão a página de login que o CLI imprimir.
pub(crate) fn run_cli_login(cli: &Path, config_dir: Option<&str>, args: &[&str], timeout: Duration, slow: &str) -> AppResult<String> {
    run_cli_with(cli, config_dir, args, timeout, slow, true)
}

fn run_cli_with(cli: &Path, config_dir: Option<&str>, args: &[&str], timeout: Duration, slow: &str, open_page: bool) -> AppResult<String> {
    let mut child = blocking_command(cli, config_dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError(format!("Não foi possível executar o Claude Code: {e}")))?;
    let run = super::cli_output::BlockingRun::watch(&mut child, open_page);
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError(slow.into()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(AppError(format!("Falha ao aguardar o Claude Code: {e}"))),
        }
    };
    let output = run.finish(status);
    if status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let detail = super::cli_output::failure_detail(&output);
    Err(AppError(if detail.is_empty() { "O Claude Code recusou a operação.".into() } else { detail }))
}

/// PATH do shell de login do usuário. Um app aberto pelo Finder herda um PATH
/// mínimo, sem `npx`, `node` e afins, de que os conectores locais dependem.
#[cfg(unix)]
pub(crate) fn shell_path() -> Option<&'static str> {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    PATH.get_or_init(|| {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let output = std::process::Command::new(shell)
            .args(["-ilc", "printf '__PATH__%s__END__' \"$PATH\""])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        extract_path(&String::from_utf8_lossy(&output.stdout))
    })
    .as_deref()
}

#[cfg(not(unix))]
pub(crate) fn shell_path() -> Option<&'static str> {
    None
}

/// Isola o PATH entre os marcadores, ignorando o que o shell imprimir ao iniciar.
#[cfg_attr(not(unix), allow(dead_code))]
fn extract_path(output: &str) -> Option<String> {
    let start = output.find("__PATH__")? + "__PATH__".len();
    let end = output[start..].find("__END__")? + start;
    let path = output[start..end].trim();
    (!path.is_empty()).then(|| path.to_owned())
}

/// Procura o executável do Claude Code nos locais usuais e no PATH.
pub fn find_cli() -> Option<PathBuf> {
    let name = if cfg!(windows) { "claude.exe" } else { "claude" };
    let mut candidates = vec![];
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".local/bin").join(name));
        candidates.push(home.join(".claude/local").join(name));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/claude"));
    candidates.push(PathBuf::from("/usr/local/bin/claude"));
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|dir| dir.join(name)));
    }
    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_path_between_markers() {
        assert_eq!(extract_path("bem-vindo\n__PATH__/usr/bin:/opt/bin__END__\n"), Some("/usr/bin:/opt/bin".into()));
        assert_eq!(extract_path("__PATH____END__"), None);
        assert_eq!(extract_path("sem marcadores"), None);
    }
}
