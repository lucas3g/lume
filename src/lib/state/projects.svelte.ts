// Projetos cadastrados e qual está em foco na interface.

import { api } from "../api";
import type { Project } from "../types";

export const projects = $state({
  list: [] as Project[],
  /** Projeto aberto na tela de projeto; `null` é o formulário de um projeto novo. */
  selectedId: null as string | null,
  /** Projeto em que a próxima conversa nova será criada. */
  pendingId: null as string | null,
});

export async function loadProjects(): Promise<void> {
  projects.list = await api.listProjects();
}

export function projectName(id: string | null): string | null {
  return projects.list.find((p) => p.id === id)?.name ?? null;
}
