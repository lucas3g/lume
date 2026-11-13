//! Conta do Claude e as regras do pool: qual conta usar e quando ela volta.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    /// Diretório de configuração isolado; `None` é o login padrão do computador.
    pub config_dir: Option<String>,
    pub priority: i64,
    pub enabled: bool,
    pub auth_ok: bool,
    pub limited_until: Option<i64>,
    pub util_5h: Option<f64>,
    pub util_7d: Option<f64>,
    pub resets_5h: Option<i64>,
    pub resets_7d: Option<i64>,
    /// Quando o login da conta vence e precisa ser refeito.
    pub login_expires_at: Option<i64>,
}

/// Uso da janela de 5 h a partir do qual a conta é poupada enquanto houver outra livre.
pub const HEADROOM_LIMIT: f64 = 0.95;

impl Account {
    pub fn is_usable(&self, now: i64) -> bool {
        self.enabled && self.auth_ok && self.limited_until.is_none_or(|t| t <= now)
    }

    /// Está prestes a atingir o limite da janela atual.
    pub fn near_limit(&self, now: i64) -> bool {
        self.util_5h.is_some_and(|u| u >= HEADROOM_LIMIT) && self.resets_5h.is_some_and(|t| t > now)
    }
}

/// Identidade informada pelo provedor após o login.
#[derive(Debug, Clone, Default)]
pub struct Identity {
    pub logged_in: bool,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub login_expires_at: Option<i64>,
}

/// Primeira conta utilizável na ordem de prioridade, ignorando as já tentadas neste envio.
/// Com `spare_near_limit`, contas perto do limite só são usadas se não houver outra.
pub fn pick_next<'a>(accounts: &'a [Account], now: i64, tried: &[String], spare_near_limit: bool) -> Option<&'a Account> {
    let usable = || accounts.iter().filter(|a| a.is_usable(now) && !tried.contains(&a.id));
    let preferred = spare_near_limit.then(|| usable().filter(|a| !a.near_limit(now)).min_by_key(|a| a.priority)).flatten();
    preferred.or_else(|| usable().min_by_key(|a| a.priority))
}

/// Horário do reset mais próximo entre as contas limitadas.
pub fn earliest_reset(accounts: &[Account], now: i64) -> Option<i64> {
    accounts
        .iter()
        .filter(|a| a.enabled && a.auth_ok)
        .filter_map(|a| a.limited_until)
        .filter(|t| *t > now)
        .min()
}

#[cfg(test)]
pub(crate) fn sample(id: &str, priority: i64) -> Account {
    Account {
        id: id.into(),
        name: id.into(),
        email: None,
        plan: None,
        config_dir: None,
        priority,
        enabled: true,
        auth_ok: true,
        limited_until: None,
        util_5h: None,
        util_7d: None,
        resets_5h: None,
        resets_7d: None,
        login_expires_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_by_priority_and_skips_unusable() {
        let mut list = vec![sample("b", 2), sample("a", 1), sample("c", 3)];
        assert_eq!(pick_next(&list, 100, &[], true).unwrap().id, "a");

        list[1].limited_until = Some(500); // "a" limitada
        assert_eq!(pick_next(&list, 100, &[], true).unwrap().id, "b");
        assert_eq!(pick_next(&list, 600, &[], true).unwrap().id, "a", "volta após o reset");

        list[0].enabled = false; // "b" desativada
        assert_eq!(pick_next(&list, 100, &[], true).unwrap().id, "c");

        list[2].auth_ok = false;
        assert!(pick_next(&list, 100, &[], true).is_none());
        assert_eq!(earliest_reset(&list, 100), Some(500));
    }

    #[test]
    fn skips_accounts_already_tried() {
        let list = vec![sample("a", 1), sample("b", 2)];
        assert_eq!(pick_next(&list, 0, &["a".into()], true).unwrap().id, "b");
        assert!(pick_next(&list, 0, &["a".into(), "b".into()], true).is_none());
    }

    #[test]
    fn spares_account_near_its_limit_while_another_is_free() {
        let mut list = vec![sample("a", 1), sample("b", 2)];
        list[0].util_5h = Some(0.97);
        list[0].resets_5h = Some(900);
        assert_eq!(pick_next(&list, 100, &[], true).unwrap().id, "b", "poupa a conta quase no limite");
        assert_eq!(pick_next(&list, 100, &[], false).unwrap().id, "a", "sem a opção, segue a prioridade");
        assert_eq!(pick_next(&list, 1000, &[], true).unwrap().id, "a", "depois do reset volta ao normal");
        list[1].enabled = false;
        assert_eq!(pick_next(&list, 100, &[], true).unwrap().id, "a", "sem outra conta, usa a que tem");
    }

}
