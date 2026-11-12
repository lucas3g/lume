//! Diretório público de servidores MCP (registry.modelcontextprotocol.io).

use crate::application::{ports::ServerDirectory, AppError, AppResult};
use crate::domain::store::{InputPlace, ServerInput, ServerInstall, ServerListing};
use serde_json::Value;

const REGISTRY_URL: &str = "https://registry.modelcontextprotocol.io/v0/servers";
const PAGE_SIZE: usize = 30;
const TIMEOUT_SECS: u64 = 20;

pub struct McpRegistry;

/// Lê `environmentVariables`/`headers`/`packageArguments` como campos para o usuário preencher.
fn inputs_of(list: Option<&Vec<Value>>, place: impl Fn(&Value) -> Option<InputPlace>) -> Vec<ServerInput> {
    list.map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|v| {
            Some(ServerInput {
                name: v["name"].as_str()?.to_owned(),
                description: v["description"].as_str().unwrap_or_default().to_owned(),
                secret: v["isSecret"].as_bool().unwrap_or(false),
                required: v["isRequired"].as_bool().unwrap_or(false),
                place: place(v)?,
            })
        })
        .collect()
}

/// Argumentos do pacote: os fixos entram direto; os sem valor viram campos a preencher.
fn package_arguments(package: &Value) -> (Vec<String>, Vec<ServerInput>) {
    let mut fixed = vec![];
    let mut inputs = vec![];
    for arg in package["packageArguments"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let named = arg["type"] == "named";
        let flag = arg["name"].as_str().unwrap_or_default().to_owned();
        match arg["value"].as_str().filter(|v| !v.is_empty()) {
            Some(value) => {
                if named {
                    fixed.push(flag);
                }
                fixed.push(value.to_owned());
            }
            None => {
                let label = if named { flag.trim_start_matches('-').to_owned() } else { arg["valueHint"].as_str().unwrap_or("valor").to_owned() };
                inputs.push(ServerInput {
                    name: label,
                    description: arg["description"].as_str().unwrap_or_default().to_owned(),
                    secret: arg["isSecret"].as_bool().unwrap_or(false),
                    required: arg["isRequired"].as_bool().unwrap_or(false),
                    place: if named { InputPlace::Named { flag } } else { InputPlace::Positional },
                });
            }
        }
    }
    (fixed, inputs)
}

/// Converte uma entrada do registro: prefere o acesso remoto, depois npm, depois pypi.
pub fn parse_server(entry: &Value) -> Option<ServerListing> {
    let server = if entry.get("server").is_some() { &entry["server"] } else { entry };
    let name = server["name"].as_str()?.to_owned();
    let remotes = server["remotes"].as_array().map(Vec::as_slice).unwrap_or_default();
    let packages = server["packages"].as_array().map(Vec::as_slice).unwrap_or_default();

    let (install, inputs) = if let Some(remote) = remotes.iter().find(|r| r["url"].is_string()) {
        let sse = remote["type"].as_str().is_some_and(|t| t.contains("sse"));
        let inputs = inputs_of(remote["headers"].as_array(), |_| Some(InputPlace::Header));
        (ServerInstall::Remote { url: remote["url"].as_str().unwrap_or_default().to_owned(), sse }, inputs)
    } else if let Some(package) = packages.iter().find(|p| matches!(p["registryType"].as_str(), Some("npm" | "pypi"))) {
        let (args, mut inputs) = package_arguments(package);
        inputs.extend(inputs_of(package["environmentVariables"].as_array(), |_| Some(InputPlace::Env)));
        let identifier = package["identifier"].as_str().unwrap_or_default().to_owned();
        let version = package["version"].as_str().map(Into::into);
        let install = if package["registryType"] == "npm" {
            ServerInstall::Npm { package: identifier, version, args }
        } else {
            ServerInstall::Pypi { package: identifier, version, args }
        };
        (install, inputs)
    } else {
        let kinds: Vec<&str> = packages.iter().filter_map(|p| p["registryType"].as_str()).collect();
        let reason = if kinds.is_empty() {
            "Este servidor não informa como ser instalado.".to_owned()
        } else {
            format!("Instalação por {} ainda não é suportada pelo app.", kinds.join(", "))
        };
        (ServerInstall::Unsupported { reason }, vec![])
    };

    Some(ServerListing {
        title: server["title"].as_str().unwrap_or_default().to_owned(),
        description: server["description"].as_str().unwrap_or_default().to_owned(),
        website: server["websiteUrl"].as_str().or(server["repository"]["url"].as_str()).map(Into::into),
        name,
        install,
        inputs,
    })
}

