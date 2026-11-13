//! Gestão do pool de contas.

use super::{
    ports::{AccountGateway, AccountRepository, EventSink, IdGenerator, UsageProbe},
    AppError, AppResult,
};
use crate::domain::account::{Account, Identity, Provider, ProviderInfo, UsageLimit};
use std::sync::Arc;

const DEFAULT_ACCOUNT_NAME: &str = "Principal";

/// Tamanho máximo do apelido de uma conta.
const MAX_NAME: usize = 40;

pub struct ManageAccounts {
    pub accounts: Arc<dyn AccountRepository>,
    pub gateway: Arc<dyn AccountGateway>,
    pub events: Arc<dyn EventSink>,
    pub ids: Arc<dyn IdGenerator>,
    pub usage: Arc<dyn UsageProbe>,
}

impl ManageAccounts {
    /// Consulta o uso atual de cada conta com login válido e guarda o resultado.
    /// Contas que falham ficam como estavam; devolve quantas foram atualizadas.
    pub async fn refresh_usage(&self) -> AppResult<usize> {
        let mut updated = 0;
        for account in self.accounts.all()?.into_iter().filter(|a| a.enabled && a.auth_ok) {
            if !self.machine_login_still_ours(&account).await? {
                updated += 1;
                continue;
            }
            if let Ok(rate) = self.usage.usage(account.provider, account.config_dir.as_deref()).await {
                self.accounts.set_usage(&account.id, &rate)?;
                updated += 1;
            }
        }
        if updated > 0 {
            self.events.accounts_changed();
        }
        Ok(updated)
    }

    /// Limites da conta consultados agora no provedor: sessão, semana e semana por modelo.
    pub async fn limits(&self, id: &str) -> AppResult<Vec<UsageLimit>> {
        let account = self.find(id)?;
        if !account.auth_ok {
            return Err(AppError("Entre novamente nesta conta para ver os limites.".into()));
        }
        if !self.machine_login_still_ours(&account).await? {
            self.events.accounts_changed();
            return Err(AppError(MACHINE_LOGIN_MOVED.into()));
        }
        self.usage.limits(account.provider, account.config_dir.as_deref()).await
    }

    /// Confere se o login do computador ainda é de quem o usa; avisa a tela se algo mudou.
    pub async fn verify_machine_logins(&self) -> AppResult<()> {
        let mut changed = false;
        for account in self.accounts.all()?.into_iter().filter(|a| a.auth_ok && a.config_dir.is_none()) {
            changed |= !self.machine_login_still_ours(&account).await?;
        }
        if changed {
            self.events.accounts_changed();
        }
        Ok(())
    }

    /// O login do computador pode ser trocado por fora (no terminal ou em outro app). Se ele
    /// passou a ser de outra pessoa, a conta fica como "login necessário" em vez de consultar
    /// e gastar a cota de outra conta. Contas com perfil próprio não dependem dele.
    async fn machine_login_still_ours(&self, account: &Account) -> AppResult<bool> {
        if account.config_dir.is_some() {
            return Ok(true);
        }
        let identity = self.gateway.identity(account.provider, None).await;
        if same_owner(account, &identity) {
            return Ok(true);
        }
        self.accounts.set_auth(&account.id, false)?;
        Ok(false)
    }

    pub fn list(&self) -> AppResult<Vec<Account>> {
        self.accounts.all()
    }

    /// Na primeira execução, registra o login do Claude que já existe neste computador.
    pub async fn ensure_default(&self) -> AppResult<()> {
        if !self.accounts.all()?.is_empty() {
            return Ok(());
        }
        let identity = self.gateway.identity(Provider::Claude, None).await;
        if identity.logged_in {
            self.accounts.insert(&self.ids.new_id(), DEFAULT_ACCOUNT_NAME, Provider::Claude, &identity, None)?;
            self.events.accounts_changed();
        }
        Ok(())
    }

    /// Tipos de conta que o Lume conhece e se o CLI de cada um está instalado.
    pub fn providers(&self) -> Vec<ProviderInfo> {
        Provider::ALL
            .into_iter()
            .map(|p| ProviderInfo { id: p, label: p.label().into(), cli: p.cli_label().into(), installed: self.gateway.installed(p) })
            .collect()
    }

