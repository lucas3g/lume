<script lang="ts">
  import { summarize, type ToolUse } from "../tools";
  import ToolPreview from "./ToolPreview.svelte";

  interface Props {
    tool: ToolUse;
  }
  let { tool }: Props = $props();

  const summary = $derived(summarize(tool.name, tool.input));
  const state = $derived(tool.is_error ? "falhou" : tool.output === null ? "não executada" : "concluída");
</script>

<details>
  <summary>
    <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" class:bad={tool.is_error} class:skipped={tool.output === null && !tool.is_error}>
      {#if tool.is_error || tool.output === null}
        <path d="M6 6l12 12M18 6L6 18" />
      {:else}
        <path d="M5 12.5l4.5 4.5L19 7.5" />
      {/if}
    </svg>
    <span class="label">{summary.label}</span>
    <span class="target">{summary.target}</span>
    <span class="sr-only">({state})</span>
  </summary>
  <div class="body">
    <ToolPreview input={tool.input} output={tool.output} />
  </div>
</details>

<style>
  details { border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); }
  summary { display: flex; align-items: center; gap: 12px; min-height: 38px; padding: 0 14px; cursor: pointer; list-style: none; }
  summary::-webkit-details-marker { display: none; }
  summary:hover { background: var(--hover); border-radius: var(--radius); }
  svg { flex-shrink: 0; stroke: var(--ok); }
  svg.bad { stroke: var(--danger-text); }
  svg.skipped { stroke: var(--muted); }
  .label { flex-shrink: 0; font-size: 13px; font-weight: 500; }
  .target { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); font-size: 12px; color: var(--muted); }
  .body { padding: 0 12px 12px; display: flex; flex-direction: column; gap: 8px; }
  .sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
</style>
