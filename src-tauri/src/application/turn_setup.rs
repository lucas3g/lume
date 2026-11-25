//! Monta o que um turno leva além da mensagem: agente, skills, conectores e
//! contexto do histórico.

use super::{
    flags,
    projects::{sync_knowledge, ProjectKnowledge},
    ports::{
        AgentRepository, ConnectorRepository, ConversationRepository, HistoryIndex, KnowledgeFolder, ProjectRepository, Settings,
        SkillRepository,
    },
    AppResult,
};
use crate::domain::{
    chat::{transcript_prompt, Conversation, ARTIFACTS_PREFIX, ARTIFACT_TOOL, NO_ACCOUNT_CONNECTORS_PREFIX, PIPELINE_STAGE_PREFIX},
    pipeline::Role,
    extension::{denied_server_rules, denied_skills, mcp_setup, SCOPE_ALL, SERVER_OFF_PREFIX},
    mcp::{turn_servers, McpServer},
    history::context_prompt,
    project::project_rules,
};
use std::sync::Arc;

const CONTEXT_SNIPPETS: usize = 3;
/// Conversa comum: consulta à web, skills e o Claude Design, sem acesso a arquivos.
const CHAT_TOOLS: [&str; 4] = ["WebSearch", "WebFetch", "Skill", ARTIFACT_TOOL];
/// O que a conversa comum usa sem pedir. O Artifact fica de fora: ele também apaga e republica
/// artifacts da conta, então cada uso passa pela sua aprovação.
const CHAT_ALLOWED: [&str; 3] = ["WebSearch", "WebFetch", "Skill"];
/// Leitura que a conversa comum ganha quando o projeto tem arquivos de conhecimento em disco.
/// Sem pedir autorização, ela alcança só a pasta deles e a pasta de trabalho das conversas
/// comuns (que fica vazia); fora disso a leitura pede autorização ao usuário.
const KNOWLEDGE_TOOLS: [&str; 2] = ["Read", "Grep"];
/// Sessão de agente: o conjunto padrão de ferramentas do provedor.
const AGENT_TOOLS: &str = "default";

/// Orienta o modelo a entregar conteúdo visual de um jeito que o painel de artifacts exibe.
const HTML_GUIDE: &str = "Quando o usuário pedir uma página, interface, gráfico, diagrama ou documento visual, \
entregue um único bloco de código ```html completo e autossuficiente (CSS e JavaScript embutidos, sem depender de \
arquivos externos); o aplicativo mostra esse bloco como prévia ao lado da conversa. Para imagens vetoriais simples, \
use um bloco ```svg.";

/// Conversa comum: o conteúdo visual vai para o Claude Design, que o aplicativo abre pelo link. Sem a
/// ferramenta (CLI antigo ou outro provedor), vale o bloco ```html.
const DESIGN_GUIDE: &str = "Quando o usuário pedir uma página, interface, tela, gráfico, diagrama, documento visual ou \
design, crie-o no Claude Design: chame a ferramenta Artifact com action \"quickstart\" e a intent adequada e siga o que \
ela indicar (crie a partir do `type_url` que ela der, com um título curto). Para ajustar um artifact já criado nesta \
conversa, atualize esse mesmo artifact (publique com o `url` dele, lendo-o antes). Não escreva o HTML na resposta nem em \
arquivos: o aplicativo abre o artifact pelo link. Termine a resposta com a linha `DESIGN: <link do artifact>`. Isso vale \
mesmo que conversas anteriores ou trechos de contexto tragam mockups em HTML. Se a ferramenta Artifact não estiver \
disponível e o usuário pediu o Claude Design, não faça mockup em HTML no lugar: diga que o Claude Design não está \
disponível nesta versão do Claude Code e pergunte se ele quer o mockup em HTML. Sem o pedido explícito do Claude Design, \
entregue um único bloco de código ```html completo e autossuficiente (CSS e JavaScript embutidos); o aplicativo mostra \
esse bloco como prévia ao lado da conversa.";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TurnSetup {
    pub model: String,
    pub extra_system: Option<String>,
    /// Instruções do projeto e do agente, parte de `extra_system` que pode mudar no meio
    /// da conversa. O CLI fixa o prompt de sistema quando a sessão abre e ignora o novo
    /// ao retomá-la; quando ela muda, segue junto da mensagem.
    pub rules: Option<String>,
    /// Lista dos arquivos de conhecimento do projeto (o conteúdo fica em disco); como
    /// `rules`, reenviada quando muda.
    pub knowledge: Option<String>,
    /// Muda quando muda a lista ou o conteúdo de algum arquivo de conhecimento.
    pub knowledge_key: Option<String>,
    /// Pastas que o assistente pode ler além da de trabalho (a do conhecimento do projeto).
    pub read_dirs: Vec<String>,
    pub tools: String,
    pub effort: String,
    pub permission_mode: String,
    pub interactive: bool,
    /// A conversa usa o Claude Design (a etapa do Designer).
    pub artifacts: bool,
    /// Oferece o Claude Design sem exigi-lo (toda conversa fora da esteira).
    pub design: bool,
    /// Carrega os conectores da conta do claude.ai (as etapas da esteira que não usam o Jira rodam sem).
    pub account_connectors: bool,
    /// Títulos das conversas anteriores usadas como contexto.
    pub context_titles: Vec<String>,
    pub denied_skills: Vec<String>,
    pub denied_tools: Vec<String>,
    pub mcp_config: Option<String>,
    pub allowed_tools: Vec<String>,
    /// Conectores para o ChatGPT: os do Lume e as pontes para os conectores da conta claude.ai.
    pub mcp_servers: Vec<McpServer>,
}

