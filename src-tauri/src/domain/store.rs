//! Lojas: catálogo de plugins (skills) e diretório de servidores MCP (conectores).

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Plugin de um marketplace do Claude Code. O `id` é `nome@marketplace`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PluginListing {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub author: Option<String>,
    pub category: Option<String>,
    pub marketplace: String,
    pub homepage: Option<String>,
    /// Quantas skills o catálogo declara; `None` quando não informa.
    pub skill_count: Option<usize>,
    pub installed: bool,
    pub enabled: bool,
}

/// Onde o valor informado pelo usuário entra na configuração do conector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "place", rename_all = "snake_case")]
pub enum InputPlace {
    Header,
    Env,
    /// Argumento posicional da linha de comando.
    Positional,
    /// Argumento nomeado (`--nome valor`).
    Named { flag: String },
}

/// Valor que o usuário precisa (ou pode) informar para usar um servidor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerInput {
    pub name: String,
    pub description: String,
    pub secret: bool,
    pub required: bool,
    #[serde(flatten)]
    pub place: InputPlace,
}

/// Como o servidor é executado ou alcançado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerInstall {
    /// Servidor remoto por HTTP; `sse` para o transporte antigo.
    Remote { url: String, sse: bool },
    /// Pacote npm executado com `npx`.
    Npm { package: String, version: Option<String>, args: Vec<String> },
    /// Pacote Python executado com `uvx`.
    Pypi { package: String, version: Option<String>, args: Vec<String> },
    /// Formato que o app não sabe instalar (docker, binários etc.).
    Unsupported { reason: String },
}

/// Servidor MCP do diretório público.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerListing {
    /// Identificador no registro, ex.: `io.github.autor/servidor`.
    pub name: String,
    pub title: String,
    pub description: String,
    pub website: Option<String>,
    pub install: ServerInstall,
    pub inputs: Vec<ServerInput>,
}

impl ServerListing {
    pub fn is_installable(&self) -> bool {
        !matches!(self.install, ServerInstall::Unsupported { .. })
    }
}

/// Nome curto para o conector: o título, ou o último trecho do identificador.
pub fn connector_name(listing: &ServerListing) -> String {
    let title = listing.title.trim();
    if !title.is_empty() {
        return title.to_owned();
    }
    listing.name.rsplit('/').next().unwrap_or(&listing.name).to_owned()
}

/// Monta a configuração MCP (JSON) do servidor com os valores informados pelo usuário.
/// Valores obrigatórios ausentes impedem a montagem.
pub fn server_config(listing: &ServerListing, values: &HashMap<String, String>) -> Result<String, String> {
    let value_of = |input: &ServerInput| values.get(&input.name).map(|v| v.trim()).filter(|v| !v.is_empty());
    if let Some(missing) = listing.inputs.iter().find(|i| i.required && value_of(i).is_none()) {
        return Err(format!("Informe \"{}\".", missing.name));
    }
    let mut headers = Map::new();
    let mut env = Map::new();
    let mut extra_args: Vec<String> = vec![];
    for input in &listing.inputs {
        let Some(value) = value_of(input) else { continue };
        match &input.place {
            InputPlace::Header => {
                headers.insert(input.name.clone(), json!(value));
            }
            InputPlace::Env => {
                env.insert(input.name.clone(), json!(value));
            }
            InputPlace::Positional => extra_args.push(value.to_owned()),
            InputPlace::Named { flag } => {
                extra_args.push(flag.clone());
                extra_args.push(value.to_owned());
            }
        }
    }

    let config = match &listing.install {
        ServerInstall::Remote { url, sse } => {
            let mut config = json!({ "type": if *sse { "sse" } else { "http" }, "url": url });
            if !headers.is_empty() {
                config["headers"] = Value::Object(headers);
            }
            config
        }
        ServerInstall::Npm { package, version, args } => {
            let spec = match version {
                Some(v) => format!("{package}@{v}"),
                None => package.clone(),
            };
            let mut all = vec!["-y".to_owned(), spec];
            all.extend(args.iter().cloned());
            all.extend(extra_args);
            command_config("npx", all, env)
        }
        ServerInstall::Pypi { package, version, args } => {
            let spec = match version {
                Some(v) => format!("{package}@{v}"),
                None => package.clone(),
            };
            let mut all = vec![spec];
            all.extend(args.iter().cloned());
            all.extend(extra_args);
            command_config("uvx", all, env)
        }
        ServerInstall::Unsupported { reason } => return Err(reason.clone()),
    };
    Ok(config.to_string())
}

