//! Comandos expostos à interface. Só traduzem a chamada para um caso de uso.

use super::{windows, Container};
use crate::application::{extensions::SkillView, flags, store::ServerPage, tasks::TaskInput, AppError, AppResult};
use crate::domain::{
    account::Account,
    chat::{needs_user, Attachment, AttachmentKind, Conversation, Message, PermissionDecision, Switch},
    extension::{AccountConnector, Agent, AgentDraft, Connector},
    history::{HistSession, HistoryStatus},
    project::{MemoryFile, Project, ProjectFile},
    schedule::{Repeat, Task},
    store::{PluginListing, ServerListing},
    workspace::{FileEntry, FileView, WorkspaceStatus},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tauri::{AppHandle, State};

type App<'a> = State<'a, Arc<Container>>;

// ---- contas

#[tauri::command]
pub fn list_accounts(app: App<'_>) -> AppResult<Vec<Account>> {
    app.accounts.list()
}

#[tauri::command]
pub async fn add_account(app: App<'_>, name: String) -> AppResult<()> {
    app.accounts.add(&name).await
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
pub fn list_switches(app: App<'_>) -> AppResult<Vec<Switch>> {
    app.switches.recent()
}

// ---- conversas

#[tauri::command]
pub fn list_conversations(app: App<'_>) -> AppResult<Vec<Conversation>> {
    app.conversations.list()
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
/// `answers` responde às perguntas do agente; `next_mode` é o modo após aprovar um plano.
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
) -> AppResult<()> {
    if allow && always && !needs_user(&tool) {
        app.conversations.allow_tool(&conversation_id, &tool)?;
    }
    if let (true, Some(mode)) = (allow, &next_mode) {
        app.conversations.set_permission_mode(&conversation_id, mode)?;
    }
    let decision = PermissionDecision { request_id, allow, answers: answers.unwrap_or_default(), next_mode };
    app.running.decide(&conversation_id, decision);
    Ok(())
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
pub fn remove_project_file(app: App<'_>, file_id: String) -> AppResult<()> {
    app.projects.remove_file(&file_id)
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

#[tauri::command]
pub fn list_connectors(app: App<'_>) -> AppResult<Vec<Connector>> {
    app.connectors.list()
}

#[tauri::command]
pub fn add_connector(app: App<'_>, name: String, config: String) -> AppResult<()> {
    app.connectors.add(&name, &config).map(|_| ())
}

#[tauri::command]
pub fn import_desktop_connectors(app: App<'_>) -> AppResult<usize> {
    app.connectors.import_from_desktop()
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
