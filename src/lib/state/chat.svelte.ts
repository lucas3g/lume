// Estado das conversas: lista, mensagens da conversa aberta e respostas em andamento.

import { api, errorMessage } from "../api";
import { DEFAULT_MODEL, type ChatEvent, type Conversation, type Message, type PastedImage } from "../types";
import { loadProjects } from "./projects.svelte";

/** Resposta em andamento de uma conversa. */
export interface Turn {
  text: string;
  status: string | null;
  account: string | null;
  context: string[];
}

/** Como terminou o último turno, quando não foi com uma resposta. */
export type Outcome =
  | { kind: "error"; message: string }
  | { kind: "exhausted"; resetsAt: number | null }
  | null;

/** Pedido de autorização do agente aguardando resposta. */
export interface PermissionRequest {
  requestId: string;
  tool: string;
  input: string;
}

/** O que acompanha a resposta a um pedido, além de permitir ou negar. */
export interface DecisionExtras {
  answers?: [string, string][];
  nextMode?: string;
}

export const chat = $state({
  conversations: [] as Conversation[],
  activeId: null as string | null,
  messages: [] as Message[],
  turns: {} as Record<string, Turn>,
  outcomes: {} as Record<string, Outcome>,
  permissions: {} as Record<string, PermissionRequest>,
  /** Mensagem do usuário sendo editada: o envio ramifica a conversa antes dela. */
  editing: null as { messageId: number; original: string } | null,
});

let localId = -1;

function localMessage(role: Message["role"], content: string): Message {
  return { id: localId--, role, content, account_id: null, meta: null, created_at: Math.floor(Date.now() / 1000) };
}

export function activeConversation(): Conversation | undefined {
  return chat.conversations.find((c) => c.id === chat.activeId);
}

export async function loadConversations(): Promise<void> {
  chat.conversations = await api.listConversations();
}

export async function openConversation(id: string | null): Promise<void> {
  chat.activeId = id;
  chat.editing = null;
  chat.messages = id ? await api.listMessages(id) : [];
}

export async function removeConversation(id: string): Promise<void> {
  await api.deleteConversation(id);
  delete chat.turns[id];
  delete chat.outcomes[id];
  delete chat.permissions[id];
  if (chat.activeId === id) await openConversation(null);
  await loadConversations();
}

export async function setAgent(agentId: string | null): Promise<void> {
  const conversation = activeConversation();
  if (!conversation) return;
  conversation.agent_id = agentId;
  await api.setConversationAgent(conversation.id, agentId);
}

export async function setEffort(effort: string): Promise<void> {
  const conversation = activeConversation();
  if (!conversation) return;
  conversation.effort = effort;
  await api.setConversationEffort(conversation.id, effort);
}

export async function setMode(mode: string): Promise<void> {
  const conversation = activeConversation();
  if (!conversation) return;
  conversation.permission_mode = mode;
  await api.setConversationMode(conversation.id, mode);
}

export async function setModel(model: string): Promise<void> {
  const conversation = activeConversation();
  if (!conversation) return;
  conversation.model = model;
  await api.setConversationModel(conversation.id, model);
}

