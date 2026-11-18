// Trabalho dos subagentes de cada conversa: o que cada um fez, passo a passo, ao vivo.

import { api } from "../api";
import { parseInput, summarize } from "../tools";
import type { SubagentStep } from "../types";

export type SubagentEntry =
  | { kind: "text"; text: string }
  | { kind: "tool"; id: string; name: string; input: Record<string, unknown>; output: string | null; is_error: boolean };

export type SubagentStatus = "running" | "completed" | "failed" | "stopped";

export interface SubagentRun {
  /** Id da chamada da ferramenta que criou o subagente. */
  id: string;
  description: string;
  agentType: string;
  prompt: string;
  background: boolean;
  status: SubagentStatus;
  /** O que ele está fazendo agora, enquanto trabalha. */
  activity: string | null;
  /** O andamento (`activity`) chegou depois da última ferramenta: é a notícia mais nova. */
  activityIsLatest: boolean;
  summary: string;
  entries: SubagentEntry[];
}

export const subagents = $state({
  /** Conversa → chamada → subagente; só as conversas já carregadas. */
  byConversation: {} as Record<string, Record<string, SubagentRun>>,
  /** Subagente aberto no painel lateral. */
  open: null as { conversationId: string; parent: string } | null,
});

/** Passos que chegaram enquanto a conversa ainda carregava os gravados. */
const buffered = new Map<string, { parent: string; step: SubagentStep }[]>();
const loading = new Map<string, Promise<void>>();

function newRun(id: string): SubagentRun {
  return { id, description: "", agentType: "", prompt: "", background: false, status: "running", activity: null, activityIsLatest: false, summary: "", entries: [] };
}

function finalStatus(status: string): SubagentStatus {
  if (status === "completed") return "completed";
  if (status === "failed") return "failed";
  return "stopped";
}

/** Aplica um passo; repetir um passo já aplicado não muda nada (os gravados e os ao vivo se sobrepõem). */
function fold(runs: Record<string, SubagentRun>, parent: string, step: SubagentStep): void {
  let run = runs[parent];
  // Andamento e fim também chegam de tarefas que não são subagentes (ex.: comandos em segundo plano).
  if (!run && (step.kind === "progress" || step.kind === "finished")) return;
  run ??= runs[parent] = newRun(parent);
  switch (step.kind) {
    case "started":
      run.description = step.description;
      run.agentType = step.agent_type;
      run.prompt = step.prompt;
      run.background = step.background;
      break;
    case "progress":
      if (run.status === "running") {
        run.activity = step.description;
        run.activityIsLatest = true;
      }
      break;
    case "text": {
      const last = run.entries.at(-1);
      if (last?.kind !== "text" || last.text !== step.text) run.entries.push({ kind: "text", text: step.text });
      break;
    }
    case "tool_call":
      if (!run.entries.some((e) => e.kind === "tool" && e.id === step.id)) {
        run.entries.push({ kind: "tool", id: step.id, name: step.name, input: parseInput(step.input), output: null, is_error: false });
        run.activityIsLatest = false;
      }
      break;
    case "tool_result": {
      const call = run.entries.find((e) => e.kind === "tool" && e.id === step.id);
      if (call?.kind === "tool") {
        call.output = step.output;
        call.is_error = step.is_error;
      }
      break;
    }
    case "finished":
      run.status = finalStatus(step.status);
      run.activity = null;
      if (step.summary) run.summary = step.summary;
      break;
  }
}

/** Carrega os passos gravados da conversa, uma vez; os que chegarem nesse meio-tempo são somados depois. */
export function loadSubagents(conversationId: string): Promise<void> {
  if (subagents.byConversation[conversationId]) return Promise.resolve();
  const pending = loading.get(conversationId);
  if (pending) return pending;
  const load = api
    .listSubagentSteps(conversationId)
    .then((rows) => {
      // Apagada enquanto carregava: nada a guardar.
      if (!loading.has(conversationId)) return;
      const runs: Record<string, SubagentRun> = {};
      for (const { parent, step } of rows) fold(runs, parent, step);
      // Os passos ao vivo são gravados antes de chegar: os que a leitura já trouxe não se repetem.
      const saved = new Set(rows.map((row) => JSON.stringify(row)));
      for (const row of buffered.get(conversationId) ?? []) {
        if (!saved.has(JSON.stringify(row))) fold(runs, row.parent, row.step);
      }
      buffered.delete(conversationId);
      subagents.byConversation[conversationId] = runs;
    })
    // Uma falha não fica guardada: a próxima abertura (ou passo ao vivo) tenta de novo.
    .catch(() => {})
    .finally(() => loading.delete(conversationId));
  loading.set(conversationId, load);
  return load;
}

