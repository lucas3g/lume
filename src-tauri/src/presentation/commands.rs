//! Comandos expostos à interface. Só traduzem a chamada para um caso de uso.

use super::{windows, Bots, Container};
use crate::application::{
    extensions::OtherServices,
    extensions::SkillView,
    flags,
    pipeline::{RunDetail, StartInput},
    prefs,
    store::ServerPage,
    tasks::TaskInput,
    AppError, AppResult,
};
use crate::domain::{
    office::{BotLook, OfficeLooks},
    account::{Account, Provider, ProviderInfo, UsageLimit},
    chat::{needs_user, Attachment, AttachmentKind, Conversation, Message, PermissionDecision, Switch},
    extension::{AccountConnector, Agent, AgentDraft, Connector},
    jira::{JiraBoards, JiraIssueList, JiraTaskBrief},
    history::{HistSession, HistoryStatus},
    pipeline::{Delivery, PipelineConfig, PipelineRun, ReviewPick, TierModels},
    project::{GitInfo, MemoryFile, Project, ProjectFile},
    schedule::{Repeat, Task},
    store::{PluginListing, ServerListing},
    workspace::{FileEntry, FileView, WorkspaceStatus},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use crate::infrastructure::cli_output::CodeScope;
use tauri::{AppHandle, State};

type App<'a> = State<'a, Arc<Container>>;

// ---- contas

#[tauri::command]
pub fn list_accounts(app: App<'_>) -> AppResult<Vec<Account>> {
    app.accounts.list()
}

#[tauri::command]
pub async fn add_account(app: App<'_>, name: String, provider: Option<Provider>) -> AppResult<()> {
    app.accounts.add(&name, provider.unwrap_or_default()).await
}

#[tauri::command]
pub fn list_providers(app: App<'_>) -> Vec<ProviderInfo> {
    app.accounts.providers()
}

#[tauri::command]
pub fn remove_account(app: App<'_>, id: String) -> AppResult<()> {
    app.accounts.remove(&id)
}

#[tauri::command]
pub fn set_account_enabled(app: App<'_>, id: String, enabled: bool) -> AppResult<()> {
    app.accounts.set_enabled(&id, enabled)
}

#[tauri::command]
pub fn rename_account(app: App<'_>, id: String, name: String) -> AppResult<()> {
    app.accounts.rename(&id, &name)
}

#[tauri::command]
pub fn reorder_accounts(app: App<'_>, ids: Vec<String>) -> AppResult<()> {
    app.accounts.reorder(&ids)
}

#[tauri::command]
pub async fn refresh_account(app: App<'_>, id: String) -> AppResult<()> {
    app.accounts.refresh(&id).await
}

#[tauri::command]
pub async fn login_account_again(app: App<'_>, id: String) -> AppResult<()> {
    app.accounts.login_again(&id).await
}

/// Consulta o uso atual de cada conta junto ao provedor; devolve quantas foram atualizadas.
#[tauri::command]
pub async fn refresh_usage(app: App<'_>) -> AppResult<usize> {
    app.accounts.refresh_usage().await
}

#[tauri::command]
pub async fn account_limits(app: App<'_>, id: String) -> AppResult<Vec<UsageLimit>> {
    app.accounts.limits(&id).await
}

/// Gasto de tokens dos últimos `days` dias, no fuso de quem vê, e o ritmo de cada conta.
#[tauri::command]
pub fn usage_report(app: App<'_>, days: i64, tz_offset_minutes: i64) -> AppResult<crate::application::usage::UsageReport> {
    app.usage.report(days, tz_offset_minutes)
}

#[tauri::command]
pub fn list_switches(app: App<'_>) -> AppResult<Vec<Switch>> {
    app.switches.recent()
}

// ---- conversas

#[tauri::command]
pub fn list_conversations(app: App<'_>) -> AppResult<Vec<Conversation>> {
    app.conversations.list()
}

/// Conversas cujas mensagens têm o termo, com o trecho em volta (para a busca global).
#[tauri::command]
pub fn search_messages(app: App<'_>, query: String) -> AppResult<Vec<crate::domain::search::MessageHit>> {
    app.conversations.search(&query)
}

#[tauri::command]
pub fn create_conversation(app: App<'_>, model: String) -> AppResult<Conversation> {
    app.conversations.create(&model)
}

/// Nova conversa com as mensagens anteriores a `before_message` (editar, regenerar ou ramificar).
#[tauri::command]
pub fn branch_conversation(app: App<'_>, id: String, before_message: i64) -> AppResult<Conversation> {
    app.conversations.branch(&id, before_message)
}

#[tauri::command]
pub fn delete_conversation(app: App<'_>, id: String) -> AppResult<()> {
    app.running.cancel(&id);
    app.terminals.close(&id);
    app.conversations.delete(&id)
}

#[tauri::command]
pub fn list_messages(app: App<'_>, conversation_id: String) -> AppResult<Vec<Message>> {
    app.conversations.messages(&conversation_id)
}

/// Passos dos subagentes da conversa, em ordem: `{ parent, step }`, com o passo em JSON.
#[tauri::command]
pub fn list_subagent_steps(app: App<'_>, conversation_id: String) -> AppResult<Vec<serde_json::Value>> {
    let steps = app.conversations.subagent_steps(&conversation_id)?;
    Ok(steps
        .into_iter()
        .filter_map(|(parent, step)| Some(serde_json::json!({ "parent": parent, "step": serde_json::from_str::<serde_json::Value>(&step).ok()? })))
        .collect())
}

#[tauri::command]
pub fn set_conversation_model(app: App<'_>, id: String, model: String) -> AppResult<()> {
    app.conversations.set_model(&id, &model)
}

#[tauri::command]
pub fn set_conversation_effort(app: App<'_>, id: String, effort: String) -> AppResult<()> {
    app.conversations.set_effort(&id, &effort)
}

#[tauri::command]
pub fn set_conversation_mode(app: App<'_>, id: String, mode: String) -> AppResult<()> {
    app.conversations.set_permission_mode(&id, &mode)
}

/// De onde vem a imagem a copiar.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum ImageSource {
    /// Imagem colada numa mensagem já gravada: o conteúdo é lido do banco.
    Stored { message_id: i64, index: usize },
    /// Mensagem ainda não gravada (só existe na tela): o conteúdo vem junto.
    Inline { data: String },
}

