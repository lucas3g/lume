//! O que as execuções da esteira de um projeto mostram: quanto o trabalho volta, quanto cada
//! etapa leva, o que o code review costuma achar e o resumo das devoluções para gerar lições.

use super::chat::TokenUsage;
use super::ci::is_ci_feedback;
use super::pipeline::{parse_findings, PipelineRun, Role, RunStatus, StageRun, StageStatus};
use serde::Serialize;
use std::collections::BTreeMap;

/// Execuções mais recentes que entram no resumo para as lições.
const DIGEST_RUNS: usize = 15;
/// Tamanho máximo de cada trecho e do resumo inteiro, em caracteres.
const DIGEST_ITEM_CHARS: usize = 1_200;
const DIGEST_CHARS: usize = 24_000;

/// Tempo médio de uma etapa de bot.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoleTime {
    pub role: Role,
    pub passes: i64,
    pub avg_seconds: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PipelineMetrics {
    pub runs: i64,
    pub done: i64,
    pub failed: i64,
    pub cancelled: i64,
    pub needs_info: i64,
    /// Concluídas sem nenhuma devolução do Revisor nem do Testador.
    pub first_pass: i64,
    /// Devoluções do Revisor e do Testador, somadas em todas as execuções.
    pub reviewer_returns: i64,
    pub tester_returns: i64,
    /// Execuções reabertas por você com um ajuste depois de concluídas.
    pub reopened: i64,
    /// Correções automáticas do CI do pull request.
    pub ci_fixes: i64,
    pub roles: Vec<RoleTime>,
    /// Achados do code review por gravidade ("alta", "média", "baixa"; "" sem gravidade).
    pub findings: BTreeMap<String, i64>,
    pub usage: TokenUsage,
}

/// Métricas das execuções (cada uma com as etapas e o gasto delas).
pub fn metrics(runs: &[(PipelineRun, Vec<StageRun>, TokenUsage)]) -> PipelineMetrics {
    let mut m = PipelineMetrics { runs: runs.len() as i64, ..Default::default() };
    let mut times: BTreeMap<Role, (i64, i64)> = BTreeMap::new();
    for (run, stages, usage) in runs {
        m.usage.add(usage);
        match run.status {
            RunStatus::Done => m.done += 1,
            RunStatus::Failed => m.failed += 1,
            RunStatus::Cancelled => m.cancelled += 1,
            RunStatus::NeedsInfo => m.needs_info += 1,
            _ => {}
        }
        let returned = |role| stages.iter().filter(|s| s.role == role && s.status == StageStatus::Returned).count() as i64;
        let (reviewer, tester) = (returned(Role::Reviewer), returned(Role::Tester));
        m.reviewer_returns += reviewer;
        m.tester_returns += tester;
        if run.status == RunStatus::Done && reviewer + tester == 0 {
            m.first_pass += 1;
        }
        let from_ci = |s: &StageRun| s.output.as_deref().is_some_and(is_ci_feedback);
        if stages.iter().any(|s| s.role == Role::Feedback && !from_ci(s)) {
            m.reopened += 1;
        }
        m.ci_fixes += stages.iter().filter(|s| s.role == Role::Feedback && from_ci(s)).count() as i64;
        for s in stages.iter().filter(|s| s.role.is_bot() && matches!(s.status, StageStatus::Done | StageStatus::Returned)) {
            if let Some(end) = s.finished_at {
                let entry = times.entry(s.role).or_default();
                entry.0 += 1;
                entry.1 += (end - s.started_at).max(0);
            }
        }
        for s in stages.iter().filter(|s| s.role == Role::CodeReview && s.status == StageStatus::Done) {
            for f in s.output.as_deref().and_then(parse_findings).unwrap_or_default() {
                *m.findings.entry(f.severity.unwrap_or_default().to_lowercase()).or_default() += 1;
            }
        }
    }
    m.roles = Role::ALL
        .iter()
        .filter_map(|role| times.get(role).map(|(passes, total)| RoleTime { role: *role, passes: *passes, avg_seconds: total / passes }))
        .collect();
    m
}

fn clip(text: &str, max: usize) -> String {
    let text = text.trim();
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_owned(),
    }
}

