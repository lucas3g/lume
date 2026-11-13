//! Caso de uso central: enviar uma mensagem e, se a conta atingir o limite,
//! continuar a mesma conversa na próxima conta disponível.

use super::{
    flags,
    ports::{
        AccountRepository, ChatEngine, Clock, ConnectorRepository, ConversationRepository, EventSink, FileLoader,
        Settings, SkillRepository, SwitchLog,
    },
    turn_setup::PrepareTurn,
    AppError, AppResult, ChatEvent, TurnControl,
};
use crate::domain::{
    account::{earliest_reset, pick_next, Account},
    workspace::context_percent,
    chat::{
        tool_message, Attachment, Conversation, PermissionDecision, TurnEnd, TurnEvent, TurnRequest, ROLE_ASSISTANT, ROLE_NOTICE,
        ROLE_TOOL, ROLE_USER,
    },
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

const TITLE_MAX_CHARS: usize = 48;
/// Usado quando o provedor não informa o horário do reset.
const DEFAULT_LIMIT_SECONDS: i64 = 3600;

pub struct SendMessage {
    pub accounts: Arc<dyn AccountRepository>,
    pub conversations: Arc<dyn ConversationRepository>,
    pub switches: Arc<dyn SwitchLog>,
    pub settings: Arc<dyn Settings>,
    pub engine: Arc<dyn ChatEngine>,
    pub prepare: PrepareTurn,
    pub skills: Arc<dyn SkillRepository>,
    pub connectors: Arc<dyn ConnectorRepository>,
    pub files: Arc<dyn FileLoader>,
    pub events: Arc<dyn EventSink>,
    pub clock: Arc<dyn Clock>,
}

/// O que o turno produziu e ainda não foi gravado na conversa.
#[derive(Default)]
struct Journal {
    /// Texto do assistente desde o último registro.
    segment: String,
    /// Chamadas de ferramenta aguardando resultado: id → (nome, argumentos).
    calls: HashMap<String, (String, String)>,
    /// Algo deste turno já foi gravado.
    recorded: bool,
}

impl Journal {
    /// Texto final a gravar: o trecho em aberto ou, se nada foi transmitido, a resposta inteira.
    fn closing_text(&mut self, full_text: String) -> String {
        let segment = std::mem::take(&mut self.segment);
        if !segment.trim().is_empty() || self.recorded {
            segment
        } else {
            full_text
        }
    }
}

/// Por que a conta anterior deixou de atender.
enum Handoff {
    Limit,
    Auth,
    /// Está quase no limite: poupada enquanto outra conta estiver livre.
    NearLimit,
}

impl SendMessage {
    /// `attachment_paths`: arquivos escolhidos pelo usuário; `pasted`: anexos que já
    /// chegaram com conteúdo (ex.: imagens coladas no campo de mensagem).
    pub async fn execute(
        &self,
        conversation_id: &str,
        text: &str,
        attachment_paths: &[String],
        pasted: Vec<Attachment>,
        mut control: TurnControl,
    ) -> AppResult<()> {
        let conversation = self
            .conversations
            .get(conversation_id)?
            .ok_or_else(|| AppError("Conversa não encontrada.".into()))?;
        let mut attachments = attachment_paths.iter().map(|p| self.files.load(p)).collect::<AppResult<Vec<_>>>()?;
        attachments.extend(pasted);
        let names: Vec<&str> = attachments.iter().map(|a| a.name.as_str()).collect();
        let title = self.store_user_message(&conversation, text, &names)?;
        let setup = self.prepare.prepare(&conversation, text)?;
        let context_meta = self.announce_context(conversation_id, &setup.context_titles);

        let mut tried: Vec<String> = vec![];
        let mut previous: Option<(Account, Handoff)> = None;

        loop {
            let all = self.accounts.all()?;
            let now = self.clock.now();
            let spare = self.settings.flag(flags::SWITCH_EARLY, true);
            let Some(account) = pick_next(&all, now, &tried, spare).cloned() else {
                self.emit(conversation_id, ChatEvent::Exhausted { resets_at: earliest_reset(&all, now) });
                return Ok(());
            };
            // Na primeira tentativa, avisa se a conta da vez foi poupada por estar quase no limite.
            let spared = pick_next(&all, now, &tried, false).filter(|a| a.id != account.id).cloned();
            match (previous.take(), spared) {
                (Some((from, reason)), _) => self.announce_switch(conversation_id, &title, &from, &account, reason)?,
                (None, Some(from)) if tried.is_empty() => {
                    self.announce_switch(conversation_id, &title, &from, &account, Handoff::NearLimit)?
                }
                _ => {}
            }
            self.emit(conversation_id, ChatEvent::Account { id: account.id.clone(), name: account.name.clone() });
            tried.push(account.id.clone());

            let request = TurnRequest {
                config_dir: account.config_dir.clone(),
                cwd: conversation.cwd.clone(),
                model: setup.model.clone(),
                resume: conversation.cli_session_id.clone(),
                prompt: text.to_owned(),
                attachments: attachments.clone(),
                extra_system: setup.extra_system.clone(),
                tools: setup.tools.clone(),
                effort: setup.effort.clone(),
                permission_mode: setup.permission_mode.clone(),
                interactive: setup.interactive,
                denied_skills: setup.denied_skills.clone(),
                denied_tools: setup.denied_tools.clone(),
                mcp_config: setup.mcp_config.clone(),
                allowed_tools: setup.allowed_tools.clone(),
            };
            let mut journal = Journal::default();
            let answer = control.answer.clone();
            let end = self
                .engine
                .run_turn(&request, &mut control, &mut |event| {
                    self.forward(conversation_id, &account.id, &mut journal, &answer, event)
                })
                .await;

            match end {
                TurnEnd::Done { session_id, text } => {
                    self.record_unanswered_calls(conversation_id, &account.id, &mut journal);
                    let text = journal.closing_text(text);
                    if !text.trim().is_empty() {
                        self.conversations.add_message(
                            conversation_id,
                            ROLE_ASSISTANT,
                            &text,
                            Some(&account.id),
                            context_meta.as_deref(),
                        )?;
                    }
                    self.conversations.update(conversation_id, None, session_id.as_deref(), None)?;
                    self.emit(conversation_id, ChatEvent::Done { text });
                    return Ok(());
                }
                TurnEnd::Limited { resets_at } => {
                    let until = resets_at.unwrap_or(now + DEFAULT_LIMIT_SECONDS);
                    self.accounts.set_limited(&account.id, Some(until))?;
                    self.events.accounts_changed();
                    self.emit(conversation_id, ChatEvent::Reset);
                    if !self.settings.flag(flags::AUTO_SWITCH, true) {
                        let message = format!(
                            "A conta {} atingiu o limite e a troca automática está desligada.",
                            account.name
                        );
                        self.emit(conversation_id, ChatEvent::Error { message });
                        return Ok(());
                    }
                    previous = Some((account, Handoff::Limit));
                }
                TurnEnd::AuthError(_) => {
                    self.accounts.set_auth(&account.id, false)?;
                    self.events.accounts_changed();
                    self.emit(conversation_id, ChatEvent::Reset);
                    previous = Some((account, Handoff::Auth));
                }
                TurnEnd::Failed(message) => {
                    self.emit(conversation_id, ChatEvent::Error { message });
                    return Ok(());
                }
                TurnEnd::Cancelled => {
                    self.record_segment(conversation_id, &account.id, &mut journal);
                    self.emit(conversation_id, ChatEvent::Cancelled);
                    return Ok(());
                }
            }
        }
    }

    fn emit(&self, conversation_id: &str, event: ChatEvent) {
        self.events.chat(conversation_id, event);
    }

    /// Grava a mensagem do usuário e dá título à conversa na primeira mensagem.
    fn store_user_message(&self, conversation: &Conversation, text: &str, attachments: &[&str]) -> AppResult<String> {
        let first = self.conversations.message_count(&conversation.id)? == 0;
        let meta = (!attachments.is_empty()).then(|| serde_json::json!({ "attachments": attachments }).to_string());
        self.conversations.add_message(&conversation.id, ROLE_USER, text, None, meta.as_deref())?;
        if !first {
            return Ok(conversation.title.clone());
        }
        let title = make_title(text);
        self.conversations.update(&conversation.id, Some(&title), None, None)?;
        self.emit(&conversation.id, ChatEvent::Title { title: title.clone() });
        Ok(title)
    }

    /// Informa quais conversas anteriores entraram como contexto; devolve o registro para a mensagem.
    fn announce_context(&self, conversation_id: &str, titles: &[String]) -> Option<String> {
        if titles.is_empty() {
            return None;
        }
        self.emit(conversation_id, ChatEvent::Context { titles: titles.to_vec() });
        Some(serde_json::json!({ "context": titles }).to_string())
    }

    fn announce_switch(
        &self,
        conversation_id: &str,
        title: &str,
        from: &Account,
        to: &Account,
        reason: Handoff,
    ) -> AppResult<()> {
        self.switches.record(&from.name, &to.name, title)?;
        if !self.settings.flag(flags::NOTIFY_SWITCH, true) {
            return Ok(());
        }
        let text = match reason {
            Handoff::Limit => format!("Limite da conta {} atingido · continuando com {}", from.name, to.name),
            Handoff::Auth => format!("A conta {} precisa de novo login · continuando com {}", from.name, to.name),
            Handoff::NearLimit => format!("A conta {} está quase no limite · usando {}", from.name, to.name),
        };
        self.conversations.add_message(conversation_id, ROLE_NOTICE, &text, None, None)?;
        self.emit(conversation_id, ChatEvent::Notice { text });
        Ok(())
    }

    /// Grava o texto transmitido até aqui como uma mensagem do assistente.
    fn record_segment(&self, conversation_id: &str, account_id: &str, journal: &mut Journal) {
        let segment = std::mem::take(&mut journal.segment);
        if segment.trim().is_empty() {
            return;
        }
        if self.conversations.add_message(conversation_id, ROLE_ASSISTANT, &segment, Some(account_id), None).is_ok() {
            journal.recorded = true;
            self.emit(conversation_id, ChatEvent::Step);
        }
    }

    fn record_tool(&self, conversation_id: &str, account_id: &str, journal: &mut Journal, content: &str) {
        if self.conversations.add_message(conversation_id, ROLE_TOOL, content, Some(account_id), None).is_ok() {
            journal.recorded = true;
            self.emit(conversation_id, ChatEvent::Step);
        }
    }

    /// Chamadas que terminaram sem resultado (ex.: não autorizadas) também ficam na conversa.
    fn record_unanswered_calls(&self, conversation_id: &str, account_id: &str, journal: &mut Journal) {
        for (name, input) in std::mem::take(&mut journal.calls).into_values() {
            self.record_tool(conversation_id, account_id, journal, &tool_message(&name, &input, None, false));
        }
    }

    fn is_tool_allowed(&self, conversation_id: &str, tool: &str) -> bool {
        matches!(self.conversations.get(conversation_id), Ok(Some(c)) if c.allowed_tools.iter().any(|t| t == tool))
    }

    /// Repassa à interface o que acontece durante o turno, grava o uso de
    /// ferramentas na conversa e registra o consumo da conta.
    fn forward(
        &self,
        conversation_id: &str,
        account_id: &str,
        journal: &mut Journal,
        answer: &UnboundedSender<PermissionDecision>,
        event: &TurnEvent,
    ) {
        match event {
            TurnEvent::Delta(text) => {
                journal.segment.push_str(text);
                self.emit(conversation_id, ChatEvent::Delta { text: text.clone() });
            }
            TurnEvent::ToolCall { id, name, input } => {
                self.record_segment(conversation_id, account_id, journal);
                journal.calls.insert(id.clone(), (name.clone(), input.clone()));
            }
            TurnEvent::ToolResult { id, output, is_error } => {
                if let Some((name, input)) = journal.calls.remove(id) {
                    let content = tool_message(&name, &input, Some(output), *is_error);
                    self.record_tool(conversation_id, account_id, journal, &content);
                }
            }
            TurnEvent::Permission { request_id, tool, input, description } => {
                if self.is_tool_allowed(conversation_id, tool) {
                    let _ = answer.send(PermissionDecision::new(request_id, true));
                } else {
                    let event = ChatEvent::Permission {
                        request_id: request_id.clone(),
                        tool: tool.clone(),
                        input: input.clone(),
                        description: description.clone(),
                    };
                    self.emit(conversation_id, event);
                }
            }
            TurnEvent::Thinking(text) => self.emit(conversation_id, ChatEvent::Thinking { text: text.clone() }),
            TurnEvent::SubagentDelta(text) => self.emit(conversation_id, ChatEvent::Subagent { text: text.clone() }),
            TurnEvent::Tool(name) => self.emit(conversation_id, ChatEvent::Tool { name: name.clone() }),
            TurnEvent::Rate(rate) => {
                if self.accounts.set_usage(account_id, rate.util_5h, rate.util_7d, rate.resets_5h).is_ok() {
                    self.events.accounts_changed();
                }
            }
            TurnEvent::Context { used_tokens, window_tokens } => {
                if let Some(percent) = context_percent(*used_tokens, *window_tokens) {
                    let _ = self.conversations.set_context_percent(conversation_id, percent);
                }
            }
            TurnEvent::Environment { skills, connectors } => {
                let _ = self.skills.remember(skills);
                for (key, status) in connectors {
                    let _ = self.connectors.set_status(key, status);
                }
            }
            TurnEvent::Session(_) | TurnEvent::ApiError(_) | TurnEvent::Result { .. } => {}
        }
    }
}

pub fn make_title(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("Nova conversa").trim();
    if line.chars().count() <= TITLE_MAX_CHARS {
        return line.to_owned();
    }
    let cut: String = line.chars().take(TITLE_MAX_CHARS).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;
    use crate::domain::{account::sample, chat::{AttachmentKind, TurnRequest}, history::Snippet};

    fn use_case(world: &World, engine: ScriptedEngine) -> SendMessage {
        SendMessage {
            accounts: world.store.clone(),
            conversations: world.store.clone(),
            switches: world.store.clone(),
            settings: world.store.clone(),
            engine: Arc::new(engine),
            prepare: PrepareTurn {
                settings: world.store.clone(),
                history: world.history.clone(),
                skills: world.skills.clone(),
                agents: world.store.clone(),
                projects: world.store.clone(),
                connectors: world.store.clone(),
                conversations: world.store.clone(),
            },
            skills: world.skills.clone(),
            connectors: world.store.clone(),
            events: world.events.clone(),
            files: Arc::new(FakeFiles(vec![Attachment::text("nota.md", "conteúdo")])),
            clock: Arc::new(FixedClock(1000)),
        }
    }

    #[tokio::test]
    async fn continues_on_next_account_when_limit_is_reached() {
        let world = World::with_accounts(vec![sample("pessoal", 1), sample("trabalho", 2)]);
        let engine = ScriptedEngine::new(|req, account_calls| match account_calls {
            1 => (vec![TurnEvent::Delta("parcial".into())], TurnEnd::Limited { resets_at: Some(5000) }),
            _ => {
                assert_eq!(req.prompt, "olá", "a mesma mensagem é reenviada");
                (vec![TurnEvent::Delta("resposta".into())], TurnEnd::Done { session_id: Some("s1".into()), text: "resposta".into() })
            }
        });
        use_case(&world, engine).execute("c1", "olá", &[], vec![], TurnControl::new().0).await.unwrap();

        let accounts = world.store.all().unwrap();
        assert_eq!(accounts[0].limited_until, Some(5000));
        assert_eq!(accounts[1].limited_until, None);

        let messages = world.store.messages("c1").unwrap();
        let roles: Vec<&str> = messages.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(roles, ["user", "notice", "assistant"]);
        assert_eq!(messages[1].content, "Limite da conta pessoal atingido · continuando com trabalho");
        assert_eq!(messages[2].content, "resposta");
        assert_eq!(messages[2].account_id.as_deref(), Some("trabalho"));

        assert_eq!(world.store.recent().unwrap().len(), 1);
        assert_eq!(world.conversation("c1").cli_session_id.as_deref(), Some("s1"));

        let events = world.events.chat_events();
        assert!(events.contains(&ChatEvent::Reset), "o texto parcial é descartado");
        assert_eq!(events.last(), Some(&ChatEvent::Done { text: "resposta".into() }));
    }

    #[tokio::test]
    async fn reports_earliest_reset_when_every_account_is_limited() {
        let world = World::with_accounts(vec![sample("a", 1), sample("b", 2)]);
        let engine = ScriptedEngine::new(|_, n| (vec![], TurnEnd::Limited { resets_at: Some(2000 + n as i64) }));
        use_case(&world, engine).execute("c1", "olá", &[], vec![], TurnControl::new().0).await.unwrap();
        assert_eq!(world.events.chat_events().last(), Some(&ChatEvent::Exhausted { resets_at: Some(2001) }));
    }

    #[tokio::test]
    async fn does_not_switch_when_auto_switch_is_off() {
        let world = World::with_accounts(vec![sample("a", 1), sample("b", 2)]);
        world.store.set_flag(flags::AUTO_SWITCH, false).unwrap();
        let engine = ScriptedEngine::new(|_, _| (vec![], TurnEnd::Limited { resets_at: None }));
        let calls = engine.calls.clone();
        use_case(&world, engine).execute("c1", "olá", &[], vec![], TurnControl::new().0).await.unwrap();
        assert_eq!(*calls.lock().unwrap(), 1);
        assert!(matches!(world.events.chat_events().last(), Some(ChatEvent::Error { .. })));
        assert_eq!(world.store.all().unwrap()[0].limited_until, Some(1000 + DEFAULT_LIMIT_SECONDS));
    }

    #[tokio::test]
    async fn skips_account_that_needs_login() {
        let world = World::with_accounts(vec![sample("a", 1), sample("b", 2)]);
        let engine = ScriptedEngine::new(|_, n| match n {
            1 => (vec![], TurnEnd::AuthError("login".into())),
            _ => (vec![], TurnEnd::Done { session_id: None, text: "ok".into() }),
        });
        use_case(&world, engine).execute("c1", "olá", &[], vec![], TurnControl::new().0).await.unwrap();
        assert!(!world.store.all().unwrap()[0].auth_ok);
        assert!(world.store.messages("c1").unwrap()[1].content.contains("precisa de novo login"));
    }

    #[tokio::test]
    async fn adds_history_context_and_titles_first_message() {
        let world = World::with_accounts(vec![sample("a", 1)]);
        world.history.set(vec![Snippet { session_id: "h".into(), title: "Índices".into(), text: "btree".into() }]);
        let engine = ScriptedEngine::new(|req, _| {
            assert!(req.extra_system.as_deref().unwrap().contains("[Índices]"));
            (vec![], TurnEnd::Done { session_id: None, text: "ok".into() })
        });
        use_case(&world, engine).execute("c1", "Como indexar pedidos?", &[], vec![], TurnControl::new().0).await.unwrap();
        let events = world.events.chat_events();
        assert_eq!(events[0], ChatEvent::Title { title: "Como indexar pedidos?".into() });
        assert!(events.contains(&ChatEvent::Context { titles: vec!["Índices".into()] }));
        assert!(world.store.messages("c1").unwrap()[1].meta.as_deref().unwrap().contains("Índices"));
    }

    #[test]
    fn title_is_first_line_truncated() {
        assert_eq!(make_title("\n  Olá  \nresto"), "Olá");
        let long = "a".repeat(60);
        assert_eq!(make_title(&long).chars().count(), TITLE_MAX_CHARS + 1);
    }

    #[tokio::test]
    async fn records_tool_use_between_text_segments() {
        let world = World::with_accounts(vec![sample("a", 1)]);
        let engine = ScriptedEngine::new(|_, _| {
            let events = vec![
                TurnEvent::Delta("Vou ler o arquivo.".into()),
                TurnEvent::ToolCall { id: "t1".into(), name: "Read".into(), input: r#"{"file_path":"a.txt"}"#.into() },
                TurnEvent::ToolResult { id: "t1".into(), output: "conteúdo".into(), is_error: false },
                TurnEvent::ToolCall { id: "t2".into(), name: "Edit".into(), input: "{}".into() },
                TurnEvent::Delta("Pronto.".into()),
            ];
            (events, TurnEnd::Done { session_id: None, text: "Vou ler o arquivo.Pronto.".into() })
        });
        use_case(&world, engine).execute("c1", "olá", &[], vec![], TurnControl::new().0).await.unwrap();

        let messages = world.store.messages("c1").unwrap();
        let roles: Vec<&str> = messages.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(roles, ["user", "assistant", "tool", "tool", "assistant"]);
        assert_eq!(messages[1].content, "Vou ler o arquivo.");
        assert!(messages[2].content.contains("\"output\":\"conteúdo\""));
        assert!(messages[3].content.contains("\"name\":\"Edit\"") && messages[3].content.contains("\"output\":null"));
        assert_eq!(messages[4].content, "Pronto.");
        assert_eq!(world.events.chat_events().iter().filter(|e| **e == ChatEvent::Step).count(), 3);
    }

    #[tokio::test]
    async fn asks_permission_unless_tool_was_allowed_for_the_conversation() {
        let ask = || TurnEvent::Permission { request_id: "r1".into(), tool: "Edit".into(), input: "{}".into(), description: "a.txt".into() };
        let script = move |_: &TurnRequest, _| (vec![ask()], TurnEnd::Done { session_id: None, text: "ok".into() });

        let world = World::with_accounts(vec![sample("a", 1)]);
        let (control, _handle) = TurnControl::new();
        use_case(&world, ScriptedEngine::new(script)).execute("c1", "olá", &[], vec![], control).await.unwrap();
        assert!(world.events.chat_events().iter().any(|e| matches!(e, ChatEvent::Permission { tool, .. } if tool == "Edit")));

        let world = World::with_accounts(vec![sample("a", 1)]);
        world.store.allow_tool("c1", "Edit").unwrap();
        let engine = ScriptedEngine::new(script);
        let decisions = engine.decisions.clone();
        use_case(&world, engine).execute("c1", "olá", &[], vec![], TurnControl::new().0).await.unwrap();
        assert!(!world.events.chat_events().iter().any(|e| matches!(e, ChatEvent::Permission { .. })), "não pergunta de novo");
        assert_eq!(*decisions.lock().unwrap(), [PermissionDecision::new("r1", true)]);
    }

    #[tokio::test]
    async fn spares_the_account_near_its_limit_and_says_so() {
        let world = World::with_accounts(vec![sample("a", 1), sample("b", 2)]);
        world.store.set_usage("a", Some(0.98), None, Some(5000)).unwrap();
        let engine = ScriptedEngine::new(|req, _| {
            assert_eq!(req.config_dir, None);
            (vec![], TurnEnd::Done { session_id: None, text: "ok".into() })
        });
        use_case(&world, engine).execute("c1", "olá", &[], vec![], TurnControl::new().0).await.unwrap();
        let messages = world.store.messages("c1").unwrap();
        assert_eq!(messages[1].content, "A conta a está quase no limite · usando b");
        assert_eq!(messages[2].account_id.as_deref(), Some("b"));

        world.store.set_flag(flags::SWITCH_EARLY, false).unwrap();
        let engine = ScriptedEngine::new(|_, _| (vec![], TurnEnd::Done { session_id: None, text: "ok".into() }));
        use_case(&world, engine).execute("c1", "de novo", &[], vec![], TurnControl::new().0).await.unwrap();
        assert_eq!(world.store.messages("c1").unwrap().last().unwrap().account_id.as_deref(), Some("a"));
    }

    #[tokio::test]
    async fn sends_attachments_and_notes_them_on_the_user_message() {
        let world = World::with_accounts(vec![sample("a", 1)]);
        let engine = ScriptedEngine::new(|req, _| {
            assert_eq!(req.attachments, [Attachment::text("nota.md", "conteúdo")]);
            (vec![], TurnEnd::Done { session_id: None, text: "ok".into() })
        });
        use_case(&world, engine).execute("c1", "resuma", &["nota.md".into()], vec![], TurnControl::new().0).await.unwrap();
        assert_eq!(world.store.messages("c1").unwrap()[0].meta.as_deref(), Some(r#"{"attachments":["nota.md"]}"#));

        let pasted = Attachment { name: "colada.png".into(), kind: AttachmentKind::Image, media_type: "image/png".into(), data: "AAAA".into() };
        let engine = ScriptedEngine::new(|req, _| {
            assert_eq!(req.attachments.len(), 2, "arquivo escolhido e imagem colada");
            assert_eq!(req.attachments[1].name, "colada.png");
            (vec![], TurnEnd::Done { session_id: None, text: "ok".into() })
        });
        use_case(&world, engine).execute("c1", "veja", &["nota.md".into()], vec![pasted], TurnControl::new().0).await.unwrap();
        assert!(world.store.messages("c1").unwrap()[2].meta.as_deref().unwrap().contains("colada.png"));

        let missing = use_case(&world, ScriptedEngine::new(|_, _| unreachable!()))
            .execute("c1", "resuma", &["sumiu.txt".into()], vec![], TurnControl::new().0)
            .await;
        assert!(missing.is_err(), "arquivo ilegível impede o envio");
        assert_eq!(world.store.messages("c1").unwrap().len(), 4, "e nada é gravado");
    }
}
