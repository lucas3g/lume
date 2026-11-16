import { api } from "./api";
import { accounts, activeAccount } from "./state/accounts.svelte";
import { chat } from "./state/chat.svelte";
import { pipelines } from "./state/pipelines.svelte";
import {
  duration, isActive, passesByRole, plannedModelText, stageAccountId, stageModelText, stageVisual, STATIONS, VISUAL,
  type Station, type Visual,
} from "./pipeline";
import { EFFORTS, type PipelineDetail, type Provider, type StageRun, type TierModels } from "./types";

/** Estado de uma estação na execução aberta, lido igual pela trilha e pelo escritório. */
export interface StationCard {
  station: Station;
  visual: Visual;
  label: string;
  account: string | null;
  agent: string | null;
  model: string | null;
  effort: string | null;
  off: boolean;
  time: string;
  /** Passagem mais recente da estação, se já rodou. */
  last: StageRun | null;
  /** Atividade atual da sessão do CLI ("Pensando", "Usando Read"...), enquanto ela roda. */
  activity: string | null;
}

/** Modelos de cada nível, para as estações que ainda não rodaram. Carregados uma vez. */
const tiers = $state<{ models: TierModels | null }>({ models: null });
let tiersLoading = false;
export function ensureTierModels() {
  if (tiers.models || tiersLoading) return;
  tiersLoading = true;
  api.pipelineModels().then((m) => (tiers.models = m), () => (tiersLoading = false));
}

/** Serviço da conta (pelo id ou pelo nome); sem conta ainda, o da conta que atende agora. */
const providerOf = (account: { id?: string | null; name?: string | null }): Provider =>
  accounts.list.find((a) => (account.id && a.id === account.id) || (account.name && a.name === account.name))?.provider ??
  activeAccount()?.provider ??
  "claude";

/** Estado de cada estação: o da passagem mais recente dela. Reativo quando lido dentro de `$derived`. */
export function stationCards(detail: PipelineDetail, now: number): StationCard[] {
  const passes = passesByRole(detail);
  const finished = !isActive(detail.status);
  return STATIONS.map((station) => {
    const list = passes[station.role];
    const last = list.at(-1) ?? null;
    const off = detail.skipped.includes(station.role);
    let visual: Visual = finished || off ? "skipped" : "waiting";
    let label = off ? "Desligada" : VISUAL[visual].label;
    let account: string | null = null;
    let activity: string | null = null;
    if (last) {
      const asking = !!chat.permissions[last.conversation_id];
      ({ visual, label } = stageVisual(last, asking));
      if (last.status === "running" && last.conversation_id) {
        const turn = chat.turns[last.conversation_id];
        account = turn?.account ?? null;
        activity = turn?.status ?? null;
      }
      if (!account && last.conversation_id) {
        const id = stageAccountId(pipelines.messages[last.conversation_id] ?? []);
        account = accounts.list.find((a) => a.id === id)?.name ?? null;
      }
      if (station.gate) account = "você";
    }
    const agentInfo = detail.agents[station.role] ?? null;
    let model: string | null = null;
    if (!station.gate) {
      if (last?.conversation_id) {
        const id = stageAccountId(pipelines.messages[last.conversation_id] ?? []);
        const live = last.status === "running" ? chat.turns[last.conversation_id]?.account : null;
        model = stageModelText(detail, last, providerOf(live ? { name: live } : { id }));
      }
      model ??= plannedModelText(station.role, providerOf({}), tiers.models, agentInfo?.model ?? null);
    }
    const spent = list.reduce((sum, s) => sum + ((s.finished_at ?? now) - s.started_at), 0);
    const agent = agentInfo?.name ?? null;
    const effortId = detail.efforts[station.role];
    const effort = effortId ? (EFFORTS.find((e) => e.id === effortId)?.label.toLowerCase() ?? effortId) : null;
    return { station, visual, label, account, agent, model, effort, off, time: list.length ? duration(spent) : "", last, activity };
  });
}
