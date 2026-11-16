// Tipos espelhados do núcleo Rust (camadas domain e application).

/** Serviço de IA de uma conta; cada um é atendido pelo CLI dele instalado no computador. */
export type Provider = "claude" | "chatgpt" | "gemini";

/** Um tipo de conta e se o CLI dele está instalado neste computador. */
export interface ProviderInfo {
  id: Provider;
  /** Nome do serviço, ex.: "ChatGPT". */
  label: string;
  /** Nome do CLI que atende, ex.: "Codex". */
  cli: string;
  installed: boolean;
}

export const PROVIDER_LABEL: Record<Provider, string> = { claude: "Claude", chatgpt: "ChatGPT", gemini: "Gemini" };
export const PROVIDER_CLI: Record<Provider, string> = { claude: "Claude Code", chatgpt: "Codex", gemini: "Antigravity" };

export interface Account {
  id: string;
  name: string;
  provider: Provider;
  email: string | null;
  plan: string | null;
  config_dir: string | null;
  priority: number;
  enabled: boolean;
  auth_ok: boolean;
  limited_until: number | null;
  util_5h: number | null;
  util_7d: number | null;
  resets_5h: number | null;
  resets_7d: number | null;
  /** Quando o login da conta vence. */
  login_expires_at: number | null;
}

/** Um limite de uso da conta: sessão, semana ou semana de um modelo. */
export interface UsageLimit {
  kind: "session" | "weekly_all" | "weekly_scoped" | (string & {});
  /** Modelo a que o limite se restringe; `null` vale para todos. */
  model: string | null;
  /** Fração usada, de 0 a 1. */
  used: number;
  resets_at: number | null;
}

export interface Conversation {
  id: string;
  title: string;
  cli_session_id: string | null;
  model: string;
  cwd: string;
  agent_id: string | null;
  project_id: string | null;
  kind: "chat" | "agent";
  allowed_tools: string[];
  /** Percentual do contexto do modelo ocupado no último turno. */
  context_percent: number | null;
  effort: string;
  permission_mode: string;
  updated_at: number;
}

/** Imagem colada no campo de mensagem, já em base64. */
export interface PastedImage {
  name: string;
  media_type: string;
  data: string;
}

export type Role = "user" | "assistant" | "notice" | "tool";

export interface Message {
  id: number;
  role: Role;
  content: string;
  account_id: string | null;
  meta: string | null;
  created_at: number;
}

export interface Switch {
  at: number;
  from_name: string;
  to_name: string;
  conversation_title: string;
}

export interface HistSession {
  id: string;
  source: "code" | "desktop";
  cwd: string | null;
  title: string | null;
  mtime: number;
  msg_count: number;
}

export interface HistoryStatus {
  sessions: number;
  projects: number;
  desktop_sessions: number;
}

export type FlagKey = "auto_switch" | "notify_switch" | "use_history" | "switch_early" | "notify_desktop";
export type Flags = Record<FlagKey, boolean>;

/** Passo de um subagente, como o núcleo envia e grava. */
export type SubagentStep =
  | { kind: "started"; description: string; agent_type: string; prompt: string; background: boolean }
  | { kind: "progress"; description: string }
  | { kind: "text"; text: string }
  | { kind: "tool_call"; id: string; name: string; input: string }
  | { kind: "tool_result"; id: string; output: string; is_error: boolean }
  | { kind: "finished"; status: string; summary: string };

export type ChatEvent = { conversation_id: string } & (
  | { kind: "title"; title: string }
  | { kind: "account"; id: string; name: string }
  | { kind: "context"; titles: string[] }
  | { kind: "delta"; text: string }
  | { kind: "thinking"; text: string }
  | { kind: "subagent"; text: string }
  | { kind: "agent_step"; parent: string; step: SubagentStep }
  | { kind: "waiting"; text: string }
  | { kind: "tool"; name: string }
  | { kind: "step" }
  | { kind: "injected"; id: string }
  | { kind: "permission"; request_id: string; tool: string; input: string; description: string }
  | { kind: "reset" }
  | { kind: "notice"; text: string }
  | { kind: "done"; text: string }
  | { kind: "cancelled" }
  | { kind: "exhausted"; resets_at: number | null }
  | { kind: "error"; message: string }
);

/** Modelos atuais, pelos apelidos que o Claude Code resolve para a versão mais recente. */
export const MODELS = [
  { id: "opus", label: "Opus 5.5" },
  { id: "sonnet", label: "Sonnet 5.5" },
  { id: "fable", label: "Fable 5.2" },
  { id: "haiku", label: "Haiku 5.5" },
] as const;

