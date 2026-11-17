//! Conectores (servidores MCP) levados ao Codex (ChatGPT) e ao Antigravity (Gemini).
//!
//! O Claude Code recebe os conectores do Lume por `--mcp-config` e os da conta claude.ai sozinho.
//! Os outros serviços não enxergam nada disso:
//! - o Codex recebe, a cada turno, os servidores por `--config mcp_servers.<nome>…` (TOML);
//! - o Antigravity só lê um arquivo global, mesclado pelo Lume sem tocar nos servidores do usuário.
//!
//! Os conectores da conta claude.ai não saem da conta do Claude, mas o endereço público deles
//! (ex.: o servidor MCP do Atlassian) pode ser usado nos outros serviços, com login próprio em
//! cada um: são as "pontes".

use super::extension::{connector_key, root_domain, Connector};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// Prefixo dos servidores que o Lume põe nos outros serviços: não colide com os do usuário.
pub const SERVER_PREFIX: &str = "lume-";

#[derive(Debug, Clone, PartialEq)]
pub enum McpTransport {
    Stdio { command: String, args: Vec<String>, env: BTreeMap<String, String> },
    Http { url: String, headers: BTreeMap<String, String> },
}

/// Quanto o serviço libera sem perguntar. Fora do terminal nenhum dos dois pergunta: o que
/// precisaria de aprovação é cancelado.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Approval {
    /// Tudo liberado (conectores do Lume, como no Claude, e pontes liberadas para a conversa).
    All,
    /// Só o que o servidor marca como leitura; o que escreve fica bloqueado.
    ReadOnly,
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpServer {
    /// Nome no serviço (`lume-<chave>`): as ferramentas aparecem como `mcp__lume-<chave>__…`.
    pub name: String,
    pub transport: McpTransport,
    pub approval: Approval,
}

/// Conector da conta claude.ai usado também no ChatGPT e no Gemini, pelo endereço público.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bridge {
    /// Nome no Claude Code (ex.: "claude.ai Atlassian").
    pub name: String,
    /// Nome curto (ex.: "Atlassian").
    pub display_name: String,
    pub endpoint: String,
}

impl Bridge {
    pub fn server_name(&self) -> String {
        server_name(&self.display_name)
    }

    /// Ponte para o Jira: o Relator da esteira escreve nela (comenta e muda o status da task).
    pub fn is_jira(&self) -> bool {
        root_domain(&self.endpoint).as_deref() == Some("atlassian.com")
    }
}

pub fn server_name(name: &str) -> String {
    format!("{SERVER_PREFIX}{}", connector_key(name))
}

/// Regra de permissão (no estilo da conversa) que libera tudo de um servidor nos outros serviços.
pub fn server_rule(name: &str) -> String {
    format!("mcp__{name}")
}

fn string_map(value: Option<&Value>) -> BTreeMap<String, String> {
    value
        .and_then(Value::as_object)
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned()))).collect())
        .unwrap_or_default()
}

/// Lê a definição JSON de um conector (`command`/`args`/`env` ou `url`/`headers`).
pub fn parse_transport(config: &str) -> Option<McpTransport> {
    let v: Value = serde_json::from_str(config).ok()?;
    let text = |key: &str| v.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
    if let Some(command) = text("command") {
        let args = v.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default();
        return Some(McpTransport::Stdio { command, args, env: string_map(v.get("env")) });
    }
    let url = text("url").or_else(|| text("serverUrl"))?;
    (url.starts_with("https://") || url.starts_with("http://")).then(|| McpTransport::Http { url, headers: string_map(v.get("headers")) })
}