pub struct PrepareTurn {
    pub settings: Arc<dyn Settings>,
    pub history: Arc<dyn HistoryIndex>,
    pub skills: Arc<dyn SkillRepository>,
    pub agents: Arc<dyn AgentRepository>,
    pub projects: Arc<dyn ProjectRepository>,
    pub connectors: Arc<dyn ConnectorRepository>,
    pub conversations: Arc<dyn ConversationRepository>,
    pub knowledge: Arc<dyn KnowledgeFolder>,
}

impl PrepareTurn {

    /// Sem sessão no provedor, o diálogo já gravado (menos a mensagem que está sendo enviada) vira contexto.
    fn earlier_dialogue(&self, conversation: &Conversation) -> AppResult<Option<String>> {
        if conversation.cli_session_id.is_some() {
            return Ok(None);
        }
        self.dialogue(conversation)
    }

    /// O diálogo já gravado (menos a mensagem que está sendo enviada), para quem não
    /// consegue retomar a sessão: uma conta de outro serviço assume a conversa por ele.
    pub fn dialogue(&self, conversation: &Conversation) -> AppResult<Option<String>> {
        let mut messages = self.conversations.messages(&conversation.id)?;
        messages.pop();
        Ok(transcript_prompt(&messages))
    }

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
        let connectors = self.connectors.list()?;
        let (mcp_config, connector_tools) = mcp_setup(&connectors);
        let mcp_servers = turn_servers(&connectors, &super::extensions::active_bridges(&*self.settings), &conversation.allowed_tools, &conversation.permission_mode);
        let denied_tools = denied_server_rules(&self.settings.flags_with_prefix(SERVER_OFF_PREFIX)?);
        let project = match &conversation.project_id {
            Some(id) => self.projects.get(id)?,
            None => None,
        };
        let synced = match &project {
            Some(p) => sync_knowledge(&*self.projects, &*self.knowledge, &p.id)?,
            None => ProjectKnowledge::None,
        };
        let (read_dirs, knowledge, knowledge_key) = match synced {
            ProjectKnowledge::OnDisk { dir, listing, key } => (vec![dir], Some(listing), Some(key)),
            ProjectKnowledge::Inline(text) => (vec![], Some(text.clone()), Some(text)),
            ProjectKnowledge::None => (vec![], None, None),
        };
        // Etapa da esteira em andamento: só as ferramentas do papel, sem o contexto da conversa comum.
        let stage = self.settings.text(&format!("{PIPELINE_STAGE_PREFIX}{}", conversation.id)).and_then(|k| Role::parse(&k));
        // Só a conversa comum usa o Claude Design; a sessão de agente cria as telas no código.
        let design = !conversation.is_agent();
        let (tools, own_tools): (String, Vec<String>) = if conversation.is_agent() {
            let tools = stage.map(|role| role.tools(agent.is_some())).unwrap_or_else(|| AGENT_TOOLS.into());
            (tools, conversation.allowed_tools.clone())
        } else {
            let mut tools: Vec<&str> = CHAT_TOOLS.to_vec();
            if !read_dirs.is_empty() {
                tools.extend(KNOWLEDGE_TOOLS);
            }
            (tools.join(","), CHAT_ALLOWED.iter().map(|t| (*t).to_owned()).collect())
        };
        let allowed_tools = own_tools.into_iter().chain(connector_tools).collect();

