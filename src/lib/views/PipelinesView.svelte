<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage } from "../api";
  import PipelineRunView from "../components/pipeline/PipelineRunView.svelte";
  import { formatWhen } from "../format";
  import { RUN_STATUS } from "../pipeline";
  import { dayClock } from "../state/clock.svelte";
  import { projectName, projects } from "../state/projects.svelte";
  import { loadPipelines, openPipeline, pipelines } from "../state/pipelines.svelte";
  import type { View } from "../types";

  interface Props {
    onnavigate: (view: View) => void;
  }
  let { onnavigate }: Props = $props();

  let projectId = $state("");
  let jiraKey = $state("");
  let title = $state("");
  let request = $state("");
  let budget = $state(90);
  let error = $state<string | null>(null);
  let busy = $state(false);

  const withFolder = $derived(projects.list.filter((p) => p.folder));
  const project = $derived(withFolder.find((p) => p.id === projectId));

  onMount(() => {
    void loadPipelines();
  });

  // Projeto padrão: o primeiro com pasta.
  $effect(() => {
    if (!projectId && withFolder.length) projectId = withFolder[0].id;
  });

  // Rascunho vindo de uma execução que precisava de mais informação.
  $effect(() => {
    const draft = pipelines.draft;
    if (!draft) return;
    projectId = draft.projectId;
    jiraKey = draft.jiraKey;
    title = draft.title;
    request = draft.request;
    pipelines.draft = null;
  });

  async function start(event: SubmitEvent) {
    event.preventDefault();
    error = null;
    busy = true;
    try {
      const run = await api.startPipeline({
        projectId,
        jiraKey: jiraKey.trim() || null,
        title: title.trim(),
        request: request.trim(),
        budgetMinutes: Number(budget) || null,
      });
      jiraKey = title = request = "";
      await loadPipelines();
      await openPipeline(run.id);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if pipelines.selectedId && pipelines.detail}
  <PipelineRunView detail={pipelines.detail} {onnavigate} />
{:else}
  <div class="page">
    <div class="page-inner">
      <header class="page-head">
        <div>
          <h1>Esteira de bots</h1>
          <p class="muted">
            Bots com papéis fixos resolvem uma task do Jira ou um pedido seu: Triagem, Planejador, a sua aprovação,
            Implementador, Revisor, Testador e Relator. Cada etapa roda numa sessão própria, com a troca de contas de sempre.
          </p>
        </div>
      </header>

      {#if error}<p class="error-text" role="alert">{error}</p>{/if}

      <form class="card panel" onsubmit={start}>
        <h2 class="section-title">Nova execução</h2>
        {#if withFolder.length === 0}
          <p class="muted small">A esteira trabalha na pasta de um projeto. Abra uma pasta como projeto na barra lateral para começar.</p>
        {:else}
          <div class="grid">
            <label>Projeto
              <select class="input" bind:value={projectId}>
                {#each withFolder as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
              </select>
            </label>
            <label>Task do Jira <span class="opt">(opcional)</span>
              <input class="input mono-in" bind:value={jiraKey} placeholder="Ex.: LUME-142" autocapitalize="characters" />
            </label>
            <label>Orçamento (minutos)
              <input class="input" type="number" min="10" step="10" bind:value={budget} />
            </label>
          </div>
          <label>Título {#if jiraKey.trim()}<span class="opt">(opcional: sem ele, vale o da task)</span>{/if}
            <input class="input" bind:value={title} placeholder="Ex.: Exportar conversa em Markdown" />
          </label>
          <label>O que fazer {#if jiraKey.trim()}<span class="opt">(opcional: observações sobre a task)</span>{/if}
            <textarea class="input" rows="4" bind:value={request} placeholder="Descreva o problema ou a funcionalidade, com o que conta como pronto."></textarea>
          </label>
          {#if project}
            <p class="mode" class:warn={!project.versioned}>
              {#if project.versioned}
                Projeto com versionamento: os bots trabalham numa cópia isolada (git worktree), num ramo novo, sem mexer na pasta principal.
              {:else}
                Projeto sem versionamento: os bots alteram os arquivos direto em <code>{project.folder}</code>. Isso se muda na tela do projeto.
              {/if}
            </p>
          {/if}
          <p class="muted small">O texto da task é tratado como dado, não como instrução. Nenhum bot faz commit nem push: o commit fica com você no fim.</p>
          <div class="row">
            <button type="submit" class="btn primary" disabled={busy || !projectId || (!jiraKey.trim() && !title.trim() && !request.trim())}>Iniciar a esteira</button>
          </div>
        {/if}
      </form>

      <section class="card" aria-label="Execuções">
        <div class="head"><h2 class="section-title">Execuções</h2></div>
        {#each pipelines.list as run (run.id)}
          {@const s = RUN_STATUS[run.status]}
          <button type="button" class="run" onclick={() => openPipeline(run.id)}>
            <span class="text">
              <span class="r1">
                {#if run.jira_key}<span class="key">{run.jira_key}</span>{/if}
                <strong>{run.title}</strong>
              </span>
              <span class="muted small">{projectName(run.project_id) ?? "?"} · {run.mode === "worktree" ? `ramo ${run.branch}` : "direto na pasta"} · {formatWhen(run.created_at, dayClock.now)}</span>
            </span>
            <span class="pill {s.tone}">{s.label}</span>
          </button>
        {:else}
          <div class="empty muted">Nenhuma execução ainda.</div>
        {/each}
      </section>
    </div>
  </div>
{/if}

<style>
  .panel { padding: 20px; display: flex; flex-direction: column; gap: 12px; }
  .panel label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; font-weight: 500; }
  .panel .input { font-weight: 400; }
  .panel textarea { padding: 8px 12px; resize: vertical; }
  .opt { font-weight: 400; color: var(--muted); }
  .mono-in { font-family: var(--mono); font-size: 12.5px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; }
  .mode { padding: 8px 12px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface); font-size: 12.5px; color: var(--text-2); }
  .mode.warn { border-color: rgba(217, 164, 65, 0.4); background: var(--warn-bg); color: var(--warn-text); }
  .mode code { font-family: var(--mono); font-size: 12px; }
  .head { padding: 14px 20px 6px; }
  .run { display: flex; align-items: center; gap: 16px; width: 100%; padding: 12px 20px; border: 0; border-top: 1px solid var(--hairline); background: none; color: inherit; text-align: left; }
  .run:hover { background: var(--hover); }
  .text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .r1 { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .r1 strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }
  .key { flex: none; font-family: var(--mono); font-size: 11.5px; color: var(--accent-text); padding: 0 6px; border-radius: 5px; background: rgba(91, 106, 217, 0.14); border: 1px solid rgba(91, 106, 217, 0.3); }
  .pill { flex: none; display: inline-flex; align-items: center; padding: 2px 9px; border-radius: 999px; font-size: 11.5px; font-weight: 600; background: var(--surface-2); color: var(--text-2); }
  .pill.ok { background: var(--ok-bg); color: var(--ok-text); }
  .pill.warn { background: var(--warn-bg); color: var(--warn-text); }
  .pill.acc { background: rgba(91, 106, 217, 0.16); color: var(--accent-text); }
  .pill.bad { background: var(--danger-bg); color: var(--danger-text); }
  .empty { padding: 14px 20px; border-top: 1px solid var(--hairline); }
</style>
