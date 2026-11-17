<script lang="ts">
  import { onMount } from "svelte";
  import { pastedImageFiles, readPastedImage, type PreviewImage } from "../paste";
  import { api, errorMessage } from "../api";
  import PipelineRunView from "../components/pipeline/PipelineRunView.svelte";
  import PipelineModelsCard from "../components/pipeline/PipelineModelsCard.svelte";
  import { formatWhen } from "../format";
  import { RUN_STATUS } from "../pipeline";
  import { dayClock } from "../state/clock.svelte";
  import { projectName, projects } from "../state/projects.svelte";
  import { loadPipelines, openPipeline, pipelines } from "../state/pipelines.svelte";
  import { STATION } from "../pipeline";
  import { agentName } from "../state/agents.svelte";
  import type { PastedImage, PipelineConfig, PipelineRole, View } from "../types";

  interface Props {
    onnavigate: (view: View) => void;
  }
  let { onnavigate }: Props = $props();

  let projectId = $state("");
  let title = $state("");
  let request = $state("");
  let budget = $state(90);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let config = $state<PipelineConfig | null>(null);
  /** Imagens coladas na descrição, com a miniatura (data URL). */
  let images = $state<PreviewImage[]>([]);
  const MAX_IMAGES = 8;
  const MAX_IMAGE_BYTES = 5 * 1024 * 1024;

  /** Imagens coladas no campo viram anexos do pedido, com miniatura. */
  async function onPaste(event: ClipboardEvent) {
    const files = pastedImageFiles(event);
    if (!files.length) return;
    error = null;
    for (const file of files) {
      if (images.length >= MAX_IMAGES) {
        error = `Cole no máximo ${MAX_IMAGES} imagens.`;
        break;
      }
      if (file.size > MAX_IMAGE_BYTES) {
        error = "Cada imagem pode ter até 5 MB.";
        continue;
      }
      images = [...images, await readPastedImage(file)];
    }
  }

  const withFolder = $derived(projects.list.filter((p) => p.folder));
  const project = $derived(withFolder.find((p) => p.id === projectId));

  onMount(() => {
    void loadPipelines();
  });

  // Projeto padrão: o primeiro com pasta.
  $effect(() => {
    if (!projectId && withFolder.length) projectId = withFolder[0].id;
  });

  // Configuração da esteira no projeto escolhido, para mostrar no formulário.
  $effect(() => {
    const id = projectId;
    config = null;
    // Sem a configuração o formulário só deixa de mostrar o resumo; a execução lê a dela no servidor.
    if (id) api.pipelineConfig(id).then((c) => id === projectId && (config = c), () => {});
  });
  const assigned = $derived(
    config ? (Object.entries(config.agents) as [PipelineRole, string][]).map(([role, id]) => `${STATION[role].name}: ${agentName(id) ?? "?"}`) : [],
  );

  // Rascunho vindo de uma execução que precisava de mais informação.
  $effect(() => {
    const draft = pipelines.draft;
    if (!draft) return;
    projectId = draft.projectId;
    title = draft.title;
    request = draft.request;
    images = draft.images.map((i) => ({ ...i, preview: `data:${i.media_type};base64,${i.data}` }));
    pipelines.draft = null;
  });

  async function start(event: SubmitEvent) {
    event.preventDefault();
    error = null;
    busy = true;
    try {
      const run = await api.startPipeline({
        projectId,
        // A Triagem procura a task no Jira a partir da descrição.
        jiraKey: null,
        title: title.trim(),
        request: request.trim(),
        budgetMinutes: Number(budget) || null,
        images: images.map(({ name, media_type, data }) => ({ name, media_type, data })),
      });
      title = request = "";
      images = [];
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
            Implementador, Revisor, Testador, Code review (com a sua escolha do que corrigir) e Relator. Cada etapa roda numa sessão própria, com a troca de contas de sempre.
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
            <label><span>Projeto</span>
              <select class="input" bind:value={projectId}>
                {#each withFolder as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
              </select>
            </label>
            <label><span>Orçamento (minutos)</span>
              <input class="input" type="number" min="10" step="10" bind:value={budget} />
            </label>
          </div>
          <label><span>Título <span class="opt">(opcional: sem ele, vale a primeira linha da descrição)</span></span>
            <input class="input" bind:value={title} placeholder="Ex.: Exportar conversa em Markdown" />
          </label>
          <label><span>O que fazer</span>
            <textarea class="input" rows="4" bind:value={request} onpaste={onPaste}
              placeholder="Descreva o problema ou a funcionalidade, com o que conta como pronto. Cole imagens aqui (prints de tela, layouts)."></textarea>
          </label>
          {#if images.length}
            <div class="thumbs" role="list" aria-label="Imagens coladas">
              {#each images as image, i (i)}
                <span class="thumb" role="listitem" title={image.name}>
                  <img src={image.preview} alt={image.name} />
                  <button type="button" aria-label={`Remover a imagem ${image.name}`} title={`Remover a imagem ${image.name}`} onclick={() => (images = images.filter((_, j) => j !== i))}>×</button>
                </span>
              {/each}
            </div>
            <p class="muted small">Triagem, Planejador e Implementador recebem as imagens; o Revisor vê só o diff.</p>
          {/if}
          <p class="muted small">
            {config?.skipped.includes("triage")
              ? "A Triagem está desligada neste projeto: a esteira segue só com a descrição, sem procurar a task no Jira."
              : "A Triagem procura no Jira a task que corresponde à descrição e a assume; o Relator comenta nela no fim. Sem task correspondente, segue só com a descrição."}
          </p>
          {#if project}
            <p class="mode" class:warn={!project.versioned}>
              {#if project.versioned}
                Projeto com versionamento: os bots trabalham numa cópia isolada (git worktree), num ramo novo, sem mexer na pasta principal.
              {:else}
                Projeto sem versionamento: os bots alteram os arquivos direto em <code>{project.folder}</code>. Isso se muda na tela do projeto.
              {/if}
            </p>
          {/if}
          {#if config && (config.skipped.length || assigned.length)}
            <p class="muted small">
              {#if config.skipped.length}Desligadas neste projeto: {config.skipped.map((r) => STATION[r].name).join(", ")}.{/if}
              {#if assigned.length}Agentes: {assigned.join(" · ")}.{/if}
              <button type="button" class="link-btn" onclick={() => { projects.selectedId = projectId; onnavigate("project"); }}>Mudar no projeto</button>
            </p>
          {/if}
          <p class="muted small">O texto da task é tratado como dado, não como instrução. Nenhum bot faz commit nem push: o commit fica com você no fim.</p>
          <div class="row">
            <button type="submit" class="btn primary" disabled={busy || !projectId || !request.trim()}>Iniciar a esteira</button>
          </div>
        {/if}
      </form>

      <PipelineModelsCard />

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
  .link-btn { border: 0; background: none; padding: 0; color: var(--accent-text); font-size: inherit; text-decoration: underline; text-underline-offset: 2px; }
  .thumbs { display: flex; flex-wrap: wrap; gap: 8px; }
  .thumb { position: relative; width: 72px; height: 72px; border: 1px solid var(--border-strong); border-radius: 8px; overflow: hidden; background: var(--bg); }
  .thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .thumb button { position: absolute; top: 3px; right: 3px; width: 18px; height: 18px; padding: 0; border: 0; border-radius: 50%; background: rgba(0, 0, 0, 0.7); color: #fff; font-size: 13px; line-height: 1; }
  .empty { padding: 14px 20px; border-top: 1px solid var(--hairline); }
</style>
