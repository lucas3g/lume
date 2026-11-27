//! Lista as tasks do Jira pelo conector do Atlassian da conta, para escolher a da esteira.
//! Não há acesso direto ao Jira no app: um turno curto do Claude Code roda a busca e devolve JSON.

use super::{
    extensions::{known_servers, remember_servers},
    ports::{AccountConnectors, AccountRepository, ChatEngine, Clock, EventSink, Settings},
    send_message::DEFAULT_LIMIT_SECONDS,
    AppError, AppResult, TurnControl,
};
use crate::domain::{
    account::{pick_next, Provider},
    chat::{TurnEnd, TurnRequest},
    extension::{cli_server_key, server_deny_rule},
    jira::{
        boards_system_prompt, brief_system_prompt, issues_jql, issues_system_prompt, parse_boards, parse_brief, parse_issue_list, JiraBoards,
        JiraIssueList, JiraTaskBrief,
    },
    pipeline::normalize_jira_key,
};
use std::{sync::Arc, time::Duration};

const LIST_MODEL: &str = "haiku";

/// Tempo máximo de uma consulta: um conector travado (ex.: esperando login) não prende a lista para sempre.
const LIST_TIMEOUT: Duration = Duration::from_secs(180);

/// Ler a task inteira e entender o pedido pede um modelo melhor e mais tempo que listar.
const BRIEF_MODEL: &str = "sonnet";
const BRIEF_TIMEOUT: Duration = Duration::from_secs(300);

/// Ferramentas de leitura para entender uma task: a task, os comentários e o que ela cita (Confluence, Loom).
pub const JIRA_BRIEF_TOOLS: [&str; 7] = [
    "mcp__claude_ai_Atlassian__getAccessibleAtlassianResources",
    "mcp__claude_ai_Atlassian__getJiraIssue",
    "mcp__claude_ai_Atlassian__listJiraIssueComments",
    "mcp__claude_ai_Atlassian__searchJiraIssuesUsingJql",
    "mcp__claude_ai_Atlassian__getConfluenceContent",
    "mcp__claude_ai_Atlassian__getLoomObjectsContent",
    "mcp__claude_ai_Atlassian__executeRead",
];

/// Como roda um turno de consulta: modelo, esforço, ferramentas liberadas e tempo máximo.
struct Ask<'a> {
    model: &'a str,
    effort: &'a str,
    tools: &'a [&'a str],
    timeout: Duration,
}

const LIST_ASK: Ask<'static> = Ask { model: LIST_MODEL, effort: "low", tools: &JIRA_READ_TOOLS, timeout: LIST_TIMEOUT };

/// Só as ferramentas de leitura do conector ficam liberadas. O `executeRead` roda as operações de
/// leitura (busca com filtro de campos, quadros e colunas); as de escrita ficam em outras ferramentas.
pub const JIRA_READ_TOOLS: [&str; 3] = [
    "mcp__claude_ai_Atlassian__getAccessibleAtlassianResources",
    "mcp__claude_ai_Atlassian__searchJiraIssuesUsingJql",
    "mcp__claude_ai_Atlassian__executeRead",
];

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

/// Servidor do conector do Atlassian da conta, o único que a consulta usa.
const ATLASSIAN_SERVER: &str = "claude.ai Atlassian";

pub struct JiraIssues {
    pub engine: Arc<dyn ChatEngine>,
    pub accounts: Arc<dyn AccountRepository>,
    pub clock: Arc<dyn Clock>,
    pub settings: Arc<dyn Settings>,
    /// Avisa a tela de contas quando uma conta fica no limite ou sem login.
    pub events: Arc<dyn EventSink>,
    /// Para saber quais outros conectores bloquear quando ainda não houve verificação.
    pub account_connectors: Arc<dyn AccountConnectors>,
    /// Pasta neutra em que o turno roda.
    pub chat_dir: String,
}

impl JiraIssues {
    /// Conectores da conta além do Atlassian: os da última verificação ou, sem ela, uma agora.
    async fn other_servers(&self, config_dir: Option<String>) -> Vec<String> {
        let names = match known_servers(&*self.settings) {
            Some(names) => names,
            None => {
                let source = self.account_connectors.clone();
                let loaded = tokio::task::spawn_blocking(move || source.list(config_dir.as_deref())).await;
                match loaded {
                    Ok(Ok(list)) => {
                        remember_servers(&*self.settings, &list);
                        list.into_iter().map(|c| c.name).collect()
                    }
                    _ => vec![],
                }
            }
        };
        let atlassian = cli_server_key(ATLASSIAN_SERVER);
        names.into_iter().filter(|n| cli_server_key(n) != atlassian).collect()
    }

