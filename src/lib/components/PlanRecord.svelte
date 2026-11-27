<script lang="ts">
  import { slide } from "svelte/transition";
  import { renderMarkdown } from "../format";
  import { parsePlan } from "../plan";
  import type { ToolUse } from "../tools";

  /** Um plano já respondido na conversa: aprovado ou com ajustes pedidos. */
  let { tool, version }: { tool: ToolUse; version: number } = $props();

  const markdown = $derived(String(tool.input.plan ?? ""));
  const plan = $derived(parsePlan(markdown));
  const approved = $derived(!tool.is_error && tool.output !== null);
  /** O turno terminou antes da resposta (ex.: interrompido): nem aprovado nem recusado. */
  const unanswered = $derived(tool.output === null);
  /** O que o usuário pediu para mudar, quando a recusa trouxe texto. */
  const asked = $derived(!approved && tool.output && !tool.output.startsWith("O usuário não autorizou") ? tool.output : null);
  let open = $state(false);
</script>

<section class="record" class:approved aria-label={`Plano v${version}`}>
  <button type="button" class="head" aria-expanded={open} onclick={() => (open = !open)}>
    <span class="ic" aria-hidden="true">
      {#if approved}
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>
      {:else}
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 4L3 6v14l6-2 6 2 6-2V4l-6 2zM9 4v14M15 6v14" /></svg>
      {/if}
    </span>
    <span class="tx">
      <span class="kicker">{approved ? "Plano aprovado" : unanswered ? "Plano sem resposta" : "Ajustes pedidos"} · v{version}</span>
      <span class="title">{plan.title}</span>
    </span>
    <span class="more">{open ? "Esconder plano" : "Ver plano"}</span>
  </button>
  {#if asked}
    <p class="asked">{asked}</p>
  {/if}
  {#if open}
    <div class="markdown" transition:slide={{ duration: 260 }}>{@html renderMarkdown(markdown)}</div>
  {/if}
</section>

<style>
  .record { border: 1px solid var(--note-line); border-radius: 12px; background: var(--surface); overflow: hidden; }
  .record.approved { border-color: var(--ok-line); }
  .head { width: 100%; display: flex; align-items: center; gap: 12px; padding: 10px 12px; border: 0; background: transparent; text-align: left; color: var(--text); }
  .head:hover { background: var(--hover); }
  .ic { width: 26px; height: 26px; flex-shrink: 0; display: grid; place-items: center; border-radius: 8px; color: var(--note); background: var(--note-bg); }
  .approved .ic { color: var(--ok); background: var(--ok-solid); }
  .tx { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .kicker { font-size: 10.5px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: var(--note); }
  .approved .kicker { color: var(--ok-text); }
  .title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13.5px; font-weight: 500; }
  .more { flex-shrink: 0; font-size: 12px; color: var(--muted); }
  .asked { margin: 0; padding: 0 12px 12px 50px; font-size: 13px; color: var(--text-2); white-space: pre-wrap; }
  .markdown { padding: 4px 16px 14px 50px; font-size: 13px; color: var(--text-body); overflow-wrap: anywhere; border-top: 1px solid var(--border); }
  .markdown :global(p), .markdown :global(ul), .markdown :global(ol) { margin: 10px 0 0; }
  .markdown :global(ul), .markdown :global(ol) { padding-left: 20px; }
  .markdown :global(h1), .markdown :global(h2), .markdown :global(h3) { margin: 14px 0 0; font-size: 13.5px; color: var(--text); }
  .markdown :global(code) { font-family: var(--mono); font-size: 12px; }
</style>
