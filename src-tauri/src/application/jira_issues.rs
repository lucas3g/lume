//! Lista as tasks do Jira pelo conector do Atlassian da conta, para escolher a da esteira.
//! Não há acesso direto ao Jira no app: um turno curto do Claude Code roda a busca e devolve JSON.

use super::{
    ports::{AccountRepository, ChatEngine, Clock},
    AppError, AppResult, TurnControl,
};
use crate::domain::{
    account::{pick_next, Provider},
    chat::{TurnEnd, TurnRequest},
    jira::{issues_jql, issues_system_prompt, parse_issue_list, JiraIssueList},
};
use std::sync::Arc;

const LIST_MODEL: &str = "haiku";

/// Só as ferramentas de leitura do conector; o resto fica bloqueado.
pub const JIRA_READ_TOOLS: [&str; 2] =
    ["mcp__claude_ai_Atlassian__getAccessibleAtlassianResources", "mcp__claude_ai_Atlassian__searchJiraIssuesUsingJql"];

pub struct JiraIssues {
    pub engine: Arc<dyn ChatEngine>,
    pub accounts: Arc<dyn AccountRepository>,
    pub clock: Arc<dyn Clock>,
    /// Pasta neutra em que o turno roda.
    pub chat_dir: String,
}

impl JiraIssues {
    /// Tasks em aberto ou do usuário; com `query`, a busca por texto ou chave em todo o Jira.
    pub async fn list(&self, query: Option<&str>) -> AppResult<JiraIssueList> {
        let all: Vec<_> = self.accounts.all()?.into_iter().filter(|a| a.provider == Provider::Claude).collect();
        let account = pick_next(&all, self.clock.now(), &[], true)
            .ok_or_else(|| AppError("Nenhuma conta do Claude disponível para consultar o Jira.".into()))?;
        let request = TurnRequest {
            provider: Provider::Claude,
            config_dir: account.config_dir.clone(),
            cwd: self.chat_dir.clone(),
            model: LIST_MODEL.into(),
            resume: None,
            prompt: "Liste as tasks do Jira.".into(),
            attachments: vec![],
            extra_system: Some(issues_system_prompt(&issues_jql(query))),
            tools: String::new(),
            effort: "low".into(),
            permission_mode: "auto".into(),
            interactive: false,
            denied_skills: vec![],
            denied_tools: vec![],
            mcp_config: None,
            allowed_tools: JIRA_READ_TOOLS.iter().map(|t| (*t).to_owned()).collect(),
            read_dirs: vec![],
        };
        let (mut control, _handle) = TurnControl::new();
        match self.engine.run_turn(&request, &mut control, &mut |_| {}).await {
            TurnEnd::Done { text, .. } => match parse_issue_list(&text) {
                Ok(Some(list)) => Ok(list),
                Ok(None) => Err(AppError("O Jira não respondeu numa forma legível. Tente de novo.".into())),
                Err(reason) => Err(AppError(format!(
                    "Não deu para ler o Jira: {reason}. Confira se o conector do Atlassian está vinculado em Conectores."
                ))),
            },
            TurnEnd::Limited { .. } => Err(AppError("A conta atingiu o limite. Tente de novo em instantes.".into())),
            TurnEnd::AuthError(_) => Err(AppError("A conta precisa de novo login.".into())),
            TurnEnd::Failed(message) => Err(AppError(message)),
            TurnEnd::Cancelled => Err(AppError("Consulta cancelada.".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;
    use crate::domain::account::sample;

    fn service(engine: ScriptedEngine) -> JiraIssues {
        let world = World::with_accounts(vec![sample("a", 1)]);
        JiraIssues { engine: Arc::new(engine), accounts: world.store.clone(), clock: Arc::new(FixedClock(0)), chat_dir: "/tmp".into() }
    }

    #[tokio::test]
    async fn runs_only_the_read_tools_and_reads_the_json() {
        let engine = ScriptedEngine::new(|req, _| {
            assert!(req.tools.is_empty() && !req.interactive);
            assert_eq!(req.allowed_tools, JIRA_READ_TOOLS);
            assert!(req.extra_system.as_deref().unwrap().contains("text ~ \"frete\""));
            let text = r#"{"site":"https://loja.atlassian.net","issues":[{"key":"LOJA-1","summary":"Frete","status":"A fazer","category":"new"}]}"#;
            (vec![], TurnEnd::Done { session_id: None, text: text.into() })
        });
        let list = service(engine).list(Some("frete")).await.unwrap();
        assert_eq!((list.issues.len(), list.issues[0].category.as_str()), (1, "todo"));
    }

    #[tokio::test]
    async fn reports_why_the_jira_could_not_be_read() {
        let engine = ScriptedEngine::new(|_, _| (vec![], TurnEnd::Done { session_id: None, text: r#"{"error":"conector ausente"}"#.into() }));
        let error = service(engine).list(None).await.unwrap_err();
        assert!(error.0.contains("conector ausente") && error.0.contains("Conectores"));
    }
}
