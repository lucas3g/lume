// Como apresentar ao usuário o uso de uma ferramenta pelo agente.

import type { Question } from "./types";

export interface ToolUse {
  /** Id da chamada (mensagens antigas não têm). */
  id?: string;
  name: string;
  input: Record<string, unknown>;
  output: string | null;
  is_error: boolean;
}

export interface ToolSummary {
  /** Verbo da ação, ex.: "Editar". */
  label: string;
  /** Alvo da ação, ex.: caminho do arquivo ou comando. */
  target: string;
}

const LABELS: Record<string, string> = {
  Read: "Ler",
  Edit: "Editar",
  MultiEdit: "Editar",
  Write: "Criar arquivo",
  NotebookEdit: "Editar notebook",
  Bash: "Executar",
  Grep: "Buscar",
  Glob: "Listar arquivos",
  WebSearch: "Buscar na web",
  WebFetch: "Ler página",
  Skill: "Skill",
  Task: "Subagente",
  Agent: "Subagente",
  TodoWrite: "Atualizar tarefas",
  Artifact: "Claude Design",
};

/** Ferramentas que delegam o trabalho a um subagente. */
export function isAgentTool(name: string): boolean {
  return name === "Agent" || name === "Task";
}

/** Campo dos argumentos que melhor identifica o alvo, por ordem de preferência. */
const TARGET_FIELDS = ["file_path", "notebook_path", "command", "pattern", "query", "url", "skill", "description", "path"];

export function parseToolUse(content: string): ToolUse | null {
  try {
    const value = JSON.parse(content);
    if (typeof value?.name !== "string") return null;
    const id = typeof value.id === "string" ? value.id : undefined;
    return { id, name: value.name, input: value.input ?? {}, output: value.output ?? null, is_error: value.is_error === true };
  } catch {
    return null;
  }
}

export function summarize(name: string, input: Record<string, unknown>): ToolSummary {
  // A descrição que o agente dá ao comando (ex.: "Compilar o app") diz mais que o comando em si.
  if (typeof input.description === "string" && input.description.trim() && !isAgentTool(name)) {
    return { label: input.description.trim(), target: "" };
  }
  const field = TARGET_FIELDS.find((f) => typeof input[f] === "string" && input[f]);
  const target = field ? String(input[field]) : "";
  const connector = name.match(/^mcp__(.+?)__(.+)$/);
  if (connector) return { label: `Conector ${connector[1]}`, target: connector[2] };
  return { label: LABELS[name] ?? name, target };
}

/** Par antes/depois de uma edição, quando a ferramenta altera um trecho de arquivo. */
export function editDiff(input: Record<string, unknown>): { before: string; after: string } | null {
  if (typeof input.old_string === "string" && typeof input.new_string === "string") {
    return { before: input.old_string, after: input.new_string };
  }
  if (typeof input.content === "string") return { before: "", after: input.content };
  return null;
}

const RECOMMENDED_SUFFIX = /\s*\((recomendad[oa]|recommended)\)(?=,|\s*$)/gi;

/** O rótulo traz o sufixo "(Recomendado)" que marca a opção sugerida. */
export const isRecommended = (label: string) => new RegExp(RECOMMENDED_SUFFIX.source, "i").test(label);

/** Texto sem o sufixo "(Recomendado)" (de um rótulo ou de várias respostas separadas por vírgula). */
export const withoutRecommended = (text: string) => text.replace(RECOMMENDED_SUFFIX, "");

/** Caminho de um achado do code review sem o "(Recomendado)"; só com o marcador, fica como veio. */
export const optionText = (option: string) => withoutRecommended(option).trim() || option.trim();

/** Perguntas de um pedido do AskUserQuestion (vazio se o pedido não traz nenhuma). */
export function questionsOf(request: { input: string }): Question[] {
  const list = parseInput(request.input).questions;
  if (!Array.isArray(list)) return [];
  // Sem repetição: a pergunta é a chave do formulário (e um each com chave repetida quebra a tela).
  const seen = new Set<string>();
  return (list as Question[])
    .filter((q) => !!q && typeof q === "object" && typeof q.question === "string" && !seen.has(q.question) && !!seen.add(q.question))
    .map((q) => ({ ...q, options: Array.isArray(q.options) ? q.options.filter((o, i, all) => all.findIndex((x) => x.label === o.label) === i) : [] }));
}

