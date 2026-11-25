<script lang="ts">
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api, errorMessage } from "../../api";
  import { formatTime, formatWhen, fullDate, renderMarkdown } from "../../format";
  import { addPastedImages, type PreviewImage } from "../../paste";
  import { accounts } from "../../state/accounts.svelte";
  import { chat, decideFor, openConversation, type DecisionExtras, type PermissionRequest } from "../../state/chat.svelte";
  import { liveActivity, liveSubagentProgress, liveSubagentTools, loadSubagents } from "../../state/subagents.svelte";
  import PermissionCard from "../PermissionCard.svelte";
  import PlanCard from "../PlanCard.svelte";
  import { questionsOf, withoutRecommended } from "../../tools";
  import QuestionCard from "../QuestionCard.svelte";
  import { projectName, projects } from "../../state/projects.svelte";
  import { loadDetail, loadPipelines, openPipeline, pipelines, stageMessages } from "../../state/pipelines.svelte";
  import {
    choiceFindings, clock, designSpec, designUrl, duration, findingsMarkdown, icon, inputSummary, isActive, isMarker, jiraOutcome, isUserStep, isWaitingYou, outputSummary, passesByRole, passLabel, RUN_STATUS,
    reviewPicks, roleIcon, stageAccountId, stageLog, stageModelText, stageVisual, STATION, STATIONS, stationRole, switchedAccount, costText, tokenCount, totalTokens, toolLogLine, triageQuestions, triageSummary, usageHint, usageText, VISUAL, opensPr,
  } from "../../pipeline";
  import { EFFORTS, type CiWatch, type Delivery, type GitInfo, type PastedImage, type PipelineDetail, type PipelineRole, type ReviewPick, type StageRun, type View } from "../../types";
  import Odometer from "./Odometer.svelte";
  import PipelineNow from "./PipelineNow.svelte";
  import PipelineTrack, { type Transit } from "./PipelineTrack.svelte";
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
  /** Entrega escolhida no fim; começa pela padrão do projeto. */
  let delivery = $state<Delivery>("main");
  let push = $state(false);
  /** Git do projeto da execução, para saber se há remoto e `gh`. */
  let git = $state<GitInfo | null>(null);
  /** Reabrir uma execução concluída com um ajuste. */
  let reopening = $state(false);
  let adjustment = $state("");
  /** Prints colados no ajuste, com a miniatura. */
  let adjustImages = $state<PreviewImage[]>([]);

  /** Imagens coladas no campo do ajuste viram anexos dele, com miniatura. */
  async function onAdjustPaste(event: ClipboardEvent) {
    const added = await addPastedImages(event, adjustImages);
    if (added) ({ images: adjustImages, error } = added);
  }
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
    const id = stageAccountId(stageMessages(designer));
    return accounts.list.find((a) => a.id === id)?.name ?? null;
  });
  /** Parada da sua escolha sobre o code review, e a passagem do Code review que a abriu. */
  const choice = $derived(
    detail.status === "awaiting_choice" ? (detail.stages.findLast((s) => s.role === "review_choice" && s.status === "running") ?? null) : null,
  );
  const choiceReview = $derived(choice ? (detail.stages.findLast((s) => s.role === "code_review" && s.status === "done") ?? null) : null);
  /** Marcas da sua escolha: ficam aqui para o cartão Agora decidir com elas também. */
  let choiceMarked = $state<Record<string, boolean>>({});
  let choiceChosen = $state<Record<string, number>>({});
  let choiceNote = $state("");
  const choiceId = $derived(choice?.id ?? null);
  // Outra parada (ex.: um novo ciclo): começa sem nada marcado.
  $effect(() => {
    void choiceId;
    choiceMarked = {};
    choiceChosen = {};
    choiceNote = "";
  });
  const choiceFound = $derived(choice ? choiceFindings(choice) : []);
  const choiceDecision = $derived(reviewPicks(choiceFound, choiceMarked, choiceChosen));
  const status = $derived(RUN_STATUS[detail.status]);
  /** Segundos gastos pelos bots, contando a etapa em andamento até agora. */
  const spent = $derived(detail.spent_seconds + (running ? Math.max(0, now - pipelines.fetchedAt / 1000) : 0));
  const elapsed = $derived((active ? now : detail.updated_at) - detail.created_at);
  const budgetPercent = $derived(Math.min(100, (spent / (detail.budget_minutes * 60)) * 100));
  /** Tokens e custo do cabeçalho, separados para o número rolar como odômetro. */
  const tokParts = $derived(tokenCount(totalTokens(detail.usage)).split(" "));
  const tokNum = $derived(tokParts[0]);
  const tokUnit = $derived(tokParts[1] ?? "");
  const cost = $derived(costText(detail.usage));

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

  // Entrega padrão do projeto e o git dele, uma vez por execução com git.
  let deliveryFor: string | null = null;
  $effect(() => {
    if (detail.mode === "direct" || detail.status !== "done" || deliveryFor === detail.id) return;
    const id = (deliveryFor = detail.id);
    git = null;
    api.pipelineConfig(detail.project_id).then((c) => {
      if (deliveryFor !== id) return;
      delivery = c.delivery;
      push = c.push;
    }, () => {});
    api.projectGitInfo(detail.project_id).then((g) => deliveryFor === id && (git = g), () => {});
  });
  const canPush = $derived(!!git?.remote);
  const canPr = $derived(!!git?.remote && !!git?.gh_ready);
  const prBlock = $derived(!git ? "Verificando o git…" : !git.remote ? "O repositório não tem remoto." : !git.gh_ready ? "Instale o GitHub CLI e entre nele (gh auth login)." : null);
  const DELIVERY_OPTIONS = $derived<{ id: Delivery; label: string; hint: string; disabled: string | null }[]>([
    {
      id: "main",
      label: detail.mode === "worktree" ? `Commit na ${detail.base_branch ?? "main"}` : `Commit no ramo atual${detail.base_branch ? ` (${detail.base_branch})` : ""}`,
      hint: detail.mode === "worktree" ? "Faz o commit na cópia, junta na pasta principal e apaga a cópia." : "Faz o commit na pasta, no ramo em que ela está.",
      disabled: null,
    },
    {
      id: "branch",
      label: detail.mode === "worktree" ? `Commit no ramo ${detail.branch ?? ""}` : "Commit num ramo novo",
      hint: detail.mode === "worktree" ? "O ramo e a cópia ficam para você juntar depois." : "Cria um ramo lume/… na pasta, que fica nele.",
      disabled: null,
    },
    {
      id: "pull_request",
      label: "Abrir pull request",
      hint: `Commit num ramo próprio, push e pull request para ${detail.base_branch ?? "a main"}, com o relatório do Relator como texto.`,
      disabled: prBlock,
    },
    {
      id: "pull_request_merge",
      label: `Pull request com merge na ${detail.base_branch ?? "main"}`,
      hint: `Abre o pull request e já faz o merge no GitHub; a ${detail.base_branch ?? "main"} do seu computador é atualizada e o ramo sai. Se o GitHub exigir revisão, o pull request fica aberto.`,
      disabled: prBlock,
    },
  ]);

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
  const panelMessages = $derived(panelStage ? stageMessages(panelStage) : []);
  const panelAsking = $derived(!!panelStage && panelStage.status === "running" && !!chat.permissions[panelStage.conversation_id]);
  const panelVisual = $derived(panelStage ? stageVisual(panelStage, panelAsking) : null);
  const panelPermission = $derived(panelAsking && panelStage ? chat.permissions[panelStage.conversation_id] : undefined);
  /** Etapa desta execução esperando uma resposta sua: o pedido abre numa janela grande, fácil de ler. */
  const askingStage = $derived(detail.stages.find((s) => s.status === "running" && !!chat.permissions[s.conversation_id]) ?? null);
  /** Pedido que o usuário deixou para depois: ele segue só no painel da estação. */
  let laterId = $state<string | null>(null);
  const askModal = $derived.by(() => {
    const request = askingStage ? chat.permissions[askingStage.conversation_id] : undefined;
    return askingStage && request && request.requestId !== laterId ? { stage: askingStage, request } : null;
  });
  /** Janela modal: o foco entra nela ao abrir e o Tab circula só entre os controles dela. */
  function trapFocus(node: HTMLElement) {
    const focusables = () => [...node.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), a[href], [tabindex]:not([tabindex='-1'])")];
    node.focus();
    function onkeydown(e: KeyboardEvent) {
      if (e.key !== "Tab") return;
      const list = focusables();
      if (!list.length) return e.preventDefault();
      const [first, last] = [list[0], list[list.length - 1]];
      if (e.shiftKey && (document.activeElement === first || document.activeElement === node)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
    node.addEventListener("keydown", onkeydown);
    return { destroy: () => node.removeEventListener("keydown", onkeydown) };
  }
  /** Responde ao pedido da etapa direto daqui; se a resposta não chegar, o pedido volta e o erro aparece. */
  async function answerStage(id: string, allow: boolean, always: boolean, extras: DecisionExtras = {}) {
    try {
      await decideFor(id, allow, always, extras);
    } catch (e) {
      error = errorMessage(e);
    }
  }
  const panelLog = $derived(stageLog(panelMessages));
  /** Linhas que já estavam no log ao abrir a etapa entram em sequência; as que chegam depois piscam. */
  let logIntro = $state(0);
  let logIntroFor: string | null | undefined;
  $effect.pre(() => {
    const n = panelLog.length;
    if (panelStageId === logIntroFor) return;
    logIntroFor = panelStageId;
    logIntro = n;
  });
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
    void subLog.length;
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
    const id = stageAccountId(stageMessages(panelStage));
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

  /** Linha ao vivo do registro; com as ferramentas do subagente já listadas acima dela, só o andamento dele. */
  const liveStatus = $derived.by(() => {
    if (panelStage?.status !== "running" || !panelStage.conversation_id) return null;
    const turn = chat.turns[panelStage.conversation_id];
    return subLog.length ? liveSubagentProgress(panelStage.conversation_id, turn) : liveActivity(panelStage.conversation_id, turn);
  });
  /** O que os subagentes da etapa (ex.: a skill do code review) já fizeram, enquanto ela roda. */
  const subLog = $derived(
    panelStage?.status === "running" && panelStage.conversation_id
      ? liveSubagentTools(panelStage.conversation_id, chat.turns[panelStage.conversation_id]).map((t) => toolLogLine(t.name, t.input, t.is_error, 0))
      : [],
  );
  // Aberta no meio da etapa, a tela busca o que os subagentes já fizeram (os passos novos chegam ao vivo).
  $effect(() => {
    if (running?.conversation_id) void loadSubagents(running.conversation_id);
  });

  const accountName = (id: string | null) => accounts.list.find((a) => a.id === id)?.name ?? null;
  function stageAccount(stage: StageRun): string {
    if (isUserStep(stage.role)) return "você";
    const live = stage.status === "running" ? chat.turns[stage.conversation_id]?.account : null;
    const name = live ?? accountName(stageAccountId(stageMessages(stage)));
    const swapped = switchedAccount(stageMessages(stage));
    return name ? (swapped ? `${name} (após troca)` : name) : "—";
  }
  const stageSeconds = (s: StageRun) => (s.finished_at ?? now) - s.started_at;
  const indexOf = (s: StageRun) => passes[s.role].indexOf(s);

  /** O trabalho passando de uma estação a outra, enquanto o pacote anda no trilho. */
  let transit = $state<Transit | null>(null);
  /** Cor do cartão Agora e do facho: índigo trabalhando, âmbar esperando você, coral na devolução ou parada. */
  const tone = $derived(
    transit ? (transit.back ? "bad" : "live")
    : isWaitingYou(detail.status) || (running && chat.permissions[running.conversation_id]) ? "you"
    : detail.status === "running" ? "live"
    : detail.status === "done" ? "ok"
    : detail.status === "cancelled" ? "off"
    : "bad",
  );
  /** Estação ativa (a da etapa mais nova), para o entalhe do cartão Agora e o facho do trilho. */
  const activeIndex = $derived.by(() => {
    const last = detail.stages.findLast((s) => !isMarker(s.role));
    if (!last || detail.status === "done") return last ? STATIONS.length - 1 : 0;
    return Math.max(0, STATIONS.findIndex((s) => s.role === stationRole(last.role)));
  });
  /** Linha sob os botões do cartão Agora. */
  const nowNote = $derived(
    approval ? (designing ? (designAccount ? `Publicado pela conta ${designAccount}: o link só abre com essa conta logada no navegador.` : "A especificação do design está no painel da etapa.") : "O plano completo está no painel da etapa, ao lado do registro.")
    : choice ? (choiceFound.length ? `${choiceDecision.picks.length} de ${choiceFound.length} achados marcados no painel da etapa.` : "O texto do code review está no painel da etapa.")
    : null,
  );

  /** Registro longo: as passagens antigas ficam recolhidas, com a contagem por papel. */
  const KEEP_ROWS = 12;
  let olderOpen = $state(false);
  const olderCount = $derived(detail.stages.length > KEEP_ROWS + 3 ? detail.stages.length - KEEP_ROWS : 0);
  const olderStages = $derived(detail.stages.slice(0, olderCount));
  const shownStages = $derived(detail.stages.slice(olderCount));
  const olderChips = $derived(
    [...STATIONS.map((s) => s.role), "designer", "design_approval", "review_choice", "feedback", "resume"]
      .map((role) => ({ role: role as PipelineRole, n: olderStages.filter((s) => s.role === role).length }))
      .filter((c) => c.n > 0),
  );
  /** Linhas que já estavam lá ao abrir a execução entram em sequência; as que chegam depois piscam. */
  let introRun: string | null = null;
  let introCount = $state(0);
  $effect.pre(() => {
    if (detail.id === introRun) return;
    introRun = detail.id;
    olderOpen = false;
    introCount = detail.stages.length;
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
    if (!adjustment.trim() && !adjustImages.length) return;
    await api.reopenPipeline(detail.id, adjustment.trim(), adjustImages.map(({ name, media_type, data }) => ({ name, media_type, data })));
    adjustment = "";
    adjustImages = [];
    reopening = false;
    follow = true;
  });
  const stopCi = () => act(async () => {
    await api.stopPipelineCi(detail.id);
  });
  const CI_TEXT: Record<CiWatch["status"], { text: string; tone: string }> = {
    watching: { text: "esperando os checks; se falharem, a esteira corrige com o log e manda ao mesmo pull request", tone: "" },
    passed: { text: "todos os checks passaram", tone: "ok" },
    fixing: { text: "falhou e a esteira está corrigindo; no fim, o app faz o commit e o push no ramo do pull request", tone: "warn" },
    failed: { text: "falhou", tone: "bad" },
    no_checks: { text: "o repositório não rodou checks neste pull request", tone: "" },
    stopped: { text: "não acompanhado", tone: "" },
  };
  const stopWaiting = () => act(async () => {
    await api.stopPipelineWaiting(detail.id);
  });
  const retry = () => act(async () => {
    await api.retryPipeline(detail.id);
    follow = true;
  });
  const deliver = () => act(async () => {
    await api.deliverPipeline(detail.id, delivery, commitMessage, push && canPush);
    await loadDetail();
  });
  const DELIVERED_LABEL: Record<Delivery, string> = {
    main: "Commit feito",
    branch: "Commit feito no ramo",
    pull_request: "Pull request aberto",
    pull_request_merge: "Pull request aberto e juntado",
  };
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

  /** Perguntas da última Triagem (com opções, quando ela as trouxe). */
  const questions = $derived(triageQuestions(passes.triage.at(-1)?.output ?? ""));

  /** Respostas às perguntas da Triagem; sem `answers`, cada pergunta com a sua linha de resposta em branco. */
  function answersTemplate(answers?: [string, string][]): string {
    // O "(Recomendado)" do rótulo é só etiqueta da tela; não vai para o pedido.
    const list: [string, string][] = answers?.map(([q, a]) => [q, withoutRecommended(a)])
      ?? questions.map((q) => [q.question, ""]);
    if (!list.length) return "Respostas às dúvidas da Triagem:\n- ";
    return `Respostas às dúvidas da Triagem:\n\n${list.map(([q, a], i) => `${i + 1}. ${q}\n   Resposta: ${a}`).join("\n\n")}`;
  }

  function complete(answers?: [string, string][]) {
    // O pedido volta ao formulário com espaço para responder ao que a Triagem achou vago.
    // A task escolhida segue escolhida; o título igual à chave é o padrão e não precisa voltar.
    const key = detail.jira_key;
    pipelines.draft = {
      projectId: detail.project_id,
      title: key && detail.title === key ? "" : detail.title,
      request: `${detail.request}\n\n${answersTemplate(answers)}`.trimStart(),
      images: $state.snapshot(images),
      task: key ? { key, site: detail.jira_url?.split("/browse/")[0] ?? null, issue: null } : null,
      design: detail.design,
      createJira: detail.create_jira,
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
        {:else if detail.mode === "in_place"}
          <span class="m">{@html icon("branch", 13)}Direto na pasta{#if detail.base_branch}<code>{detail.base_branch}</code>{/if}</span>
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
        <span class="meter" class:hot={budgetPercent > 85} aria-hidden="true"><span style:width={`${budgetPercent.toFixed(1)}%`}></span></span>
      </div>
      <div class="num" title={totalTokens(detail.usage) ? `Todas as etapas somadas, contadas quando cada turno termina.\n${usageHint(detail.usage)}` : "Contados quando o primeiro turno de uma etapa termina."}>
        <span class="label">Tokens</span>
        <span class="value"><Odometer value={tokNum} />{#if tokUnit}<small>{tokUnit}</small>{/if}{#if cost}<small class="usd">US$ <Odometer value={cost.replace(/^US\$\s*/, "").replace(/\+$/, "")} />{cost.endsWith("+") ? "+" : ""}</small>{/if}</span>
      </div>
      <div class="num">
        <span class="label">Voltas</span>
        <span class="value">
          <span class="pips" class:limit={detail.round_limit && detail.status === "failed"} aria-hidden="true">{#each Array.from({ length: detail.max_rounds }, (_, i) => i) as i (i)}<i class="pip" class:on={i < detail.rounds}></i>{/each}</span>
          {detail.rounds}<small>/{detail.max_rounds}</small>
        </span>
      </div>
      {#if active}
        <div class="cancel">
          <button type="button" class="btn danger" aria-haspopup="dialog" aria-expanded={confirmCancel} onclick={() => (confirmCancel = !confirmCancel)}>Cancelar</button>
          {#if confirmCancel}
            <div class="confirm" role="dialog" aria-label="Confirmar cancelamento">
              <strong>Cancelar a execução?</strong>
              <p>{detail.mode === "worktree" ? "O que já foi alterado fica na worktree, para você conferir ou descartar." : detail.mode === "in_place" ? "O que já foi alterado continua na pasta, sem commit: dá para descartar pelo git." : "Sem versionamento, o que já foi alterado continua na pasta de desenvolvimento."}</p>
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

  <section class="card esteira tone-{tone}" style:--bi={activeIndex} style:--bms="900ms" aria-label="Esteira de bots">
    <PipelineNow {detail} {now} {transit} note={nowNote} notch={ui.pipelineView !== "office"} actions={nowActions} />
    <div class="track-wrap" class:office={ui.pipelineView === "office"}>
      <div class="tools">
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
        <PipelineTrack {detail} {now} selected={panelRole} onselect={(role) => select(role)} ontransit={(t) => (transit = t)} />
      {/if}
    </div>
  </section>

  {#snippet nowActions()}
    {#if approval}
      {#if designLink}<button type="button" class="btn" title={designLink} onclick={() => openUrl(designLink!)}>Abrir design ↗</button>{/if}
      <button type="button" class="btn" aria-expanded={adjusting} onclick={() => { adjusting = !adjusting; select(approval.role, approval.id); }}>Pedir ajustes</button>
      <button type="button" class="btn warnp" disabled={busy} onclick={approve}>Aprovar e seguir</button>
    {:else if choice}
      <button type="button" class="btn" disabled={busy} title="Nada é corrigido: a esteira segue para o Relator" onclick={() => decideReview([], null)}>Seguir sem corrigir</button>
      <button type="button" class="btn warnp" disabled={busy || choiceDecision.undecided.length > 0 || (choiceDecision.picks.length === 0 && !choiceNote.trim())}
        title={choiceDecision.undecided.length ? `Escolha o caminho de ${choiceDecision.undecided.map((f) => f.id).join(", ")} no painel.` : undefined}
        onclick={() => decideReview(choiceDecision.picks, choiceNote.trim() || null)}>
        {choiceDecision.picks.length ? `Corrigir ${choiceDecision.picks.length === 1 ? "o escolhido" : `os ${choiceDecision.picks.length} escolhidos`}` : choiceFound.length ? "Marque um achado" : "Enviar ao Implementador"}
      </button>
    {:else if detail.status === "done"}
      <button type="button" class="btn" aria-expanded={reopening} onclick={() => (reopening = !reopening)}>Reabrir com um ajuste</button>
      <button type="button" class="btn primary" onclick={() => select("reporter")}>Ver relatório</button>
    {:else if detail.status === "failed" || detail.status === "cancelled"}
      <button type="button" class="btn primary" disabled={busy} onclick={retry}>Retomar execução</button>
    {/if}
  {/snippet}

  <div class="bottom">
    <div class="main">
      {#if !active}
        <section class="card outcome" class:ok={detail.status === "done"} class:bad={detail.status === "failed" || detail.status === "needs_info"} aria-label="Resultado da execução">
          {#if detail.status === "done"}
            <div class="o-top"><span class="pill ok">Concluída</span><h2>Pronto para você conferir</h2></div>
            <ul class="o-body">
              <li>{detail.mode === "worktree" ? `Alterações no ramo ${detail.branch}, em ${detail.workdir}` : detail.mode === "in_place" ? `Alterações direto em ${detail.workdir}, ainda sem commit` : "Alterações feitas direto na pasta de desenvolvimento"}</li>
              <li>{detail.rounds === 0 ? "O Revisor aprovou de primeira" : `O trabalho voltou ${detail.rounds} ${detail.rounds === 1 ? "vez" : "vezes"} antes de ser aprovado`}</li>
              {#if detail.jira_key}
                <li>
                  {createdTask ? "Task criada no Jira pelo Relator:" : "Solução escrita pelo Relator na task do Jira:"}
                  {#if detail.jira_url}<button type="button" class="lnk inline" onclick={() => openUrl(detail.jira_url!)}>{detail.jira_key} ↗</button>{:else}<b>{detail.jira_key}</b>{/if}
                </li>
                {#if jiraResult}<li class:warn-text={jiraResult.failed}>{jiraResult.failed ? `Falhou no Jira: ${jiraResult.text}` : `No Jira: ${jiraResult.text}`}</li>{/if}
              {:else if !detail.skipped.includes("reporter")}
                <li class="warn-text">O Relator não criou a task no Jira{jiraResult?.failed ? `: ${jiraResult.text}` : "; veja o motivo no relatório dele"}.</li>
              {/if}
            </ul>
            {#if detail.delivered}
              {@const d = detail.delivered}
              <div class="commit delivered">
                <strong>{DELIVERED_LABEL[d.kind]}</strong>
                <span class="muted small">
                  No ramo <code>{d.branch}</code>{d.pushed ? ", enviado ao remoto" : ", só no seu computador"}, às {formatTime(d.at)}.
                  {#if d.worktree_removed}A cópia isolada foi apagada.{:else if d.kind === "main" && detail.mode === "worktree"}A cópia isolada não saiu (ex.: arquivo aberto): apague-a quando quiser.{/if}
                  {#if (d.kind === "branch" || d.kind === "pull_request") && detail.mode === "in_place"}A pasta do projeto ficou no ramo <code>{d.branch}</code>.{/if}
                </span>
                {#if d.pr_url}<button type="button" class="lnk inline" onclick={() => openUrl(d.pr_url!)}>{d.pr_url} ↗</button>{/if}
                {#if detail.ci}
                  {@const ci = detail.ci}
                  <span class="ci {CI_TEXT[ci.status].tone}" role="status">
                    <b>CI:</b> {CI_TEXT[ci.status].text}{ci.failed.length && ci.status !== "passed" ? ` (falhou: ${ci.failed.join(", ")})` : ""}.
                    {#if ci.fixes}A esteira já corrigiu {ci.fixes} {ci.fixes === 1 ? "vez" : "vezes"}.{/if}
                    {#if ci.note}{ci.note}{/if}
                    {#if ci.status === "watching"}<button type="button" class="lnk inline" disabled={busy} onclick={stopCi}>Parar de acompanhar</button>{/if}
                  </span>
                {/if}
              </div>
            {:else if detail.mode !== "direct"}
              <div class="commit" role="group" aria-label="Entrega do resultado">
                <span class="label">Como entregar</span>
                <div class="deliveries">
                  {#each DELIVERY_OPTIONS as o (o.id)}
                    <label class="delivery" class:on={delivery === o.id} class:off={!!o.disabled} title={o.disabled ?? o.hint}>
                      <input type="radio" name="pl-delivery" value={o.id} bind:group={delivery} disabled={!!o.disabled} />
                      <span><b>{o.label}</b><span class="muted small">{o.disabled ?? o.hint}</span></span>
                    </label>
                  {/each}
                </div>
                <label class="label" for="pl-commit">Mensagem do commit</label>
                <div class="row-w">
                  <input id="pl-commit" class="input grow" bind:value={commitMessage} />
                  <label class="push" title={canPush ? "Envia o ramo ao remoto depois do commit" : "O repositório não tem remoto"}>
                    <input type="checkbox" checked={push || opensPr(delivery)} disabled={!canPush || opensPr(delivery)} onchange={(e) => (push = e.currentTarget.checked)} />
                    Fazer push
                  </label>
                  <button type="button" class="btn primary" disabled={busy || !commitMessage.trim() || (opensPr(delivery) && !canPr)} onclick={deliver}>Entregar</button>
                </div>
                <p class="muted small">Os bots não fazem commit nem push: só o que você escolher aqui. A opção marcada e o push padrão se mudam na tela do projeto.</p>
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
                <textarea id="pl-adjust" class="input area" rows="3" bind:value={adjustment} onpaste={onAdjustPaste}
                  placeholder="Ex.: o botão Exportar gera o arquivo vazio quando a conversa tem imagens. Cole prints aqui."
                  onkeydown={(e) => e.key === "Enter" && (e.metaKey || e.ctrlKey) && reopen()}></textarea>
                {#if adjustImages.length}
                  <div class="thumbs" role="list" aria-label="Prints colados no ajuste">
                    {#each adjustImages as image, i (i)}
                      <span class="thumb" role="listitem" title={image.name}>
                        <img src={image.preview} alt={image.name} />
                        <button type="button" aria-label={`Remover a imagem ${image.name}`} title={`Remover a imagem ${image.name}`} onclick={() => (adjustImages = adjustImages.filter((_, j) => j !== i))}>×</button>
                      </span>
                    {/each}
                  </div>
                {/if}
                <div class="row-w">
                  <button type="button" class="btn primary small" disabled={busy || (!adjustment.trim() && !adjustImages.length)} onclick={reopen}>Enviar aos bots</button>
                  <button type="button" class="btn small" onclick={() => (reopening = false)}>Voltar</button>
                </div>
                <p class="muted small">
                  O Implementador corrige a partir do seu texto{adjustImages.length ? " e dos prints" : ""}, o Revisor e o Testador conferem de novo e o Relator atualiza a Solução na
                  {detail.jira_key ? ` mesma task ${detail.jira_key}` : " task do Jira (sem criar outra, se ela já existir)"}.
                </p>
              </div>
            {/if}
          {:else if detail.status === "needs_info"}
            <div class="o-top"><span class="pill bad">Precisa de informação</span><h2>A Triagem achou a task vaga demais para seguir</h2></div>
            <div class="o-body md">{@html renderMarkdown(triageSummary(passes.triage.at(-1)?.output ?? ""))}</div>
            {#if questions.length}
              <div class="o-body">
                <QuestionCard {questions} title="Responda às dúvidas da Triagem" submitLabel="Completar o pedido" cancelLabel="Escrever do meu jeito" allowPartial
                  onanswer={(answers) => complete(answers)} oncancel={() => complete()} />
              </div>
            {:else}
              <div class="row-w acts"><button type="button" class="btn primary" onclick={() => complete()}>Completar o pedido</button></div>
            {/if}
          {:else}
            <div class="o-top"><span class="pill {status.tone}">{status.label}</span><h2>{detail.status === "cancelled" ? "Execução cancelada por você" : "A esteira parou"}</h2></div>
            {#if detail.note}<p class="o-body">{detail.note}</p>{/if}
            {#if detail.waiting_account && accounts.flags.resume_on_reset}
              <p class="o-body">{detail.resume_at ? `A primeira conta volta às ${formatTime(detail.resume_at)}; a esteira retoma sozinha logo depois.` : "A esteira retoma sozinha assim que uma conta voltar."}</p>
            {/if}
            <p class="o-body muted">{detail.mode === "worktree" ? `O que já foi alterado continua na worktree ${detail.branch}.` : "O que já foi alterado continua na pasta de desenvolvimento."} {detail.round_limit ? "Ao retomar, a contagem de voltas recomeça e o Implementador corrige o que foi apontado por último." : "Ao retomar, a etapa que parou roda de novo."}</p>
            <div class="row-w acts">
              <button type="button" class="btn primary" disabled={busy} onclick={retry}>Retomar execução</button>
              {#if detail.waiting_account && accounts.flags.resume_on_reset}
                <button type="button" class="btn" disabled={busy} onclick={stopWaiting}>Não retomar sozinha</button>
              {/if}
            </div>
          {/if}
        </section>
      {/if}

      <section class="card log-card" aria-label="Registro da execução">
        <div class="r-top"><h2>Registro</h2><span class="muted small">cada etapa roda numa sessão própria; o que recebeu e o que entregou ficam guardados</span></div>
        <ol class="reg" bind:this={regEl} onscroll={onRegScroll}>
          {#if olderCount}
            <li class="older">
              <button type="button" class="older-btn" aria-expanded={olderOpen} onclick={() => (olderOpen = !olderOpen)}>
                {@html icon("chev", 13)}<b>{olderCount} passagens anteriores</b>
                <span class="dim">{formatWhen(olderStages[0].started_at, now * 1000)} → {formatWhen(olderStages[olderCount - 1].started_at, now * 1000)}</span>
              </button>
              <div class="fold" class:open={olderOpen}>
                <div>
                  <div class="ochips">
                    {#each olderChips as c (c.role)}
                      <button type="button" class="ochip" tabindex={olderOpen ? 0 : -1} onclick={() => select(c.role)}>{STATION[c.role].name} <b>{c.n}</b></button>
                    {/each}
                  </div>
                </div>
              </div>
            </li>
          {/if}
          {#each shownStages as s, i (s.id)}
            {@const v = stageVisual(s, s.status === "running" && !!chat.permissions[s.conversation_id])}
            {@const tag = passLabel(s, indexOf(s), detail.max_rounds)}
            {@const at = olderCount + i}
            <li class="reg-row" class:fresh={at >= introCount && at === detail.stages.length - 1} style:animation-delay={at < introCount ? `${Math.min(i, 14) * 30 + 380}ms` : "0ms"}>
              <button type="button" class="reg-item" class:sel={panelStageId === s.id} aria-pressed={panelStageId === s.id} onclick={() => select(s.role, s.id)}>
                <span class="r-time" title={fullDate(s.started_at)}>{formatTime(s.started_at)}</span>
                <span class="r-mark"><span class="node mini s-{v.visual}" class:gate={isUserStep(s.role)}>{@html stationGlyph(s.role, v.visual)}</span></span>
                <span class="r-body">
                  <span class="r-l1">
                    <b>{STATION[s.role].name}</b>{#if tag}<span class="tag">{tag}</span>{/if}
                    <span class="r-right"><span class="mono">{duration(stageSeconds(s))}</span>{#if detail.stage_usage[s.id]}<span class="sep">·</span><span class="mono" title={usageHint(detail.stage_usage[s.id])}>{tokenCount(totalTokens(detail.stage_usage[s.id]))} tok</span>{/if}<span class="sep">·</span>{stageAccount(s)}{#if STATION[s.role].model}<span class="sep">·</span>{detail.agents[s.role]?.model ? `${detail.agents[s.role]?.model} no Claude` : STATION[s.role].model}{/if}</span>
                  </span>
                  <span class="r-l2"><span class="io">{inputSummary(s, detail)}</span>{@html icon("arrow", 11)}<span class="io out o-{v.visual}" class:shim={v.visual === "working"} title={outputSummary(s)}>{outputSummary(s)}</span></span>
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
      {@const list = passes[panelStation.role]}
      {@const pIdx = panelStage ? list.indexOf(panelStage) : -1}
      {@const live = panelStage?.status === "running"}
      {@const user = isUserStep(panelStation.role)}
      {@const stop = !!panelStage && ((!!approval && panelStage.id === approval.id) || (!!choice && panelStage.id === choice.id))}
      <aside class="card panel" aria-label="Detalhes da estação">
        <header class="p-top">
          <div class="p-l1">
            <span class="p-ava s-{panelVisual?.visual ?? 'waiting'}" aria-hidden="true">{@html roleIcon(panelStation.role, 16)}</span>
            <h2>{panelStation.name}</h2>
            {#if panelVisual}<span class="chip c-{panelVisual.visual}"><i></i>{panelVisual.label}</span>{/if}
            {#if !follow && latest && active}
              <button type="button" class="lnk p-follow" onclick={() => { follow = true; }}>Voltar para a etapa ativa</button>
            {/if}
          </div>
          <div class="p-meta">
            {#if panelAgent}<span class="pm">agente <b>{panelAgent.name}</b></span>{/if}
            {#if panelModel}<span class="pm">modelo <b>{panelModel}</b></span>{:else if panelStation.model}<span class="pm">{panelStation.model.toLowerCase()}</span>{/if}
            {#if panelEffort}<span class="pm">esforço <b>{panelEffort}</b></span>{/if}
            <span class="pm">{panelStation.perm}</span>
            {#if panelStage && !isUserStep(panelStage.role)}<span class="pm">conta <b>{stageAccount(panelStage)}</b></span>{/if}
            {#if panelStage}<span class="pm mono">{duration(stageSeconds(panelStage))}</span>{/if}
            {#if panelStage && detail.stage_usage[panelStage.id]}<span class="pm mono" title={usageHint(detail.stage_usage[panelStage.id])}>{usageText(detail.stage_usage[panelStage.id])}</span>{/if}
          </div>
          {#if list.length > 1}
            <div class="p-pass">
              <div class="pdots" role="group" aria-label="Passagens desta estação">
                {#each list as s, i (s.id)}
                  {@const v = stageVisual(s, false)}
                  {@const lab = `${passLabel(s, i, detail.max_rounds) ?? "1ª passagem"} · ${v.label}`}
                  <button type="button" class="pd s-{v.visual}" class:sel={s.id === panelStageId} title={lab} aria-label={lab} aria-pressed={s.id === panelStageId} onclick={() => select(s.role, s.id)}><span></span></button>
                {/each}
              </div>
              <div class="p-nav">
                <button type="button" class="btn icon" aria-label="Passagem anterior" disabled={pIdx <= 0} onclick={() => select(list[pIdx - 1].role, list[pIdx - 1].id)}>
                  <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 6l-6 6 6 6" /></svg>
                </button>
                <span class="lbl">{#if panelStage && pIdx >= 0}{passLabel(panelStage, pIdx, detail.max_rounds) ?? "1ª passagem"} <span class="cnt">{pIdx + 1}/{list.length}</span>{/if}</span>
                <button type="button" class="btn icon" aria-label="Próxima passagem" disabled={pIdx < 0 || pIdx >= list.length - 1} onclick={() => select(list[pIdx + 1].role, list[pIdx + 1].id)}>
                  <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 6l6 6-6 6" /></svg>
                </button>
              </div>
            </div>
          {/if}
        </header>
        <div class="p-body">
          {#key panelStageId ?? panelRole}
          <div class="p-anim">
          {#if !panelStage}
            <section class="p-role">
              <h3 class="sec-h">O que faz</h3><p>{panelStation.does}</p>
              <div class="role-grid"><div><span class="lab">Recebe</span><span class="v">{panelStation.receives}</span></div><div><span class="lab">Entrega</span><span class="v">{panelStation.delivers}</span></div></div>
            </section>
            <p class="p-out muted">{detail.skipped.includes(panelStation.role) ? "Desligada neste projeto: a esteira passa direto por ela." : active ? "Ainda não recebeu o trabalho." : "Não rodou nesta execução."}</p>
          {:else}
            {#if panelAsking && panelPermission}
              {@const cid = panelStage.conversation_id}
              <!-- O mesmo pedido que apareceria no chat, respondido aqui sem abrir a conversa da etapa. -->
              <div class="ask-card">
                <p class="ask">{@html icon("you", 13)}<span>A etapa pede sua autorização.</span>
                  <button type="button" class="btn small" onclick={() => openStageConversation(cid)}>Abrir conversa</button></p>
                {@render askBody(cid, panelPermission)}
              </div>
            {/if}
            <section>
              <div class="pio">
                <div class="pio-box"><span class="lab">Entrada</span><span class="v">{inputSummary(panelStage, detail)}</span></div>
                <span class="pio-arrow" aria-hidden="true">{@html icon("arrow", 13)}</span>
                <div class="pio-box o-{panelVisual?.visual ?? 'waiting'}">
                  <span class="lab">{live ? (user ? "Falta" : "Vai entregar") : "Entregou"}</span>
                  <span class="v">{live ? (panelStage.role === "review_choice" ? "O que corrigir e como." : panelStage.role === "design_approval" ? "A sua decisão sobre o design." : user ? "A sua decisão sobre o plano." : panelStation.delivers) : outputSummary(panelStage)}</span>
                </div>
              </div>
              {#if !isMarker(panelStage.role) && !stop}
                <div class="entry-t"><button type="button" class="lnk" aria-expanded={entryOpen} onclick={() => (entryOpen = !entryOpen)}>{entryOpen ? "Esconder a entrada completa" : "Ver a entrada completa"}</button></div>
                <div class="fold" class:open={entryOpen}>
                  <div>
                    <div class="entry">
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
                    </div>
                  </div>
                </div>
              {/if}
            </section>
            {#if approval && panelStage.id === approval.id}
              <section>
                <h3 class="sec-h">{designing ? "O design proposto" : "O plano proposto"}</h3>
                <div class="plan md">{@html renderMarkdown(designing ? designSpec(approval.input) || "O Designer não escreveu a especificação; abra o design." : approval.input)}</div>
              </section>
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
              <div class="p-acts">{@render nowActions()}</div>
            {:else if choice && panelStage.id === choice.id}
              <ReviewChoiceCard stage={choice} review={choiceReview} {busy} bind:marked={choiceMarked} bind:chosen={choiceChosen} bind:note={choiceNote} ondecide={decideReview} />
            {/if}
            {#if !isUserStep(panelStage.role)}
              <section>
                <h3 class="sec-h">{live ? "Agora" : "O que fez"}{#if panelLog.length + subLog.length}<span class="n">{panelLog.length + subLog.length}</span>{/if}</h3>
                <ol class="tlog" bind:this={logEl} onscroll={onLogScroll}>
                  {#each panelLog as l, i (i)}
                    <li class="ln {l.tone}" class:say={l.verb === "Escreveu"} class:fresh={i >= logIntro && i === panelLog.length - 1} style:animation-delay={i < logIntro ? `${Math.min(i, 14) * 22}ms` : "0ms"} title={l.title ?? l.target}>
                      <span class="t" title={fullDate(l.at)}>{formatTime(l.at)}</span><span class="i">{@html icon(l.glyph, 12)}</span><span class="va"><span class="v">{l.verb}</span> <span class="a">{l.target}</span></span>
                    </li>
                  {/each}
                  {#each subLog as l, i (i)}
                    <li class="ln sub {l.tone}" title={l.title ?? l.target}><span class="t"></span><span class="i">{@html icon(l.glyph, 12)}</span><span class="va"><span class="v">{l.verb}</span> <span class="a">{l.target}</span></span></li>
                  {/each}
                  {#if live}
                    <li class="ln live"><span class="t"></span><span class="i">{#if panelAsking}<span class="dot warn pulse"></span>{:else}<span class="spinner mini"></span>{/if}</span><span class="va"><span class="a">{panelAsking ? "Esperando sua autorização" : transit ? "Passando o trabalho adiante…" : (liveStatus ?? "Trabalhando")}</span></span></li>
                  {:else if panelLog.length === 0}
                    <li class="ln"><span class="t"></span><span class="i"></span><span class="va muted">Sem ferramentas registradas.</span></li>
                  {/if}
                </ol>
              </section>
            {/if}
            {#if !live}
              <section>
                <h3 class="sec-h">{panelStage.role === "feedback" ? "O que você pediu" : panelStage.role === "resume" ? "O que aconteceu" : "Saída"}</h3>
                {#if panelStage.output}
                  {#if panelStage.status === "failed"}<p class="failed">{panelStage.output}</p>{:else}<div class="md">{@html renderMarkdown(panelStage.output)}</div>{/if}
                {:else}
                  <p class="p-out muted">Nada entregue.</p>
                {/if}
              </section>
            {/if}
            <section class="p-role">
              <h3 class="sec-h">O que faz</h3><p>{panelStation.does}</p>
              <div class="role-grid"><div><span class="lab">Recebe</span><span class="v">{panelStation.receives}</span></div><div><span class="lab">Entrega</span><span class="v">{panelStation.delivers}</span></div></div>
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
          {/key}
        </div>
      </aside>
    {/if}
  </div>
</div>

{#snippet askBody(cid: string, request: PermissionRequest)}
  {#if request.tool === "AskUserQuestion"}
    <QuestionCard questions={questionsOf(request)} onanswer={(answers) => answerStage(cid, true, false, { answers })} oncancel={() => answerStage(cid, false, false)} />
  {:else if request.tool === "ExitPlanMode"}
    <PlanCard {request} ondecide={(mode) => (mode ? answerStage(cid, true, false, { nextMode: mode }) : answerStage(cid, false, false))} />
  {:else}
    <PermissionCard {request} ondecide={(allow, always) => answerStage(cid, allow, always)} />
  {/if}
{/snippet}

<svelte:window onkeydown={(e) => {
  if (e.key !== "Escape") return;
  if (zoomed) zoomed = null;
  else if (askModal && !(e.target instanceof HTMLElement && e.target.closest("input, textarea"))) laterId = askModal.request.requestId;
}} />

{#if askModal && !zoomed}
  {@const cid = askModal.stage.conversation_id}
  <div class="ask-modal" role="presentation">
    <div class="card ask-dialog" role="dialog" aria-modal="true" tabindex="-1" use:trapFocus aria-label={`${STATION[askModal.stage.role].name} pede sua resposta`}>
      <header class="ask-head">
        <span class="p-ava s-you" aria-hidden="true">{@html roleIcon(askModal.stage.role, 16)}</span>
        <h2>{STATION[askModal.stage.role].name} pede sua resposta</h2>
        <button type="button" class="btn small" onclick={() => openStageConversation(cid)}>Abrir conversa</button>
        <button type="button" class="btn small" title="O pedido continua no painel da estação" onclick={() => askModal && (laterId = askModal.request.requestId)}>Responder depois</button>
      </header>
      <div class="ask-scroll">
        {@render askBody(cid, askModal.request)}
      </div>
    </div>
  </div>
{/if}

{#if zoomed}
  <button type="button" class="zoom" aria-label="Fechar a imagem" onclick={() => (zoomed = null)}>
    <img src={imageSrc(zoomed)} alt={zoomed.name} />
  </button>
{/if}

<style>
  .screen { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 14px; padding: 18px 24px; }
  .screen > * { width: 100%; max-width: 1440px; margin-inline: auto; }
  .head { flex: none; display: flex; flex-wrap: wrap; align-items: flex-end; gap: 12px 28px; animation: fade-down 0.6s var(--ease) backwards; }
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
  .meter span { display: block; height: 100%; border-radius: 2px; background: var(--accent); transition: width 1s linear, background-color 0.4s; }
  .meter.hot span { background: var(--warn); }
  .value { display: inline-flex; align-items: center; gap: 6px; }
  .value small.usd { display: inline-flex; align-items: center; gap: 4px; font-family: var(--mono); line-height: 1.35; }
  .pips { display: inline-flex; align-items: center; gap: 4px; margin-right: 2px; }
  .pip { display: block; width: 8px; height: 8px; border: 1.5px solid var(--border-strong); border-radius: 50%; transition: background-color 0.35s, border-color 0.35s, transform 0.45s var(--spring); }
  .pip.on { background: var(--accent-text); border-color: var(--accent-text); transform: scale(1.1); }
  .pips.limit { animation: shake 0.55s ease; }
  .pips.limit .pip { background: var(--bad); border-color: var(--bad); }
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
  .btn.warnp { border-color: transparent; background: var(--warn); color: #1b1406; font-weight: 600; }
  .btn.warnp:hover:not(:disabled) { background: var(--warn-btn-hover); }
  .row-w { display: flex; align-items: center; gap: 8px 10px; flex-wrap: wrap; }
  .grow { flex: 1; min-width: 200px; }

  /* Cartão da esteira: o Agora em cima e o trilho; a cor do momento vem do tom. */
  .esteira { flex: none; position: relative; padding: 14px 16px 4px; animation: rise 0.7s var(--ease) 0.08s backwards; }
  .esteira::before { content: ""; position: absolute; inset: 0; border-radius: inherit; background-image: radial-gradient(rgba(255, 255, 255, 0.05) 1px, transparent 1.3px); background-size: 18px 18px; -webkit-mask-image: linear-gradient(to bottom, transparent 0, #000 38%, #000 86%, transparent); mask-image: linear-gradient(to bottom, transparent 0, #000 38%, #000 86%, transparent); pointer-events: none; }
  .tone-live { --hc: var(--accent); --hc-text: var(--accent-text); --hc-bg: var(--acc-bg); --hc-line: var(--acc-line); --hc-glow: var(--acc-glow); --hc-solid: var(--acc-solid); }
  .tone-you { --hc: var(--warn); --hc-text: var(--warn-text); --hc-bg: var(--warn-bg); --hc-line: var(--warn-line); --hc-glow: var(--warn-glow); --hc-solid: var(--warn-solid); }
  .tone-bad { --hc: var(--bad); --hc-text: var(--danger-text); --hc-bg: var(--danger-bg); --hc-line: var(--bad-line); --hc-glow: var(--bad-glow); --hc-solid: var(--bad-solid); }
  .tone-ok { --hc: var(--ok); --hc-text: var(--ok-text); --hc-bg: var(--ok-bg); --hc-line: var(--ok-line); --hc-glow: var(--ok-glow); --hc-solid: var(--ok-solid); }
  .tone-off { --hc: var(--muted); --hc-text: var(--text-2); --hc-bg: var(--surface-2); --hc-line: var(--border-strong); --hc-glow: transparent; --hc-solid: var(--surface-2); }
  .track-wrap { position: relative; margin: 12px -16px 0; }
  .tools { position: absolute; top: 0; right: 16px; z-index: 8; display: flex; gap: 8px; }
  .track-wrap.office { padding: 0 16px; }
  .track-wrap.office .tools { position: static; justify-content: flex-end; margin-bottom: 8px; }
  .legend { position: absolute; right: 16px; top: 38px; z-index: 20; width: 330px; max-width: calc(100% - 32px); padding: 12px 14px; border: 1px solid var(--border-strong); border-radius: 10px; background: var(--surface-2); box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5); }
  .legend ul { list-style: none; padding: 0; margin: 8px 0 0; display: flex; flex-direction: column; gap: 7px; }
  .legend li { display: grid; grid-template-columns: 18px minmax(0, 1fr); gap: 9px; align-items: center; font-size: 12.5px; color: var(--text-2); }
  .legend b { color: var(--text); font-weight: 600; }
  .legend .foot { margin-top: 10px; padding-top: 9px; border-top: 1px solid var(--border-strong); font-size: 12px; color: var(--muted); }

  .node.mini { position: relative; display: grid; place-items: center; width: 16px; height: 16px; flex: none; border: 1.5px solid var(--border-strong); border-radius: 50%; background: var(--bg-side); color: var(--faint); }
  .node.mini :global(svg) { display: block; }
  .node.mini.gate { width: 13px; height: 13px; margin: 1.5px; border-radius: 4px; transform: rotate(45deg); }
  .node.mini.gate :global(svg) { transform: rotate(-45deg); }
  .node.mini.s-done { background: var(--ok-solid); border-color: var(--ok-line); color: var(--ok-text); }
  .node.mini.s-returned, .node.mini.s-failed { background: var(--bad-solid); border-color: var(--bad-line); color: var(--danger-text); }
  .node.mini.s-you { background: var(--warn-solid); border-color: var(--warn); color: var(--warn-text); }
  .node.mini.s-skipped { border-style: dashed; }
  .node.mini.s-working { border-color: var(--border-strong); border-top-color: var(--accent); animation: spin 0.9s linear infinite; }
  .node.mini.s-working :global(svg) { display: none; }

  .bottom { flex: 1 1 auto; min-height: 400px; display: grid; grid-template-columns: minmax(0, 1fr) minmax(380px, 470px); gap: 14px; }
  .main { min-width: 0; min-height: 0; display: flex; flex-direction: column; gap: 14px; }

  .adjust { display: flex; flex-direction: column; gap: 6px; }
  .adjust .thumbs { margin-bottom: 0; }
  .adjust .thumb { position: relative; cursor: default; }
  .adjust .thumb button { position: absolute; top: 3px; right: 3px; width: 18px; height: 18px; padding: 0; border: 0; border-radius: 50%; background: rgba(0, 0, 0, 0.7); color: #fff; font-size: 13px; line-height: 1; }
  .area { width: 100%; min-height: 72px; padding: 9px 11px; resize: vertical; line-height: 1.5; }
  .plan { padding: 12px 14px; max-height: 52vh; overflow-y: auto; border: 1px solid var(--warn-line); border-radius: 10px; background: var(--bg); }

  .outcome { padding: 14px 16px 16px; }
  .outcome.ok { border-color: rgba(88, 184, 138, 0.4); }
  .outcome.bad { border-color: rgba(239, 164, 155, 0.4); }
  .o-top { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .o-body { margin: 8px 0 0; font-size: 13px; color: var(--text-body); }
  ul.o-body { padding-left: 18px; display: flex; flex-direction: column; gap: 3px; }
  .commit { margin-top: 12px; display: flex; flex-direction: column; gap: 6px; }
  .commit.delivered { align-items: flex-start; gap: 2px; font-size: 13px; }
  .ci { font-size: 12.5px; color: var(--text-2); }
  .ci.ok b { color: var(--ok-text); }
  .ci.warn b { color: var(--warn-text); }
  .ci.bad b { color: var(--danger-text); }
  .deliveries { display: grid; grid-template-columns: repeat(auto-fit, minmax(190px, 1fr)); gap: 6px; margin-bottom: 4px; }
  .delivery { display: flex; align-items: flex-start; gap: 8px; padding: 8px 10px; border: 1px solid var(--hairline); border-radius: 8px; cursor: pointer; font-size: 13px; }
  .delivery > span { display: flex; flex-direction: column; gap: 2px; }
  .delivery.on { border-color: var(--accent, currentColor); }
  .delivery.off { opacity: 0.55; cursor: default; }
  .push { display: flex; align-items: center; gap: 6px; white-space: nowrap; font-size: 13px; }
  .outcome .acts { justify-content: flex-start; margin-top: 12px; }

  .log-card { flex: 1 1 120px; min-height: 120px; display: flex; flex-direction: column; padding: 12px 8px 8px; animation: rise 0.7s var(--ease) 0.22s backwards; }
  .r-top { display: flex; align-items: baseline; gap: 4px 10px; flex-wrap: wrap; padding: 0 8px; margin-bottom: 6px; }
  .reg { flex: 1; min-height: 0; overflow-y: auto; list-style: none; margin: 0; padding: 0; }
  .reg .empty { padding: 6px 8px; }
  .older { margin: 2px 0 6px; }
  .older-btn { width: 100%; display: flex; align-items: center; gap: 8px; padding: 8px 10px; border: 1px dashed var(--border-strong); border-radius: 10px; background: transparent; color: var(--text-2); font-size: 12.5px; text-align: left; transition: background-color 0.2s; }
  .older-btn:hover { background: var(--surface); }
  .older-btn :global(svg) { flex: none; transition: transform 0.35s var(--ease); }
  .older-btn[aria-expanded="true"] :global(svg) { transform: rotate(90deg); }
  .older-btn b { font-weight: 600; color: var(--text); }
  .older-btn .dim { margin-left: auto; font-size: 11.5px; color: var(--muted); }
  .fold { display: grid; grid-template-rows: 0fr; transition: grid-template-rows 0.45s var(--ease); }
  .fold.open { grid-template-rows: 1fr; }
  .fold > div { min-height: 0; overflow: hidden; }
  .ochips { display: flex; flex-wrap: wrap; gap: 6px; padding: 10px 2px 4px; }
  .ochip { display: inline-flex; align-items: center; gap: 6px; height: 28px; padding: 0 10px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface); color: var(--text-2); font-size: 12px; transition: border-color 0.2s; }
  .ochip b { font: 600 11.5px var(--mono); color: var(--text); }
  .ochip:hover { border-color: var(--border-strong); }
  .reg-row { animation: row-in 0.5s var(--ease) backwards; }
  .reg-row.fresh .reg-item { animation: row-flash 2s ease-out; }
  .reg-item { display: grid; grid-template-columns: 40px 18px minmax(0, 1fr); gap: 0 8px; align-items: start; width: 100%; padding: 6px 8px; border: 0; border-radius: 10px; background: none; color: inherit; text-align: left; transition: background-color 0.25s, box-shadow 0.25s; }
  .reg-item:hover { background: var(--surface); }
  .reg-item.sel { background: var(--surface-2); box-shadow: inset 0 0 0 1px var(--border-strong); }
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
  .o-working { color: var(--accent-text) !important; }
  .shim { background: linear-gradient(90deg, var(--accent-text) 0%, var(--text) 45%, var(--accent-text) 90%); background-size: 200% 100%; -webkit-background-clip: text; background-clip: text; color: transparent !important; animation: shimmer 1.8s linear infinite; }

  .panel { min-width: 0; min-height: 0; max-height: 78vh; display: flex; flex-direction: column; overflow: hidden; position: sticky; top: 0; animation: rise 0.7s var(--ease) 0.3s backwards; }
  .p-top { flex: none; padding: 12px 14px 11px; border-bottom: 1px solid var(--border); }
  .p-l1 { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .p-l1 h2 { font-size: 15px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .p-ava { flex: none; width: 32px; height: 32px; display: grid; place-items: center; border: 1px solid var(--border); border-radius: 10px; background: var(--surface-2); color: var(--text-2); transition: background-color 0.4s, border-color 0.4s, color 0.4s; }
  .p-ava :global(svg) { display: block; }
  .p-ava.s-working { background: var(--acc-solid); border-color: var(--acc-line); color: var(--accent-text); }
  .p-ava.s-done { background: var(--ok-solid); border-color: var(--ok-line); color: var(--ok-text); }
  .p-ava.s-returned, .p-ava.s-failed { background: var(--bad-solid); border-color: var(--bad-line); color: var(--danger-text); }
  .p-ava.s-you { background: var(--warn-solid); border-color: var(--warn-line); color: var(--warn-text); }
  .p-follow { margin-left: auto; flex: none; align-self: center; }
  .chip { display: inline-flex; align-items: center; gap: 5px; flex: none; padding: 1px 8px 1px 7px; border: 1px solid transparent; border-radius: 999px; font-size: 11px; font-weight: 600; line-height: 1.55; white-space: nowrap; }
  .chip i { width: 6px; height: 6px; border-radius: 50%; background: currentColor; }
  .c-waiting { color: var(--muted); }
  .c-working { color: var(--accent-text); background: var(--acc-bg); }
  .c-done { color: var(--ok-text); background: var(--ok-bg); }
  .c-returned { color: var(--danger-text); border-color: var(--bad-line); }
  .c-failed { color: var(--danger-text); background: var(--danger-bg); }
  .c-skipped { color: var(--muted); border: 1px dashed var(--border-strong); }
  .c-you { color: var(--warn-text); background: var(--warn-bg); }
  .p-meta { display: flex; flex-wrap: wrap; margin-top: 8px; font-size: 12px; line-height: 1.7; color: var(--muted); }
  .pm b { font-weight: 600; color: var(--text-2); }
  .pm.mono { font-size: 11.5px; }
  .pm + .pm::before { content: "·"; margin: 0 7px; color: var(--faint); font-family: var(--font); }
  .p-pass { display: flex; align-items: center; gap: 10px; margin-top: 10px; }
  .pdots { flex: 1; min-width: 0; display: flex; flex-wrap: wrap; gap: 1px; }
  .pd { width: 14px; height: 14px; display: grid; place-items: center; padding: 0; border: 0; border-radius: 4px; background: transparent; }
  .pd span { display: block; width: 9px; height: 9px; border-radius: 3px; background: var(--border-strong); transition: transform 0.25s var(--spring), box-shadow 0.25s; }
  .pd:hover span { transform: scale(1.35); }
  .pd.s-done span { background: var(--ok); }
  .pd.s-returned span, .pd.s-failed span { background: var(--bad); }
  .pd.s-you span { background: var(--warn); }
  .pd.s-working span { background: var(--accent); animation: blink 1.2s ease-in-out infinite; }
  .pd.s-skipped span { background: var(--faint); }
  .pd.sel span { box-shadow: 0 0 0 2px var(--bg-side), 0 0 0 3.5px var(--text); transform: scale(1.15); }
  .p-nav { flex: none; display: flex; align-items: center; gap: 2px; }
  .p-nav .btn.icon { width: 26px; min-height: 26px; }
  .p-nav .lbl { min-width: 112px; text-align: center; font: 500 11.5px var(--mono); color: var(--text-2); white-space: nowrap; }
  .p-nav .cnt { color: var(--muted); }
  .p-body { flex: 1; min-height: 0; overflow-y: auto; padding: 14px 16px 18px; font-size: 13px; color: var(--text-body); }
  .p-anim { display: flex; flex-direction: column; gap: 16px; }
  .p-anim > :global(*) { animation: p-in 0.5s var(--ease) backwards; }
  .p-anim > :global(:nth-child(2)) { animation-delay: 0.05s; }
  .p-anim > :global(:nth-child(3)) { animation-delay: 0.1s; }
  .p-anim > :global(:nth-child(4)) { animation-delay: 0.15s; }
  .p-anim > :global(:nth-child(5)) { animation-delay: 0.2s; }
  .p-anim > :global(:nth-child(6)) { animation-delay: 0.25s; }
  .sec-h { display: flex; align-items: center; gap: 8px; margin: 0 0 8px; font-size: 11px; font-weight: 600; letter-spacing: 0.05em; text-transform: uppercase; color: var(--muted); }
  .sec-h .n { font: 500 10.5px var(--mono); letter-spacing: 0; color: var(--faint); }
  .lab { display: block; margin-bottom: 3px; font-size: 10px; font-weight: 600; letter-spacing: 0.05em; text-transform: uppercase; color: var(--muted); }
  .pio { display: grid; grid-template-columns: minmax(0, 1fr) 26px minmax(0, 1fr); align-items: stretch; }
  .pio-box { min-width: 0; padding: 9px 11px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); }
  .pio-box .v { display: block; font-size: 12.5px; line-height: 1.45; color: var(--text); text-wrap: pretty; overflow-wrap: anywhere; }
  .pio-box.o-working { border-color: var(--acc-line); }
  .pio-box.o-returned, .pio-box.o-failed { border-color: var(--bad-line); }
  .pio-box.o-done { border-color: var(--ok-line); }
  .pio-box.o-you { border-color: var(--warn-line); }
  .pio-box.o-working, .pio-box.o-returned, .pio-box.o-failed, .pio-box.o-done, .pio-box.o-you { color: inherit !important; }
  .pio-arrow { display: grid; place-items: center; color: var(--faint); }
  .entry-t { display: flex; margin-top: 8px; }
  .entry { padding-top: 8px; }
  .p-out { font-size: 13px; line-height: 1.6; color: var(--text); }
  .p-out.muted { font-size: 12.5px; color: var(--muted); }
  .p-role { padding: 12px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); }
  .p-role p { font-size: 12.5px; line-height: 1.55; color: var(--text-2); }
  .role-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-top: 10px; }
  .role-grid .v { font-size: 12px; line-height: 1.45; color: var(--text-2); }
  .p-acts { display: flex; flex-wrap: wrap; gap: 8px; }
  .ask-card { display: flex; flex-direction: column; gap: 8px; }
  .ask-card .ask button { margin-left: auto; }
  .ask { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; padding: 8px 10px; border: 1px solid rgba(217, 164, 65, 0.45); border-radius: 8px; background: var(--warn-bg); color: var(--warn-text); font-size: 12.5px; }
  .prompt { margin: 0; padding: 10px 12px; max-height: 280px; overflow: auto; border: 1px solid var(--border); border-radius: 10px; background: var(--bg); font-family: var(--mono); font-size: 11.5px; line-height: 1.6; white-space: pre-wrap; overflow-wrap: anywhere; color: var(--text-2); }
  .tlog { list-style: none; margin: 0; padding: 4px; max-height: 290px; overflow-y: auto; border: 1px solid var(--border); border-radius: 12px; background: var(--bg); }
  .ln { display: grid; grid-template-columns: 38px 16px minmax(0, 1fr); column-gap: 8px; align-items: start; padding: 4px 8px; border-radius: 7px; font-size: 12px; color: var(--muted); animation: ln-in 0.45s var(--ease) backwards; }
  .ln.fresh { animation: ln-in 0.45s var(--ease) backwards, ln-flash 1.6s ease-out; }
  .ln .t { font: 400 10.5px/18px var(--mono); font-variant-numeric: tabular-nums; color: var(--muted); }
  .ln .i { display: grid; place-items: center; height: 18px; color: var(--muted); }
  .ln .va { min-width: 0; line-height: 18px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ln .v { margin-right: 2px; font-weight: 500; color: var(--text-2); }
  .ln .a { font: 400 11.5px var(--mono); color: var(--text); }
  .ln.say .a { font: 400 12px var(--font); color: var(--text-2); }
  .ln.warn .i, .ln.warn .v, .ln.warn .a { color: var(--warn-text); }
  .ln.swap .i, .ln.swap .v { color: var(--warn-text); }
  .ln.swap .a, .ln.warn .a { font: 400 12px var(--font); }
  .ln.error .i, .ln.error .v, .ln.error .a { color: var(--danger-text); }
  .ln.live .a { font: 500 12px var(--font); color: var(--accent-text); }
  .ln.sub .i { opacity: 0.75; }
  .ln.sub .va { padding-left: 10px; border-left: 1px solid var(--border-strong); }
  .failed { color: var(--danger-text); }
  .thumbs { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px; }
  .thumb { padding: 0; cursor: zoom-in; width: 56px; height: 56px; border: 1px solid var(--border-strong); border-radius: 6px; overflow: hidden; background: var(--bg); }
  .thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .ask-modal { position: fixed; inset: 0; z-index: 70; display: grid; place-items: center; padding: 32px; background: rgba(0, 0, 0, 0.6); }
  .ask-dialog { width: min(820px, 100%); max-height: calc(100vh - 64px); display: flex; flex-direction: column; overflow: hidden; background: var(--surface); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.5); }
  .ask-dialog:focus { outline: none; }
  .ask-head { flex: none; display: flex; align-items: center; gap: 10px; padding: 14px 18px; border-bottom: 1px solid var(--border); }
  .ask-head h2 { margin: 0 auto 0 0; font-size: 16px; }
  .ask-scroll { overflow: auto; padding: 18px; font-size: 14px; }
  .zoom { position: fixed; inset: 0; z-index: 80; display: grid; place-items: center; padding: 32px; border: 0; background: rgba(0, 0, 0, 0.75); cursor: zoom-out; }
  .zoom img { max-width: 100%; max-height: 100%; border-radius: 8px; box-shadow: 0 12px 40px rgba(0, 0, 0, 0.6); }
  .md :global(:first-child) { margin-top: 0; }
  .md :global(p), .md :global(ul), .md :global(ol) { margin: 0 0 8px; }
  .md :global(h1), .md :global(h2), .md :global(h3), .md :global(h4) { margin: 12px 0 6px; font-size: 13px; font-family: var(--font); font-weight: 600; color: var(--text); }
  .md :global(code) { padding: 0 4px; border-radius: 4px; background: var(--surface-2); font-family: var(--mono); font-size: 12px; }
  .md :global(pre) { padding: 10px; border-radius: 8px; background: var(--bg); overflow-x: auto; }
  .md :global(pre code) { padding: 0; background: none; }

  @keyframes fade-down { from { opacity: 0; transform: translateY(-8px); } to { opacity: 1; transform: none; } }
  @keyframes rise { from { opacity: 0; transform: translateY(12px); } to { opacity: 1; transform: none; } }
  @keyframes row-in { from { opacity: 0; transform: translateY(10px); } to { opacity: 1; transform: none; } }
  @keyframes row-flash { from { background-color: var(--acc-bg); } to { background-color: transparent; } }
  @keyframes ln-in { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
  @keyframes ln-flash { from { background-color: var(--acc-bg); } to { background-color: transparent; } }
  @keyframes p-in { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
  @keyframes shimmer { to { background-position: -200% 0; } }
  @keyframes blink { 50% { opacity: 0.35; } }
  @keyframes shake { 0%, 100% { transform: translateX(0); } 20% { transform: translateX(-3px); } 40% { transform: translateX(3px); } 60% { transform: translateX(-2px); } 80% { transform: translateX(2px); } }

  @media (max-width: 1100px) {
    .bottom { grid-template-columns: minmax(0, 1fr); }
    .panel { position: static; max-height: none; }
  }
  @media (max-width: 640px) {
    .pio, .role-grid { grid-template-columns: minmax(0, 1fr); }
    .pio-arrow { height: 22px; transform: rotate(90deg); }
  }
</style>
