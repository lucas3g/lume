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

/** Muda a cada escolha do usuário: uma leitura atrasada do backend não desfaz a escolha feita depois dela. */
let choice = 0;

/** O backend é a fonte da verdade; a cópia local só evita piscar na abertura. */
export async function loadUiPrefs() {
  const asked = choice;
  const stored = await api.getPipelineView().catch(() => null);
  if (asked !== choice || !isMode(stored)) return;
  ui.pipelineView = stored;
  rememberLocally(stored);
}

/** Grava a escolha; se o backend recusar, volta ao modo anterior para as duas cópias não divergirem. */
export async function setPipelineView(mode: PipelineViewMode) {
  const previous = ui.pipelineView;
  const mine = ++choice;
  ui.pipelineView = mode;
  rememberLocally(mode);
  try {
    await api.setPipelineView(mode);
  } catch {
    if (mine !== choice) return;
    ui.pipelineView = previous;
    rememberLocally(previous);
  }
}
