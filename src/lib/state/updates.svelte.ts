import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { errorMessage } from "../api";

/**
 * Atualização do app, compartilhada entre o aviso no topo e o botão em Configurações.
 * `found`: a versão nova encontrada (null se não há); `checked`: quando a última busca
 * terminou, sem erro; `progress`: o download em andamento (null fora dele).
 */
export const updates = $state({
  current: null as string | null,
  found: null as Update | null,
  checking: false,
  checked: null as number | null,
  checkError: null as string | null,
  progress: null as number | null,
  installError: null as string | null,
  dismissed: false,
});

if (typeof window !== "undefined") {
  getVersion().then(
    (v) => (updates.current = v),
    () => {},
  );
}

/** Procura versão nova. Na busca automática, sem rede ou sem versão publicada fica em silêncio. */
export async function checkForUpdate(manual = false): Promise<void> {
  if (updates.checking || updates.progress !== null) return;
  updates.checking = true;
  if (manual) updates.checkError = null;
  try {
    const found = await check();
    if (found && found.version !== updates.found?.version) updates.dismissed = false;
    // Pedida por você, a versão encontrada volta a aparecer no topo mesmo se você tinha dispensado.
    if (found && manual) updates.dismissed = false;
    updates.found = found;
    updates.checked = Date.now();
    updates.checkError = null;
  } catch (e) {
    if (manual) {
      const message = errorMessage(e);
      // As versões publicadas só trazem o instalador do Windows: no macOS não há o que baixar.
      updates.checkError = /platforms?\b/i.test(message)
        ? "as versões publicadas são só para Windows; neste sistema, atualize compilando o Lume."
        : message;
    }
  } finally {
    updates.checking = false;
  }
}

/** Baixa, instala e reabre o app na versão encontrada. */
export async function installUpdate(): Promise<void> {
  const update = updates.found;
  if (!update || updates.progress !== null) return;
  updates.installError = null;
  updates.progress = 0;
  let total = 0;
  let received = 0;
  try {
    await update.downloadAndInstall((event) => {
      if (event.event === "Started") total = event.data.contentLength ?? 0;
      else if (event.event === "Progress") {
        received += event.data.chunkLength;
        updates.progress = total ? Math.round((received / total) * 100) : 0;
      }
    });
    await relaunch();
  } catch (e) {
    updates.installError = errorMessage(e);
    updates.progress = null;
  }
}
