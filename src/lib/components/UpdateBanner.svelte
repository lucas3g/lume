<script lang="ts">
  import { onMount } from "svelte";
  import { checkForUpdate, installUpdate, updates } from "../state/updates.svelte";

  // Primeira checagem logo depois de abrir; depois, de hora em hora, para quem
  // deixa o app aberto o dia inteiro ver a versão nova sem precisar reabrir.
  // Em Configurações dá para procurar na hora.
  const CHECK_DELAY_MS = 15_000;
  const CHECK_INTERVAL_MS = 60 * 60 * 1000;

  onMount(() => {
    const first = setTimeout(() => void checkForUpdate(), CHECK_DELAY_MS);
    const every = setInterval(() => void checkForUpdate(), CHECK_INTERVAL_MS);
    return () => {
      clearTimeout(first);
      clearInterval(every);
    };
  });
</script>

{#if updates.found && !updates.dismissed}
  <div class="banner" role="status">
    <span>Lume {updates.found.version} disponível.{updates.installError ? ` ${updates.installError}` : ""}</span>
    {#if updates.progress !== null}
      <span class="muted">Baixando{updates.progress ? ` ${updates.progress}%` : ""}…</span>
    {:else}
      <button type="button" class="btn small primary" onclick={installUpdate}>Atualizar e reabrir</button>
      <button type="button" class="btn small" onclick={() => (updates.dismissed = true)}>Depois</button>
    {/if}
  </div>
{/if}

<style>
  .banner { display: flex; align-items: center; gap: 12px; padding: 8px 24px; background: var(--surface-2); border-bottom: 1px solid var(--border); font-size: 13px; }
  .banner span:first-child { flex: 1; }
</style>
