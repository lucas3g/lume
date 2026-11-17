//! Caso de uso central: enviar uma mensagem e, se a conta atingir o limite,
//! continuar a mesma conversa na próxima conta disponível.

use super::{
    conversations::NEW_CONVERSATION_TITLE,
    flags,
    projects::NEW_SESSION_TITLE,
    ports::{
        AccountRepository, ChatEngine, Clock, ConnectorRepository, ConversationRepository, EventSink, FileLoader,
        Settings, SkillRepository, SwitchLog,
    },
    turn_setup::PrepareTurn,
    AppError, AppResult, ChatEvent, PendingUsers, TurnControl,
};
use crate::domain::{
    account::{earliest_reset, pick_next, Account, Provider},
    workspace::context_percent,
    chat::{
        tool_message, Attachment, APP_NOTE_CLOSE, APP_NOTE_OPEN, Conversation, Injection, PermissionDecision, SubagentStep, TurnEnd, TurnEvent, TurnRequest, ROLE_ASSISTANT, ROLE_NOTICE,
        ROLE_TOOL, ROLE_USER,
    },
};
use std::{collections::{HashMap, HashSet}, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

const TITLE_MAX_CHARS: usize = 48;
/// Usado quando o provedor não informa o horário do reset.
const DEFAULT_LIMIT_SECONDS: i64 = 3600;
/// Configurações com o resumo das instruções e dos arquivos de conhecimento que a sessão já conhece.
const RULES_PREFIX: &str = "rules_sent:";
const KNOWLEDGE_PREFIX: &str = "knowledge_sent:";

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
    /// Chamadas que criaram subagentes neste turno; as demais tarefas (ex.: comandos
    /// em segundo plano) também mandam andamento e fim, que não interessam.
    subagents: HashSet<String>,
    /// Sessão aberta pelo provedor neste turno.
    session: Option<String>,
    /// A resposta já foi entregue e gravada; o processo segue só pelos agentes em segundo plano.
    answered: bool,
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

/// Modelo do turno no serviço da conta: o da conversa é do Claude; nos outros serviços
/// vale o escolhido para a conversa ou, sem escolha, o padrão do CLI (texto vazio).
fn model_for(provider: Provider, claude_model: &str, others: &std::collections::BTreeMap<String, String>) -> String {
    match provider {
        Provider::Claude => claude_model.to_owned(),
        other => others.get(other.as_str()).cloned().unwrap_or_default(),
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
        control: TurnControl,
    ) -> AppResult<()> {
        self.execute_turn(conversation_id, text, attachment_paths, pasted, control, true).await
    }

    /// Turno de uma etapa da esteira: as imagens do pedido já ficam guardadas com a execução,
    /// então a mensagem registra só os nomes delas, sem copiar o conteúdo a cada etapa.
    pub async fn execute_stage(&self, conversation_id: &str, text: &str, images: Vec<Attachment>, control: TurnControl) -> AppResult<()> {
        self.execute_turn(conversation_id, text, &[], images, control, false).await
    }

    async fn execute_turn(
        &self,
        conversation_id: &str,
        text: &str,
        attachment_paths: &[String],
        pasted: Vec<Attachment>,
        mut control: TurnControl,
        keep_image_data: bool,
    ) -> AppResult<()> {
        let conversation = self
            .conversations
            .get(conversation_id)?
            .ok_or_else(|| AppError("Conversa não encontrada.".into()))?;
        let mut attachments = attachment_paths.iter().map(|p| self.files.load(p)).collect::<AppResult<Vec<_>>>()?;
        let meta = user_meta(&attachments, attachment_paths, &pasted, keep_image_data);
        attachments.extend(pasted);
        let title = self.store_user_message(&conversation, text, meta.as_deref())?;
        let setup = self.prepare.prepare(&conversation, text)?;
        let provider_models = conversation.provider_models.clone();
        let context_meta = self.announce_context(conversation_id, &setup.context_titles);
        // Quem respondeu por último nesta conversa: poupar uma conta só é novidade se a conta mudou.
        let last_account = self
            .conversations
            .messages(conversation_id)?
            .iter()
            .rev()
            .find(|m| m.role == ROLE_ASSISTANT)
            .and_then(|m| m.account_id.clone());

        let mut tried: Vec<String> = vec![];
        let mut previous: Option<(Account, Handoff)> = None;
        // Instruções e conhecimento que a sessão já recebeu: cada parte só é reenviada quando muda.
        let standing = [(RULES_PREFIX, setup.rules.as_deref()), (KNOWLEDGE_PREFIX, setup.knowledge_key.as_deref())]
            .map(|(prefix, text)| {
                let key = format!("{prefix}{conversation_id}");
                let print = fingerprint(text);
                let changed = self.settings.text(&key).unwrap_or_default() != print;
                (key, print, changed)
            });
        let [rules_changed, knowledge_changed] = [standing[0].2, standing[1].2];
        // A sessão gravou a mensagem (e o que foi junto); falhar aqui só faz reenviar.
        let remember_standing = || {
            for (key, print, _) in &standing {
                let _ = self.settings.set_text(key, print);
            }
        };

        loop {
            let all = self.accounts.all()?;
            let now = self.clock.now();
            let spare = self.settings.flag(flags::SWITCH_EARLY, true);
            let Some(account) = pick_next(&all, now, &tried, spare).cloned() else {
                self.emit(conversation_id, ChatEvent::Exhausted { resets_at: earliest_reset(&all, now) });
                return Ok(());
            };
            if let Some((from, reason)) = previous.take() {
                self.announce_switch(conversation_id, &title, &from, &account, reason)?;
            } else if tried.is_empty() {
                // Na primeira tentativa, avisa se a conta da vez foi poupada por estar quase no limite.
                let spared = pick_next(&all, now, &tried, false).filter(|a| a.id != account.id);
                if let Some(from) = spared.filter(|_| last_account.as_deref() != Some(account.id.as_str())) {
                    self.announce_switch(conversation_id, &title, &from.clone(), &account, Handoff::NearLimit)?;
                }
            }
            self.emit(conversation_id, ChatEvent::Account { id: account.id.clone(), name: account.name.clone() });
            tried.push(account.id.clone());

            // A sessão só existe no serviço que a criou: uma conta de outro tipo começa
            // uma sessão nova e recebe o diálogo já gravado como contexto.
            let foreign_session = conversation.cli_session_id.is_some() && conversation.session_provider != account.provider;
            let extra_system = if foreign_session {
                let sections = [setup.extra_system.clone(), self.prepare.dialogue(&conversation)?];
                Some(sections.into_iter().flatten().collect::<Vec<_>>().join("\n\n")).filter(|s| !s.is_empty())
            } else {
                setup.extra_system.clone()
            };
            let resume = conversation.cli_session_id.clone().filter(|_| !foreign_session);
            let prompt = match &resume {
                Some(_) if rules_changed || knowledge_changed => updated_instructions(
                    rules_changed.then(|| setup.rules.as_deref().unwrap_or_default()),
                    knowledge_changed.then(|| setup.knowledge.as_deref().unwrap_or_default()),
                    text,
                ),
                _ => text.to_owned(),
            };
            let request = TurnRequest {
                provider: account.provider,
                config_dir: account.config_dir.clone(),
                cwd: conversation.cwd.clone(),
                model: model_for(account.provider, &setup.model, &provider_models),
                resume,
                prompt,
                attachments: attachments.clone(),
                extra_system,
                tools: setup.tools.clone(),
                effort: setup.effort.clone(),
                permission_mode: setup.permission_mode.clone(),
                interactive: setup.interactive,
                denied_skills: setup.denied_skills.clone(),
                denied_tools: setup.denied_tools.clone(),
                mcp_config: setup.mcp_config.clone(),
                allowed_tools: setup.allowed_tools.clone(),
                read_dirs: setup.read_dirs.clone(),
                mcp_servers: setup.mcp_servers.clone(),
            };
            let mut journal = Journal::default();
            let answer = control.answer.clone();
            let pending_users = control.pending_users.clone();
            let end = self
                .engine
                .run_turn(&request, &mut control, &mut |event| {
                    self.forward(conversation_id, &account, &mut journal, &answer, &pending_users, event)
                })
                .await;

            // Com a resposta já entregue, um limite ou login vencido no fim (enquanto os agentes
            // em segundo plano terminavam) não refaz a pergunta em outra conta: ela já foi respondida.
            if journal.answered && matches!(end, TurnEnd::Limited { .. } | TurnEnd::AuthError(_)) {
                remember_standing();
                let notice = match end {
                    TurnEnd::Limited { resets_at } => {
                        self.accounts.set_limited(&account.id, Some(resets_at.unwrap_or(now + DEFAULT_LIMIT_SECONDS)))?;
                        format!("A conta {} atingiu o limite antes de os agentes em segundo plano terminarem.", account.name)
                    }
                    _ => {
                        self.accounts.set_auth(&account.id, false)?;
                        format!("A conta {} precisa de novo login; os agentes em segundo plano não terminaram.", account.name)
                    }
                };
                self.events.accounts_changed();
                self.record_segment(conversation_id, &account.id, &mut journal);
                self.conversations.add_message(conversation_id, ROLE_NOTICE, &notice, None, None)?;
                self.emit(conversation_id, ChatEvent::Notice { text: notice });
                self.emit(conversation_id, ChatEvent::Done { text: String::new() });
                return Ok(());
            }
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
                    remember_standing();
                    if session_id.is_some() {
                        self.conversations.set_session_provider(conversation_id, account.provider)?;
                    }
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
                    remember_standing();
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

    /// Grava a mensagem do usuário e dá título à conversa na primeira mensagem,
    /// a não ser que ela já tenha um título próprio (ex.: a rodada de uma tarefa agendada).
    fn store_user_message(&self, conversation: &Conversation, text: &str, meta: Option<&str>) -> AppResult<String> {
        let first = self.conversations.message_count(&conversation.id)? == 0;
        self.conversations.add_message(&conversation.id, ROLE_USER, text, None, meta)?;
        let untitled = [NEW_CONVERSATION_TITLE, NEW_SESSION_TITLE].contains(&conversation.title.as_str());
        if !first || !untitled {
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
        for (id, (name, input)) in std::mem::take(&mut journal.calls) {
            self.record_tool(conversation_id, account_id, journal, &tool_message(&id, &name, &input, None, false));
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
        account: &Account,
        journal: &mut Journal,
        answer: &UnboundedSender<PermissionDecision>,
        pending_users: &PendingUsers,
        event: &TurnEvent,
    ) {
        let account_id = account.id.as_str();
        match event {
            TurnEvent::Delta(text) => {
                journal.segment.push_str(text);
                self.emit(conversation_id, ChatEvent::Delta { text: text.clone() });
            }
            TurnEvent::ToolCall { id, name, input } => {
                self.record_segment(conversation_id, account_id, journal);
                journal.calls.insert(id.clone(), (name.clone(), input.clone()));
                self.emit(conversation_id, ChatEvent::ToolStart { id: id.clone(), name: name.clone(), input: input.clone() });
            }
            TurnEvent::ToolResult { id, output, is_error } => {
                if let Some((name, input)) = journal.calls.remove(id) {
                    let content = tool_message(id, &name, &input, Some(output), *is_error);
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
            TurnEvent::Subagent { parent, step } => {
                let lifecycle = matches!(step, SubagentStep::Progress { .. } | SubagentStep::Finished { .. });
                if lifecycle && !journal.subagents.contains(parent) {
                    return;
                }
                journal.subagents.insert(parent.clone());
                // Saídas longas são encurtadas já aqui: a interface ao vivo e a gravada mostram o mesmo.
                let step = step.truncated();
                // O andamento é passageiro; o resto fica gravado para rever o trabalho depois.
                if !matches!(step, SubagentStep::Progress { .. }) {
                    let _ = self.conversations.add_subagent_step(conversation_id, parent, &step.to_record());
                }
                self.emit(conversation_id, ChatEvent::AgentStep { parent: parent.clone(), step });
            }
            TurnEvent::AwaitingBackground => {
                // A resposta já está completa: vira mensagem (copiável) enquanto os agentes trabalham.
                let text = journal.segment.clone();
                self.record_unanswered_calls(conversation_id, account_id, journal);
                self.record_segment(conversation_id, account_id, journal);
                // A sessão fica guardada já: parar os agentes ou fechar o app não a perde.
                if let Some(session) = &journal.session {
                    let _ = self.conversations.update(conversation_id, None, Some(session), None);
                    let _ = self.conversations.set_session_provider(conversation_id, account.provider);
                }
                journal.answered = true;
                self.emit(conversation_id, ChatEvent::Waiting { text });
            }
            TurnEvent::Tool(name) => self.emit(conversation_id, ChatEvent::Tool { name: name.clone() }),
            TurnEvent::ToolTask(id) => self.emit(conversation_id, ChatEvent::ToolTask { id: id.clone() }),
            TurnEvent::Rate(rate) => {
                if self.accounts.set_usage(account_id, rate).is_ok() {
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
            TurnEvent::UserRead(id) => {
                let record = pending_users.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
                if let Some((text, meta)) = record {
                    // O que o agente disse até aqui fica antes da mensagem, como na conversa real.
                    self.record_segment(conversation_id, account_id, journal);
                    if self.conversations.add_message(conversation_id, ROLE_USER, &text, None, meta.as_deref()).is_ok() {
                        journal.recorded = true;
                        self.emit(conversation_id, ChatEvent::Injected { id: id.clone() });
                    }
                }
            }
            TurnEvent::Session(id) => journal.session = Some(id.clone()),
            TurnEvent::ApiError(_) | TurnEvent::Result { .. } => {}
        }
    }
}

impl SendMessage {
    /// Monta a mensagem a entregar durante o turno e o registro dos anexos para gravá-la.
    pub fn injection(&self, id: &str, text: &str, attachment_paths: &[String], pasted: Vec<Attachment>) -> AppResult<(Injection, Option<String>)> {
        let mut attachments = attachment_paths.iter().map(|p| self.files.load(p)).collect::<AppResult<Vec<_>>>()?;
        let meta = user_meta(&attachments, attachment_paths, &pasted, true);
        attachments.extend(pasted);
        Ok((Injection { id: id.to_owned(), prompt: text.to_owned(), attachments }, meta))
    }
}

/// Registro dos anexos na mensagem do usuário: nomes para exibir e o necessário
/// para reenviar (caminhos dos arquivos e conteúdo das imagens coladas).
/// `keep_image_data`: guarda o conteúdo das imagens coladas (para as miniaturas da conversa);
/// sem ele, só os nomes.
fn user_meta(files: &[Attachment], paths: &[String], pasted: &[Attachment], keep_image_data: bool) -> Option<String> {
    if files.is_empty() && pasted.is_empty() {
        return None;
    }
    let names: Vec<&str> = files.iter().chain(pasted).map(|a| a.name.as_str()).collect();
    let images: Vec<serde_json::Value> = pasted
        .iter()
        .filter(|_| keep_image_data)
        .map(|a| serde_json::json!({ "name": a.name, "media_type": a.media_type, "data": a.data }))
        .collect();
    Some(serde_json::json!({ "attachments": names, "paths": paths, "images": images }).to_string())
}

/// Resumo das instruções em vigor; vazio quando não há nenhuma.
fn fingerprint(standing: Option<&str>) -> String {
    use sha2::{Digest, Sha256};
    match standing.filter(|s| !s.is_empty()) {
        Some(text) => Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect(),
        None => String::new(),
    }
}

/// Mensagem de uma sessão retomada cujas instruções ou arquivos de conhecimento mudaram
/// depois que ela abriu: só a parte que mudou vai na frente, marcada como aviso do aplicativo.
fn updated_instructions(rules: Option<&str>, knowledge: Option<&str>, text: &str) -> String {
    let mut note = String::new();
    if let Some(rules) = rules {
        let rules = if rules.is_empty() { "Nenhuma instrução de projeto ou de agente está em vigor." } else { rules };
        note.push_str(&format!(
            "As instruções de projeto e de agente desta conversa foram atualizadas; valem a partir desta mensagem \
             e substituem as anteriores:\n{rules}\n"
        ));
    }
    if let Some(knowledge) = knowledge {
        let knowledge = if knowledge.is_empty() { "O projeto não tem mais arquivos de conhecimento." } else { knowledge };
        note.push_str(&format!(
            "Os arquivos de conhecimento do projeto mudaram; esta lista substitui a anterior e o que já foi lido deles \
             pode estar desatualizado:\n{knowledge}\n"
        ));
    }
    format!("{APP_NOTE_OPEN}\n{note}{APP_NOTE_CLOSE}\n\n{text}")
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

    #[test]
    fn stage_turns_record_only_the_image_names() {
        let image = Attachment { name: "tela.png".into(), kind: crate::domain::chat::AttachmentKind::Image, media_type: "image/png".into(), data: "iVBORw0K".into() };
        let full = user_meta(&[], &[], std::slice::from_ref(&image), true).unwrap();
        let names_only = user_meta(&[], &[], &[image], false).unwrap();
        assert!(full.contains("iVBORw0K"));
        assert!(!names_only.contains("iVBORw0K") && names_only.contains("tela.png"));
    }

    #[test]
    fn each_service_gets_its_own_model() {
        let others = std::collections::BTreeMap::from([("gemini".to_owned(), "gemini-pro".to_owned())]);
        assert_eq!(model_for(Provider::Claude, "opus", &others), "opus");
        assert_eq!(model_for(Provider::Gemini, "opus", &others), "gemini-pro");
        assert_eq!(model_for(Provider::ChatGpt, "opus", &others), "", "sem escolha, o Codex usa o modelo dele");
    }

    use super::*;
    use crate::application::fakes::*;
    use crate::domain::{account::{sample, Provider}, chat::{AttachmentKind, RateInfo, TurnRequest}, history::Snippet};

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
                knowledge: world.knowledge.clone(),
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
    async fn message_read_mid_turn_is_recorded_between_the_answers() {
        let world = World::with_accounts(vec![sample("pessoal", 1)]);
        let engine = ScriptedEngine::new(|_, _| {
            let events = vec![
                TurnEvent::Delta("Vou começar.".into()),
                TurnEvent::UserRead("primeira".into()),
                TurnEvent::UserRead("desconhecida".into()),
                TurnEvent::Delta("Feito, com BANANA.".into()),
            ];
            (events, TurnEnd::Done { session_id: None, text: "Vou começar.Feito, com BANANA.".into() })
        });
        let control = TurnControl::new().0;
        control.pending_users.lock().unwrap().insert("primeira".into(), ("diga BANANA".into(), None));
        use_case(&world, engine).execute("c1", "olá", &[], vec![], control).await.unwrap();

        let messages = world.store.messages("c1").unwrap();
        let shown: Vec<(&str, &str)> = messages.iter().map(|m| (m.role.as_str(), m.content.as_str())).collect();
        assert_eq!(shown, [("user", "olá"), ("assistant", "Vou começar."), ("user", "diga BANANA"), ("assistant", "Feito, com BANANA.")]);
        assert!(world.events.chat_events().contains(&ChatEvent::Injected { id: "primeira".into() }));
    }

    #[tokio::test]
    async fn answer_with_background_agents_is_kept_even_if_the_account_runs_out() {
        let world = World::with_accounts(vec![sample("pessoal", 1), sample("trabalho", 2)]);
        let engine = ScriptedEngine::new(|_, call| {
            assert_eq!(call, 1, "a pergunta já respondida não vai para outra conta");
            let events = vec![
                TurnEvent::Session("s1".into()),
                TurnEvent::Delta("Agente lançado.".into()),
                TurnEvent::AwaitingBackground,
            ];
            (events, TurnEnd::Limited { resets_at: Some(5000) })
        });
        let uc = use_case(&world, engine);
        uc.execute("c1", "olá", &[], vec![], TurnControl::new().0).await.unwrap();

        let messages = world.store.messages("c1").unwrap();
        let roles: Vec<&str> = messages.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(roles, ["user", "assistant", "notice"]);
        assert_eq!(messages[1].content, "Agente lançado.");
        assert_eq!(world.conversation("c1").cli_session_id.as_deref(), Some("s1"), "a sessão foi guardada na espera");
        assert_eq!(world.store.all().unwrap()[0].limited_until, Some(5000));
        let events = world.events.chat_events();
        assert!(events.contains(&ChatEvent::Waiting { text: "Agente lançado.".into() }));
        assert!(events.contains(&ChatEvent::Done { text: String::new() }));
    }

    #[tokio::test]
    async fn account_of_another_service_starts_its_own_session_with_the_dialogue() {
        let mut gpt = sample("gpt", 2);
        gpt.provider = Provider::ChatGpt;
        let world = World::with_accounts(vec![sample("claude", 1), gpt]);
        let engine = ScriptedEngine::new(|req, call| match call {
            1 => {
                assert_eq!((req.provider, req.resume.as_deref()), (Provider::Claude, None));
                (vec![], TurnEnd::Done { session_id: Some("sessao-claude".into()), text: "o número é 417".into() })
            }
            2 => {
                assert_eq!(req.resume.as_deref(), Some("sessao-claude"), "o mesmo serviço retoma a sessão");
                (vec![], TurnEnd::Limited { resets_at: Some(5000) })
            }
            _ => {
                assert_eq!(req.provider, Provider::ChatGpt);
                assert_eq!(req.resume, None, "a sessão do Claude não existe no Codex");
                assert!(req.extra_system.as_deref().unwrap().contains("417"), "o diálogo anterior vai como contexto");
                (vec![], TurnEnd::Done { session_id: Some("sessao-codex".into()), text: "417".into() })
            }
        });
        let uc = use_case(&world, engine);
        uc.execute("c1", "guarde 417", &[], vec![], TurnControl::new().0).await.unwrap();
        assert_eq!(world.conversation("c1").session_provider, Provider::Claude);
        uc.execute("c1", "qual era?", &[], vec![], TurnControl::new().0).await.unwrap();

        let conversation = world.conversation("c1");
        assert_eq!(conversation.cli_session_id.as_deref(), Some("sessao-codex"));
        assert_eq!(conversation.session_provider, Provider::ChatGpt, "a próxima mensagem retoma a sessão do Codex");
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

    #[tokio::test]
    async fn resumed_session_gets_project_instructions_changed_after_it_opened() {
        use crate::application::ports::ProjectRepository;
        use crate::domain::project::Project;
        let world = World::with_accounts(vec![sample("a", 1)]);
        let mut project = Project { id: "p1".into(), name: "Loja".into(), instructions: String::new(), folder: None, versioned: false };
        ProjectRepository::save(&*world.store, &project).unwrap();
        world.store.set_project("c1", Some("p1")).unwrap();
        world.store.add_file("f1", "p1", "glossario.md", "SKU é o código.").unwrap();
        let engine = ScriptedEngine::new(|req, call| {
            match call {
                1 => assert_eq!(req.prompt, "oi", "sessão nova: as instruções vão no prompt de sistema"),
                2 => assert_eq!(req.prompt, "tudo bem?", "nada mudou desde que a sessão abriu"),
                3 => {
                    assert_eq!(req.resume.as_deref(), Some("s1"));
                    assert!(req.prompt.contains("Responda em português.") && req.prompt.ends_with("e agora?"));
                    assert!(!req.prompt.contains("SKU"), "o conhecimento não mudou e não é reenviado");
                    assert_eq!(crate::domain::chat::without_app_note(&req.prompt), "e agora?");
                }
                _ => assert_eq!(req.prompt, "de novo", "a sessão já recebeu as instruções novas"),
            }
            (vec![], TurnEnd::Done { session_id: Some("s1".into()), text: "ok".into() })
        });
        let uc = use_case(&world, engine);
        uc.execute("c1", "oi", &[], vec![], TurnControl::new().0).await.unwrap();
        uc.execute("c1", "tudo bem?", &[], vec![], TurnControl::new().0).await.unwrap();
        project.instructions = "Responda em português.".into();
        ProjectRepository::save(&*world.store, &project).unwrap();
        uc.execute("c1", "e agora?", &[], vec![], TurnControl::new().0).await.unwrap();
        uc.execute("c1", "de novo", &[], vec![], TurnControl::new().0).await.unwrap();
        assert_eq!(world.store.messages("c1").unwrap()[4].content, "e agora?", "a conversa mostra só o que o usuário escreveu");
    }

    #[tokio::test]
    async fn resumed_session_is_told_when_knowledge_content_changes_with_same_name_and_size() {
        use crate::application::ports::ProjectRepository;
        use crate::domain::project::Project;
        let world = World::with_accounts(vec![sample("a", 1)]);
        let project = Project { id: "p1".into(), name: "Loja".into(), instructions: String::new(), folder: None, versioned: false };
        ProjectRepository::save(&*world.store, &project).unwrap();
        world.store.set_project("c1", Some("p1")).unwrap();
        world.store.add_file("f1", "p1", "precos.csv", "a;1").unwrap();
        let engine = ScriptedEngine::new(|req, call| {
            match call {
                1 => assert_eq!(req.read_dirs, ["/dados/knowledge/p1"]),
                _ => assert!(req.prompt.contains("mudaram") && req.prompt.contains("- precos.csv (3 caracteres)")),
            }
            (vec![], TurnEnd::Done { session_id: Some("s1".into()), text: "ok".into() })
        });
        let uc = use_case(&world, engine);
        uc.execute("c1", "preço?", &[], vec![], TurnControl::new().0).await.unwrap();
        ProjectRepository::remove_file(&*world.store, "f1").unwrap();
        world.store.add_file("f2", "p1", "precos.csv", "a;2").unwrap();
        uc.execute("c1", "e agora?", &[], vec![], TurnControl::new().0).await.unwrap();
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
        let started = world.events.chat_events().iter().filter(|e| matches!(e, ChatEvent::ToolStart { .. })).count();
        assert_eq!(started, 2, "cada chamada aparece ao começar, antes do resultado");
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
    async fn keeps_a_title_the_conversation_already_has() {
        let world = World::with_accounts(vec![sample("a", 1)]);
        world.store.update("c1", Some("⏱ Resumo"), None, None).unwrap();
        let engine = ScriptedEngine::new(|_, _| (vec![], TurnEnd::Done { session_id: None, text: "ok".into() }));
        use_case(&world, engine).execute("c1", "Resuma o dia.", &[], vec![], TurnControl::new().0).await.unwrap();
        assert_eq!(world.conversation("c1").title, "⏱ Resumo");
        assert!(!world.events.chat_events().iter().any(|e| matches!(e, ChatEvent::Title { .. })));
    }

    #[tokio::test]
    async fn spares_the_account_near_its_limit_and_says_so() {
        let world = World::with_accounts(vec![sample("a", 1), sample("b", 2)]);
        let rate = RateInfo { status: "allowed".into(), resets_at: None, util_5h: Some(0.98), util_7d: None, resets_5h: Some(5000), resets_7d: None };
        world.store.set_usage("a", &rate).unwrap();
        let engine = ScriptedEngine::new(|req, _| {
            assert_eq!(req.config_dir, None);
            (vec![], TurnEnd::Done { session_id: None, text: "ok".into() })
        });
        use_case(&world, engine).execute("c1", "olá", &[], vec![], TurnControl::new().0).await.unwrap();
        let messages = world.store.messages("c1").unwrap();
        assert_eq!(messages[1].content, "A conta a está quase no limite · usando b");
        assert_eq!(messages[2].account_id.as_deref(), Some("b"));

        let engine = ScriptedEngine::new(|_, _| (vec![], TurnEnd::Done { session_id: None, text: "ok".into() }));
        use_case(&world, engine).execute("c1", "mais", &[], vec![], TurnControl::new().0).await.unwrap();
        let notices = world.store.messages("c1").unwrap().iter().filter(|m| m.role == ROLE_NOTICE).count();
        assert_eq!(notices, 1, "a mesma conta seguir atendendo não é novidade");
        assert_eq!(world.store.recent().unwrap().len(), 1);

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
        let meta: serde_json::Value = serde_json::from_str(world.store.messages("c1").unwrap()[0].meta.as_deref().unwrap()).unwrap();
        assert_eq!(meta["attachments"][0], "nota.md");
        assert_eq!(meta["paths"][0], "nota.md", "o caminho fica guardado para reenviar");

        let pasted = Attachment { name: "colada.png".into(), kind: AttachmentKind::Image, media_type: "image/png".into(), data: "AAAA".into() };
        let engine = ScriptedEngine::new(|req, _| {
            assert_eq!(req.attachments.len(), 2, "arquivo escolhido e imagem colada");
            assert_eq!(req.attachments[1].name, "colada.png");
            (vec![], TurnEnd::Done { session_id: None, text: "ok".into() })
        });
        use_case(&world, engine).execute("c1", "veja", &["nota.md".into()], vec![pasted], TurnControl::new().0).await.unwrap();
        let meta: serde_json::Value = serde_json::from_str(world.store.messages("c1").unwrap()[2].meta.as_deref().unwrap()).unwrap();
        assert_eq!(meta["attachments"][1], "colada.png");
        assert_eq!(meta["images"][0]["data"], "AAAA", "a imagem colada fica guardada para reenviar");

        let missing = use_case(&world, ScriptedEngine::new(|_, _| unreachable!()))
            .execute("c1", "resuma", &["sumiu.txt".into()], vec![], TurnControl::new().0)
            .await;
        assert!(missing.is_err(), "arquivo ilegível impede o envio");
        assert_eq!(world.store.messages("c1").unwrap().len(), 4, "e nada é gravado");
    }
}
