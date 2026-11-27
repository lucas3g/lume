<script lang="ts">
  import type { Snippet } from "svelte";
  import { slide } from "svelte/transition";
  import { formatDuration } from "../format";
  import { liveClock, useLiveClock } from "../state/clock.svelte";
  import { pullRequests, summarize, type ToolItem } from "../tools";
  import PrCard from "./PrCard.svelte";

  interface Props {
    items: ToolItem[];
    /** O agente ainda está nesta sequência de ações. */
    live?: boolean;
    /** Início da sequência (ms), para o tempo que corre no cabeçalho. */
    since?: number | null;
    /** A linha do tempo continua depois deste grupo (outra etapa ou a explicação do agente). */
    rail?: boolean;
    /** Cada ação, desenhada por quem usa o grupo. */
    row: Snippet<[ToolItem]>;
  }
  let { items, live = false, since = null, rail = false, row }: Props = $props();

  // Aberto enquanto trabalha; sequências já concluídas começam recolhidas. A escolha do usuário vale.
  let chosen = $state<boolean | null>(null);
  const open = $derived(chosen ?? live);

  const last = $derived(items.at(-1));
  const label = $derived(last ? summarize(last.tool.name, last.tool.input).label : "");
  const failures = $derived(items.filter((i) => i.tool.is_error).length);
  /** A sequência terminou numa falha; falhas no meio, seguidas de acerto, só ganham o aviso. */
  const failed = $derived(!live && !!last?.tool.is_error);
  const start = $derived(since ?? items.find((i) => i.running)?.running?.startedAt ?? null);
  const count = $derived(items.length === 1 ? "1 ação" : `${items.length} ações`);
  const prs = $derived(live ? [] : pullRequests(items));

  $effect(() => {
    if (live) return useLiveClock();
  });
</script>

<section class="group" class:live class:failed class:rail aria-label={`Ações do agente: ${label}`} data-mm={failed ? label : undefined} data-mm-kind="bad">
  <span class="node" aria-hidden="true">
    {#if live}
      <span class="pulse"></span>
    {:else if failed}
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.8" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
    {:else}
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.8" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>
    {/if}
  </span>
  <div class="main">
    <button type="button" class="head" aria-expanded={open} onclick={() => (chosen = !open)}>
      <span class="label">{label}</span>
      <span class="chip">{live && start !== null ? formatDuration(liveClock.now - start) : count}</span>
      {#if failures && !failed && !live}
        <span class="chip retry" title="Uma ou mais ações falharam e o agente seguiu por outro caminho">{failures === 1 ? "1 falha no caminho" : `${failures} falhas no caminho`}</span>
      {/if}
      <svg class="chevron" class:open aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 6l6 6-6 6" /></svg>
    </button>
    {#if open}
      <div class="box" transition:slide={{ duration: 240 }}>
        {#each items as item (item.key)}
          <div class="item">{@render row(item)}</div>
        {/each}
      </div>
    {/if}
    {#each prs as pr (pr.number)}
      <PrCard {pr} />
    {/each}
  </div>
</section>

<style>
  .group { position: relative; display: grid; grid-template-columns: 24px minmax(0, 1fr); column-gap: 12px; }
  /* A linha do tempo desce do nó até a próxima etapa (o espaço entre blocos incluído). */
  .group.rail::before { content: ""; position: absolute; left: 11.5px; top: 30px; bottom: -16px; width: 1px; background: var(--rail); transform-origin: top; animation: draw 0.6s var(--ease) both; }
  .group:not(.failed):not(.live).rail::before { background: linear-gradient(rgba(88, 184, 138, 0.45), var(--rail) 60%); }

  .node { position: relative; z-index: 1; width: 24px; height: 24px; margin-top: 2px; display: grid; place-items: center; border-radius: 50%; border: 1px solid rgba(88, 184, 138, 0.45); background: var(--ok-solid); color: var(--ok); }
  .node svg { animation: pop 0.45s var(--spring); }
  .failed .node { border-color: var(--bad-line); background: var(--bad-solid); color: var(--bad); animation: shake 0.45s var(--ease); }
  .live .node { border-color: transparent; background: var(--surface); }
  .live .node::after { content: ""; position: absolute; inset: -2px; border-radius: 50%; background: conic-gradient(from 0deg, transparent 0 55%, var(--accent-text)); -webkit-mask: radial-gradient(farthest-side, transparent calc(100% - 2px), #000 calc(100% - 1.5px)); mask: radial-gradient(farthest-side, transparent calc(100% - 2px), #000 calc(100% - 1.5px)); animation: spin 0.9s linear infinite; }
  .pulse { width: 6px; height: 6px; border-radius: 50%; background: var(--accent-text); animation: breathe 1s ease-in-out infinite; }

  .main { min-width: 0; display: flex; flex-direction: column; gap: 10px; }
  .head { display: flex; align-items: center; gap: 10px; align-self: flex-start; max-width: 100%; min-height: 28px; margin-left: -8px; padding: 2px 8px; border: 0; border-radius: 8px; background: transparent; color: var(--text); font-size: 13.5px; font-weight: 500; cursor: pointer; transition: background 0.2s; }
  .head:hover { background: var(--surface); }
  .label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .live .label { background: linear-gradient(90deg, var(--muted) 20%, var(--text) 50%, var(--muted) 80%); background-size: 200% 100%; -webkit-background-clip: text; background-clip: text; color: transparent; animation: shimmer 1.4s linear infinite; }
  .chip { flex-shrink: 0; padding: 1px 7px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface-2); font-size: 11.5px; font-weight: 400; color: var(--muted); font-variant-numeric: tabular-nums; }
  .chip.retry { border-color: var(--warn-line); background: var(--warn-solid); color: var(--warn-text); }
  .chevron { flex-shrink: 0; stroke: var(--faint); transition: transform 0.35s var(--spring); }
  .chevron.open { transform: rotate(90deg); }

  .box { border: 1px solid var(--border); border-radius: 12px; background: var(--surface); overflow: hidden; }
  .item + .item { border-top: 1px solid var(--border); }

  @keyframes draw { from { transform: scaleY(0); } to { transform: scaleY(1); } }
  @keyframes spin { to { transform: rotate(360deg); } }
  @keyframes breathe { 50% { transform: scale(0.6); opacity: 0.5; } }
  @keyframes shimmer { from { background-position: 200% 0; } to { background-position: -200% 0; } }
  @keyframes pop { 0% { transform: scale(0.4); opacity: 0; } 60% { transform: scale(1.15); opacity: 1; } 100% { transform: scale(1); } }
  @keyframes shake { 25% { transform: translateX(-3px); } 50% { transform: translateX(3px); } 75% { transform: translateX(-2px); } }
</style>
