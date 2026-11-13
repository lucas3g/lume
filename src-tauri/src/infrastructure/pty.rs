//! Terminais interativos com o shell do usuário, via pseudo-TTY.

use crate::application::{ports::TerminalPort, AppError, AppResult};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::Mutex,
};

struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

#[derive(Default)]
pub struct PtyTerminals {
    sessions: Mutex<HashMap<String, Session>>,
}

/// Shell de login do usuário: o do sistema, ou o configurado em `SHELL`.
fn shell_command(cwd: &str) -> CommandBuilder {
    let mut cmd = if cfg!(windows) {
        CommandBuilder::new("powershell.exe")
    } else {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let mut cmd = CommandBuilder::new(shell);
        cmd.arg("-l");
        cmd
    };
    cmd.cwd(cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("LANG", std::env::var("LANG").unwrap_or_else(|_| "pt_BR.UTF-8".into()));
    cmd
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }
}

impl TerminalPort for PtyTerminals {
    fn open(&self, id: &str, cwd: &str, cols: u16, rows: u16, on_output: Box<dyn Fn(&[u8]) + Send>) -> AppResult<()> {
        self.close(id);
        let pair = native_pty_system().openpty(size(cols, rows)).map_err(fail)?;
        let child = pair.slave.spawn_command(shell_command(cwd)).map_err(fail)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(fail)?;
        let writer = pair.master.take_writer().map_err(fail)?;
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            while let Ok(n) = reader.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                on_output(&buffer[..n]);
            }
        });
        let session = Session { master: pair.master, writer, child };
        self.sessions.lock().unwrap_or_else(|e| e.into_inner()).insert(id.to_owned(), session);
        Ok(())
    }

    fn write(&self, id: &str, data: &[u8]) -> AppResult<()> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        let session = sessions.get_mut(id).ok_or_else(|| AppError("Este terminal não está aberto.".into()))?;
        session.writer.write_all(data).map_err(|e| AppError(format!("O terminal não aceitou a entrada: {e}")))?;
        let _ = session.writer.flush();
        Ok(())
    }

    fn resize(&self, id: &str, cols: u16, rows: u16) -> AppResult<()> {
        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(session) = sessions.get(id) {
            session.master.resize(size(cols, rows)).map_err(|e| AppError(format!("Não foi possível redimensionar: {e}")))?;
        }
        Ok(())
    }

    fn close(&self, id: &str) {
        if let Some(mut session) = self.sessions.lock().unwrap_or_else(|e| e.into_inner()).remove(id) {
            let _ = session.child.kill();
        }
    }
}

fn fail(e: impl std::fmt::Display) -> AppError {
    AppError(format!("Não foi possível abrir o terminal: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::{mpsc, Arc, Mutex}, time::Duration};

    #[test]
    #[cfg(unix)]
    fn runs_a_shell_and_echoes_output() {
        let terminals = PtyTerminals::default();
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let collected = Arc::new(Mutex::new(Vec::new()));
        let sink = collected.clone();
        terminals
            .open("t1", "/tmp", 80, 24, Box::new(move |bytes| {
                sink.lock().unwrap().extend_from_slice(bytes);
                let _ = tx.send(bytes.to_vec());
            }))
            .unwrap();
        terminals.write("t1", b"echo lume-pty-ok\n").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            let _ = rx.recv_timeout(Duration::from_millis(200));
            if String::from_utf8_lossy(&collected.lock().unwrap()).contains("lume-pty-ok") {
                break;
            }
        }
        assert!(String::from_utf8_lossy(&collected.lock().unwrap()).contains("lume-pty-ok"));
        terminals.resize("t1", 100, 30).unwrap();
        terminals.close("t1");
        assert!(terminals.write("t1", b"x").is_err());
    }
}
