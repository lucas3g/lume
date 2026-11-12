// Painel lateral da conversa e situação da pasta da sessão.

import { api } from "../api";
import type { FileView, WorkspaceStatus } from "../types";

export type Panel = "environment" | "files" | "changes";

export const workspace = $state({
  /** Painel aberto à direita; `null` é fechado. */
  panel: "environment" as Panel | null,
  /** Situação do git da sessão aberta; `null` quando a conversa não tem pasta. */
  status: null as WorkspaceStatus | null,
  /** Arquivo aberto no centro, acima do chat. */
  openFile: null as FileView | null,
});

export function togglePanel(panel: Panel): void {
  workspace.panel = workspace.panel === panel ? null : panel;
}

/** Atualiza a situação da pasta; conversas sem pasta ficam sem situação. */
export async function refreshStatus(conversationId: string | null, hasFolder: boolean): Promise<void> {
  if (!conversationId || !hasFolder) {
    workspace.status = null;
    return;
  }
  try {
    workspace.status = await api.workspaceStatus(conversationId);
  } catch {
    workspace.status = null;
  }
}
