<script lang="ts">
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, errorMessage } from "../../api";
  import { formatTime, fullDate, renderMarkdown } from "../../format";
  import { accounts } from "../../state/accounts.svelte";
  import { chat, decideFor, openConversation, type DecisionExtras } from "../../state/chat.svelte";
  import PermissionCard from "../PermissionCard.svelte";
  import PlanCard from "../PlanCard.svelte";
  import QuestionCard from "../QuestionCard.svelte";
  import { projectName, projects } from "../../state/projects.svelte";
  import { loadDetail, loadPipelines, openPipeline, pipelines } from "../../state/pipelines.svelte";
  import {
    choiceFindings, clock, designSpec, designUrl, duration, findingsMarkdown, icon, inputSummary, isActive, isMarker, jiraOutcome, isUserStep, isWaitingYou, outputSummary, passesByRole, passLabel, RUN_STATUS,
    stageAccountId, stageLog, stageModelText, stageVisual, STATION, switchedAccount, triageQuestions, VISUAL,
  } from "../../pipeline";
  import { EFFORTS, type PastedImage, type PipelineDetail, type PipelineRole, type ReviewPick, type StageRun, type View } from "../../types";
  import PipelineTrack from "./PipelineTrack.svelte";
  import PipelineOffice from "./PipelineOffice.svelte";
  import ReviewChoiceCard from "./ReviewChoiceCard.svelte";
  import { ui } from "../../state/ui.svelte";

  interface Props {
    detail: PipelineDetail;
    onnavigate: (view: View) => void;
  }
  let { detail, onnavigate }: Props = $props();

  let now = $state(Date.now() / 1000);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let confirmCancel = $state(false);
  let legendOpen = $state(false);
  let adjusting = $state(false);
  let feedback = $state("");
  let commitMessage = $state("");
  let committed = $state(false);
  /** Reabrir uma execução concluída com um ajuste. */
  let reopening = $state(false);
  let adjustment = $state("");
  /** Estação aberta no painel; `follow` acompanha a etapa ativa. */
  let panelRole = $state<PipelineRole | null>(null);
  let panelStageId = $state<string | null>(null);
  let follow = $state(true);
  let entryOpen = $state(false);

  onMount(() => {
    const timer = setInterval(() => (now = Date.now() / 1000), 1000);
    return () => clearInterval(timer);
  });

  const active = $derived(isActive(detail.status));
  const passes = $derived(passesByRole(detail));
  const latest = $derived(detail.stages.at(-1) ?? null);
  const running = $derived(detail.stages.find((s) => s.status === "running" && !isUserStep(s.role)) ?? null);
  const approval = $derived(
    detail.status === "awaiting_approval"
      ? (detail.stages.findLast((s) => (s.role === "approval" || s.role === "design_approval") && s.status === "running") ?? null)
      : null,
  );
  /** A parada é a do design: o cartão mostra o link do Claude Design e a conta que o publicou. */
  const designing = $derived(approval?.role === "design_approval");
  const designLink = $derived(designing && approval ? designUrl(approval.input) : null);
  const designer = $derived(designing ? (detail.stages.findLast((s) => s.role === "designer" && s.status === "done") ?? null) : null);
  const designAccount = $derived.by(() => {
    if (!designer) return null;
    const id = stageAccountId(pipelines.messages[designer.conversation_id] ?? []);
    return accounts.list.find((a) => a.id === id)?.name ?? null;
  });
  /** Parada da sua escolha sobre o code review, e a passagem do Code review que a abriu. */
  const choice = $derived(
    detail.status === "awaiting_choice" ? (detail.stages.findLast((s) => s.role === "review_choice" && s.status === "running") ?? null) : null,
  );
  const choiceReview = $derived(choice ? (detail.stages.findLast((s) => s.role === "code_review" && s.status === "done") ?? null) : null);
  const status = $derived(RUN_STATUS[detail.status]);
  /** Segundos gastos pelos bots, contando a etapa em andamento até agora. */
  const spent = $derived(detail.spent_seconds + (running ? Math.max(0, now - pipelines.fetchedAt / 1000) : 0));
  const elapsed = $derived((active ? now : detail.updated_at) - detail.created_at);
  const budgetPercent = $derived(Math.min(100, (spent / (detail.budget_minutes * 60)) * 100));

  // Acompanha a etapa ativa enquanto o usuário não fixar outra.
  $effect(() => {
    if (follow && latest) {
      panelRole = latest.role;
      panelStageId = latest.id;
    } else if (follow && !latest) {
      panelRole = "triage";
      panelStageId = null;
    }
  });

  // Mensagem de commit sugerida pelo Relator, uma vez só: o usuário pode apagá-la e reescrever.
  let suggested = false;
  $effect(() => {
    if (detail.commit_message && !suggested) {
      suggested = true;
      commitMessage = detail.commit_message;
    }
  });

  function select(role: PipelineRole, stageId?: string) {
    panelRole = role;
    panelStageId = stageId ?? passes[role].at(-1)?.id ?? null;
    follow = !!latest && panelStageId === latest.id;
  }

  const panelStage = $derived(detail.stages.find((s) => s.id === panelStageId) ?? null);
  const panelStation = $derived(panelRole ? STATION[panelRole] : null);
  /** Agente do papel nesta execução: o modelo dele, quando tem, substitui o do papel. */
  const panelAgent = $derived(panelRole ? (detail.agents[panelRole] ?? null) : null);
  const panelMessages = $derived(panelStage?.conversation_id ? (pipelines.messages[panelStage.conversation_id] ?? []) : []);
  const panelAsking = $derived(!!panelStage && panelStage.status === "running" && !!chat.permissions[panelStage.conversation_id]);
  const panelVisual = $derived(panelStage ? stageVisual(panelStage, panelAsking) : null);
  const panelPermission = $derived(panelAsking && panelStage ? chat.permissions[panelStage.conversation_id] : undefined);
  /** Responde ao pedido da etapa direto daqui; se a resposta não chegar, o pedido volta e o erro aparece. */
  async function answerStage(id: string, allow: boolean, always: boolean, extras: DecisionExtras = {}) {
    try {
      await decideFor(id, allow, always, extras);
    } catch (e) {
      error = errorMessage(e);
    }
  }
  const panelLog = $derived(stageLog(panelMessages));
  /** Log da etapa: acompanha a última linha, a não ser que o usuário role para cima para ler. */
  let logEl = $state<HTMLOListElement>();
  let logStuck = $state(true);
  const onLogScroll = () => {
    if (logEl) logStuck = logEl.scrollHeight - logEl.scrollTop - logEl.clientHeight < 24;
  };
  // Outra etapa no painel: volta a acompanhar o fim.
  $effect(() => {
    void panelStageId;
    logStuck = true;
  });
  $effect(() => {
    void panelLog.length;
    void liveStatus;
    if (logEl && logStuck) logEl.scrollTop = logEl.scrollHeight;
  });

  /** Registro das etapas: o mesmo comportamento do log, acompanhando a etapa mais nova. */
  let regEl = $state<HTMLOListElement>();
  let regStuck = $state(true);
  const onRegScroll = () => {
    if (regEl) regStuck = regEl.scrollHeight - regEl.scrollTop - regEl.clientHeight < 24;
  };
  // Outra execução aberta: volta a acompanhar o fim.
  $effect(() => {
    void detail.id;
    regStuck = true;
  });
  $effect(() => {
    // Etapa nova ou a última mudando de situação (o resumo do que entregou aumenta a linha).
    const last = detail.stages.at(-1);
    void detail.stages.length;
    void last?.status;
    void last?.output;
    if (regEl && regStuck) regEl.scrollTop = regEl.scrollHeight;
  });

  /** Modelo da etapa aberta no serviço da conta que a atende. */
  const panelModel = $derived.by(() => {
    if (!panelStage?.conversation_id) return null;
    const live = panelStage.status === "running" ? chat.turns[panelStage.conversation_id]?.account : null;
    const id = stageAccountId(pipelines.messages[panelStage.conversation_id] ?? []);
    const account = accounts.list.find((a) => (live ? a.name === live : a.id === id));
    return stageModelText(detail, panelStage, account?.provider ?? "claude");
  });

  /** Esforço escolhido para o papel aberto nesta execução. */
  const panelEffort = $derived.by(() => {
    const id = panelRole ? detail.efforts[panelRole] : undefined;
    return id ? (EFFORTS.find((e) => e.id === id)?.label.toLowerCase() ?? id) : null;
  });

  function configureAgents() {
    projects.selectedId = detail.project_id;
    onnavigate("project");
  }

  const liveStatus = $derived(panelStage?.status === "running" && panelStage.conversation_id ? chat.turns[panelStage.conversation_id]?.status : null);

  const accountName = (id: string | null) => accounts.list.find((a) => a.id === id)?.name ?? null;
  function stageAccount(stage: StageRun): string {
    if (isUserStep(stage.role)) return "você";
    const live = stage.status === "running" ? chat.turns[stage.conversation_id]?.account : null;
    const name = live ?? accountName(stageAccountId(pipelines.messages[stage.conversation_id] ?? []));
    const swapped = switchedAccount(pipelines.messages[stage.conversation_id] ?? []);
    return name ? (swapped ? `${name} (após troca)` : name) : "—";
  }
  const stageSeconds = (s: StageRun) => (s.finished_at ?? now) - s.started_at;
  const indexOf = (s: StageRun) => passes[s.role].indexOf(s);

  /** O que está acontecendo agora, em uma linha. */
  const nowLine = $derived.by(() => {
    if (detail.status === "awaiting_approval" && designing) return { tone: "you", who: "Aprovação do design", text: "a esteira está parada até você aprovar o design ou pedir ajustes" };
    if (detail.status === "awaiting_approval") return { tone: "you", who: "Sua aprovação", text: "a esteira está parada até você aprovar o plano ou pedir ajustes" };
    if (detail.status === "awaiting_choice") return { tone: "you", who: "Sua escolha", text: "a esteira está parada até você escolher o que corrigir do code review" };
    if (running) {
      const who = STATION[running.role].name;
      if (chat.permissions[running.conversation_id]) return { tone: "you", who, text: "pede sua autorização para continuar" };
      return { tone: "live", who, text: chat.turns[running.conversation_id]?.status ?? "trabalhando" };
    }
    if (detail.status === "running") return { tone: "live", who: "", text: "Preparando a próxima etapa…" };
    if (detail.status === "done") return { tone: "ok", who: "", text: `Esteira concluída em ${clock(detail.spent_seconds)} de trabalho dos bots` };
    return { tone: detail.status === "cancelled" ? "off" : "bad", who: "", text: detail.note ?? status.label };
  });

  async function act(action: () => Promise<void>) {
    error = null;
    busy = true;
    try {
      await action();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  const approve = () => act(async () => {
    await api.decidePipelinePlan(detail.id, null);
    adjusting = false;
    follow = true;
  });
  const requestChanges = () => act(async () => {
    if (!feedback.trim()) return;
    await api.decidePipelinePlan(detail.id, feedback.trim());
    feedback = "";
    adjusting = false;
    follow = true;
  });
  const decideReview = (picks: ReviewPick[], note: string | null) => act(async () => {
    await api.decidePipelineReview(detail.id, picks, note);
    follow = true;
  });
  const cancelRun = () => act(async () => {
    confirmCancel = false;
    await api.cancelPipeline(detail.id);
  });
  const reopen = () => act(async () => {
    if (!adjustment.trim()) return;
    await api.reopenPipeline(detail.id, adjustment.trim());
    adjustment = "";
    reopening = false;
    committed = false;
    follow = true;
  });
  const retry = () => act(async () => {
    await api.retryPipeline(detail.id);
    follow = true;
  });
  const commit = () => act(async () => {
    await api.commitPipeline(detail.id, commitMessage);
    committed = true;
  });
  const remove = () => act(async () => {
    await api.deletePipeline(detail.id);
    await loadPipelines();
    await openPipeline(null);
  });
  /** Imagens do pedido, carregadas uma vez por execução. */
  let images = $state<PastedImage[]>([]);
  // Valores derivados: o `detail` é trocado a cada atualização, mas a busca só se repete
  // quando muda a execução (ou a quantidade de imagens).
  const runId = $derived(detail.id);
  const imageCount = $derived(detail.image_count);
  $effect(() => {
    const id = runId;
    images = [];
    if (imageCount > 0) api.pipelineImages(id).then((list) => id === detail.id && (images = list), () => {});
  });
  /** Imagem ampliada sobre a tela. */
  let zoomed = $state<PastedImage | null>(null);
  const imageSrc = (i: PastedImage) => `data:${i.media_type};base64,${i.data}`;

  /** A task foi criada pelo Relator (a execução começou sem task). */
  const jiraResult = $derived(jiraOutcome(passes.reporter.at(-1)?.output ?? ""));
  const createdTask = $derived(!!detail.jira_key && /TASK_CRIADA:/i.test(passes.reporter.at(-1)?.output ?? ""));

  /** Espaço para responder: cada pergunta da Triagem com a sua linha de resposta. */
  function answersTemplate(): string {
    const questions = triageQuestions(passes.triage.at(-1)?.output ?? "");
    if (!questions.length) return "Respostas às dúvidas da Triagem:\n- ";
    return `Respostas às dúvidas da Triagem:\n\n${questions.map((q, i) => `${i + 1}. ${q}\n   Resposta: `).join("\n\n")}`;
  }

  function complete() {
    // O pedido volta ao formulário com espaço para responder ao que a Triagem achou vago.
    // A task escolhida segue escolhida; o título igual à chave é o padrão e não precisa voltar.
    const key = detail.jira_key;
    pipelines.draft = {
      projectId: detail.project_id,
      title: key && detail.title === key ? "" : detail.title,
      request: `${detail.request}\n\n${answersTemplate()}`.trimStart(),
      images: $state.snapshot(images),
      task: key ? { key, site: detail.jira_url?.split("/browse/")[0] ?? null, issue: null } : null,
    };
    void openPipeline(null);
  }
  async function openStageConversation(conversationId: string) {
    await openConversation(conversationId);
    onnavigate("chat");
  }

  const stationGlyph = (role: PipelineRole, visual: keyof typeof VISUAL) =>
    icon(STATION[role].gate && visual === "waiting" ? "you" : VISUAL[visual].glyph, 9, 2.8);
</script>

<div class="screen">
  <header class="head">
    <div class="ident">
      <div class="l1">
        <button type="button" class="btn icon" aria-label="Voltar à lista de execuções" title="Voltar à lista" onclick={() => openPipeline(null)}>
          <svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 6l-6 6 6 6" /></svg>
        </button>
        {#if detail.jira_key && detail.jira_url}
          <button type="button" class="key link" title={`Abrir ${detail.jira_key} no Jira`} onclick={() => openUrl(detail.jira_url!)}>{detail.jira_key} ↗</button>
        {:else if detail.jira_key}<span class="key">{detail.jira_key}</span>{:else}<span class="key manual">Pedido escrito</span>{/if}
        <h1>{detail.title}</h1>
        <span class="pill {status.tone}">{#if isWaitingYou(detail.status)}<span class="dot warn pulse"></span>{:else if detail.status === "running"}<span class="dot acc"></span>{/if}{status.label}</span>
      </div>
      <div class="meta">
        <span class="m">{@html icon("folder", 13)}Projeto <b>{projectName(detail.project_id) ?? "?"}</b></span>
        {#if detail.mode === "worktree"}
          <span class="m">{@html icon("branch", 13)}Worktree isolada<code>{detail.branch}</code></span>
        {:else}
          <span class="m warn">{@html icon("alert", 13)}Editando direto na pasta · projeto sem versionamento</span>
        {/if}
      </div>
    </div>
    <div class="nums">
      <div class="num"><span class="label">Tempo decorrido</span><span class="value" class:stopped={!active}>{clock(elapsed)}</span></div>
      <div class="num" title="Só o tempo dos bots conta: a espera pelas suas decisões fica de fora.">
        <span class="label">Orçamento</span>
        <span class="value">{clock(spent)} <small>de {detail.budget_minutes} min</small></span>
        <span class="meter" aria-hidden="true"><span style:width={`${budgetPercent.toFixed(1)}%`}></span></span>
      </div>
      <div class="num"><span class="label">Voltas</span><span class="value">{detail.rounds}<small>/{detail.max_rounds}</small></span></div>
      {#if active}
        <div class="cancel">
          <button type="button" class="btn danger" aria-haspopup="dialog" aria-expanded={confirmCancel} onclick={() => (confirmCancel = !confirmCancel)}>Cancelar</button>
          {#if confirmCancel}
            <div class="confirm" role="dialog" aria-label="Confirmar cancelamento">
              <strong>Cancelar a execução?</strong>
              <p>{detail.mode === "worktree" ? "O que já foi alterado fica na worktree, para você conferir ou descartar." : "Sem versionamento, o que já foi alterado continua na pasta de desenvolvimento."}</p>
              <div class="acts">
                <button type="button" class="btn small" onclick={() => (confirmCancel = false)}>Continuar</button>
                <button type="button" class="btn small danger" disabled={busy} onclick={cancelRun}>Cancelar execução</button>
              </div>
            </div>
          {/if}
        </div>
      {:else}
        <button type="button" class="btn" disabled={busy} title="Apaga o registro desta execução; a pasta de trabalho fica" onclick={remove}>Apagar registro</button>
      {/if}
    </div>
  </header>

  {#if error}<p class="error-text" role="alert">{error}</p>{/if}

  <section class="card track" aria-label="Esteira de bots">
    <div class="track-top">
      <div class="now t-{nowLine.tone}">
        <span class="ic">
          {#if nowLine.tone === "live"}<span class="spinner"></span>{:else if nowLine.tone === "you"}<span class="dot warn pulse"></span>{:else if nowLine.tone === "ok"}{@html icon("check", 14, 2.4)}{:else if nowLine.tone === "bad"}{@html icon("x", 14, 2.4)}{:else}<span class="dot"></span>{/if}
        </span>
        {#if nowLine.who}<b>{nowLine.who}</b>{/if}
        <span class="what">{nowLine.text}</span>
      </div>
      <button type="button" class="btn small" title="Agente de cada papel e etapas ligadas, na página do projeto" onclick={configureAgents}>Configurar agentes</button>
      <button type="button" class="btn small" aria-expanded={legendOpen} onclick={() => (legendOpen = !legendOpen)}>Legenda</button>
    </div>
    {#if legendOpen}
      <div class="legend">
        <span class="label">Estados de uma estação</span>
        <ul>
          {#each Object.entries(VISUAL) as [key, v] (key)}
            <li><span class="node mini s-{key}">{@html icon(v.glyph, 9, 2.8)}</span><span><b>{v.label}</b> · {v.desc}</span></li>
          {/each}
        </ul>
        <p class="foot">Caminhos de devolução: você devolve o plano ao Planejador; o Revisor e o Testador devolvem o trabalho ao Implementador, no máximo {detail.max_rounds} vezes. Depois do Code review, você escolhe o que corrigir e o trabalho volta ao Implementador. O losango é a parada em que a decisão é sua.</p>
      </div>
    {/if}
    {#if ui.pipelineView === "office"}
      <PipelineOffice {detail} {now} selected={panelRole} onselect={(role) => select(role)} />
    {:else}
      <PipelineTrack {detail} {now} selected={panelRole} onselect={(role) => select(role)} />
    {/if}
  </section>

  <div class="bottom">
    <div class="main">
      {#if approval}
        <section class="card approval" aria-label={designing ? "Design para aprovação" : "Plano para aprovação"}>
          <div class="ap-top">
            <div class="ap-title">
              <div class="row-w">
                <span class="pill warn"><span class="dot warn pulse"></span>Aguardando aprovação</span>
                {#if designing}
                  <h2>{indexOf(approval) > 0 ? `Design revisado (v${indexOf(approval) + 1}) com o seu ajuste` : "O Designer propõe este design"}</h2>
                {:else}
                  <h2>{indexOf(approval) > 0 ? `Plano revisado (v${indexOf(approval) + 1}) com o seu ajuste` : "O Planejador propõe este plano"}</h2>
                {/if}
              </div>
              {#if designing}
                <p class="note">
                  Abra o design no Claude Design e decida. O plano só começa depois, seguindo o design aprovado.
                  {#if designAccount}<b>Publicado pela conta {designAccount}</b>: o link só abre com essa conta logada no navegador.{/if}
                </p>
              {:else}
                <p class="note">A esteira está parada: nada é alterado antes da sua decisão, e a espera não conta no orçamento.</p>
              {/if}
            </div>
            <div class="row-w">
              {#if designLink}<button type="button" class="btn" title={designLink} onclick={() => openUrl(designLink!)}>Abrir design ↗</button>{/if}
              <button type="button" class="btn" aria-expanded={adjusting} onclick={() => (adjusting = !adjusting)}>Pedir ajustes</button>
              <button type="button" class="btn primary" disabled={busy} onclick={approve}>Aprovar e seguir</button>
            </div>
          </div>
          {#if adjusting}
            <div class="adjust">
              <label class="label" for="pl-feedback">{designing ? "O que ajustar no design" : "O que ajustar no plano"}</label>
              <textarea id="pl-feedback" class="input area" rows="3" bind:value={feedback} placeholder={designing ? "Ex.: botão maior e no rodapé; tire a coluna de data." : "Ex.: não ponha o botão no cabeçalho; use o menu da conversa."}
                onkeydown={(e) => e.key === "Enter" && (e.metaKey || e.ctrlKey) && requestChanges()}></textarea>
              <div class="row-w">
                <button type="button" class="btn primary small" disabled={busy || !feedback.trim()} onclick={requestChanges}>{designing ? "Enviar ao Designer" : "Enviar ao Planejador"}</button>
                <button type="button" class="btn small" onclick={() => (adjusting = false)}>Voltar</button>
                <span class="muted small">{designing ? "O design volta ao Designer com o seu texto." : "O plano volta ao Planejador com o seu texto."}</span>
              </div>
            </div>
          {/if}
          <div class="plan md">{@html renderMarkdown(designing ? designSpec(approval.input) || "O Designer não escreveu a especificação; abra o design." : approval.input)}</div>
        </section>
      {:else if choice}
        <ReviewChoiceCard stage={choice} review={choiceReview} {busy} ondecide={decideReview} />
      {:else if !active}
        <section class="card outcome" class:ok={detail.status === "done"} class:bad={detail.status === "failed" || detail.status === "needs_info"} aria-label="Resultado da execução">
          {#if detail.status === "done"}
            <div class="o-top"><span class="pill ok">Concluída</span><h2>Pronto para você conferir</h2></div>
            <ul class="o-body">
              <li>{detail.mode === "worktree" ? `Alterações no ramo ${detail.branch}, em ${detail.workdir}` : "Alterações feitas direto na pasta de desenvolvimento"}</li>
              <li>{detail.rounds === 0 ? "O Revisor aprovou de primeira" : `O trabalho voltou ${detail.rounds} ${detail.rounds === 1 ? "vez" : "vezes"} antes de ser aprovado`}</li>
              {#if detail.jira_key}
                <li>
                  {createdTask ? "Task criada no Jira pelo Relator:" : "Task do Jira comentada pelo Relator:"}
                  {#if detail.jira_url}<button type="button" class="lnk inline" onclick={() => openUrl(detail.jira_url!)}>{detail.jira_key} ↗</button>{:else}<b>{detail.jira_key}</b>{/if}
                </li>
                {#if jiraResult}<li class:warn-text={jiraResult.failed}>{jiraResult.failed ? `Falhou no Jira: ${jiraResult.text}` : `No Jira: ${jiraResult.text}`}</li>{/if}
              {:else if !detail.skipped.includes("reporter")}
                <li class="warn-text">O Relator não criou a task no Jira{jiraResult?.failed ? `: ${jiraResult.text}` : "; veja o motivo no relatório dele"}.</li>
              {/if}
            </ul>
            {#if detail.mode === "worktree"}
              <div class="commit">
                <label class="label" for="pl-commit">Mensagem do commit</label>
                <div class="row-w">
                  <input id="pl-commit" class="input grow" bind:value={commitMessage} disabled={committed} />
                  <button type="button" class="btn primary" disabled={busy || committed || !commitMessage.trim()} onclick={commit}>{committed ? "Commit feito" : "Fazer commit no ramo"}</button>
                </div>
                <p class="muted small">Nada foi commitado nem enviado pelos bots: quem decide é você. O push fica com você.</p>
              </div>
            {/if}
            <div class="row-w acts">
              <button type="button" class="btn" onclick={() => select("implementer")}>Ver alterações</button>
              <button type="button" class="btn" onclick={() => select("reporter")}>Ver relatório</button>
              {#if detail.jira_key && detail.jira_url}
                <button type="button" class="btn" title={detail.jira_url} onclick={() => openUrl(detail.jira_url!)}>Abrir {detail.jira_key} no Jira ↗</button>
              {/if}
              <button type="button" class="btn" aria-expanded={reopening} onclick={() => (reopening = !reopening)}>Reabrir com um ajuste</button>
            </div>
            {#if reopening}
              <div class="adjust">
                <label class="label" for="pl-adjust">O que não funcionou ao testar</label>
                <textarea id="pl-adjust" class="input area" rows="3" bind:value={adjustment}
                  placeholder="Ex.: o botão Exportar gera o arquivo vazio quando a conversa tem imagens."
                  onkeydown={(e) => e.key === "Enter" && (e.metaKey || e.ctrlKey) && reopen()}></textarea>
                <div class="row-w">
                  <button type="button" class="btn primary small" disabled={busy || !adjustment.trim()} onclick={reopen}>Enviar aos bots</button>
                  <button type="button" class="btn small" onclick={() => (reopening = false)}>Voltar</button>
                </div>
                <p class="muted small">
                  O Implementador corrige a partir do seu texto, o Revisor e o Testador conferem de novo e o Relator comenta na
                  {detail.jira_key ? ` mesma task ${detail.jira_key}` : " task do Jira (sem criar outra, se ela já existir)"}.
                </p>
              </div>
            {/if}
          {:else if detail.status === "needs_info"}
            <div class="o-top"><span class="pill bad">Precisa de informação</span><h2>A Triagem achou a task vaga demais para seguir</h2></div>
            <div class="o-body md">{@html renderMarkdown(passes.triage.at(-1)?.output ?? "")}</div>
            <div class="row-w acts"><button type="button" class="btn primary" onclick={complete}>Completar o pedido</button></div>
          {:else}
            <div class="o-top"><span class="pill {status.tone}">{status.label}</span><h2>{detail.status === "cancelled" ? "Execução cancelada por você" : "A esteira parou"}</h2></div>
            {#if detail.note}<p class="o-body">{detail.note}</p>{/if}
            <p class="o-body muted">{detail.mode === "worktree" ? `O que já foi alterado continua na worktree ${detail.branch}.` : "O que já foi alterado continua na pasta de desenvolvimento."} {detail.round_limit ? "Ao retomar, a contagem de voltas recomeça e o Implementador corrige o que foi apontado por último." : "Ao retomar, a etapa que parou roda de novo."}</p>
            <div class="row-w acts"><button type="button" class="btn primary" disabled={busy} onclick={retry}>Retomar execução</button></div>
          {/if}
        </section>
      {/if}

      <section class="card log-card" aria-label="Registro da execução">
        <div class="r-top"><h2>Registro</h2><span class="muted small">cada etapa roda numa sessão própria; o que recebeu e o que entregou ficam guardados</span></div>
        <ol class="reg" bind:this={regEl} onscroll={onRegScroll}>
          {#each detail.stages as s (s.id)}
            {@const v = stageVisual(s, s.status === "running" && !!chat.permissions[s.conversation_id])}
            {@const tag = passLabel(s, indexOf(s), detail.max_rounds)}
            <li>
              <button type="button" class="reg-item" class:sel={panelStageId === s.id} onclick={() => select(s.role, s.id)}>
                <span class="r-time" title={fullDate(s.started_at)}>{formatTime(s.started_at)}</span>
                <span class="r-mark"><span class="node mini s-{v.visual}" class:gate={isUserStep(s.role)}>{@html stationGlyph(s.role, v.visual)}</span></span>
                <span class="r-body">
                  <span class="r-l1">
                    <b>{STATION[s.role].name}</b>{#if tag}<span class="tag">{tag}</span>{/if}
                    <span class="r-right"><span class="mono">{duration(stageSeconds(s))}</span><span class="sep">·</span>{stageAccount(s)}{#if STATION[s.role].model}<span class="sep">·</span>{detail.agents[s.role]?.model ? `${detail.agents[s.role]?.model} no Claude` : STATION[s.role].model}{/if}</span>
                  </span>
                  <span class="r-l2"><span class="io">{inputSummary(s, detail)}</span>{@html icon("arrow", 11)}<span class="io out o-{v.visual}">{outputSummary(s)}</span></span>
                </span>
              </button>
            </li>
          {:else}
            <li class="muted small empty">Preparando a primeira etapa…</li>
          {/each}
        </ol>
      </section>
    </div>

    {#if panelStation}
      <aside class="card panel" aria-label="Detalhes da estação">
        <header class="p-top">
          <div class="p-l1">
            <h2>{panelStation.name}</h2>
            {#if panelVisual}<span class="chip c-{panelVisual.visual}"><i></i>{panelVisual.label}</span>{/if}
          </div>
          <div class="p-meta">
            {#if panelAgent}agente <b>{panelAgent.name}</b> · {/if}{#if panelModel}modelo <b>{panelModel}</b> · {:else if panelStation.model}{panelStation.model.toLowerCase()} · {/if}{#if panelEffort}esforço <b>{panelEffort}</b> · {/if}{panelStation.perm}
            {#if panelStage && !isUserStep(panelStage.role)} · conta <b>{stageAccount(panelStage)}</b>{/if}
            {#if panelStage} · <span class="mono">{duration(stageSeconds(panelStage))}</span>{/if}
          </div>
          {#if passes[panelStation.role].length > 1 || (!follow && latest)}
            <div class="p-bar">
              {#if passes[panelStation.role].length > 1}
                <span class="seg" role="group" aria-label="Passagens desta estação">
                  {#each passes[panelStation.role] as s, i (s.id)}
                    <button type="button" aria-pressed={s.id === panelStageId} onclick={() => select(s.role, s.id)}>{passLabel(s, i, detail.max_rounds) ?? "1ª passagem"}</button>
                  {/each}
                </span>
              {/if}
              {#if !follow && latest && active}
                <button type="button" class="lnk" onclick={() => { follow = true; }}>Voltar para a etapa ativa</button>
              {/if}
            </div>
          {/if}
        </header>
        <div class="p-body">
          {#if !panelStage}
            <section><h3 class="label">O que faz</h3><p>{panelStation.does}</p>
              <div class="pair"><div><span class="label">Recebe</span>{panelStation.receives}</div><div><span class="label">Entrega</span>{panelStation.delivers}</div></div>
            </section>
            <p class="muted small">{detail.skipped.includes(panelStation.role) ? "Desligada neste projeto: a esteira passa direto por ela." : active ? "Ainda não recebeu o trabalho." : "Não rodou nesta execução."}</p>
          {:else}
            {#if panelAsking && panelPermission}
              {@const cid = panelStage.conversation_id}
              <!-- O mesmo pedido que apareceria no chat, respondido aqui sem abrir a conversa da etapa. -->
              <div class="ask-card">
                <p class="ask">{@html icon("you", 13)}<span>A etapa pede sua autorização.</span>
                  <button type="button" class="btn small" onclick={() => openStageConversation(cid)}>Abrir conversa</button></p>
                {#if panelPermission.tool === "AskUserQuestion"}
                  <QuestionCard request={panelPermission} onanswer={(answers) => answerStage(cid, true, false, { answers })} oncancel={() => answerStage(cid, false, false)} />
                {:else if panelPermission.tool === "ExitPlanMode"}
                  <PlanCard request={panelPermission} ondecide={(mode) => (mode ? answerStage(cid, true, false, { nextMode: mode }) : answerStage(cid, false, false))} />
                {:else}
                  <PermissionCard request={panelPermission} ondecide={(allow, always) => answerStage(cid, allow, always)} />
                {/if}
              </div>
            {/if}
            {#if !isMarker(panelStage.role)}
            <details class="entry" bind:open={entryOpen}>
              <summary><h3 class="label">Entrada</h3><span class="res">{inputSummary(panelStage, detail)}</span>{@html icon("chev", 13, 2.2)}</summary>
              {#if images.length && ["triage", "designer", "planner", "implementer"].includes(panelStage.role)}
                <div class="thumbs" aria-label="Imagens do pedido">
                  {#each images as image, i (i)}
                    <button type="button" class="thumb" title={`Ampliar ${image.name}`} onclick={() => (zoomed = image)}><img src={imageSrc(image)} alt={image.name} /></button>
                  {/each}
                </div>
              {/if}
              {#if panelStage.role === "approval" || panelStage.role === "design_approval"}
                <div class="md">{@html renderMarkdown(panelStage.input)}</div>
              {:else if panelStage.role === "review_choice"}
                <div class="md">{@html renderMarkdown(findingsMarkdown(choiceFindings(panelStage)) || "O code review não trouxe a lista de achados.")}</div>
              {:else}
                <pre class="prompt">{panelStage.input}</pre>
              {/if}
            </details>
            {/if}
            {#if !isUserStep(panelStage.role)}
              <section>
                <h3 class="label">{panelStage.status === "running" ? "Agora" : "O que fez"}</h3>
                <ol class="tlog" bind:this={logEl} onscroll={onLogScroll}>
                  {#each panelLog as l, i (i)}
                    <li class={l.tone}><span class="t" title={fullDate(l.at)}>{formatTime(l.at)}</span><span class="i">{@html icon(l.glyph, 12)}</span><span class="va"><span class="v">{l.verb}</span> <span class="a">{l.target}</span></span></li>
                  {/each}
                  {#if panelStage.status === "running"}
                    <li class="live"><span class="t"></span><span class="i">{#if panelAsking}<span class="dot warn pulse"></span>{:else}<span class="spinner mini"></span>{/if}</span><span class="va"><span class="a">{panelAsking ? "Esperando sua autorização" : (liveStatus ?? "Trabalhando")}</span></span></li>
                  {:else if panelLog.length === 0}
                    <li><span class="t"></span><span class="i"></span><span class="va muted">Sem ferramentas registradas.</span></li>
                  {/if}
                </ol>
              </section>
            {/if}
            <section>
              <h3 class="label">{panelStage.role === "feedback" ? "O que você pediu" : panelStage.role === "resume" ? "O que aconteceu" : "Saída"}</h3>
              {#if panelStage.output}
                {#if panelStage.status === "failed"}<p class="failed">{panelStage.output}</p>{:else}<div class="md">{@html renderMarkdown(panelStage.output)}</div>{/if}
              {:else}
                <p class="muted small">{panelStage.status === "running" ? (panelStage.role === "approval" ? "Falta a sua decisão: aprovar o plano ou pedir ajustes." : panelStage.role === "design_approval" ? "Falta a sua decisão: aprovar o design ou pedir ajustes." : panelStage.role === "review_choice" ? "Falta a sua escolha: o que corrigir dos achados do code review." : `Ainda trabalhando. Vai entregar: ${panelStation.delivers.toLowerCase()}`) : "Nada entregue."}</p>
              {/if}
            </section>
            {#if panelStage.role === "designer" && panelStage.output && designUrl(panelStage.output)}
              {@const link = designUrl(panelStage.output)!}
              <button type="button" class="lnk" title={link} onclick={() => openUrl(link)}>Abrir o design no Claude Design ↗</button>
            {/if}
            {#if panelStage.conversation_id}
              <button type="button" class="lnk" onclick={() => openStageConversation(panelStage.conversation_id)}>Abrir a conversa desta etapa</button>
            {/if}
          {/if}
        </div>
      </aside>
    {/if}
  </div>
</div>

<svelte:window onkeydown={(e) => zoomed && e.key === "Escape" && (zoomed = null)} />

{#if zoomed}
  <button type="button" class="zoom" aria-label="Fechar a imagem" onclick={() => (zoomed = null)}>
    <img src={imageSrc(zoomed)} alt={zoomed.name} />
  </button>
{/if}

<style>
  .screen { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 14px; padding: 18px 24px; }
  .screen > * { width: 100%; max-width: 1440px; margin-inline: auto; }
  .head { flex: none; display: flex; flex-wrap: wrap; align-items: flex-end; gap: 12px 28px; }
  .ident { flex: 1 1 420px; min-width: 0; }
  .l1 { display: flex; align-items: center; gap: 6px 10px; flex-wrap: wrap; }
  .head h1 { font-family: var(--display); font-size: 22px; font-weight: 600; letter-spacing: -0.02em; line-height: 1.25; overflow-wrap: anywhere; }
  .key { font-family: var(--mono); font-size: 12px; font-weight: 500; color: var(--accent-text); padding: 1px 7px; border-radius: 6px; background: rgba(91, 106, 217, 0.14); border: 1px solid rgba(91, 106, 217, 0.3); white-space: nowrap; }
  .key.link { cursor: pointer; }
  .key.link:hover { background: rgba(91, 106, 217, 0.24); }
  .lnk.inline { align-self: auto; font-size: inherit; }
  .warn-text { color: var(--warn-text); }
  .key.manual { color: var(--text-2); background: var(--surface-2); border-color: var(--border-strong); font-family: var(--font); font-weight: 600; font-size: 11.5px; }
  .meta { display: flex; flex-wrap: wrap; gap: 4px 18px; margin-top: 8px; font-size: 12.5px; color: var(--text-2); }
  .m { display: inline-flex; align-items: center; gap: 6px; min-width: 0; }
  .m :global(svg) { flex: none; color: var(--muted); }
  .m code { padding: 0 6px; border: 1px solid var(--border); border-radius: 5px; background: var(--surface-2); font-family: var(--mono); font-size: 12px; }
  .m.warn, .m.warn :global(svg) { color: var(--warn-text); }
  .nums { display: flex; align-items: center; gap: 12px 24px; flex-wrap: wrap; padding-bottom: 6px; }
  .num { position: relative; display: flex; flex-direction: column; gap: 2px; }
  .value { font-family: var(--mono); font-size: 15px; font-weight: 500; line-height: 1.35; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .value small { font-family: var(--font); font-size: 12px; font-weight: 400; color: var(--muted); }
  .value.stopped { color: var(--muted); }
  .meter { position: absolute; left: 0; right: 0; bottom: -6px; height: 3px; border-radius: 2px; background: var(--border); overflow: hidden; }
  .meter span { display: block; height: 100%; border-radius: 2px; background: var(--accent); }
  .cancel { position: relative; }
  .confirm { position: absolute; right: 0; top: calc(100% + 8px); z-index: 40; width: 290px; padding: 12px; border: 1px solid var(--border-strong); border-radius: 10px; background: var(--surface-2); box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5); }
  .confirm p { font-size: 12.5px; color: var(--text-2); margin: 4px 0 10px; }
  .acts { display: flex; gap: 8px; justify-content: flex-end; }

  .pill { display: inline-flex; align-items: center; gap: 6px; padding: 2px 9px; border-radius: 999px; font-size: 11.5px; font-weight: 600; background: var(--surface-2); color: var(--text-2); white-space: nowrap; }
  .pill.ok { background: var(--ok-bg); color: var(--ok-text); }
  .pill.warn { background: var(--warn-bg); color: var(--warn-text); }
  .pill.acc { background: rgba(91, 106, 217, 0.16); color: var(--accent-text); }
  .pill.bad { background: var(--danger-bg); color: var(--danger-text); }
  .dot { width: 7px; height: 7px; flex: none; border-radius: 50%; background: var(--faint); display: inline-block; }
  .dot.acc { background: var(--accent-text); }
  .dot.warn { background: var(--warn); }
  .dot.pulse { animation: pulse 1.6s ease-in-out infinite; }
  @keyframes pulse { 50% { box-shadow: 0 0 0 3px rgba(217, 164, 65, 0.28); } }
  .spinner { display: inline-block; flex: none; width: 12px; height: 12px; border: 2px solid var(--border-strong); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  .spinner.mini { width: 10px; height: 10px; border-width: 1.5px; }
  @keyframes spin { to { transform: rotate(360deg); } }
  h2 { font-family: var(--display); font-size: 14px; font-weight: 600; }
  .tag { display: inline-flex; align-items: center; padding: 0 6px; border-radius: 5px; border: 1px solid var(--border-strong); font-family: var(--mono); font-size: 10.5px; color: var(--text-2); white-space: nowrap; }
  .lnk { align-self: flex-start; border: 0; background: none; padding: 0; color: var(--accent-text); font-size: 12px; text-decoration: underline; text-decoration-color: rgba(166, 176, 247, 0.35); text-underline-offset: 2px; }
  .row-w { display: flex; align-items: center; gap: 8px 10px; flex-wrap: wrap; }
  .grow { flex: 1; min-width: 200px; }

  .track { flex: none; position: relative; padding: 10px 16px 6px; }
  .track-top { display: flex; align-items: center; gap: 12px; min-height: 28px; }
  .now { flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; font-size: 12.5px; color: var(--text-2); }
  .now .ic { flex: none; display: grid; place-items: center; width: 14px; height: 14px; }
  .now b { flex: none; font-weight: 600; color: var(--text); }
  .what { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t-ok .ic { color: var(--ok-text); }
  .t-bad .ic { color: var(--danger-text); }
  .legend { position: absolute; right: 16px; top: 42px; z-index: 20; width: 330px; max-width: calc(100% - 32px); padding: 12px 14px; border: 1px solid var(--border-strong); border-radius: 10px; background: var(--surface-2); box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5); }
  .legend ul { list-style: none; padding: 0; margin: 8px 0 0; display: flex; flex-direction: column; gap: 7px; }
  .legend li { display: grid; grid-template-columns: 18px minmax(0, 1fr); gap: 9px; align-items: center; font-size: 12.5px; color: var(--text-2); }
  .legend b { color: var(--text); font-weight: 600; }
  .legend .foot { margin-top: 10px; padding-top: 9px; border-top: 1px solid var(--border-strong); font-size: 12px; color: var(--muted); }

  .node.mini { position: relative; display: grid; place-items: center; width: 16px; height: 16px; border: 1.25px solid var(--border-strong); border-radius: 50%; background: var(--bg-side); color: var(--faint); }
  .node.mini :global(svg) { display: block; }
  .node.mini.gate { width: 13px; height: 13px; margin: 1.5px; border-radius: 4px; transform: rotate(45deg); }
  .node.mini.gate :global(svg) { transform: rotate(-45deg); }
  .s-working { border-color: var(--accent) !important; color: var(--accent-text) !important; }
  .s-done { background: var(--ok-bg) !important; border-color: rgba(88, 184, 138, 0.7) !important; color: var(--ok-text) !important; }
  .s-returned { border-color: rgba(239, 164, 155, 0.75) !important; color: var(--danger-text) !important; }
  .s-failed { background: var(--danger-bg) !important; border-color: var(--danger-text) !important; color: var(--danger-text) !important; }
  .s-skipped { border-style: dashed !important; }
  .s-you { background: var(--warn-bg) !important; border-color: var(--warn) !important; color: var(--warn-text) !important; }

  .bottom { flex: 1 1 auto; min-height: 400px; display: grid; grid-template-columns: minmax(0, 1fr) minmax(340px, 430px); gap: 14px; }
  .main { min-width: 0; min-height: 0; display: flex; flex-direction: column; gap: 14px; }

  .approval { flex: 0 1 auto; min-height: 0; display: flex; flex-direction: column; padding: 14px 16px 16px; border-color: rgba(217, 164, 65, 0.45); }
  .ap-top { display: flex; align-items: center; gap: 10px 14px; flex-wrap: wrap; }
  .ap-title { flex: 1 1 300px; min-width: 0; }
  .note { margin-top: 4px; font-size: 12.5px; color: var(--text-2); }
  .adjust { margin-top: 12px; display: flex; flex-direction: column; gap: 6px; }
  .area { width: 100%; min-height: 72px; padding: 9px 11px; resize: vertical; line-height: 1.5; }
  .plan { margin-top: 12px; padding: 14px 16px; max-height: 52vh; overflow-y: auto; border: 1px solid var(--border); border-radius: 10px; background: var(--bg); }

  .outcome { padding: 14px 16px 16px; }
  .outcome.ok { border-color: rgba(88, 184, 138, 0.4); }
  .outcome.bad { border-color: rgba(239, 164, 155, 0.4); }
  .o-top { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .o-body { margin: 8px 0 0; font-size: 13px; color: var(--text-body); }
  ul.o-body { padding-left: 18px; display: flex; flex-direction: column; gap: 3px; }
  .commit { margin-top: 12px; display: flex; flex-direction: column; gap: 6px; }
  .outcome .acts { justify-content: flex-start; margin-top: 12px; }

  .log-card { flex: 1 1 120px; min-height: 120px; display: flex; flex-direction: column; padding: 12px 8px 8px; }
  .r-top { display: flex; align-items: baseline; gap: 4px 10px; flex-wrap: wrap; padding: 0 8px; margin-bottom: 6px; }
  .reg { flex: 1; min-height: 0; overflow-y: auto; list-style: none; margin: 0; padding: 0; }
  .reg .empty { padding: 6px 8px; }
  .reg-item { display: grid; grid-template-columns: 40px 18px minmax(0, 1fr); gap: 0 8px; align-items: start; width: 100%; padding: 6px 8px; border: 0; border-radius: 8px; background: none; color: inherit; text-align: left; }
  .reg-item:hover { background: var(--hover); }
  .reg-item.sel { background: var(--surface-2); }
  .r-time { padding-top: 1px; font-family: var(--mono); font-size: 11px; font-variant-numeric: tabular-nums; color: var(--faint); }
  .r-mark { display: grid; place-items: center; height: 20px; }
  .r-body { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .r-l1 { display: flex; align-items: center; gap: 8px; min-width: 0; line-height: 20px; }
  .r-l1 b { flex: none; font-weight: 600; }
  .r-right { margin-left: auto; display: inline-flex; align-items: center; gap: 6px; min-width: 0; font-size: 12px; color: var(--muted); white-space: nowrap; overflow: hidden; }
  .r-right .mono { color: var(--text-2); font-size: 11.5px; }
  .sep { color: var(--faint); }
  .r-l2 { display: flex; align-items: center; gap: 6px; min-width: 0; font-size: 12.5px; color: var(--muted); }
  .r-l2 :global(svg) { flex: none; color: var(--faint); }
  .io { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .io.out { flex: 1 1 0; color: var(--text-2); }
  .o-returned, .o-failed { color: var(--danger-text) !important; }
  .o-done { color: var(--ok-text) !important; }
  .o-you { color: var(--warn-text) !important; }

  .panel { min-width: 0; min-height: 0; max-height: 78vh; display: flex; flex-direction: column; overflow: hidden; position: sticky; top: 0; }
  .p-top { flex: none; padding: 12px 14px 11px; border-bottom: 1px solid var(--border); }
  .p-l1 { display: flex; align-items: center; gap: 8px; }
  .p-l1 h2 { font-size: 15px; }
  .p-meta { margin-top: 6px; font-size: 12px; color: var(--muted); }
  .p-meta b { font-weight: 500; color: var(--text-2); }
  .p-bar { display: flex; align-items: center; gap: 8px 12px; flex-wrap: wrap; margin-top: 9px; }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: 6px; overflow: hidden; }
  .seg button { border: 0; border-left: 1px solid var(--border); background: transparent; color: var(--muted); min-height: 24px; padding: 0 8px; font-size: 12px; }
  .seg button:first-child { border-left: 0; }
  .seg button[aria-pressed="true"] { background: var(--surface-2); color: var(--text); }
  .p-body { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; padding: 12px 14px 16px; font-size: 13px; color: var(--text-body); }
  .p-body h3 { margin: 0 0 6px; }
  .pair { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; margin-top: 10px; }
  .pair div { padding: 8px 10px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface); font-size: 12.5px; }
  .pair .label { display: block; margin-bottom: 2px; }
  .ask-card { display: flex; flex-direction: column; gap: 8px; }
  .ask-card .ask button { margin-left: auto; }
  .ask { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; padding: 8px 10px; border: 1px solid rgba(217, 164, 65, 0.45); border-radius: 8px; background: var(--warn-bg); color: var(--warn-text); font-size: 12.5px; }
  .entry summary { display: flex; align-items: center; gap: 8px; list-style: none; cursor: pointer; }
  .entry summary::-webkit-details-marker { display: none; }
  .entry summary h3 { margin: 0; }
  .entry .res { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12.5px; color: var(--text-2); }
  .entry summary :global(svg) { flex: none; color: var(--muted); transition: transform 0.15s; }
  .entry[open] summary :global(svg) { transform: rotate(90deg); }
  .entry[open] summary { margin-bottom: 8px; }
  .prompt { margin: 0; padding: 10px 12px; max-height: 280px; overflow: auto; border: 1px solid var(--border); border-radius: 8px; background: var(--bg); font-family: var(--mono); font-size: 11.5px; line-height: 1.6; white-space: pre-wrap; overflow-wrap: anywhere; color: var(--text-2); }
  .tlog { list-style: none; margin: 0; padding: 8px 10px; max-height: 260px; overflow-y: auto; border: 1px solid var(--border); border-radius: 8px; background: var(--bg); font-family: var(--mono); font-size: 11.5px; line-height: 1.75; }
  .tlog li { display: grid; grid-template-columns: 40px 14px minmax(0, 1fr); gap: 0 6px; align-items: center; color: var(--muted); }
  .tlog .t { font-variant-numeric: tabular-nums; color: var(--faint); }
  .tlog .i { display: grid; place-items: center; height: 14px; }
  .tlog .va { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tlog .a { color: var(--text-2); }
  .tlog li.warn, .tlog li.warn .a { color: var(--warn-text); }
  .tlog li.swap, .tlog li.swap .a { color: var(--accent-text); }
  .tlog li.error, .tlog li.error .a { color: var(--danger-text); }
  .tlog li.live .a { color: var(--text); }
  .failed { color: var(--danger-text); }
  .thumbs { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px; }
  .thumb { padding: 0; cursor: zoom-in; width: 56px; height: 56px; border: 1px solid var(--border-strong); border-radius: 6px; overflow: hidden; background: var(--bg); }
  .thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .zoom { position: fixed; inset: 0; z-index: 80; display: grid; place-items: center; padding: 32px; border: 0; background: rgba(0, 0, 0, 0.75); cursor: zoom-out; }
  .zoom img { max-width: 100%; max-height: 100%; border-radius: 8px; box-shadow: 0 12px 40px rgba(0, 0, 0, 0.6); }
  .md :global(:first-child) { margin-top: 0; }
  .md :global(p), .md :global(ul), .md :global(ol) { margin: 0 0 8px; }
  .md :global(h1), .md :global(h2), .md :global(h3), .md :global(h4) { margin: 12px 0 6px; font-size: 13px; font-family: var(--font); font-weight: 600; color: var(--text); }
  .md :global(code) { padding: 0 4px; border-radius: 4px; background: var(--surface-2); font-family: var(--mono); font-size: 12px; }
  .md :global(pre) { padding: 10px; border-radius: 8px; background: var(--bg); overflow-x: auto; }
  .md :global(pre code) { padding: 0; background: none; }

  @media (max-width: 1040px) {
    .bottom { grid-template-columns: minmax(0, 1fr); }
    .panel { position: static; max-height: none; }
  }
</style>
