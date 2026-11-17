//! Gestão de skills, agentes e conectores.

use super::{
    ports::{
        AccountConnectors, AccountRepository, AgentRepository, AntigravityMcp, Clock, CodexMcp, ConnectorRepository, ConnectorSource,
        ConversationRepository, IconSource, IdGenerator, Settings, SkillRepository,
    },
    AppError, AppResult,
};
use crate::domain::{
    account::{pick_next, Provider},
    extension::{icon_media_type, root_domain, AccountConnector, SERVER_OFF_PREFIX},
};
use crate::domain::extension::{
    connector_key, is_enabled_for_all, is_granted_to_agent, validate_connector_config, without_shadowed_plugins, Agent, Connector, Skill,
    SCOPE_ALL,
};
use crate::domain::mcp::{antigravity_allow_rules, turn_servers, Approval, Bridge, McpServer, McpTransport};
use serde::Serialize;
use std::sync::Arc;

/// Configuração (texto JSON) com as pontes: conectores da conta claude.ai usados também no ChatGPT e no Gemini.
pub const BRIDGES_KEY: &str = "connector_bridges";

/// Configuração (texto JSON) com os nomes dos conectores que o Claude Code carregou na última
/// verificação: turnos que só podem usar um deles bloqueiam os outros pelo nome.
pub const KNOWN_SERVERS_KEY: &str = "known_servers";

/// Guarda os nomes dos conectores que o Claude Code carrega.
pub fn remember_servers(settings: &dyn Settings, list: &[AccountConnector]) {
    let names: Vec<&str> = list.iter().map(|c| c.name.as_str()).collect();
    if let Ok(text) = serde_json::to_string(&names) {
        let _ = settings.set_text(KNOWN_SERVERS_KEY, &text);
    }
}

/// Nomes guardados por `remember_servers`; `None` quando nunca houve verificação.
pub fn known_servers(settings: &dyn Settings) -> Option<Vec<String>> {
    serde_json::from_str(&settings.text(KNOWN_SERVERS_KEY)?).ok()
}

/// Pontes gravadas; texto ilegível vale como nenhuma.
pub fn load_bridges(settings: &dyn Settings) -> Vec<Bridge> {
    settings.text(BRIDGES_KEY).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Pontes que valem nos turnos: as gravadas, menos as de conectores desligados na tela de Conectores.
pub fn active_bridges(settings: &dyn Settings) -> Vec<Bridge> {
    load_bridges(settings).into_iter().filter(|b| !settings.flag(&format!("{SERVER_OFF_PREFIX}{}", b.name), false)).collect()
}

/// Uma skill vista a partir de um escopo (todos ou um agente).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SkillView {
    #[serde(flatten)]
    pub skill: Skill,
    pub enabled: bool,
    /// No escopo de um agente: já disponível por estar ligada para todos.
    pub inherited: bool,
}

pub struct ManageSkills {
    pub skills: Arc<dyn SkillRepository>,
}

impl ManageSkills {
    pub fn list(&self, scope: &str) -> AppResult<Vec<SkillView>> {
        let all = self.skills.rules(SCOPE_ALL)?;
        let own = self.skills.rules(scope)?;
        let for_all = scope == SCOPE_ALL;
        Ok(self
            .skills
            .catalog()?
            .into_iter()
            .map(|skill| {
                let on_for_all = is_enabled_for_all(&all, &skill.name);
                SkillView {
                    enabled: if for_all { on_for_all } else { is_granted_to_agent(&own, &skill.name) },
                    inherited: !for_all && on_for_all,
                    skill,
                }
            })
            .collect())
    }

    pub fn set(&self, scope: &str, skill: &str, enabled: bool) -> AppResult<()> {
        self.skills.set_rule(scope, skill, enabled)
    }
}

pub struct ManageAgents {
    pub agents: Arc<dyn AgentRepository>,
    pub conversations: Arc<dyn ConversationRepository>,
    pub ids: Arc<dyn IdGenerator>,
}

impl ManageAgents {
    pub fn list(&self) -> AppResult<Vec<Agent>> {
        self.agents.list()
    }

