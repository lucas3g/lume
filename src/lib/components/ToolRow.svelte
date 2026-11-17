<script lang="ts">
  import { formatDuration } from "../format";
  import { liveClock } from "../state/clock.svelte";
  import { summarize, type RunningTool, type ToolUse } from "../tools";
  import ToolPreview from "./ToolPreview.svelte";

  interface Props {
    tool: ToolUse;
    /** A ferramenta ainda está executando. */
    running?: boolean;
    /** Dados da execução em curso (início, se pode ir para segundo plano); dá o rodapé com o tempo. */
    live?: RunningTool | null;
    /** Linha dentro de um grupo: sem borda nem ícone, só o rótulo e a seta. */
    flat?: boolean;
    /** Abre o detalhe em outro lugar (ex.: o trabalho de um subagente) em vez de expandir a linha. */
    onopen?: () => void;
    /** Situação exibida à direita, no lugar do alvo, quando há `onopen`. */
    note?: string | null;
    /** Manda o comando em execução para segundo plano. */
    onbackground?: () => void;
  }
  let { tool, running = false, live = null, flat = false, onopen, note = null, onbackground }: Props = $props();

  const busy = $derived(running || live !== null);
  const summary = $derived(summarize(tool.name, tool.input));
  const failed = $derived(tool.is_error);
  const skipped = $derived(!busy && tool.output === null && !tool.is_error);
  const state = $derived(busy ? "executando" : failed ? "falhou" : skipped ? "não executada" : "concluída");
  const canBackground = $derived(live !== null && live.task && !live.backgrounded && onbackground !== undefined);
</script>

{#snippet icon()}
  {#if busy}
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
{/snippet}

{#snippet head()}
  {#if !flat}{@render icon()}{/if}
  <span class="label">{summary.label}</span>
  {#if summary.target}<span class="target">{summary.target}</span>{/if}
  {#if flat && failed}<span class="tag bad">falhou</span>{/if}
  <span class="sr-only">({state})</span>
{/snippet}

{#snippet chevron()}
  <svg class="chevron" aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 6l6 6-6 6" /></svg>
{/snippet}

{#if onopen}
  <button type="button" class="row" class:flat title="Ver o que o subagente fez" onclick={onopen}>
    {#if flat}{@render icon()}{/if}
    {@render head()}
    {#if note}<span class="note">{note}</span>{/if}
    {@render chevron()}
  </button>
{:else}
  <details class:flat>
    <summary>{@render head()}{#if flat}{@render chevron()}{/if}</summary>
    <div class="body">
      <ToolPreview input={tool.input} output={tool.output} />
      {#if live}
        <div class="foot">
          <span class="muted">{live.backgrounded ? "Em segundo plano" : "Em execução"} · {formatDuration(liveClock.now - live.startedAt)}</span>
          {#if canBackground}
            <button type="button" class="background" title="O comando segue rodando e o agente continua sem esperar" onclick={onbackground}>
              Executar em segundo plano
              <svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="8" y="8" width="12" height="12" rx="2" /><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" /></svg>
            </button>
          {/if}
        </div>
      {/if}
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
  .chevron { margin-left: auto; stroke: var(--muted); transition: transform 0.15s; }
  .spinner { flex-shrink: 0; width: 12px; height: 12px; margin: 1px; border: 2px solid var(--border-strong); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .label { flex-shrink: 0; font-size: 13px; font-weight: 500; }
  .target { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); font-size: 12px; color: var(--muted); }
  .note { flex-shrink: 0; margin-left: auto; font-size: 12px; color: var(--muted); }
  .note + .chevron { margin-left: 0; }
  .body { padding: 0 12px 12px; display: flex; flex-direction: column; gap: 8px; }
  .sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }

  /* Dentro de um grupo: linhas separadas por divisórias, rótulo apagado e seta logo após o texto. */
  details.flat, .row.flat { border: 0; border-radius: 0; background: transparent; }
  .flat summary, .row.flat { gap: 6px; min-height: 44px; padding: 0 18px; }
  .flat summary:hover, .row.flat:hover { background: var(--hover); border-radius: 0; }
  .flat summary:focus-visible, .row.flat:focus-visible { outline: 2px solid var(--accent); outline-offset: -4px; border-radius: 10px; }
  .flat .label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex-shrink: 1; font-size: 14px; font-weight: 400; color: var(--text-2); }
  .flat .target { flex-shrink: 1; }
  .flat .chevron { margin-left: 0; }
  .row.flat .chevron { margin-left: auto; }
  .row.flat { gap: 10px; }
  .row.flat .note + .chevron { margin-left: 0; }
  details.flat[open] > summary .chevron { transform: rotate(90deg); }
  .flat .body { padding: 2px 18px 14px; }
  .tag { flex-shrink: 0; font-size: 11.5px; }
  .tag.bad { color: var(--danger-text); }

  .foot { display: flex; align-items: center; gap: 12px; padding-top: 4px; font-size: 13px; }
  .muted { color: var(--muted); }
  .background { display: inline-flex; align-items: center; gap: 10px; margin-left: auto; padding: 4px 6px; border: 0; border-radius: 6px; background: transparent; color: var(--text); font-size: 13px; cursor: pointer; }
  .background:hover { background: var(--hover); }
  .background svg { stroke: currentColor; }
</style>