/// Copia uma imagem colada para a área de transferência do sistema.
#[tauri::command]
pub async fn copy_image(app: App<'_>, source: ImageSource) -> AppResult<()> {
    let data = match source {
        ImageSource::Stored { message_id, index } => app.conversations.message_image(message_id, index)?,
        ImageSource::Inline { data } => data,
    };
    tauri::async_runtime::spawn_blocking(move || crate::infrastructure::clipboard::copy_image(&data))
        .await
        .map_err(|e| AppError(e.to_string()))?
}

/// Imagem colada no campo de mensagem: já chega com o conteúdo em base64.
#[derive(Deserialize)]
pub struct PastedImage {
    name: String,
    media_type: String,
    data: String,
}

/// Inicia o turno em segundo plano; o andamento chega pelos eventos de conversa.
#[tauri::command]
pub fn send_message(
    app: App<'_>,
    conversation_id: String,
    text: String,
    attachments: Vec<String>,
    images: Vec<PastedImage>,
) -> AppResult<()> {
    let text = text.trim().to_owned();
    if text.is_empty() && images.is_empty() {
        return Err(AppError("Escreva uma mensagem.".into()));
    }
    let text = if text.is_empty() { "Veja as imagens anexadas.".to_owned() } else { text };
    let pasted: Vec<Attachment> = images
        .into_iter()
        .map(|i| Attachment { name: i.name, kind: AttachmentKind::Image, media_type: i.media_type, data: i.data })
        .collect();
    if !app.spawn_turn(conversation_id, text, attachments, pasted) {
        return Err(AppError("Aguarde a resposta em andamento.".into()));
    }
    Ok(())
}

/// Prazo para o motor confirmar a entrega; motores que não leem mensagens no meio do turno não respondem.
const INJECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Entrega a mensagem ao turno em andamento para o agente ler assim que puder.
/// `false` quando não há turno ou o motor não aceita: a interface envia depois, ao fim do turno.
#[tauri::command]
pub async fn inject_message(
    app: App<'_>,
    conversation_id: String,
    id: String,
    text: String,
    attachments: Vec<String>,
    images: Vec<PastedImage>,
) -> AppResult<bool> {
    let text = text.trim().to_owned();
    let text = if text.is_empty() { "Veja as imagens anexadas.".to_owned() } else { text };
    let pasted: Vec<Attachment> = images
        .into_iter()
        .map(|i| Attachment { name: i.name, kind: AttachmentKind::Image, media_type: i.media_type, data: i.data })
        .collect();
    let (injection, meta) = app.send_message.injection(&id, &text, &attachments, pasted)?;
    let Some(accepted) = app.running.inject(&conversation_id, injection, (text, meta)) else {
        return Ok(false);
    };
    let accepted = matches!(tokio::time::timeout(INJECT_TIMEOUT, accepted).await, Ok(Ok(true)));
    if !accepted {
        app.running.forget_injection(&conversation_id, &id);
    }
    Ok(accepted)
}

// ---- tarefas agendadas

