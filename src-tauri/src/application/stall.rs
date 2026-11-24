//! Vigia do turno: nenhum bot fica travado sem resposta. Um conector que não devolve a chamada
//! (ex.: o do Jira) ou um CLI que para de mandar qualquer coisa interrompem o turno depois de um
//! tempo sem atividade, e quem roda o turno continua a sessão avisando o agente do que travou.
//! Os agentes em segundo plano (que têm o próprio limite) não contam. Esperar a sua autorização conta
//! só depois de um prazo longo: um pedido que se perdeu (ex.: a tela recarregou) não trava o turno para sempre.

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
/// Pedido de autorização sem resposta: a pessoa pode ter saído, mas o turno não espera para sempre.
pub const YOU_LIMIT: Duration = Duration::from_secs(30 * 60);
/// Interrupções seguidas no mesmo turno antes de desistir com erro.
pub const MAX_STALLS: usize = 2;

#[derive(Debug)]
pub struct Watch {
    last: Instant,
    /// Ferramentas chamadas e ainda sem resultado: id → nome.
    open: HashMap<String, String>,
    /// Pedidos de autorização que foram para você e ainda esperam a resposta (id do pedido).
    asked: HashSet<String>,
    /// Desde quando há pedido seu esperando.
    asked_since: Option<Instant>,
    /// A resposta saiu e só os agentes em segundo plano seguem trabalhando (até o próximo resultado).
    background: bool,
}

impl Watch {
    pub fn new(now: Instant) -> Watch {
        Watch { last: now, open: HashMap::new(), asked: HashSet::new(), asked_since: None, background: false }
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
                self.asked_since.get_or_insert(now);
                self.asked.insert(request_id.clone());
            }
            // Um pedido que o CLI cancelou não espera mais você: não pode deixar o vigia pausado.
            TurnEvent::PermissionCancelled(request_id) => {
                self.asked.remove(request_id);
                if self.asked.is_empty() {
                    self.asked_since = None;
                }
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
            // Com outro pedido ainda esperando, o prazo dele recomeça: você está respondendo.
            self.asked_since = (!self.asked.is_empty()).then_some(now);
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

    /// Quando o turno vence, se nada acontecer até lá; None enquanto pausado pelos agentes em segundo plano.
    pub fn deadline(&self) -> Option<Instant> {
        if self.background {
            return None;
        }
        match self.asked_since {
            Some(since) => Some(since + YOU_LIMIT),
            None => Some(self.last + self.limit()),
        }
    }

    /// O que travou, para o aviso e para o agente: "o conector X (getJiraIssue)", "o comando Bash"...
    pub fn culprit(&self) -> String {
        if self.asked_since.is_some() {
            return format!("O pedido de autorização ficou {} minutos sem resposta", YOU_LIMIT.as_secs() / 60);
        }
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
    fn esperar_voce_tem_prazo_longo_e_o_segundo_plano_nao_conta() {
        let t0 = Instant::now();
        let mut w = Watch::new(t0);
        let ask = TurnEvent::Permission { request_id: "r".into(), tool: "Bash".into(), input: "{}".into(), description: String::new() };
        w.see(&ask, true, t0);
        assert_eq!(w.deadline(), Some(t0 + YOU_LIMIT));
        assert_eq!(w.culprit(), "O pedido de autorização ficou 30 minutos sem resposta");
        // Outra chamada que termina enquanto o pedido espera você não muda o prazo do pedido.
        w.see(&TurnEvent::ToolResult { id: "x".into(), output: String::new(), is_error: false }, false, t0 + Duration::from_secs(60));
        assert_eq!(w.deadline(), Some(t0 + YOU_LIMIT));
        let answered = t0 + Duration::from_secs(1200);
        w.answered("r", answered);
        assert_eq!(w.deadline(), Some(answered + SILENCE_LIMIT));
        // Um pedido que o CLI cancelou volta a contar o tempo.
        w.see(&ask, true, t0);
        w.see(&TurnEvent::PermissionCancelled("r".into()), false, answered);
        assert_eq!(w.deadline(), Some(answered + SILENCE_LIMIT));
        w.see(&TurnEvent::AwaitingBackground, false, t0);
        assert_eq!(w.deadline(), None);
        // Os agentes em segundo plano mandam notícias e depois ficam calados: segue pausado.
        w.see(&TurnEvent::SubagentDelta("lendo".into()), false, t0);
        assert_eq!(w.deadline(), None);
        w.see(&TurnEvent::Result { is_error: false, text: String::new(), session_id: None }, false, t0);
        assert_eq!(w.deadline(), Some(t0 + SILENCE_LIMIT));
    }

    #[test]
    fn dois_pedidos_ao_mesmo_tempo_esperam_os_dois() {
        let t0 = Instant::now();
        let mut w = Watch::new(t0);
        let ask = |id: &str| TurnEvent::Permission { request_id: id.into(), tool: "Read".into(), input: "{}".into(), description: String::new() };
        w.see(&ask("a"), true, t0);
        w.see(&ask("b"), true, t0);
        let later = t0 + Duration::from_secs(120);
        w.answered("b", later);
        assert_eq!(w.deadline(), Some(later + YOU_LIMIT), "o outro pedido segue esperando, com prazo novo");
        w.answered("a", later);
        assert_eq!(w.deadline(), Some(later + SILENCE_LIMIT));
    }
}
