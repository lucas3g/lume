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