/// Cadência e dados de uma tarefa, como a interface envia.
#[derive(Deserialize)]
pub struct TaskForm {
    id: Option<String>,
    name: String,
    prompt: String,
    project_id: Option<String>,
    model: String,
    repeat: Repeat,
    tz_offset_minutes: i64,
}

#[tauri::command]
pub fn list_tasks(app: App<'_>) -> AppResult<Vec<Task>> {
    app.tasks.list()
}

#[tauri::command]
pub fn save_task(app: App<'_>, task: TaskForm) -> AppResult<Task> {
    let TaskForm { id, name, prompt, project_id, model, repeat, tz_offset_minutes } = task;
    app.tasks.save(TaskInput { id, name, prompt, project_id, model, repeat, tz_offset_minutes })
}

#[tauri::command]
pub fn set_task_enabled(app: App<'_>, id: String, enabled: bool) -> AppResult<()> {
    app.tasks.set_enabled(&id, enabled)
}

#[tauri::command]
pub fn delete_task(app: App<'_>, id: String) -> AppResult<()> {
    app.tasks.delete(&id)
}

/// Roda a tarefa agora e devolve a conversa em que ela está rodando.
#[tauri::command]
pub fn run_task(app: App<'_>, id: String) -> AppResult<Conversation> {
    app.run_task(&id)
}

/// Abre uma sessão de agente na pasta; a pasta vira (ou reutiliza) um projeto.
#[tauri::command]
pub fn start_folder_session(app: App<'_>, folder: String, model: String) -> AppResult<Conversation> {
    app.projects.start_session(&folder, &model)
}

/// Sessão em uma cópia isolada (git worktree) do projeto, em um ramo novo.
#[tauri::command(async)]
pub fn start_isolated_session(app: App<'_>, project_id: String, model: String) -> AppResult<Conversation> {
    app.projects.start_isolated_session(&project_id, &model)
}

/// Resposta do usuário a um pedido de autorização; `always` libera a ferramenta na conversa.
/// `answers` responde às perguntas do agente; `next_mode` é o modo após aprovar um plano;
/// `feedback`, ao recusar, diz ao agente o que mudar.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn decide_permission(
    app: App<'_>,
    conversation_id: String,
    request_id: String,
    tool: String,
    allow: bool,
    always: bool,
    answers: Option<Vec<(String, String)>>,
    next_mode: Option<String>,
    feedback: Option<String>,
) -> AppResult<()> {
    if allow && always && !needs_user(&tool) {
        app.conversations.allow_tool(&conversation_id, &tool)?;
    }
    if let (true, Some(mode)) = (allow, &next_mode) {
        app.conversations.set_permission_mode(&conversation_id, mode)?;
    }
    let decision = PermissionDecision { request_id, allow, answers: answers.unwrap_or_default(), next_mode, feedback };
    app.running.decide(&conversation_id, decision);
    Ok(())
}

/// Manda para segundo plano um comando (ou subagente) em execução no turno da conversa;
/// devolve se o pedido chegou à CLI (o turno pode já ter acabado, ou o comando terminado).
#[tauri::command]
pub async fn background_tool(app: App<'_>, conversation_id: String, tool_id: String) -> AppResult<bool> {
    let Some(sent) = app.running.background(&conversation_id, &tool_id) else { return Ok(false) };
    Ok(sent.await.unwrap_or(false))
}

/// Conversas paradas no limite de todas as contas, esperando uma voltar: id → quando a primeira volta.
#[tauri::command]
pub fn list_parked(app: App<'_>) -> HashMap<String, Option<i64>> {
    app.parked.all().into_iter().filter(|(_, p)| !p.stage).map(|(id, p)| (id, p.resets_at)).collect()
}

/// A conversa parada no limite deixa de esperar: não continua sozinha quando uma conta voltar.
#[tauri::command]
pub fn forget_parked(app: App<'_>, conversation_id: String) -> AppResult<()> {
    app.parked.take(&conversation_id).map(|_| ())
}

#[tauri::command]
pub fn cancel_message(app: App<'_>, conversation_id: String) {
    app.running.cancel(&conversation_id);
}

// ---- configurações

