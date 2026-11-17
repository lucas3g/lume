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
use std::{sync::Arc, time::Duration};

const LIST_MODEL: &str = "haiku";

/// Tempo máximo de uma consulta: um conector travado (ex.: esperando login) não prende a lista para sempre.
const LIST_TIMEOUT: Duration = Duration::from_secs(180);

/// Só as ferramentas de leitura do conector ficam liberadas.
pub const JIRA_READ_TOOLS: [&str; 2] =
    ["mcp__claude_ai_Atlassian__getAccessibleAtlassianResources", "mcp__claude_ai_Atlassian__searchJiraIssuesUsingJql"];

/// Ferramentas que alteram o Jira ou o Confluence, bloqueadas mesmo que as configurações da conta as liberem:
/// o texto das tasks entra no turno e não pode levar a uma escrita.
pub const ATLASSIAN_WRITE_TOOLS: [&str; 16] = [
    "mcp__claude_ai_Atlassian__addOrEditJiraIssueComment",
    "mcp__claude_ai_Atlassian__addCommentToJiraIssue",
    "mcp__claude_ai_Atlassian__addWorklogToJiraIssue",
    "mcp__claude_ai_Atlassian__createJiraIssue",
    "mcp__claude_ai_Atlassian__editJiraIssue",
    "mcp__claude_ai_Atlassian__transitionJiraIssue",
    "mcp__claude_ai_Atlassian__createIssueLink",
    "mcp__claude_ai_Atlassian__executeWrite",
    "mcp__claude_ai_Atlassian__executeDestructive",
    "mcp__claude_ai_Atlassian__createConfluenceContent",
    "mcp__claude_ai_Atlassian__updateConfluenceContent",
    "mcp__claude_ai_Atlassian__createConfluencePage",
    "mcp__claude_ai_Atlassian__updateConfluencePage",
    "mcp__claude_ai_Atlassian__createConfluenceFooterComment",
    "mcp__claude_ai_Atlassian__createConfluenceInlineComment",
    "mcp__claude_ai_Atlassian__addGraphContext",
];

pub struct JiraIssues {
    pub engine: Arc<dyn ChatEngine>,
    pub accounts: Arc<dyn AccountRepository>,
    pub clock: Arc<dyn Clock>,
    /// Pasta neutra em que o turno roda.
    pub chat_dir: String,
}

impl JiraIssues {
    /// Tasks em aberto ou do usuário; com `query`, a busca por texto ou chave em todo o Jira.
    /// Usa a conta que atende a próxima mensagem (a mesma da tela de Conectores); no limite ou sem
    /// login, tenta a seguinte.
    pub async fn list(&self, query: Option<&str>) -> AppResult<JiraIssueList> {
        let all: Vec<_> = self.accounts.all()?.into_iter().filter(|a| a.provider == Provider::Claude).collect();
        let (jql, fallback) = issues_jql(query);
        let mut tried: Vec<String> = Vec::new();
        let mut last = AppError("Nenhuma conta do Claude disponível para consultar o Jira.".into());
        while let Some(account) = pick_next(&all, self.clock.now(), &tried, false) {
            tried.push(account.id.clone());
            let request = TurnRequest {
                provider: Provider::Claude,
                config_dir: account.config_dir.clone(),
                cwd: self.chat_dir.clone(),
                model: LIST_MODEL.into(),
                resume: None,
                prompt: "Liste as tasks do Jira.".into(),
                attachments: vec![],
                extra_system: Some(issues_system_prompt(&jql, fallback.as_deref())),
                tools: String::new(),
                effort: "low".into(),
                permission_mode: "auto".into(),
                interactive: false,
                denied_skills: vec![],
                denied_tools: ATLASSIAN_WRITE_TOOLS.iter().map(|t| (*t).to_owned()).collect(),
                mcp_config: None,
                allowed_tools: JIRA_READ_TOOLS.iter().map(|t| (*t).to_owned()).collect(),
                read_dirs: vec![],
                mcp_servers: vec![],
            };
            let (mut control, _handle) = TurnControl::new();
            let cancel = control.cancel.clone();
            let timer = tokio::spawn(async move {
                tokio::time::sleep(LIST_TIMEOUT).await;
                cancel.notify_one();
            });
            let end = self.engine.run_turn(&request, &mut control, &mut |_| {}).await;
            timer.abort();
            match end {
                TurnEnd::Done { text, .. } => {
                    return match parse_issue_list(&text) {
                        Ok(Some(list)) => Ok(JiraIssueList { account_id: Some(account.id.clone()), ..list }),
                        Ok(None) => Err(AppError("O Jira não respondeu numa forma legível. Tente de novo.".into())),
                        Err(reason) => Err(AppError(format!(
                            "Não deu para ler o Jira: {reason}. Confira se o conector do Atlassian está vinculado em Conectores."
                        ))),
                    };
                }
                TurnEnd::Limited { .. } => last = AppError("Todas as contas estão no limite. Tente de novo mais tarde.".into()),
                TurnEnd::AuthError(_) => last = AppError("As contas precisam de novo login.".into()),
                TurnEnd::Failed(message) => return Err(AppError(message)),
                TurnEnd::Cancelled => {
                    return Err(AppError(format!("O Jira não respondeu em {} minutos. Tente de novo.", LIST_TIMEOUT.as_secs() / 60)));
                }
            }
        }
        Err(last)
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
            assert!(req.denied_tools.iter().any(|t| t.ends_with("__editJiraIssue")) && req.denied_tools.iter().any(|t| t.ends_with("__executeWrite")));
            assert!(req.extra_system.as_deref().unwrap().contains("text ~ \"frete\""));
            let text = r#"{"site":"https://loja.atlassian.net","issues":[{"key":"LOJA-1","summary":"Frete","status":"A fazer","category":"new"}]}"#;
            (vec![], TurnEnd::Done { session_id: None, text: text.into() })
        });
        let list = service(engine).list(Some("frete")).await.unwrap();
        assert_eq!((list.issues.len(), list.issues[0].category.as_str()), (1, "todo"));
        assert_eq!(list.account_id.as_deref(), Some("a"), "a lista diz de qual conta veio");
    }

    #[tokio::test]
    async fn a_limited_account_hands_over_to_the_next() {
        let world = World::with_accounts(vec![sample("a", 1), sample("b", 2)]);
        let engine = ScriptedEngine::new(|_, call| match call {
            1 => (vec![], TurnEnd::Limited { resets_at: None }),
            _ => (vec![], TurnEnd::Done { session_id: None, text: r#"{"i":[["LOJA-1","Frete","A fazer",0]]}"#.into() }),
        });
        let uc = JiraIssues { engine: Arc::new(engine), accounts: world.store.clone(), clock: Arc::new(FixedClock(0)), chat_dir: "/tmp".into() };
        let list = uc.list(None).await.unwrap();
        assert_eq!((list.account_id.as_deref(), list.issues.len()), (Some("b"), 1));
    }

    #[tokio::test]
    async fn reports_why_the_jira_could_not_be_read() {
        let engine = ScriptedEngine::new(|_, _| (vec![], TurnEnd::Done { session_id: None, text: r#"{"error":"conector ausente"}"#.into() }));
        let error = service(engine).list(None).await.unwrap_err();
        assert!(error.0.contains("conector ausente") && error.0.contains("Conectores"));
    }
}
