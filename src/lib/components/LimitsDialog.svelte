<script lang="ts">
  import { api, errorMessage } from "../api";
  import { formatReset, percent } from "../format";
  import type { Account, UsageLimit } from "../types";

  interface Props {
    /** Conta cujos limites aparecem; `null` mantém o diálogo fechado. */
    account: Account | null;
    onclose: () => void;
  }
  let { account, onclose }: Props = $props();

  let dialog = $state<HTMLDialogElement>();
  let limits = $state<UsageLimit[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  const session = $derived(limits.filter((l) => l.kind === "session"));
  const weekly = $derived(limits.filter((l) => l.kind !== "session"));

  // O elemento nativo cuida do foco, do Esc e do fundo escurecido.
  $effect(() => {
    if (account && !dialog?.open) dialog?.showModal();
    if (!account && dialog?.open) dialog.close();
  });

  // Cada abertura consulta o provedor de novo: o número guardado pode estar velho.
  $effect(() => {
    const id = account?.id;
    if (id) void load(id);
  });

  async function load(id: string) {
    loading = true;
    error = null;
    limits = [];
    try {
      const found = await api.accountLimits(id);
      if (account?.id === id) limits = found;
    } catch (e) {
      if (account?.id === id) error = errorMessage(e);
    } finally {
      if (account?.id === id) loading = false;
    }
  }

  function title(l: UsageLimit): string {
    if (l.kind === "session") return "Sessão atual";
    return l.model ?? "Todos os modelos";
  }

  /** Clique no fundo (fora do conteúdo) fecha. */
  function onBackdrop(event: MouseEvent) {
    if (event.target === dialog) onclose();
  }
</script>

{#snippet meter(l: UsageLimit)}
  <div class="limit">
    <strong>{title(l)}</strong>
    <div class="line muted">
      <span>{l.resets_at ? `Zera ${formatReset(l.resets_at)}` : "Sem data para zerar"}</span>
      <span>{percent(l.used)}% usado</span>
    </div>
    <div class="bar" role="img" aria-label={`${title(l)}: ${percent(l.used)}% usado`}>
      <div class:full={l.used >= 1} style:width={`${percent(l.used)}%`}></div>
    </div>
  </div>
{/snippet}

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={dialog} aria-labelledby="limits-title" onclose={onclose} onclick={onBackdrop}>
  <div class="content">
    <h2 id="limits-title">Limites de uso{#if account}<span class="muted"> · {account.name}</span>{/if}</h2>

    {#if loading}
      <p class="muted" role="status">Consultando o provedor…</p>
    {:else if error}
      <p class="error-text" role="alert">{error}</p>
    {:else}
      {#each session as l (l.kind)}{@render meter(l)}{/each}
      {#if weekly.length}
        <h3>Limites semanais</h3>
        {#each weekly as l (l.kind + (l.model ?? ""))}{@render meter(l)}{/each}
      {/if}
    {/if}

    <div class="row foot">
      {#if account && !loading}<button type="button" class="btn" onclick={() => account && load(account.id)}>Atualizar</button>{/if}
      <button type="button" class="btn primary" onclick={onclose}>Fechar</button>
    </div>
  </div>
</dialog>

<style>
  dialog { width: min(480px, calc(100vw - 32px)); padding: 0; border: 1px solid var(--border-strong); border-radius: 16px; background: var(--surface); color: var(--text); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6); }
  dialog::backdrop { background: rgba(0, 0, 0, 0.55); }
  .content { padding: 20px; display: flex; flex-direction: column; gap: 16px; }
  h2 { font-family: var(--display); font-size: 19px; font-weight: 600; letter-spacing: -0.01em; }
  h2 .muted { font-family: inherit; font-size: 14px; font-weight: 400; }
  h3 { margin-top: 6px; font-size: 14px; font-weight: 600; }
  .limit { display: flex; flex-direction: column; gap: 6px; }
  .limit strong { font-weight: 500; }
  .line { display: flex; justify-content: space-between; gap: 12px; font-size: 12.5px; }
  .bar { height: 6px; border-radius: 3px; background: var(--border); overflow: hidden; }
  .bar div { height: 100%; border-radius: 3px; background: var(--bar); }
  .bar div.full { background: var(--warn); }
  .foot { justify-content: flex-end; gap: 8px; margin-top: 4px; }
</style>