#[tauri::command]
pub fn get_flags(app: App<'_>) -> HashMap<&'static str, bool> {
    flags::ALL.iter().map(|key| (*key, app.settings.flag(key, true))).collect()
}

#[tauri::command]
pub fn set_flag(app: App<'_>, key: String, value: bool) -> AppResult<()> {
    if !flags::ALL.contains(&key.as_str()) {
        return Err(AppError("Configuração desconhecida.".into()));
    }
    app.settings.set_flag(&key, value)
}

#[tauri::command]
pub fn get_project_order(app: App<'_>) -> Option<String> {
    app.settings.text(prefs::PROJECT_ORDER).filter(|v| prefs::PROJECT_ORDERS.contains(&v.as_str()))
}

#[tauri::command]
pub fn set_project_order(app: App<'_>, order: String) -> AppResult<()> {
    if !prefs::PROJECT_ORDERS.contains(&order.as_str()) {
        return Err(AppError("Ordem desconhecida.".into()));
    }
    app.settings.set_text(prefs::PROJECT_ORDER, &order)
}

#[tauri::command]
pub fn get_pipeline_view(app: App<'_>) -> Option<String> {
    app.settings.text(prefs::PIPELINE_VIEW).filter(|v| prefs::PIPELINE_VIEWS.contains(&v.as_str()))
}

#[tauri::command]
pub fn set_pipeline_view(app: App<'_>, view: String) -> AppResult<()> {
    if !prefs::PIPELINE_VIEWS.contains(&view.as_str()) {
        return Err(AppError("Visualização desconhecida.".into()));
    }
    app.settings.set_text(prefs::PIPELINE_VIEW, &view)
}

// ---- histórico

#[tauri::command]
pub fn history_status(app: App<'_>) -> HistoryStatus {
    app.history.status()
}

#[tauri::command]
pub fn history_list(app: App<'_>, query: String) -> Vec<HistSession> {
    app.history.list(&query)
}

#[tauri::command]
pub async fn history_reindex(app: App<'_>) -> AppResult<usize> {
    let container = Arc::clone(&app);
    tauri::async_runtime::spawn_blocking(move || container.history.reindex())
        .await
        .map_err(|e| AppError(format!("A indexação falhou: {e}")))
}

#[tauri::command]
pub fn history_open(app: App<'_>, session_id: String, model: String) -> AppResult<Conversation> {
    app.history.open(&session_id, &model)
}

// ---- skills e agentes

#[tauri::command]
pub fn list_skills(app: App<'_>, scope: String) -> AppResult<Vec<SkillView>> {
    app.skills.list(&scope)
}

#[tauri::command]
pub fn set_skill(app: App<'_>, scope: String, skill: String, enabled: bool) -> AppResult<()> {
    app.skills.set(&scope, &skill, enabled)
}

#[tauri::command]
pub fn list_agents(app: App<'_>) -> AppResult<Vec<Agent>> {
    app.agents.list()
}

#[tauri::command]
pub fn save_agent(
    app: App<'_>,
    id: Option<String>,
    name: String,
    instructions: String,
    model: Option<String>,
) -> AppResult<Agent> {
    app.agents.save(id, &name, &instructions, model)
}

/// Instruções e skills sugeridas pelo modelo para um agente, a partir do nome.
#[tauri::command]
pub async fn draft_agent(app: App<'_>, name: String) -> AppResult<AgentDraft> {
    app.draft_agent.draft(&name).await
}

#[tauri::command]
pub fn delete_agent(app: App<'_>, id: String) -> AppResult<()> {
    app.agents.delete(&id)
}

#[tauri::command]
pub fn set_conversation_agent(app: App<'_>, id: String, agent_id: Option<String>) -> AppResult<()> {
    app.agents.assign(&id, agent_id.as_deref())
}

// ---- projetos

#[tauri::command]
pub fn list_projects(app: App<'_>) -> AppResult<Vec<Project>> {
    app.projects.list()
}

#[tauri::command]
pub fn save_project(app: App<'_>, id: Option<String>, name: String, instructions: String) -> AppResult<Project> {
    app.projects.save(id, &name, &instructions)
}

/// O git da pasta do projeto (consultar o `gh` pode demorar: fora da thread principal).
#[tauri::command(async)]
pub fn project_git_info(app: App<'_>, id: String) -> AppResult<GitInfo> {
    app.projects.git_info(&id)
}

/// Com git na pasta, diz se a esteira usa uma cópia isolada ou trabalha direto na pasta.
#[tauri::command]
pub fn set_project_isolate(app: App<'_>, id: String, isolate: bool) -> AppResult<Project> {
    app.projects.set_isolate(&id, isolate)
}

#[tauri::command]
pub fn delete_project(app: App<'_>, id: String) -> AppResult<()> {
    app.projects.delete(&id)
}

#[tauri::command]
pub fn list_project_files(app: App<'_>, project_id: String) -> AppResult<Vec<ProjectFile>> {
    app.projects.files(&project_id)
}

#[tauri::command]
pub fn add_project_files(app: App<'_>, project_id: String, paths: Vec<String>) -> AppResult<()> {
    app.projects.add_files(&project_id, &paths)
}

#[tauri::command]
pub fn remove_project_file(app: App<'_>, project_id: String, file_id: String) -> AppResult<()> {
    app.projects.remove_file(&project_id, &file_id)
}

// ---- memória do projeto (lê e grava em disco)

#[tauri::command(async)]
pub fn project_memory_list(app: App<'_>, project_id: String) -> AppResult<Vec<MemoryFile>> {
    app.projects.memory_files(&project_id)
}

#[tauri::command(async)]
pub fn project_memory_read(app: App<'_>, project_id: String, name: String) -> AppResult<String> {
    app.projects.memory_read(&project_id, &name)
}

#[tauri::command(async)]
pub fn project_memory_write(app: App<'_>, project_id: String, name: String, content: String) -> AppResult<()> {
    app.projects.memory_write(&project_id, &name, &content)
}

#[tauri::command(async)]
pub fn project_memory_remove(app: App<'_>, project_id: String, name: String) -> AppResult<()> {
    app.projects.memory_remove(&project_id, &name)
}

#[tauri::command]
pub fn set_conversation_project(app: App<'_>, id: String, project_id: Option<String>) -> AppResult<()> {
    app.projects.assign(&id, project_id.as_deref())
}

// ---- pasta da sessão (rodam fora da thread da interface: leem disco e chamam o git)

#[tauri::command(async)]
pub fn workspace_entries(app: App<'_>, conversation_id: String, dir: String) -> AppResult<Vec<FileEntry>> {
    app.workspace.entries(&conversation_id, &dir)
}

#[tauri::command(async)]
pub fn workspace_read(app: App<'_>, conversation_id: String, path: String) -> AppResult<FileView> {
    app.workspace.read(&conversation_id, &path)
}

#[tauri::command(async)]
pub fn workspace_status(app: App<'_>, conversation_id: String) -> AppResult<WorkspaceStatus> {
    app.workspace.status(&conversation_id)
}

#[tauri::command(async)]
pub fn workspace_diff(app: App<'_>, conversation_id: String, path: String) -> AppResult<String> {
    app.workspace.diff(&conversation_id, &path)
}

#[tauri::command(async)]
pub fn workspace_commit(app: App<'_>, conversation_id: String, message: String) -> AppResult<()> {
    app.workspace.commit(&conversation_id, &message)
}

// ---- terminal integrado

#[tauri::command(async)]
pub fn terminal_open(app: App<'_>, conversation_id: String, cols: u16, rows: u16) -> AppResult<String> {
    app.terminals.open(&conversation_id, cols, rows)
}

#[tauri::command(async)]
pub fn terminal_write(app: App<'_>, id: String, data: String) -> AppResult<()> {
    app.terminals.write(&id, data.as_bytes())
}

#[tauri::command(async)]
pub fn terminal_resize(app: App<'_>, id: String, cols: u16, rows: u16) -> AppResult<()> {
    app.terminals.resize(&id, cols, rows)
}

#[tauri::command(async)]
pub fn terminal_close(app: App<'_>, id: String) {
    app.terminals.close(&id);
}

// ---- conectores

/// Conectores carregados pelo Claude Code para a conta ativa (consulta a situação de cada um).
#[tauri::command(async)]
pub fn list_account_connectors(app: App<'_>) -> AppResult<Vec<AccountConnector>> {
    app.connectors.account_list()
}

/// Entrega ao login em andamento (de conta ou de conector) o código de autenticação
/// que o navegador mostrou para colar no CLI.
#[tauri::command]
pub fn submit_login_code(scope: CodeScope, code: String) -> AppResult<()> {
    crate::infrastructure::cli_output::submit_login_code(scope, &code)
}

/// Abre o login do conector no navegador e espera a conclusão.
#[tauri::command(async)]
pub fn link_connector(app: App<'_>, name: String) -> AppResult<()> {
    app.connectors.link(&name)
}

#[tauri::command(async)]
pub fn unlink_connector(app: App<'_>, name: String) -> AppResult<()> {
    app.connectors.unlink(&name)
}

/// Ícone do serviço de um conector (data URL), buscado na primeira vez e guardado em disco.
#[tauri::command(async)]
pub fn connector_icon(app: App<'_>, endpoint: String) -> AppResult<Option<String>> {
    app.connectors.icon(&endpoint)
}

#[tauri::command]
pub fn set_server_enabled(app: App<'_>, name: String, enabled: bool) -> AppResult<()> {
    app.connectors.set_server_enabled(&name, enabled)
}

/// Conectores no ChatGPT e no Gemini: o que está instalado e a situação de cada ponte.
#[tauri::command]
pub async fn connectors_elsewhere(app: App<'_>) -> AppResult<OtherServices> {
    app.connectors.other_services().await
}

/// Liga ou desliga o uso de um conector da conta claude.ai também no ChatGPT e no Gemini.
#[tauri::command(async)]
pub fn set_connector_bridge(app: App<'_>, name: String, display_name: String, endpoint: String, on: bool) -> AppResult<()> {
    app.connectors.set_bridge(&name, &display_name, &endpoint, on)
}

/// Entra no conector pelo ChatGPT (Codex), em cada conta do ChatGPT que ainda não entrou.
#[tauri::command]
pub async fn login_connector_chatgpt(app: App<'_>, name: String) -> AppResult<()> {
    app.connectors.login_chatgpt(&name).await
}

#[tauri::command]
pub fn list_connectors(app: App<'_>) -> AppResult<Vec<Connector>> {
    app.connectors.list()
}

#[tauri::command]
pub fn add_connector(app: App<'_>, name: String, config: String) -> AppResult<()> {
    app.connectors.add(&name, &config).map(|_| ())
}

#[tauri::command]
pub fn import_desktop_connectors(app: App<'_>, taken: Vec<String>) -> AppResult<usize> {
    app.connectors.import_from_desktop(&taken)
}

#[tauri::command]
pub fn set_connector_enabled(app: App<'_>, id: String, enabled: bool) -> AppResult<()> {
    app.connectors.set_enabled(&id, enabled)
}

#[tauri::command]
pub fn remove_connector(app: App<'_>, id: String) -> AppResult<()> {
    app.connectors.remove(&id)
}

// ---- loja (acessam disco, rede e o CLI: fora da thread da interface)

#[tauri::command(async)]
pub fn store_plugins(app: App<'_>, query: String, category: Option<String>) -> AppResult<Vec<PluginListing>> {
    app.store.plugins(&query, category.as_deref())
}

#[tauri::command(async)]
pub fn store_categories(app: App<'_>) -> AppResult<Vec<String>> {
    app.store.categories()
}

#[tauri::command(async)]
pub fn store_install_plugin(app: App<'_>, id: String) -> AppResult<()> {
    app.store.install_plugin(&id)
}

#[tauri::command(async)]
pub fn store_uninstall_plugin(app: App<'_>, id: String) -> AppResult<()> {
    app.store.uninstall_plugin(&id)
}

#[tauri::command(async)]
pub fn store_refresh_catalog(app: App<'_>) -> AppResult<()> {
    app.store.refresh_catalog()
}

#[tauri::command(async)]
pub fn store_servers(app: App<'_>, query: String, cursor: Option<String>) -> AppResult<ServerPage> {
    app.store.servers(&query, cursor.as_deref())
}

#[tauri::command(async)]
pub fn store_add_server(app: App<'_>, listing: ServerListing, values: HashMap<String, String>) -> AppResult<()> {
    app.store.add_server(&listing, &values)
}

// ---- janelas e arquivos

/// A entrada rápida enviou uma mensagem: mostra a conversa na janela principal.
#[tauri::command]
pub fn quick_entry_sent(app_handle: AppHandle, conversation_id: String) {
    windows::open_conversation(&app_handle, &conversation_id);
}

#[tauri::command]
pub fn hide_quick_entry(app_handle: AppHandle) {
    windows::hide_quick_entry(&app_handle);
}

/// Abre um artifact do Claude Design na janela própria do Lume.
#[tauri::command]
pub async fn open_design(app_handle: AppHandle, url: String) -> AppResult<()> {
    windows::open_design(&app_handle, &url).map_err(AppError)
}

/// Salva um texto (ex.: o código de um artifact) no caminho escolhido pelo usuário.
#[tauri::command]
pub fn save_text_file(app: App<'_>, path: String, content: String) -> AppResult<()> {
    app.exports.save(&path, &content)
}

// ---- ambiente

#[derive(Serialize)]
pub struct Environment {
    cli_found: bool,
}

#[tauri::command]
pub fn environment(app: App<'_>) -> Environment {
    Environment { cli_found: app.cli_found }
}

// ---- esteira de bots

/// Agente de cada papel e etapas desligadas da esteira no projeto.
#[tauri::command]
pub fn pipeline_config(app: App<'_>, project_id: String) -> AppResult<PipelineConfig> {
    app.pipelines.config(&project_id)
}

#[tauri::command]
pub fn save_pipeline_config(app: App<'_>, project_id: String, config: PipelineConfig) -> AppResult<PipelineConfig> {
    app.pipelines.save_config(&project_id, config)
}

/// Modelo de task do Jira de todos os projetos (o de cada projeto vem antes).
#[tauri::command]
pub fn pipeline_default_template(app: App<'_>) -> Option<String> {
    app.pipelines.default_template()
}

#[tauri::command]
pub fn save_pipeline_default_template(app: App<'_>, template: Option<String>) -> AppResult<Option<String>> {
    app.pipelines.save_default_template(template)
}

/// Modelo de cada nível (rápido, equilibrado, forte) em cada serviço, para a esteira.
#[tauri::command]
pub fn pipeline_models(app: App<'_>) -> TierModels {
    app.pipelines.models()
}

/// Modelos que o CLI do serviço aceita (Codex e Antigravity); vazio para o Claude.
#[tauri::command]
pub async fn cli_models(app: App<'_>, provider: Provider) -> AppResult<Vec<String>> {
    app.pipelines.cli_models(provider).await
}

#[tauri::command]
pub fn save_pipeline_models(app: App<'_>, models: TierModels) -> AppResult<TierModels> {
    app.pipelines.save_models(models)
}

/// Visual dos bonecos do escritório, por papel.
#[tauri::command]
pub fn office_looks(app: App<'_>) -> OfficeLooks {
    app.pipelines.looks()
}

#[tauri::command]
pub fn save_office_looks(app: App<'_>, looks: OfficeLooks) -> AppResult<OfficeLooks> {
    app.pipelines.save_looks(looks)
}

/// Visual de boneco sugerido pelo modelo a partir de uma foto (não salva: só preenche o editor).
#[tauri::command]
pub async fn draft_bot_look(app: App<'_>, image: PastedImage) -> AppResult<BotLook> {
    let image = Attachment { name: image.name, kind: AttachmentKind::Image, media_type: image.media_type, data: image.data };
    app.draft_look.draft(image).await
}

#[tauri::command]
pub fn list_pipelines(app: App<'_>) -> AppResult<Vec<PipelineRun>> {
    app.pipelines.list()
}

#[tauri::command]
pub fn pipeline_detail(app: App<'_>, id: String) -> AppResult<RunDetail> {
    app.pipelines.detail(&id)
}

/// Tasks do Jira pelo conector do Atlassian, para escolher a da esteira; `query` busca em todo o Jira.
#[tauri::command]
pub async fn list_jira_issues(app: App<'_>, query: Option<String>, statuses: Option<Vec<String>>) -> AppResult<JiraIssueList> {
    app.jira_issues.list(query.as_deref(), &statuses.unwrap_or_default()).await
}

/// Lê a task do Jira escolhida e sugere o título e o que fazer, para o formulário da esteira (não salva).
#[tauri::command]
pub async fn brief_jira_task(app: App<'_>, key: String, site: Option<String>) -> AppResult<JiraTaskBrief> {
    app.jira_issues.brief(&key, site.as_deref()).await
}

/// Reescreve o título e o que fazer digitados (sem task do Jira) num pedido mais claro para a Triagem (não salva).
#[tauri::command]
pub async fn draft_pipeline_request(app: App<'_>, title: String, request: String, images: Option<Vec<PastedImage>>) -> AppResult<JiraTaskBrief> {
    let images = images
        .unwrap_or_default()
        .into_iter()
        .map(|i| Attachment { name: i.name, kind: AttachmentKind::Image, media_type: i.media_type, data: i.data })
        .collect();
    app.draft_request.draft(&title, &request, images).await
}

/// Reescreve a mensagem digitada no chat num texto mais claro e organizado (não envia).
#[tauri::command]
pub async fn draft_chat_message(app: App<'_>, text: String, images: Option<Vec<PastedImage>>) -> AppResult<String> {
    let images = images
        .unwrap_or_default()
        .into_iter()
        .map(|i| Attachment { name: i.name, kind: AttachmentKind::Image, media_type: i.media_type, data: i.data })
        .collect();
    app.draft_request.draft_message(&text, images).await
}

/// Quadros do Jira e as colunas de cada um, para listar as tasks de uma coluna.
#[tauri::command]
pub async fn list_jira_boards(app: App<'_>) -> AppResult<JiraBoards> {
    app.jira_issues.boards().await
}

/// Cria a execução (reservando a cópia isolada, se o projeto tem versionamento) e põe os bots para trabalhar;
/// a cópia é criada em segundo plano, antes da primeira etapa.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn start_pipeline(
    app: App<'_>,
    project_id: String,
    jira_key: Option<String>,
    jira_site: Option<String>,
    title: String,
    request: String,
    budget_minutes: Option<i64>,
    images: Option<Vec<PastedImage>>,
    design: Option<bool>,
    create_jira: Option<bool>,
) -> AppResult<PipelineRun> {
    let images = images
        .unwrap_or_default()
        .into_iter()
        .map(|i| Attachment { name: i.name, kind: AttachmentKind::Image, media_type: i.media_type, data: i.data })
        .collect();
    let run = app.pipelines.start(StartInput { project_id, jira_key, jira_site, title, request, budget_minutes, images, design: design.unwrap_or(false), create_jira: create_jira.unwrap_or(true) })?;
    app.spawn_pipeline(run.id.clone());
    Ok(run)
}

