//! Monta o que um turno leva além da mensagem: agente, skills, conectores e
//! contexto do histórico.

use super::{
    flags,
    ports::{AgentRepository, ConnectorRepository, HistoryIndex, ProjectRepository, Settings, SkillRepository},
    AppResult,
};
use crate::domain::{
    chat::Conversation,
    extension::{denied_skills, mcp_setup, SCOPE_ALL},
    history::context_prompt,
    project::project_prompt,
};
use std::sync::Arc;

const CONTEXT_SNIPPETS: usize = 3;
/// Conversa comum: consulta à web e skills, tudo liberado, sem acesso a arquivos.
const CHAT_TOOLS: [&str; 3] = ["WebSearch", "WebFetch", "Skill"];
/// Sessão de agente: o conjunto padrão de ferramentas do provedor.
const AGENT_TOOLS: &str = "default";

/// Orienta o modelo a entregar conteúdo visual de um jeito que o painel de artifacts exibe.
const ARTIFACT_GUIDE: &str = "Quando o usuário pedir uma página, interface, gráfico, diagrama ou documento visual, \
entregue um único bloco de código ```html completo e autossuficiente (CSS e JavaScript embutidos, sem depender de \
arquivos externos); o aplicativo mostra esse bloco como prévia ao lado da conversa. Para imagens vetoriais simples, \
use um bloco ```svg.";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TurnSetup {
    pub model: String,
    pub extra_system: Option<String>,
    pub tools: String,
    pub effort: String,
    pub permission_mode: String,
    pub interactive: bool,
    /// Títulos das conversas anteriores usadas como contexto.
    pub context_titles: Vec<String>,
    pub denied_skills: Vec<String>,
    pub mcp_config: Option<String>,
    pub allowed_tools: Vec<String>,
}

pub struct PrepareTurn {
    pub settings: Arc<dyn Settings>,
    pub history: Arc<dyn HistoryIndex>,
    pub skills: Arc<dyn SkillRepository>,
    pub agents: Arc<dyn AgentRepository>,
    pub projects: Arc<dyn ProjectRepository>,
    pub connectors: Arc<dyn ConnectorRepository>,
}