    /// Cria (sem `id`) ou atualiza um agente.
    pub fn save(&self, id: Option<String>, name: &str, instructions: &str, model: Option<String>) -> AppResult<Agent> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError("Dê um nome para o agente.".into()));
        }
        let agent = Agent {
            id: id.unwrap_or_else(|| self.ids.new_id()),
            name: name.to_owned(),
            instructions: instructions.trim().to_owned(),
            model: model.filter(|m| !m.trim().is_empty()),
        };
        self.agents.save(&agent)?;
        Ok(agent)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        self.agents.delete(id)
    }

    pub fn assign(&self, conversation_id: &str, agent_id: Option<&str>) -> AppResult<()> {
        self.conversations.set_agent(conversation_id, agent_id)
    }
}

pub struct ManageConnectors {
    pub connectors: Arc<dyn ConnectorRepository>,
    pub desktop: Arc<dyn ConnectorSource>,
    pub ids: Arc<dyn IdGenerator>,
    pub account_connectors: Arc<dyn AccountConnectors>,
    pub accounts: Arc<dyn AccountRepository>,
    pub settings: Arc<dyn Settings>,
    pub clock: Arc<dyn Clock>,
    pub icons: Arc<dyn IconSource>,
    pub antigravity: Arc<dyn AntigravityMcp>,
    pub codex: Arc<dyn CodexMcp>,
}

/// Servidor de uma ponte para o login e a situação dele (sem a liberação de cada turno).
fn login_server(bridge: &Bridge) -> McpServer {
    McpServer {
        name: bridge.server_name(),
        transport: McpTransport::Http { url: bridge.endpoint.clone(), headers: Default::default() },
        approval: Approval::ReadOnly,
    }
}

/// Uma ponte (conector da conta claude.ai) como a tela de Conectores a mostra.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BridgeView {
    #[serde(flatten)]
    pub bridge: Bridge,
    /// Nome do servidor no ChatGPT e no Gemini (`lume-…`).
    pub server: String,
    /// Ponte para o Jira (a tela explica quem escreve nela fora do Claude).
    pub jira: bool,
    /// Login feito em todas as contas do ChatGPT; `None` quando não dá para saber.
    pub chatgpt: Option<bool>,
    /// Login feito no Antigravity; `None` quando não dá para saber.
    pub gemini: Option<bool>,
}

/// Conectores nos outros serviços: o que está instalado e a situação de cada ponte.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OtherServices {
    pub chatgpt_installed: bool,
    pub gemini_installed: bool,
    /// Há conta do ChatGPT para usar os conectores.
    pub chatgpt_accounts: usize,
    /// Por que os conectores não chegaram ao Gemini (ex.: arquivo de configuração ilegível).
    pub gemini_error: Option<String>,
    pub bridges: Vec<BridgeView>,
}

impl ManageConnectors {
    pub fn list(&self) -> AppResult<Vec<Connector>> {
        self.connectors.list()
    }

    /// Devolve o id do conector criado.
    pub fn add(&self, name: &str, config: &str) -> AppResult<String> {
        let name = name.trim();
        if connector_key(name).is_empty() {
            return Err(AppError("Dê um nome para o conector, com letras ou números.".into()));
        }
        validate_connector_config(config).map_err(|e| AppError(e.into()))?;
        if self.exists(name)? {
            return Err(AppError("Já existe um conector com este nome.".into()));
        }
        let id = self.ids.new_id();
        self.connectors.insert(&id, name, config.trim())?;
        self.sync_others();
        Ok(id)
    }