export function parseInput(json: string): Record<string, unknown> {
  try {
    const value = JSON.parse(json);
    return value && typeof value === "object" ? value : {};
  } catch {
    return {};
  }
}

/** Ferramenta que ainda está rodando no turno, antes de virar mensagem gravada. */
export interface RunningTool {
  id: string;
  name: string;
  input: Record<string, unknown>;
  /** Quando começou (ms). */
  startedAt: number;
  /** Já virou tarefa no CLI e pode ir para segundo plano. */
  task: boolean;
  /** O pedido de segundo plano foi enviado. */
  backgrounded: boolean;
}

export type TodoStatus = "pending" | "in_progress" | "completed";

export interface Todo {
  text: string;
  status: TodoStatus;
}

/** Ferramentas que mantêm a lista de tarefas do agente. */
const TODO_TOOLS = new Set(["TodoWrite", "TaskCreate", "TaskUpdate"]);

export function isTodoTool(name: string): boolean {
  return TODO_TOOLS.has(name);
}

function todoStatus(value: unknown): TodoStatus {
  return value === "in_progress" || value === "completed" ? value : "pending";
}

/**
 * Lista de tarefas depois de uma sequência de chamadas: `TodoWrite` manda a lista inteira;
 * `TaskCreate`/`TaskUpdate` criam e mudam uma tarefa por vez (o id vem no resultado da criação).
 */
