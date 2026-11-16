import { api } from "../api";

/** Como a execução aberta da esteira aparece. */
export type PipelineViewMode = "track" | "office";

const VIEW_KEY = "lume.pipelineView";

const isMode = (value: unknown): value is PipelineViewMode => value === "track" || value === "office";

/** Cópia local, para a tela abrir já no modo certo antes de o backend responder. */
function savedMode(): PipelineViewMode {
  try {
    const value = localStorage.getItem(VIEW_KEY);
    if (isMode(value)) return value;
  } catch {
    // Sem armazenamento local vale o padrão até as configurações chegarem.
  }
  return "track";
}

function rememberLocally(mode: PipelineViewMode) {
  try {
    localStorage.setItem(VIEW_KEY, mode);
  } catch {
    // Sem armazenamento local vale a cópia das configurações.
  }
}

/** Preferências de interface. */
export const ui = $state<{ pipelineView: PipelineViewMode }>({ pipelineView: savedMode() });

export async function loadUiPrefs() {
  const stored = await api.getPipelineView().catch(() => null);
  if (isMode(stored)) {
    ui.pipelineView = stored;
    rememberLocally(stored);
  }
}

export function setPipelineView(mode: PipelineViewMode) {
  ui.pipelineView = mode;
  rememberLocally(mode);
  api.setPipelineView(mode).catch(() => {});
}