    /// Adiciona uma conta do tipo indicado. ChatGPT e Gemini aproveitam o login que o CLI
    /// já tem neste computador; do contrário (e sempre no Claude) o login abre em um perfil isolado.
    pub async fn add(&self, name: &str, provider: Provider) -> AppResult<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError("Dê um nome para a conta.".into()));
        }
        if !self.gateway.installed(provider) {
            return Err(AppError(format!(
                "Instale o {} neste computador para adicionar uma conta do {}.",
                provider.cli_label(),
                provider.label()
            )));
        }
        let id = self.ids.new_id();
        let existing = self.accounts.all()?;
        let machine_login_used = existing.iter().any(|a| a.provider == provider && a.config_dir.is_none());
        if provider != Provider::Claude && !machine_login_used {
            let identity = self.gateway.identity(provider, None).await;
            if identity.logged_in {
                self.accounts.insert(&id, name, provider, &identity, None)?;
                self.events.accounts_changed();
                return Ok(());
            }
        }
        let config_dir = self.gateway.prepare_profile(provider, &id)?;
        let identity = match self.gateway.login(provider, &config_dir).await {
            Ok(identity) => identity,
            Err(e) => {
                self.gateway.discard_profile(provider, &config_dir);
                return Err(e);
            }
        };
        let duplicate = existing
            .iter()
            .any(|a| a.provider == provider && a.email.is_some() && a.email == identity.email);
        if duplicate {
            self.gateway.discard_profile(provider, &config_dir);
            return Err(AppError("Esta conta já está cadastrada.".into()));
        }
        self.accounts.insert(&id, name, provider, &identity, Some(&config_dir))?;
        self.events.accounts_changed();
        Ok(())
    }

    /// Remove a conta. O login do Claude deste computador fica: é ele que o Lume registra sozinho.
    pub fn remove(&self, id: &str) -> AppResult<()> {
        let account = self.find(id)?;
        if account.config_dir.is_none() && account.provider == Provider::Claude {
            return Err(AppError(
                "Esta conta usa o login deste computador. Desative-a em vez de remover.".into(),
            ));
        }
        self.accounts.delete(id)?;
        if let Some(config_dir) = &account.config_dir {
            self.gateway.discard_profile(account.provider, config_dir);
        }
        self.events.accounts_changed();
        Ok(())
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> AppResult<()> {
        self.accounts.set_enabled(id, enabled)?;
        self.events.accounts_changed();
        Ok(())
    }

    /// Muda o apelido da conta, que só existe no Lume; o login não é tocado.
    pub fn rename(&self, id: &str, name: &str) -> AppResult<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError("Dê um nome para a conta.".into()));
        }
        if name.chars().count() > MAX_NAME {
            return Err(AppError(format!("O nome pode ter no máximo {MAX_NAME} caracteres.")));
        }
        self.find(id)?;
        self.accounts.set_name(id, name)?;
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
        let identity = self.gateway.identity(account.provider, account.config_dir.as_deref()).await;
        if account.config_dir.is_none() && identity.logged_in && !same_owner(&account, &identity) {
            // Não adota o e-mail de quem entrou no computador: a conta continua sendo a mesma.
            self.accounts.set_auth(id, false)?;
        } else {
            self.accounts.set_identity(id, &identity)?;
        }
        self.events.accounts_changed();
        Ok(())
    }

    pub async fn login_again(&self, id: &str) -> AppResult<()> {
        let account = self.find(id)?;
        let identity = match &account.config_dir {
            Some(config_dir) => self.gateway.login(account.provider, config_dir).await?,
            // Login do computador: se ainda é desta conta, basta conferir; se foi trocado
            // ou perdido, a conta ganha um perfil próprio e deixa de depender dele.
            None => {
                let identity = self.gateway.identity(account.provider, None).await;
                if same_owner(&account, &identity) {
                    identity
                } else {
                    self.move_to_own_profile(&account).await?
                }
            }
        };
        self.accounts.set_identity(id, &identity)?;
        self.events.accounts_changed();
        Ok(())
    }

    /// Abre o login da conta em um perfil isolado e passa a usá-lo no lugar do login do computador.
    async fn move_to_own_profile(&self, account: &Account) -> AppResult<Identity> {
        let config_dir = self.gateway.prepare_profile(account.provider, &account.id).map_err(|_| {
            AppError(format!("Refaça o login desta conta pelo {} no terminal e tente de novo.", account.provider.cli_label()))
        })?;
        let identity = match self.gateway.login(account.provider, &config_dir).await {
            Ok(identity) => identity,
            Err(e) => {
                self.gateway.discard_profile(account.provider, &config_dir);
                return Err(e);
            }
        };
        let taken_by = self.accounts.all()?.into_iter().find(|a| {
            a.id != account.id && a.provider == account.provider && identity.email.is_some() && same_email(&a.email, &identity.email)
        });
        if let Some(other) = taken_by {
            self.gateway.discard_profile(account.provider, &config_dir);
            return Err(AppError(format!("Este login já é da conta \"{}\". Entre com o login de \"{}\".", other.name, account.name)));
        }
        self.accounts.set_config_dir(&account.id, Some(&config_dir))?;
        Ok(identity)
    }

    fn find(&self, id: &str) -> AppResult<Account> {
        self.accounts
            .all()?
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| AppError("Conta não encontrada.".into()))
    }
}