export function todosAfter(tools: ToolUse[]): Todo[] {
  let list: { id: string; todo: Todo }[] = [];
  let created = 0;
  for (const tool of tools) {
    const input = tool.input;
    if (tool.name === "TodoWrite" && Array.isArray(input.todos)) {
      list = input.todos.map((t: Record<string, unknown>, i: number) => ({
        id: String(i),
        todo: { text: String(t?.content ?? ""), status: todoStatus(t?.status) },
      }));
    } else if (tool.name === "TaskCreate") {
      created += 1;
      const id = tool.output?.match(/Task #(\S+?) created/)?.[1] ?? String(created);
      list.push({ id, todo: { text: String(input.subject ?? ""), status: "pending" } });
    } else if (tool.name === "TaskUpdate") {
      const entry = list.find((e) => e.id === String(input.taskId));
      if (!entry) continue;
      if (input.status === "deleted") list = list.filter((e) => e !== entry);
      else {
        if (input.status !== undefined) entry.todo.status = todoStatus(input.status);
        if (typeof input.subject === "string") entry.todo.text = input.subject;
      }
    }
  }
  return list.map((e) => e.todo).filter((t) => t.text);
}

/** Uma ação dentro de um grupo: gravada (`message`) ou ainda rodando (`running`). */
export interface ToolItem {
  key: string;
  tool: ToolUse;
  running: RunningTool | null;
}

export type ThreadBlock<M> =
  | { kind: "message"; key: string; message: M }
  | { kind: "tools"; key: string; items: ToolItem[] }
  | { kind: "todos"; key: string; todos: Todo[] }
  | { kind: "plan"; key: string; tool: ToolUse; version: number };

/** Ferramenta com que o agente pede a aprovação do plano. */
export const EXIT_PLAN = "ExitPlanMode";

/**
 * Monta a conversa em blocos: ferramentas seguidas viram um grupo; a lista de tarefas aparece
 * uma vez, como cartão, onde foi atualizada pela última vez; as que ainda rodam entram no fim.
 */
export function groupThread<M extends { id: number; role: string; content: string }>(
  messages: M[],
  running: RunningTool[],
): ThreadBlock<M>[] {
  type Entry = { message: M; tool: ToolUse | null } | { running: RunningTool; tool: ToolUse };
  const entries: Entry[] = messages.flatMap<Entry>((message) => {
    if (message.role !== "tool") return [{ message, tool: null }];
    // Uso de ferramenta ilegível não aparece (nem vira texto do agente).
    const tool = parseToolUse(message.content);
    return tool ? [{ message, tool }] : [];
  });
  const recorded = new Set(entries.map((e) => e.tool?.id).filter(Boolean));
  for (const r of running) {
    if (!recorded.has(r.id)) entries.push({ running: r, tool: { id: r.id, name: r.name, input: r.input, output: null, is_error: false } });
  }

  const todoTools = entries.flatMap((e) => (e.tool && isTodoTool(e.tool.name) ? [e.tool] : []));
  const lastTodo = todoTools.at(-1);
  const blocks: ThreadBlock<M>[] = [];
  let group: ToolItem[] | null = null;
  let plans = 0;
  for (const entry of entries) {
    const tool = entry.tool;
    if (!tool) {
      group = null;
      if ("message" in entry) blocks.push({ kind: "message", key: `m${entry.message.id}`, message: entry.message });
      continue;
    }
    const key = tool.id ?? ("message" in entry ? `m${entry.message.id}` : "");
    // Cada plano apresentado vira um cartão próprio; o que ainda espera aprovação aparece no pedido.
    if (tool.name === EXIT_PLAN) {
      if ("message" in entry) {
        group = null;
        blocks.push({ kind: "plan", key: `plan-${key}`, tool, version: ++plans });
      }
      continue;
    }
    if (isTodoTool(tool.name)) {
      if (tool !== lastTodo) continue;
      const todos = todosAfter(todoTools);
      if (todos.length) {
        group = null;
        blocks.push({ kind: "todos", key: `todos-${key}`, todos });
      }
      continue;
    }
    if (!group) {
      group = [];
      blocks.push({ kind: "tools", key: `g-${key}`, items: group });
    }
    group.push({ key, tool, running: "running" in entry ? entry.running : null });
  }
  return blocks;
}

/** Planos já apresentados na conversa (o próximo é a versão seguinte). */
export function planCount(messages: { role: string; content: string }[]): number {
  return messages.filter((m) => m.role === "tool" && parseToolUse(m.content)?.name === EXIT_PLAN).length;
}

const EDIT_TOOLS = new Set(["Edit", "MultiEdit", "Write", "NotebookEdit"]);

/** Arquivos que o agente criou ou alterou na conversa, sem repetir, na ordem em que apareceram. */
export function touchedFiles(messages: { role: string; content: string }[]): string[] {
  const files = new Set<string>();
  for (const m of messages) {
    if (m.role !== "tool") continue;
    const tool = parseToolUse(m.content);
    const path = tool && EDIT_TOOLS.has(tool.name) && !tool.is_error ? (tool.input.file_path ?? tool.input.notebook_path) : null;
    if (typeof path === "string" && path) files.add(path);
  }
  return [...files];
}

export type CheckState = "pass" | "fail" | "pending" | "skip";

/** Pull request que o agente abriu, conferiu ou juntou com o `gh` numa sequência de ações. */
export interface PullRequest {
  number: number;
  title: string | null;
  checks: { name: string; state: CheckState }[];
  merged: boolean;
}

const CHECK_STATES: Record<string, CheckState> = { pass: "pass", fail: "fail", pending: "pending", skipping: "skip" };

/** Pull requests de um grupo de ações, a partir dos comandos `gh pr` que já terminaram. */
export function pullRequests(items: ToolItem[]): PullRequest[] {
  const prs = new Map<number, PullRequest>();
  let current: number | null = null;
  for (const { tool } of items) {
    if (tool.name !== "Bash" || tool.output === null) continue;
    const command = String(tool.input.command ?? "");
    const action = command.match(/\bgh pr (create|checks|merge|view)\b(.*)/);
    if (!action) continue;
    const output = tool.output;
    const number: number = Number(action[2].match(/^\s+#?(\d+)/)?.[1] ?? output.match(/\/pull\/(\d+)/)?.[1] ?? output.match(/pull request #(\d+)/i)?.[1] ?? current ?? NaN);
    if (!Number.isFinite(number)) continue;
    current = number;
    const pr = prs.get(number) ?? { number, title: null, checks: [], merged: false };
    prs.set(number, pr);
    if (action[1] === "create") pr.title = command.match(/(?:--title|-t)\s+(["'])(.+?)\1/)?.[2] ?? pr.title;
    if (action[1] === "merge" && !tool.is_error && !/\s--auto\b/.test(action[2])) {
      pr.merged = true;
      pr.title = output.match(/Merged pull request #\d+ \((.+)\)/)?.[1] ?? pr.title;
    }
    if (action[1] === "checks") {
      for (const line of output.split("\n")) {
        const [name, status] = line.trim().split(/\t+|\s{2,}/);
        const state = status ? CHECK_STATES[status.toLowerCase()] : undefined;
        if (!name || !state) continue;
        pr.checks = [...pr.checks.filter((c) => c.name !== name), { name, state }];
      }
    }
  }
  return [...prs.values()];
}