impl PrepareTurn {
    pub fn prepare(&self, conversation: &Conversation, text: &str) -> AppResult<TurnSetup> {
        let agent = match &conversation.agent_id {
            Some(id) => self.agents.get(id)?,
            None => None,
        };
        let agent_rules = match &agent {
            Some(a) => Some(self.skills.rules(&a.id)?),
            None => None,
        };
        let denied = denied_skills(&self.skills.catalog()?, &self.skills.rules(SCOPE_ALL)?, agent_rules.as_ref());
        let (mcp_config, connector_tools) = mcp_setup(&self.connectors.list()?);
        let (tools, own_tools): (String, Vec<String>) = if conversation.is_agent() {
            (AGENT_TOOLS.into(), conversation.allowed_tools.clone())
        } else {
            (CHAT_TOOLS.join(","), CHAT_TOOLS.iter().map(|t| (*t).to_owned()).collect())
        };
        let allowed_tools = own_tools.into_iter().chain(connector_tools).collect();

        let snippets = if self.settings.flag(flags::USE_HISTORY, true) {
            self.history.search(text, CONTEXT_SNIPPETS, conversation.cli_session_id.as_deref())
        } else {
            vec![]
        };
        let instructions = agent
            .as_ref()
            .filter(|a| !a.instructions.trim().is_empty())
            .map(|a| format!("Você está atuando como o agente \"{}\".\n{}", a.name, a.instructions.trim()));
        let project = match &conversation.project_id {
            Some(id) => self.projects.get(id)?,
            None => None,
        };
        let project_section = match &project {
            Some(p) => project_prompt(p, &self.projects.knowledge(&p.id)?),
            None => None,
        };
        let sections: Vec<String> = [Some(ARTIFACT_GUIDE.to_owned()), project_section, instructions, context_prompt(&snippets)]
            .into_iter()
            .flatten()
            .collect();

        Ok(TurnSetup {
            model: agent.and_then(|a| a.model).unwrap_or_else(|| conversation.model.clone()),
            extra_system: Some(sections.join("\n\n")),
            tools,
            effort: conversation.effort.clone(),
            permission_mode: conversation.permission_mode.clone(),
            interactive: conversation.is_agent(),
            context_titles: snippets.into_iter().map(|s| s.title).collect(),
            denied_skills: denied,
            mcp_config,
            allowed_tools,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::World;
    use crate::application::ports::ConversationRepository;
    use crate::domain::{chat::KIND_AGENT, extension::Agent, history::Snippet};

    fn prepare(world: &World) -> PrepareTurn {
        PrepareTurn {
            settings: world.store.clone(),
            history: world.history.clone(),
            skills: world.skills.clone(),
            agents: world.store.clone(),
            projects: world.store.clone(),
            connectors: world.store.clone(),
        }
    }

    #[test]
    fn plain_conversation_uses_its_model_and_no_extensions() {
        let world = World::with_accounts(vec![]);
        let conversation = world.conversation("c1");
        let setup = prepare(&world).prepare(&conversation, "olá").unwrap();
        assert_eq!(setup.model, "opus");
        assert_eq!((setup.tools.as_str(), setup.interactive), ("WebSearch,WebFetch,Skill", false));
        assert_eq!(setup.allowed_tools, ["WebSearch", "WebFetch", "Skill"]);
        assert!(setup.denied_skills.is_empty() && setup.mcp_config.is_none() && setup.context_titles.is_empty());
        assert!(setup.extra_system.unwrap().contains("```html"));
    }

    #[test]
    fn agent_brings_instructions_model_and_its_skills() {
        let world = World::with_accounts(vec![]);
        world.skills.remember(&["pdf".into(), "planilha".into()]).unwrap();
        world.skills.set_rule(SCOPE_ALL, "pdf", false).unwrap();
        world.skills.set_rule(SCOPE_ALL, "planilha", false).unwrap();
        world.skills.set_rule("a1", "pdf", true).unwrap();
        let agent = Agent { id: "a1".into(), name: "Redator".into(), instructions: "Escreva de forma direta.".into(), model: Some("sonnet".into()) };
        AgentRepository::save(&*world.store, &agent).unwrap();
        world.store.set_agent("c1", Some("a1")).unwrap();
        world.store.insert_connector("k1", "Arquivos", r#"{"command":"npx"}"#, true);
        world.history.set(vec![Snippet { session_id: "h".into(), title: "Índices".into(), text: "btree".into() }]);

        let conversation = world.conversation("c1");
        let setup = prepare(&world).prepare(&conversation, "olá").unwrap();
        assert_eq!(setup.model, "sonnet");
        assert_eq!(setup.denied_skills, ["planilha"]);
        assert_eq!(setup.allowed_tools, ["WebSearch", "WebFetch", "Skill", "mcp__arquivos"]);
        assert!(setup.mcp_config.unwrap().contains("\"arquivos\""));
        assert_eq!(setup.context_titles, ["Índices"]);
        let system = setup.extra_system.unwrap();
        assert!(system.contains("agente \"Redator\"") && system.contains("Escreva de forma direta.") && system.contains("[Índices]"));
    }

    #[test]
    fn agent_session_asks_permission_except_for_tools_already_allowed() {
        let world = World::with_accounts(vec![]);
        ConversationRepository::insert(&*world.store, "s1", "Sessão", "opus", "/tmp/loja", KIND_AGENT, None).unwrap();
        world.store.allow_tool("s1", "Edit").unwrap();
        world.store.allow_tool("s1", "Edit").unwrap();
        let setup = prepare(&world).prepare(&world.conversation("s1"), "corrija").unwrap();
        assert_eq!((setup.tools.as_str(), setup.interactive), ("default", true));
        assert_eq!(setup.allowed_tools, ["Edit"]);
    }

    #[test]
    fn project_adds_instructions_and_knowledge() {
        let world = World::with_accounts(vec![]);
        let project = crate::domain::project::Project { id: "p1".into(), name: "Portal".into(), instructions: "Use REST.".into(), folder: None };
        ProjectRepository::save(&*world.store, &project).unwrap();
        world.store.add_file("f1", "p1", "glossario.md", "SKU é o código do produto.").unwrap();
        world.store.set_project("c1", Some("p1")).unwrap();
        let system = prepare(&world).prepare(&world.conversation("c1"), "olá").unwrap().extra_system.unwrap();
        assert!(system.contains("Instruções do projeto \"Portal\":\nUse REST."));
        assert!(system.contains("glossario.md:\nSKU é o código do produto."));
    }
}
