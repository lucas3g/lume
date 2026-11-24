//! Gasto de tokens ao longo do tempo: o que cada turno gastou, somado por dia, origem, projeto e
//! conta, e a previsão de quando uma conta esgota no ritmo atual.

use super::chat::TokenUsage;
use serde::Serialize;
use std::collections::BTreeMap;

const DAY: i64 = 86_400;
/// Janelas de uso das contas.
pub const SESSION_WINDOW: i64 = 5 * 3600;
pub const WEEK_WINDOW: i64 = 7 * DAY;
/// Antes disto da janela, o ritmo ainda não diz nada.
const MIN_ELAPSED: i64 = 30 * 60;

/// De onde veio o gasto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Chat,
    /// Etapas da esteira de bots.
    Pipeline,
    /// Rodadas das tarefas agendadas.
    Task,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Chat => "chat",
            Source::Pipeline => "pipeline",
            Source::Task => "task",
        }
    }

    pub fn parse(text: &str) -> Source {
        match text {
            "pipeline" => Source::Pipeline,
            "task" => Source::Task,
            _ => Source::Chat,
        }
    }
}

/// O que um turno gastou (um registro por resultado informado pelo CLI).
#[derive(Debug, Clone, PartialEq)]
pub struct UsageEntry {
    pub at: i64,
    /// `None` no gasto de antes do registro por turno (só o total de cada conversa era guardado).
    pub account_id: Option<String>,
    pub project_id: Option<String>,
    pub source: Source,
    pub usage: TokenUsage,
}

/// Gasto de um grupo (um projeto, uma conta, uma origem).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsageGroup {
    /// Id do projeto ou da conta, ou o nome da origem; vazio = sem projeto / conta desconhecida.
    pub key: String,
    pub usage: TokenUsage,
}

/// Gasto de um dia, a partir da meia-noite local (em segundos).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsageDay {
    pub start: i64,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsageSummary {
    pub since: i64,
    pub total: TokenUsage,
    /// Todos os dias do período, inclusive os sem gasto, do mais antigo ao mais novo.
    pub days: Vec<UsageDay>,
    pub by_source: Vec<UsageGroup>,
    pub by_project: Vec<UsageGroup>,
    pub by_account: Vec<UsageGroup>,
}

/// Início do dia local que contém `at`.
fn day_start(at: i64, tz_offset_minutes: i64) -> i64 {
    let offset = tz_offset_minutes * 60;
    (at + offset).div_euclid(DAY) * DAY - offset
}

/// Soma o gasto dos últimos `days` dias (contando hoje), no fuso de quem vê.
pub fn summarize(entries: &[UsageEntry], days: i64, now: i64, tz_offset_minutes: i64) -> UsageSummary {
    let today = day_start(now, tz_offset_minutes);
    let since = today - (days.max(1) - 1) * DAY;
    let mut total = TokenUsage::default();
    let mut per_day: BTreeMap<i64, TokenUsage> = (0..days.max(1)).map(|i| (since + i * DAY, TokenUsage::default())).collect();
    let mut by_source: BTreeMap<String, TokenUsage> = BTreeMap::new();
    let mut by_project: BTreeMap<String, TokenUsage> = BTreeMap::new();
    let mut by_account: BTreeMap<String, TokenUsage> = BTreeMap::new();
    for e in entries.iter().filter(|e| e.at >= since && e.at <= now) {
        total.add(&e.usage);
        // Um dia que mudou de fuso no meio do período cai no mais próximo.
        let day = day_start(e.at, tz_offset_minutes);
        per_day.entry(day).or_default().add(&e.usage);
        by_source.entry(e.source.as_str().to_owned()).or_default().add(&e.usage);
        by_project.entry(e.project_id.clone().unwrap_or_default()).or_default().add(&e.usage);
        by_account.entry(e.account_id.clone().unwrap_or_default()).or_default().add(&e.usage);
    }
    let ranked = |map: BTreeMap<String, TokenUsage>| {
        let mut groups: Vec<UsageGroup> = map.into_iter().map(|(key, usage)| UsageGroup { key, usage }).collect();
        groups.sort_by_key(|g| std::cmp::Reverse(g.usage.total()));
        groups
    };
    UsageSummary {
        since,
        total,
        days: per_day.into_iter().map(|(start, usage)| UsageDay { start, usage }).collect(),
        by_source: ranked(by_source),
        by_project: ranked(by_project),
        by_account: ranked(by_account),
    }
}

/// Quando a conta chega a 100% da janela no ritmo de agora, se for antes de a janela renovar.
/// `used` vai de 0 a 1; a janela começou `window` segundos antes de `resets_at`.
pub fn runs_out_at(used: f64, resets_at: i64, window: i64, now: i64) -> Option<i64> {
    let elapsed = now - (resets_at - window);
    if used >= 1.0 || resets_at <= now || elapsed < MIN_ELAPSED || used <= 0.0 {
        return None;
    }
    let rate = used / elapsed as f64;
    let at = now + ((1.0 - used) / rate).ceil() as i64;
    (at < resets_at).then_some(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(at: i64, account: &str, project: Option<&str>, source: Source, input: u64) -> UsageEntry {
        UsageEntry {
            at,
            account_id: Some(account.into()),
            project_id: project.map(Into::into),
            source,
            usage: TokenUsage { input, cost_usd: Some(input as f64 / 100.0), ..Default::default() },
        }
    }

    #[test]
    fn sums_by_local_day_source_project_and_account() {
        // Fuso -3 h: 02:00 UTC do dia 2 ainda é o dia 1 local.
        let tz = -180;
        let now = 2 * DAY + 12 * 3600;
        let entries = [
            entry(2 * DAY + 2 * 3600, "a", Some("p"), Source::Chat, 10),
            entry(2 * DAY + 4 * 3600, "b", None, Source::Pipeline, 30),
            entry(2 * DAY + 5 * 3600, "a", Some("p"), Source::Pipeline, 5),
            entry(-5 * DAY, "a", None, Source::Chat, 999),
        ];
        let s = summarize(&entries, 2, now, tz);
        assert_eq!(s.total.input, 45, "fora do período não conta");
        assert_eq!(s.days.iter().map(|d| d.usage.input).collect::<Vec<_>>(), [10, 35]);
        assert_eq!(s.days[1].start, 2 * DAY + 3 * 3600, "meia-noite local");
        assert_eq!(s.by_source.iter().map(|g| (g.key.as_str(), g.usage.input)).collect::<Vec<_>>(), [("pipeline", 35), ("chat", 10)]);
        assert_eq!(s.by_project.iter().map(|g| (g.key.as_str(), g.usage.input)).collect::<Vec<_>>(), [("", 30), ("p", 15)]);
        assert_eq!(s.by_account[0].key, "b");
        assert_eq!(s.total.cost_usd, Some(0.45));
    }

    #[test]
    fn forecasts_only_when_the_pace_runs_out_before_the_reset() {
        let window = WEEK_WINDOW;
        let reset = 10 * DAY;
        let start = reset - window;
        // Metade usada em 2 dias: acaba em mais 2 dias, antes do reset (5 dias depois).
        assert_eq!(runs_out_at(0.5, reset, window, start + 2 * DAY), Some(start + 4 * DAY));
        // 10% em 2 dias: não chega a 100% antes do reset.
        assert_eq!(runs_out_at(0.1, reset, window, start + 2 * DAY), None);
        assert_eq!(runs_out_at(0.9, reset, window, start + 60), None, "cedo demais para medir o ritmo");
        assert_eq!(runs_out_at(1.0, reset, window, start + DAY), None, "já esgotou");
    }
}
