//! Métricas da esteira de um projeto e as lições tiradas do que os bots erraram, para gravar no
//! conhecimento do projeto (todas as sessões dele, inclusive o Implementador, passam a lê-las).

use super::{
    draft_agent::{one_shot, OneShot},
    ports::{AccountRepository, ChatEngine, Clock, PipelineRepository},
    AppError, AppResult,
};
use crate::domain::{
    chat::TokenUsage,
    pipeline::{PipelineRun, StageRun},
    pipeline_metrics::{lessons_digest, metrics, PipelineMetrics},
};
use std::sync::Arc;

/// Arquivo de conhecimento do projeto com as lições da esteira.
pub const LESSONS_FILE: &str = "licoes-da-esteira.md";

pub struct PipelineInsights {
    pub pipelines: Arc<dyn PipelineRepository>,
    pub engine: Arc<dyn ChatEngine>,
    pub accounts: Arc<dyn AccountRepository>,
    pub clock: Arc<dyn Clock>,
    /// Pasta neutra em que o pedido ao modelo roda.
    pub chat_dir: String,
}

const SYSTEM: &str = "Você analisa o histórico de uma esteira de bots que programa num projeto de software: o que o \
Revisor e o Testador devolveram ao Implementador, as correções escolhidas no code review e os ajustes que o usuário \
pediu depois de entregue. Escreva, em português do Brasil, de 4 a 10 lições para o Implementador não repetir esses erros \
neste projeto. Cada lição é um item de lista em Markdown, no imperativo, com uma frase curta e concreta (o que fazer e \
onde, citando arquivos, comandos ou convenções quando o histórico mostrar). Ignore problemas pontuais que não se repetem \
e não invente nada que o histórico não mostre. Se houver lições atuais, mantenha as que continuam valendo, junte as \
repetidas e acrescente as novas. Responda só com a lista, sem título nem comentário.";

impl PipelineInsights {
    fn project_runs(&self, project_id: &str) -> AppResult<Vec<(PipelineRun, Vec<StageRun>)>> {
        self.pipelines
            .runs()?
            .into_iter()
            .filter(|r| r.project_id == project_id)
            .map(|r| {
                let stages = self.pipelines.stages(&r.id)?;
                Ok((r, stages))
            })
            .collect()
    }

    pub fn metrics(&self, project_id: &str) -> AppResult<PipelineMetrics> {
        let runs = self
            .project_runs(project_id)?
            .into_iter()
            .map(|(run, stages)| {
                let usage = self.pipelines.stage_usage(&run.id)?.values().fold(TokenUsage::default(), |mut sum, u| {
                    sum.add(u);
                    sum
                });
                Ok((run, stages, usage))
            })
            .collect::<AppResult<Vec<_>>>()?;
        Ok(metrics(&runs))
    }

    /// Lições tiradas das devoluções das execuções recentes, juntando as `current` que continuam valendo.
    pub async fn draft_lessons(&self, project_id: &str, current: Option<&str>) -> AppResult<String> {
        let digest = lessons_digest(&self.project_runs(project_id)?);
        if digest.is_empty() {
            return Err(AppError("Nenhuma execução deste projeto teve trabalho devolvido ainda: não há do que tirar lições.".into()));
        }
        let mut prompt = format!("Histórico das execuções recentes:\n\n{digest}");
        if let Some(current) = current.map(str::trim).filter(|c| !c.is_empty()) {
            prompt.push_str(&format!("\nLições atuais:\n{current}\n"));
        }
        let ctx = OneShot { engine: &*self.engine, accounts: &*self.accounts, clock: &*self.clock, chat_dir: &self.chat_dir };
        let text = one_shot(ctx, prompt, vec![], SYSTEM.into(), false).await?;
        let text = text.trim();
        if text.is_empty() {
            return Err(AppError("O modelo não devolveu lições. Tente de novo.".into()));
        }
        Ok(text.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;
    use crate::domain::{
        account::sample,
        chat::TurnEnd,
        pipeline::{Role, RunStatus, StageStatus, WorkMode},
    };

    fn insights(world: &World, engine: ScriptedEngine) -> PipelineInsights {
        PipelineInsights { pipelines: world.store.clone(), engine: Arc::new(engine), accounts: world.store.clone(), clock: Arc::new(FixedClock(0)), chat_dir: "/tmp".into() }
    }

    fn save_run(world: &World, returned: bool) {
        let run = PipelineRun {
            id: "r1".into(), project_id: "p1".into(), jira_key: None, title: "Botão".into(), request: String::new(),
            status: RunStatus::Done, mode: WorkMode::Direct, workdir: "/tmp".into(), branch: None, base_branch: None,
            base_commit: None, delivered: None, rounds: 0, skipped: vec![], agents: Default::default(), image_count: 0,
            jira_template: None, jira_url: None, efforts: Default::default(), budget_minutes: 60, design: false,
            create_jira: false, note: None, created_at: 1, updated_at: 1,
        };
        world.store.save_run(&run).unwrap();
        let stage = StageRun {
            id: "s1".into(), run_id: "r1".into(), role: Role::Reviewer, round: 0,
            status: if returned { StageStatus::Returned } else { StageStatus::Done },
            conversation_id: String::new(), input: String::new(), output: Some("faltou rodar o pnpm check".into()),
            started_at: 0, finished_at: Some(10),
        };
        world.store.save_stage(&stage).unwrap();
    }

    #[tokio::test]
    async fn drafts_lessons_from_what_came_back_and_keeps_the_current_ones() {
        let world = World::with_accounts(vec![sample("a", 1)]);
        save_run(&world, true);
        let engine = ScriptedEngine::new(|req, _| {
            assert!(req.prompt.contains("Revisor devolveu: faltou rodar o pnpm check"));
            assert!(req.prompt.contains("- use pt-BR"), "as lições atuais vão junto");
            assert!(req.tools.is_empty());
            (vec![], TurnEnd::Done { session_id: None, text: "\n- Rode o pnpm check antes de entregar.\n".into() })
        });
        let uc = insights(&world, engine);
        assert_eq!(uc.draft_lessons("p1", Some("- use pt-BR")).await.unwrap(), "- Rode o pnpm check antes de entregar.");
        assert_eq!(uc.metrics("p1").unwrap().reviewer_returns, 1);
        assert_eq!(uc.metrics("outro").unwrap().runs, 0);
    }

    #[tokio::test]
    async fn nothing_came_back_means_nothing_to_learn() {
        let world = World::with_accounts(vec![sample("a", 1)]);
        save_run(&world, false);
        let uc = insights(&world, ScriptedEngine::new(|_, _| panic!("não chama o modelo")));
        assert!(uc.draft_lessons("p1", None).await.is_err());
    }
}