/// Servidores de um turno no ChatGPT ou no Gemini: os conectores ligados do Lume (liberados, como
/// no Claude) e as pontes. Uma ponte libera tudo só quando a conversa a liberou (`allowed`, ex.: o
/// Relator da esteira) ou roda sem pedir permissão; senão, só leitura.
pub fn turn_servers(connectors: &[Connector], bridges: &[Bridge], allowed: &[String], permission_mode: &str) -> Vec<McpServer> {
    let own = connectors.iter().filter(|c| c.enabled).filter_map(|c| {
        Some(McpServer { name: server_name(&c.name), transport: parse_transport(&c.config)?, approval: Approval::All })
    });
    let bridged = bridges.iter().map(|b| {
        let name = b.server_name();
        let open = permission_mode == "bypassPermissions" || allowed.iter().any(|t| *t == server_rule(&name));
        McpServer {
            transport: McpTransport::Http { url: b.endpoint.clone(), headers: BTreeMap::new() },
            approval: if open { Approval::All } else { Approval::ReadOnly },
            name,
        }
    });
    let mut out: Vec<McpServer> = Vec::new();
    for server in own.chain(bridged) {
        // Um conector do Lume com o mesmo nome de uma ponte vence: é o que o usuário configurou.
        if !out.iter().any(|s| s.name == server.name) {
            out.push(server);
        }
    }
    out
}

/// Texto TOML entre aspas, com escapes.
pub fn toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn toml_array(items: &[String]) -> String {
    format!("[{}]", items.iter().map(|i| toml_string(i)).collect::<Vec<_>>().join(", "))
}

/// Tabela inline; as chaves também vão entre aspas, porque vêm do usuário.
fn toml_table(map: &BTreeMap<String, String>) -> String {
    let pairs: Vec<String> = map.iter().map(|(k, v)| format!("{} = {}", toml_string(k), toml_string(v))).collect();
    format!("{{ {} }}", pairs.join(", "))
}

/// Sobrescritas `--config` do Codex que definem os servidores do turno.
pub fn codex_overrides(servers: &[McpServer]) -> Vec<String> {
    let mut out = Vec::new();
    for s in servers {
        // `connector_key` deixa o nome só com letras, números, `-` e `_`: vale como chave TOML sem aspas.
        let base = format!("mcp_servers.{}", s.name);
        match &s.transport {
            McpTransport::Stdio { command, args, env } => {
                out.push(format!("{base}.command={}", toml_string(command)));
                out.push(format!("{base}.args={}", toml_array(args)));
                if !env.is_empty() {
                    out.push(format!("{base}.env={}", toml_table(env)));
                }
            }
            McpTransport::Http { url, headers } => {
                out.push(format!("{base}.url={}", toml_string(url)));
                if !headers.is_empty() {
                    out.push(format!("{base}.http_headers={}", toml_table(headers)));
                }
            }
        }
        let mode = match s.approval {
            Approval::All => "approve",
            Approval::ReadOnly => "writes",
        };
        out.push(format!("{base}.default_tools_approval_mode={}", toml_string(mode)));
    }
    out
}

/// Ferramentas de leitura do servidor MCP do Atlassian, liberadas no Gemini (que só aceita
/// liberar por nome; o resto pediria aprovação e é recusado fora do terminal).
pub const ATLASSIAN_READ_TOOLS: [&str; 12] = [
    "atlassianUserInfo",
    "getAccessibleAtlassianResources",
    "getJiraIssue",
    "searchJiraIssuesUsingJql",
    "listJiraIssueComments",
    "getTransitionsForJiraIssue",
    "getVisibleJiraProjects",
    "getJiraProjectIssueTypesMetadata",
    "getConfluencePage",
    "getConfluenceContent",
    "searchConfluenceUsingCql",
    "search",
];

/// Mescla os servidores do Lume no `mcp_config.json` do Antigravity: tira os `lume-*` antigos e
/// põe os atuais, sem tocar nos do usuário. `None` quando o arquivo existente não é um JSON legível
/// (melhor não mexer do que apagar a configuração do usuário).
pub fn antigravity_config(existing: Option<&str>, servers: &[McpServer]) -> Option<String> {
    let mut root: Map<String, Value> = match existing.map(str::trim).filter(|t| !t.is_empty()) {
        Some(text) => match serde_json::from_str(text).ok()? {
            Value::Object(map) => map,
            _ => return None,
        },
        None => Map::new(),
    };
    let mut list = match root.remove("mcpServers") {
        Some(Value::Object(map)) => map,
        Some(_) => return None,
        None => Map::new(),
    };
    list.retain(|name, _| !name.starts_with(SERVER_PREFIX));
    for s in servers {
        let entry = match &s.transport {
            McpTransport::Stdio { command, args, env } => {
                let mut e = json!({ "command": command, "args": args });
                if !env.is_empty() {
                    e["env"] = json!(env);
                }
                e
            }
            McpTransport::Http { url, headers } => {
                let mut e = json!({ "serverUrl": url });
                if !headers.is_empty() {
                    e["headers"] = json!(headers);
                }
                e
            }
        };
        list.insert(s.name.clone(), entry);
    }
    root.insert("mcpServers".into(), Value::Object(list));
    serde_json::to_string_pretty(&Value::Object(root)).ok()
}