const MACHINE_LOGIN_MOVED: &str =
    "O login deste computador agora é de outra conta. Clique em Entrar para ligar esta conta ao login dela.";

/// O login informado pelo CLI é o desta conta (sem e-mail guardado, qualquer login serve).
fn same_owner(account: &Account, identity: &Identity) -> bool {
    identity.logged_in && (account.email.is_none() || identity.email.is_none() || same_email(&account.email, &identity.email))
}

fn same_email(a: &Option<String>, b: &Option<String>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a.trim().eq_ignore_ascii_case(b.trim()))
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
            usage: Arc::new(FakeUsage),
        }
    }

    #[tokio::test]
    async fn refreshes_usage_of_accounts_that_can_answer() {
        let world = World::with_accounts(vec![crate::domain::account::sample("a", 1), crate::domain::account::sample("b", 2)]);
        world.store.set_auth("b", false).unwrap();
        let uc = use_case(&world, FakeGateway::logged_in(identity("a@exemplo.com")));
        assert_eq!(uc.refresh_usage().await.unwrap(), 1);
        let all = uc.list().unwrap();
        assert_eq!((all[0].util_5h, all[0].resets_7d), (Some(0.5), Some(900)));
        assert_eq!(all[1].util_5h, None, "conta sem login não é consultada");
    }

    #[tokio::test]
    async fn lists_limits_only_for_accounts_with_login() {
        let world = World::with_accounts(vec![crate::domain::account::sample("a", 1), crate::domain::account::sample("b", 2)]);
        world.store.set_auth("b", false).unwrap();
        let uc = use_case(&world, FakeGateway::logged_in(identity("a@exemplo.com")));
        let limits = uc.limits("a").await.unwrap();
        assert_eq!(limits.iter().map(|l| l.kind.as_str()).collect::<Vec<_>>(), ["session", "weekly_all"]);
        assert_eq!((limits[1].used, limits[1].resets_at), (0.2, Some(900)));
        assert!(uc.limits("b").await.is_err(), "conta sem login não é consultada");
    }

    fn identity(email: &str) -> Identity {
        Identity { logged_in: true, email: Some(email.into()), plan: Some("max".into()), login_expires_at: Some(99) }
    }

    #[test]
    fn renames_account_and_rejects_empty_or_long_names() {
        let world = World::with_accounts(vec![crate::domain::account::sample("a", 1)]);
        let uc = use_case(&world, FakeGateway::logged_in(identity("a@exemplo.com")));
        uc.rename("a", "  Trabalho  ").unwrap();
        assert_eq!(uc.list().unwrap()[0].name, "Trabalho");
        assert!(uc.rename("a", "   ").is_err());
        assert!(uc.rename("a", &"x".repeat(41)).is_err());
        assert!(uc.rename("nao-existe", "Outra").is_err());
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
        uc.add("Trabalho", Provider::Claude).await.unwrap();
        let all = uc.list().unwrap();
        assert_eq!(all[0].config_dir.as_deref(), Some("/perfis/id-1"));
        assert_eq!(all[0].plan.as_deref(), Some("max"));

        let err = uc.add("Outra", Provider::Claude).await.unwrap_err();
        assert!(err.0.contains("já está cadastrada"));
        assert_eq!(uc.list().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn other_services_reuse_the_machine_login_then_isolate() {
        let world = World::with_accounts(vec![]);
        let uc = use_case(&world, FakeGateway::logged_in(identity("c@exemplo.com")));
        uc.add("GPT", Provider::ChatGpt).await.unwrap();
        let all = uc.list().unwrap();
        assert_eq!((all[0].provider, all[0].config_dir.as_deref()), (Provider::ChatGpt, None), "aproveita o login do Codex");

        // O login do computador já está em uso: a segunda conta abre o login em um perfil próprio
        // e é recusada por repetir o e-mail.
        let err = uc.add("GPT 2", Provider::ChatGpt).await.unwrap_err();
        assert!(err.0.contains("já está cadastrada"));

        // O mesmo e-mail em outro serviço é outra conta.
        uc.add("Google", Provider::Gemini).await.unwrap();
        assert_eq!(uc.list().unwrap().len(), 2);
        // O Antigravity guarda um único login: não há perfil isolado para a segunda conta.
        assert!(uc.add("Google 2", Provider::Gemini).await.is_err());

        // Diferente do login do Claude, o de outro serviço pode ser removido.
        let gpt = uc.list().unwrap()[0].id.clone();
        uc.remove(&gpt).unwrap();
        assert_eq!(uc.list().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn refuses_a_service_whose_cli_is_missing() {
        let world = World::with_accounts(vec![]);
        let mut gateway = FakeGateway::logged_in(identity("a@exemplo.com"));
        gateway.missing = vec![Provider::Gemini];
        let uc = use_case(&world, gateway);
        let err = uc.add("Google", Provider::Gemini).await.unwrap_err();
        assert!(err.0.contains("Antigravity"), "{}", err.0);
        let info = uc.providers();
        assert_eq!(info.iter().map(|p| p.installed).collect::<Vec<_>>(), [true, true, false]);
    }

    #[tokio::test]
    async fn failed_login_leaves_nothing_behind() {
        let world = World::with_accounts(vec![]);
        let gateway = FakeGateway::failing("cancelado");
        let discarded = gateway.discarded.clone();
        let uc = use_case(&world, gateway);
        assert!(uc.add("Trabalho", Provider::Claude).await.is_err());
        assert!(uc.list().unwrap().is_empty());
        assert_eq!(discarded.lock().unwrap().len(), 1);
    }

    /// Conta "Lucas" no login do computador e "Dev" em perfil próprio; o computador passou a "dev@".
    fn machine_login_switched(world: &World, browser_login: &str) -> ManageAccounts {
        AccountRepository::insert(&*world.store, "m", "Lucas", Provider::Claude, &identity("lucas@exemplo.com"), None).unwrap();
        AccountRepository::insert(&*world.store, "d", "Dev", Provider::Claude, &identity("dev@exemplo.com"), Some("/perfis/d")).unwrap();
        let mut gateway = FakeGateway::logged_in(identity(browser_login));
        gateway.machine = Some(identity("DEV@exemplo.com"));
        use_case(world, gateway)
    }

    #[tokio::test]
    async fn switched_machine_login_stops_lending_its_quota() {
        let world = World::with_accounts(vec![]);
        let uc = machine_login_switched(&world, "lucas@exemplo.com");
        uc.refresh_usage().await.unwrap();
        let all = uc.list().unwrap();
        let lucas = all.iter().find(|a| a.id == "m").unwrap();
        assert!(!lucas.auth_ok, "o computador agora é de outra conta");
        assert_eq!(lucas.util_5h, None, "não guarda o uso de outra conta");
        assert_eq!(all.iter().find(|a| a.id == "d").unwrap().util_5h, Some(0.5));
        assert!(uc.limits("m").await.is_err());

        uc.refresh("m").await.unwrap();
        let lucas = uc.list().unwrap().into_iter().find(|a| a.id == "m").unwrap();
        assert_eq!(lucas.email.as_deref(), Some("lucas@exemplo.com"), "não adota o e-mail de quem entrou no computador");
        assert!(!lucas.auth_ok);
    }

    #[tokio::test]
    async fn login_again_moves_switched_machine_account_to_its_own_profile() {
        let world = World::with_accounts(vec![]);
        let uc = machine_login_switched(&world, "lucas@exemplo.com");
        uc.verify_machine_logins().await.unwrap();
        uc.login_again("m").await.unwrap();
        let lucas = uc.list().unwrap().into_iter().find(|a| a.id == "m").unwrap();
        assert_eq!(lucas.config_dir.as_deref(), Some("/perfis/m"));
        assert!(lucas.auth_ok);
        assert!(uc.remove("m").is_ok(), "com perfil próprio, a conta pode ser removida");
    }

    #[tokio::test]
    async fn moving_to_own_profile_refuses_another_accounts_login() {
        let world = World::with_accounts(vec![]);
        let gateway_uc = machine_login_switched(&world, "dev@exemplo.com");
        let err = gateway_uc.login_again("m").await.unwrap_err();
        assert!(err.0.contains("Dev"), "{}", err.0);
        let lucas = gateway_uc.list().unwrap().into_iter().find(|a| a.id == "m").unwrap();
        assert_eq!(lucas.config_dir, None);
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
