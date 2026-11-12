<script lang="ts">
  import { dismissable } from "../../actions";
  import { EFFORTS, MODELS, MORE_MODELS, modelLabel } from "../../types";

  interface Props {
    model: string;
    effort: string;
    /** O agente da conversa fixa o modelo. */
    locked: boolean;
    onmodel: (model: string) => void;
    oneffort: (effort: string) => void;
  }
  let { model, effort, locked, onmodel, oneffort }: Props = $props();

  let open = $state(false);
  let submenu = $state<"effort" | "more" | null>(null);

  const effortLabel = $derived(EFFORTS.find((e) => e.id === effort)?.label ?? "Médio");

  function close() {
    open = false;
    submenu = null;
  }

  function pickModel(id: string) {
    onmodel(id);
    close();
  }

  function pickEffort(id: string) {
    oneffort(id);
    close();
  }
</script>

<div class="anchor" use:dismissable={close}>
  <button type="button" class="trigger" class:on={open} aria-haspopup="menu" aria-expanded={open} onclick={() => (open ? close() : (open = true))}>
    <span>{modelLabel(model)}</span>
    <span class="muted">{effortLabel}</span>
  </button>

  {#if open}
    <div class="popover">
      {#if submenu === "effort"}
        <div class="menu side" role="menu" aria-label="Esforço">
          <p class="hint">Mais esforço significa respostas mais completas, mas leva mais tempo e consome seus limites mais rápido.</p>
          {#each EFFORTS as e (e.id)}
            <button type="button" role="menuitemradio" aria-checked={effort === e.id} class="item" onclick={() => pickEffort(e.id)}>
              <span>{e.label}</span>
              {#if e.note}<span class="tag" class:warn={e.warn}>{e.note}</span>{/if}
              <span class="grow"></span>
              {#if effort === e.id}<svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--accent-text)" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{/if}
            </button>
          {/each}
        </div>
      {:else if submenu === "more"}
        <div class="menu side" role="menu" aria-label="Mais modelos">
          {#each MORE_MODELS as m (m.id)}
            <button type="button" role="menuitemradio" aria-checked={model === m.id} class="item" disabled={locked} onclick={() => pickModel(m.id)}>
              <span>{m.label}</span>
              <span class="grow"></span>
              {#if model === m.id}<svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--accent-text)" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{/if}
            </button>
          {/each}
        </div>
      {/if}

      <div class="menu" role="menu" aria-label="Modelo">
        {#if locked}<p class="hint">O modelo é definido pelo agente desta conversa.</p>{/if}
        {#each MODELS as m (m.id)}
          <button type="button" role="menuitemradio" aria-checked={model === m.id} class="item" disabled={locked} onclick={() => pickModel(m.id)}>
            <span>{m.label}</span>
            <span class="grow"></span>
            {#if model === m.id}<svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--accent-text)" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>{/if}
          </button>
        {/each}
        <hr />
        <button type="button" role="menuitem" aria-haspopup="menu" aria-expanded={submenu === "effort"} class="item" class:on={submenu === "effort"} onclick={() => (submenu = submenu === "effort" ? null : "effort")}>
          <span>Esforço</span>
          <span class="grow"></span>
          <span class="muted">{effortLabel}</span>
          <svg aria-hidden="true" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M9 6l6 6-6 6" /></svg>
        </button>
        <hr />
        <button type="button" role="menuitem" aria-haspopup="menu" aria-expanded={submenu === "more"} class="item" class:on={submenu === "more"} onclick={() => (submenu = submenu === "more" ? null : "more")}>
          <span>Mais modelos</span>
          <span class="grow"></span>
          <svg aria-hidden="true" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M9 6l6 6-6 6" /></svg>
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .anchor { position: relative; }
  .trigger { display: flex; align-items: center; gap: 7px; min-height: 28px; padding: 0 8px; border: 0; border-radius: 7px; background: transparent; font-size: 12.5px; color: var(--text); }
  .trigger:hover, .trigger.on { background: var(--hover); }
  .popover { position: absolute; right: 0; bottom: calc(100% + 8px); z-index: 20; display: flex; align-items: flex-end; gap: 6px; }
  .menu { width: 250px; padding: 6px; border: 1px solid var(--border-strong); border-radius: 12px; background: var(--surface-2); box-shadow: 0 16px 40px rgba(0, 0, 0, 0.5); display: flex; flex-direction: column; gap: 1px; }
  .side { width: 270px; }
  .hint { padding: 6px 10px 8px; font-size: 12px; line-height: 1.45; color: var(--muted); }
  .item { display: flex; align-items: center; gap: 8px; min-height: 32px; padding: 0 10px; border: 0; border-radius: 7px; background: transparent; text-align: left; color: var(--text); }
  .item:hover:not(:disabled), .item.on { background: var(--hover); }
  .grow { flex: 1; }
  .tag { padding: 0 7px; border: 1px solid var(--border-strong); border-radius: 999px; font-size: 11px; color: var(--text-2); }
  .tag.warn { border-color: transparent; background: var(--warn-bg); color: var(--warn-text); }
  hr { width: 100%; height: 1px; margin: 4px 0; border: 0; background: var(--border-strong); }
</style>
