<script lang="ts">
  import { dismissable } from "../../actions";
  import { MODES } from "../../types";

  interface Props {
    mode: string;
    onmode: (mode: string) => void;
  }
  let { mode, onmode }: Props = $props();

  let open = $state(false);
  const current = $derived(MODES.find((m) => m.id === mode) ?? MODES[0]);

  function pick(id: string) {
    onmode(id);
    open = false;
  }
</script>

<div class="anchor" use:dismissable={() => (open = false)}>
  <button type="button" class="trigger" class:on={open} class:risky={current.risky} class:plan={current.id === "plan"} aria-haspopup="menu" aria-expanded={open} title="Modo de permissão" onclick={() => (open = !open)}>
    {#if current.id === "plan"}<svg aria-hidden="true" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 4L3 6v14l6-2 6 2 6-2V4l-6 2zM9 4v14M15 6v14" /></svg>{/if}
    {current.label}
  </button>

  {#if open}
    <div class="menu" role="menu" aria-label="Modo">
      <p class="label">Modo</p>
      {#each MODES as m (m.id)}
        <button type="button" role="menuitemradio" aria-checked={mode === m.id} class="item" onclick={() => pick(m.id)}>
          <span class="text">
            <span class="name">{m.label}{#if m.isDefault}<span class="tag">Padrão</span>{/if}</span>
            <span class="muted">{m.description}</span>
          </span>
          {#if mode === m.id}<svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--accent-text)" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .anchor { position: relative; }
  .trigger { min-height: 28px; padding: 0 8px; border: 0; border-radius: 7px; background: transparent; font-size: 12.5px; color: var(--text); }
  .trigger:hover, .trigger.on { background: var(--hover); }
  .trigger.risky { color: var(--warn-text); }
  /* No modo plano o seletor ganha a cor do acento: o agente só planeja até você aprovar. */
  .trigger.plan { display: inline-flex; align-items: center; gap: 6px; color: var(--accent-text); background: var(--acc-bg); }
  .menu { position: absolute; right: 0; bottom: calc(100% + 8px); z-index: 20; width: 300px; padding: 6px; border: 1px solid var(--border-strong); border-radius: 12px; background: var(--surface-2); box-shadow: 0 16px 40px rgba(0, 0, 0, 0.5); display: flex; flex-direction: column; gap: 1px; }
  .label { padding: 6px 10px; }
  .item { display: flex; align-items: flex-start; gap: 8px; padding: 7px 10px; border: 0; border-radius: 7px; background: transparent; text-align: left; color: var(--text); }
  .item:hover { background: var(--hover); }
  .text { flex: 1; display: flex; flex-direction: column; }
  .name { display: flex; align-items: center; gap: 8px; font-weight: 500; }
  .text .muted { font-size: 12px; line-height: 1.4; }
  .tag { padding: 0 7px; border: 1px solid var(--border-strong); border-radius: 999px; font-size: 11px; font-weight: 400; color: var(--text-2); }
  .item svg { flex-shrink: 0; margin-top: 3px; }
</style>
