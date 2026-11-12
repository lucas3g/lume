//! Extensões do assistente: skills, agentes e conectores (servidores MCP).

use serde::Serialize;
use std::collections::HashMap;

/// Escopo das regras que valem para qualquer conversa e qualquer agente.
pub const SCOPE_ALL: &str = "all";
pub const ORIGIN_PERSONAL: &str = "personal";
pub const ORIGIN_PLUGIN: &str = "plugin";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Skill {
    pub name: String,
    pub description: Option<String>,
    pub origin: String,
}

/// Agente: instruções próprias e um conjunto de skills além das gerais.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub instructions: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Connector {
    pub id: String,
    pub name: String,
    /// Definição do servidor MCP em JSON (`command`/`args`/`env` ou `url`).
    pub config: String,
    pub enabled: bool,
    pub status: Option<String>,
}

/// Situação de um conector carregado pelo Claude Code.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorStatus {
    Connected,
    NeedsAuth,
    Failed,
    Pending,
}

/// De onde vem um conector carregado pelo Claude Code.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorSource {
    /// Conector da conta claude.ai (serviço web vinculado por login).
    Account,
    /// Configurado pelo usuário no Claude Code.
    User,
    Plugin,
}

/// Conector que o Claude Code carrega sozinho para a conta em uso.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AccountConnector {
    /// Nome completo, como o Claude Code o chama (ex.: "claude.ai Asana").
    pub name: String,
    pub display_name: String,
    /// Prefixo das ferramentas do servidor (ex.: `claude_ai_Asana`).
    pub key: String,
    pub endpoint: String,
    pub status: ConnectorStatus,
    pub source: ConnectorSource,
    pub enabled: bool,
}

const ACCOUNT_PREFIX: &str = "claude.ai ";
const PLUGIN_PREFIX: &str = "plugin:";

/// Prefixo das configurações que desligam um conector carregado pelo Claude Code.
pub const SERVER_OFF_PREFIX: &str = "server_off:";

/// Regras que bloqueiam, nos turnos, os conectores desligados (configurações `server_off:<nome>`).
pub fn denied_server_rules(flags: &[(String, bool)]) -> Vec<String> {
    flags
        .iter()
        .filter(|(key, off)| *off && key.starts_with(SERVER_OFF_PREFIX))
        .map(|(key, _)| server_deny_rule(&key[SERVER_OFF_PREFIX.len()..]))
        .collect()
}

/// Domínio raiz de um endereço (`https://mcp.asana.com/v2` → `asana.com`); `None` para comandos locais.
pub fn root_domain(endpoint: &str) -> Option<String> {
    let rest = endpoint.trim().strip_prefix("https://").or_else(|| endpoint.trim().strip_prefix("http://"))?;
    let host = rest.split(['/', '?', ':']).next()?.to_lowercase();
    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    if labels.len() < 2 || host.parse::<std::net::IpAddr>().is_ok() || host == "localhost" {
        return None;
    }
    // Sufixos de dois níveis (com.br, co.uk): o domínio raiz tem três rótulos.
    let keep = if labels.len() >= 3 && labels[labels.len() - 2].len() <= 3 && labels[labels.len() - 1].len() == 2 { 3 } else { 2 };
    Some(labels[labels.len() - keep..].join("."))
}

/// Tipo MIME de um ícone pelo conteúdo.
pub fn icon_media_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(b"<") || bytes.starts_with(b"\xEF\xBB\xBF<") {
        "image/svg+xml"
    } else if bytes.starts_with(b"\xFF\xD8") {
        "image/jpeg"
    } else if bytes.starts_with(b"RIFF") {
        "image/webp"
    } else {
        "image/x-icon"
    }
}

/// Como o Claude Code transforma o nome de um servidor no prefixo das suas ferramentas.
pub fn cli_server_key(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect()
}

/// Regra de permissão que bloqueia todas as ferramentas de um servidor.
pub fn server_deny_rule(name: &str) -> String {
    format!("mcp__{}", cli_server_key(name))
}

