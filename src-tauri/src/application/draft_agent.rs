//! Gera, com o próprio modelo, as instruções e as skills recomendadas de um agente a partir do nome.

use super::{
    ports::{AccountRepository, ChatEngine, Clock, SkillRepository},
    AppError, AppResult, TurnControl,
};
use crate::domain::{
    account::{pick_next, Provider},
    chat::{TurnEnd, TurnRequest},
    extension::{parse_agent_draft, AgentDraft, Skill},
};
use std::sync::Arc;

const DRAFT_MODEL: &str = "sonnet";

pub struct DraftAgent {
    pub engine: Arc<dyn ChatEngine>,
    pub accounts: Arc<dyn AccountRepository>,
    pub skills: Arc<dyn SkillRepository>,
    pub clock: Arc<dyn Clock>,
    /// Pasta neutra em que o turno roda.
    pub chat_dir: String,
}

/// O que o modelo precisa saber: o formato da resposta e as skills que existem.
fn system_prompt(catalog: &[Skill]) -> String {
    let mut list = String::new();
    for skill in catalog {
        list.push_str(&format!("- {}: {}\n", skill.name, skill.description.as_deref().unwrap_or("sem descrição")));
    }
    format!(
        "Você ajuda a configurar agentes de um assistente. O usuário informa só o nome do agente. \
         Responda apenas com um JSON, sem comentários, no formato \
         {{\"instructions\": \"...\", \"skills\": [\"nome\", ...]}}. \
         Em `instructions`, escreva em português do Brasil, em segunda pessoa, de 4 a 8 frases: o papel do agente, \
         como deve trabalhar, o que priorizar, o tom das respostas e o que evitar. \
         Em `skills`, liste só nomes exatos da lista abaixo que ajudem esse agente (pode ser vazia).\n\n\
         Skills disponíveis:\n{list}"
    )
}

impl DraftAgent {
    pub async fn draft(&self, name: &str) -> AppResult<AgentDraft> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError("Dê um nome para o agente antes de gerar.".into()));
        }
        let catalog = self.skills.catalog()?;
        // O rascunho usa um modelo do Claude; contas de outros serviços ficam de fora.
        let all: Vec<_> = self.accounts.all()?.into_iter().filter(|a| a.provider == Provider::Claude).collect();
        let account = pick_next(&all, self.clock.now(), &[], true)
            .ok_or_else(|| AppError("Nenhuma conta do Claude disponível para gerar.".into()))?;
        let request = TurnRequest {
            provider: Provider::Claude,
            config_dir: account.config_dir.clone(),
            cwd: self.chat_dir.clone(),
            model: DRAFT_MODEL.into(),
            resume: None,
            prompt: format!("Nome do agente: {name}"),
            attachments: vec![],
            extra_system: Some(system_prompt(&catalog)),
            tools: String::new(),
            effort: "low".into(),
            permission_mode: "auto".into(),
            interactive: false,
            denied_skills: vec![],
            denied_tools: vec![],
            mcp_config: None,
            allowed_tools: vec![],
            read_dirs: vec![],
            mcp_servers: vec![],
        };
        let (mut control, _handle) = TurnControl::new();
        match self.engine.run_turn(&request, &mut control, &mut |_| {}).await {
            TurnEnd::Done { text, .. } => {
                parse_agent_draft(&text, &catalog).ok_or_else(|| AppError("O modelo não devolveu uma sugestão válida. Tente de novo.".into()))
            }
            TurnEnd::Limited { .. } => Err(AppError("A conta atingiu o limite. Tente com outra conta.".into())),
            TurnEnd::AuthError(_) => Err(AppError("A conta precisa de novo login.".into())),
            TurnEnd::Failed(message) => Err(AppError(message)),
            TurnEnd::Cancelled => Err(AppError("Geração cancelada.".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;
    use crate::domain::account::sample;

    #[tokio::test]
    async fn asks_the_model_with_the_catalog_and_reads_its_json() {
        let world = World::with_accounts(vec![sample("a", 1)]);
        world.skills.remember(&["pdf".into(), "planilha".into()]).unwrap();
        let engine = ScriptedEngine::new(|req, _| {
            assert_eq!(req.prompt, "Nome do agente: Revisor de código");
            assert!(req.extra_system.as_deref().unwrap().contains("- pdf"));
            assert!(req.tools.is_empty() && !req.interactive);
            let text = r#"{"instructions":"Revise com rigor.","skills":["pdf","outra"]}"#;
            (vec![], TurnEnd::Done { session_id: None, text: text.into() })
        });
        let uc = DraftAgent { engine: Arc::new(engine), accounts: world.store.clone(), skills: world.skills.clone(), clock: Arc::new(FixedClock(0)), chat_dir: "/tmp".into() };
        let draft = uc.draft(" Revisor de código ").await.unwrap();
        assert_eq!((draft.instructions.as_str(), draft.skills), ("Revise com rigor.", vec!["pdf".to_owned()]));
        assert!(uc.draft("  ").await.is_err());
    }
}
