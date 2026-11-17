<script lang="ts">
  import { relaunch } from "@tauri-apps/plugin-process";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { onMount } from "svelte";
  import { errorMessage } from "../api";

  let update = $state<Update | null>(null);
  let progress = $state<number | null>(null);
  let error = $state<string | null>(null);
  let dismissed = $state(false);

  // Primeira checagem logo depois de abrir; depois, de hora em hora, para quem
  // deixa o app aberto o dia inteiro ver a versão nova sem precisar reabrir.
  const CHECK_DELAY_MS = 15_000;
  const CHECK_INTERVAL_MS = 60 * 60 * 1000;

  async function install() {
    if (!update) return;
    error = null;
    progress = 0;
    let total = 0;
    let received = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? 0;
        else if (event.event === "Progress") {
          received += event.data.chunkLength;
          progress = total ? Math.round((received / total) * 100) : null;
        }
      });
      await relaunch();
    } catch (e) {
      error = errorMessage(e);
      progress = null;
    }
  }

  function lookForUpdate() {
    if (progress !== null) return;
    void check()
      .then((found) => {
        if (!found) return;
        if (found.version !== update?.version) dismissed = false;
        update = found;
      })
      .catch(() => {
        /* sem rede ou sem versão publicada: silencioso */
      });
  }

  onMount(() => {
    const first = setTimeout(lookForUpdate, CHECK_DELAY_MS);
    const every = setInterval(lookForUpdate, CHECK_INTERVAL_MS);
    return () => {
      clearTimeout(first);
      clearInterval(every);
    };
  });
</script>

{#if update && !dismissed}
  <div class="banner" role="status">
    <span>Lume {update.version} disponível.{error ? ` ${error}` : ""}</span>
    {#if progress !== null}
      <span class="muted">Baixando{progress ? ` ${progress}%` : ""}…</span>
    {:else}
      <button type="button" class="btn small primary" onclick={install}>Atualizar e reabrir</button>
      <button type="button" class="btn small" onclick={() => (dismissed = true)}>Depois</button>
    {/if}
  </div>
{/if}

<style>
  .banner { display: flex; align-items: center; gap: 12px; padding: 8px 24px; background: var(--surface-2); border-bottom: 1px solid var(--border); font-size: 13px; }
  .banner span:first-child { flex: 1; }
</style>