/** Envia a mensagem, criando a conversa se ainda não houver uma aberta. */
/** O que acompanha uma mensagem além do texto. */
export interface SendOptions {
  model?: string;
  /** Agente e projeto de uma conversa nova. */
  agentId?: string | null;
  projectId?: string | null;
  /** Esforço escolhido antes de a conversa existir. */
  effort?: string;
  /** Caminhos dos arquivos anexados. */
  attachments?: string[];
  /** Imagens coladas no campo de mensagem. */
  images?: PastedImage[];
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** Nova conversa com as mensagens anteriores a `beforeMessage`, já aberta. */
export async function branchAt(beforeMessage: number): Promise<string | null> {
  const id = chat.activeId;
  if (!id) return null;
  const branch = await api.branchConversation(id, beforeMessage);
  chat.conversations.unshift(branch);
  chat.activeId = branch.id;
  chat.messages = await api.listMessages(branch.id);
  return branch.id;
}

/** Repete a pergunta que gerou a resposta, em um ramo novo da conversa. */
export async function regenerate(assistantMessage: Message): Promise<void> {
  const asked = [...chat.messages].reverse().find((m) => m.role === "user" && m.id < assistantMessage.id);
  if (!asked || !(await branchAt(asked.id))) return;
  await send(asked.content, resendOptions(asked));
}

/** Anexos guardados com a mensagem, para reenviá-la igual. */
function resendOptions(message: Message): SendOptions {
  if (!message.meta) return {};
  try {
    const meta = JSON.parse(message.meta) as { paths?: string[]; images?: PastedImage[] };
    return { attachments: meta.paths ?? [], images: meta.images ?? [] };
  } catch {
    return {};
  }
}

export function startEdit(message: Message): void {
  chat.editing = { messageId: message.id, original: message.content };
}

export function cancelEdit(): void {
  chat.editing = null;
}

export async function send(text: string, options: SendOptions = {}): Promise<void> {
  const { model = DEFAULT_MODEL, agentId = null, projectId = null, effort, attachments = [], images = [] } = options;
  if (chat.editing) {
    const { messageId } = chat.editing;
    chat.editing = null;
    await branchAt(messageId);
  }
  let id = chat.activeId;
  if (!id) {
    const created = await api.createConversation(model);
    if (agentId) {
      await api.setConversationAgent(created.id, agentId);
      created.agent_id = agentId;
    }
    if (projectId) {
      await api.setConversationProject(created.id, projectId);
      created.project_id = projectId;
    }
    if (effort && effort !== created.effort) {
      await api.setConversationEffort(created.id, effort);
      created.effort = effort;
    }
    chat.conversations.unshift(created);
    chat.activeId = id = created.id;
    chat.messages = [];
  }
  chat.outcomes[id] = null;
  chat.turns[id] = { text: "", status: null, account: null, context: [] };
  const sent = localMessage("user", text || "Veja as imagens anexadas.");
  const names = [...attachments.map(fileName), ...images.map((i) => i.name)];
  if (names.length) sent.meta = JSON.stringify({ attachments: names });
  chat.messages.push(sent);
  try {
    await api.sendMessage(id, text, attachments, images);
  } catch (e) {
    delete chat.turns[id];
    chat.outcomes[id] = { kind: "error", message: errorMessage(e) };
  }
}

/** Abre uma sessão de agente na pasta; a pasta vira (ou reutiliza) um projeto. */
export async function startAgentSession(folder: string, model = DEFAULT_MODEL): Promise<void> {
  const created = await api.startFolderSession(folder, model);
  // Sessões antigas da mesma pasta podem ter sido adotadas pelo projeto.
  await Promise.all([loadConversations(), loadProjects()]);
  chat.activeId = created.id;
  chat.messages = [];
}

/** Sessão em uma cópia isolada (worktree) do projeto. */
export async function startIsolatedSession(projectId: string, model = DEFAULT_MODEL): Promise<void> {
  const created = await api.startIsolatedSession(projectId, model);
  await loadConversations();
  chat.activeId = created.id;
  chat.messages = [];
}

/** Responde ao pedido de autorização da conversa aberta. */
export async function decide(allow: boolean, always: boolean, extras: DecisionExtras = {}): Promise<void> {
  const id = chat.activeId;
  const request = id ? chat.permissions[id] : undefined;
  if (!id || !request) return;
  delete chat.permissions[id];
  const conversation = activeConversation();
  if (allow && extras.nextMode && conversation) conversation.permission_mode = extras.nextMode;
  await api.decidePermission({ conversationId: id, requestId: request.requestId, tool: request.tool, allow, always, ...extras });
}

export function cancel(): void {
  if (chat.activeId) void api.cancelMessage(chat.activeId);
}

async function finish(id: string, outcome: Outcome = null): Promise<void> {
  delete chat.turns[id];
  delete chat.permissions[id];
  chat.outcomes[id] = outcome;
  if (chat.activeId === id) chat.messages = await api.listMessages(id);
  await loadConversations();
}

const TOOL_LABELS: Record<string, string> = {
  WebSearch: "Buscando na web",
  WebFetch: "Lendo uma página",
  Skill: "Carregando uma skill",
};

/** Eventos que indicam um turno em andamento, mesmo que iniciado por outra janela. */
const TURN_ACTIVITY = new Set<ChatEvent["kind"]>(["account", "context", "delta", "thinking", "subagent", "tool", "step", "permission"]);

const SUBAGENT_PREFIX = "Subagente: ";
const SUBAGENT_MAX = 90;

/** Última linha do que o subagente escreveu, encurtada, como situação do turno. */
function subagentStatus(current: string | null, delta: string): string {
  const previous = current?.startsWith(SUBAGENT_PREFIX) ? current.slice(SUBAGENT_PREFIX.length) : "";
  const text = (previous + delta).split("\n").filter((l) => l.trim()).pop() ?? "";
  return SUBAGENT_PREFIX + (text.length > SUBAGENT_MAX ? "…" + text.slice(-SUBAGENT_MAX) : text);
}

/** Aplica ao estado um evento vindo do núcleo. */
export function handleChatEvent(event: ChatEvent): void {
  const id = event.conversation_id;
  if (!chat.turns[id] && TURN_ACTIVITY.has(event.kind)) {
    chat.turns[id] = { text: "", status: null, account: null, context: [] };
  }
  const turn = chat.turns[id];
  switch (event.kind) {
    case "title": {
      const conversation = chat.conversations.find((c) => c.id === id);
      if (conversation) conversation.title = event.title;
      break;
    }
    case "account":
      if (turn) turn.account = event.name;
      break;
    case "context":
      if (turn) turn.context = event.titles;
      break;
    case "delta":
      if (turn) {
        turn.text += event.text;
        turn.status = null;
      }
      break;
    case "thinking":
      if (turn && !turn.text) turn.status = "Pensando";
      break;
    case "subagent":
      if (turn) turn.status = subagentStatus(turn.status, event.text);
      break;
    case "tool":
      if (turn) turn.status = TOOL_LABELS[event.name] ?? (event.name.startsWith("mcp__") ? "Usando um conector" : `Usando ${event.name}`);
      break;
    case "step":
      // Um trecho de texto ou uso de ferramenta foi gravado: passa a vir da lista de mensagens.
      if (turn) turn.text = "";
      if (chat.activeId === id) void api.listMessages(id).then((messages) => (chat.messages = messages));
      break;
    case "permission":
      chat.permissions[id] = { requestId: event.request_id, tool: event.tool, input: event.input };
      if (turn) turn.status = event.tool === "AskUserQuestion" ? "Aguardando sua resposta" : "Aguardando sua aprovação";
      break;
    case "reset":
      if (turn) {
        turn.text = "";
        turn.status = null;
      }
      break;
    case "notice":
      if (chat.activeId === id) chat.messages.push(localMessage("notice", event.text));
      break;
    case "done":
    case "cancelled":
      void finish(id);
      break;
    case "exhausted":
      void finish(id, { kind: "exhausted", resetsAt: event.resets_at });
      break;
    case "error":
      void finish(id, { kind: "error", message: event.message });
      break;
  }
}
