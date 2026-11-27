<script lang="ts">
  import type { PullRequest } from "../tools";

  let { pr }: { pr: PullRequest } = $props();

  const passed = $derived(pr.checks.filter((c) => c.state === "pass" || c.state === "skip").length);
  const failed = $derived(pr.checks.some((c) => c.state === "fail"));
  const pending = $derived(pr.checks.some((c) => c.state === "pending"));
</script>

<div class="pr" aria-label={`Pull request #${pr.number}`}>
  <div class="top">
    <span class="ic" aria-hidden="true">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="2" /><circle cx="6" cy="18" r="2" /><circle cx="18" cy="18" r="2" /><path d="M6 8v8M18 16V9a3 3 0 0 0-3-3h-4M13 3l-3 3 3 3" /></svg>
    </span>
    <span class="title">#{pr.number}{pr.title ? ` · ${pr.title}` : ""}</span>
    {#if pr.merged}
      <span class="badge merged">merge feito</span>
    {:else if pr.checks.length}
      <span class="badge" class:ok={!failed && !pending} class:bad={failed}>{passed} de {pr.checks.length} checks</span>
    {/if}
  </div>
  {#if pr.checks.length}
    <ul class="checks" aria-label="Verificações">
      {#each pr.checks as check (check.name)}
        <li class={check.state}>
          {#if check.state === "fail"}
            <svg aria-hidden="true" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.8" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
          {:else if check.state === "pending"}
            <span class="dot" aria-hidden="true"></span>
          {:else}
            <svg aria-hidden="true" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.8" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>
          {/if}
          {check.name}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .pr { border: 1px solid var(--border); border-radius: 12px; background: var(--surface); overflow: hidden; }
  .top { display: flex; align-items: center; gap: 10px; padding: 10px 12px; }
  .ic { width: 28px; height: 28px; flex-shrink: 0; display: grid; place-items: center; border-radius: 8px; color: var(--note); background: var(--note-bg); }
  .title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; font-weight: 500; color: var(--text); }
  .badge { flex-shrink: 0; padding: 2px 8px; border-radius: 999px; font-size: 11.5px; color: var(--muted); background: var(--surface-2); }
  .badge.ok { color: var(--ok-text); background: var(--ok-bg); }
  .badge.bad { color: var(--danger-text); background: var(--danger-bg); }
  .badge.merged { color: var(--note); background: var(--note-bg); }
  .checks { margin: 0; padding: 8px 12px; list-style: none; display: flex; flex-wrap: wrap; gap: 4px 14px; border-top: 1px solid var(--border); background: var(--bg-side); }
  .checks li { display: inline-flex; align-items: center; gap: 6px; font-family: var(--mono); font-size: 11.5px; color: var(--text-2); }
  .checks .pass svg, .checks .skip svg { color: var(--ok); }
  .checks .fail { color: var(--danger-text); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--warn); animation: blink 1.2s ease-in-out infinite; }
  @keyframes blink { 50% { opacity: 0.3; } }
</style>
