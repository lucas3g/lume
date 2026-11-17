//! Vigia do turno: nenhum bot fica travado sem resposta. Um conector que não devolve a chamada
//! (ex.: o do Jira) ou um CLI que para de mandar qualquer coisa interrompem o turno depois de um
//! tempo sem atividade, e quem roda o turno continua a sessão avisando o agente do que travou.
//! Esperar a sua autorização, ou os agentes em segundo plano (que têm o próprio limite), não conta.

use crate::domain::chat::TurnEvent;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use tokio::time::Instant;

/// Chamada de conector (MCP) sem resposta: elas costumam voltar em segundos.
pub const CONNECTOR_LIMIT: Duration = Duration::from_secs(3 * 60);
/// Outras ferramentas em andamento (comandos longos, testes, builds): o Bash do CLI para sozinho em 10 min.
pub const TOOL_LIMIT: Duration = Duration::from_secs(15 * 60);
/// Nenhuma ferramenta em andamento e nada chegando: o modelo transmite enquanto pensa e escreve.
pub const SILENCE_LIMIT: Duration = Duration::from_secs(10 * 60);
/// Interrupções seguidas no mesmo turno antes de desistir com erro.
pub const MAX_STALLS: usize = 2;

#[derive(Debug)]
pub struct Watch {
    last: Instant,
    /// Ferramentas chamadas e ainda sem resultado: id → nome.
    open: HashMap<String, String>,
    /// Pedidos de autorização que foram para você e ainda esperam a resposta (id do pedido).
    asked: HashSet<String>,
    /// A resposta saiu e só os agentes em segundo plano seguem trabalhando (até o próximo resultado).
    background: bool,
}

impl Watch {
    pub fn new(now: Instant) -> Watch {
        Watch { last: now, open: HashMap::new(), asked: HashSet::new(), background: false }
    }

    /// Um evento do turno. `asks_you`: um pedido de autorização que foi para você (não aprovado sozinho).
    pub fn see(&mut self, event: &TurnEvent, asks_you: bool, now: Instant) {
        self.last = now;
        match event {
            TurnEvent::ToolCall { id, name, .. } => {
                self.open.insert(id.clone(), name.clone());
            }
            TurnEvent::ToolResult { id, .. } => {
                self.open.remove(id);
            }
            TurnEvent::Permission { request_id, .. } if asks_you => {
                self.asked.insert(request_id.clone());
            }
            TurnEvent::AwaitingBackground => self.background = true,
            TurnEvent::Result { .. } => self.background = false,
            _ => {}
        }
    }

    /// Você respondeu a um pedido: o tempo volta a contar a partir daqui.
    pub fn answered(&mut self, request_id: &str, now: Instant) {
        if self.asked.remove(request_id) {
            self.last = now;
        }
    }

    /// Tempo sem atividade que encerra o turno agora: menor se há conector esperando.
    pub fn limit(&self) -> Duration {
        if self.open.values().any(|n| n.starts_with("mcp__")) {
            CONNECTOR_LIMIT
        } else if self.open.is_empty() {
            SILENCE_LIMIT
        } else {
            TOOL_LIMIT
        }
    }

    /// Quando o turno vence, se nada acontecer até lá; None enquanto pausado.
    pub fn deadline(&self) -> Option<Instant> {
        (self.asked.is_empty() && !self.background).then(|| self.last + self.limit())
    }

    /// O que travou, para o aviso e para o agente: "o conector X (getJiraIssue)", "o comando Bash"...
    pub fn culprit(&self) -> String {
        let minutes = self.limit().as_secs() / 60;
        let tool = self.open.values().find(|n| n.starts_with("mcp__")).or_else(|| self.open.values().next());
        match tool {
            Some(name) if name.starts_with("mcp__") => {
                let mut parts = name.trim_start_matches("mcp__").splitn(2, "__");
                let server = parts.next().unwrap_or_default().trim_start_matches("claude_ai_").replace('_', " ");
                match parts.next() {
                    Some(op) => format!("O conector {server} ({op}) não respondeu em {minutes} minutos"),
                    None => format!("O conector {server} não respondeu em {minutes} minutos"),
                }
            }
            Some(name) => format!("A ferramenta {name} não terminou em {minutes} minutos"),
            None => format!("O agente ficou {minutes} minutos sem responder"),
        }
    }
}

/// Pedido para seguir na mesma sessão depois de uma interrupção pelo vigia.
pub fn resume_prompt(culprit: &str) -> String {
    format!(
        "{culprit} e foi interrompido pelo app. Não repita a mesma chamada: continue o trabalho de onde parou sem ela. \
         Se a informação for indispensável, siga com o que já tem e diga no resultado o que ficou faltando."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: &str, name: &str) -> TurnEvent {
        TurnEvent::ToolCall { id: id.into(), name: name.into(), input: "{}".into() }
    }

    #[test]
    fn conector_sem_resposta_vence_antes() {
        let t0 = Instant::now();
        let mut w = Watch::new(t0);
        assert_eq!(w.deadline(), Some(t0 + SILENCE_LIMIT));
        w.see(&call("1", "Bash"), false, t0);
        assert_eq!(w.limit(), TOOL_LIMIT);
        w.see(&call("2", "mcp__claude_ai_Atlassian__getJiraIssue"), false, t0);
        assert_eq!(w.deadline(), Some(t0 + CONNECTOR_LIMIT));
        assert_eq!(w.culprit(), "O conector Atlassian (getJiraIssue) não respondeu em 3 minutos");
        w.see(&TurnEvent::ToolResult { id: "2".into(), output: String::new(), is_error: false }, false, t0);
        assert_eq!(w.culprit(), "A ferramenta Bash não terminou em 15 minutos");
    }

    #[test]
    fn esperar_voce_ou_o_segundo_plano_nao_conta() {
        let t0 = Instant::now();
        let mut w = Watch::new(t0);
        let ask = TurnEvent::Permission { request_id: "r".into(), tool: "Bash".into(), input: "{}".into(), description: String::new() };
        w.see(&ask, true, t0);
        assert_eq!(w.deadline(), None);
        // Outra chamada que termina enquanto o pedido espera você não volta a contar o tempo.
        w.see(&TurnEvent::ToolResult { id: "x".into(), output: String::new(), is_error: false }, false, t0);
        assert_eq!(w.deadline(), None);
        let answered = t0 + Duration::from_secs(1200);
        w.answered("r", answered);
        assert_eq!(w.deadline(), Some(answered + SILENCE_LIMIT));
        w.see(&TurnEvent::AwaitingBackground, false, t0);
        assert_eq!(w.deadline(), None);
        // Os agentes em segundo plano mandam notícias e depois ficam calados: segue pausado.
        w.see(&TurnEvent::SubagentDelta("lendo".into()), false, t0);
        assert_eq!(w.deadline(), None);
        w.see(&TurnEvent::Result { is_error: false, text: String::new(), session_id: None }, false, t0);
        assert_eq!(w.deadline(), Some(t0 + SILENCE_LIMIT));
    }
}
