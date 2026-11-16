// Único ponto de contato da interface com o núcleo Rust.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { CodeScope } from "./state/logins.svelte";
import type {
  Account, AccountConnector, Agent, SubagentStep, AgentDraft, ChatEvent, Connector, Conversation, FileEntry, FileView, FlagKey, Flags, HistSession, HistoryStatus,
  MemoryFile, Message, PipelineDetail, PipelineRun, PipelineStatus, PastedImage, PluginListing, Project, ProjectFile, Provider, ProviderInfo, Task, TaskForm, ServerListing, ServerPage, Skill, Switch, UsageLimit, WorkspaceStatus,
} from "./types";

/** Os comandos devolvem o erro como texto pronto para exibir. */
export function errorMessage(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : "Algo deu errado.";
}

export const api = {
  environment: () => invoke<{ cli_found: boolean }>("environment"),

  listAccounts: () => invoke<Account[]>("list_accounts"),
  addAccount: (name: string, provider: Provider) => invoke<void>("add_account", { name, provider }),
  listProviders: () => invoke<ProviderInfo[]>("list_providers"),
  removeAccount: (id: string) => invoke<void>("remove_account", { id }),
  setAccountEnabled: (id: string, enabled: boolean) => invoke<void>("set_account_enabled", { id, enabled }),
  renameAccount: (id: string, name: string) => invoke<void>("rename_account", { id, name }),
  reorderAccounts: (ids: string[]) => invoke<void>("reorder_accounts", { ids }),
  refreshAccount: (id: string) => invoke<void>("refresh_account", { id }),
  loginAccountAgain: (id: string) => invoke<void>("login_account_again", { id }),
  /** Entrega ao login em andamento o código de autenticação mostrado no navegador. */
  submitLoginCode: (scope: CodeScope, code: string) => invoke<void>("submit_login_code", { scope, code }),
  listSwitches: () => invoke<Switch[]>("list_switches"),
  refreshUsage: () => invoke<number>("refresh_usage"),
  accountLimits: (id: string) => invoke<UsageLimit[]>("account_limits", { id }),

  listConversations: () => invoke<Conversation[]>("list_conversations"),
  createConversation: (model: string) => invoke<Conversation>("create_conversation", { model }),
  deleteConversation: (id: string) => invoke<void>("delete_conversation", { id }),
  branchConversation: (id: string, beforeMessage: number) =>
    invoke<Conversation>("branch_conversation", { id, beforeMessage }),
  listMessages: (conversationId: string) => invoke<Message[]>("list_messages", { conversationId }),
  listSubagentSteps: (conversationId: string) =>
    invoke<{ parent: string; step: SubagentStep }[]>("list_subagent_steps", { conversationId }),
  setConversationModel: (id: string, model: string) => invoke<void>("set_conversation_model", { id, model }),
  setConversationEffort: (id: string, effort: string) => invoke<void>("set_conversation_effort", { id, effort }),
  setConversationMode: (id: string, mode: string) => invoke<void>("set_conversation_mode", { id, mode }),
  /** Mensagem gravada: o backend lê a imagem do banco. Só a que ainda não foi gravada manda o conteúdo. */
  copyImage: (source: { message_id: number; index: number } | { data: string }) => invoke<void>("copy_image", { source }),
  sendMessage: (conversationId: string, text: string, attachments: string[], images: PastedImage[]) =>
    invoke<void>("send_message", { conversationId, text, attachments, images }),
  /** Entrega a mensagem ao turno em andamento; `false` se o agente não puder lê-la agora. */
  injectMessage: (conversationId: string, id: string, text: string, attachments: string[], images: PastedImage[]) =>
    invoke<boolean>("inject_message", { conversationId, id, text, attachments, images }),
  cancelMessage: (conversationId: string) => invoke<void>("cancel_message", { conversationId }),
  startFolderSession: (folder: string, model: string) => invoke<Conversation>("start_folder_session", { folder, model }),
  startIsolatedSession: (projectId: string, model: string) =>
    invoke<Conversation>("start_isolated_session", { projectId, model }),
  decidePermission: (decision: {
    conversationId: string;
    requestId: string;
    tool: string;
    allow: boolean;
    always: boolean;
    /** Respostas às perguntas do agente: [pergunta, opção]. */
    answers?: [string, string][];
    /** Modo da sessão após aprovar um plano. */
    nextMode?: string;
  }) => invoke<void>("decide_permission", decision),

  listTasks: () => invoke<Task[]>("list_tasks"),
  saveTask: (task: TaskForm) => invoke<Task>("save_task", { task }),
  setTaskEnabled: (id: string, enabled: boolean) => invoke<void>("set_task_enabled", { id, enabled }),
  deleteTask: (id: string) => invoke<void>("delete_task", { id }),
  runTask: (id: string) => invoke<Conversation>("run_task", { id }),

  getFlags: () => invoke<Flags>("get_flags"),
  setFlag: (key: FlagKey, value: boolean) => invoke<void>("set_flag", { key, value }),
  getProjectOrder: () => invoke<string | null>("get_project_order"),
  setProjectOrder: (order: string) => invoke<void>("set_project_order", { order }),

  historyStatus: () => invoke<HistoryStatus>("history_status"),
  historyList: (query: string) => invoke<HistSession[]>("history_list", { query }),
  historyReindex: () => invoke<number>("history_reindex"),
  historyOpen: (sessionId: string, model: string) => invoke<Conversation>("history_open", { sessionId, model }),

  listSkills: (scope: string) => invoke<Skill[]>("list_skills", { scope }),
  setSkill: (scope: string, skill: string, enabled: boolean) => invoke<void>("set_skill", { scope, skill, enabled }),
  listAgents: () => invoke<Agent[]>("list_agents"),
  saveAgent: (agent: { id: string | null; name: string; instructions: string; model: string | null }) =>
    invoke<Agent>("save_agent", agent),
  deleteAgent: (id: string) => invoke<void>("delete_agent", { id }),
  draftAgent: (name: string) => invoke<AgentDraft>("draft_agent", { name }),
  setConversationAgent: (id: string, agentId: string | null) =>
    invoke<void>("set_conversation_agent", { id, agentId }),

  listProjects: () => invoke<Project[]>("list_projects"),
  saveProject: (project: { id: string | null; name: string; instructions: string }) =>
    invoke<Project>("save_project", project),
  deleteProject: (id: string) => invoke<void>("delete_project", { id }),
  listPipelines: () => invoke<PipelineRun[]>("list_pipelines"),
  pipelineDetail: (id: string) => invoke<PipelineDetail>("pipeline_detail", { id }),
  startPipeline: (input: { projectId: string; jiraKey: string | null; title: string; request: string; budgetMinutes: number | null }) =>
    invoke<PipelineRun>("start_pipeline", input),
  decidePipelinePlan: (id: string, feedback: string | null) => invoke<void>("decide_pipeline_plan", { id, feedback }),
  cancelPipeline: (id: string) => invoke<void>("cancel_pipeline", { id }),
  retryPipeline: (id: string) => invoke<void>("retry_pipeline", { id }),
  commitPipeline: (id: string, message: string) => invoke<void>("commit_pipeline", { id, message }),
  deletePipeline: (id: string) => invoke<void>("delete_pipeline", { id }),
  setProjectVersioned: (id: string, versioned: boolean) => invoke<Project>("set_project_versioned", { id, versioned }),
  listProjectFiles: (projectId: string) => invoke<ProjectFile[]>("list_project_files", { projectId }),
  addProjectFiles: (projectId: string, paths: string[]) => invoke<void>("add_project_files", { projectId, paths }),
  removeProjectFile: (projectId: string, fileId: string) => invoke<void>("remove_project_file", { projectId, fileId }),
  projectMemoryList: (projectId: string) => invoke<MemoryFile[]>("project_memory_list", { projectId }),
  projectMemoryRead: (projectId: string, name: string) => invoke<string>("project_memory_read", { projectId, name }),
  projectMemoryWrite: (projectId: string, name: string, content: string) =>
    invoke<void>("project_memory_write", { projectId, name, content }),
  projectMemoryRemove: (projectId: string, name: string) => invoke<void>("project_memory_remove", { projectId, name }),
  setConversationProject: (id: string, projectId: string | null) =>
    invoke<void>("set_conversation_project", { id, projectId }),

  workspaceEntries: (conversationId: string, dir: string) =>
    invoke<FileEntry[]>("workspace_entries", { conversationId, dir }),
  workspaceRead: (conversationId: string, path: string) => invoke<FileView>("workspace_read", { conversationId, path }),
  workspaceStatus: (conversationId: string) => invoke<WorkspaceStatus>("workspace_status", { conversationId }),
  workspaceDiff: (conversationId: string, path: string) => invoke<string>("workspace_diff", { conversationId, path }),
  workspaceCommit: (conversationId: string, message: string) =>
    invoke<void>("workspace_commit", { conversationId, message }),

  terminalOpen: (conversationId: string, cols: number, rows: number) =>
    invoke<string>("terminal_open", { conversationId, cols, rows }),
  terminalWrite: (id: string, data: string) => invoke<void>("terminal_write", { id, data }),
  terminalResize: (id: string, cols: number, rows: number) => invoke<void>("terminal_resize", { id, cols, rows }),
  terminalClose: (id: string) => invoke<void>("terminal_close", { id }),
  onTerminalOutput: (handler: (p: { id: string; data: string }) => void): Promise<UnlistenFn> =>
    listen<{ id: string; data: string }>("terminal-output", (e) => handler(e.payload)),

  listAccountConnectors: () => invoke<AccountConnector[]>("list_account_connectors"),
  linkConnector: (name: string) => invoke<void>("link_connector", { name }),
  unlinkConnector: (name: string) => invoke<void>("unlink_connector", { name }),
  connectorIcon: (endpoint: string) => invoke<string | null>("connector_icon", { endpoint }),
  setServerEnabled: (name: string, enabled: boolean) => invoke<void>("set_server_enabled", { name, enabled }),
  listConnectors: () => invoke<Connector[]>("list_connectors"),
  addConnector: (name: string, config: string) => invoke<void>("add_connector", { name, config }),
  importDesktopConnectors: (taken: string[]) => invoke<number>("import_desktop_connectors", { taken }),
  setConnectorEnabled: (id: string, enabled: boolean) => invoke<void>("set_connector_enabled", { id, enabled }),
  removeConnector: (id: string) => invoke<void>("remove_connector", { id }),

  storePlugins: (query: string, category: string | null) => invoke<PluginListing[]>("store_plugins", { query, category }),
  storeCategories: () => invoke<string[]>("store_categories"),
  storeInstallPlugin: (id: string) => invoke<void>("store_install_plugin", { id }),
  storeUninstallPlugin: (id: string) => invoke<void>("store_uninstall_plugin", { id }),
  storeRefreshCatalog: () => invoke<void>("store_refresh_catalog"),
  storeServers: (query: string, cursor: string | null) => invoke<ServerPage>("store_servers", { query, cursor }),
  storeAddServer: (listing: ServerListing, values: Record<string, string>) =>
    invoke<void>("store_add_server", { listing, values }),

  quickEntrySent: (conversationId: string) => invoke<void>("quick_entry_sent", { conversationId }),
  hideQuickEntry: () => invoke<void>("hide_quick_entry"),
  saveTextFile: (path: string, content: string) => invoke<void>("save_text_file", { path, content }),
  onOpenConversation: (handler: (conversationId: string) => void): Promise<UnlistenFn> =>
    listen<string>("open-conversation", (e) => handler(e.payload)),

  onChat: (handler: (e: ChatEvent) => void): Promise<UnlistenFn> =>
    listen<ChatEvent>("chat", (e) => handler(e.payload)),
  onAccountsChanged: (handler: () => void): Promise<UnlistenFn> => listen("accounts-changed", handler),
  onPipelineChanged: (handler: (p: { id: string; status: PipelineStatus }) => void): Promise<UnlistenFn> =>
    listen<{ id: string; status: PipelineStatus }>("pipeline-changed", (e) => handler(e.payload)),
  onHistoryProgress: (handler: (p: { done: number; total: number }) => void): Promise<UnlistenFn> =>
    listen<{ done: number; total: number }>("history-progress", (e) => handler(e.payload)),
};
