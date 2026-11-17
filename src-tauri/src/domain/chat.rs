//! Conversas, mensagens e o ciclo de um turno com o modelo.

use serde::Serialize;
use super::account::Provider;

pub const ROLE_USER: &str = "user";
/// Marca o trecho que o aplicativo põe na frente da mensagem do usuário (ex.: instruções
/// atualizadas). Não é do usuário: a leitura do histórico o descarta.
pub const APP_NOTE_OPEN: &str = "<lume-aviso>";
pub const APP_NOTE_CLOSE: &str = "</lume-aviso>";

/// A mensagem sem o trecho do aplicativo na frente.
pub fn without_app_note(text: &str) -> &str {
    match text.trim_start().strip_prefix(APP_NOTE_OPEN).and_then(|rest| rest.split_once(APP_NOTE_CLOSE)) {
        Some((_, user)) => user.trim_start(),
        None => text,
    }
}
pub const ROLE_ASSISTANT: &str = "assistant";
/// Aviso do próprio app dentro da conversa (ex.: troca de conta).
pub const ROLE_NOTICE: &str = "notice";
/// Uso de ferramenta registrado na conversa (conteúdo em JSON, ver `tool_message`).
pub const ROLE_TOOL: &str = "tool";

/// Conversa comum: só ferramentas de consulta, sem acesso a arquivos.
pub const KIND_CHAT: &str = "chat";
/// Sessão de agente: trabalha em uma pasta e pede aprovação para agir.
pub const KIND_AGENT: &str = "agent";

const MAX_TOOL_OUTPUT_CHARS: usize = 4000;

/// Níveis de esforço de raciocínio aceitos pelo provedor, do menor para o maior.
pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// Modos de permissão de uma sessão de agente.
/// `auto`: o provedor decide quando perguntar; `manual`: sempre pergunta antes de alterar;
/// `acceptEdits`: edições de arquivo passam direto; `plan`: planeja antes de alterar;
/// `bypassPermissions`: nada é perguntado.
pub const PERMISSION_MODES: [&str; 5] = ["auto", "manual", "acceptEdits", "plan", "bypassPermissions"];
pub const DEFAULT_PERMISSION_MODE: &str = "auto";

#[derive(Debug, Clone, Serialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub cli_session_id: Option<String>,
    /// Serviço em que a sessão acima existe; conta de outro tipo não consegue retomá-la.
    pub session_provider: Provider,
    pub model: String,
    pub cwd: String,
    pub agent_id: Option<String>,
    pub project_id: Option<String>,
    pub kind: String,
    /// Ferramentas que o usuário liberou de vez nesta conversa.
    pub allowed_tools: Vec<String>,
    /// Percentual do contexto do modelo ocupado no último turno.
    pub context_percent: Option<i64>,
    pub updated_at: i64,
    pub effort: String,
    pub permission_mode: String,
    /// Modelo da conversa nos serviços que não são o Claude ("chatgpt", "gemini"); sem
    /// entrada, o CLI do serviço escolhe. O da conversa (`model`) é o do Claude.
    pub provider_models: std::collections::BTreeMap<String, String>,
}