/// Regras `permissions.allow` do Antigravity para os servidores do Lume: tudo dos conectores do
/// Lume; das pontes, só a leitura do Atlassian (as outras pedem aprovação, recusada fora do terminal).
pub fn antigravity_allow_rules(servers: &[McpServer], bridges: &[Bridge]) -> Vec<String> {
    let mut rules = Vec::new();
    for s in servers {
        match bridges.iter().find(|b| b.server_name() == s.name) {
            None => rules.push(format!("mcp({}/*)", s.name)),
            Some(b) if b.is_jira() => rules.extend(ATLASSIAN_READ_TOOLS.iter().map(|t| format!("mcp({}/{t})", s.name))),
            Some(_) => {}
        }
    }
    rules
}

/// Mescla as regras do Lume em `permissions.allow` das configurações do Antigravity, tirando as
/// antigas (`mcp(lume-…)`) e mantendo as do usuário. `None` quando o arquivo não é legível.
pub fn antigravity_settings(existing: Option<&str>, rules: &[String]) -> Option<String> {
    let mut root: Map<String, Value> = match existing.map(str::trim).filter(|t| !t.is_empty()) {
        Some(text) => match serde_json::from_str(text).ok()? {
            Value::Object(map) => map,
            _ => return None,
        },
        None => Map::new(),
    };
    let mut permissions = match root.remove("permissions") {
        Some(Value::Object(map)) => map,
        Some(_) => return None,
        None => Map::new(),
    };
    let ours = format!("mcp({SERVER_PREFIX}");
    let mut allow: Vec<Value> = match permissions.remove("allow") {
        Some(Value::Array(list)) => list,
        Some(_) => return None,
        None => vec![],
    };
    allow.retain(|r| !r.as_str().is_some_and(|r| r.starts_with(&ours)));
    allow.extend(rules.iter().map(|r| Value::String(r.clone())));
    permissions.insert("allow".into(), Value::Array(allow));
    root.insert("permissions".into(), Value::Object(permissions));
    serde_json::to_string_pretty(&Value::Object(root)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connector(name: &str, config: &str, enabled: bool) -> Connector {
        Connector { id: name.into(), name: name.into(), config: config.into(), enabled, status: None }
    }

    fn atlassian() -> Bridge {
        Bridge { name: "claude.ai Atlassian".into(), display_name: "Atlassian".into(), endpoint: "https://mcp.atlassian.com/v1/mcp".into() }
    }

    #[test]
    fn turn_servers_join_lume_connectors_and_bridges() {
        let connectors = [
            connector("Arquivos", r#"{"command":"npx","args":["-y","fs"],"env":{"A":"1"}}"#, true),
            connector("Desligado", r#"{"command":"x"}"#, false),
            connector("Quebrado", r#"{"nada":1}"#, true),
        ];
        let servers = turn_servers(&connectors, &[atlassian()], &[], "manual");
        let names: Vec<_> = servers.iter().map(|s| (s.name.as_str(), s.approval)).collect();
        assert_eq!(names, [("lume-arquivos", Approval::All), ("lume-atlassian", Approval::ReadOnly)]);
        let open = turn_servers(&[], &[atlassian()], &["mcp__lume-atlassian".into()], "manual");
        assert_eq!(open[0].approval, Approval::All, "a conversa liberou a ponte (ex.: o Relator)");
        assert_eq!(turn_servers(&[], &[atlassian()], &[], "bypassPermissions")[0].approval, Approval::All);
    }

    #[test]
    fn codex_overrides_are_valid_toml_values() {
        let servers = turn_servers(
            &[connector("Arquivos", r#"{"command":"C:\\bin\\fs \"x\"","args":["a\nb"],"env":{"TOKEN":"t\"1"}}"#, true)],
            &[atlassian()],
            &[],
            "manual",
        );
        let o = codex_overrides(&servers);
        assert_eq!(o[0], r#"mcp_servers.lume-arquivos.command="C:\\bin\\fs \"x\"""#);
        assert_eq!(o[1], r#"mcp_servers.lume-arquivos.args=["a\nb"]"#);
        assert_eq!(o[2], r#"mcp_servers.lume-arquivos.env={ "TOKEN" = "t\"1" }"#);
        assert_eq!(o[3], r#"mcp_servers.lume-arquivos.default_tools_approval_mode="approve""#);
        assert_eq!(o[4], r#"mcp_servers.lume-atlassian.url="https://mcp.atlassian.com/v1/mcp""#);
        assert_eq!(o[5], r#"mcp_servers.lume-atlassian.default_tools_approval_mode="writes""#);
        // Cada valor é TOML válido.
        for line in &o {
            let (_, value) = line.split_once('=').unwrap();
            assert!(format!("v = {value}").parse::<toml::Table>().is_ok(), "{line}");
        }
    }

    #[test]
    fn antigravity_config_keeps_the_user_servers() {
        let existing = r#"{"mcpServers":{"meu":{"command":"x"},"lume-velho":{"command":"y"}},"outra":1}"#;
        let servers = turn_servers(&[connector("Web", r#"{"url":"https://a.com/mcp","headers":{"Authorization":"Bearer k"}}"#, true)], &[], &[], "manual");
        let merged: Value = serde_json::from_str(&antigravity_config(Some(existing), &servers).unwrap()).unwrap();
        assert_eq!(merged["mcpServers"]["meu"]["command"], "x");
        assert!(merged["mcpServers"].get("lume-velho").is_none(), "o que o Lume não tem mais sai");
        assert_eq!(merged["mcpServers"]["lume-web"]["serverUrl"], "https://a.com/mcp");
        assert_eq!(merged["mcpServers"]["lume-web"]["headers"]["Authorization"], "Bearer k");
        assert_eq!(merged["outra"], 1);
        assert!(antigravity_config(Some("{quebrado"), &servers).is_none(), "arquivo ilegível não é sobrescrito");
        assert!(antigravity_config(None, &[]).unwrap().contains("mcpServers"));
    }

    #[test]
    fn antigravity_rules_open_lume_connectors_and_only_atlassian_reads() {
        let gmail = Bridge { name: "claude.ai Gmail".into(), display_name: "Gmail".into(), endpoint: "https://gmail.mcp.claude.com/mcp".into() };
        let servers = turn_servers(&[connector("Web", r#"{"url":"https://a.com/mcp"}"#, true)], &[atlassian(), gmail.clone()], &[], "manual");
        let rules = antigravity_allow_rules(&servers, &[atlassian(), gmail]);
        assert_eq!(rules[0], "mcp(lume-web/*)");
        assert!(rules.contains(&"mcp(lume-atlassian/getJiraIssue)".into()));
        assert!(!rules.iter().any(|r| r.contains("editJiraIssue") || r.contains("lume-gmail")));

        let settings = antigravity_settings(Some(r#"{"permissions":{"allow":["mcp(meu/*)","mcp(lume-velho/*)"],"deny":["x"]},"tema":"escuro"}"#), &rules).unwrap();
        let v: Value = serde_json::from_str(&settings).unwrap();
        let allow: Vec<&str> = v["permissions"]["allow"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert_eq!(allow[0], "mcp(meu/*)");
        assert!(!allow.contains(&"mcp(lume-velho/*)"));
        assert_eq!((v["permissions"]["deny"][0].as_str(), v["tema"].as_str()), (Some("x"), Some("escuro")));
    }
}
