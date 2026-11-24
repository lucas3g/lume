//! Turnos parados porque todas as contas atingiram o limite: o app os continua sozinho
//! quando uma conta volta (ver `flags::RESUME_ON_RESET`).

use super::{ports::Settings, AppResult};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

const KEY: &str = "parked_turns";

/// Um turno à espera de conta.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Parked {
    /// Quando a primeira conta deve voltar, se o provedor informou.
    pub resets_at: Option<i64>,
    /// Uma conta já tinha trabalhado no pedido: a próxima continua a sessão em vez de refazê-lo.
    pub continuing: bool,
    /// Turno de uma etapa da esteira: quem o retoma é a esteira, não a conversa.
    #[serde(default)]
    pub stage: bool,
    /// Mensagem que começou o turno: as mandadas no meio dele vêm depois e não são o pedido.
    #[serde(default)]
    pub message_id: Option<i64>,
}

/// Conversas paradas, guardadas nas configurações (sobrevivem ao fechamento do app).
pub struct ParkedTurns {
    pub settings: Arc<dyn Settings>,
    /// Ler e gravar a lista é um passo só.
    pub lock: Mutex<()>,
}

impl ParkedTurns {
    pub fn new(settings: Arc<dyn Settings>) -> Self {
        ParkedTurns { settings, lock: Mutex::default() }
    }

    fn read(&self) -> BTreeMap<String, Parked> {
        self.settings.text(KEY).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn change(&self, f: impl FnOnce(&mut BTreeMap<String, Parked>) -> Option<Parked>) -> AppResult<Option<Parked>> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut all = self.read();
        let out = f(&mut all);
        self.settings.set_text(KEY, &serde_json::to_string(&all).unwrap_or_default())?;
        Ok(out)
    }

    pub fn park(&self, conversation_id: &str, parked: Parked) -> AppResult<()> {
        self.change(|all| all.insert(conversation_id.to_owned(), parked)).map(|_| ())
    }

    /// Tira a conversa da espera; devolve como ela estava, se estava.
    pub fn take(&self, conversation_id: &str) -> AppResult<Option<Parked>> {
        if !self.read().contains_key(conversation_id) {
            return Ok(None);
        }
        self.change(|all| all.remove(conversation_id))
    }

    #[cfg(test)]
    pub fn get(&self, conversation_id: &str) -> Option<Parked> {
        self.read().get(conversation_id).copied()
    }

    pub fn all(&self) -> BTreeMap<String, Parked> {
        self.read()
    }

    /// Ao abrir o app: a espera de uma etapa passa para a execução assim que o turno acaba; uma que
    /// ficou para trás é de uma etapa que o fechamento do app interrompeu.
    pub fn clear_stages(&self) -> AppResult<()> {
        self.change(|all| {
            all.retain(|_, p| !p.stage);
            None
        })
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::World;

    #[test]
    fn parks_takes_and_lists() {
        let world = World::with_accounts(vec![]);
        let parked = ParkedTurns::new(world.store.clone());
        let p = Parked { resets_at: Some(10), continuing: true, stage: false, message_id: Some(1) };
        parked.park("c1", p).unwrap();
        parked.park("c2", Parked { resets_at: None, continuing: false, stage: true, message_id: None }).unwrap();
        assert_eq!(parked.get("c1"), Some(p));
        assert_eq!(parked.all().len(), 2);
        assert_eq!(parked.take("c1").unwrap(), Some(p));
        assert_eq!(parked.take("c1").unwrap(), None);
        assert_eq!(parked.all().keys().collect::<Vec<_>>(), ["c2"]);
        parked.clear_stages().unwrap();
        assert!(parked.all().is_empty());
    }
}
