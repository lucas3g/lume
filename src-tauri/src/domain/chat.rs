//! Conversas, mensagens e o ciclo de um turno com o modelo.

use serde::Serialize;

pub const ROLE_USER: &str = "user";
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
    Tool(String),
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
    /// Configuração MCP (JSON) dos conectores ligados.
    pub mcp_config: Option<String>,
    /// Regras que liberam as ferramentas dos conectores.
    pub allowed_tools: Vec<String>,
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

/// Resposta do usuário a um pedido de autorização.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionDecision {
    pub request_id: String,
    pub allow: bool,
}

/// Conteúdo de uma mensagem de ferramenta: o que foi chamado e o que voltou.
pub fn tool_message(name: &str, input: &str, output: Option<&str>, is_error: bool) -> String {
    let input: serde_json::Value = serde_json::from_str(input).unwrap_or(serde_json::Value::Null);
    let output = output.map(|text| match text.char_indices().nth(MAX_TOOL_OUTPUT_CHARS) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_owned(),
    });
    serde_json::json!({ "name": name, "input": input, "output": output, "is_error": is_error }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_message_keeps_input_as_json_and_truncates_output() {
        let long = "x".repeat(MAX_TOOL_OUTPUT_CHARS + 10);
        let v: serde_json::Value =
            serde_json::from_str(&tool_message("Bash", r#"{"command":"ls"}"#, Some(&long), false)).unwrap();
        assert_eq!(v["name"], "Bash");
        assert_eq!(v["input"]["command"], "ls");
        assert_eq!(v["output"].as_str().unwrap().chars().count(), MAX_TOOL_OUTPUT_CHARS + 1);
        let pending: serde_json::Value = serde_json::from_str(&tool_message("Edit", "{}", None, false)).unwrap();
        assert!(pending["output"].is_null());
    }
}
