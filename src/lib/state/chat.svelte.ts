// Estado das conversas: lista, mensagens da conversa aberta e respostas em andamento.

import { api, errorMessage } from "../api";
import { DEFAULT_MODEL, type ChatEvent, type Conversation, type Message, type PastedImage } from "../types";
import { loadProjects } from "./projects.svelte";
import { parseInput, parseToolUse, type RunningTool } from "../tools";
import { applySubagentStep, forgetSubagents, loadSubagents, settleSubagents } from "./subagents.svelte";

/** Resposta em andamento de uma conversa. */
export interface Turn {
  text: string;
  status: string | null;
  account: string | null;
  context: string[];
  /** A resposta terminou; só subagentes em segundo plano seguem trabalhando, e dá para conversar. */
  waiting: boolean;
  /** Ferramentas rodando agora, antes de virarem mensagens gravadas. */
  running: RunningTool[];
  /** Quando começou a sequência de ferramentas em curso (ms); texto do agente encerra a sequência. */
  toolsSince: number | null;
}

/** Como terminou o último turno, quando não foi com uma resposta. */
export type Outcome =
  | { kind: "error"; message: string }
  | { kind: "exhausted"; resetsAt: number | null }
  | null;

/** Mensagem mandada enquanto o agente trabalha: espera o fim do turno, ou vai já com "Enviar agora". */
export interface QueuedMessage {
  id: string;
  text: string;
  attachments: string[];
  images: (PastedImage & { preview?: string })[];
  /** Entregue ao agente, esperando ele ler. */
  sending: boolean;
  /** O agente desta conta não lê mensagens no meio do turno. */
  waitsForEnd: boolean;
}

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
  /** Mensagens na fila de cada conversa, na ordem em que foram escritas. */
  queued: {} as Record<string, QueuedMessage[]>,
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
  if (!id) {
    chat.messages = [];
    return;
  }
  const [messages] = await Promise.all([api.listMessages(id), loadSubagents(id)]);
  chat.messages = messages;
  // Sem turno em andamento, nenhum subagente gravado pode estar trabalhando.
  if (!chat.turns[id]) settleSubagents(id);
}

export async function removeConversation(id: string): Promise<void> {
  await api.deleteConversation(id);
  delete chat.turns[id];
  delete chat.outcomes[id];
  delete chat.permissions[id];
  delete chat.queued[id];
  forgetSubagents(id);
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
  await deliver(id, text, attachments, images);
}

/** Inicia o turno de uma conversa que já existe (aberta ou não). */
async function deliver(id: string, text: string, attachments: string[], images: PastedImage[]): Promise<void> {
  chat.outcomes[id] = null;
  chat.turns[id] = { text: "", status: null, account: null, context: [], waiting: false, running: [], toolsSince: null };
  if (chat.activeId === id) {
    const sent = localMessage("user", text || "Veja as imagens anexadas.");
    const names = [...attachments.map(fileName), ...images.map((i) => i.name)];
    if (names.length) sent.meta = JSON.stringify({ attachments: names, images });
    chat.messages.push(sent);
  }
  try {
    await api.sendMessage(id, text, attachments, images);
  } catch (e) {
    delete chat.turns[id];
    chat.outcomes[id] = { kind: "error", message: errorMessage(e) };
  }
}

const imagesOnly = (images: QueuedMessage["images"]): PastedImage[] =>
  images.map(({ name, media_type, data }) => ({ name, media_type, data }));

/** Põe a mensagem na fila da conversa enquanto o agente trabalha. */
export function enqueue(id: string, text: string, attachments: string[], images: QueuedMessage["images"]): string {
  const item: QueuedMessage = { id: crypto.randomUUID(), text, attachments, images, sending: false, waitsForEnd: false };
  chat.queued[id] = [...(chat.queued[id] ?? []), item];
  return item.id;
}

export function unqueue(id: string, messageId: string): void {
  chat.queued[id] = (chat.queued[id] ?? []).filter((m) => m.id !== messageId || m.sending);
}