impl Conversation {
    pub fn is_agent(&self) -> bool {
        self.kind == KIND_AGENT
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub account_id: Option<String>,
    pub meta: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Switch {
    pub at: i64,
    pub from_name: String,
    pub to_name: String,
    pub conversation_title: String,
}

/// Situação do limite de uso informada pelo provedor durante um turno.
#[derive(Debug, Clone, PartialEq)]
pub struct RateInfo {
    pub status: String,
    pub resets_at: Option<i64>,
    pub util_5h: Option<f64>,
    pub util_7d: Option<f64>,
    pub resets_5h: Option<i64>,
    pub resets_7d: Option<i64>,
}

impl RateInfo {
    /// O provedor reporta "allowed" / "allowed_warning" enquanto há cota.
    pub fn is_limited(&self) -> bool {
        !self.status.starts_with("allowed")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TurnEvent {
    Session(String),
    /// O que o provedor carregou para o turno: skills disponíveis e conectores (nome, situação).
    Environment {
        skills: Vec<String>,
        connectors: Vec<(String, String)>,
    },
    Delta(String),
    Thinking(String),
    /// Texto produzido por um subagente, para acompanhar o andamento.
    SubagentDelta(String),
    /// A resposta terminou, mas subagentes em segundo plano seguem trabalhando no mesmo
    /// processo, que continua aceitando mensagens do usuário.
    AwaitingBackground,
    /// Passo de um subagente; `parent` é o id da chamada da ferramenta que o criou.
    Subagent {
        parent: String,
        step: SubagentStep,
    },
    Tool(String),
    /// A chamada de ferramenta virou uma tarefa (ex.: comando do terminal) que pode ir para segundo plano.
    ToolTask(String),
    /// Chamada de ferramenta completa; `input` é o JSON dos argumentos.
    ToolCall {
        id: String,
        name: String,
        input: String,
    },
    ToolResult {
        id: String,
        output: String,
        is_error: bool,
    },
    /// O provedor pede autorização para usar uma ferramenta.
    Permission {
        request_id: String,
        tool: String,
        input: String,
        description: String,
    },
    Rate(RateInfo),
    /// O provedor leu uma mensagem do usuário (eco com o identificador dela).
    UserRead(String),
    /// Tokens ocupados no contexto ao fim do turno e o tamanho da janela do modelo.
    Context {
        used_tokens: u64,
        window_tokens: u64,
    },
    /// Erro de API anexado a uma mensagem do assistente (ex.: "rate_limit").
    ApiError(String),
    Result {
        is_error: bool,
        text: String,
        session_id: Option<String>,
    },
}

/// O que é preciso para executar um turno em nome de uma conta.
#[derive(Debug, Clone)]
pub struct TurnRequest {
    /// Tipo da conta: decide qual CLI executa o turno.
    pub provider: Provider,
    pub config_dir: Option<String>,
    pub cwd: String,
    pub model: String,
    pub resume: Option<String>,
    pub prompt: String,
    pub attachments: Vec<Attachment>,
    pub extra_system: Option<String>,
    /// Ferramentas internas disponíveis (lista separada por vírgula ou "default").
    pub tools: String,
    pub effort: String,
    pub permission_mode: String,
    /// Pede autorização ao usuário para o que não estiver liberado.
    pub interactive: bool,
    pub denied_skills: Vec<String>,
    /// Regras de permissão que bloqueiam ferramentas inteiras (ex.: `mcp__servidor`).
    pub denied_tools: Vec<String>,
    /// Configuração MCP (JSON) dos conectores ligados.
    pub mcp_config: Option<String>,
    /// Regras que liberam as ferramentas dos conectores.
    pub allowed_tools: Vec<String>,
    /// Pastas que o assistente pode ler além da de trabalho (ex.: o conhecimento do projeto).
    pub read_dirs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TurnEnd {
    Done { session_id: Option<String>, text: String },
    Limited { resets_at: Option<i64> },
    AuthError(String),
    Failed(String),
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AttachmentKind {
    Text,
    Image,
    Pdf,
}

/// Mensagem que o usuário mandou durante um turno em andamento, para o agente ler
/// assim que puder (entre um passo e outro) sem interromper o que está fazendo.
#[derive(Debug, Clone, PartialEq)]
pub struct Injection {
    /// Identificador que o provedor devolve quando a mensagem é lida.
    pub id: String,
    pub prompt: String,
    pub attachments: Vec<Attachment>,
}

/// Arquivo enviado junto com a mensagem.
#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    pub name: String,
    pub kind: AttachmentKind,
    pub media_type: String,
    /// Conteúdo: o próprio texto, ou base64 para imagens e PDFs.
    pub data: String,
}

#[cfg(test)]
impl Attachment {
    pub fn text(name: &str, content: &str) -> Attachment {
        Attachment { name: name.into(), kind: AttachmentKind::Text, media_type: "text/plain".into(), data: content.into() }
    }
}

/// Ferramenta com que o agente faz perguntas ao usuário; a resposta vai nos próprios argumentos.
pub const TOOL_ASK: &str = "AskUserQuestion";
/// Ferramenta com que o agente pede aprovação do plano antes de agir.
pub const TOOL_EXIT_PLAN: &str = "ExitPlanMode";

/// Resposta do usuário a um pedido de autorização.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionDecision {
    pub request_id: String,
    pub allow: bool,
    /// Respostas às perguntas do agente (pergunta → opção escolhida).
    pub answers: Vec<(String, String)>,
    /// Modo de permissão que a sessão passa a usar ao aprovar um plano.
    pub next_mode: Option<String>,
}

impl PermissionDecision {
    pub fn new(request_id: &str, allow: bool) -> PermissionDecision {
        PermissionDecision { request_id: request_id.into(), allow, answers: vec![], next_mode: None }
    }
}

/// Pedidos que só o usuário pode responder: nunca são liberados "para sempre".
pub fn needs_user(tool: &str) -> bool {
    tool == TOOL_ASK || tool == TOOL_EXIT_PLAN
}

/// Tamanho máximo da transcrição enviada como contexto de uma conversa ramificada.
const MAX_TRANSCRIPT_CHARS: usize = 60_000;

/// Texto anexado ao prompt de sistema quando a conversa não tem sessão do
/// provedor (ramo de outra conversa, ou importada): o diálogo até aqui.
/// Fica com o fim da conversa se ela não couber inteira.
pub fn transcript_prompt(messages: &[Message]) -> Option<String> {
    let mut lines: Vec<String> = messages
        .iter()
        .filter_map(|m| match m.role.as_str() {
            ROLE_USER => Some(format!("Usuário: {}", m.content.trim())),
            ROLE_ASSISTANT => Some(format!("Assistente: {}", m.content.trim())),
            ROLE_TOOL => serde_json::from_str::<serde_json::Value>(&m.content)
                .ok()
                .and_then(|v| v["name"].as_str().map(|n| format!("[o assistente usou a ferramenta {n}]"))),
            _ => None,
        })
        .collect();
    if lines.is_empty() {
        return None;
    }
    let mut total: usize = lines.iter().map(|l| l.chars().count() + 2).sum();
    while total > MAX_TRANSCRIPT_CHARS && lines.len() > 1 {
        total -= lines.remove(0).chars().count() + 2;
    }
    Some(format!(
        "A conversa a seguir já aconteceu entre o usuário e você; continue a partir dela como se lembrasse de tudo.\n\n{}",
        lines.join("\n\n")
    ))
}

/// O que um subagente fez, passo a passo, para acompanhar o trabalho dele.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SubagentStep {
    Started {
        description: String,
        agent_type: String,
        prompt: String,
        background: bool,
    },
    /// Resumo do que ele está fazendo agora (ex.: "Lendo a.txt").
    Progress { description: String },
    Text { text: String },
    /// `input` é o JSON dos argumentos.
    ToolCall { id: String, name: String, input: String },
    ToolResult { id: String, output: String, is_error: bool },
    /// Fim do trabalho: "completed", "failed", "stopped"…
    Finished { status: String, summary: String },
}

impl SubagentStep {
    /// O passo com textos longos encurtados, para enviar à interface e gravar.
    pub fn truncated(&self) -> SubagentStep {
        let mut step = self.clone();
        if let SubagentStep::ToolResult { output, .. } = &mut step {
            *output = truncate_output(output);
        }
        step
    }

    /// O passo em JSON, como é gravado (chame sobre o passo já encurtado).
    pub fn to_record(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

fn truncate_output(text: &str) -> String {
    match text.char_indices().nth(MAX_TOOL_OUTPUT_CHARS) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_owned(),
    }
}

/// Conteúdo de uma mensagem de ferramenta: o que foi chamado e o que voltou.
/// O `id` da chamada liga a mensagem ao trabalho de um subagente.
pub fn tool_message(id: &str, name: &str, input: &str, output: Option<&str>, is_error: bool) -> String {
    let input: serde_json::Value = serde_json::from_str(input).unwrap_or(serde_json::Value::Null);
    let output = output.map(truncate_output);
    serde_json::json!({ "id": id, "name": name, "input": input, "output": output, "is_error": is_error }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: &str, content: &str) -> Message {
        Message { id: 1, role: role.into(), content: content.into(), account_id: None, meta: None, created_at: 0 }
    }

    #[test]
    fn transcript_keeps_dialogue_and_names_tools() {
        let messages = [
            message(ROLE_USER, "Oi"),
            message(ROLE_NOTICE, "troca de conta"),
            message(ROLE_TOOL, &tool_message("t1", "Read", "{}", Some("x"), false)),
            message(ROLE_ASSISTANT, "Olá!"),
        ];
        let text = transcript_prompt(&messages).unwrap();
        assert!(text.contains("Usuário: Oi\n\n[o assistente usou a ferramenta Read]\n\nAssistente: Olá!"));
        assert!(!text.contains("troca de conta"));
        assert_eq!(transcript_prompt(&[message(ROLE_NOTICE, "x")]), None);

        let long: Vec<Message> = (0..5).map(|i| message(ROLE_USER, &format!("{i}{}", "x".repeat(20_000)))).collect();
        let text = transcript_prompt(&long).unwrap();
        assert!(!text.contains("Usuário: 0") && text.contains("Usuário: 4"), "fica com o fim da conversa");
    }

    #[test]
    fn tool_message_keeps_input_as_json_and_truncates_output() {
        let long = "x".repeat(MAX_TOOL_OUTPUT_CHARS + 10);
        let v: serde_json::Value =
            serde_json::from_str(&tool_message("t1", "Bash", r#"{"command":"ls"}"#, Some(&long), false)).unwrap();
        assert_eq!(v["name"], "Bash");
        assert_eq!(v["id"], "t1");
        assert_eq!(v["input"]["command"], "ls");
        assert_eq!(v["output"].as_str().unwrap().chars().count(), MAX_TOOL_OUTPUT_CHARS + 1);
        let pending: serde_json::Value = serde_json::from_str(&tool_message("t2", "Edit", "{}", None, false)).unwrap();
        assert!(pending["output"].is_null());
    }
}