        // O histórico ajuda a começar um assunto; no meio da conversa, ela mesma já é o contexto.
        // Na esteira, os trechos seriam de outras etapas ("Você é o Testador..."): ruído para o bot.
        let starting = self.conversations.message_count(&conversation.id)? <= 1;
        let snippets = if starting && stage.is_none() && self.settings.flag(flags::USE_HISTORY, true) {
            self.history.search(text, CONTEXT_SNIPPETS, conversation.cli_session_id.as_deref())
        } else {
            vec![]
        };
        let instructions = agent
            .as_ref()
            .filter(|a| !a.instructions.trim().is_empty())
            .map(|a| format!("Você está atuando como o agente \"{}\".\n{}", a.name, a.instructions.trim()));
        let earlier = self.earlier_dialogue(conversation)?;
        let rules = [project.as_ref().and_then(project_rules), instructions].into_iter().flatten().collect::<Vec<_>>().join("\n\n");
        let rules = (!rules.is_empty()).then_some(rules);
        let sections: Vec<String> = [stage.is_none().then(|| if design { DESIGN_GUIDE } else { HTML_GUIDE }.to_owned()), rules.clone(), knowledge.clone(), earlier, context_prompt(&snippets)]
            .into_iter()
            .flatten()
            .collect();

        Ok(TurnSetup {
            model: agent.and_then(|a| a.model).unwrap_or_else(|| conversation.model.clone()),
            extra_system: Some(sections.join("\n\n")),
            rules,
            knowledge,
            knowledge_key,
            read_dirs,
            tools,
            effort: conversation.effort.clone(),
            permission_mode: conversation.permission_mode.clone(),
            // Conectores da conta pedem aprovação em qualquer conversa; o resto já está liberado.
            interactive: true,
            artifacts: self.settings.flag(&format!("{ARTIFACTS_PREFIX}{}", conversation.id), false),
            design,
            account_connectors: !self.settings.flag(&format!("{NO_ACCOUNT_CONNECTORS_PREFIX}{}", conversation.id), false),
            context_titles: snippets.into_iter().map(|s| s.title).collect(),
            denied_skills: denied,
            denied_tools,
            mcp_config,
            allowed_tools,
            mcp_servers,
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
            conversations: world.store.clone(),
            knowledge: world.knowledge.clone(),
        }
    }

    #[test]
    fn earlier_dialogue_is_context_only_while_there_is_no_provider_session() {
        let world = World::with_accounts(vec![]);
        world.store.add_message("c1", "user", "Guarde o número 417.", None, None).unwrap();
        world.store.add_message("c1", "assistant", "Guardado.", None, None).unwrap();
        world.store.add_message("c1", "user", "Qual número?", None, None).unwrap();
        let system = prepare(&world).prepare(&world.conversation("c1"), "Qual número?").unwrap().extra_system.unwrap();
        assert!(system.contains("Usuário: Guarde o número 417.\n\nAssistente: Guardado."));
        assert!(!system.contains("Usuário: Qual número?"), "a mensagem atual não se repete");

        world.store.update("c1", None, Some("s1"), None).unwrap();
        let system = prepare(&world).prepare(&world.conversation("c1"), "Qual número?").unwrap().extra_system.unwrap();
        assert!(!system.contains("Guarde o número"), "com sessão, o provedor já tem o contexto");
    }

    #[test]
    fn plain_conversation_uses_its_model_and_no_extensions() {
        let world = World::with_accounts(vec![]);
        let conversation = world.conversation("c1");
        let setup = prepare(&world).prepare(&conversation, "olá").unwrap();
        assert_eq!(setup.model, "opus");
        assert_eq!((setup.tools.as_str(), setup.interactive), ("WebSearch,WebFetch,Skill,Artifact", true));
        assert!(setup.denied_tools.is_empty());
        assert_eq!(setup.allowed_tools, ["WebSearch", "WebFetch", "Skill"]);
        assert!(setup.design && !setup.artifacts, "oferece o Claude Design sem exigi-lo");
        assert!(setup.denied_skills.is_empty() && setup.mcp_config.is_none() && setup.context_titles.is_empty());
        assert!(setup.extra_system.unwrap().contains("Claude Design"));
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
        world.store.set_flag("server_off:claude.ai Asana", true).unwrap();
        world.store.set_flag("server_off:rea", false).unwrap();
        let setup = prepare(&world).prepare(&conversation, "olá").unwrap();
        assert_eq!(setup.denied_tools, ["mcp__claude_ai_Asana"]);
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
        assert!(!setup.design && setup.extra_system.unwrap_or_default().contains("```html"), "a sessão de agente cria as telas no código");
    }