/// Imagens coladas no pedido da execução (base64), para as miniaturas.
#[tauri::command]
pub fn pipeline_images(app: App<'_>, id: String) -> AppResult<Vec<ImageOut>> {
    Ok(app.pipelines.images(&id)?.into_iter().map(|i| ImageOut { name: i.name, media_type: i.media_type, data: i.data }).collect())
}

#[derive(Serialize)]
pub struct ImageOut {
    name: String,
    media_type: String,
    data: String,
}

/// Aprova o plano ou o design (sem `feedback`) ou pede ajustes ao Planejador ou ao Designer.
#[tauri::command]
pub fn decide_pipeline_plan(app: App<'_>, id: String, feedback: Option<String>) -> AppResult<()> {
    app.pipelines.decide_plan(&id, feedback.as_deref())?;
    app.spawn_pipeline(id);
    Ok(())
}

/// Escolha do usuário sobre o code review: os achados a corrigir e uma observação livre.
#[tauri::command]
pub fn decide_pipeline_review(app: App<'_>, id: String, picks: Vec<ReviewPick>, note: Option<String>) -> AppResult<()> {
    app.pipelines.decide_review(&id, &picks, note.as_deref())?;
    app.spawn_pipeline(id);
    Ok(())
}

/// Reabre uma execução concluída com o ajuste que o usuário encontrou ao testar (texto e prints).
#[tauri::command]
pub fn reopen_pipeline(app: App<'_>, id: String, feedback: String, images: Option<Vec<PastedImage>>) -> AppResult<()> {
    let images: Vec<Attachment> = images
        .unwrap_or_default()
        .into_iter()
        .map(|i| Attachment { name: i.name, kind: AttachmentKind::Image, media_type: i.media_type, data: i.data })
        .collect();
    app.pipelines.reopen(&id, &feedback, &images)?;
    app.spawn_pipeline(id);
    Ok(())
}