export const MORE_MODELS = [
  { id: "claude-fable-5-1", label: "Fable 5.1" },
  { id: "claude-haiku-4-5", label: "Haiku 4.5" },
  { id: "claude-sonnet-5", label: "Sonnet 5" },
  { id: "claude-opus-5", label: "Opus 5" },
  { id: "claude-fable-5", label: "Fable 5" },
  { id: "claude-opus-4-8", label: "Opus 4.8" },
  { id: "claude-opus-4-7", label: "Opus 4.7" },
  { id: "claude-opus-4-6", label: "Opus 4.6" },
  { id: "claude-sonnet-4-6", label: "Sonnet 4.6" },
] as const;

export const DEFAULT_MODEL = "opus";

export function modelLabel(id: string): string {
  return [...MODELS, ...MORE_MODELS].find((m) => m.id === id)?.label ?? id;
}

/** Níveis de esforço de raciocínio, do menor para o maior. */
export const EFFORTS: { id: string; label: string; note?: string; warn?: boolean }[] = [
  { id: "low", label: "Baixo" },
  { id: "medium", label: "Médio", note: "Recomendado" },
  { id: "high", label: "Alto" },
  { id: "xhigh", label: "Extra" },
  { id: "max", label: "Máx", note: "consome muito mais uso", warn: true },
];

export const DEFAULT_EFFORT = "medium";

/** Modos de permissão de uma sessão de agente. */
export const MODES: { id: string; label: string; description: string; isDefault?: boolean; risky?: boolean }[] = [
  { id: "auto", label: "Automático", description: "O Claude decide quando pedir permissão", isDefault: true },
  { id: "manual", label: "Manual", description: "Sempre perguntar antes de fazer alterações" },
  { id: "acceptEdits", label: "Aceitar edições", description: "Aceitar todas as edições de arquivo automaticamente" },
  { id: "plan", label: "Plano", description: "Criar um plano antes de fazer alterações" },
  { id: "bypassPermissions", label: "Ignorar permissões", description: "Aceita todas as ações sem perguntar", risky: true },
];

/** Telas principais do aplicativo. */
export type View = "chat" | "project" | "skills" | "connectors" | "history" | "accounts" | "tasks" | "pipelines";

export type Repeat =
  | { kind: "every_minutes"; minutes: number }
  | { kind: "daily"; hour: number; minute: number }
  | { kind: "weekly"; weekday: number; hour: number; minute: number };

/** Tarefa agendada: um prompt que roda sozinho na cadência escolhida. */
export interface Task {
  id: string;
  name: string;
  prompt: string;
  project_id: string | null;
  model: string;
  repeat: Repeat;
  tz_offset_minutes: number;
  enabled: boolean;
  last_run_at: number | null;
  next_run_at: number;
}

export type TaskForm = Omit<Task, "id" | "enabled" | "last_run_at" | "next_run_at"> & { id: string | null };

const WEEKDAYS = ["domingo", "segunda", "terça", "quarta", "quinta", "sexta", "sábado"];
const two = (n: number) => String(n).padStart(2, "0");

export function describeRepeat(r: Repeat): string {
  if (r.kind === "every_minutes") {
    if (r.minutes % 60 === 0) return `a cada ${r.minutes / 60} ${r.minutes === 60 ? "hora" : "horas"}`;
    return `a cada ${r.minutes} min`;
  }
  if (r.kind === "daily") return `todo dia às ${two(r.hour)}:${two(r.minute)}`;
  return `toda ${WEEKDAYS[r.weekday]} às ${two(r.hour)}:${two(r.minute)}`;
}

export interface Project {
  id: string;
  name: string;
  instructions: string;
  /** Pasta do projeto; as sessões de agente do projeto trabalham nela. */
  folder: string | null;
  /** Tem versionamento (git): a esteira de bots trabalha numa cópia isolada; sem ele, direto na pasta. */
  versioned: boolean;
}

export interface FileEntry {
  name: string;
  /** Caminho relativo à pasta da sessão. */
  path: string;
  is_dir: boolean;
}

export interface FileView {
  path: string;
  size: number;
  /** `null` quando o arquivo não é texto ou é grande demais para exibir. */
  text: string | null;
}

export interface ChangedFile {
  path: string;
  status: string;
  added: number;
  removed: number;
}

export interface WorkspaceStatus {
  is_repo: boolean;
  branch: string | null;
  added: number;
  removed: number;
  files: ChangedFile[];
}

/** Arquivo da memória que o Claude Code mantém na pasta do projeto. */
export interface MemoryFile {
  name: string;
  size: number;
  modified: number;
}

export interface ProjectFile {
  id: string;
  name: string;
  size: number;
}

export interface Skill {
  name: string;
  description: string | null;
  origin: "personal" | "plugin";
  enabled: boolean;
  /** No escopo de um agente: já disponível por estar ligada para todos. */
  inherited: boolean;
}

/** Sugestão do modelo para um agente novo. */
export interface AgentDraft {
  instructions: string;
  skills: string[];
}