fn command_config(command: &str, args: Vec<String>, env: Map<String, Value>) -> Value {
    let mut config = json!({ "command": command, "args": args });
    if !env.is_empty() {
        config["env"] = Value::Object(env);
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str, required: bool, place: InputPlace) -> ServerInput {
        ServerInput { name: name.into(), description: String::new(), secret: true, required, place }
    }

    fn listing(install: ServerInstall, inputs: Vec<ServerInput>) -> ServerListing {
        ServerListing {
            name: "io.github.autor/servidor".into(),
            title: String::new(),
            description: String::new(),
            website: None,
            install,
            inputs,
        }
    }

    #[test]
    fn names_connector_after_title_or_last_segment() {
        let mut l = listing(ServerInstall::Remote { url: "https://x".into(), sse: false }, vec![]);
        assert_eq!(connector_name(&l), "servidor");
        l.title = " GitHub ".into();
        assert_eq!(connector_name(&l), "GitHub");
    }

    #[test]
    fn builds_remote_config_with_headers() {
        let l = listing(
            ServerInstall::Remote { url: "https://mcp.exemplo.com/mcp".into(), sse: false },
            vec![input("Authorization", true, InputPlace::Header), input("X-Opcional", false, InputPlace::Header)],
        );
        assert_eq!(server_config(&l, &HashMap::new()), Err("Informe \"Authorization\".".into()));
        let values = HashMap::from([("Authorization".to_owned(), " Bearer abc ".to_owned()), ("X-Opcional".to_owned(), "".to_owned())]);
        let v: Value = serde_json::from_str(&server_config(&l, &values).unwrap()).unwrap();
        assert_eq!(v["type"], "http");
        assert_eq!(v["url"], "https://mcp.exemplo.com/mcp");
        assert_eq!(v["headers"]["Authorization"], "Bearer abc");
        assert!(v["headers"].get("X-Opcional").is_none(), "vazio não entra");
    }

    #[test]
    fn builds_npm_config_with_env_and_arguments() {
        let l = listing(
            ServerInstall::Npm { package: "@acme/mcp".into(), version: Some("1.2.0".into()), args: vec!["serve".into()] },
            vec![
                input("API_KEY", true, InputPlace::Env),
                input("pasta", true, InputPlace::Positional),
                input("porta", false, InputPlace::Named { flag: "--port".into() }),
            ],
        );
        let values = HashMap::from([
            ("API_KEY".to_owned(), "k".to_owned()),
            ("pasta".to_owned(), "/tmp".to_owned()),
            ("porta".to_owned(), "8080".to_owned()),
        ]);
        let v: Value = serde_json::from_str(&server_config(&l, &values).unwrap()).unwrap();
        assert_eq!(v["command"], "npx");
        assert_eq!(v["args"], json!(["-y", "@acme/mcp@1.2.0", "serve", "/tmp", "--port", "8080"]));
        assert_eq!(v["env"]["API_KEY"], "k");
    }

    #[test]
    fn pypi_uses_uvx_and_unsupported_is_refused() {
        let l = listing(ServerInstall::Pypi { package: "mcp-x".into(), version: None, args: vec![] }, vec![]);
        let v: Value = serde_json::from_str(&server_config(&l, &HashMap::new()).unwrap()).unwrap();
        assert_eq!((v["command"].as_str(), v["args"][0].as_str()), (Some("uvx"), Some("mcp-x")));
        assert!(v.get("env").is_none());

        let u = listing(ServerInstall::Unsupported { reason: "docker".into() }, vec![]);
        assert!(!u.is_installable());
        assert_eq!(server_config(&u, &HashMap::new()), Err("docker".into()));
    }
}
