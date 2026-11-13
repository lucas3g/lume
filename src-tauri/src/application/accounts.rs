//! Gestão do pool de contas.

use super::{
    ports::{AccountGateway, AccountRepository, EventSink, IdGenerator},
    AppError, AppResult,
};
use crate::domain::account::Account;
use std::sync::Arc;

const DEFAULT_ACCOUNT_NAME: &str = "Principal";

pub struct ManageAccounts {
    pub accounts: Arc<dyn AccountRepository>,
    pub gateway: Arc<dyn AccountGateway>,
    pub events: Arc<dyn EventSink>,
    pub ids: Arc<dyn IdGenerator>,
}

impl ManageAccounts {
    pub fn list(&self) -> AppResult<Vec<Account>> {
        self.accounts.all()
    }

    /// Na primeira execução, registra o login que já existe neste computador.
    pub async fn ensure_default(&self) -> AppResult<()> {
        if !self.accounts.all()?.is_empty() {
            return Ok(());
        }
        let identity = self.gateway.identity(None).await;
        if identity.logged_in {
            self.accounts.insert(&self.ids.new_id(), DEFAULT_ACCOUNT_NAME, &identity, None)?;
            self.events.accounts_changed();
        }
        Ok(())
    }

    /// Cria um perfil isolado e abre o login oficial para ele.
    pub async fn add(&self, name: &str) -> AppResult<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError("Dê um nome para a conta.".into()));
        }
        let id = self.ids.new_id();
        let config_dir = self.gateway.prepare_profile(&id)?;
        let identity = match self.gateway.login(&config_dir).await {
            Ok(identity) => identity,
            Err(e) => {
                self.gateway.discard_profile(&config_dir);
                return Err(e);
            }
        };
        let duplicate = self
            .accounts
            .all()?
            .iter()
            .any(|a| a.email.is_some() && a.email == identity.email);
        if duplicate {
            self.gateway.discard_profile(&config_dir);
            return Err(AppError("Esta conta já está cadastrada.".into()));
        }
        self.accounts.insert(&id, name, &identity, Some(&config_dir))?;
        self.events.accounts_changed();
        Ok(())
    }

    pub fn remove(&self, id: &str) -> AppResult<()> {
        let account = self.find(id)?;
        let Some(config_dir) = account.config_dir else {
            return Err(AppError(
                "Esta conta usa o login deste computador. Desative-a em vez de remover.".into(),
            ));
        };
        self.accounts.delete(id)?;
        self.gateway.discard_profile(&config_dir);
        self.events.accounts_changed();
        Ok(())
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> AppResult<()> {
        self.accounts.set_enabled(id, enabled)?;
        self.events.accounts_changed();
        Ok(())
    }

    pub fn reorder(&self, ids: &[String]) -> AppResult<()> {
        self.accounts.reorder(ids)?;
        self.events.accounts_changed();
        Ok(())
    }

    /// Consulta novamente o provedor: e-mail, plano e se o login continua válido.
    pub async fn refresh(&self, id: &str) -> AppResult<()> {
        let account = self.find(id)?;
        let identity = self.gateway.identity(account.config_dir.as_deref()).await;
        self.accounts.set_identity(id, &identity)?;
        self.events.accounts_changed();
        Ok(())
    }

    pub async fn login_again(&self, id: &str) -> AppResult<()> {
        let account = self.find(id)?;
        let Some(config_dir) = account.config_dir else {
            return Err(AppError("Refaça o login desta conta pelo Claude Code no terminal.".into()));
        };
        let identity = self.gateway.login(&config_dir).await?;
        self.accounts.set_identity(id, &identity)?;
        self.events.accounts_changed();
        Ok(())
    }

    fn find(&self, id: &str) -> AppResult<Account> {
        self.accounts
            .all()?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| AppError("Conta não encontrada.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;
    use crate::domain::account::Identity;

    fn use_case(world: &World, gateway: FakeGateway) -> ManageAccounts {
        ManageAccounts {
            accounts: world.store.clone(),
            gateway: Arc::new(gateway),
            events: world.events.clone(),
            ids: Arc::new(SequentialIds::default()),
        }
    }

    fn identity(email: &str) -> Identity {
        Identity { logged_in: true, email: Some(email.into()), plan: Some("max".into()), login_expires_at: Some(99) }
    }

    #[tokio::test]
    async fn registers_machine_login_once() {
        let world = World::with_accounts(vec![]);
        let uc = use_case(&world, FakeGateway::logged_in(identity("a@exemplo.com")));
        uc.ensure_default().await.unwrap();
        uc.ensure_default().await.unwrap();
        let all = uc.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "Principal");
        assert_eq!(all[0].config_dir, None);
    }

    #[tokio::test]
    async fn adds_account_with_isolated_profile_and_rejects_duplicates() {
        let world = World::with_accounts(vec![]);
        let uc = use_case(&world, FakeGateway::logged_in(identity("b@exemplo.com")));
        uc.add("Trabalho").await.unwrap();
        let all = uc.list().unwrap();
        assert_eq!(all[0].config_dir.as_deref(), Some("/perfis/id-1"));
        assert_eq!(all[0].plan.as_deref(), Some("max"));

        let err = uc.add("Outra").await.unwrap_err();
        assert!(err.0.contains("já está cadastrada"));
        assert_eq!(uc.list().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn failed_login_leaves_nothing_behind() {
        let world = World::with_accounts(vec![]);
        let gateway = FakeGateway::failing("cancelado");
        let discarded = gateway.discarded.clone();
        let uc = use_case(&world, gateway);
        assert!(uc.add("Trabalho").await.is_err());
        assert!(uc.list().unwrap().is_empty());
        assert_eq!(discarded.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn machine_login_cannot_be_removed() {
        let world = World::with_accounts(vec![]);
        let uc = use_case(&world, FakeGateway::logged_in(identity("a@exemplo.com")));
        uc.ensure_default().await.unwrap();
        let id = uc.list().unwrap()[0].id.clone();
        assert!(uc.remove(&id).is_err());
    }
}
