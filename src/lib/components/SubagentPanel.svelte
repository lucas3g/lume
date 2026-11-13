<script lang="ts">
  import { tick } from "svelte";
  import { renderMarkdown } from "../format";
  import { statusLabel, type SubagentRun } from "../state/subagents.svelte";
  import ToolRow from "./ToolRow.svelte";

  interface Props {
    run: SubagentRun | undefined;
    /** Argumentos da chamada que criou o subagente, para quando o início não foi registrado. */
    fallback?: { description: string; prompt: string } | null;
    onclose: () => void;
  }
  let { run, fallback = null, onclose }: Props = $props();

  let scroller = $state<HTMLDivElement>();
  /** Segue o fim enquanto o usuário não rolar para cima. */
  let follow = $state(true);

  const description = $derived(run?.description || fallback?.description || "Subagente");
  const prompt = $derived(run?.prompt || fallback?.prompt || "");
  const entries = $derived(run?.entries ?? []);
  const lastText = $derived(entries.findLast((e) => e.kind === "text"));
  // O resumo final costuma repetir a última fala; só aparece se trouxer algo novo.
  const showSummary = $derived(!!run?.summary && (lastText?.kind !== "text" || lastText.text.trim() !== run.summary.trim()));

  function onscroll() {
    if (!scroller) return;
    follow = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 40;
  }

  $effect(() => {
    void [entries.length, run?.activity, run?.status, entries.at(-1)];
    if (follow) void tick().then(() => scroller?.scrollTo({ top: scroller.scrollHeight }));
  });
</script>

<section aria-label={`Subagente: ${description}`}>
  <header>
    <div class="title">
      <strong>{description}</strong>
      <span class="meta">
        {#if run}
          <span class="badge {run.status}">{#if run.status === "running"}<span class="pulse" aria-hidden="true"></span>{/if}{statusLabel(run)}</span>
          {#if run.agentType}<span class="mono">{run.agentType}</span>{/if}
        {:else}
          <span class="badge stopped">Sem registro da atividade</span>
        {/if}
      </span>
    </div>
    <button type="button" class="btn icon" aria-label="Fechar o subagente" title="Fechar" onclick={onclose}>
      <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
    </button>
  </header>

  <div class="scroll" bind:this={scroller} {onscroll}>
    {#if prompt}
      <details class="prompt">
        <summary>Tarefa recebida</summary>
        <div class="markdown">{@html renderMarkdown(prompt)}</div>
      </details>
    {/if}

    {#each entries as entry, i (entry.kind === "tool" ? entry.id : `t${i}`)}
      {#if entry.kind === "text"}
        <div class="markdown">{@html renderMarkdown(entry.text)}</div>
      {:else}
        <ToolRow tool={entry} running={entry.output === null && run?.status === "running"} />
      {/if}
    {/each}

    {#if run?.status === "running"}
      <div class="working" aria-live="polite"><span class="pulse" aria-hidden="true"></span>{run.activity ?? "Trabalhando"}…</div>
    {:else if showSummary && run}
      <div class="summary">
        <span class="label">Resultado</span>
        <div class="markdown">{@html renderMarkdown(run.summary)}</div>
      </div>
    {:else if run && entries.length === 0}
      <p class="muted">Nenhum passo registrado.</p>
    {:else if !run}
      <p class="muted">Este subagente rodou antes de o Lume registrar a atividade dos subagentes.</p>
    {/if}
  </div>
</section>

<style>
  section { flex: 1.2 1 440px; min-width: 0; height: 100%; display: flex; flex-direction: column; border-left: 1px solid var(--border); background: var(--bg-side); }
  header { display: flex; align-items: center; gap: 10px; min-height: 48px; padding: 6px 12px 6px 16px; border-bottom: 1px solid var(--border); }
  .title { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .title strong { font-size: 13.5px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { display: flex; align-items: center; gap: 8px; font-size: 11.5px; color: var(--muted); }
  .mono { font-family: var(--mono); }
  .badge { display: inline-flex; align-items: center; gap: 6px; }
  .badge.completed { color: var(--ok); }
  .badge.failed { color: var(--danger-text); }
  .scroll { flex: 1; overflow-y: auto; padding: 16px; display: flex; flex-direction: column; gap: 10px; }
  .prompt { border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); }
  .prompt summary { padding: 9px 14px; font-size: 12.5px; font-weight: 500; color: var(--text-2); cursor: pointer; }
  .prompt .markdown { padding: 0 14px 12px; font-size: 13px; }
  .markdown { font-size: 13.5px; overflow-wrap: anywhere; user-select: text; }
  .markdown :global(p) { margin: 0 0 10px; }
  .markdown :global(> :last-child) { margin-bottom: 0; }
  .markdown :global(pre) { margin: 0 0 10px; padding: 10px 12px; overflow-x: auto; border: 1px solid var(--border); border-radius: 8px; background: var(--bg); }
  .markdown :global(code) { font-family: var(--mono); font-size: 12px; }
  .markdown :global(ul), .markdown :global(ol) { margin: 0 0 10px; padding-left: 20px; }
  .working { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--muted); }
  .pulse { width: 7px; height: 7px; border-radius: 50%; background: var(--accent); animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.3; } }
  .summary { display: flex; flex-direction: column; gap: 6px; padding: 12px 14px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); }
  .summary .label { font-size: 11.5px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }
  .muted { margin: 0; font-size: 13px; color: var(--muted); }
</style>
