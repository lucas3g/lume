//! Esteira de bots: cria a execução, prepara a pasta de trabalho, roda cada etapa
//! numa sessão própria e decide a próxima pelo registro das anteriores.

use super::{
    projects::add_free_worktree,
    ports::{
        AgentRepository, Clock, ConversationRepository, EventSink, IdGenerator, ModelCatalog, PipelineRepository, ProjectRepository, Settings,
        StageRunner, VersionControl,
    },
    send_message::make_title,
    AppError, AppResult,
};
use crate::domain::mcp::server_rule;
use crate::domain::permission::AUTO_APPROVE_PREFIX;
use crate::domain::{
    account::Provider,
    chat::{Attachment, Message, ARTIFACTS_PREFIX, ARTIFACT_TOOL, KIND_AGENT, ROLE_ASSISTANT, ROLE_USER},
    pipeline::{
        branch_slug, clip_diff, created_task, jira_site, task_ref, task_url, context_for, finished_status, hit_round_limit, starts_task, next_step, normalize_jira_key, implementer_round, rounds, spent_seconds, stage_prompt,
        suggested_commit, fix_request, fixing_review, parse_design_url, parse_findings, Finding, Next, ReviewPick, PipelineConfig, PipelineRun, Role, RunAgent, TierModels, MAX_IMAGES, MAX_IMAGE_BYTES, MAX_ROUNDS, RunStatus, StageRun, StageStatus, WorkMode, DEFAULT_BUDGET_MINUTES,
    },
    project::{folder_name, normalize_folder},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, Mutex},
};

/// Minutos acrescentados ao orçamento quando o usuário retoma uma execução que o esgotou.
const EXTRA_BUDGET_MINUTES: i64 = 30;

/// Configuração da esteira de cada projeto, guardada como texto: prefixo + id do projeto.
const CONFIG_PREFIX: &str = "pipeline_config:";
/// Modelo de task do Jira de todos os projetos (o de cada projeto vem antes).
const DEFAULT_TEMPLATE_KEY: &str = "pipeline_default_template";

pub struct ManagePipelines {
    pub pipelines: Arc<dyn PipelineRepository>,
    pub agents: Arc<dyn AgentRepository>,
    pub settings: Arc<dyn Settings>,
    pub catalog: Arc<dyn ModelCatalog>,
    pub projects: Arc<dyn ProjectRepository>,
    pub conversations: Arc<dyn ConversationRepository>,
    pub vcs: Arc<dyn VersionControl>,
    pub events: Arc<dyn EventSink>,
    pub ids: Arc<dyn IdGenerator>,
    pub clock: Arc<dyn Clock>,
    /// Execuções sendo conduzidas agora (uma condução por execução).
    pub active: Mutex<HashSet<String>>,
    /// Execuções que o usuário mandou parar.
    pub cancelled: Mutex<HashSet<String>>,
}

/// Pedido de uma execução vindo da interface.
pub struct StartInput {
    pub project_id: String,
    pub jira_key: Option<String>,
    /// Site do Jira de onde a task foi escolhida, para o link quando o projeto não tem modelo de task.
    pub jira_site: Option<String>,
    pub title: String,
    pub request: String,
    pub budget_minutes: Option<i64>,
    /// Imagens coladas no pedido.
    pub images: Vec<Attachment>,
    /// Começar pelo design no Claude Design, com a sua aprovação antes do plano.
    pub design: bool,
}

/// Execução com o registro das etapas, para a tela de acompanhamento.
#[derive(Debug, Clone, Serialize)]
pub struct RunDetail {
    #[serde(flatten)]
    pub run: PipelineRun,
    pub stages: Vec<StageRun>,
    /// Segundos já gastos pelos bots.
    pub spent_seconds: i64,
    /// Mensagem de commit sugerida pelo Relator.
    pub commit_message: Option<String>,
    /// Voltas máximas do Implementador antes de a esteira parar.
    pub max_rounds: i64,
    /// A execução parou no limite de voltas: retomar zera a contagem.
    pub round_limit: bool,
    /// Modelo de cada etapa (por id) em cada serviço.
    pub stage_models: BTreeMap<String, StageModels>,
}

/// Modelo com que a etapa roda em cada serviço: no Claude, o do agente (se tiver) ou o do
/// nível; nos outros, o escolhido para o nível (sem entrada, o padrão do CLI).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StageModels {
    pub claude: String,
    pub others: BTreeMap<String, String>,
}