export interface Agent {
  id: string;
  name: string;
  instructions: string;
  model: string | null;
}

export interface Connector {
  id: string;
  name: string;
  config: string;
  enabled: boolean;
  status: string | null;
}

/** Conector que o Claude Code carrega para a conta ativa. */
export interface AccountConnector {
  name: string;
  display_name: string;
  key: string;
  endpoint: string;
  status: "connected" | "needs_auth" | "failed" | "pending";
  source: "account" | "user" | "plugin";
  enabled: boolean;
}

/** Plugin de um marketplace do Claude Code (loja de skills). */
export interface PluginListing {
  id: string;
  name: string;
  display_name: string;
  description: string;
  author: string | null;
  category: string | null;
  marketplace: string;
  homepage: string | null;
  skill_count: number | null;
  installed: boolean;
  enabled: boolean;
}

export type InputPlace = { place: "header" } | { place: "env" } | { place: "positional" } | { place: "named"; flag: string };

export type ServerInput = { name: string; description: string; secret: boolean; required: boolean } & InputPlace;

export type ServerInstall =
  | { kind: "remote"; url: string; sse: boolean }
  | { kind: "npm"; package: string; version: string | null; args: string[] }
  | { kind: "pypi"; package: string; version: string | null; args: string[] }
  | { kind: "unsupported"; reason: string };

/** Servidor MCP do diretório público (loja de conectores). */
export interface ServerListing {
  name: string;
  title: string;
  description: string;
  website: string | null;
  install: ServerInstall;
  inputs: ServerInput[];
}

export interface ServerPage {
  servers: ServerListing[];
  next_cursor: string | null;
}

export const CATEGORY_LABEL: Record<string, string> = {
  development: "Desenvolvimento",
  productivity: "Produtividade",
  database: "Banco de dados",
  security: "Segurança",
  monitoring: "Monitoramento",
  deployment: "Implantação",
  design: "Design",
  testing: "Testes",
  automation: "Automação",
  learning: "Aprendizado",
  location: "Localização",
  migration: "Migração",
  math: "Matemática",
};

/** Escopo das skills que valem para qualquer conversa e agente. */
export const SCOPE_ALL = "all";

// ---- esteira de bots

export type PipelineRole = "triage" | "planner" | "approval" | "implementer" | "reviewer" | "tester" | "reporter";
export type PipelineStatus = "running" | "awaiting_approval" | "needs_info" | "done" | "failed" | "cancelled";
export type StageStatus = "running" | "done" | "returned" | "failed" | "cancelled";

export interface PipelineRun {
  id: string;
  project_id: string;
  jira_key: string | null;
  title: string;
  request: string;
  status: PipelineStatus;
  /** worktree: cópia isolada num ramo novo; direct: direto na pasta do projeto. */
  mode: "worktree" | "direct";
  workdir: string;
  branch: string | null;
  rounds: number;
  /** Etapas desligadas no projeto quando a execução começou. */
  skipped: PipelineRole[];
  /** Agente de cada papel quando a execução começou; `model` nulo usa o modelo padrão do papel. */
  agents: Partial<Record<PipelineRole, { id: string; name: string; model: string | null }>>;
  /** Imagens coladas no pedido; o conteúdo vem à parte (`pipelineImages`). */
  image_count: number;
  budget_minutes: number;
  note: string | null;
  created_at: number;
  updated_at: number;
}

export interface StageRun {
  id: string;
  run_id: string;
  role: PipelineRole;
  round: number;
  status: StageStatus;
  /** Conversa da etapa: o andamento chega pelos eventos de conversa dela. */
  conversation_id: string;
  input: string;
  output: string | null;
  started_at: number;
  finished_at: number | null;
}

export interface PipelineDetail extends PipelineRun {
  stages: StageRun[];
  spent_seconds: number;
  commit_message: string | null;
  /** Voltas máximas do Implementador antes de a esteira parar. */
  max_rounds: number;
  /** Modelo de cada etapa (por id) em cada serviço: o do Claude e o dos outros (ausente = padrão do CLI). */
  stage_models: Record<string, { claude: string; others: Partial<Record<Provider, string>> }>;
}

/** Nível de modelo de um papel da esteira. */
export type Tier = "fast" | "balanced" | "strong";

/** Modelo de cada nível em cada serviço; ausente = padrão (Claude: haiku/sonnet/opus; outros: o do CLI). */
export interface TierModels {
  claude: Partial<Record<Tier, string>>;
  chatgpt: Partial<Record<Tier, string>>;
  gemini: Partial<Record<Tier, string>>;
}

/** Como a esteira roda num projeto: agente (id) de cada papel e etapas desligadas. */
export interface PipelineConfig {
  agents: Partial<Record<PipelineRole, string>>;
  skipped: PipelineRole[];
}