/// Lê a saída de `claude mcp list`: uma linha `nome: endereço - situação` por servidor.
pub fn parse_mcp_list(text: &str) -> Vec<AccountConnector> {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.split_once(": ")?;
            let (endpoint, state) = rest.rsplit_once(" - ")?;
            let status = match state.trim() {
                s if s.contains("Connected") => ConnectorStatus::Connected,
                s if s.contains("authentication") => ConnectorStatus::NeedsAuth,
                s if s.contains("Pending") => ConnectorStatus::Pending,
                s if s.contains("Failed") || s.starts_with('✘') => ConnectorStatus::Failed,
                _ => return None,
            };
            let name = name.trim();
            let (source, display_name) = if let Some(short) = name.strip_prefix(ACCOUNT_PREFIX) {
                (ConnectorSource::Account, short.to_owned())
            } else if let Some(rest) = name.strip_prefix(PLUGIN_PREFIX) {
                (ConnectorSource::Plugin, rest.rsplit(':').next().unwrap_or(rest).to_owned())
            } else {
                (ConnectorSource::User, name.to_owned())
            };
            Some(AccountConnector {
                key: cli_server_key(name),
                name: name.to_owned(),
                display_name,
                endpoint: endpoint.trim().to_owned(),
                status,
                source,
                enabled: true,
            })
        })
        .collect()
}

/// Regras de skills de um escopo: nome da skill → ligada ou desligada.
pub type SkillRules = HashMap<String, bool>;

/// Uma skill vale para todos, salvo se desligada; para um agente, só se ligada.
pub fn is_enabled_for_all(rules: &SkillRules, skill: &str) -> bool {
    rules.get(skill).copied().unwrap_or(true)
}

pub fn is_granted_to_agent(rules: &SkillRules, skill: &str) -> bool {
    rules.get(skill).copied().unwrap_or(false)
}

/// Skills que o turno não pode usar: as desligadas para todos e não concedidas ao agente.
pub fn denied_skills(catalog: &[Skill], all: &SkillRules, agent: Option<&SkillRules>) -> Vec<String> {
    catalog
        .iter()
        .map(|s| s.name.as_str())
        .filter(|name| !is_enabled_for_all(all, name))
        .filter(|name| !agent.is_some_and(|rules| is_granted_to_agent(rules, name)))
        .map(str::to_owned)
        .collect()
}

/// Nome aceito pelo protocolo MCP para identificar o servidor e suas ferramentas.
pub fn connector_key(name: &str) -> String {
    let key: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '-' })
        .collect();
    key.trim_matches('-').to_owned()
}

/// Valida a definição de um conector: objeto JSON com `command` ou `url`.
pub fn validate_connector_config(config: &str) -> Result<(), &'static str> {
    let value: serde_json::Value = serde_json::from_str(config).map_err(|_| "A configuração não é um JSON válido.")?;
    let has = |key: &str| value.get(key).and_then(|v| v.as_str()).is_some_and(|s| !s.trim().is_empty());
    if value.is_object() && (has("command") || has("url")) {
        Ok(())
    } else {
        Err("Informe \"command\" (servidor local) ou \"url\" (servidor remoto).")
    }
}

