//! Casos de uso. Dependem apenas do domínio e das portas.

pub mod accounts;
pub mod conversations;
pub mod draft_agent;
pub mod draft_look;
pub mod extensions;
pub mod history;
pub mod jira_issues;
pub mod parked;
pub mod pipeline_insights;
pub mod pipeline;
pub mod ports;
pub mod projects;
pub mod send_message;
mod stall;
pub mod store;
pub mod tasks;
pub mod terminal;
pub mod turn_setup;
pub mod usage;
pub mod workspace;

#[cfg(test)]
pub(crate) mod fakes;

use serde::Serialize;
use std::{collections::HashMap, fmt, sync::{Arc, Mutex}};
use crate::domain::chat::{Injection, PermissionDecision, SubagentStep};
use tokio::sync::{mpsc, oneshot, Notify};

/// Sinal de cancelamento de um turno em andamento.
pub type Cancel = Arc<Notify>;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AppError(pub String);

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AppError {}

pub type AppResult<T> = Result<T, AppError>;

/// Chaves das configurações booleanas.
pub mod flags {
    pub const AUTO_SWITCH: &str = "auto_switch";
    pub const NOTIFY_SWITCH: &str = "notify_switch";
    pub const USE_HISTORY: &str = "use_history";
    /// Poupar a conta que está quase no limite enquanto outra estiver livre.
    pub const SWITCH_EARLY: &str = "switch_early";
    /// Avisos do sistema quando o app está em segundo plano.
    pub const NOTIFY_DESKTOP: &str = "notify_desktop";
    /// Com todas as contas no limite, continuar sozinho (conversa e esteira) quando uma voltar.
    pub const RESUME_ON_RESET: &str = "resume_on_reset";
    pub const ALL: [&str; 6] = [AUTO_SWITCH, NOTIFY_SWITCH, USE_HISTORY, SWITCH_EARLY, NOTIFY_DESKTOP, RESUME_ON_RESET];
}

/// Preferências de interface guardadas como texto.
pub mod prefs {
    /// Ordem dos projetos na barra lateral: "recent", "name" ou "created".
    pub const PROJECT_ORDER: &str = "project_order";
    pub const PROJECT_ORDERS: [&str; 3] = ["recent", "name", "created"];
    /// Modelo de cada nível em cada serviço para a esteira de bots (JSON de `TierModels`).
    pub const PIPELINE_MODELS: &str = "pipeline_models";
    /// Visual dos bonecos do escritório, por papel (JSON de `OfficeLooks`).
    pub const OFFICE_LOOKS: &str = "office_looks";
    /// Como a execução aberta da esteira aparece: "track" (trilha de cartões) ou "office" (escritório em pixel art).
    pub const PIPELINE_VIEW: &str = "pipeline_view";
    pub const PIPELINE_VIEWS: [&str; 2] = ["track", "office"];
}

/// Eventos de uma conversa enviados à interface.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatEvent {
    Title { title: String },
    Account { id: String, name: String },
    Context { titles: Vec<String> },
    Delta { text: String },
    Thinking { text: String },
    /// Andamento de um subagente (texto que ele produziu).
    Subagent { text: String },
    /// Resposta pronta; só subagentes em segundo plano seguem trabalhando, e o usuário pode escrever.
    Waiting { text: String },
    /// Passo de um subagente, para acompanhar o que ele faz; `parent` é a chamada que o criou.
    AgentStep { parent: String, step: SubagentStep },
    Tool { name: String },
    /// Uma ferramenta começou a rodar (argumentos completos); vira mensagem gravada quando termina.
    ToolStart { id: String, name: String, input: String },
    /// A ferramenta em execução virou tarefa e já pode ir para segundo plano.
    ToolTask { id: String },
    /// Algo foi gravado na conversa (trecho de texto ou uso de ferramenta): recarregue as mensagens.
    Step,
    /// Uma mensagem enviada durante o turno foi lida pelo agente e gravada na conversa.
    Injected { id: String },
    Permission { request_id: String, tool: String, input: String, description: String },
    /// O agente desistiu do pedido de autorização: ele sai da tela.
    PermissionCancelled { request_id: String },
    /// Descarta o texto parcial: o turno será refeito em outra conta.
    Reset,
    Notice { text: String },
    Done { text: String },
    Cancelled,
    /// Todas as contas atingiram o limite; com `resumes`, o app continua sozinho quando uma voltar.
    Exhausted { resets_at: Option<i64>, resumes: bool },
    /// Uma conta voltou e o app retomou o turno que tinha parado no limite.
    Resumed,
    Error { message: String },
}

