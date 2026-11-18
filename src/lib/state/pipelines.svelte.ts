// Esteira de bots: execuções, a que está aberta e as mensagens das conversas das etapas.

import { api } from "../api";
import type { JiraIssue, Message, PastedImage, PipelineDetail, PipelineRun, StageRun } from "../types";
import { loadConversations } from "./chat.svelte";

export const pipelines = $state({
  list: [] as PipelineRun[],
  /** Execução aberta; `null` mostra a lista e o formulário. */
  selectedId: null as string | null,
  detail: null as PipelineDetail | null,
  /** Quando o detalhe chegou (ms): o tempo da etapa em andamento conta a partir daí. */
  fetchedAt: 0,
  /** Mensagens das conversas das etapas, por conversa. */
  messages: {} as Record<string, Message[]>,
  /** Pedido para preencher o formulário de uma execução nova (ex.: completar uma task vaga). */
  draft: null as {
    projectId: string;
    title: string;
    request: string;
    images: PastedImage[];
    /** Task do Jira escolhida na execução anterior (só a chave, quando a lista não a tem). */
    task: { key: string; site: string | null; issue: JiraIssue | null } | null;
  } | null,
});

export async function loadPipelines(): Promise<void> {
  pipelines.list = await api.listPipelines();
}

/** Recarrega a execução aberta e as mensagens das etapas que ainda não foram lidas ou estão rodando. */
export async function loadDetail(): Promise<void> {
  const id = pipelines.selectedId;
  if (!id) {
    pipelines.detail = null;
    return;
  }
  const detail = await api.pipelineDetail(id);
  if (pipelines.selectedId !== id) return;
  pipelines.detail = detail;
  pipelines.fetchedAt = Date.now();
  await Promise.all(
    detail.stages
      .filter((s) => s.conversation_id && (s.status === "running" || !pipelines.messages[s.conversation_id]))
      .map((s) => loadStageMessages(s.conversation_id)),
  );
}

/**
 * Mensagens de uma etapa. Quem roda de novo (Implementador, Revisor, Planejador, Designer) continua a
 * sessão da passagem anterior: as passagens dividem a conversa, e cada uma mostra só o trecho dela.
 */
export function stageMessages(stage: StageRun): Message[] {
  const all = pipelines.messages[stage.conversation_id] ?? [];
  const sharing = pipelines.detail?.stages.filter((s) => s.conversation_id === stage.conversation_id) ?? [];
  if (sharing.length < 2) return all;
  const next = sharing.find((s) => s.started_at > stage.started_at);
  const first = sharing[0]?.id === stage.id;
  return all.filter((m) => (first || m.created_at >= stage.started_at) && (!next || m.created_at < next.started_at));
}

export async function loadStageMessages(conversationId: string): Promise<void> {
  pipelines.messages[conversationId] = await api.listMessages(conversationId);
}

export async function openPipeline(id: string | null): Promise<void> {
  pipelines.selectedId = id;
  await loadDetail();
}

/** Uma execução mudou: atualiza a lista, a execução aberta e as conversas novas das etapas. */
export async function handlePipelineChanged(id: string): Promise<void> {
  await Promise.all([loadPipelines(), loadConversations(), id === pipelines.selectedId ? loadDetail() : Promise.resolve()]);
}

/** Conversa de alguma etapa da execução aberta. */
export function isStageConversation(conversationId: string): boolean {
  return !!pipelines.detail?.stages.some((s) => s.conversation_id === conversationId);
}
