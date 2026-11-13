<script lang="ts">
  import { renderMarkdown } from "../format";
  import type { PermissionRequest } from "../state/chat.svelte";
  import { parseInput } from "../tools";

  interface Props {
    request: PermissionRequest;
    /** Aprovado: o modo com que a sessão segue; `null` nega e o agente continua planejando. */
    ondecide: (nextMode: string | null) => void;
  }
  let { request, ondecide }: Props = $props();

  const plan = $derived(String(parseInput(request.input).plan ?? ""));
</script>

<div class="card" role="group" aria-label="Plano para aprovação">
  <strong>O agente propõe este plano</strong>
  <div class="plan markdown">{@html renderMarkdown(plan)}</div>
  <div class="actions">
    <button type="button" class="btn primary" onclick={() => ondecide("acceptEdits")}>Aprovar e aceitar edições</button>
    <button type="button" class="btn" onclick={() => ondecide("auto")}>Aprovar, perguntando antes de agir</button>
    <button type="button" class="btn" onclick={() => ondecide(null)}>Pedir mudanças no plano</button>
  </div>
</div>

<style>
  .card { padding: 16px; border-color: var(--border-strong); display: flex; flex-direction: column; gap: 12px; }
  .plan { max-height: 420px; overflow-y: auto; padding: 12px 14px; border: 1px solid var(--border); border-radius: 10px; background: var(--bg-side); font-size: 13px; }
  .plan :global(p) { margin: 0 0 10px; }
  .plan :global(ul), .plan :global(ol) { margin: 0 0 10px; padding-left: 20px; }
  .plan :global(h1), .plan :global(h2), .plan :global(h3) { margin: 10px 0 6px; font-size: 14px; }
  .plan :global(code) { font-family: var(--mono); font-size: 12px; }
  .plan :global(> :last-child) { margin-bottom: 0; }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; }
  .actions .btn { min-height: 40px; }
</style>
