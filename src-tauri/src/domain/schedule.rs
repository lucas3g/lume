//! Tarefas agendadas: um prompt que roda sozinho, em uma conversa nova, na cadência escolhida.

use serde::{Deserialize, Serialize};

const DAY: i64 = 86_400;
const WEEK: i64 = 7 * DAY;

/// Cadência de uma tarefa. Horários são no fuso do usuário (`tz_offset_minutes` da tarefa).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Repeat {
    EveryMinutes { minutes: i64 },
    Daily { hour: i64, minute: i64 },
    /// `weekday`: 0 = domingo … 6 = sábado.
    Weekly { weekday: i64, hour: i64, minute: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub prompt: String,
    /// Projeto em que a conversa é criada; com pasta, vira uma sessão de agente nela.
    pub project_id: Option<String>,
    pub model: String,
    pub repeat: Repeat,
    /// Diferença do fuso do usuário para o UTC, em minutos (ex.: -180 para Brasília).
    pub tz_offset_minutes: i64,
    pub enabled: bool,
    pub last_run_at: Option<i64>,
    pub next_run_at: i64,
}

impl Task {
    pub fn is_due(&self, now: i64) -> bool {
        self.enabled && self.next_run_at <= now
    }
}

/// A cadência faz sentido: minutos positivos, horas e dias válidos.
pub fn validate_repeat(repeat: &Repeat) -> Result<(), String> {
    let time_ok = |h: i64, m: i64| (0..24).contains(&h) && (0..60).contains(&m);
    match repeat {
        Repeat::EveryMinutes { minutes } if *minutes < 5 => Err("O intervalo mínimo é de 5 minutos.".into()),
        Repeat::EveryMinutes { .. } => Ok(()),
        Repeat::Daily { hour, minute } | Repeat::Weekly { hour, minute, .. } if !time_ok(*hour, *minute) => {
            Err("Horário inválido.".into())
        }
        Repeat::Weekly { weekday, .. } if !(0..7).contains(weekday) => Err("Dia da semana inválido.".into()),
        _ => Ok(()),
    }
}

/// Próxima execução estritamente depois de `after` (instante Unix), na cadência dada.
pub fn next_run(repeat: &Repeat, after: i64, tz_offset_minutes: i64) -> i64 {
    let offset = tz_offset_minutes * 60;
    match repeat {
        Repeat::EveryMinutes { minutes } => after + (*minutes).max(1) * 60,
        Repeat::Daily { hour, minute } => {
            let local = after + offset;
            let day_start = local.div_euclid(DAY) * DAY;
            let mut candidate = day_start + hour * 3600 + minute * 60;
            if candidate <= local {
                candidate += DAY;
            }
            candidate - offset
        }
        Repeat::Weekly { weekday, hour, minute } => {
            let local = after + offset;
            let day_start = local.div_euclid(DAY) * DAY;
            // 1970-01-01 foi quinta-feira (4).
            let today = (local.div_euclid(DAY) + 4).rem_euclid(7);
            let ahead = (weekday - today).rem_euclid(7);
            let mut candidate = day_start + ahead * DAY + hour * 3600 + minute * 60;
            if candidate <= local {
                candidate += WEEK;
            }
            candidate - offset
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-11-13T01:27:24Z, sexta-feira; em Brasília (UTC-3) é quinta 22:27.
    const NOW: i64 = 1_794_533_244;
    const BRT: i64 = -180;

    #[test]
    fn daily_runs_at_the_next_local_time() {
        let next = next_run(&Repeat::Daily { hour: 9, minute: 0 }, NOW, BRT);
        assert_eq!(next, 1_794_571_200, "sexta 09:00 BRT = 12:00Z");
        let same_day = next_run(&Repeat::Daily { hour: 23, minute: 0 }, NOW, BRT);
        assert_eq!(same_day, NOW + 33 * 60 - 24, "ainda hoje, 23:00 BRT");
        assert_eq!(next_run(&Repeat::Daily { hour: 9, minute: 0 }, next, BRT), next + DAY);
    }

    #[test]
    fn weekly_waits_for_the_weekday() {
        // Quinta 22:27 BRT: a próxima quinta 10:00 é daqui a 7 dias menos 12h27.
        let thursday = next_run(&Repeat::Weekly { weekday: 4, hour: 10, minute: 0 }, NOW, BRT);
        assert_eq!(thursday, NOW + WEEK - (12 * 3600 + 27 * 60 + 24));
        let friday = next_run(&Repeat::Weekly { weekday: 5, hour: 8, minute: 30 }, NOW, BRT);
        assert_eq!(friday, NOW + 10 * 3600 + 2 * 60 + 36, "amanhã (sexta) 08:30 BRT");
    }

    #[test]
    fn interval_and_validation() {
        assert_eq!(next_run(&Repeat::EveryMinutes { minutes: 30 }, 100, 0), 1900);
        assert!(validate_repeat(&Repeat::EveryMinutes { minutes: 1 }).is_err());
        assert!(validate_repeat(&Repeat::Daily { hour: 24, minute: 0 }).is_err());
        assert!(validate_repeat(&Repeat::Weekly { weekday: 7, hour: 1, minute: 0 }).is_err());
        assert!(validate_repeat(&Repeat::Weekly { weekday: 6, hour: 23, minute: 59 }).is_ok());
    }
}
