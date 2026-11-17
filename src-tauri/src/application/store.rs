//! Loja: instalar plugins de skills e adicionar conectores do diretório MCP.

use super::{
    extensions::ManageConnectors,
    ports::{PluginCatalog, ServerDirectory},
    AppError, AppResult,
};
use crate::domain::store::{connector_name, server_config, PluginListing, ServerListing};
use std::{collections::HashMap, sync::Arc};

pub struct BrowseStore {
    pub plugins: Arc<dyn PluginCatalog>,
    pub directory: Arc<dyn ServerDirectory>,
    pub connectors: Arc<ManageConnectors>,
}

/// Página de resultados do diretório de servidores.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct ServerPage {
    pub servers: Vec<ServerListing>,
    pub next_cursor: Option<String>,
}

impl BrowseStore {
    /// Plugins do catálogo filtrados por texto e categoria, instalados primeiro.
    pub fn plugins(&self, query: &str, category: Option<&str>) -> AppResult<Vec<PluginListing>> {
        let query = query.trim().to_lowercase();
        let mut list: Vec<PluginListing> = self
            .plugins
            .listings()?
            .into_iter()
            .filter(|p| category.is_none_or(|c| p.category.as_deref() == Some(c)))
            .filter(|p| {
                query.is_empty()
                    || p.name.to_lowercase().contains(&query)
                    || p.display_name.to_lowercase().contains(&query)
                    || p.description.to_lowercase().contains(&query)
            })
            .collect();
        list.sort_by(|a, b| b.installed.cmp(&a.installed).then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())));
        Ok(list)
    }

    /// Categorias presentes no catálogo, em ordem alfabética.
    pub fn categories(&self) -> AppResult<Vec<String>> {
        let mut categories: Vec<String> = self.plugins.listings()?.into_iter().filter_map(|p| p.category).collect();
        categories.sort();
        categories.dedup();
        Ok(categories)
    }

    pub fn install_plugin(&self, id: &str) -> AppResult<()> {
        self.plugins.install(id)
    }

    pub fn uninstall_plugin(&self, id: &str) -> AppResult<()> {
        self.plugins.uninstall(id)
    }

    pub fn refresh_catalog(&self) -> AppResult<()> {
        self.plugins.refresh()
    }

    pub fn servers(&self, query: &str, cursor: Option<&str>) -> AppResult<ServerPage> {
        let (servers, next_cursor) = self.directory.search(query.trim(), cursor)?;
        Ok(ServerPage { servers, next_cursor })
    }

    /// Adiciona o servidor como conector, já ligado, com os valores informados pelo usuário.
    pub fn add_server(&self, listing: &ServerListing, values: &HashMap<String, String>) -> AppResult<()> {
        if !listing.is_installable() {
            return Err(AppError("Este servidor não pode ser instalado pelo app.".into()));
        }
        let config = server_config(listing, values).map_err(AppError)?;
        let id = self.connectors.add(&connector_name(listing), &config)?;
        self.connectors.set_enabled(&id, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;
    use crate::domain::store::{InputPlace, ServerInput, ServerInstall};

    fn plugin(name: &str, category: &str, installed: bool) -> PluginListing {
        PluginListing {
            id: format!("{name}@oficial"),
            name: name.into(),
            display_name: name.into(),
            description: format!("Plugin {name}"),
            author: None,
            category: Some(category.into()),
            marketplace: "oficial".into(),
            homepage: None,
            skill_count: None,
            installed,
            enabled: installed,
        }
    }

    fn store(world: &World, catalog: Arc<FakePluginCatalog>) -> BrowseStore {
        BrowseStore {
            plugins: catalog,
            directory: Arc::new(FakeServerDirectory),
            connectors: Arc::new(ManageConnectors {
                connectors: world.store.clone(),
                desktop: Arc::new(FixedConnectors(vec![])),
                ids: Arc::new(SequentialIds::default()),
                account_connectors: Arc::new(FakeAccountConnectors::default()),
                accounts: world.store.clone(),
                settings: world.store.clone(),
                clock: Arc::new(FixedClock(1000)),
                icons: Arc::new(FakeIcons),
                antigravity: Arc::new(FakeAntigravity::default()),
                codex: Arc::new(FakeCodexMcp::default()),
            }),
        }
    }

    #[test]
    fn filters_sorts_and_installs_plugins() {
        let world = World::with_accounts(vec![]);
        let catalog = Arc::new(FakePluginCatalog::new(vec![
            plugin("zeta-tests", "testing", false),
            plugin("alpha-design", "design", false),
            plugin("beta-design", "design", true),
        ]));
        let uc = store(&world, catalog.clone());

        let names: Vec<String> = uc.plugins("", None).unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["beta-design", "alpha-design", "zeta-tests"], "instalados primeiro, depois alfabético");
        assert_eq!(uc.plugins("DESIGN", Some("design")).unwrap().len(), 2);
        assert_eq!(uc.plugins("tests", Some("design")).unwrap().len(), 0);
        assert_eq!(uc.categories().unwrap(), ["design", "testing"]);

        uc.install_plugin("alpha-design@oficial").unwrap();
        assert!(uc.plugins("alpha", None).unwrap()[0].installed);
        uc.uninstall_plugin("alpha-design@oficial").unwrap();
        assert!(!uc.plugins("alpha", None).unwrap()[0].installed);
        assert_eq!(*catalog.log.lock().unwrap(), ["install alpha-design@oficial", "uninstall alpha-design@oficial"]);
    }

    #[test]
    fn adds_directory_server_as_enabled_connector() {
        let world = World::with_accounts(vec![]);
        let uc = store(&world, Arc::new(FakePluginCatalog::new(vec![])));
        let page = uc.servers("github", None).unwrap();
        assert_eq!(page.servers.len(), 1);
        assert_eq!(page.next_cursor, None);

        let listing = ServerListing {
            name: "io.github.x/github".into(),
            title: "GitHub".into(),
            description: String::new(),
            website: None,
            install: ServerInstall::Remote { url: "https://api.githubcopilot.com/mcp/".into(), sse: false },
            inputs: vec![ServerInput { name: "Authorization".into(), description: String::new(), secret: true, required: true, place: InputPlace::Header }],
        };
        assert!(uc.add_server(&listing, &HashMap::new()).is_err(), "valor obrigatório ausente");
        uc.add_server(&listing, &HashMap::from([("Authorization".to_owned(), "Bearer t".to_owned())])).unwrap();
        let all = uc.connectors.list().unwrap();
        assert_eq!((all.len(), all[0].name.as_str(), all[0].enabled), (1, "GitHub", true));
        assert!(all[0].config.contains("\"Bearer t\""));
        assert!(uc.add_server(&listing, &HashMap::from([("Authorization".to_owned(), "x".to_owned())])).is_err(), "nome repetido");
    }
}