    /// Tasks dos status de uma coluna do quadro (sem status, as em aberto ou do usuário); com `query`,
    /// a busca por texto ou chave em todo o Jira.
    pub async fn list(&self, query: Option<&str>, statuses: &[String]) -> AppResult<JiraIssueList> {
        let (jql, fallback) = issues_jql(query, statuses);
        let (text, account_id) = self.ask("Liste as tasks do Jira.", issues_system_prompt(&jql, fallback.as_deref()), LIST_ASK).await?;
        match parse_issue_list(&text) {
            Ok(Some(list)) => Ok(JiraIssueList { account_id: Some(account_id), ..list }),
            Ok(None) => Err(AppError("O Jira não respondeu numa forma legível. Tente de novo.".into())),
            Err(reason) => Err(unreadable(&reason)),
        }
    }

    /// Quadros do Jira e as colunas de cada um, para escolher de qual coluna vêm as tasks.
    pub async fn boards(&self) -> AppResult<JiraBoards> {
        let (text, account_id) = self.ask("Liste os quadros do Jira.", boards_system_prompt(), LIST_ASK).await?;
        match parse_boards(&text) {
            Ok(Some(boards)) => Ok(JiraBoards { account_id: Some(account_id), ..boards }),
            Ok(None) => Err(AppError("O Jira não respondeu numa forma legível. Tente de novo.".into())),
            Err(reason) => Err(unreadable(&reason)),
        }
    }

    /// Lê a task inteira (descrição, critérios, comentários, anexos e o que ela cita) e escreve o título e
    /// o que fazer, para preencher o formulário da esteira (não salva nada).
    pub async fn brief(&self, key: &str, site: Option<&str>) -> AppResult<JiraTaskBrief> {
        let key = normalize_jira_key(key).ok_or_else(|| AppError("Escolha uma task do Jira antes de gerar.".into()))?;
        let ask = Ask { model: BRIEF_MODEL, effort: "medium", tools: &JIRA_BRIEF_TOOLS, timeout: BRIEF_TIMEOUT };
        let (text, _) = self.ask(&format!("Leia a task {key} e escreva o que fazer."), brief_system_prompt(&key, site), ask).await?;
        match parse_brief(&text) {
            Ok(Some(brief)) => Ok(brief),
            Ok(None) => Err(AppError("O modelo não devolveu um texto para a task. Tente de novo.".into())),
            Err(reason) => Err(unreadable(&reason)),
        }
    }

    /// Roda um turno curto de consulta e devolve a resposta e a conta que respondeu.
    /// Usa a conta que atende a próxima mensagem (a mesma da tela de Conectores); no limite ou sem
    /// login, tenta a seguinte.
    async fn ask(&self, prompt: &str, system: String, how: Ask<'_>) -> AppResult<(String, String)> {
        let all: Vec<_> = self.accounts.all()?.into_iter().filter(|a| a.provider == Provider::Claude).collect();
        let mut tried: Vec<String> = Vec::new();
        let mut last = AppError("Nenhuma conta do Claude disponível para consultar o Jira.".into());
        // O texto das tasks entra no turno: além das escritas do Atlassian, todos os outros
        // conectores ficam bloqueados, mesmo que as configurações da conta os liberem.
        let mut denied: Vec<String> = ATLASSIAN_WRITE_TOOLS.iter().map(|t| (*t).to_owned()).collect();
        denied.extend(
            self.other_servers(pick_next(&all, self.clock.now(), &[], false).and_then(|a| a.config_dir.clone()))
                .await
                .iter()
                .map(|name| server_deny_rule(name)),
        );
        while let Some(account) = pick_next(&all, self.clock.now(), &tried, false) {
            tried.push(account.id.clone());
            let request = TurnRequest {
                provider: Provider::Claude,
                config_dir: account.config_dir.clone(),
                cwd: self.chat_dir.clone(),
                model: how.model.into(),
                resume: None,
                prompt: prompt.into(),
                attachments: vec![],
                extra_system: Some(system.clone()),
                tools: String::new(),
                effort: how.effort.into(),
                permission_mode: "auto".into(),
                interactive: false,
                denied_skills: vec![],
                denied_tools: denied.clone(),
                mcp_config: None,
                allowed_tools: how.tools.iter().map(|t| (*t).to_owned()).collect(),
                read_dirs: vec![],
                mcp_servers: vec![],
                artifacts: false,
                design: false,
                account_connectors: true,
                background_tasks: false,
            };
            let (mut control, _handle) = TurnControl::new();
            let cancel = control.cancel.clone();
            let timeout = how.timeout;
            let timer = tokio::spawn(async move {
                tokio::time::sleep(timeout).await;
                cancel.notify_one();
            });
            let end = self.engine.run_turn(&request, &mut control, &mut |_| {}).await;
            timer.abort();
            match end {
                TurnEnd::Done { text, .. } => return Ok((text, account.id.clone())),
                TurnEnd::Limited { resets_at } => {
                    // Fica marcada: as próximas buscas e as etapas da esteira já começam em outra conta.
                    let until = resets_at.unwrap_or(self.clock.now() + DEFAULT_LIMIT_SECONDS);
                    if self.accounts.set_limited(&account.id, Some(until)).is_ok() {
                        self.events.accounts_changed();
                    }
                    last = AppError("Todas as contas estão no limite. Tente de novo mais tarde.".into());
                }
                TurnEnd::AuthError(_) => {
                    // Como nas conversas: sem login, a conta sai da fila até entrar de novo.
                    if self.accounts.set_auth(&account.id, false).is_ok() {
                        self.events.accounts_changed();
                    }
                    last = AppError("As contas precisam de novo login.".into());
                }
                TurnEnd::Failed(message) => return Err(AppError(message)),
                TurnEnd::Cancelled => {
                    return Err(AppError(format!("O Jira não respondeu em {} minutos. Tente de novo.", how.timeout.as_secs() / 60)));
                }
            }
        }
        Err(last)
    }
}

