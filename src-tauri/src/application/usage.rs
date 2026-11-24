//! Painel de uso: o gasto de tokens do período e o ritmo de cada conta.

use super::{
    ports::{AccountRepository, Clock, UsageLog},
    AppResult,
};
use crate::domain::usage::{runs_out_at, summarize, UsageSummary, SESSION_WINDOW, WEEK_WINDOW};
use serde::Serialize;
use std::sync::Arc;

pub struct ManageUsage {
    pub log: Arc<dyn UsageLog>,
    pub accounts: Arc<dyn AccountRepository>,
    pub clock: Arc<dyn Clock>,
}

/// Quando a conta esgota no ritmo atual, em cada janela (só quando é antes de a janela renovar).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AccountPace {
    pub id: String,
    pub session_out_at: Option<i64>,
    pub week_out_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsageReport {
    #[serde(flatten)]
    pub summary: UsageSummary,
    /// Trocas de conta no período.
    pub switches: i64,
    pub paces: Vec<AccountPace>,
}

impl ManageUsage {
    /// Gasto dos últimos `days` dias, agrupado no fuso de quem vê.
    pub fn report(&self, days: i64, tz_offset_minutes: i64) -> AppResult<UsageReport> {
        let now = self.clock.now();
        let days = days.clamp(1, 366);
        let since = now - days * 86_400 - 86_400;
        let summary = summarize(&self.log.usage_since(since)?, days, now, tz_offset_minutes);
        let switches = self.log.switches_since(summary.since)?;
        let pace = |used: Option<f64>, resets: Option<i64>, window| runs_out_at(used?, resets?, window, now);
        let paces = self
            .accounts
            .all()?
            .into_iter()
            .filter(|a| a.enabled)
            .map(|a| AccountPace {
                session_out_at: pace(a.util_5h, a.resets_5h, SESSION_WINDOW),
                week_out_at: pace(a.util_7d, a.resets_7d, WEEK_WINDOW),
                id: a.id,
            })
            .collect();
        Ok(UsageReport { summary, switches, paces })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::{FixedClock, World};
    use crate::domain::{account::sample, chat::{RateInfo, TokenUsage}};
    use crate::application::ports::ConversationRepository as _;

    #[test]
    fn reports_spend_switches_and_the_pace_of_each_account() {
        // O banco de teste grava no relógio dele (1000 s), dentro dos últimos 7 dias.
        let now = 2 * 86_400;
        let world = World::with_accounts(vec![sample("a", 1)]);
        world.store.add_usage("c1", Some("a"), &TokenUsage { input: 5, ..Default::default() }).unwrap();
        // Metade da semana usada em 2 dias: esgota antes do reset.
        let rate = RateInfo { status: "allowed".into(), resets_at: None, util_5h: None, util_7d: Some(0.5), resets_5h: None, resets_7d: Some(now + 5 * 86_400) };
        world.store.set_usage("a", &rate).unwrap();
        let uc = ManageUsage { log: world.store.clone(), accounts: world.store.clone(), clock: Arc::new(FixedClock(now)) };
        let report = uc.report(7, 0).unwrap();
        assert_eq!(report.paces, [AccountPace { id: "a".into(), session_out_at: None, week_out_at: Some(now + 2 * 86_400) }]);
        assert_eq!(report.switches, 0);
        assert_eq!((report.summary.total.input, report.summary.by_account[0].key.as_str()), (5, "a"));
        assert_eq!(report.summary.days.len(), 7);
    }
}
