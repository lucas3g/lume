<script lang="ts">
  import type { Snippet } from "svelte";
  import { formatDuration } from "../format";
  import { liveClock, useLiveClock } from "../state/clock.svelte";
  import { summarize, type ToolItem } from "../tools";

  interface Props {
    items: ToolItem[];
    /** O agente ainda está nesta sequência de ações. */
    live?: boolean;
    /** Início da sequência (ms), para o tempo que corre no cabeçalho. */
    since?: number | null;
    /** Cada ação, desenhada por quem usa o grupo. */
    row: Snippet<[ToolItem]>;
  }
  let { items, live = false, since = null, row }: Props = $props();

  // Aberto enquanto trabalha; sequências já concluídas começam recolhidas. A escolha do usuário vale.
  let chosen = $state<boolean | null>(null);
  const open = $derived(chosen ?? live);

  const last = $derived(items.at(-1));
  const label = $derived(last ? summarize(last.tool.name, last.tool.input).label : "");
  const failures = $derived(items.filter((i) => i.tool.is_error).length);
  const start = $derived(since ?? items.find((i) => i.running)?.running?.startedAt ?? null);
  const count = $derived(items.length === 1 ? "1 ação" : `${items.length} ações`);

  $effect(() => {
    if (live) return useLiveClock();
  });
</script>

<section class="group" aria-label={`Ações do agente: ${label}`}>
  <button type="button" class="head" aria-expanded={open} onclick={() => (chosen = !open)}>
    {#if live}
      <span class="dots" aria-hidden="true"><i></i><i></i><i></i><i></i></span>
    {:else if failures}
      <svg class="mark bad" aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke-width="2.4" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
    {:else}
      <svg class="mark" aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>
    {/if}
    <span class="label">{label}</span>
    <span class="meta">{live && start !== null ? formatDuration(liveClock.now - start) : count}</span>
    <svg class="chevron" class:open aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6" /></svg>
  </button>
  {#if open}
    <div class="box">
      {#each items as item (item.key)}
        <div class="item">{@render row(item)}</div>
      {/each}
    </div>
  {/if}
</section>

<style>
  .group { display: flex; flex-direction: column; gap: 10px; }
  .head { display: flex; align-items: center; gap: 12px; align-self: flex-start; max-width: 100%; padding: 4px 6px 4px 0; border: 0; background: transparent; color: var(--text-2); font-size: 14px; cursor: pointer; }
  .head:hover .label { color: var(--text); }
  .label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { flex-shrink: 0; color: var(--muted); }
  .chevron { flex-shrink: 0; margin-left: -6px; stroke: var(--muted); transform: rotate(-90deg); transition: transform 0.15s; }
  .chevron.open { transform: none; }
  .mark { flex-shrink: 0; width: 18px; stroke: var(--ok); }
  .mark.bad { stroke: var(--danger-text); }

  /* Quatro pontos que acendem em roda, como o agente "pensando". */
  .dots { flex-shrink: 0; display: grid; grid-template-columns: 6px 6px; gap: 5px; width: 18px; }
  .dots i { width: 5px; height: 5px; border-radius: 50%; background: var(--working); animation: blink 1.2s infinite ease-in-out; }
  .dots i:nth-child(2) { animation-delay: 0.3s; }
  .dots i:nth-child(4) { animation-delay: 0.6s; }
  .dots i:nth-child(3) { animation-delay: 0.9s; }
  @keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0.25; } }

  .box { margin-left: 30px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); overflow: hidden; }
  .item + .item { border-top: 1px solid var(--border); }
</style>