/// Página do registro: servidores e o cursor da próxima.
pub fn parse_page(json: &Value) -> (Vec<ServerListing>, Option<String>) {
    let servers = json["servers"].as_array().map(Vec::as_slice).unwrap_or_default();
    // Só a versão mais recente de cada servidor interessa.
    let listings = servers
        .iter()
        .filter(|s| s["_meta"]["io.modelcontextprotocol.registry/official"]["isLatest"].as_bool() != Some(false))
        .filter_map(parse_server)
        .collect();
    (listings, json["metadata"]["nextCursor"].as_str().map(Into::into))
}

impl ServerDirectory for McpRegistry {
    fn search(&self, query: &str, cursor: Option<&str>) -> AppResult<(Vec<ServerListing>, Option<String>)> {
        let mut request = ureq::get(REGISTRY_URL)
            .config()
            .timeout_global(Some(std::time::Duration::from_secs(TIMEOUT_SECS)))
            .build()
            .query("limit", PAGE_SIZE.to_string());
        if !query.is_empty() {
            request = request.query("search", query);
        }
        if let Some(cursor) = cursor {
            request = request.query("cursor", cursor);
        }
        let json: Value = request
            .call()
            .map_err(|e| AppError(format!("Não foi possível consultar o diretório de servidores: {e}")))?
            .body_mut()
            .read_json()
            .map_err(|e| AppError(format!("Resposta inesperada do diretório: {e}")))?;
        Ok(parse_page(&json))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Entradas no formato do registro (schema 2025-12-11), reduzidas.
    const PAGE: &str = r#"{"servers":[
      {"server":{"name":"com.exemplo/remoto","title":"Remoto","description":"d","websiteUrl":"https://r.com",
        "remotes":[{"type":"streamable-http","url":"https://r.com/mcp","headers":[{"name":"Authorization","description":"Bearer","isSecret":true,"isRequired":true}]}]},
       "_meta":{"io.modelcontextprotocol.registry/official":{"isLatest":true}}},
      {"server":{"name":"com.exemplo/npm","description":"n","repository":{"url":"https://github.com/x/y"},
        "packages":[{"registryType":"npm","identifier":"@x/mcp","version":"2.0.0","transport":{"type":"stdio"},
          "packageArguments":[{"type":"positional","value":"serve"},{"type":"named","name":"--root","valueHint":"pasta","isRequired":true},{"type":"positional","valueHint":"perfil"}],
          "environmentVariables":[{"name":"API_KEY","isRequired":true,"isSecret":true}]}]}},
      {"server":{"name":"com.exemplo/docker","packages":[{"registryType":"oci","identifier":"img"}]}},
      {"server":{"name":"com.exemplo/antigo","remotes":[{"type":"sse","url":"https://a.com/sse"}]},
       "_meta":{"io.modelcontextprotocol.registry/official":{"isLatest":false}}}
    ],"metadata":{"nextCursor":"com.exemplo/npm:2.0.0","count":4}}"#;

    #[test]
    fn parses_remote_npm_and_unsupported_entries() {
        let (servers, cursor) = parse_page(&serde_json::from_str(PAGE).unwrap());
        assert_eq!(cursor.as_deref(), Some("com.exemplo/npm:2.0.0"));
        assert_eq!(servers.len(), 3, "versão antiga fica de fora");

        let remote = &servers[0];
        assert_eq!(remote.install, ServerInstall::Remote { url: "https://r.com/mcp".into(), sse: false });
        assert_eq!(remote.inputs.len(), 1);
        assert_eq!((remote.inputs[0].name.as_str(), remote.inputs[0].required, &remote.inputs[0].place), ("Authorization", true, &InputPlace::Header));
        assert_eq!(remote.website.as_deref(), Some("https://r.com"));

        let npm = &servers[1];
        assert_eq!(npm.install, ServerInstall::Npm { package: "@x/mcp".into(), version: Some("2.0.0".into()), args: vec!["serve".into()] });
        let names: Vec<&str> = npm.inputs.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["root", "perfil", "API_KEY"]);
        assert_eq!(npm.inputs[0].place, InputPlace::Named { flag: "--root".into() });
        assert_eq!(npm.inputs[1].place, InputPlace::Positional);
        assert_eq!(npm.website.as_deref(), Some("https://github.com/x/y"));

        assert!(!servers[2].is_installable());
        assert!(matches!(&servers[2].install, ServerInstall::Unsupported { reason } if reason.contains("oci")));
    }

    #[test]
    #[ignore = "acessa a internet"]
    fn real_registry_answers_a_search() {
        let (servers, _) = McpRegistry.search("github", None).unwrap();
        assert!(!servers.is_empty());
        assert!(servers.iter().any(|s| s.name.to_lowercase().contains("github")));
    }
}