/** Entrega já a mensagem da fila: o agente a lê no próximo passo, sem parar o que está fazendo. */
export async function sendNow(id: string, messageId: string): Promise<void> {
  const item = chat.queued[id]?.find((m) => m.id === messageId);
  if (!item || item.sending) return;
  if (!chat.turns[id]) {
    unqueue(id, messageId);
    await deliver(id, item.text, item.attachments, imagesOnly(item.images));
    return;
  }
  item.sending = true;
  let accepted = false;
  try {
    accepted = await api.injectMessage(id, item.id, item.text, item.attachments, imagesOnly(item.images));
  } catch {
    accepted = false;
  }
  if (!accepted) {
    item.sending = false;
    item.waitsForEnd = true;
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
  const turn = chat.turns[id];
  const status = turn?.status ?? null;
  if (turn) turn.status = null;
  const conversation = activeConversation();
  const mode = conversation?.permission_mode;
  if (allow && extras.nextMode && conversation) conversation.permission_mode = extras.nextMode;
  try {
    await api.decidePermission({ conversationId: id, requestId: request.requestId, tool: request.tool, allow, always, ...extras });
  } catch (e) {
    // A resposta não chegou: o agente continua esperando, então o pedido volta para a tela.
    if (chat.turns[id] && !chat.permissions[id]) {
      chat.permissions[id] = request;
      chat.turns[id].status = status;
      if (conversation && mode !== undefined) conversation.permission_mode = mode;
    }
    throw e;
  }
}

/** Manda para segundo plano um comando em execução: ele segue rodando e o agente continua. */
export function backgroundTool(toolId: string): void {
  const id = chat.activeId;
  const tool = id ? chat.turns[id]?.running.find((r) => r.id === toolId) : undefined;
  if (!id || !tool || tool.backgrounded) return;
  // Já esconde o botão; volta se o pedido não chegou à CLI (ex.: o comando acabou antes).
  tool.backgrounded = true;
  const undo = () => tool && (tool.backgrounded = false);
  void api.backgroundTool(id, toolId).then((sent) => sent || undo(), undo);
}

export function cancel(): void {
  if (chat.activeId) void api.cancelMessage(chat.activeId);
}

/** Encerra o turno. Depois de uma resposta completa, a próxima mensagem da fila é enviada. */
async function finish(id: string, outcome: Outcome = null, continueQueue = true): Promise<void> {
  delete chat.turns[id];
  delete chat.permissions[id];
  chat.outcomes[id] = outcome;
  settleSubagents(id);
  // Entregues e não lidas (o turno acabou antes): voltam a esperar a vez.
  for (const item of chat.queued[id] ?? []) item.sending = false;
  if (chat.activeId === id) chat.messages = await api.listMessages(id);
  await loadConversations();
  const next = continueQueue && !outcome && !chat.turns[id] ? chat.queued[id]?.[0] : undefined;
  if (next) {
    chat.queued[id] = chat.queued[id].slice(1);
    await deliver(id, next.text, next.attachments, imagesOnly(next.images));
  }
}

const TOOL_LABELS: Record<string, string> = {
  WebSearch: "Buscando na web",
  WebFetch: "Lendo uma página",
  Skill: "Carregando uma skill",
};

/** Eventos que indicam um turno em andamento, mesmo que iniciado por outra janela. */
const TURN_ACTIVITY = new Set<ChatEvent["kind"]>(["account", "context", "delta", "thinking", "subagent", "agent_step", "tool", "tool_start", "tool_task", "step", "permission", "waiting"]);

/** Eventos do agente principal: um deles depois da espera significa que ele voltou a trabalhar. */
const MAIN_ACTIVITY = new Set<ChatEvent["kind"]>(["delta", "thinking", "tool", "tool_start", "permission", "injected"]);

const SUBAGENT_PREFIX = "Subagente: ";
const SUBAGENT_MAX = 90;

/** Última linha do que o subagente escreveu, encurtada, como situação do turno. */
function subagentStatus(current: string | null, delta: string): string {
  const previous = current?.startsWith(SUBAGENT_PREFIX) ? current.slice(SUBAGENT_PREFIX.length) : "";
  const text = (previous + delta).split("\n").filter((l) => l.trim()).pop() ?? "";
  return SUBAGENT_PREFIX + (text.length > SUBAGENT_MAX ? "…" + text.slice(-SUBAGENT_MAX) : text);
}

/** O texto em andamento foi gravado: some do turno e passa a vir da lista de mensagens. */
function takeRecorded(id: string): void {
  const turn = chat.turns[id];
  if (turn) turn.text = "";
  // Fora da conversa aberta, só busca para tirar das que rodam as ferramentas já gravadas.
  if (chat.activeId !== id && !turn?.running.length) return;
  void api.listMessages(id).then((messages) => {
    // A resposta de uma conversa que o usuário já deixou não substitui a aberta.
    if (chat.activeId === id) chat.messages = messages;
    const live = chat.turns[id];
    if (live?.running.length) {
      const recorded = new Set(messages.filter((m) => m.role === "tool").map((m) => parseToolUse(m.content)?.id));
      live.running = live.running.filter((r) => !recorded.has(r.id));
    }
  });
}

/** Aplica ao estado um evento vindo do núcleo. */
export function handleChatEvent(event: ChatEvent): void {
  const id = event.conversation_id;
  if (!chat.turns[id] && TURN_ACTIVITY.has(event.kind)) {
    chat.turns[id] = { text: "", status: null, account: null, context: [], waiting: false, running: [], toolsSince: null };
  }
  const turn = chat.turns[id];
  if (turn?.waiting && MAIN_ACTIVITY.has(event.kind)) turn.waiting = false;
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
        if (!turn.running.length) turn.toolsSince = null;
      }
      break;
    case "thinking":
      if (turn && !turn.text) turn.status = "Pensando";
      break;
    case "subagent":
      if (turn) turn.status = subagentStatus(turn.status, event.text);
      break;
    case "waiting":
      // A resposta foi gravada; só os agentes em segundo plano seguem trabalhando.
      if (turn) {
        turn.waiting = true;
        turn.status = null;
      }
      takeRecorded(id);
      break;
    case "agent_step":
      applySubagentStep(id, event.parent, event.step);
      break;
    case "tool":
      if (turn) turn.status = TOOL_LABELS[event.name] ?? (event.name.startsWith("mcp__") ? "Usando um conector" : `Usando ${event.name}`);
      break;
    case "tool_start":
      if (turn) {
        const now = Date.now();
        turn.toolsSince ??= now;
        turn.running.push({ id: event.id, name: event.name, input: parseInput(event.input), startedAt: now, task: false, backgrounded: false });
      }
      break;
    case "tool_task": {
      const tool = turn?.running.find((r) => r.id === event.id);
      if (tool) tool.task = true;
      break;
    }
    case "step":
      // Um trecho de texto ou uso de ferramenta foi gravado: passa a vir da lista de mensagens.
      takeRecorded(id);
      break;
    case "injected":
      // O agente leu a mensagem: ela sai da fila e passa a vir da lista de mensagens.
      chat.queued[id] = (chat.queued[id] ?? []).filter((m) => m.id !== event.id);
      takeRecorded(id);
      break;
    case "permission":
      chat.permissions[id] = { requestId: event.request_id, tool: event.tool, input: event.input };
      if (turn) turn.status = event.tool === "AskUserQuestion" ? "Aguardando sua resposta" : "Aguardando sua aprovação";
      break;
    case "reset":
      // A tentativa foi descartada: as ferramentas dela não vão devolver resultado.
      if (turn) {
        turn.text = "";
        turn.status = null;
        turn.running = [];
        turn.toolsSince = null;
      }
      break;
    case "notice":
      if (chat.activeId === id) chat.messages.push(localMessage("notice", event.text));
      break;
    case "done":
      void finish(id);
      break;
    case "cancelled":
      // Parar é decisão do usuário: a fila fica esperando.
      void finish(id, null, false);
      break;
    case "exhausted":
      void finish(id, { kind: "exhausted", resetsAt: event.resets_at });
      break;
    case "error":
      void finish(id, { kind: "error", message: event.message });
      break;
  }
}
