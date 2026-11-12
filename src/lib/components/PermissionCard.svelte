<script lang="ts">
  import type { PermissionRequest } from "../state/chat.svelte";
  import { parseInput, summarize } from "../tools";
  import ToolPreview from "./ToolPreview.svelte";

  interface Props {
    request: PermissionRequest;
    ondecide: (allow: boolean, always: boolean) => void;
  }
  let { request, ondecide }: Props = $props();

  const input = $derived(parseInput(request.input));
  const summary = $derived(summarize(request.tool, input));
</script>

<div class="card" role="group" aria-label="Pedido de aprovação">
  <div>
    <strong>Permitir esta ação?</strong>
    <div class="muted small">{summary.label}{summary.target ? ` · ${summary.target}` : ""}</div>
  </div>
  <ToolPreview {input} />
  <div class="actions">
    <button type="button" class="btn primary" onclick={() => ondecide(true, false)}>Permitir uma vez</button>
    <button type="button" class="btn" onclick={() => ondecide(true, true)}>Sempre permitir “{summary.label}” nesta sessão</button>
    <button type="button" class="btn" onclick={() => ondecide(false, false)}>Negar</button>
  </div>
</div>

<style>
  .card { padding: 16px; border-color: var(--border-strong); display: flex; flex-direction: column; gap: 12px; }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; }
  .actions .btn { min-height: 40px; }
</style>