/// Como o usuário (ou o próprio app) interfere em um turno em andamento.
pub struct TurnControl {
    pub cancel: Cancel,
    /// Respostas aos pedidos de autorização.
    pub decisions: mpsc::UnboundedReceiver<PermissionDecision>,
    /// Permite ao caso de uso responder sozinho (ferramenta já liberada).
    pub answer: mpsc::UnboundedSender<PermissionDecision>,
    /// Mensagens mandadas durante o turno; o motor confirma se conseguiu entregá-las.
    pub injections: mpsc::UnboundedReceiver<(Injection, oneshot::Sender<bool>)>,
    /// O que gravar na conversa quando o agente ler cada mensagem: id → (texto, registro dos anexos).
    pub pending_users: PendingUsers,
    /// Chamadas de ferramenta (Bash, subagente) que o usuário mandou para segundo plano;
    /// o motor confirma se conseguiu fazer o pedido.
    pub backgrounds: mpsc::UnboundedReceiver<(String, oneshot::Sender<bool>)>,
}

/// Mensagens entregues durante o turno e ainda não lidas pelo agente.
pub type PendingUsers = Arc<Mutex<HashMap<String, (String, Option<String>)>>>;

impl TurnControl {
    pub fn new() -> (TurnControl, TurnHandle) {
        let cancel = Cancel::default();
        let (answer, decisions) = mpsc::unbounded_channel();
        let (inject, injections) = mpsc::unbounded_channel();
        let (background, backgrounds) = mpsc::unbounded_channel();
        let pending_users = PendingUsers::default();
        let handle = TurnHandle { cancel: cancel.clone(), answer: answer.clone(), inject, pending_users: pending_users.clone(), background };
        (TurnControl { cancel, decisions, answer, injections, pending_users, backgrounds }, handle)
    }
}

/// Ponta do controle que fica com quem atende o usuário.
pub struct TurnHandle {
    cancel: Cancel,
    answer: mpsc::UnboundedSender<PermissionDecision>,
    inject: mpsc::UnboundedSender<(Injection, oneshot::Sender<bool>)>,
    pending_users: PendingUsers,
    background: mpsc::UnboundedSender<(String, oneshot::Sender<bool>)>,
}

/// Turnos em andamento, por conversa, para cancelar e responder autorizações.
#[derive(Default)]
pub struct RunningTurns(Mutex<HashMap<String, TurnHandle>>);

impl RunningTurns {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<String, TurnHandle>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Registra o turno; devolve `None` se a conversa já tem um em andamento.
    pub fn start(&self, conversation_id: &str) -> Option<TurnControl> {
        let mut map = self.map();
        if map.contains_key(conversation_id) {
            return None;
        }
        let (control, handle) = TurnControl::new();
        map.insert(conversation_id.to_owned(), handle);
        Some(control)
    }

    pub fn finish(&self, conversation_id: &str) {
        self.map().remove(conversation_id);
    }

    pub fn cancel(&self, conversation_id: &str) {
        if let Some(turn) = self.map().get(conversation_id) {
            turn.cancel.notify_one();
        }
    }

    pub fn decide(&self, conversation_id: &str, decision: PermissionDecision) {
        if let Some(turn) = self.map().get(conversation_id) {
            let _ = turn.answer.send(decision);
        }
    }

    /// Manda para segundo plano a ferramenta em execução (o turno segue sem esperá-la).
    /// O receptor diz se o motor fez o pedido; `None` quando não há turno nesta conversa.
    pub fn background(&self, conversation_id: &str, tool_id: &str) -> Option<oneshot::Receiver<bool>> {
        let map = self.map();
        let turn = map.get(conversation_id)?;
        let (ack, sent) = oneshot::channel();
        turn.background.send((tool_id.to_owned(), ack)).ok()?;
        Some(sent)
    }

    /// Esquece uma mensagem que o motor não aceitou.
    pub fn forget_injection(&self, conversation_id: &str, id: &str) {
        if let Some(turn) = self.map().get(conversation_id) {
            turn.pending_users.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
        }
    }

    /// Entrega uma mensagem ao turno em andamento. O receptor diz se o motor a aceitou;
    /// `None` quando não há turno nesta conversa.
    pub fn inject(&self, conversation_id: &str, injection: Injection, record: (String, Option<String>)) -> Option<oneshot::Receiver<bool>> {
        let map = self.map();
        let turn = map.get(conversation_id)?;
        let (ack, accepted) = oneshot::channel();
        turn.pending_users.lock().unwrap_or_else(|e| e.into_inner()).insert(injection.id.clone(), record);
        turn.inject.send((injection, ack)).ok()?;
        Some(accepted)
    }
}
