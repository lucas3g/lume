<script lang="ts">
  import { summarize, type ToolUse } from "../tools";
  import ToolPreview from "./ToolPreview.svelte";

  interface Props {
    tool: ToolUse;
    /** A ferramenta ainda está executando. */
    running?: boolean;
    /** Abre o detalhe em outro lugar (ex.: o trabalho de um subagente) em vez de expandir a linha. */
    onopen?: () => void;
    /** Situação exibida à direita, no lugar do alvo, quando há `onopen`. */
    note?: string | null;
  }
  let { tool, running = false, onopen, note = null }: Props = $props();

  const summary = $derived(summarize(tool.name, tool.input));
  const failed = $derived(tool.is_error);
  const skipped = $derived(!running && tool.output === null && !tool.is_error);
  const state = $derived(running ? "executando" : failed ? "falhou" : skipped ? "não executada" : "concluída");
</script>

{#snippet head()}
  {#if running}
    <span class="spinner" aria-hidden="true"></span>
  {:else}
    <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" class:bad={failed} class:skipped>
      {#if failed || skipped}
        <path d="M6 6l12 12M18 6L6 18" />
      {:else}
        <path d="M5 12.5l4.5 4.5L19 7.5" />
      {/if}
    </svg>
  {/if}
  <span class="label">{summary.label}</span>
  <span class="target">{summary.target}</span>
  <span class="sr-only">({state})</span>
{/snippet}

{#if onopen}
  <button type="button" class="row" title="Ver o que o subagente fez" onclick={onopen}>
    {@render head()}
    {#if note}<span class="note">{note}</span>{/if}
    <svg class="chevron" aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 6l6 6-6 6" /></svg>
  </button>
{:else}
  <details>
    <summary>{@render head()}</summary>
    <div class="body">
      <ToolPreview input={tool.input} output={tool.output} />
    </div>
  </details>
{/if}

<style>
  details, .row { border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); }
  summary, .row { display: flex; align-items: center; gap: 12px; min-height: 38px; padding: 0 14px; cursor: pointer; list-style: none; }
  .row { width: 100%; text-align: left; color: var(--text); }
  summary::-webkit-details-marker { display: none; }
  summary:hover, .row:hover { background: var(--hover); border-radius: var(--radius); }
  svg { flex-shrink: 0; stroke: var(--ok); }
  svg.bad { stroke: var(--danger-text); }
  svg.skipped { stroke: var(--muted); }
  .chevron { margin-left: auto; stroke: var(--muted); }
  .spinner { flex-shrink: 0; width: 12px; height: 12px; margin: 1px; border: 2px solid var(--border-strong); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .label { flex-shrink: 0; font-size: 13px; font-weight: 500; }
  .target { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); font-size: 12px; color: var(--muted); }
  .note { flex-shrink: 0; margin-left: auto; font-size: 12px; color: var(--muted); }
  .note + .chevron { margin-left: 0; }
  .body { padding: 0 12px 12px; display: flex; flex-direction: column; gap: 8px; }
  .sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
</style>