fn unreadable(reason: &str) -> AppError {
    AppError(format!("Não deu para ler o Jira: {reason}. Confira se o conector do Atlassian está vinculado em Conectores."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;
    use crate::domain::account::sample;

    fn service(engine: ScriptedEngine) -> JiraIssues {
        let world = World::with_accounts(vec![sample("a", 1)]);
        JiraIssues {
            engine: Arc::new(engine),
            accounts: world.store.clone(),
            clock: Arc::new(FixedClock(0)),
            settings: world.store.clone(),
            events: world.events.clone(),
            account_connectors: Arc::new(FakeAccountConnectors::default()),
            chat_dir: "/tmp".into(),
        }
    }

    #[tokio::test]
    async fn runs_only_the_read_tools_and_reads_the_json() {
        let engine = ScriptedEngine::new(|req, _| {
            assert!(req.tools.is_empty() && !req.interactive);
            assert_eq!(req.allowed_tools, JIRA_READ_TOOLS);
            assert!(req.denied_tools.iter().any(|t| t.ends_with("__editJiraIssue")) && req.denied_tools.iter().any(|t| t.ends_with("__executeWrite")));
            // Os outros conectores da conta ficam bloqueados; o Atlassian, não.
            assert!(req.denied_tools.contains(&"mcp__claude_ai_Asana".to_owned()) && req.denied_tools.contains(&"mcp__rea".to_owned()));
            assert!(!req.denied_tools.contains(&"mcp__claude_ai_Atlassian".to_owned()));
            assert!(req.extra_system.as_deref().unwrap().contains("text ~ \"frete\""));
            let text = r#"{"site":"https://loja.atlassian.net","issues":[{"key":"LOJA-1","summary":"Frete","status":"A fazer","category":"new"}]}"#;
            (vec![], TurnEnd::Done { session_id: None, text: text.into() })
        });
        let list = service(engine).list(Some("frete"), &[]).await.unwrap();
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
        let uc = JiraIssues {
            engine: Arc::new(engine),
            accounts: world.store.clone(),
            clock: Arc::new(FixedClock(0)),
            settings: world.store.clone(),
            events: world.events.clone(),
            account_connectors: Arc::new(FakeAccountConnectors::default()),
            chat_dir: "/tmp".into(),
        };
        let list = uc.list(None, &[]).await.unwrap();
        assert_eq!((list.account_id.as_deref(), list.issues.len()), (Some("b"), 1));
        let a = world.store.all().unwrap().into_iter().find(|a| a.id == "a").unwrap();
        assert_eq!(a.limited_until, Some(3600), "a conta no limite fica marcada");
    }

    #[tokio::test]
    async fn an_account_without_login_is_marked_and_skipped() {
        let world = World::with_accounts(vec![sample("a", 1), sample("b", 2)]);
        let engine = ScriptedEngine::new(|_, call| match call {
            1 => (vec![], TurnEnd::AuthError("Please run /login".into())),
            _ => (vec![], TurnEnd::Done { session_id: None, text: r#"{"i":[["LOJA-1","Frete","A fazer",0]]}"#.into() }),
        });
        let uc = JiraIssues {
            engine: Arc::new(engine),
            accounts: world.store.clone(),
            clock: Arc::new(FixedClock(0)),
            settings: world.store.clone(),
            events: world.events.clone(),
            account_connectors: Arc::new(FakeAccountConnectors::default()),
            chat_dir: "/tmp".into(),
        };
        assert_eq!(uc.list(None, &[]).await.unwrap().account_id.as_deref(), Some("b"));
        let a = world.store.all().unwrap().into_iter().find(|a| a.id == "a").unwrap();
        assert!(!a.auth_ok, "a conta sem login sai da fila");
    }

    #[tokio::test]
    async fn reports_why_the_jira_could_not_be_read() {
        let engine = ScriptedEngine::new(|_, _| (vec![], TurnEnd::Done { session_id: None, text: r#"{"error":"conector ausente"}"#.into() }));
        let error = service(engine).list(None, &[]).await.unwrap_err();
        assert!(error.0.contains("conector ausente") && error.0.contains("Conectores"));
    }

    #[tokio::test]
    async fn briefs_a_task_with_the_read_tools_only() {
        let engine = ScriptedEngine::new(|req, _| {
            assert_eq!(req.model, BRIEF_MODEL);
            assert_eq!(req.allowed_tools, JIRA_BRIEF_TOOLS);
            assert!(req.denied_tools.iter().any(|t| t.ends_with("__editJiraIssue")));
            assert!(req.extra_system.as_deref().unwrap().contains("LOJA-7"));
            let text = r#"{"title":"Exportar CSV","request":"Exportar a lista em CSV."}"#;
            (vec![], TurnEnd::Done { session_id: None, text: text.into() })
        });
        let uc = service(engine);
        let brief = uc.brief("loja-7", None).await.unwrap();
        assert_eq!((brief.title.as_deref(), brief.request.as_str()), (Some("Exportar CSV"), "Exportar a lista em CSV."));
        assert!(uc.brief("não é chave", None).await.is_err());
    }

    #[tokio::test]
    async fn reads_the_boards_with_the_read_tool() {
        let engine = ScriptedEngine::new(|req, _| {
            assert!(req.allowed_tools.iter().any(|t| t.ends_with("__executeRead")));
            assert!(req.extra_system.as_deref().unwrap().contains("getJiraBoardConfig"));
            let text = r#"{"site":"https://loja.atlassian.net","b":[[1,"Principal","LOJA",[["Em andamento",["3"],"indeterminate"]]]]}"#;
            (vec![], TurnEnd::Done { session_id: None, text: text.into() })
        });
        let boards = service(engine).boards().await.unwrap();
        assert_eq!((boards.account_id.as_deref(), boards.boards[0].columns[0].statuses.as_slice()), (Some("a"), ["3".to_owned()].as_slice()));
    }

    #[tokio::test]
    #[ignore = "consome uso da conta real e lê o Jira do conector"]
    async fn real_connector_reads_boards_and_one_column() {
        let cli = crate::infrastructure::claude_cli::find_cli().expect("Claude Code instalado");
        let world = World::with_accounts(vec![sample("maquina", 1)]);
        let uc = JiraIssues {
            engine: Arc::new(crate::infrastructure::claude_cli::engine::ClaudeCliEngine::new(cli)),
            accounts: world.store.clone(),
            clock: Arc::new(FixedClock(0)),
            settings: world.store.clone(),
            events: world.events.clone(),
            account_connectors: Arc::new(FakeAccountConnectors::default()),
            chat_dir: std::env::temp_dir().to_string_lossy().into_owned(),
        };
        let boards = uc.boards().await.unwrap();
        eprintln!("{boards:#?}");
        let column = boards.boards.iter().flat_map(|b| &b.columns).find(|c| c.category == "doing").expect("uma coluna em andamento");
        let list = uc.list(None, &column.statuses).await.unwrap();
        eprintln!("{} tasks em {}: {:?}", list.issues.len(), column.name, list.issues.iter().map(|i| (&i.key, &i.status)).collect::<Vec<_>>());
        assert!(!list.issues.is_empty());
    }
}
