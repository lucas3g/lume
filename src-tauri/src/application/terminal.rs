//! Terminal integrado: um por conversa, aberto na pasta da sessão.

use super::{
    ports::{ConversationRepository, EventSink, TerminalPort},
    AppError, AppResult,
};
use std::sync::Arc;

pub struct ManageTerminals {
    pub conversations: Arc<dyn ConversationRepository>,
    pub terminals: Arc<dyn TerminalPort>,
    pub events: Arc<dyn EventSink>,
}

impl ManageTerminals {
    /// Abre (ou reabre) o terminal da conversa, na pasta dela; devolve o id do terminal.
    pub fn open(&self, conversation_id: &str, cols: u16, rows: u16) -> AppResult<String> {
        let conversation = self
            .conversations
            .get(conversation_id)?
            .ok_or_else(|| AppError("Conversa não encontrada.".into()))?;
        if !conversation.is_agent() {
            return Err(AppError("O terminal existe só em sessões com pasta.".into()));
        }
        let events = self.events.clone();
        let id = conversation_id.to_owned();
        let sink = id.clone();
        self.terminals.open(&id, &conversation.cwd, cols, rows, Box::new(move |bytes| events.terminal_output(&sink, bytes)))?;
        Ok(id)
    }

    pub fn write(&self, id: &str, data: &[u8]) -> AppResult<()> {
        self.terminals.write(id, data)
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> AppResult<()> {
        self.terminals.resize(id, cols, rows)
    }

    pub fn close(&self, id: &str) {
        self.terminals.close(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{fakes::*, ports::TerminalOutput};
    use crate::domain::chat::KIND_AGENT;
    use std::sync::Mutex;

    /// Terminal que só anota o que lhe pediram.
    #[derive(Default)]
    struct FakeTerminal(Mutex<Vec<String>>);

    impl TerminalPort for FakeTerminal {
        fn open(&self, id: &str, cwd: &str, cols: u16, rows: u16, on_output: TerminalOutput) -> AppResult<()> {
            self.0.lock().unwrap().push(format!("open {id} {cwd} {cols}x{rows}"));
            on_output(b"$ ");
            Ok(())
        }
        fn write(&self, id: &str, data: &[u8]) -> AppResult<()> {
            self.0.lock().unwrap().push(format!("write {id} {}", String::from_utf8_lossy(data)));
            Ok(())
        }
        fn resize(&self, id: &str, cols: u16, rows: u16) -> AppResult<()> {
            self.0.lock().unwrap().push(format!("resize {id} {cols}x{rows}"));
            Ok(())
        }
        fn close(&self, id: &str) {
            self.0.lock().unwrap().push(format!("close {id}"));
        }
    }

    #[test]
    fn opens_only_for_sessions_with_a_folder() {
        let world = World::with_accounts(vec![]);
        ConversationRepository::insert(&*world.store, "s1", "Sessão", "opus", "/tmp/loja", KIND_AGENT, None).unwrap();
        let terminal = Arc::new(FakeTerminal::default());
        let uc = ManageTerminals { conversations: world.store.clone(), terminals: terminal.clone(), events: world.events.clone() };
        assert!(uc.open("c1", 80, 24).is_err(), "conversa comum não tem pasta");
        assert_eq!(uc.open("s1", 80, 24).unwrap(), "s1");
        uc.write("s1", b"ls").unwrap();
        uc.resize("s1", 120, 40).unwrap();
        uc.close("s1");
        assert_eq!(*terminal.0.lock().unwrap(), ["open s1 /tmp/loja 80x24", "write s1 ls", "resize s1 120x40", "close s1"]);
    }
}