#[tauri::command]
pub fn cancel_pipeline(app: App<'_>, id: String) -> AppResult<()> {
    app.pipelines.cancel(&id, &Bots(Arc::clone(&app)))
}

/// Métricas das execuções da esteira do projeto.
#[tauri::command]
pub fn pipeline_metrics(app: App<'_>, project_id: String) -> AppResult<crate::domain::pipeline_metrics::PipelineMetrics> {
    app.insights.metrics(&project_id)
}

/// Lições da esteira gravadas no conhecimento do projeto, se houver.
#[tauri::command]
pub fn pipeline_lessons(app: App<'_>, project_id: String) -> AppResult<Option<String>> {
    app.projects.file_content(&project_id, crate::application::pipeline_insights::LESSONS_FILE)
}

/// Sugere lições a partir do que os bots erraram, juntando as que já estão gravadas. Não grava nada.
#[tauri::command]
pub async fn draft_pipeline_lessons(app: App<'_>, project_id: String) -> AppResult<String> {
    let current = app.projects.file_content(&project_id, crate::application::pipeline_insights::LESSONS_FILE)?;
    app.insights.draft_lessons(&project_id, current.as_deref()).await
}

/// Grava as lições no conhecimento do projeto (vazio apaga o arquivo).
#[tauri::command]
pub fn save_pipeline_lessons(app: App<'_>, project_id: String, text: String) -> AppResult<()> {
    app.projects.put_file(&project_id, crate::application::pipeline_insights::LESSONS_FILE, &text)
}

/// Para de acompanhar o CI do pull request da execução.
#[tauri::command]
pub fn stop_pipeline_ci(app: App<'_>, id: String) -> AppResult<()> {
    app.pipelines.stop_ci(&id)
}

/// A execução parada no limite de todas as contas deixa de esperar: só volta se você retomar.
#[tauri::command]
pub fn stop_pipeline_waiting(app: App<'_>, id: String) -> AppResult<()> {
    app.pipelines.stop_waiting(&id)
}

#[tauri::command]
pub fn retry_pipeline(app: App<'_>, id: String) -> AppResult<()> {
    app.pipelines.retry(&id)?;
    app.spawn_pipeline(id);
    Ok(())
}

/// Entrega o resultado: commit na main, num ramo próprio ou num pull request.
#[tauri::command(async)]
pub fn deliver_pipeline(app: App<'_>, id: String, kind: Delivery, message: String, push: bool) -> AppResult<PipelineRun> {
    app.pipelines.deliver(&id, kind, &message, push)
}

#[tauri::command]
pub fn delete_pipeline(app: App<'_>, id: String) -> AppResult<()> {
    app.pipelines.delete(&id)
}
