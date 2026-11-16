<script lang="ts">
  import { setPipelineView, ui, type PipelineViewMode } from "../state/ui.svelte";

  const PIPELINE_VIEWS: { id: PipelineViewMode; title: string; description: string }[] = [
    { id: "track", title: "Esteira", description: "Cartões em trilha, com as voltas entre as estações e o trabalho passando de uma para a outra." },
    { id: "office", title: "Escritório", description: "Um escritório em pixel art: cada bot na sua mesa, andando para entregar ou devolver o trabalho." },
  ];
</script>

<div class="page">
  <div class="page-inner">
    <header class="page-head">
      <div>
        <h1>Configurações</h1>
        <p>Preferências do Lume neste computador.</p>
      </div>
    </header>

    <section class="card panel" aria-labelledby="pipeline-view-title">
      <h2 class="section-title" id="pipeline-view-title">Esteira de bots</h2>
      <p class="muted small">Como a execução aberta aparece. O cabeçalho, a aprovação, o registro e o painel de cada etapa são os mesmos nas duas.</p>
      <div class="choices" role="radiogroup" aria-label="Visualização da execução">
        {#each PIPELINE_VIEWS as option (option.id)}
          <button
            type="button"
            role="radio"
            aria-checked={ui.pipelineView === option.id}
            class="choice"
            class:on={ui.pipelineView === option.id}
            onclick={() => void setPipelineView(option.id)}
          >
            <span class="preview {option.id}" aria-hidden="true">
              {#if option.id === "track"}
                <i></i><b></b><i class="lit"></i><b></b><i></i>
              {:else}
                <i></i><i class="lit"></i><i></i><em></em>
              {/if}
            </span>
            <span class="title">{option.title}</span>
            <span class="muted small">{option.description}</span>
          </button>
        {/each}
      </div>
    </section>
  </div>
</div>

<style>
  .panel { display: flex; flex-direction: column; gap: 12px; }
  .choices { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; }
  .choice {
    display: flex; flex-direction: column; gap: 6px; text-align: left; padding: 12px;
    background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--radius);
    color: var(--text); font: inherit; cursor: pointer;
  }
  .choice:hover { border-color: var(--border-strong); }
  .choice.on { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent) inset; }
  .choice:focus-visible { outline: 2px solid var(--accent-text); outline-offset: 2px; }
  .title { font-weight: 600; }

  .preview {
    position: relative; height: 56px; border-radius: 6px; background: var(--bg);
    display: flex; align-items: center; justify-content: center; gap: 6px; margin-bottom: 4px;
  }
  .preview.track i { width: 22px; height: 16px; border-radius: 4px; border: 1px solid var(--border-strong); background: var(--surface); }
  .preview.track b { width: 14px; border-top: 2px dashed var(--faint); }
  .preview.track i.lit { border-color: var(--accent); box-shadow: 0 0 6px var(--accent); }
  .preview.office { gap: 14px; image-rendering: pixelated; background: repeating-linear-gradient(90deg, #15171b 0 8px, #181a1f 8px 16px); }
  .preview.office i { width: 18px; height: 10px; background: #6b4a2f; box-shadow: 0 -6px 0 -2px var(--faint); }
  .preview.office i.lit { box-shadow: 0 -6px 0 -2px var(--accent-text); }
  .preview.office em { position: absolute; left: 50%; bottom: 8px; width: 6px; height: 8px; background: var(--accent); }
</style>
