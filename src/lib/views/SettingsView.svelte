<script lang="ts">
  import { api, errorMessage } from "../api";
  import { setPipelineView, ui, type PipelineViewMode } from "../state/ui.svelte";
  import { checkForUpdate, installUpdate, updates } from "../state/updates.svelte";

  /** Situação da atualização, em uma frase. */
  const updateText = $derived.by(() => {
    if (updates.progress !== null) return `Baixando a versão ${updates.found?.version}${updates.progress ? ` (${updates.progress}%)` : ""}…`;
    if (updates.checking) return "Procurando versão nova…";
    if (updates.found) return `A versão ${updates.found.version} está disponível.`;
    if (updates.checked) return `Você está na versão mais recente. Última busca às ${new Date(updates.checked).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}.`;
    return "O Lume procura versão nova sozinho ao abrir e de hora em hora.";
  });

  const PIPELINE_VIEWS: { id: PipelineViewMode; title: string; description: string }[] = [
    { id: "track", title: "Esteira", description: "Cartões em trilha, com as voltas entre as estações e o trabalho passando de uma para a outra." },
    { id: "office", title: "Escritório", description: "Um escritório em pixel art: cada bot na sua mesa, andando para entregar ou devolver o trabalho." },
  ];

  /** Modelo de task do Jira de todos os projetos: o que está salvo e o texto do campo, salvo ao sair dele. */
  let savedTemplate = $state<string | null>(null);
  let template = $state("");
  let templateLoaded = $state(false);
  let templateError = $state<string | null>(null);

  api.pipelineDefaultTemplate().then(
    (t) => {
      savedTemplate = t;
      template = t ?? "";
      templateLoaded = true;
    },
    (e) => (templateError = errorMessage(e)),
  );

  function saveTemplate() {
    if (!templateLoaded) return;
    const value = template.trim() || null;
    if (value === savedTemplate) return;
    templateError = null;
    api.savePipelineDefaultTemplate(value).then(
      (saved) => {
        savedTemplate = saved;
        template = saved ?? "";
      },
      (e) => (templateError = errorMessage(e)),
    );
  }
</script>

<div class="page">
  <div class="page-inner">
    <header class="page-head">
      <div>
        <h1>Configurações</h1>
        <p>Preferências do Lume neste computador.</p>
      </div>
    </header>

    <section class="card panel" aria-labelledby="updates-title">
      <h2 class="section-title" id="updates-title">Atualizações</h2>
      <div class="update-row">
        <div class="grow">
          <span class="title">Lume {updates.current ?? ""}</span>
          <p class="muted small" role="status">{updateText}</p>
        </div>
        {#if updates.found}
          <button type="button" class="btn primary" disabled={updates.progress !== null} onclick={installUpdate}>Atualizar e reabrir</button>
        {:else}
          <button type="button" class="btn" disabled={updates.checking} onclick={() => void checkForUpdate(true)}>
            {updates.checking ? "Procurando…" : "Procurar atualização"}
          </button>
        {/if}
      </div>
      {#if updates.checkError}<p class="error-text" role="alert">Não deu para procurar: {updates.checkError}</p>{/if}
      {#if updates.installError}<p class="error-text" role="alert">Não deu para atualizar: {updates.installError}</p>{/if}
    </section>

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
      <div class="template">
        <label class="grow" for="default-jira-template">
          <span class="title">Modelo padrão de task do Jira</span>
          <span class="muted small">
            Vale para todos os projetos sem modelo próprio (o de cada projeto fica na tela do projeto). Quando a execução começa sem task
            escolhida, o Relator cria uma no fim copiando o projeto, o tipo, os campos e a estrutura da descrição deste modelo. Vazio no
            projeto e aqui, o modelo é a sua task mais recente no Jira. Este campo fica salvo só neste computador.
          </span>
        </label>
        <input id="default-jira-template" class="input tpl" bind:value={template} placeholder="https://empresa.atlassian.net/browse/EI-5198"
          disabled={!templateLoaded} onblur={saveTemplate} onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()} />
      </div>
      {#if templateError}<p class="error-text" role="alert">{templateError}</p>{/if}
    </section>
  </div>
</div>

<style>
  .panel { display: flex; flex-direction: column; gap: 12px; padding: 16px; }
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
  .update-row { display: flex; align-items: center; gap: 12px; }
  .update-row p { margin: 2px 0 0; }
  .template { display: flex; align-items: center; gap: 12px; padding-top: 12px; border-top: 1px solid var(--hairline); }
  .template label { display: flex; flex-direction: column; gap: 2px; }
  .grow { flex: 1; min-width: 0; }
  .tpl { width: 330px; min-height: 34px; font-family: var(--mono); font-size: 12px; }
  @media (max-width: 720px) {
    .template { flex-wrap: wrap; }
    .tpl { width: 100%; }
  }

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