/// Configuração MCP dos conectores ligados e as regras que liberam suas ferramentas.
pub fn mcp_setup(connectors: &[Connector]) -> (Option<String>, Vec<String>) {
    let mut servers = serde_json::Map::new();
    let mut allowed = vec![];
    for c in connectors.iter().filter(|c| c.enabled) {
        let Ok(config) = serde_json::from_str::<serde_json::Value>(&c.config) else { continue };
        let key = connector_key(&c.name);
        allowed.push(format!("mcp__{key}"));
        servers.insert(key, config);
    }
    if servers.is_empty() {
        return (None, vec![]);
    }
    (Some(serde_json::json!({ "mcpServers": servers }).to_string()), allowed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(name: &str) -> Skill {
        Skill { name: name.into(), description: None, origin: ORIGIN_PERSONAL.into() }
    }

    #[test]
    fn denies_skills_off_for_all_unless_granted_to_agent() {
        let catalog = vec![skill("revisar"), skill("pdf"), skill("planilha")];
        let all = SkillRules::from([("pdf".into(), false), ("planilha".into(), false)]);
        assert_eq!(denied_skills(&catalog, &all, None), ["pdf", "planilha"]);

        let agent = SkillRules::from([("pdf".into(), true)]);
        assert_eq!(denied_skills(&catalog, &all, Some(&agent)), ["planilha"]);
        assert!(denied_skills(&catalog, &SkillRules::new(), None).is_empty(), "sem regras, tudo liberado");
    }

    #[test]
    fn parses_claude_code_server_list() {
        let text = "Checking MCP server health…\n\nclaude.ai Claude Data: https://api.anthropic.com/mcp - ✔ Connected\nclaude.ai monday.com: https://mcp.monday.com/mcp - ! Needs authentication\nplugin:context7:context7: https://mcp.context7.com/mcp?client=x (HTTP) - ! Needs authentication\nrea: npx -y rea-agents@6.4.0 mcp - ✔ Connected\nquebrado: https://x - ✘ Failed to connect\nlinha sem formato\n";
        let list = parse_mcp_list(text);
        assert_eq!(list.len(), 5);
        assert_eq!((list[0].display_name.as_str(), list[0].source, list[0].status, list[0].key.as_str()), ("Claude Data", ConnectorSource::Account, ConnectorStatus::Connected, "claude_ai_Claude_Data"));
        assert_eq!((list[1].display_name.as_str(), list[1].status, list[1].key.as_str()), ("monday.com", ConnectorStatus::NeedsAuth, "claude_ai_monday_com"));
        assert_eq!((list[2].display_name.as_str(), list[2].source), ("context7", ConnectorSource::Plugin));
        assert_eq!((list[3].source, list[3].endpoint.as_str()), (ConnectorSource::User, "npx -y rea-agents@6.4.0 mcp"));
        assert_eq!(list[4].status, ConnectorStatus::Failed);
        assert_eq!(server_deny_rule("claude.ai Asana"), "mcp__claude_ai_Asana");
        assert_eq!(root_domain("https://mcp.asana.com/v2/mcp").as_deref(), Some("asana.com"));
        assert_eq!(root_domain("https://slidesmcp.googleapis.com/mcp/v1").as_deref(), Some("googleapis.com"));
        assert_eq!(root_domain("https://api.empresa.com.br:8443/x").as_deref(), Some("empresa.com.br"));
        assert_eq!(root_domain("npx -y rea-agents mcp"), None);
        assert_eq!(root_domain("http://localhost:3000/mcp"), None);
        assert_eq!(icon_media_type(b"\x89PNG\r\n"), "image/png");
        assert_eq!(icon_media_type(b"<svg/>"), "image/svg+xml");
        assert_eq!(icon_media_type(b"\x00\x00\x01\x00"), "image/x-icon");
        let flags = vec![("server_off:claude.ai Asana".to_owned(), true), ("server_off:rea".to_owned(), false), ("auto_switch".to_owned(), true)];
        assert_eq!(denied_server_rules(&flags), ["mcp__claude_ai_Asana"]);
    }

    #[test]
    fn builds_mcp_config_only_for_enabled_connectors() {
        let connector = |name: &str, enabled| Connector {
            id: name.into(),
            name: name.into(),
            config: r#"{"command":"npx","args":["x"]}"#.into(),
            enabled,
            status: None,
        };
        let (config, allowed) = mcp_setup(&[connector("Arquivos locais", true), connector("off", false)]);
        assert_eq!(allowed, ["mcp__arquivos-locais"]);
        let parsed: serde_json::Value = serde_json::from_str(&config.unwrap()).unwrap();
        assert_eq!(parsed["mcpServers"]["arquivos-locais"]["command"], "npx");
        assert!(parsed["mcpServers"].get("off").is_none());
        assert_eq!(mcp_setup(&[connector("off", false)]), (None, vec![]));
    }

    #[test]
    fn validates_connector_config() {
        assert!(validate_connector_config(r#"{"command":"npx"}"#).is_ok());
        assert!(validate_connector_config(r#"{"url":"https://exemplo.com/mcp"}"#).is_ok());
        assert!(validate_connector_config(r#"{"args":[]}"#).is_err());
        assert!(validate_connector_config("não é json").is_err());
    }
}