export function applySubagentStep(conversationId: string, parent: string, step: SubagentStep): void {
  const runs = subagents.byConversation[conversationId];
  if (runs) {
    fold(runs, parent, step);
    return;
  }
  buffered.set(conversationId, [...(buffered.get(conversationId) ?? []), { parent, step }]);
  void loadSubagents(conversationId);
}

/** O turno acabou: quem ainda constava trabalhando foi interrompido junto. */
export function settleSubagents(conversationId: string): void {
  for (const run of Object.values(subagents.byConversation[conversationId] ?? {})) {
    if (run.status === "running") {
      run.status = "stopped";
      run.activity = null;
    }
  }
}

export function forgetSubagents(conversationId: string): void {
  delete subagents.byConversation[conversationId];
  buffered.delete(conversationId);
  // Uma leitura ainda em curso vê que a conversa saiu e não a recoloca.
  loading.delete(conversationId);
  if (subagents.open?.conversationId === conversationId) subagents.open = null;
}

export function openSubagent(conversationId: string, parent: string): void {
  subagents.open = { conversationId, parent };
}

export function closeSubagent(): void {
  subagents.open = null;
}

/** Subagentes da conversa, na ordem em que começaram. */
export function runsOf(conversationId: string | null): SubagentRun[] {
  return conversationId ? Object.values(subagents.byConversation[conversationId] ?? {}) : [];
}

/** Subagente criado pela chamada `parent` da conversa, se já se sabe dele. */
export function runOf(conversationId: string | null, parent: string): SubagentRun | undefined {
  return conversationId ? subagents.byConversation[conversationId]?.[parent] : undefined;
}

/** Rótulo curto da situação, para a interface. */
export function statusLabel(run: SubagentRun): string {
  switch (run.status) {
    case "running":
      return run.background ? "Trabalhando em segundo plano" : "Trabalhando";
    case "completed":
      return "Concluído";
    case "failed":
      return "Falhou";
    case "stopped":
      return "Interrompido";
  }
}

/** O turno da conversa que roda agora: a situação dele e as chamadas ainda abertas. */
export interface LiveTurn {
  status: string | null;
  running: { id: string }[];
}

/**
 * Subagentes que trabalham agora na conversa: os criados por uma chamada ainda aberta no turno e os que
 * seguem em segundo plano (a chamada deles volta logo). Um subagente gravado como "trabalhando" de uma
 * passagem antiga não entra: a chamada dele já fechou.
 */
function workingRuns(conversationId: string, turn: LiveTurn | undefined): SubagentRun[] {
  if (!turn) return [];
  const open = new Set(turn.running.map((r) => r.id));
  return runsOf(conversationId).filter((r) => r.status === "running" && (open.has(r.id) || r.background));
}

/** Uma linha sobre a ferramenta, ex.: "Ler src/a.ts". */
function toolLine(name: string, input: Record<string, unknown>): string {
  const { label, target } = summarize(name, input);
  return target ? `${label} ${target}` : label;
}

/**
 * O que a conversa está fazendo agora, contando os subagentes: enquanto uma skill ou um subagente
 * trabalha, o agente principal fica parado na chamada (ex.: "Carregando uma skill") e quem mostra o
 * andamento é o subagente: o texto de andamento, se é mais novo, ou a última ferramenta. Sem
 * subagente trabalhando, vale a situação do turno.
 */
export function liveActivity(conversationId: string, turn: LiveTurn | undefined): string | null {
  const run = workingRuns(conversationId, turn).at(-1);
  if (!run) return turn?.status ?? null;
  const tool = run.entries.findLast((e) => e.kind === "tool");
  if (tool?.kind === "tool" && !(run.activity && run.activityIsLatest)) return toolLine(tool.name, tool.input);
  return run.activity ?? (run.description || turn?.status || null);
}

/**
 * Andamento do subagente em curso para a linha ao vivo do registro, que já lista as ferramentas dele:
 * só o texto de andamento, sem repetir a última ferramenta. `null` sem subagente trabalhando.
 */
export function liveSubagentProgress(conversationId: string, turn: LiveTurn | undefined): string | null {
  const run = workingRuns(conversationId, turn).at(-1);
  if (!run) return null;
  return run.activity ?? (run.description || "Subagente trabalhando");
}

/** Ferramentas que os subagentes em curso na conversa já usaram, para o registro ao vivo da etapa. */
export function liveSubagentTools(conversationId: string, turn: LiveTurn | undefined): Extract<SubagentEntry, { kind: "tool" }>[] {
  return workingRuns(conversationId, turn).flatMap((r) => r.entries.filter((e) => e.kind === "tool"));
}