/// Resumo do que voltou nas execuções mais recentes (devoluções do Revisor e do Testador,
/// correções escolhidas no code review e ajustes pedidos ao reabrir), para o modelo tirar
/// lições. Vazio quando nada voltou.
pub fn lessons_digest(runs: &[(PipelineRun, Vec<StageRun>)]) -> String {
    let mut out = String::new();
    let mut recent: Vec<&(PipelineRun, Vec<StageRun>)> = runs.iter().collect();
    recent.sort_by_key(|r| std::cmp::Reverse(r.0.created_at));
    for (run, stages) in recent.into_iter().take(DIGEST_RUNS) {
        let items: Vec<String> = stages
            .iter()
            .filter_map(|s| {
                let label = match (s.role, s.status) {
                    (Role::Reviewer, StageStatus::Returned) => "Revisor devolveu",
                    (Role::Tester, StageStatus::Returned) => "Testador devolveu",
                    (Role::ReviewChoice, StageStatus::Returned) => "Correções escolhidas no code review",
                    (Role::Feedback, _) if s.output.as_deref().is_some_and(is_ci_feedback) => "CI do pull request falhou depois da entrega",
                    (Role::Feedback, _) => "Ajuste pedido pelo usuário depois de concluída",
                    _ => return None,
                };
                Some(format!("- {label}: {}", clip(s.output.as_deref()?, DIGEST_ITEM_CHARS)))
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        let section = format!("## {}\n{}\n\n", run.title, items.join("\n"));
        if out.chars().count() + section.chars().count() > DIGEST_CHARS {
            break;
        }
        out.push_str(&section);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::pipeline::WorkMode;

    fn run(id: &str, status: RunStatus, created_at: i64) -> PipelineRun {
        PipelineRun {
            id: id.into(),
            project_id: "p".into(),
            jira_key: None,
            title: format!("Pedido {id}"),
            request: String::new(),
            status,
            mode: WorkMode::Direct,
            workdir: "/tmp".into(),
            branch: None,
            base_branch: None,
            base_commit: None,
            delivered: None,
            rounds: 0,
            skipped: vec![],
            agents: Default::default(),
            image_count: 0,
            jira_template: None,
            jira_url: None,
            efforts: Default::default(),
            budget_minutes: 60,
            design: false,
            create_jira: false,
            note: None,
            created_at,
            updated_at: created_at,
        }
    }

    fn stage(role: Role, status: StageStatus, started: i64, finished: i64, output: &str) -> StageRun {
        StageRun {
            id: format!("{}-{started}", role.key()),
            run_id: "r".into(),
            role,
            round: 0,
            status,
            conversation_id: String::new(),
            input: String::new(),
            output: Some(output.into()),
            started_at: started,
            finished_at: Some(finished),
        }
    }

    #[test]
    fn counts_returns_first_pass_times_and_findings() {
        let review = "ok\n```json\n[{\"id\":\"1\",\"title\":\"a\",\"severity\":\"Alta\",\"detail\":\"x\"},{\"id\":\"2\",\"title\":\"b\",\"severity\":\"baixa\",\"detail\":\"y\"}]\n```";
        let runs = vec![
            (run("1", RunStatus::Done, 1), vec![stage(Role::Implementer, StageStatus::Done, 0, 100, "feito")], TokenUsage { input: 5, ..Default::default() }),
            (
                run("2", RunStatus::Done, 2),
                vec![
                    stage(Role::Implementer, StageStatus::Done, 0, 300, "feito"),
                    stage(Role::Reviewer, StageStatus::Returned, 300, 360, "faltou teste"),
                    stage(Role::CodeReview, StageStatus::Done, 400, 500, review),
                    stage(Role::Feedback, StageStatus::Done, 600, 600, "quebrou"),
                    stage(Role::Feedback, StageStatus::Done, 700, 700, "O CI do pull request https://pr/1 falhou depois da entrega."),
                ],
                TokenUsage { input: 7, ..Default::default() },
            ),
            (run("3", RunStatus::Failed, 3), vec![], TokenUsage::default()),
        ];
        let m = metrics(&runs);
        assert_eq!((m.runs, m.done, m.failed, m.first_pass, m.reviewer_returns, m.reopened, m.ci_fixes), (3, 2, 1, 1, 1, 1, 1));
        assert_eq!(m.roles[0], RoleTime { role: Role::Implementer, passes: 2, avg_seconds: 200 });
        assert_eq!(m.findings, BTreeMap::from([("alta".into(), 1), ("baixa".into(), 1)]));
        assert_eq!(m.usage.input, 12);
    }

    #[test]
    fn digest_lists_what_came_back_newest_first() {
        let runs = vec![
            (run("velho", RunStatus::Done, 1), vec![stage(Role::Tester, StageStatus::Returned, 0, 1, "teste falhou")]),
            (run("novo", RunStatus::Done, 2), vec![stage(Role::Reviewer, StageStatus::Returned, 0, 1, &"x".repeat(5000))]),
            (run("limpo", RunStatus::Done, 3), vec![stage(Role::Reviewer, StageStatus::Done, 0, 1, "aprovado")]),
        ];
        let digest = lessons_digest(&runs);
        assert!(digest.find("Pedido novo").unwrap() < digest.find("Pedido velho").unwrap());
        assert!(!digest.contains("Pedido limpo") && digest.contains("Testador devolveu: teste falhou"));
        assert!(digest.chars().count() < 1_500, "cada trecho é cortado");
    }
}