    #[test]
    fn pipeline_stage_gets_only_its_role_tools_and_no_chat_context() {
        let world = World::with_accounts(vec![]);
        ConversationRepository::insert(&*world.store, "s1", "🤖 Revisor", "opus", "/tmp/loja", KIND_AGENT, None).unwrap();
        world.history.set(vec![Snippet { session_id: "h".into(), title: "Você é o Testador".into(), text: "outra execução".into() }]);
        world.store.set_text("etapa_da_esteira:s1", "reviewer").unwrap();
        let setup = prepare(&world).prepare(&world.conversation("s1"), "Revise").unwrap();
        assert_eq!(setup.tools, "Read,Bash,Task,ToolSearch", "sem as ferramentas que o papel não usa nem as de edição");
        assert!(setup.context_titles.is_empty(), "trechos de outras etapas só confundiriam o bot");
        assert!(!setup.extra_system.unwrap_or_default().contains("Claude Design"));
        assert!(!setup.design, "a etapa da esteira não ganha o Claude Design da conversa comum");

        let agent = Agent { id: "a1".into(), name: "Revisor rigoroso".into(), instructions: "Seja rigoroso.".into(), model: None };
        AgentRepository::save(&*world.store, &agent).unwrap();
        world.store.set_agent("s1", Some("a1")).unwrap();
        let setup = prepare(&world).prepare(&world.conversation("s1"), "Revise").unwrap();
        assert_eq!(setup.tools, "Read,Bash,Task,ToolSearch,Skill", "o agente pode ter skills liberadas");

        world.store.set_text("etapa_da_esteira:s1", "").unwrap();
        let setup = prepare(&world).prepare(&world.conversation("s1"), "e agora?").unwrap();
        assert_eq!(setup.tools, "default", "terminada a etapa, conversar com o bot volta a ter tudo");
    }

    #[test]
    fn project_adds_instructions_and_lists_knowledge_to_read_on_demand() {
        let world = World::with_accounts(vec![]);
        let project = crate::domain::project::Project { id: "p1".into(), name: "Portal".into(), instructions: "Use REST.".into(), folder: None, isolate: true };
        ProjectRepository::save(&*world.store, &project).unwrap();
        world.store.set_project("c1", Some("p1")).unwrap();
        let setup = prepare(&world).prepare(&world.conversation("c1"), "olá").unwrap();
        assert!(setup.read_dirs.is_empty() && setup.knowledge.is_none());
        assert_eq!(setup.tools, "WebSearch,WebFetch,Skill,Artifact", "sem conhecimento, a conversa segue sem ler arquivos");

        world.store.add_file("f1", "p1", "glossario.md", "SKU é o código do produto.").unwrap();
        let setup = prepare(&world).prepare(&world.conversation("c1"), "olá").unwrap();
        let system = setup.extra_system.unwrap();
        assert!(system.contains("Instruções do projeto \"Portal\":\nUse REST."));
        assert!(system.contains("na pasta /dados/knowledge/p1:\n- glossario.md (26 caracteres)"));
        assert!(!system.contains("SKU"), "o conteúdo fica em disco, fora do prompt");
        assert_eq!(setup.read_dirs, ["/dados/knowledge/p1"]);
        assert_eq!(setup.tools, "WebSearch,WebFetch,Skill,Artifact,Read,Grep");
        assert_eq!(setup.allowed_tools, ["WebSearch", "WebFetch", "Skill"], "a leitura não é liberada fora da pasta");
        let synced = world.knowledge.0.lock().unwrap().clone();
        assert_eq!(synced, [("p1".to_owned(), vec![("glossario.md".to_owned(), "SKU é o código do produto.".to_owned())])]);
    }

    #[test]
    fn knowledge_goes_in_the_prompt_when_it_cannot_be_written_to_disk() {
        let world = World::with_accounts(vec![]);
        let project = crate::domain::project::Project { id: "p1".into(), name: "Portal".into(), instructions: String::new(), folder: None, isolate: true };
        ProjectRepository::save(&*world.store, &project).unwrap();
        world.store.set_project("c1", Some("p1")).unwrap();
        world.store.add_file("f1", "p1", "glossario.md", "SKU é o código do produto.").unwrap();
        *world.knowledge.1.lock().unwrap() = true;
        let setup = prepare(&world).prepare(&world.conversation("c1"), "olá").unwrap();
        assert!(setup.extra_system.unwrap().contains("glossario.md:\nSKU é o código do produto."), "o turno segue, com o conteúdo no prompt");
        assert!(setup.read_dirs.is_empty());
        assert_eq!(setup.tools, "WebSearch,WebFetch,Skill,Artifact");
    }
}