/// Resposta final de um bot: a última mensagem do assistente depois do pedido.
/// Sem ela, o turno acabou sem resposta (limite de todas as contas, erro ou cancelamento).
pub fn final_answer(messages: &[Message]) -> AppResult<String> {
    let start = messages.iter().rposition(|m| m.role == ROLE_USER).map_or(0, |i| i + 1);
    messages[start..]
        .iter()
        .rev()
        .find(|m| m.role == ROLE_ASSISTANT && !m.content.trim().is_empty())
        .map(|m| m.content.clone())
        .ok_or_else(|| AppError("a etapa terminou sem resposta (limite de todas as contas, erro ou cancelamento).".into()))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl ManagePipelines {
    pub fn list(&self) -> AppResult<Vec<PipelineRun>> {
        self.pipelines.runs()
    }

    pub fn detail(&self, id: &str) -> AppResult<RunDetail> {
        let run = self.get(id)?;
        let stages = self.pipelines.stages(id)?;
        let commit_message = stages
            .iter()
            .rev()
            .find(|s| s.role == Role::Reporter && s.status == StageStatus::Done)
            .and_then(|s| s.output.as_deref().and_then(suggested_commit));
        let mut stage_models = BTreeMap::new();
        for stage in stages.iter().filter(|s| !s.conversation_id.is_empty()) {
            let Some(conversation) = self.conversations.get(&stage.conversation_id)? else { continue };
            let claude = run.agents.get(&stage.role).and_then(|a| a.model.clone()).unwrap_or(conversation.model);
            let others = conversation.provider_models;
            stage_models.insert(stage.id.clone(), StageModels { claude, others });
        }
        Ok(RunDetail { stage_models, max_rounds: MAX_ROUNDS, round_limit: hit_round_limit(&stages), spent_seconds: spent_seconds(&stages, self.clock.now()), commit_message, run, stages })
    }

    /// Como a esteira roda no projeto; sem configuração, todos os papéis com as regras padrão.
    /// Agentes apagados saem da resposta, para que salvar de novo não esbarre neles.
    pub fn config(&self, project_id: &str) -> AppResult<PipelineConfig> {
        let agents = self.role_agents(project_id)?.into_iter().map(|(role, agent)| (role, agent.id)).collect();
        Ok(PipelineConfig { agents, default_jira_template: self.default_template(), ..self.stored_config(project_id) })
    }

    /// Modelo de task do Jira de todos os projetos, se houver.
    pub fn default_template(&self) -> Option<String> {
        self.settings.text(DEFAULT_TEMPLATE_KEY).filter(|t| !t.trim().is_empty())
    }

    /// A configuração como foi gravada, sem conferir se os agentes ainda existem.
    fn stored_config(&self, project_id: &str) -> PipelineConfig {
        self.settings
            .text(&format!("{CONFIG_PREFIX}{project_id}"))
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save_config(&self, project_id: &str, mut config: PipelineConfig) -> AppResult<PipelineConfig> {
        let clean = |t: Option<String>| t.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
        config.jira_template = clean(config.jira_template);
        // O modelo padrão é de todos os projetos e tem o próprio caminho (`save_default_template`): salvar
        // um projeto não o regrava com o valor que a tela carregou.
        config.default_jira_template = None;
        self.projects.get(project_id)?.ok_or_else(|| AppError("Projeto não encontrado.".into()))?;
        config.validate().map_err(AppError)?;
        for id in config.agents.values() {
            self.agents.get(id)?.ok_or_else(|| AppError("Agente não encontrado.".into()))?;
        }
        config.skipped.sort();
        config.skipped.dedup();
        let text = serde_json::to_string(&config).map_err(|e| AppError(e.to_string()))?;
        self.settings.set_text(&format!("{CONFIG_PREFIX}{project_id}"), &text)?;
        Ok(PipelineConfig { default_jira_template: self.default_template(), ..config })
    }

    /// Modelo de task do Jira de todos os projetos (vazio apaga). Devolve o que ficou salvo.
    pub fn save_default_template(&self, template: Option<String>) -> AppResult<Option<String>> {
        let template = template.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
        PipelineConfig { default_jira_template: template.clone(), ..Default::default() }.validate().map_err(AppError)?;
        self.settings.set_text(DEFAULT_TEMPLATE_KEY, template.as_deref().unwrap_or_default())?;
        Ok(template)
    }

    /// Agentes da configuração que ainda existem (um agente apagado volta às regras padrão).
    fn role_agents(&self, project_id: &str) -> AppResult<BTreeMap<Role, crate::domain::extension::Agent>> {
        let mut out = BTreeMap::new();
        for (role, id) in self.stored_config(project_id).agents {
            if let Some(agent) = self.agents.get(&id)? {
                out.insert(role, agent);
            }
        }
        Ok(out)
    }

    /// Modelo de cada nível em cada serviço (vale para todos os projetos).
    pub fn models(&self) -> TierModels {
        self.settings.text(super::prefs::PIPELINE_MODELS).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    /// Modelos que o CLI do serviço aceita, para escolher em uma lista.
    pub async fn cli_models(&self, provider: Provider) -> AppResult<Vec<String>> {
        self.catalog.models(provider).await
    }

    pub fn save_models(&self, models: TierModels) -> AppResult<TierModels> {
        let models = models.normalized().map_err(AppError)?;
        let text = serde_json::to_string(&models).map_err(|e| AppError(e.to_string()))?;
        self.settings.set_text(super::prefs::PIPELINE_MODELS, &text)?;
        Ok(models)
    }

    /// Link da task no site do Jira do modelo (o modelo dado por link diz o endereço do site).
    fn link_for(&self, template: Option<&str>, key: &str) -> Option<String> {
        let (_, site) = task_ref(template?)?;
        Some(task_url(&site?, key))
    }

    /// Imagens coladas no pedido da execução.
    pub fn images(&self, run_id: &str) -> AppResult<Vec<Attachment>> {
        self.pipelines.images(run_id)
    }

    /// Cria a execução e prepara a pasta de trabalho. A condução começa com `advance`.
    pub fn start(&self, input: StartInput) -> AppResult<PipelineRun> {
        let project = self.projects.get(&input.project_id)?.ok_or_else(|| AppError("Projeto não encontrado.".into()))?;
        let folder = project
            .folder
            .clone()
            .ok_or_else(|| AppError("A esteira precisa de um projeto com pasta no computador.".into()))?;
        let jira_key = match input.jira_key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
            Some(key) => Some(normalize_jira_key(key).ok_or_else(|| AppError("Chave do Jira inválida (ex.: LUME-142).".into()))?),
            None => None,
        };
        let request = input.request.trim();
        let title = match (input.title.trim(), &jira_key) {
            ("", Some(key)) => key.clone(),
            // Só o pedido escrito: o título vem da primeira linha dele.
            ("", None) if !request.is_empty() => make_title(request),
            ("", None) => return Err(AppError("Informe a chave da task do Jira ou descreva o que fazer.".into())),
            (title, _) => title.to_owned(),
        };
        if jira_key.is_none() && request.is_empty() && title.chars().count() < 12 {
            return Err(AppError("Descreva o que os bots devem fazer.".into()));
        }
        if input.images.len() > MAX_IMAGES {
            return Err(AppError(format!("Cole no máximo {MAX_IMAGES} imagens.")));
        }
        // Base64: 4 caracteres para cada 3 bytes.
        if input.images.iter().any(|i| i.data.len() / 4 * 3 > MAX_IMAGE_BYTES) {
            return Err(AppError(format!("Cada imagem pode ter até {} MB.", MAX_IMAGE_BYTES / (1024 * 1024))));
        }
        let stored = self.stored_config(&project.id);
        // Sem modelo no projeto, vale o padrão: o Relator cria a task quando não há uma escolhida.
        let jira_template = stored.jira_template.or_else(|| self.default_template());
        let (mut skipped, efforts) = (stored.skipped, stored.efforts);
        if !input.design {
            skipped.extend(Role::OPT_IN);
        }
        let jira_url = jira_key.as_deref().and_then(|key| {
            self.link_for(jira_template.as_deref(), key)
                .or_else(|| Some(task_url(&jira_site(input.jira_site.as_deref()?)?, key)))
        });
        let agents = self
            .role_agents(&project.id)?
            .into_iter()
            .map(|(role, a)| (role, RunAgent { id: a.id, name: a.name, model: a.model }))
            .collect();
        let id = self.ids.new_id();
        let (mode, workdir, branch) = if project.versioned {
            let (path, branch) = self.create_worktree(&folder, &branch_slug(jira_key.as_deref(), &id))?;
            (WorkMode::Worktree, path, Some(branch))
        } else {
            (WorkMode::Direct, normalize_folder(&folder), None)
        };
        let now = self.clock.now();
        let run = PipelineRun {
            id,
            project_id: project.id,
            jira_key,
            title,
            request: request.to_owned(),
            status: RunStatus::Running,
            mode,
            workdir,
            branch,
            rounds: 0,
            skipped,
            agents,
            image_count: input.images.len() as i64,
            jira_template,
            jira_url,
            efforts,
            budget_minutes: input.budget_minutes.filter(|m| *m > 0).unwrap_or(DEFAULT_BUDGET_MINUTES),
            design: input.design,
            note: None,
            created_at: now,
            updated_at: now,
        };
        self.pipelines.save_run(&run)?;
        self.pipelines.save_images(&run.id, &input.images)?;
        self.events.pipeline_changed(&run);
        Ok(run)
    }

    /// Cópia isolada ao lado da pasta do projeto: pasta "<projeto>-<slug>", ramo "lume/<slug>".
    fn create_worktree(&self, folder: &str, slug: &str) -> AppResult<(String, String)> {
        let base = folder_name(folder);
        add_free_worktree(&*self.vcs, folder, &[], |n| {
            let suffix = if n == 1 { slug.to_owned() } else { format!("{slug}-{n}") };
            (format!("{base}-{suffix}"), format!("lume/{suffix}"))
        })
    }

    /// Conduz a execução até uma parada: aprovação, fim, falha ou cancelamento.
    pub async fn advance(&self, run_id: &str, runner: &dyn StageRunner) -> AppResult<()> {
        loop {
            if !lock(&self.active).insert(run_id.to_owned()) {
                return Ok(());
            }
            let result = self.drive(run_id, runner).await;
            // Sair da condução e ver se chegou um cancelamento é um passo só para `cancel`.
            let cancel_pending = {
                let mut active = lock(&self.active);
                active.remove(run_id);
                lock(&self.cancelled).contains(run_id)
            };
            if let (Err(e), Ok(mut run)) = (&result, self.get(run_id))
                && !run.status.is_finished()
            {
                self.finish(&mut run, RunStatus::Failed, Some(e.0.clone()))?;
            }
            if cancel_pending {
                // O pedido chegou enquanto a condução saía: cancela como uma execução parada.
                self.cancel(run_id, runner)?;
                return result;
            }
            // Retomada ou aprovação enquanto a condução saía: a chamada delas achou esta
            // ainda ativa e desistiu, então esta continua.
            let resumed = result.is_ok()
                && self.get(run_id).is_ok_and(|r| {
                    r.status == RunStatus::Running && self.pipelines.stages(run_id).is_ok_and(|s| next_step(&s, &r.skipped) != Next::Wait)
                });
            if !resumed {
                return result;
            }
        }
    }

    async fn drive(&self, run_id: &str, runner: &dyn StageRunner) -> AppResult<()> {
        loop {
            let mut run = self.get(run_id)?;
            if run.status != RunStatus::Running {
                return Ok(());
            }
            if lock(&self.cancelled).remove(run_id) {
                return self.finish(&mut run, RunStatus::Cancelled, Some("Cancelada por você.".into()));
            }
            let stages = self.pipelines.stages(run_id)?;
            let role = match next_step(&stages, &run.skipped) {
                Next::Wait => return Ok(()),
                Next::Finish(status, note) => return self.finish(&mut run, status, note),
                Next::AskApproval => return self.ask_approval(&mut run, &stages, Role::Approval),
                Next::AskDesignApproval => return self.ask_approval(&mut run, &stages, Role::DesignApproval),
                Next::AskChoice => return self.ask_choice(&mut run, &stages),
                Next::Run(role) => role,
            };
            if spent_seconds(&stages, self.clock.now()) >= run.budget_minutes * 60 {
                let note = format!("O orçamento de {} minutos acabou antes de {}.", run.budget_minutes, role.label());
                return self.finish(&mut run, RunStatus::Failed, Some(note));
            }
            self.run_stage(&mut run, &stages, role, runner).await?;
        }
    }

    async fn run_stage(&self, run: &mut PipelineRun, stages: &[StageRun], role: Role, runner: &dyn StageRunner) -> AppResult<()> {
        let mut ctx = context_for(stages);
        if role == Role::Reviewer {
            ctx.diff = self.diff(&run.workdir);
        }
        let prompt = stage_prompt(role, run, &ctx);
        let conversation_id = self.ids.new_id();
        let label = run.jira_key.clone().unwrap_or_else(|| run.title.clone());
        // Cada serviço tem os próprios modelos: o do Claude fica na conversa; os dos outros
        // ficam na conversa também, para valer se a conta trocar para o ChatGPT ou o Gemini no meio.
        let models = self.models();
        let claude_model = models.model(Provider::Claude, role.tier()).unwrap_or_default();
        self.conversations.insert(&conversation_id, &format!("🤖 {} · {label}", role.label()), &claude_model, &run.workdir, KIND_AGENT, None)?;
        let others = models.others(role.tier());
        if !others.is_empty() {
            self.conversations.set_provider_models(&conversation_id, &others)?;
        }
        self.conversations.set_project(&conversation_id, Some(&run.project_id))?;
        self.conversations.set_permission_mode(&conversation_id, role.permission_mode())?;
        // Os pedidos de permissão que não são críticos o app aprova sozinho: só as perguntas, os planos e as
        // ações críticas chegam a você (além das suas paradas de aprovação, que não são do CLI).
        self.settings.set_flag(&format!("{AUTO_APPROVE_PREFIX}{conversation_id}"), true)?;
        // No ChatGPT ninguém aprova no meio do turno: o Relator, que comenta e muda o status da task,
        // já sai com as pontes do Jira liberadas. Os outros papéis só leem.
        // A primeira etapa a pegar a task escolhida a move para "Em andamento": ela também precisa escrever no Jira.
        if role == Role::Reporter || starts_task(role, run, &ctx) {
            for bridge in super::extensions::load_bridges(&*self.settings).iter().filter(|b| b.is_jira()) {
                self.conversations.allow_tool(&conversation_id, &server_rule(&bridge.server_name()))?;
            }
        }
        // O agente do papel soma as instruções, o modelo e as skills dele às regras fixas do papel.
        // O agente é o escolhido quando a execução começou; se foi apagado depois, vale a regra padrão.
        if let Some(agent) = run.agents.get(&role)
            && self.agents.get(&agent.id)?.is_some()
        {
            self.conversations.set_agent(&conversation_id, Some(&agent.id))?;
        }
        if let Some(effort) = run.efforts.get(&role) {
            self.conversations.set_effort(&conversation_id, effort)?;
        }
        // O Claude Design só para o Designer, que publica sem pedir. Quem planeja e implementa recebe a
        // especificação completa no prompt: não precisa (nem conseguiria, se o design é de outra conta) abrir
        // o artifact, e não pode sobrescrever o design aprovado.
        if role == Role::Designer {
            self.conversations.allow_tool(&conversation_id, ARTIFACT_TOOL)?;
            self.settings.set_flag(&format!("{ARTIFACTS_PREFIX}{conversation_id}"), true)?;
        }
        // A volta do Implementador é o número de passagens dele antes desta; a que aplica as
        // correções do code review fica na volta em que já estava.
        let round = if role == Role::Implementer && !fixing_review(stages) { implementer_round(stages) } else { rounds(stages) };
        let mut stage = StageRun {
            id: self.ids.new_id(),
            run_id: run.id.clone(),
            role,
            round,
            status: StageStatus::Running,
            conversation_id: conversation_id.clone(),
            input: prompt.clone(),
            output: None,
            started_at: self.clock.now(),
            finished_at: None,
        };
        self.pipelines.save_stage(&stage)?;
        run.rounds = round;
        self.touch(run)?;

        // Um cancelamento que chegou antes de o turno existir não teria turno para interromper.
        let result = if lock(&self.cancelled).contains(&run.id) {
            Err(AppError("Cancelada por você.".into()))
        } else {
            let images = if role.sees_images() && run.image_count > 0 { self.pipelines.images(&run.id)? } else { vec![] };
            runner.run(&conversation_id, &prompt, images).await
        };
        let cancelled = lock(&self.cancelled).contains(&run.id);
        (stage.status, stage.output) = match result {
            _ if cancelled => (StageStatus::Cancelled, Some("Cancelada por você.".into())),
            Ok(text) if role == Role::Designer && parse_design_url(&text).is_none() => (
                StageStatus::Failed,
                Some(format!("o design não foi publicado: a resposta não trouxe a linha `DESIGN: <link do artifact>`.\n\n{text}")),
            ),
            Ok(text) => (finished_status(role, &text), Some(text)),
            Err(e) => (StageStatus::Failed, Some(e.0.clone())),
        };
        stage.finished_at = Some(self.clock.now());
        self.pipelines.save_stage(&stage)?;
        // Sem task para o pedido, o Relator cria uma pelo modelo do projeto: ela passa a ser a da execução.
        if role == Role::Reporter && run.jira_key.is_none() && stage.status == StageStatus::Done
            && let Some((key, url)) = stage.output.as_deref().and_then(created_task)
        {
            run.jira_url = url.or_else(|| self.link_for(run.jira_template.as_deref(), &key));
            run.jira_key = Some(key);
        }
        if stage.status == StageStatus::Failed {
            let note = format!("{} não terminou: {}", role.label(), stage.output.clone().unwrap_or_default());
            return self.finish(run, RunStatus::Failed, Some(note));
        }
        self.touch(run)
    }

    /// Diff de tudo que mudou na pasta de trabalho, para o Revisor; `None` sem git.
    fn diff(&self, workdir: &str) -> Option<String> {
        let status = self.vcs.status(workdir);
        if !status.is_repo || status.files.is_empty() {
            return None;
        }
        // Tudo que é versionado sai num diff só; arquivos novos (ainda fora do git), um a um.
        let mut out = self.vcs.diff(workdir, ".").unwrap_or_default();
        for file in status.files.iter().filter(|f| f.status == "?") {
            if let Ok(diff) = self.vcs.diff(workdir, &file.path) {
                out.push('\n');
                out.push_str(&diff);
            }
        }
        (!out.trim().is_empty()).then(|| clip_diff(&out))
    }

    /// Abre a parada da sua aprovação: do plano (a entrada é o plano) ou do design (a resposta do
    /// Designer, com a especificação e o link do artifact).
    fn ask_approval(&self, run: &mut PipelineRun, stages: &[StageRun], role: Role) -> AppResult<()> {
        let ctx = context_for(stages);
        let input = if role == Role::DesignApproval { ctx.design } else { ctx.plan };
        let stage = StageRun {
            id: self.ids.new_id(),
            run_id: run.id.clone(),
            role,
            round: rounds(stages),
            status: StageStatus::Running,
            conversation_id: String::new(),
            input: input.unwrap_or_default(),
            output: None,
            started_at: self.clock.now(),
            finished_at: None,
        };
        self.pipelines.save_stage(&stage)?;
        run.status = RunStatus::AwaitingApproval;
        self.touch(run)
    }

    /// Abre a parada da escolha com os achados do último code review. Sem a lista na resposta,
    /// a entrada fica vazia: o usuário lê o texto do code review e escreve o que quer corrigir.
    fn ask_choice(&self, run: &mut PipelineRun, stages: &[StageRun]) -> AppResult<()> {
        let review = stages.iter().rev().find(|s| s.role == Role::CodeReview && s.status == StageStatus::Done);
        let findings = review.and_then(|s| s.output.as_deref()).and_then(parse_findings);
        let stage = StageRun {
            id: self.ids.new_id(),
            run_id: run.id.clone(),
            role: Role::ReviewChoice,
            round: rounds(stages),
            status: StageStatus::Running,
            conversation_id: String::new(),
            input: findings.map(|f| serde_json::to_string(&f).unwrap_or_default()).unwrap_or_default(),
            output: None,
            started_at: self.clock.now(),
            finished_at: None,
        };
        self.pipelines.save_stage(&stage)?;
        run.status = RunStatus::AwaitingChoice;
        self.touch(run)
    }

    /// Escolha do usuário sobre o code review: os achados a corrigir (com o caminho decidido
    /// nos que têm mais de um) e uma observação livre. Sem nada escolhido, a esteira segue para
    /// o Relator; com algo, o Implementador corrige e o Revisor e o Testador conferem.
    /// A execução volta a andar e precisa de `advance`.
    pub fn decide_review(&self, run_id: &str, picks: &[ReviewPick], note: Option<&str>) -> AppResult<()> {
        let mut run = self.get(run_id)?;
        if run.status != RunStatus::AwaitingChoice {
            return Err(AppError("Esta execução não está esperando a sua escolha.".into()));
        }
        let mut stage = self
            .pipelines
            .stages(run_id)?
            .into_iter()
            .rev()
            .find(|s| s.role == Role::ReviewChoice && s.status == StageStatus::Running)
            .ok_or_else(|| AppError("Não há code review esperando a sua escolha.".into()))?;
        let findings: Vec<Finding> = serde_json::from_str(&stage.input).unwrap_or_default();
        (stage.status, stage.output) = match fix_request(&findings, picks, note).map_err(AppError)? {
            Some(request) => (StageStatus::Returned, Some(request)),
            None => (StageStatus::Done, Some("Nenhuma correção escolhida.".into())),
        };
        stage.finished_at = Some(self.clock.now());
        self.pipelines.save_stage(&stage)?;
        run.status = RunStatus::Running;
        self.touch(&mut run)
    }

    /// Resposta do usuário ao plano ou ao design que espera aprovação: aprovar (sem `feedback`) ou
    /// pedir ajustes. A execução volta a andar e precisa de `advance`.
    pub fn decide_plan(&self, run_id: &str, feedback: Option<&str>) -> AppResult<()> {
        let mut run = self.get(run_id)?;
        if run.status != RunStatus::AwaitingApproval {
            return Err(AppError("Esta execução não está esperando aprovação.".into()));
        }
        let mut stage = self
            .pipelines
            .stages(run_id)?
            .into_iter()
            .rev()
            .find(|s| matches!(s.role, Role::Approval | Role::DesignApproval) && s.status == StageStatus::Running)
            .ok_or_else(|| AppError("Não há plano nem design esperando aprovação.".into()))?;
        let feedback = feedback.map(str::trim).filter(|f| !f.is_empty());
        let approved = if stage.role == Role::DesignApproval { "Design aprovado." } else { "Plano aprovado." };
        (stage.status, stage.output) = match feedback {
            Some(text) => (StageStatus::Returned, Some(text.to_owned())),
            None => (StageStatus::Done, Some(approved.into())),
        };
        stage.finished_at = Some(self.clock.now());
        self.pipelines.save_stage(&stage)?;
        run.status = RunStatus::Running;
        self.touch(&mut run)
    }

    /// Para a execução. A etapa em andamento é interrompida.
    pub fn cancel(&self, run_id: &str, runner: &dyn StageRunner) -> AppResult<()> {
        let mut run = self.get(run_id)?;
        if run.status.is_finished() {
            return Ok(());
        }
        let stages = self.pipelines.stages(run_id)?;
        let running = stages.into_iter().filter(|s| s.status == StageStatus::Running);
        // Com quem conduz, o pedido fica marcado no mesmo passo em que se vê a condução:
        // se ela sair logo depois, `advance` encontra o pedido e cancela.
        let conducted = {
            let active = lock(&self.active);
            let conducted = active.contains(run_id);
            if conducted {
                lock(&self.cancelled).insert(run_id.to_owned());
            }
            conducted
        };
        for mut stage in running {
            if !stage.role.is_bot() || !conducted {
                stage.status = StageStatus::Cancelled;
                stage.finished_at = Some(self.clock.now());
                self.pipelines.save_stage(&stage)?;
            } else {
                runner.cancel(&stage.conversation_id);
            }
        }
        if conducted && run.status == RunStatus::Running {
            // Quem conduz encerra a execução ao ver o pedido.
            lock(&self.cancelled).insert(run_id.to_owned());
            return Ok(());
        }
        self.finish(&mut run, RunStatus::Cancelled, Some("Cancelada por você.".into()))
    }

    /// Retoma uma execução que falhou: a etapa que falhou roda de novo.
    pub fn retry(&self, run_id: &str) -> AppResult<()> {
        let mut run = self.get(run_id)?;
        if !matches!(run.status, RunStatus::Failed | RunStatus::Cancelled) {
            return Err(AppError("Só dá para retomar uma execução que falhou ou foi cancelada.".into()));
        }
        let mut stages = self.pipelines.stages(run_id)?;
        // Parou no limite de voltas: retomar zera a contagem e devolve o trabalho ao Implementador.
        // A marca só é gravada depois de conferir que a execução pode seguir.
        let resume = hit_round_limit(&stages).then(|| {
            let now = self.clock.now();
            StageRun {
                id: self.ids.new_id(),
                run_id: run.id.clone(),
                role: Role::Resume,
                round: 0,
                status: StageStatus::Done,
                conversation_id: String::new(),
                input: String::new(),
                output: Some(format!("Você retomou depois de {MAX_ROUNDS} voltas; a contagem recomeça.")),
                started_at: now,
                finished_at: Some(now),
            }
        });
        stages.extend(resume.clone());
        if matches!(next_step(&stages, &run.skipped), Next::Finish(..)) {
            return Err(AppError("Esta execução já chegou ao fim da sequência; comece uma nova.".into()));
        }
        if let Some(stage) = &resume {
            self.pipelines.save_stage(stage)?;
            run.rounds = 0;
        }
        let spent = spent_seconds(&stages, self.clock.now());
        if spent >= run.budget_minutes * 60 {
            // Uma etapa pode passar bem do limite: a folga conta a partir do que já foi gasto.
            run.budget_minutes = (spent + 59) / 60 + EXTRA_BUDGET_MINUTES;
        }
        lock(&self.cancelled).remove(run_id);
        run.status = RunStatus::Running;
        run.note = None;
        self.touch(&mut run)
    }

    /// Reabre uma execução concluída com um ajuste do usuário (algo que não funcionou ao
    /// testar): o Implementador corrige, o Revisor e o Testador conferem e o Relator comenta
    /// na mesma task do Jira, sem criar outra. A condução recomeça com `advance`.
    pub fn reopen(&self, run_id: &str, feedback: &str) -> AppResult<()> {
        let mut run = self.get(run_id)?;
        if run.status != RunStatus::Done {
            return Err(AppError("Só dá para reabrir uma execução concluída.".into()));
        }
        let feedback = feedback.trim();
        if feedback.is_empty() {
            return Err(AppError("Descreva o que precisa mudar.".into()));
        }
        let stages = self.pipelines.stages(run_id)?;
        let now = self.clock.now();
        let stage = StageRun {
            id: self.ids.new_id(),
            run_id: run.id.clone(),
            role: Role::Feedback,
            round: 0,
            status: StageStatus::Done,
            conversation_id: String::new(),
            input: String::new(),
            output: Some(feedback.to_owned()),
            started_at: now,
            finished_at: Some(now),
        };
        self.pipelines.save_stage(&stage)?;
        // O ajuste ganha tempo próprio: a folga conta a partir do que já foi gasto, para que o
        // orçamento não acabe no meio do ciclo (ex.: depois do Implementador, antes do Revisor).
        let spent = spent_seconds(&stages, now);
        run.budget_minutes = run.budget_minutes.max((spent + 59) / 60 + EXTRA_BUDGET_MINUTES);
        lock(&self.cancelled).remove(run_id);
        run.status = RunStatus::Running;
        run.rounds = 0;
        run.note = None;
        self.touch(&mut run)
    }

    /// Registra o resultado num commit (só na cópia isolada; a decisão é do usuário).
    pub fn commit(&self, run_id: &str, message: &str) -> AppResult<()> {
        let run = self.get(run_id)?;
        if run.mode != WorkMode::Worktree {
            return Err(AppError("Projeto sem versionamento: não há commit a fazer.".into()));
        }
        if run.status != RunStatus::Done {
            return Err(AppError("A execução ainda não terminou.".into()));
        }
        let message = message.trim();
        if message.is_empty() {
            return Err(AppError("Escreva a mensagem do commit.".into()));
        }
        self.vcs.commit(&run.workdir, message)
    }

    /// Apaga o registro de uma execução terminada (a pasta de trabalho fica).
    pub fn delete(&self, run_id: &str) -> AppResult<()> {
        let run = self.get(run_id)?;
        if !run.status.is_finished() {
            return Err(AppError("Cancele a execução antes de apagá-la.".into()));
        }
        self.pipelines.delete_run(run_id)
    }

    /// Ao abrir o app: etapas que estavam rodando quando ele fechou não terminam mais.
    /// As execuções delas ficam como falhas, para o usuário retomar.
    pub fn recover(&self) -> AppResult<()> {
        for mut run in self.pipelines.runs()?.into_iter().filter(|r| r.status == RunStatus::Running) {
            for mut stage in self.pipelines.stages(&run.id)?.into_iter().filter(|s| s.status == StageStatus::Running) {
                stage.status = StageStatus::Failed;
                stage.output = Some("O app foi fechado durante a etapa.".into());
                // O tempo com o app fechado não é trabalho dos bots: a etapa acaba na última mensagem gravada.
                let last_activity = self.conversations.messages(&stage.conversation_id).ok().and_then(|m| m.last().map(|m| m.created_at));
                stage.finished_at = Some(last_activity.unwrap_or(stage.started_at).max(stage.started_at));
                self.pipelines.save_stage(&stage)?;
            }
            self.finish(&mut run, RunStatus::Failed, Some("O app foi fechado durante a execução; retome quando quiser.".into()))?;
        }
        Ok(())
    }

    fn finish(&self, run: &mut PipelineRun, status: RunStatus, note: Option<String>) -> AppResult<()> {
        lock(&self.cancelled).remove(&run.id);
        run.status = status;
        run.note = note;
        self.touch(run)
    }

    fn touch(&self, run: &mut PipelineRun) -> AppResult<()> {
        run.updated_at = self.clock.now();
        self.pipelines.save_run(run)?;
        self.events.pipeline_changed(run);
        Ok(())
    }

    fn get(&self, id: &str) -> AppResult<PipelineRun> {
        self.pipelines.run(id)?.ok_or_else(|| AppError("Execução não encontrada.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::*;
    use crate::domain::project::Project;
    use async_trait::async_trait;

    /// Bots de mentira: cada papel responde conforme a lista, na ordem das chamadas.
    struct Bots {
        answers: Mutex<Vec<AppResult<String>>>,
        prompts: Mutex<Vec<String>>,
        /// Imagens recebidas por chamada.
        images: Mutex<Vec<usize>>,
    }

    impl Bots {
        fn new(answers: Vec<AppResult<&str>>) -> Bots {
            let answers = answers.into_iter().rev().map(|a| a.map(str::to_owned)).collect();
            Bots { answers: Mutex::new(answers), prompts: Mutex::default(), images: Mutex::default() }
        }
    }

    #[async_trait]
    impl StageRunner for Bots {
        async fn run(&self, _conversation_id: &str, prompt: &str, images: Vec<Attachment>) -> AppResult<String> {
            self.prompts.lock().unwrap().push(prompt.to_owned());
            self.images.lock().unwrap().push(images.len());
            self.answers.lock().unwrap().pop().unwrap_or_else(|| Err(AppError("sem resposta".into())))
        }
        fn cancel(&self, _conversation_id: &str) {}
    }

    fn setup(versioned: bool) -> (World, ManagePipelines, Arc<FakeVcs>) {
        let world = World::with_accounts(vec![]);
        let project = Project {
            id: "p1".into(),
            name: "Loja".into(),
            instructions: String::new(),
            folder: Some("/dev/loja".into()),
            versioned,
        };
        ProjectRepository::save(&*world.store, &project).unwrap();
        let vcs = Arc::new(FakeVcs::default());
        let uc = ManagePipelines {
            pipelines: world.store.clone(),
            agents: world.store.clone(),
            settings: world.store.clone(),
            catalog: Arc::new(FakeCatalog),
            projects: world.store.clone(),
            conversations: world.store.clone(),
            vcs: vcs.clone(),
            events: world.events.clone(),
            ids: Arc::new(SequentialIds::default()),
            clock: Arc::new(FixedClock(1000)),
            active: Mutex::default(),
            cancelled: Mutex::default(),
        };
        (world, uc, vcs)
    }

    #[test]
    fn final_answer_is_the_last_assistant_message_after_the_prompt() {
        let msg = |role: &str, content: &str| Message { id: 0, role: role.into(), content: content.into(), account_id: None, meta: None, created_at: 0 };
        let turn = [msg("assistant", "antigo"), msg("user", "faça"), msg("assistant", "lendo"), msg("tool", "{}"), msg("assistant", "pronto")];
        assert_eq!(final_answer(&turn).unwrap(), "pronto");
        assert!(final_answer(&[msg("assistant", "antigo"), msg("user", "faça"), msg("notice", "limite")]).is_err());
    }

    fn input(jira: Option<&str>) -> StartInput {
        StartInput { project_id: "p1".into(), jira_key: jira.map(Into::into), jira_site: None, title: String::new(), request: String::new(), budget_minutes: None, images: vec![], design: false }
    }

    fn roles(uc: &ManagePipelines, id: &str) -> Vec<(&'static str, &'static str)> {
        uc.detail(id).unwrap().stages.iter().map(|s| (s.role.key(), s.status.key())).collect()
    }

    #[tokio::test]
    async fn design_is_approved_before_the_plan() {
        let (world, uc, _) = setup(true);
        let run = uc.start(StartInput { design: true, ..input(Some("loja-8")) }).unwrap();
        assert!(run.design && !run.skipped.contains(&Role::Designer));
        let url = "https://claude.ai/artifact/AbC123xyz";
        let design = format!("Tela nova.\nDESIGN: {url}");
        let bots = Bots::new(vec![Ok("VEREDITO: CLARO"), Ok(&design)]);
        uc.advance(&run.id, &bots).await.unwrap();
        let detail = uc.detail(&run.id).unwrap();
        assert_eq!(detail.run.status, RunStatus::AwaitingApproval);
        assert_eq!(roles(&uc, &run.id), [("triage", "done"), ("designer", "done"), ("design_approval", "running")]);
        assert_eq!(detail.stages[2].input, design, "a parada mostra a especificação e o link");
        let designer_conv = world.conversation(&detail.stages[1].conversation_id);
        assert!(designer_conv.allowed_tools.iter().any(|t| t == ARTIFACT_TOOL), "o Designer pode publicar no Claude Design");
        assert!(world.store.flag(&format!("{ARTIFACTS_PREFIX}{}", designer_conv.id), false), "e o CLI liga a ferramenta só para ele");
        assert!(bots.prompts.lock().unwrap()[1].contains("intent \"design\""));

        // Ajuste pedido: o Designer refaz com o pedido e o link anterior.
        uc.decide_plan(&run.id, Some("botão maior")).unwrap();
        let bots = Bots::new(vec![Ok(&design)]);
        uc.advance(&run.id, &bots).await.unwrap();
        assert!(bots.prompts.lock().unwrap()[0].contains("botão maior"));
        uc.decide_plan(&run.id, None).unwrap();
        assert_eq!(uc.detail(&run.id).unwrap().stages[4].output.as_deref(), Some("Design aprovado."));

        // Aprovado: o Planejador recebe o design e pode lê-lo.
        let bots = Bots::new(vec![Ok("## Plano")]);
        uc.advance(&run.id, &bots).await.unwrap();
        assert!(bots.prompts.lock().unwrap()[0].contains("<design_aprovado>"));
        let detail = uc.detail(&run.id).unwrap();
        let planner_conv = world.conversation(&detail.stages[5].conversation_id);
        assert!(!planner_conv.allowed_tools.iter().any(|t| t == ARTIFACT_TOOL), "o Planejador só lê a especificação");
        assert!(!world.store.flag(&format!("{ARTIFACTS_PREFIX}{}", planner_conv.id), false));
        assert_eq!(detail.stages.last().unwrap().role, Role::Approval, "depois, a aprovação do plano de sempre");
    }

    #[tokio::test]
    async fn designer_without_a_published_link_stops_the_run() {
        let (_, uc, _) = setup(true);
        let run = uc.start(StartInput { design: true, ..input(Some("loja-8")) }).unwrap();
        let bots = Bots::new(vec![Ok("VEREDITO: CLARO"), Ok("Não consegui publicar.")]);
        uc.advance(&run.id, &bots).await.unwrap();
        let detail = uc.detail(&run.id).unwrap();
        assert_eq!(detail.run.status, RunStatus::Failed);
        assert!(detail.run.note.unwrap().contains("DESIGN:"));
    }

    #[tokio::test]
    async fn versioned_project_runs_the_whole_sequence_in_a_worktree() {
        let (world, uc, vcs) = setup(true);
        assert!(uc.start(input(None)).is_err(), "sem chave nem pedido");
        assert!(uc.start(input(Some("loja"))).is_err(), "chave inválida");
        let models = TierModels { chatgpt: BTreeMap::from([(crate::domain::pipeline::Tier::Strong, "gpt-forte".into())]), ..Default::default() };
        uc.save_models(models).unwrap();
        let run = uc.start(input(Some("loja-7"))).unwrap();
        assert_eq!((run.mode, run.workdir.as_str(), run.branch.as_deref()), (WorkMode::Worktree, "/dev/loja-esteira-loja-7", Some("lume/esteira-loja-7")));
        assert_eq!(vcs.commits.lock().unwrap()[0].1, "worktree /dev/loja-esteira-loja-7 lume/esteira-loja-7");

        let bots = Bots::new(vec![Ok("VEREDITO: CLARO"), Ok("## Plano\nfazer X")]);
        uc.advance(&run.id, &bots).await.unwrap();
        let detail = uc.detail(&run.id).unwrap();
        assert_eq!(detail.run.status, RunStatus::AwaitingApproval);
        assert_eq!(roles(&uc, &run.id), [("triage", "done"), ("planner", "done"), ("approval", "running")]);
        assert_eq!(detail.stages[2].input, "## Plano\nfazer X");
        let triage_conv = world.conversation(&detail.stages[0].conversation_id);
        let planner_conv = world.conversation(&detail.stages[1].conversation_id);
        assert_eq!(planner_conv.provider_models.get("chatgpt").map(String::as_str), Some("gpt-forte"), "o Planejador usa o modelo forte do ChatGPT");
        let planner = &detail.stage_models[&detail.stages[1].id];
        assert_eq!((planner.claude.as_str(), planner.others.get("chatgpt").map(String::as_str)), ("opus", Some("gpt-forte")));
        assert_eq!((triage_conv.cwd.as_str(), triage_conv.model.as_str(), triage_conv.permission_mode.as_str()), ("/dev/loja-esteira-loja-7", "haiku", "manual"));

        uc.decide_plan(&run.id, Some("use CSV")).unwrap();
        let bots = Bots::new(vec![Ok("plano com CSV")]);
        uc.advance(&run.id, &bots).await.unwrap();
        assert!(bots.prompts.lock().unwrap()[0].contains("use CSV"));
        assert!(uc.decide_plan(&run.id, None).is_ok());

        let bots = Bots::new(vec![
            Ok("mudei a.rs"),
            Ok("falta tratar erro\nVEREDITO: DEVOLVIDO"),
            Ok("tratei"),
            Ok("VEREDITO: APROVADO"),
            Ok("tudo passou\nVEREDITO: APROVADO"),
            Ok("Dois achados.\n```json\n[{\"id\": \"A1\", \"title\": \"CSV sem cabeçalho\", \"file\": \"a.rs\", \"line\": 3, \"options\": []},\
                {\"id\": \"A2\", \"title\": \"Separador\", \"options\": [\"vírgula\", \"ponto e vírgula\"]}]\n```"),
        ]);
        uc.advance(&run.id, &bots).await.unwrap();
        let prompts = bots.prompts.lock().unwrap().clone();
        assert!(prompts[0].contains("plano com CSV") && prompts[2].contains("falta tratar erro"));
        assert!(prompts[5].contains("chame a ferramenta Skill com `skill: \"code-review\"`") && prompts[5].contains("Não passe `--fix`"), "o Code review roda a skill do CLI, sem alterar nada");
        let detail = uc.detail(&run.id).unwrap();
        assert_eq!(detail.run.status, RunStatus::AwaitingChoice);
        let choice = detail.stages.last().unwrap();
        assert_eq!((choice.role, choice.status), (Role::ReviewChoice, StageStatus::Running));
        assert!(choice.input.contains("CSV sem cabeçalho"));
        assert!(uc.decide_plan(&run.id, None).is_err(), "não é a parada do plano");
        assert!(uc.decide_review(&run.id, &[ReviewPick { id: "A2".into(), option: None }], None).is_err(), "A2 pede a decisão");

        uc.decide_review(&run.id, &[ReviewPick { id: "A2".into(), option: Some(1) }], Some("mantenha a API")).unwrap();
        let bots = Bots::new(vec![
            Ok("troquei o separador"),
            Ok("VEREDITO: APROVADO"),
            Ok("tudo passou\nVEREDITO: APROVADO"),
            Ok("Relatório\nCOMMIT: Exporta CSV"),
        ]);
        uc.advance(&run.id, &bots).await.unwrap();
        let prompts = bots.prompts.lock().unwrap().clone();
        assert!(prompts[0].contains("Decisão do usuário: ponto e vírgula") && prompts[0].contains("mantenha a API") && !prompts[0].contains("CSV sem cabeçalho"));
        assert!(prompts[3].contains("<code_review>") && prompts[3].contains("Escolha do usuário"), "o Relator sabe o que o code review apontou");
        let detail = uc.detail(&run.id).unwrap();
        assert_eq!((detail.run.status, detail.run.rounds), (RunStatus::Done, 1), "as correções do code review não gastam voltas");
        let fix = detail.stages.iter().filter(|s| s.role == Role::Implementer).last().unwrap();
        assert_eq!(fix.round, 1, "a passagem das correções fica na volta em que estava");
        assert_eq!(detail.commit_message.as_deref(), Some("Exporta CSV"));
        let implementer = world.conversation(&detail.stages.iter().find(|s| s.role == Role::Implementer).unwrap().conversation_id);
        assert_eq!(implementer.permission_mode, "acceptEdits");

        uc.commit(&run.id, "Exporta CSV").unwrap();
        assert_eq!(vcs.commits.lock().unwrap().last().unwrap(), &("/dev/loja-esteira-loja-7".to_owned(), "Exporta CSV".to_owned()));
    }

    #[tokio::test]
    async fn project_config_gives_roles_their_agents_and_skips_optional_stages() {
        use crate::domain::extension::Agent;
        let (world, uc, _vcs) = setup(false);
        let agent = Agent { id: "a1".into(), name: "Revisor rigoroso".into(), instructions: "Seja rigoroso.".into(), model: None };
        AgentRepository::save(&*world.store, &agent).unwrap();
        let bad = PipelineConfig { agents: BTreeMap::from([(Role::Reviewer, "nao-existe".into())]), skipped: vec![], ..Default::default() };
        assert!(uc.save_config("p1", bad).is_err());
        assert!(uc.save_config("p1", PipelineConfig { agents: BTreeMap::new(), skipped: vec![Role::Planner], ..Default::default() }).is_err());
        let config = PipelineConfig {
            agents: BTreeMap::from([(Role::Reviewer, "a1".into())]),
            skipped: vec![Role::Reporter, Role::CodeReview, Role::Triage, Role::Tester],
            efforts: BTreeMap::from([(Role::Planner, "max".into())]),
            ..Default::default()
        };
        uc.save_config("p1", config).unwrap();
        assert_eq!(uc.config("p1").unwrap().skipped, [Role::Triage, Role::Tester, Role::CodeReview, Role::Reporter]);

        let image = |n: &str| Attachment { name: n.into(), kind: crate::domain::chat::AttachmentKind::Image, media_type: "image/png".into(), data: "iVBO".into() };
        let too_many = StartInput { images: (0..=MAX_IMAGES).map(|i| image(&i.to_string())).collect(), ..input(Some("loja-9")) };
        assert!(uc.start(too_many).is_err());
        let run = uc.start(StartInput { title: "Corrigir o cálculo do frete".into(), images: vec![image("a.png"), image("b.png")], ..input(Some("loja-9")) }).unwrap();
        assert_eq!((run.image_count, uc.images(&run.id).unwrap().len()), (2, 2));
        assert_eq!(run.skipped, [Role::Triage, Role::Tester, Role::CodeReview, Role::Reporter, Role::Designer, Role::DesignApproval], "sem design pedido, o Designer não roda");
        // Mudar o projeto depois de começar não muda a execução.
        uc.save_config("p1", PipelineConfig::default()).unwrap();
        assert_eq!(run.jira_url, None, "sem modelo, o site do Jira não é conhecido");
        let picked = uc.start(StartInput { jira_site: Some("https://loja.atlassian.net/".into()), ..input(Some("LOJA-10")) }).unwrap();
        assert_eq!(picked.jira_url.as_deref(), Some("https://loja.atlassian.net/browse/LOJA-10"), "o site vem da lista de onde a task foi escolhida");
        assert!(uc.start(StartInput { jira_site: Some("javascript:x".into()), ..input(Some("LOJA-11")) }).unwrap().jira_url.is_none());
        let bots = Bots::new(vec![Ok("plano")]);
        uc.advance(&run.id, &bots).await.unwrap();
        assert!(bots.prompts.lock().unwrap()[0].contains("Não houve triagem"), "o Planejador lê o Jira sozinho");
        assert_eq!(bots.images.lock().unwrap()[0], 2, "o Planejador recebe as imagens");
        uc.decide_plan(&run.id, None).unwrap();
        let bots = Bots::new(vec![Ok("fiz"), Ok("VEREDITO: APROVADO")]);
        uc.advance(&run.id, &bots).await.unwrap();
        assert_eq!(*bots.images.lock().unwrap(), [2, 0], "o Implementador vê as imagens; o Revisor, só o diff");
        let detail = uc.detail(&run.id).unwrap();
        assert_eq!(detail.run.status, RunStatus::Done);
        assert_eq!(roles(&uc, &run.id), [("planner", "done"), ("approval", "done"), ("implementer", "done"), ("reviewer", "done")]);
        assert_eq!(detail.run.agents.get(&Role::Reviewer).map(|a| a.name.as_str()), Some("Revisor rigoroso"));
        let reviewer_models = &detail.stage_models[&detail.stages.iter().find(|s| s.role == Role::Reviewer).unwrap().id];
        assert_eq!(reviewer_models.claude, "opus", "agente sem modelo próprio usa o do nível");
        let reviewer = detail.stages.iter().find(|s| s.role == Role::Reviewer).unwrap();
        assert_eq!(world.conversation(&reviewer.conversation_id).agent_id.as_deref(), Some("a1"));
        let planner = detail.stages.iter().find(|s| s.role == Role::Planner).unwrap();
        assert_eq!(world.conversation(&planner.conversation_id).effort, "max", "esforço escolhido para o papel");
        assert_eq!(world.conversation(&reviewer.conversation_id).effort, "medium", "sem escolha, o padrão");
        let implementer = detail.stages.iter().find(|s| s.role == Role::Implementer).unwrap();
        assert_eq!(world.conversation(&implementer.conversation_id).agent_id, None);

        // Agente apagado: sai da configuração e salvar de novo não esbarra nele.
        AgentRepository::delete(&*world.store, "a1").unwrap();
        let config = uc.config("p1").unwrap();
        assert!(config.agents.is_empty());
        uc.save_config("p1", PipelineConfig { skipped: vec![Role::Triage], ..config }).unwrap();
    }

    #[test]
    fn default_template_serves_projects_without_their_own() {
        let (_world, uc, _vcs) = setup(false);
        let default = "https://elinfo.atlassian.net/browse/EI-5198";
        assert!(uc.save_default_template(Some("não é task".into())).is_err());
        uc.save_default_template(Some(format!(" {default} "))).unwrap();
        let config = uc.config("p1").unwrap();
        assert_eq!((config.jira_template, config.default_jira_template.as_deref()), (None, Some(default)));
        let run = uc.start(StartInput { request: "Exportar pedidos em CSV".into(), ..input(None) }).unwrap();
        assert_eq!(run.jira_template.as_deref(), Some(default), "sem modelo no projeto, vale o padrão");
        // O do projeto vem antes; salvar o projeto não mexe no padrão.
        uc.save_config("p1", PipelineConfig { jira_template: Some("EI-1".into()), ..Default::default() }).unwrap();
        assert_eq!(uc.config("p1").unwrap().default_jira_template.as_deref(), Some(default));
        let run = uc.start(StartInput { request: "Outro pedido".into(), ..input(None) }).unwrap();
        assert_eq!(run.jira_template.as_deref(), Some("EI-1"));
    }

    #[tokio::test]
    async fn reporter_creates_the_task_from_the_project_template() {
        let (world, uc, _vcs) = setup(false);
        // O Atlassian também no ChatGPT e no Gemini: só o Relator sai com ele liberado para escrever.
        let bridges = r#"[{"name":"claude.ai Atlassian","display_name":"Atlassian","endpoint":"https://mcp.atlassian.com/v1/mcp"},{"name":"claude.ai Gmail","display_name":"Gmail","endpoint":"https://gmail.mcp.claude.com/mcp"}]"#;
        world.store.set_text(super::super::extensions::BRIDGES_KEY, bridges).unwrap();
        let template = "https://elinfo.atlassian.net/browse/EI-5198";
        assert!(uc.save_config("p1", PipelineConfig { jira_template: Some("não é task".into()), ..Default::default() }).is_err());
        uc.save_config("p1", PipelineConfig { jira_template: Some(format!(" {template} ")), skipped: vec![Role::Triage, Role::Tester], ..Default::default() }).unwrap();
        assert_eq!(uc.config("p1").unwrap().jira_template.as_deref(), Some(template));
        let run = uc.start(StartInput { request: "Exportar pedidos em CSV".into(), ..input(None) }).unwrap();
        assert_eq!(run.jira_template.as_deref(), Some(template));
        uc.advance(&run.id, &Bots::new(vec![Ok("plano")])).await.unwrap();
        uc.decide_plan(&run.id, None).unwrap();
        let bots = Bots::new(vec![Ok("fiz"), Ok("VEREDITO: APROVADO"), Ok("Nada a corrigir.\n```json\n[]\n```"), Ok("Relatório\nTASK_CRIADA: EI-6001\nCOMMIT: Exporta CSV")]);
        uc.advance(&run.id, &bots).await.unwrap();
        assert!(bots.prompts.lock().unwrap()[3].contains("seguindo o modelo https://elinfo.atlassian.net/browse/EI-5198"));
        let done = uc.detail(&run.id).unwrap().run;
        assert_eq!(done.status, RunStatus::Done);
        assert_eq!((done.jira_key.as_deref(), done.jira_url.as_deref()), (Some("EI-6001"), Some("https://elinfo.atlassian.net/browse/EI-6001")));
        let stages = uc.detail(&run.id).unwrap().stages;
        let allowed = |role: Role| world.conversation(&stages.iter().find(|s| s.role == role).unwrap().conversation_id).allowed_tools;
        assert_eq!(allowed(Role::Reporter), ["mcp__lume-atlassian"]);
        assert!(allowed(Role::Planner).is_empty() && allowed(Role::Implementer).is_empty());
    }

    #[tokio::test]
    async fn finished_run_reopens_with_the_user_adjustment_without_a_new_jira_task() {
        let (_world, uc, _vcs) = setup(false);
        uc.save_config("p1", PipelineConfig {
            jira_template: Some("https://elinfo.atlassian.net/browse/EI-5198".into()),
            skipped: vec![Role::Triage],
            ..Default::default()
        })
        .unwrap();
        let run = uc.start(StartInput { request: "Exportar pedidos em CSV".into(), ..input(None) }).unwrap();
        assert!(uc.reopen(&run.id, "x").is_err(), "só concluída");
        uc.advance(&run.id, &Bots::new(vec![Ok("plano")])).await.unwrap();
        uc.decide_plan(&run.id, None).unwrap();
        let first = Bots::new(vec![
            Ok("fiz"),
            Ok("falta X\nVEREDITO: DEVOLVIDO"),
            Ok("corrigi"),
            Ok("VEREDITO: APROVADO"),
            Ok("VEREDITO: APROVADO"),
            Ok("```json\n[]\n```"),
            Ok("TASK_CRIADA: https://elinfo.atlassian.net/browse/EI-6001\nCOMMIT: x"),
        ]);
        uc.advance(&run.id, &first).await.unwrap();
        assert_eq!(uc.detail(&run.id).unwrap().run.rounds, 1);

        assert!(uc.reopen(&run.id, "  ").is_err());
        uc.reopen(&run.id, "O botão exporta o arquivo vazio").unwrap();
        let again = Bots::new(vec![Ok("ajustei"), Ok("VEREDITO: APROVADO"), Ok("VEREDITO: APROVADO"), Ok("```json\n[]\n```"), Ok("Escrevi a Solução na EI-6001\nCOMMIT: y")]);
        uc.advance(&run.id, &again).await.unwrap();
        let prompts = again.prompts.lock().unwrap().clone();
        assert!(prompts[0].contains("<ajuste_do_usuario>\nO botão exporta o arquivo vazio") && !prompts[0].contains("falta X"), "o Implementador recebe o ajuste, não a devolução antiga");
        assert!(prompts[1].contains("Ajuste pedido pelo usuário depois de testar: O botão exporta o arquivo vazio"));
        assert!(prompts[3].contains("os caminhos dos arquivos que o Implementador alterou"), "cada reabertura é um ciclo novo, com o seu code review");
        assert!(prompts[4].contains("Escreva na task EI-6001") && prompts[4].contains("não comente") && !prompts[4].contains("crie uma no Jira"), "escreve a Solução na mesma task, sem criar outra");
        let detail = uc.detail(&run.id).unwrap();
        assert_eq!((detail.run.status, detail.run.rounds, detail.run.jira_key.as_deref()), (RunStatus::Done, 0, Some("EI-6001")));
        let implementer_rounds: Vec<i64> = detail.stages.iter().filter(|s| s.role == Role::Implementer).map(|s| s.round).collect();
        assert_eq!(implementer_rounds, [0, 1, 0], "a reabertura recomeça a contagem de voltas");
    }

    #[tokio::test]
    async fn run_stopped_at_the_round_limit_resumes_with_the_rounds_reset() {
        let (_world, uc, _vcs) = setup(false);
        uc.save_config("p1", PipelineConfig { skipped: vec![Role::Triage, Role::Tester, Role::CodeReview, Role::Reporter], ..Default::default() }).unwrap();
        let run = uc.start(StartInput { request: "Exportar pedidos em CSV".into(), ..input(None) }).unwrap();
        uc.advance(&run.id, &Bots::new(vec![Ok("plano")])).await.unwrap();
        uc.decide_plan(&run.id, None).unwrap();
        let mut replies = Vec::new();
        for i in 0..=MAX_ROUNDS {
            replies.push(Ok("fiz"));
            replies.push(Ok(if i == MAX_ROUNDS { "falta Y\nVEREDITO: DEVOLVIDO" } else { "falta X\nVEREDITO: DEVOLVIDO" }));
        }
        uc.advance(&run.id, &Bots::new(replies)).await.unwrap();
        let stopped = uc.detail(&run.id).unwrap();
        assert_eq!((stopped.run.status, stopped.run.rounds, stopped.round_limit), (RunStatus::Failed, MAX_ROUNDS, true));

        uc.retry(&run.id).unwrap();
        let bots = Bots::new(vec![Ok("corrigi Y"), Ok("VEREDITO: APROVADO")]);
        uc.advance(&run.id, &bots).await.unwrap();
        assert!(bots.prompts.lock().unwrap()[0].contains("falta Y"), "o Implementador recebe a última devolução");
        let detail = uc.detail(&run.id).unwrap();
        assert_eq!((detail.run.status, detail.run.rounds), (RunStatus::Done, 1), "a correção depois da retomada é a volta 1");
        assert!(detail.stages.iter().any(|s| s.role == Role::Resume) && !detail.round_limit);
    }

    #[tokio::test]
    async fn unversioned_project_works_in_the_folder_and_stops_on_vague_tasks() {
        let (_world, uc, vcs) = setup(false);
        let run = uc.start(StartInput { title: "Corrigir o cálculo do frete".into(), ..input(None) }).unwrap();
        assert_eq!((run.mode, run.workdir.as_str(), run.branch), (WorkMode::Direct, "/dev/loja", None));
        assert!(vcs.commits.lock().unwrap().is_empty(), "sem worktree");
        let bots = Bots::new(vec![Ok("não sei qual frete\nVEREDITO: VAGO")]);
        uc.advance(&run.id, &bots).await.unwrap();
        assert_eq!(uc.detail(&run.id).unwrap().run.status, RunStatus::NeedsInfo);
        assert!(uc.retry(&run.id).is_err());
        assert!(uc.commit(&run.id, "x").is_err());
        uc.delete(&run.id).unwrap();
        assert!(uc.list().unwrap().is_empty());

        let only_request = uc.start(StartInput { request: "Corrigir o frete\ncom CEP vazio".into(), ..input(None) }).unwrap();
        assert_eq!(only_request.title, "Corrigir o frete", "sem título, vale a primeira linha do pedido");
    }

    #[tokio::test]
    async fn failed_stage_is_retried_and_crash_is_recovered() {
        let (_world, uc, _vcs) = setup(false);
        let run = uc.start(StartInput { request: "Trocar a cor do botão".into(), title: "Cor".into(), ..input(None) }).unwrap();
        assert_eq!(run.jira_key, None);
        let bots = Bots::new(vec![Err(AppError("todas as contas no limite".into()))]);
        uc.advance(&run.id, &bots).await.unwrap();
        let detail = uc.detail(&run.id).unwrap();
        assert_eq!(detail.run.status, RunStatus::Failed);
        assert!(detail.run.note.unwrap().contains("todas as contas no limite"));

        uc.retry(&run.id).unwrap();
        let bots = Bots::new(vec![Ok("citou\nTASK: LOJA-12\nVEREDITO: CLARO"), Ok("plano")]);
        uc.advance(&run.id, &bots).await.unwrap();
        assert_eq!(uc.detail(&run.id).unwrap().run.jira_key, None, "só vale a task que o usuário escolheu");
        assert!(!bots.prompts.lock().unwrap()[1].contains("Task do Jira"));
        assert_eq!(roles(&uc, &run.id), [("triage", "failed"), ("triage", "done"), ("planner", "done"), ("approval", "running")]);

        // Cancelar na parada de aprovação encerra na hora.
        uc.cancel(&run.id, &bots).unwrap();
        assert_eq!(uc.detail(&run.id).unwrap().run.status, RunStatus::Cancelled);

        // Uma execução que estava rodando quando o app fechou vira falha retomável.
        let other = uc.start(StartInput { title: "Outra tarefa qualquer".into(), ..input(None) }).unwrap();
        let stage = StageRun {
            id: "s".into(),
            run_id: other.id.clone(),
            role: Role::Triage,
            round: 0,
            status: StageStatus::Running,
            conversation_id: "c".into(),
            input: String::new(),
            output: None,
            started_at: 1000,
            finished_at: None,
        };
        uc.pipelines.save_stage(&stage).unwrap();
        uc.recover().unwrap();
        assert_eq!(uc.detail(&other.id).unwrap().run.status, RunStatus::Failed);
        assert!(uc.retry(&other.id).is_ok());
    }
}
