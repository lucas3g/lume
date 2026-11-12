//! Gestão de skills, agentes e conectores.

use super::{
    ports::{AgentRepository, ConnectorRepository, ConnectorSource, ConversationRepository, IdGenerator, SkillRepository},
    AppError, AppResult,
};
use crate::domain::extension::{
    connector_key, is_enabled_for_all, is_granted_to_agent, validate_connector_config, Agent, Connector, Skill,
    SCOPE_ALL,
};
use serde::Serialize;
use std::sync::Arc;

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
        Ok(id)
    }

    /// Traz os conectores do Claude Desktop que ainda não existem aqui; entram desligados.
    pub fn import_from_desktop(&self) -> AppResult<usize> {
        let mut imported = 0;
        for (name, config) in self.desktop.servers() {
            if validate_connector_config(&config).is_err() || self.exists(&name)? {
                continue;
            }
            self.connectors.insert(&self.ids.new_id(), &name, &config)?;
            imported += 1;
        }
        Ok(imported)
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> AppResult<()> {
        self.connectors.set_enabled(id, enabled)
    }

    pub fn remove(&self, id: &str) -> AppResult<()> {
        self.connectors.delete(id)
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

    #[test]
    fn imports_desktop_connectors_once_and_disabled() {
        let world = World::with_accounts(vec![]);
        let desktop = FixedConnectors(vec![
            ("rea".into(), r#"{"command":"rea","args":[]}"#.into()),
            ("quebrado".into(), r#"{"args":[]}"#.into()),
        ]);
        let uc = ManageConnectors { connectors: world.store.clone(), desktop: Arc::new(desktop), ids: Arc::new(SequentialIds::default()) };
        assert_eq!(uc.import_from_desktop().unwrap(), 1);
        assert_eq!(uc.import_from_desktop().unwrap(), 0);
        let all = uc.list().unwrap();
        assert_eq!((all.len(), all[0].name.as_str(), all[0].enabled), (1, "rea", false));

        assert!(uc.add("REA", r#"{"command":"x"}"#).is_err(), "nome repetido");
        assert!(uc.add("Novo", "{}").is_err(), "configuração inválida");
        uc.add("Novo", r#"{"url":"https://exemplo.com/mcp"}"#).unwrap();
        assert_eq!(uc.list().unwrap().len(), 2);
    }
}
