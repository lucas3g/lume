//! Acompanhamento do CI do pull request que a esteira abriu: quando os checks terminam, passou ou
//! falhou; falhando, a esteira corrige com o log da falha e manda a correção ao mesmo ramo.

use serde::{Deserialize, Serialize};

/// Correções automáticas por pull request; depois disso, só avisa.
pub const MAX_CI_FIXES: i64 = 2;
/// Intervalo entre consultas ao GitHub.
pub const CI_POLL_SECONDS: i64 = 60;
/// Sem nenhum check depois disto, o repositório não tem CI para este ramo.
pub const NO_CHECKS_GRACE_SECONDS: i64 = 10 * 60;
/// Checks que não terminam depois disto: para de acompanhar.
pub const CI_TIMEOUT_SECONDS: i64 = 6 * 3600;
/// Tamanho do log de cada check que falhou e do pedido inteiro ao Implementador.
const LOG_CHARS: usize = 6_000;
const FEEDBACK_CHARS: usize = 16_000;

/// Um check do pull request como o `gh pr checks --json name,bucket,link` informa.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Check {
    pub name: String,
    /// "pass", "fail", "pending", "skipping" ou "cancel".
    pub bucket: String,
    #[serde(default)]
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    NoChecks,
    Pending,
    Passed,
    /// Os checks que falharam.
    Failed(Vec<Check>),
}

/// Situação dos checks: falhou se algum falhou (e nenhum segue rodando), passou se todos terminaram bem.
pub fn verdict(checks: &[Check]) -> Verdict {
    if checks.is_empty() {
        return Verdict::NoChecks;
    }
    if checks.iter().any(|c| c.bucket == "pending") {
        return Verdict::Pending;
    }
    let failed: Vec<Check> = checks.iter().filter(|c| matches!(c.bucket.as_str(), "fail" | "cancel")).cloned().collect();
    if failed.is_empty() { Verdict::Passed } else { Verdict::Failed(failed) }
}

/// Id do job no link do check (".../actions/runs/<run>/job/<job>").
pub fn job_id(link: &str) -> Option<String> {
    let after = &link[link.find("/job/")? + "/job/".len()..];
    let id: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    (!id.is_empty()).then_some(id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiStatus {
    /// Esperando os checks terminarem.
    Watching,
    Passed,
    /// Falhou e a esteira está corrigindo; no fim, o app manda a correção ao ramo do pull request.
    Fixing,
    /// Falhou e não há mais correção automática (limite, erro ao enviar): só o aviso.
    Failed,
    /// O repositório não tem CI para o ramo.
    NoChecks,
    /// Você parou de acompanhar, ou os checks não terminaram a tempo.
    Stopped,
}

/// Acompanhamento do CI de uma execução.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CiWatch {
    pub pr_url: String,
    pub status: CiStatus,
    /// Quando o último push chegou ao ramo (a espera conta daí).
    pub since: i64,
    pub checked_at: i64,
    /// Correções automáticas já feitas.
    pub fixes: i64,
    /// Checks que falharam na última consulta.
    #[serde(default)]
    pub failed: Vec<String>,
    /// O que aconteceu por último, para a tela.
    #[serde(default)]
    pub note: Option<String>,
    /// Ramo do pull request: a correção vai para ele.
    pub branch: String,
}

impl CiWatch {
    /// Começa (ou recomeça, depois de um push) a esperar os checks; as correções já feitas continuam contando.
    pub fn new(pr_url: &str, branch: &str, fixes: i64, now: i64) -> CiWatch {
        CiWatch { pr_url: pr_url.into(), branch: branch.into(), status: CiStatus::Watching, since: now, checked_at: 0, fixes, failed: vec![], note: None }
    }

    pub fn due(&self, now: i64) -> bool {
        self.status == CiStatus::Watching && now - self.checked_at >= CI_POLL_SECONDS
    }
}

fn clip_tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.trim().to_owned();
    }
    let skip = text.char_indices().nth(count - max).map_or(0, |(i, _)| i);
    format!("…{}", text[skip..].trim())
}

/// Começo do ajuste que o app escreve quando o CI falha (o que o distingue de um ajuste seu).
const CI_FEEDBACK_PREFIX: &str = "O CI do pull request ";

/// O ajuste veio do CI, não de você.
pub fn is_ci_feedback(text: &str) -> bool {
    text.starts_with(CI_FEEDBACK_PREFIX)
}

/// Ajuste que o Implementador recebe: quais checks falharam e o fim do log de cada um.
pub fn ci_feedback(pr_url: &str, failed: &[(Check, Option<String>)]) -> String {
    let header = format!(
        "{CI_FEEDBACK_PREFIX}{pr_url} falhou depois da entrega. Corrija a causa no código (não desligue nem pule os checks) \
         e confira localmente o que der para rodar.\n\nChecks que falharam:\n"
    );
    let mut out = String::new();
    for (check, log) in failed {
        out.push_str(&format!("\n## {}\n", check.name));
        match log.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
            Some(log) => out.push_str(&format!("Fim do log:\n```\n{}\n```\n", clip_tail(log, LOG_CHARS))),
            None => out.push_str(&format!("(sem log disponível{})\n", check.link.as_deref().map(|l| format!("; veja em {l}")).unwrap_or_default())),
        }
    }
    // O corte é só nos logs: o começo (que identifica o ajuste do CI) fica sempre.
    format!("{header}\n{}\n", clip_tail(&out, FEEDBACK_CHARS.saturating_sub(header.chars().count())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(name: &str, bucket: &str) -> Check {
        Check { name: name.into(), bucket: bucket.into(), link: Some(format!("https://github.com/o/r/actions/runs/1/job/42{name}")) }
    }

    #[test]
    fn verdict_waits_for_pending_and_fails_on_any_failure() {
        assert_eq!(verdict(&[]), Verdict::NoChecks);
        assert_eq!(verdict(&[check("a", "pass"), check("b", "pending"), check("c", "fail")]), Verdict::Pending);
        assert_eq!(verdict(&[check("a", "pass"), check("b", "skipping")]), Verdict::Passed);
        assert_eq!(verdict(&[check("a", "pass"), check("b", "fail")]), Verdict::Failed(vec![check("b", "fail")]));
    }

    #[test]
    fn reads_the_job_id_from_the_link() {
        assert_eq!(job_id("https://github.com/o/r/actions/runs/9/job/123?pr=4").as_deref(), Some("123"));
        assert_eq!(job_id("https://ci.example.com/build/7"), None);
    }

    #[test]
    fn feedback_names_the_checks_and_keeps_the_end_of_each_log() {
        let log = format!("{}ERRO FINAL", "x".repeat(10_000));
        let text = ci_feedback("https://pr/1", &[(check("testes", "fail"), Some(log)), (check("lint", "fail"), None)]);
        assert!(text.starts_with("O CI do pull request https://pr/1 falhou") && is_ci_feedback(&text));
        assert!(!is_ci_feedback("o botão não funciona"));
        assert!(text.contains("## testes") && text.contains("ERRO FINAL") && text.contains("## lint"));
        assert!(text.chars().count() < 7_000);
        // Muitos logs longos: o corte não leva o começo que identifica o ajuste do CI.
        let many: Vec<_> = (0..5).map(|i| (check(&format!("c{i}"), "fail"), Some("y".repeat(10_000)))).collect();
        let text = ci_feedback("https://pr/1", &many);
        assert!(is_ci_feedback(&text) && text.chars().count() <= FEEDBACK_CHARS + 2);
    }
}
