<script lang="ts">
  import { onMount } from "svelte";
  import { addPastedImages, type PreviewImage } from "../paste";
  import { api, errorMessage } from "../api";
  import PipelineRunView from "../components/pipeline/PipelineRunView.svelte";
  import PipelineModelsCard from "../components/pipeline/PipelineModelsCard.svelte";
  import OfficeLooksCard from "../components/pipeline/OfficeLooksCard.svelte";
  import JiraTaskPicker, { ago, typeTone, type PickedTask } from "../components/pipeline/JiraTaskPicker.svelte";
  import { jira } from "../state/jira.svelte";
  import { activeAccount } from "../state/accounts.svelte";
  import { formatWhen } from "../format";
  import { RUN_STATUS } from "../pipeline";
  import { dayClock } from "../state/clock.svelte";
  import { projectName, projects } from "../state/projects.svelte";
  import { loadPipelines, openPipeline, pipelines } from "../state/pipelines.svelte";
  import { STATION } from "../pipeline";
  import { agentName } from "../state/agents.svelte";
  import Toggle from "../components/Toggle.svelte";
  import type { GitInfo, PastedImage, PipelineConfig, PipelineRole, View } from "../types";

  interface Props {
    onnavigate: (view: View) => void;
  }
  let { onnavigate }: Props = $props();

  let projectId = $state("");
  let title = $state("");
  let request = $state("");
  let budget = $state(90);
  /** Começar pelo design no Claude Design (desligado por padrão, por execução). */
  let design = $state(false);
  /** Sem task escolhida, o Relator cria uma no Jira no fim (ligado por padrão, por execução). */
  let createJira = $state(true);
  /** Task do Jira escolhida; sem ela a esteira segue só com a descrição e não consulta o Jira. */
  let task = $state<PickedTask | null>(null);
  let picking = $state(false);
  /** Dados da task escolhida: os da escolha ou, para uma chave digitada, os da lista se ela aparecer depois. */
  const taskIssue = $derived(task ? (task.issue ?? jira.issues.find((i) => i.key === task!.key) ?? null) : null);

  /** Último título posto pela task: trocar ou tirar a task troca ou limpa só esse, não um digitado. */
  let autoTitle = "";
  function pick(picked: PickedTask) {
    task = picked;
    picking = false;
    briefError = null;
    const summary = picked.issue?.summary ?? "";
    if (!title.trim() || title === autoTitle) title = autoTitle = summary;
  }
  function unpick() {
    task = null;
    briefError = null;
    if (title === autoTitle) title = autoTitle = "";
  }

  /** Task sendo lida no Jira para escrever o que fazer (vale só enquanto ela segue escolhida). */
  let briefingKey = $state<string | null>(null);
  const briefing = $derived(!!task && briefingKey === task.key);
  /** Último texto gerado: o que você escreveu além dele não se perde ao gerar de novo. */
  let autoRequest = "";
  let briefError = $state<string | null>(null);
  /** Lê a task (descrição, critérios, comentários, anexos) e escreve o que fazer no campo. */
  async function brief() {
    if (!task || briefing) return;
    const key = task.key;
    briefingKey = key;
    briefError = null;
    try {
      const result = await api.briefJiraTask(key, task.site);
      if (task?.key !== key) return;
      const mine = autoRequest && request.startsWith(autoRequest) ? request.slice(autoRequest.length).trim() : request.trim();
      autoRequest = result.request;
      request = mine ? `${autoRequest}\n\n${mine}` : autoRequest;
      if (result.title && !title.trim()) title = autoTitle = result.title;
    } catch (e) {
      if (task?.key === key) briefError = errorMessage(e);
    } finally {
      if (briefingKey === key) briefingKey = null;
    }
  }
  let error = $state<string | null>(null);
  let busy = $state(false);
  let config = $state<PipelineConfig | null>(null);
  /** Git da pasta do projeto escolhido; nulo enquanto carrega. */
  let git = $state<GitInfo | null>(null);
  /** Imagens coladas na descrição, com a miniatura (data URL). */
  let images = $state<PreviewImage[]>([]);

  /** Imagens coladas no campo viram anexos do pedido, com miniatura. */
  async function onPaste(event: ClipboardEvent) {
    const added = await addPastedImages(event, images);
    if (added) ({ images, error } = added);
  }

  const withFolder = $derived(projects.list.filter((p) => p.folder));
  const project = $derived(withFolder.find((p) => p.id === projectId));

  onMount(() => {
    void loadPipelines();
    // A lista guardada da conta aparece na hora; só a primeira vez consulta o Jira por trás, para
    // estar pronta ao abrir o seletor (que atualiza a lista velha).
    void jira.prefetch(activeAccount()?.id ?? null);
  });

  // Projeto padrão: o primeiro com pasta.
  $effect(() => {
    if (!projectId && withFolder.length) projectId = withFolder[0].id;
  });

  // Configuração da esteira no projeto escolhido, para mostrar no formulário.
  $effect(() => {
    const id = projectId;
    config = null;
    git = null;
    // Sem a configuração o formulário só deixa de mostrar o resumo; a execução lê a dela no servidor.
    if (id) api.pipelineConfig(id).then((c) => id === projectId && (config = c), () => {});
    if (id) api.projectGitInfo(id).then((g) => id === projectId && (git = g), () => {});
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
    task = draft.task;
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
        // Só a task escolhida: sem ela, a Triagem não consulta o Jira.
        jiraKey: task?.key ?? null,
        jiraSite: task?.site ?? null,
        title: title.trim(),
        request: request.trim(),
        budgetMinutes: Number(budget) || null,
        images: images.map(({ name, media_type, data }) => ({ name, media_type, data })),
        design,
        createJira,
      });
      title = request = autoTitle = autoRequest = "";
      images = [];
      design = false;
      createJira = true;
      task = null;
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
          <div class="field">
            <span class="field-label">Task do Jira <span class="opt">(opcional)</span></span>
            {#if task}
              {@const issue = taskIssue}
              <div class="task-card">
                <span class="ttype {typeTone(issue?.issue_type ?? null)}" aria-hidden="true"></span>
                <div class="ttext">
                  <div class="tline">
                    <span class="key">{task.key}</span>
                    {#if issue}<span class="tstatus {issue.category}">{issue.status}</span>{/if}
                  </div>
                  <strong class="tsummary">{issue?.summary ?? "Task digitada: a Triagem lê os detalhes no Jira."}</strong>
                  {#if issue}
                    <span class="muted small">
                      {issue.assignee ? `Responsável: ${issue.assignee}` : "Sem responsável"}{issue.project ? ` · ${issue.project}` : ""}{issue.updated ? ` · atualizada ${ago(issue.updated)}` : ""}
                    </span>
                  {/if}
                </div>
                <div class="tactions">
                  <button type="button" class="btn small" onclick={() => (picking = true)}>Trocar</button>
                  <button type="button" class="btn small icon" aria-label="Seguir sem task" title="Seguir sem task" onclick={unpick}>
                    <svg aria-hidden="true" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
                  </button>
                </div>
              </div>
            {:else}
              <button type="button" class="task-empty" onclick={() => (picking = true)}>
                <span class="plus" aria-hidden="true">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
                </span>
                <span class="ttext">
                  <strong>Escolher uma task do Jira</strong>
                  <span class="muted small">
                    {#if jira.loading && !jira.issues.length}Carregando as tasks do conector…
                    {:else if jira.issues.length}{jira.issues.length} tasks {jira.currentColumn ? `em ${jira.currentColumn.name}` : "disponíveis"}. Sem task, a esteira segue só com a descrição.
                    {:else}Sem task, a esteira segue só com a descrição e não consulta o Jira.{/if}
                  </span>
                </span>
                <svg class="chev" aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 6 6 6-6 6" /></svg>
              </button>
            {/if}
          </div>
          <label><span>Título <span class="opt">{task ? "(opcional: sem ele, vale a chave da task)" : "(opcional: sem ele, vale a primeira linha da descrição)"}</span></span>
            <input class="input" bind:value={title} placeholder={taskIssue?.summary ?? "Ex.: Exportar conversa em Markdown"} />
          </label>
          <div class="field">
            <div class="field-head">
              <label class="field-label" for="pipeline-request">O que fazer{#if task}<span class="opt"> (opcional: a Triagem também lê a task)</span>{/if}</label>
              {#if task}
                <button type="button" class="btn small gen" onclick={brief} disabled={briefing}
                  title={`Lê a ${task.key} no Jira (descrição, critérios de aceite, comentários e anexos) e escreve o que fazer`}>
                  {#if briefing}<span class="spinner" aria-hidden="true"></span>Lendo a {task.key}…{:else}Gerar a partir da task{/if}
                </button>
              {/if}
            </div>
            <textarea id="pipeline-request" class="input" rows={request.length > 300 ? 10 : 4} bind:value={request} onpaste={onPaste} disabled={briefing}
              placeholder={task
                ? "Use \"Gerar a partir da task\" para escrever o pedido com o que está na task, ou escreva algo além dela: restrições, por onde começar, o que não mexer. Cole imagens aqui."
                : "Descreva o problema ou a funcionalidade, com o que conta como pronto. Cole imagens aqui (prints de tela, layouts)."}></textarea>
            {#if briefError}<p class="error-text small" role="alert">{briefError}</p>{/if}
          </div>
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
            {#if task}
              {config?.skipped.includes("triage")
                ? `A Triagem está desligada neste projeto: o Planejador lê a ${task.key} no Jira e a move para Em andamento, se estiver selecionada para desenvolvimento; no fim, o Relator escreve a Solução na descrição dela e a move para Teste.`
                : `A Triagem lê a ${task.key} no Jira (descrição, critérios de aceite e comentários) e a move para Em andamento, se estiver selecionada para desenvolvimento; no fim, o Relator escreve a Solução na descrição dela e a move para Teste.`}
            {:else}
              Sem task escolhida, nenhum bot procura task no Jira: a esteira segue só com a descrição.
              {#if config?.skipped.includes("reporter")}
                O Relator está desligado neste projeto: nenhuma task é criada.
              {:else if !createJira}
                Nenhuma task é criada no Jira.
              {:else if config?.jira_template || config?.default_jira_template}
                No fim, o Relator cria uma task no Jira pelo modelo {config.jira_template ? "do projeto" : "padrão"} e mostra o link.
              {:else if config}
                No fim, o Relator cria uma task no Jira usando como modelo a sua task mais recente (sem modelo no projeto nem em Configurações) e mostra o link.
              {/if}
            {/if}
          </p>
          {#if !task && !config?.skipped.includes("reporter")}
            <label class="create-jira">
              <input type="checkbox" bind:checked={createJira} />
              Criar task no Jira no fim
            </label>
          {/if}
          {#if project && git}
            <p class="mode" class:warn={!git.is_repo}>
              {#if !git.is_repo}
                A pasta não é um repositório git: os bots alteram os arquivos direto em <code>{project.folder}</code>, sem como desfazer pelo git.
              {:else if project.isolate}
                Repositório git: os bots trabalham numa cópia isolada (worktree), num ramo novo, sem mexer na pasta principal. No fim, você escolhe entre commit na main, num ramo ou pull request.
              {:else}
                Repositório git: os bots alteram os arquivos direto em <code>{project.folder}</code>{git.branch ? `, no ramo ${git.branch}` : ""}. No fim, você escolhe entre commit no ramo atual, num ramo novo ou pull request. A cópia isolada se liga na tela do projeto.
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
          <div class="design-opt">
            <div>
              <strong>Fazer design no Claude Design</strong>
              <span class="muted small">Antes do plano, o Designer desenha a tela no Claude Design e você abre o design, aprova ou pede ajustes. O plano segue o design aprovado.</span>
            </div>
            <Toggle checked={design} label={design ? "Não fazer design" : "Fazer design no Claude Design"} onchange={(v) => (design = v)} />
          </div>
          <p class="muted small">O texto da task é tratado como dado, não como instrução. Nenhum bot faz commit nem push: o commit fica com você no fim.</p>
          <div class="row">
            <button type="submit" class="btn primary" disabled={busy || !projectId || (!task && !request.trim())}>Iniciar a esteira</button>
          </div>
        {/if}
      </form>

      <JiraTaskPicker
        open={picking}
        selected={task?.key ?? null}
        onpick={pick}
        onclear={() => { task = null; picking = false; }}
        onclose={() => (picking = false)}
        onconnectors={() => { picking = false; onnavigate("connectors"); }}
      />

      <PipelineModelsCard />

      <OfficeLooksCard />

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
  .create-jira { display: flex; align-items: center; gap: 8px; font-size: 13px; cursor: pointer; }
  .design-opt { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 10px 12px; border: 1px solid var(--border); border-radius: 10px; }
  .design-opt > div { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .panel { padding: 20px; display: flex; flex-direction: column; gap: 12px; }
  .panel label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; font-weight: 500; }
  .panel .input { font-weight: 400; }
  .panel textarea { padding: 8px 12px; resize: vertical; }
  .opt { font-weight: 400; color: var(--muted); }
  .field { display: flex; flex-direction: column; gap: 4px; }
  .field-label { font-size: 13px; font-weight: 500; }
  .field-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 26px; }
  .gen { display: inline-flex; align-items: center; gap: 6px; flex: none; }
  .spinner { width: 11px; height: 11px; border: 2px solid var(--border-strong); border-top-color: var(--accent); border-radius: 50%; animation: brief-spin 0.8s linear infinite; }
  @keyframes brief-spin { to { transform: rotate(360deg); } }
  .field textarea { padding: 8px 12px; resize: vertical; font-weight: 400; }
  .task-empty { display: flex; align-items: center; gap: 12px; width: 100%; padding: 12px 14px; border: 1px dashed var(--border-strong); border-radius: 10px; background: none; color: inherit; text-align: left; transition: border-color 0.15s, background 0.15s; }
  .task-empty:hover { border-color: var(--accent); background: rgba(91, 106, 217, 0.06); }
  .task-empty .plus { flex: none; width: 30px; height: 30px; display: grid; place-items: center; border-radius: 8px; background: rgba(91, 106, 217, 0.16); color: var(--accent-text); }
  .task-empty strong { font-weight: 500; font-size: 13.5px; }
  .ttext { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .chev { flex: none; color: var(--muted); }
  .task-card { display: flex; align-items: flex-start; gap: 12px; padding: 12px 14px; border: 1px solid rgba(91, 106, 217, 0.4); border-radius: 10px; background: rgba(91, 106, 217, 0.07); }
  .ttype { flex: none; width: 4px; align-self: stretch; border-radius: 4px; background: #4b8fe0; }
  .ttype.bug { background: #d9584f; }
  .ttype.story { background: #5aa86b; }
  .ttype.epic { background: #8e6ad6; }
  .ttype.sub { background: #4aa3b8; }
  .tline { display: flex; align-items: center; gap: 8px; }
  .tsummary { font-weight: 500; font-size: 13.5px; line-height: 1.35; }
  .tstatus { padding: 0 8px; border-radius: 999px; font-size: 10.5px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.02em; background: var(--surface-2); color: var(--text-2); }
  .tstatus.doing { background: rgba(91, 106, 217, 0.18); color: var(--accent-text); }
  .tstatus.done { background: var(--ok-bg); color: var(--ok-text); }
  .tactions { flex: none; display: flex; gap: 6px; }
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
  .warn-text { color: var(--warn-text); }
</style>