    /// Traz os conectores do Claude Desktop que ainda não existem aqui; entram desligados.
    /// `taken` são nomes que o Claude Code já carrega sozinho: importá-los duplicaria o servidor.
    pub fn import_from_desktop(&self, taken: &[String]) -> AppResult<usize> {
        let mut imported = 0;
        let taken: Vec<String> = taken.iter().map(|n| connector_key(n)).collect();
        for (name, config) in self.desktop.servers() {
            if validate_connector_config(&config).is_err() || self.exists(&name)? || taken.contains(&connector_key(&name)) {
                continue;
            }
            self.connectors.insert(&self.ids.new_id(), &name, &config)?;
            imported += 1;
        }
        if imported > 0 {
            self.sync_others();
        }
        Ok(imported)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> AppResult<()> {
        self.connectors.set_enabled(id, enabled)?;
        self.sync_others();
        Ok(())
    }

    pub fn remove(&self, id: &str) -> AppResult<()> {
        self.connectors.delete(id)?;
        self.sync_others();
        Ok(())
    }

    pub fn bridges(&self) -> Vec<Bridge> {
        load_bridges(&*self.settings)
    }

    /// Liga ou desliga o uso de um conector da conta claude.ai também no ChatGPT e no Gemini.
    pub fn set_bridge(&self, name: &str, display_name: &str, endpoint: &str, on: bool) -> AppResult<()> {
        let endpoint = endpoint.trim();
        if on && !endpoint.starts_with("https://") {
            return Err(AppError("Este conector não tem um endereço público para usar fora do Claude.".into()));
        }
        if on && connector_key(display_name).is_empty() {
            return Err(AppError("Este conector não tem um nome que sirva de servidor no ChatGPT e no Gemini.".into()));
        }
        let mut list: Vec<Bridge> = self.bridges().into_iter().filter(|b| b.name != name).collect();
        if on {
            list.push(Bridge { name: name.to_owned(), display_name: display_name.trim().to_owned(), endpoint: endpoint.to_owned() });
        }
        let text = serde_json::to_string(&list).map_err(|e| AppError(e.to_string()))?;
        self.settings.set_text(BRIDGES_KEY, &text)?;
        self.sync_others();
        Ok(())
    }

    /// Servidores que vão para o arquivo do Antigravity (a aprovação de cada turno não se aplica lá).
    fn mirrored(&self) -> AppResult<(Vec<McpServer>, Vec<Bridge>)> {
        let bridges = active_bridges(&*self.settings);
        Ok((turn_servers(&self.connectors.list()?, &bridges, &[], "manual"), bridges))
    }

    /// Leva os conectores do Lume e as pontes ao Antigravity. O Codex os recebe a cada turno.
    pub fn sync_antigravity(&self) -> AppResult<()> {
        let (servers, bridges) = self.mirrored()?;
        self.antigravity.sync(&servers, &antigravity_allow_rules(&servers, &bridges))
    }

    /// Sincroniza sem falhar a operação que a chamou: o problema aparece na tela de Conectores.
    fn sync_others(&self) {
        if let Err(e) = self.sync_antigravity() {
            eprintln!("[conectores] Gemini: {e}");
        }
    }

    fn bridge_server(&self, name: &str) -> AppResult<McpServer> {
        let bridge = self.bridges().into_iter().find(|b| b.name == name).ok_or_else(|| AppError("Ligue o conector no ChatGPT e no Gemini antes de entrar.".into()))?;
        Ok(login_server(&bridge))
    }

    fn chatgpt_profiles(&self) -> AppResult<Vec<Option<String>>> {
        Ok(self.accounts.all()?.into_iter().filter(|a| a.provider == Provider::ChatGpt && a.enabled).map(|a| a.config_dir).collect())
    }

    /// Situação dos conectores no ChatGPT e no Gemini, para a tela de Conectores.
    pub async fn other_services(&self) -> AppResult<OtherServices> {
        let profiles = self.chatgpt_profiles()?;
        let gemini_error = self.sync_antigravity().err().map(|e| e.0);
        // Cada consulta ao Codex leva alguns segundos: todas as pontes e contas de uma vez.
        let list = self.bridges();
        let mut checks = tokio::task::JoinSet::new();
        for (index, bridge) in list.iter().enumerate() {
            for dir in &profiles {
                let (codex, server, dir) = (self.codex.clone(), login_server(bridge), dir.clone());
                checks.spawn(async move { (index, codex.linked(dir.as_deref(), &server).await) });
            }
        }
        let mut chatgpt: Vec<Option<bool>> = vec![if profiles.is_empty() { None } else { Some(true) }; list.len()];
        while let Some(Ok((index, linked))) = checks.join_next().await {
            // Uma conta sem login decide; sem saber de alguma, fica desconhecido.
            chatgpt[index] = match (chatgpt[index], linked) {
                (_, Some(false)) | (Some(false), _) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            };
        }
        let bridges = list
            .into_iter()
            .zip(chatgpt)
            .map(|(bridge, chatgpt)| {
                let server = login_server(&bridge);
                let gemini = self.antigravity.linked(&server);
                BridgeView { server: server.name, jira: bridge.is_jira(), bridge, chatgpt, gemini }
            })
            .collect();
        Ok(OtherServices {
            chatgpt_installed: self.codex.installed(),
            gemini_installed: self.antigravity.installed(),
            chatgpt_accounts: profiles.len(),
            gemini_error,
            bridges,
        })
    }

    /// Faz o login da ponte no Codex, em cada conta do ChatGPT que ainda não tem.
    pub async fn login_chatgpt(&self, name: &str) -> AppResult<()> {
        if !self.codex.installed() {
            return Err(AppError("O Codex não está instalado neste computador.".into()));
        }
        let server = self.bridge_server(name)?;
        let profiles = self.chatgpt_profiles()?;
        if profiles.is_empty() {
            return Err(AppError("Adicione uma conta do ChatGPT em Contas antes de entrar.".into()));
        }
        for dir in profiles {
            if self.codex.linked(dir.as_deref(), &server).await != Some(true) {
                self.codex.login(dir.as_deref(), &server).await?;
            }
        }
        Ok(())
    }

    /// Conectores que o Claude Code carrega para a conta que atende a próxima mensagem.
    pub fn account_list(&self) -> AppResult<Vec<AccountConnector>> {
        let config_dir = self.active_config_dir()?;
        let loaded = self.account_connectors.list(config_dir.as_deref())?;
        remember_servers(&*self.settings, &loaded);
        let mut list = without_shadowed_plugins(loaded);
        for c in &mut list {
            c.enabled = !self.settings.flag(&format!("{SERVER_OFF_PREFIX}{}", c.name), false);
        }
        Ok(list)
    }

    /// Abre o login do conector no navegador, para a conta ativa.
    pub fn link(&self, name: &str) -> AppResult<()> {
        let config_dir = self.active_config_dir()?;
        self.account_connectors.login(config_dir.as_deref(), name)
    }

    pub fn unlink(&self, name: &str) -> AppResult<()> {
        let config_dir = self.active_config_dir()?;
        self.account_connectors.logout(config_dir.as_deref(), name)
    }

    pub fn set_server_enabled(&self, name: &str, enabled: bool) -> AppResult<()> {
        self.settings.set_flag(&format!("{SERVER_OFF_PREFIX}{name}"), !enabled)?;
        // Desligar um conector da conta tira também a ponte dele do Gemini.
        if self.bridges().iter().any(|b| b.name == name) {
            self.sync_others();
        }
        Ok(())
    }

    /// Ícone do serviço de um endereço, como data URL; `None` para comandos locais ou sem ícone.
    pub fn icon(&self, endpoint: &str) -> AppResult<Option<String>> {
        let Some(domain) = root_domain(endpoint) else { return Ok(None) };
        let Some(bytes) = self.icons.icon(&domain)? else { return Ok(None) };
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(Some(format!("data:{};base64,{encoded}", icon_media_type(&bytes))))
    }

    /// Perfil da conta do Claude que atende a próxima mensagem (sem conta: o login do computador).
    /// Os conectores da conta são do Claude Code; contas de outros serviços não têm os seus aqui.
    fn active_config_dir(&self) -> AppResult<Option<String>> {
        let all: Vec<_> = self.accounts.all()?.into_iter().filter(|a| a.provider == Provider::Claude).collect();
        Ok(pick_next(&all, self.clock.now(), &[], false).and_then(|a| a.config_dir.clone()))
    }

    fn exists(&self, name: &str) -> AppResult<bool> {
        let key = connector_key(name);
        Ok(self.connectors.list()?.iter().any(|c| connector_key(&c.name) == key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;
    use crate::domain::account::sample;

    #[test]
    fn skill_is_on_for_all_by_default_and_off_for_agents() {
        let world = World::with_accounts(vec![]);
        world.skills.remember(&["pdf".into()]).unwrap();
        let uc = ManageSkills { skills: world.skills.clone() };

        assert!(uc.list(SCOPE_ALL).unwrap()[0].enabled);
        let for_agent = &uc.list("a1").unwrap()[0];
        assert!(!for_agent.enabled && for_agent.inherited);

        uc.set(SCOPE_ALL, "pdf", false).unwrap();
        uc.set("a1", "pdf", true).unwrap();
        assert!(!uc.list(SCOPE_ALL).unwrap()[0].enabled);
        let for_agent = &uc.list("a1").unwrap()[0];
        assert!(for_agent.enabled && !for_agent.inherited);
    }

    #[test]
    fn saves_updates_and_assigns_agents() {
        let world = World::with_accounts(vec![]);
        let uc = ManageAgents { agents: world.store.clone(), conversations: world.store.clone(), ids: Arc::new(SequentialIds::default()) };
        assert!(uc.save(None, "  ", "", None).is_err());

        let agent = uc.save(None, "Revisor", "Revise o código.", Some("".into())).unwrap();
        assert_eq!(agent.model, None);
        uc.save(Some(agent.id.clone()), "Revisor de código", "Revise.", Some("sonnet".into())).unwrap();
        let all = uc.list().unwrap();
        assert_eq!((all.len(), all[0].name.as_str(), all[0].model.as_deref()), (1, "Revisor de código", Some("sonnet")));

        uc.assign("c1", Some(&agent.id)).unwrap();
        assert_eq!(world.conversation("c1").agent_id, Some(agent.id.clone()));
        uc.delete(&agent.id).unwrap();
        assert_eq!(world.conversation("c1").agent_id, None, "a conversa volta a ficar sem agente");
    }

    fn connectors_use_case(world: &World, desktop: FixedConnectors) -> ManageConnectors {
        ManageConnectors {
            connectors: world.store.clone(),
            desktop: Arc::new(desktop),
            ids: Arc::new(SequentialIds::default()),
            account_connectors: Arc::new(FakeAccountConnectors::default()),
            accounts: world.store.clone(),
            settings: world.store.clone(),
            clock: Arc::new(FixedClock(1000)),
            icons: Arc::new(FakeIcons),
            antigravity: Arc::new(FakeAntigravity::default()),
            codex: Arc::new(FakeCodexMcp::default()),
        }
    }

    #[tokio::test]
    async fn account_connectors_reach_chatgpt_and_gemini_through_bridges() {
        let world = World::with_accounts(vec![sample("a", 1)]);
        let mut uc = connectors_use_case(&world, FixedConnectors(vec![]));
        let gemini = Arc::new(FakeAntigravity::default());
        uc.antigravity = gemini.clone();
        let synced_last = || gemini.synced.lock().unwrap().last().cloned().unwrap();
        assert!(uc.set_bridge("claude.ai Local", "Local", "npx algo", true).is_err(), "sem endereço público não há ponte");
        let id = uc.add("Arquivos", r#"{"command":"npx","args":["fs"]}"#).unwrap();
        uc.set_enabled(&id, true).unwrap();
        uc.set_bridge("claude.ai Atlassian", "Atlassian", "https://mcp.atlassian.com/v1/mcp", true).unwrap();
        assert_eq!(uc.bridges().len(), 1);

        // O Gemini recebe os conectores do Lume (liberados) e a ponte (só leitura do Jira).
        let (servers, rules) = synced_last();
        assert_eq!(servers, ["lume-arquivos", "lume-atlassian"]);
        assert!(rules.contains(&"mcp(lume-arquivos/*)".to_owned()) && rules.contains(&"mcp(lume-atlassian/getJiraIssue)".to_owned()));

        // Sem conta do ChatGPT não há onde entrar; com duas, entra nas duas.
        assert!(uc.login_chatgpt("claude.ai Atlassian").await.is_err());
        for id in ["g1", "g2"] {
            AccountRepository::insert(&*world.store, id, id, Provider::ChatGpt, &crate::domain::account::Identity::default(), Some(&format!("/perfis/{id}"))).unwrap();
        }
        let view = uc.other_services().await.unwrap();
        assert_eq!((view.chatgpt_accounts, view.bridges[0].chatgpt, view.bridges[0].server.as_str()), (2, Some(false), "lume-atlassian"));
        uc.login_chatgpt("claude.ai Atlassian").await.unwrap();
        assert_eq!(uc.other_services().await.unwrap().bridges[0].chatgpt, Some(true));

        // Desligar o conector da conta na tela tira a ponte dos turnos e do Gemini.
        uc.set_server_enabled("claude.ai Atlassian", false).unwrap();
        assert!(active_bridges(&*world.store).is_empty());
        assert_eq!(synced_last().0, ["lume-arquivos"]);
        uc.set_server_enabled("claude.ai Atlassian", true).unwrap();
        assert_eq!(synced_last().0, ["lume-arquivos", "lume-atlassian"]);
        assert!(uc.set_bridge("claude.ai X", "★", "https://x.com/mcp", true).is_err(), "nome que não vira servidor");
        assert!(uc.other_services().await.unwrap().bridges[0].jira);

        uc.set_bridge("claude.ai Atlassian", "Atlassian", "", false).unwrap();
        assert!(uc.bridges().is_empty());
        assert_eq!(synced_last().0, ["lume-arquivos"]);
    }

    #[test]
    fn lists_account_connectors_for_the_active_account_and_blocks_disabled_ones() {
        let world = World::with_accounts(vec![sample("a", 1)]);
        world.store.set_account_config_dir("a", "/perfis/a");
        let gateway = Arc::new(FakeAccountConnectors::default());
        let mut uc = connectors_use_case(&world, FixedConnectors(vec![]));
        uc.account_connectors = gateway.clone();

        let list = uc.account_list().unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|c| c.enabled));

        uc.set_server_enabled("claude.ai Asana", false).unwrap();
        let list = uc.account_list().unwrap();
        assert!(!list.iter().find(|c| c.name == "claude.ai Asana").unwrap().enabled);

        assert_eq!(uc.icon("https://mcp.asana.com/v2").unwrap().as_deref(), Some("data:image/png;base64,iVBORw0KGgo="));
        assert_eq!(uc.icon("npx -y x").unwrap(), None);
        uc.link("claude.ai Asana").unwrap();
        uc.unlink("rea").unwrap();
        assert_eq!(*gateway.log.lock().unwrap(), ["list /perfis/a", "list /perfis/a", "login /perfis/a claude.ai Asana", "logout /perfis/a rea"]);
    }

    #[test]
    fn imports_desktop_connectors_once_and_disabled() {
        let world = World::with_accounts(vec![]);
        let desktop = FixedConnectors(vec![
            ("rea".into(), r#"{"command":"rea","args":[]}"#.into()),
            ("quebrado".into(), r#"{"args":[]}"#.into()),
        ]);
        let uc = connectors_use_case(&world, desktop);
        assert_eq!(uc.import_from_desktop(&["REA".into()]).unwrap(), 0, "já carregado pelo Claude Code");
        assert_eq!(uc.import_from_desktop(&[]).unwrap(), 1);
        assert_eq!(uc.import_from_desktop(&[]).unwrap(), 0);
        let all = uc.list().unwrap();
        assert_eq!((all.len(), all[0].name.as_str(), all[0].enabled), (1, "rea", false));

        assert!(uc.add("REA", r#"{"command":"x"}"#).is_err(), "nome repetido");
        assert!(uc.add("Novo", "{}").is_err(), "configuração inválida");
        uc.add("Novo", r#"{"url":"https://exemplo.com/mcp"}"#).unwrap();
        assert_eq!(uc.list().unwrap().len(), 2);
    }
}
