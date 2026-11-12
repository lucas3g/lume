// Agentes cadastrados, compartilhados entre a tela de Skills e a conversa.

import { api } from "../api";
import type { Agent } from "../types";

export const agents = $state({ list: [] as Agent[] });

export async function loadAgents(): Promise<void> {
  agents.list = await api.listAgents();
}

export function agentName(id: string | null): string | null {
  return agents.list.find((a) => a.id === id)?.name ?? null;
}
