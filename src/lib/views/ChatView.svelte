<script lang="ts">
  import { open as pickFiles, save as pickSave } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { tick, untrack } from "svelte";
  import { cubicOut } from "svelte/easing";
  import { actionsIn, calloutsIn, suggestionsIn } from "../callouts";
  import { callouts } from "../callout-actions";
  import Minimap from "../components/Minimap.svelte";
  import PlanRecord from "../components/PlanRecord.svelte";
  import Recap from "../components/Recap.svelte";
  import { isPinned, marks, taskKey, togglePin } from "../state/marks.svelte";
  import { extractArtifacts, versionsOf, type Artifact } from "../artifacts";
  import ArtifactPanel from "../components/ArtifactPanel.svelte";
  import UsageRing from "../components/composer/UsageRing.svelte";
  import ModeMenu from "../components/composer/ModeMenu.svelte";
  import ModelMenu from "../components/composer/ModelMenu.svelte";
  import ChangesPanel from "../components/panel/ChangesPanel.svelte";
  import EnvironmentPanel from "../components/panel/EnvironmentPanel.svelte";
  import FilesPanel from "../components/panel/FilesPanel.svelte";
  import TerminalPanel from "../components/panel/TerminalPanel.svelte";
  import PermissionCard from "../components/PermissionCard.svelte";
  import PlanCard from "../components/PlanCard.svelte";
  import { questionsOf } from "../tools";
  import { designUrl } from "../pipeline";
  import QuestionCard from "../components/QuestionCard.svelte";
  import CodeView from "../components/CodeView.svelte";
  import SubagentPanel from "../components/SubagentPanel.svelte";
  import TodoCard from "../components/TodoCard.svelte";
  import ToolGroup from "../components/ToolGroup.svelte";
  import ToolRow from "../components/ToolRow.svelte";
  import { describeFile, folderName, formatTime, formatWhen, fullDate, isoDate, renderMarkdown } from "../format";
  import { accounts, activeAccount } from "../state/accounts.svelte";
  import { agents } from "../state/agents.svelte";
  import { api, errorMessage } from "../api";
  import { activeConversation, backgroundTool, openDesign, branchAt, cancel, cancelEdit, chat, decide, enqueue, forgetParked, regenerate, send, sendNow, setAgent, setEffort, setMode, setModel, startEdit, unqueue } from "../state/chat.svelte";
  import { NEW_CONVERSATION, readDraft, saveDraft } from "../state/drafts";
  import { projectName, projects } from "../state/projects.svelte";
  import { refreshStatus, togglePanel, workspace, type Panel } from "../state/workspace.svelte";
  import { closeSubagent, loadSubagents, openSubagent, runOf, runsOf, statusLabel, subagents } from "../state/subagents.svelte";
  import { EXIT_PLAN, groupThread, isAgentTool, parseToolUse, planCount, touchedFiles, type ThreadBlock, type ToolUse } from "../tools";
  import { pastedImageFiles, readPastedImage } from "../paste";
  import { DEFAULT_EFFORT, DEFAULT_MODEL, SCOPE_ALL, type Message, type PastedImage, type View } from "../types";
  import { onMount } from "svelte";
  import { dayClock } from "../state/clock.svelte";

  interface Props {
    onnavigate: (view: View) => void;
  }
  let { onnavigate }: Props = $props();

  let draft = $state("");
  let newModel = $state<string>(DEFAULT_MODEL);
  let newAgent = $state<string | null>(null);
  let newEffort = $state<string>(DEFAULT_EFFORT);
  /** Skills que podem ser chamadas com "/" no campo de mensagem. */
  let commands = $state<{ name: string; description: string | null }[]>([]);
  let commandIndex = $state(0);
  /** O usuário fechou a lista de comandos para o texto atual. */
  let commandsDismissed = $state(false);
  let openArtifact = $state<Artifact | null>(null);
  /** Caminhos dos arquivos a enviar com a próxima mensagem. */
  let attachments = $state<string[]>([]);
  /** Imagens coladas, com a prévia (data URL) para a miniatura. */
  let pasted = $state<(PastedImage & { preview: string })[]>([]);
  let scroller = $state<HTMLElement>();

  // Subagentes da conversa aberta: os que ainda trabalham e o aberto no painel.
  const runs = $derived(runsOf(chat.activeId));
  const workingRuns = $derived(runs.filter((r) => r.status === "running"));
  const openRunId = $derived(subagents.open && subagents.open.conversationId === chat.activeId ? subagents.open.parent : null);
  const openRun = $derived(openRunId ? runOf(chat.activeId, openRunId) : undefined);
  /** A chamada que criou o subagente aberto, para o título e a tarefa quando o início não foi registrado. */
  const openRunCall = $derived.by(() => {
    if (!openRunId) return null;
    for (const m of chat.messages) {
      if (m.role !== "tool") continue;
      const tool = parseToolUse(m.content);
      if (tool?.id === openRunId) {
        const text = (key: string) => (typeof tool.input[key] === "string" ? (tool.input[key] as string) : "");
        return { description: text("description"), prompt: text("prompt") };
      }
    }
    return null;
  });

  function showSubagent(parent: string) {
    if (!chat.activeId) return;
    openArtifact = null;
    openSubagent(chat.activeId, parent);
  }

  /** Situação de um subagente ao lado da linha da ferramenta que o criou. */
  function agentNote(tool: ToolUse): string | null {
    const run = tool.id ? runOf(chat.activeId, tool.id) : undefined;
    return run ? statusLabel(run) : null;
  }

  // O rascunho (texto, anexos e imagens) é de cada conversa e sobrevive à troca de tela.
  const draftKey = $derived(chat.activeId ?? NEW_CONVERSATION);
  let draftLoadedFor: string | null = null;
  $effect.pre(() => {
    const key = draftKey;
    untrack(() => {
      const saved = readDraft(key);
      draft = saved.text;
      attachments = saved.attachments;
      pasted = saved.pasted;
      draftLoadedFor = key;
    });
  });
  $effect(() => {
    const current = { text: draft, attachments, pasted };
    if (draftLoadedFor) untrack(() => saveDraft(draftLoadedFor!, current));
  });


  const conversation = $derived(activeConversation());
  const turn = $derived(chat.activeId ? chat.turns[chat.activeId] : undefined);
  /** A conversa em blocos: mensagens, grupos de ações (com as que ainda rodam) e a lista de tarefas. */
  const thread = $derived(groupThread(chat.messages, turn?.running ?? []));
  /** O agente ainda está na última sequência de ações: nada escrito depois dela. */
  const liveGroupKey = $derived.by(() => {
    // O cartão de tarefas pode vir depois do grupo (atualizado junto com um comando): não conta.
    const last = thread.findLast((b) => b.kind !== "todos");
    if (!turn || turn.waiting || last?.kind !== "tools") return null;
    return turn.running.length || !turn.text ? last.key : null;
  });
  const outcome = $derived(chat.activeId ? chat.outcomes[chat.activeId] : null);
  const permission = $derived(chat.activeId ? chat.permissions[chat.activeId] : undefined);

  /** Bloco que segue a linha do tempo: a explicação do agente entre uma sequência de ações e a próxima. */
  const isNarration = (block: ThreadBlock<Message> | undefined, next: ThreadBlock<Message> | undefined) =>
    block?.kind === "message" && block.message.role === "assistant" && !!next && next.kind !== "message";
  /** A linha do tempo continua depois do bloco `i`. */
  const railAfter = (i: number) => thread[i + 1]?.kind === "tools" || isNarration(thread[i + 1], thread[i + 2]);

  /** Ações do usuário (cartões "Ação sua") por resposta gravada. */
  const actionLists = $derived(
    chat.messages.filter((m) => m.role === "assistant" && m.id > 0).map((m) => ({ id: m.id, items: actionsIn(m.content) })).filter((a) => a.items.length),
  );
  const pending = $derived(
    chat.activeId ? actionLists.reduce((n, a) => n + a.items.filter((_, i) => !marks.done[taskKey(chat.activeId!, a.id, i)]).length, 0) : 0,
  );
  const lastAssistant = $derived(chat.messages.findLast((m) => m.role === "assistant"));
  /** Próximos pedidos sugeridos pela última resposta. */
  const suggestions = $derived(!turn && lastAssistant ? suggestionsIn(lastAssistant.content) : []);
  /** Resumo "Retomar daqui": só em conversas com trabalho de verdade. */
  const recap = $derived.by(() => {
    const groups = thread.filter((b) => b.kind === "tools");
    if (turn || groups.length < 3) return null;
    const delivered =
      chat.messages
        .filter((m) => m.role === "assistant")
        .flatMap((m) => calloutsIn(m.content))
        .findLast((c) => c.kind === "ok")?.title ?? null;
    return {
      steps: groups.length,
      actions: groups.reduce((n, g) => n + (g.kind === "tools" ? g.items.length : 0), 0),
      delivered: delivered?.replace(/[*_`]/g, "") || null,
      files: touchedFiles(chat.messages),
    };
  });

  /** Leva a conversa até um elemento e o faz piscar. */
  function reveal(el: Element | null | undefined) {
    if (!el || !scroller) return;
    const top = el.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop - 72;
    pinned = false;
    scroller.scrollTo({ top, behavior: "smooth" });
    el.classList.remove("flash");
    void (el as HTMLElement).offsetWidth;
    el.classList.add("flash");
  }
  const revealPending = () => {
    const open = [...(threadEl?.querySelectorAll(".callout.act li.task:not(.done)") ?? [])][0];
    reveal(open?.closest(".callout") ?? threadEl?.querySelector(".callout.act"));
  };

  /** Entrada suave dos blocos novos; ao abrir uma conversa, tudo já aparece no lugar. */
  let animate = $state(false);
  $effect(() => {
    void chat.activeId;
    animate = false;
    const timer = setTimeout(() => (animate = true), 400);
    return () => clearTimeout(timer);
  });
  function rise(_node: Element, { enabled }: { enabled: boolean }) {
    if (!enabled) return { duration: 0 };
    return { duration: 460, easing: cubicOut, css: (t: number) => `opacity:${t};transform:translateY(${(1 - t) * 12}px);filter:blur(${(1 - t) * 5}px)` };
  }

  let input = $state<HTMLTextAreaElement>();
  function fill(text: string) {
    draft = text;
    void tick().then(() => input?.focus());
  }
  const isAgent = $derived(conversation?.kind === "agent");
  const account = $derived(activeAccount());
  const model = $derived(conversation?.model ?? newModel);
  const agentId = $derived(conversation ? conversation.agent_id : newAgent);
  const projectId = $derived(conversation ? conversation.project_id : projects.pendingId);
  const agentModel = $derived(agents.list.find((a) => a.id === agentId)?.model ?? null);
  const effort = $derived(conversation?.effort ?? newEffort);
  const canSend = $derived(draft.trim().length > 0 || pasted.length > 0);
  /** Mensagens esperando o agente, na conversa aberta. */
  const queued = $derived(chat.activeId ? (chat.queued[chat.activeId] ?? []) : []);
  /** Lista de comandos: aparece enquanto o texto é só "/" seguido do começo de um nome. */
  const commandQuery = $derived(commandsDismissed ? null : (draft.match(/^\/(\S*)$/)?.[1] ?? null));
  const commandMatches = $derived(
    commandQuery === null ? [] : commands.filter((c) => c.name.toLowerCase().includes(commandQuery.toLowerCase())).slice(0, 8),
  );
  /** Todos os artifacts da conversa, em ordem, para o histórico de versões e a lista de saídas. */
  const allArtifacts = $derived(
    chat.messages.filter((m) => m.role === "assistant").flatMap((m) => extractArtifacts(m.content)),
  );
  /** Link do Claude Design de cada resposta, calculado uma vez por lista de mensagens. */
  const designLinks = $derived(
    new Map(chat.messages.filter((m) => m.role === "assistant").map((m) => [m.id, [designUrl(m.content)].filter((u) => u !== null)])),
  );
  const openVersions = $derived(openArtifact ? versionsOf(openArtifact, allArtifacts) : []);
  const openIndex = $derived(
    openArtifact ? Math.max(0, openVersions.findIndex((a) => a.code === openArtifact?.code)) : 0,
  );
  /** Arquivos e Alterações só existem quando a conversa trabalha em uma pasta. */
  const panel = $derived<Panel | null>(
    workspace.panel && (isAgent || workspace.panel === "environment") ? workspace.panel : null,
  );

  const PANELS: { id: Panel; label: string; icon: string; needsFolder: boolean }[] = [
    { id: "environment", label: "Ambiente", icon: "M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01", needsFolder: false },
    { id: "files", label: "Arquivos", icon: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z", needsFolder: true },
    { id: "changes", label: "Alterações", icon: "M12 4v8M8 8h8M8 18h8M5 3h14v18H5z", needsFolder: true },
    { id: "terminal", label: "Terminal", icon: "M4 5h16v14H4zM7 9l3 3-3 3M12 15h5", needsFolder: true },
  ];

  /** Imagens coladas no campo entram como anexos, com miniatura. */
  async function onPaste(event: ClipboardEvent) {
    for (const file of pastedImageFiles(event)) pasted = [...pasted, await readPastedImage(file)];
  }

  /** "Gerar com IA": o modelo reescreve a mensagem digitada num texto mais claro, sem enviar. */
  let rewriting = $state(false);
  let rewriteError = $state<string | null>(null);
  /** Geração em andamento: trocar de conversa invalida a resposta que ainda vai chegar. */
  let rewriteRun = 0;
  const canRewrite = $derived(draft.trim().length > 0 && !draft.trimStart().startsWith("/"));
  async function rewriteDraft() {
    if (!canRewrite || rewriting) return;
    const run = ++rewriteRun;
    const conversation = chat.activeId;
    const original = draft;
    rewriting = true;
    rewriteError = null;
    try {
      const text = await api.draftChatMessage(original, pasted.map(({ name, media_type, data }) => ({ name, media_type, data })));
      if (run === rewriteRun && conversation === chat.activeId && draft === original) draft = text;
    } catch (e) {
      if (run === rewriteRun) rewriteError = errorMessage(e);
    } finally {
      if (run === rewriteRun) rewriting = false;
    }
  }
  $effect(() => {
    chat.activeId;
    untrack(() => {
      rewriteRun++;
      rewriting = false;
      rewriteError = null;
    });
  });

  function removePasted(index: number) {
    pasted = pasted.filter((_, i) => i !== index);
  }

  /** Registro da mensagem (anexos, contexto, imagens), lido uma vez só: pode trazer imagens grandes em base64. */
  function parseMeta(message: Message): Record<string, unknown> {
    if (!message.meta) return {};
    try {
      return JSON.parse(message.meta) ?? {};
    } catch {
      return {};
    }
  }

  function meta(message: Message, key: "context" | "attachments"): string[] {
    const value = parseMeta(message)[key];
    return Array.isArray(value) ? value : [];
  }

  const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;

  /** Imagem colada numa mensagem enviada; `index` é a posição na lista guardada. */
  interface SentImage {
    image: PastedImage;
    messageId: number;
    index: number;
  }

  /** Imagens coladas que foram enviadas com a mensagem. */
  function imagesOf(message: Message, info: Record<string, unknown>): SentImage[] {
    const images = Array.isArray(info.images) ? (info.images as PastedImage[]) : [];
    return images
      .map((image, index) => ({ image, messageId: message.id, index }))
      .filter(({ image }) => image?.data && image?.media_type);
  }

  /** Nomes dos anexos que não são imagens coladas: estas vêm por último na lista. */
  function fileNames(info: Record<string, unknown>): string[] {
    const names = Array.isArray(info.attachments) ? (info.attachments as string[]) : [];
    const pasted = Array.isArray(info.images) ? info.images.length : 0;
    return names.slice(0, Math.max(0, names.length - pasted));
  }

  const imageSrc = (image: PastedImage) => `data:${image.media_type};base64,${image.data}`;

  /** Imagem aberta em tamanho grande. */
  let zoomed = $state.raw<SentImage | null>(null);
  let lightbox = $state<HTMLDialogElement>();
  /** Resultado do último "Copiar": null enquanto não houve, ou a mensagem de erro. */
  let imageCopied = $state<"ok" | string | null>(null);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  function openImage(sent: SentImage) {
    zoomed = sent;
    imageCopied = null;
    clearTimeout(copiedTimer);
  }

  // O elemento nativo cuida do foco, do Esc e de bloquear o resto da tela.
  $effect(() => {
    if (zoomed && !lightbox?.open) lightbox?.showModal();
    if (!zoomed && lightbox?.open) lightbox.close();
  });

  // Trocar de conversa fecha a imagem aberta.
  $effect(() => {
    void chat.activeId;
    zoomed = null;
  });

  async function copyImage(sent: SentImage) {
    let result: "ok" | string;
    try {
      // Mensagens ainda não gravadas têm id local (≤ 0) e só existem na tela.
      await api.copyImage(sent.messageId > 0 ? { message_id: sent.messageId, index: sent.index } : { data: sent.image.data });
      result = "ok";
    } catch (e) {
      result = e instanceof Error ? e.message : String(e);
    }
    // A imagem pode ter sido fechada (ou trocada) enquanto copiava.
    if (zoomed !== sent) return;
    imageCopied = result;
    clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (imageCopied = null), imageCopied === "ok" ? 1500 : 4000);
  }

  function contextLabel(count: number): string {
    return count === 1 ? "Usou 1 conversa anterior como contexto" : `Usou ${count} conversas anteriores como contexto`;
  }

  function edit(message: Message) {
    startEdit(message);
    draft = message.content;
  }

  function stopEditing() {
    cancelEdit();
    draft = "";
  }

  /** Resposta copiada há pouco, para o botão confirmar. */
  let copiedId = $state<number | null>(null);
  let answerCopiedTimer: ReturnType<typeof setTimeout> | undefined;

  async function copyText(text: string, messageId?: number) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      return; // sem área de transferência disponível
    }
    if (messageId === undefined) return;
    copiedId = messageId;
    clearTimeout(answerCopiedTimer);
    answerCopiedTimer = setTimeout(() => (copiedId = null), 1500);
  }

  /** Por que uma mensagem ainda está na fila. */
  function queueNote(waitsForEnd: boolean): string {
    if (!turn) return "Na fila";
    if (turn.waiting) return waitsForEnd ? "Este serviço só lê quando os agentes em segundo plano terminarem" : "Na fila · vai quando os agentes em segundo plano terminarem";
    return waitsForEnd ? "Este serviço só lê ao fim da resposta" : "Na fila · vai quando a resposta terminar";
  }

  /** A última resposta mostra as ações sempre; as anteriores, ao passar o mouse. */
  const lastAssistantId = $derived(chat.messages.findLast((m) => m.role === "assistant")?.id ?? null);

  /** Ramo com tudo até esta mensagem, inclusive. */
  const branchAfter = (message: Message) => branchAt(message.id + 1);

  /** A conversa em Markdown, para guardar ou compartilhar. */
  function toMarkdown(): string {
    const parts = chat.messages.map((m) => {
      if (m.role === "user") return `## Você\n\n${m.content}`;
      if (m.role === "assistant") return `## Assistente\n\n${m.content}`;
      if (m.role === "notice") return `> ${m.content}`;
      const tool = parseToolUse(m.content);
      return tool ? `> Ferramenta: ${tool.name}` : "";
    });
    return `# ${conversation?.title ?? "Conversa"}\n\n${parts.filter(Boolean).join("\n\n")}\n`;
  }

  async function exportConversation() {
    const base = (conversation?.title ?? "conversa").replace(/[\\/:*?"<>|]+/g, "-").slice(0, 60);
    const path = await pickSave({ title: "Exportar conversa", defaultPath: `${base}.md`, filters: [{ name: "Markdown", extensions: ["md"] }] });
    if (path) await api.saveTextFile(path, toMarkdown());
  }

  async function attach() {
    const picked = await pickFiles({ multiple: true, title: "Anexar arquivos" });
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    attachments = [...new Set([...attachments, ...paths])];
  }

  async function submit() {
    if (!canSend || rewriting) return;
    rewriteError = null;
    const text = draft.trim();
    const files = attachments;
    const images = pasted.map(({ name, media_type, data }) => ({ name, media_type, data }));
    // Com o agente trabalhando, a mensagem é entregue já e ele a lê no próximo passo, como no
    // Claude Desktop. Serviços que só leem ao fim da resposta a deixam na fila até lá.
    if (turn && chat.activeId) {
      const id = chat.activeId;
      const queuedId = enqueue(id, text, files, pasted);
      draft = "";
      attachments = [];
      pasted = [];
      saveDraft(draftKey, { text: "", attachments: [], pasted: [] });
      await sendNow(id, queuedId);
      return;
    }
    draft = "";
    attachments = [];
    pasted = [];
    // A conversa nova ganha id durante o envio: o rascunho enviado não pode ficar para a próxima.
    saveDraft(draftKey, { text: "", attachments: [], pasted: [] });
    await send(text, { model, agentId, projectId, effort, attachments: files, images });
  }

  // Um painel pediu para inserir texto no campo (ex.: comentário em uma linha do diff).
  $effect(() => {
    const insert = workspace.composerInsert;
    if (!insert) return;
    workspace.composerInsert = null;
    untrack(() => (draft = draft.trim() ? `${draft.trimEnd()}\n\n${insert}` : insert));
  });

  function pickCommand(name: string) {
    draft = `/${name} `;
    commandsDismissed = true;
  }

  /** Teclas da lista de comandos; devolve `true` se a tecla foi consumida. */
  function commandKey(event: KeyboardEvent): boolean {
    if (!commandMatches.length) return false;
    const last = commandMatches.length - 1;
    if (event.key === "ArrowDown") commandIndex = commandIndex >= last ? 0 : commandIndex + 1;
    else if (event.key === "ArrowUp") commandIndex = commandIndex <= 0 ? last : commandIndex - 1;
    else if (event.key === "Enter" || event.key === "Tab") pickCommand(commandMatches[Math.min(commandIndex, last)].name);
    else if (event.key === "Escape") commandsDismissed = true;
    else return false;
    event.preventDefault();
    return true;
  }

  function onKeydown(event: KeyboardEvent) {
    if (commandKey(event)) return;
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      void submit();
    }
  }

  function changeAgent(value: string) {
    const id = value || null;
    if (conversation) void setAgent(id);
    else newAgent = id;
  }

  function changeModel(value: string) {
    if (conversation) void setModel(value);
    else newModel = value;
  }

  function changeEffort(value: string) {
    if (conversation) void setEffort(value);
    else newEffort = value;
  }

  // A cada mudança no texto, a lista de comandos volta a poder aparecer.
  $effect(() => {
    void draft;
    commandIndex = 0;
    if (!draft.startsWith("/")) commandsDismissed = false;
  });

  onMount(async () => {
    const skills = await api.listSkills(SCOPE_ALL);
    commands = skills.filter((s) => s.enabled).map((s) => ({ name: s.name, description: s.description }));
  });

  function openProject() {
    projects.selectedId = projectId;
    onnavigate("project");
  }

  // Ao trocar de conversa, a prévia aberta deixa de fazer sentido.
  $effect(() => {
    void chat.activeId;
    openArtifact = null;
    workspace.openFile = null;
  });

  // O que os subagentes da conversa fizeram é carregado ao abri-la (inclusive ramos e sessões novas).
  $effect(() => {
    if (chat.activeId) void loadSubagents(chat.activeId);
  });

  // Uma prévia de artifact aberta ocupa o lugar do subagente.
  $effect(() => {
    if (openArtifact) untrack(closeSubagent);
  });

  // A situação da pasta acompanha a conversa aberta e cada passo do agente.
  $effect(() => {
    void chat.messages.length;
    void refreshStatus(chat.activeId, isAgent);
  });

  // Acompanha o fim da conversa enquanto a resposta chega, a menos que você tenha rolado para cima.
  // Qualquer coisa que aumente a conversa (texto, ferramentas, imagens, cartões) conta: quem observa é o tamanho.
  let threadEl = $state<HTMLElement>();
  let pinned = true;
  const toBottom = () => scroller?.scrollTo({ top: scroller.scrollHeight });
  const onScroll = () => {
    if (scroller) pinned = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 48;
  };
  $effect(() => {
    if (!threadEl || !scroller) return;
    // A área também encolhe (um arquivo aberto, a janela redimensionada): seguindo o fim, ele continua à vista.
    const observer = new ResizeObserver(() => pinned && toBottom());
    observer.observe(threadEl);
    observer.observe(scroller);
    return () => observer.disconnect();
  });
  // Outra conversa, ou uma mensagem sua: volta a acompanhar o fim.
  $effect(() => {
    void chat.activeId;
    pinned = true;
  });
  $effect(() => {
    if (chat.messages.at(-1)?.role === "user") pinned = true;
    void tick().then(() => pinned && toBottom());
  });
</script>

<div class="split">
  <main>
    <header>
      {#if projectId}
        <button type="button" class="crumb" onclick={openProject}>{projectName(projectId) ?? "Projeto"}</button>
        <span class="slash" aria-hidden="true">/</span>
      {/if}
      <h1>{conversation?.title ?? "Nova conversa"}</h1>
      {#if chat.messages.length}
        <button type="button" class="btn icon" aria-label="Exportar a conversa em Markdown" title="Exportar (.md)" onclick={exportConversation}>
          <svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v11M7 10l5 5 5-5M4 19h16" /></svg>
        </button>
      {/if}
      {#if permission?.tool === EXIT_PLAN}
        <button type="button" class="pend plan" title="Ir até o plano" onclick={() => reveal(threadEl?.querySelector("#plano-pendente"))}><span class="pend-dot"></span>Aguardando sua aprovação</button>
      {:else if pending}
        <button type="button" class="pend" title="Ir até o que falta do seu lado" onclick={revealPending}><span class="pend-dot"></span><b>{pending}</b>{pending === 1 ? "pendência sua" : "pendências suas"}</button>
      {:else if actionLists.length}
        <span class="pend clear">Tudo resolvido</span>
      {/if}
      <div class="panels" role="group" aria-label="Painel lateral">
        {#each PANELS as p (p.id)}
          {#if isAgent || !p.needsFolder}
            <button type="button" aria-label={p.label} title={p.label} aria-pressed={panel === p.id && !openArtifact && !openRunId} class:on={panel === p.id && !openArtifact && !openRunId} onclick={() => ((openArtifact = null), closeSubagent(), togglePanel(p.id))}>
              <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d={p.icon} /></svg>
            </button>
          {/if}
        {/each}
      </div>
      {#if turn && !turn.waiting}<div class="progress" aria-hidden="true"></div>{/if}
    </header>

    {#if workspace.openFile}
      {@const file = workspace.openFile}
      <section class="viewer" aria-label={`Arquivo ${file.path}`}>
        <div class="viewer-head">
          <span class="mono path">{file.path}</span>
          <span class="muted info">{describeFile(file)}</span>
          <button type="button" class="btn icon" aria-label="Fechar o arquivo" title="Fechar" onclick={() => (workspace.openFile = null)}>
            <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
          </button>
        </div>
        {#if file.text !== null}
          <CodeView text={file.text} path={file.path} large />
        {:else}
          <p class="muted no-preview">Este arquivo não é texto ou é grande demais para exibir.</p>
        {/if}
      </section>
    {/if}

    <div class="stage" class:below={workspace.openFile !== null}>
    <div class="scroll" bind:this={scroller} onscroll={onScroll}>
      <div class="thread" bind:this={threadEl}>
        {#if recap}
          <Recap {...recap} {pending} onpending={revealPending} />
        {/if}
        {#if chat.messages.length === 0 && !turn}
          <div class="welcome">
            <h2>{isAgent ? "O que vamos fazer nesta pasta?" : "Como posso ajudar?"}</h2>
            <p class="muted">Suas conversas anteriores do Claude Code e do Claude Desktop entram como contexto quando ajudam.</p>
          </div>
        {/if}

        {#each thread as block, i (block.key)}
          <div class="blk" in:rise={{ enabled: animate }}>
          {#if block.kind === "todos"}
            <TodoCard todos={block.todos} />
          {:else if block.kind === "plan"}
            <PlanRecord tool={block.tool} version={block.version} />
          {:else if block.kind === "tools"}
            {@const live = block.key === liveGroupKey}
            <ToolGroup items={block.items} {live} since={live ? turn?.toolsSince : null} rail={railAfter(i)}>
              {#snippet row(item)}
                {@const tool = item.tool}
                {#if isAgentTool(tool.name) && tool.id}
                  {@const id = tool.id}
                  <ToolRow {tool} flat running={runOf(chat.activeId, id)?.status === "running" || item.running !== null} note={agentNote(tool)} onopen={() => showSubagent(id)} />
                {:else}
                  <ToolRow {tool} flat live={item.running} onbackground={() => backgroundTool(item.key)} />
                {/if}
              {/snippet}
            </ToolGroup>
          {:else}
          {@const message = block.message}
          {#if message.role === "user"}
            {@const info = parseMeta(message)}
            {@const images = imagesOf(message, info)}
            {@const names = fileNames(info)}
            <div class="user-wrap">
              {#if images.length}
                <div class="sent-images" role="group" aria-label="Imagens enviadas">
                  {#each images as sent (sent.index)}
                    <button type="button" class="sent-image" title="Ampliar a imagem" aria-label={`Ampliar a imagem ${sent.image.name}`} onclick={() => openImage(sent)}>
                      <img src={imageSrc(sent.image)} alt={sent.image.name} />
                    </button>
                  {/each}
                </div>
              {/if}
              {#if names.length || message.content}
              <div class="user">
                {#if names.length}
                  <div class="files">{#each names as n}<span class="file">{n}</span>{/each}</div>
                {/if}{message.content}
              </div>
              {/if}
              {#if message.created_at || (!turn && message.id > 0)}
                <div class="msg-actions">
                  {#if message.created_at}
                    <time class="when" datetime={isoDate(message.created_at)} title={fullDate(message.created_at)}>{formatWhen(message.created_at, dayClock.now)}</time>
                  {/if}
                  {#if !turn && message.id > 0}
                    <button type="button" onclick={() => edit(message)}>Editar</button>
                  {/if}
                </div>
              {/if}
            </div>
          {:else if message.role === "notice"}
            <div class="notice" role="status"><span></span>{message.content}<span></span></div>
          {:else}
            {@const titles = meta(message, "context")}
            {@const pin = isPinned(chat.activeId, message.id)}
            <div class="assistant" class:narr={isNarration(block, thread[i + 1])} class:pinned={pin} data-mm={pin ? "Mensagem fixada" : undefined} data-mm-kind="pin">
              {#if titles.length}
                <div class="context" title={titles.join("\n")}>{contextLabel(titles.length)}</div>
              {/if}
              <div class="markdown" {@attach callouts({ conversation: chat.activeId, message: message.id, content: message.content, onfill: fill })}>{@html renderMarkdown(message.content)}</div>
              <div class="msg-actions" class:shown={message.id === lastAssistantId && (!turn || turn.waiting)}>
                <button type="button" onclick={() => copyText(message.content, message.id)}>{copiedId === message.id ? "Copiado" : "Copiar"}</button>
                {#if !turn && message.id > 0}
                  <button type="button" onclick={() => regenerate(message)}>Regenerar</button>
                  <button type="button" onclick={() => branchAfter(message)}>Ramificar daqui</button>
                {/if}
                {#if message.id > 0 && chat.activeId}
                  {@const id = chat.activeId}
                  <button type="button" aria-pressed={pin} class:on={pin} onclick={() => togglePin(id, message.id)}>{pin ? "Fixada" : "Fixar"}</button>
                {/if}
                {#if message.created_at}
                  <time class="when" datetime={isoDate(message.created_at)} title={fullDate(message.created_at)}>{formatWhen(message.created_at, dayClock.now)}</time>
                {/if}
              </div>
              {#each extractArtifacts(message.content) as artifact (artifact.code)}
                <button type="button" class="artifact" onclick={() => (openArtifact = artifact)}>
                  <svg aria-hidden="true" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="var(--accent-text)" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3h9l4 4v14H6zM14 3v5h5M9 13h7M9 17h5" /></svg>
                  <span>
                    <strong>{artifact.title}</strong>
                    <span class="muted">{artifact.kind === "html" ? "Página" : "Imagem"} · abrir prévia</span>
                  </span>
                </button>
              {/each}
              {#each designLinks.get(message.id) ?? [] as url (url)}
                <div class="design-link">
                  <button type="button" class="artifact" onclick={() => openDesign(url)}>
                    <svg aria-hidden="true" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="var(--accent-text)" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 5h18v14H3zM3 9h18M8 9v10" /></svg>
                    <span>
                      <strong>Claude Design</strong>
                      <span class="muted">Artifact · abrir</span>
                    </span>
                  </button>
                  <!-- A janela do Lume tem o próprio login do claude.ai; o navegador já está logado. -->
                  <button type="button" class="lnk" title={url} onclick={() => openUrl(url)}>Abrir no navegador ↗</button>
                </div>
              {/each}
            </div>
          {/if}
          {/if}
          </div>
        {/each}

        {#if turn}
          <div class="assistant" aria-live="polite">
            {#if turn.context.length}
              <div class="context" title={turn.context.join("\n")}>{contextLabel(turn.context.length)}</div>
            {/if}
            {#if turn.text}
              <div class="markdown">{@html renderMarkdown(turn.text)}</div>
            {/if}
            <!-- Com a sequência de ações à vista, ela já mostra o trabalho; aqui só o que pede atenção. -->
            {#if !turn.waiting && (turn.status || !turn.text) && (!liveGroupKey || chat.permissions[chat.activeId ?? ""])}
              <div class="working"><span class="pulse" aria-hidden="true"></span>{turn.status ?? "Respondendo"}…</div>
            {/if}
          </div>
        {/if}

        {#if workingRuns.length}
          <div class="agents" role="group" aria-label="Subagentes trabalhando">
            {#each workingRuns as run (run.id)}
              <button type="button" class="agent-chip" class:on={openRunId === run.id} title="Ver o que o subagente está fazendo" onclick={() => showSubagent(run.id)}>
                <span class="pulse" aria-hidden="true"></span>
                <span class="agent-name">{run.description || "Subagente"}</span>
                <span class="agent-activity">{run.activity ?? (run.background ? "em segundo plano" : "trabalhando")}</span>
                <svg aria-hidden="true" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 6l6 6-6 6" /></svg>
              </button>
            {/each}
          </div>
        {/if}

        {#each queued as item (item.id)}
          <div class="user-wrap queued" aria-label="Mensagem na fila">
            <div class="user">
              {#if item.attachments.length || item.images.length}
                <div class="files">
                  {#each [...item.attachments.map(baseName), ...item.images.map((i) => i.name)] as name, i (i)}
                    <span class="file">{name}</span>
                  {/each}
                </div>
              {/if}{item.text}
            </div>
            <div class="queue-actions">
              {#if item.sending}
                <span class="muted small">Entregue · o agente lê no próximo passo</span>
              {:else}
                <span class="muted small">{queueNote(item.waitsForEnd)}</span>
                <button type="button" class="btn small" title="Remover da fila" onclick={() => chat.activeId && unqueue(chat.activeId, item.id)}>Remover</button>
                {#if !item.waitsForEnd || !turn}
                  <button type="button" class="btn small primary" title="O agente lê esta mensagem já, sem parar o que está fazendo" onclick={() => chat.activeId && sendNow(chat.activeId, item.id)}>Enviar agora</button>
                {/if}
              {/if}
            </div>
          </div>
        {/each}

        {#if permission?.tool === "AskUserQuestion"}
          <QuestionCard questions={questionsOf(permission)} onanswer={(answers) => decide(true, false, { answers })} oncancel={() => decide(false, false)} />
        {:else if permission?.tool === "ExitPlanMode"}
          {#key permission.requestId}<PlanCard request={permission} version={planCount(chat.messages) + 1} ondecide={(mode, feedback) => (mode ? decide(true, false, { nextMode: mode }) : decide(false, false, { feedback }))} />{/key}
        {:else if permission}
          <PermissionCard request={permission} ondecide={decide} />
        {/if}
        {#if permission && chat.permissionQueue[chat.activeId ?? ""]?.length}
          {@const waiting = chat.permissionQueue[chat.activeId ?? ""].length}
          <p class="muted small" role="status">{waiting === 1 ? "Mais 1 pedido de autorização espera depois deste." : `Mais ${waiting} pedidos de autorização esperam depois deste.`}</p>
        {/if}

        {#if outcome?.kind === "exhausted"}
          <div class="card alert" role="alert">
            <strong>Todas as contas atingiram o limite.</strong>
            <span class="muted">
              {outcome.resetsAt ? `A primeira volta às ${formatTime(outcome.resetsAt)}.` : "Nenhuma conta está disponível."}
              {outcome.resumes && accounts.flags.resume_on_reset ? "O Lume continua este pedido sozinho quando uma conta voltar." : "Envie a mensagem de novo quando houver uma conta livre."}
            </span>
            <div class="row-w">
              <button type="button" class="btn small" onclick={() => onnavigate("accounts")}>Ver contas</button>
              {#if outcome.resumes && accounts.flags.resume_on_reset && chat.activeId}
                {@const id = chat.activeId}
                <button type="button" class="btn small" onclick={() => void forgetParked(id)}>Não continuar sozinho</button>
              {/if}
            </div>
          </div>
        {:else if outcome?.kind === "error"}
          <div class="card alert" role="alert">
            <strong>Não foi possível responder.</strong>
            <span class="muted">{outcome.message}</span>
          </div>
        {/if}
      </div>
    </div>
    <Minimap {scroller} root={threadEl} />
    </div>

    <div class="composer-wrap">
      <div class="composer-area">
        {#if commandMatches.length}
          <div class="commands" role="listbox" aria-label="Comandos">
            {#each commandMatches as c, i (c.name)}
              <button type="button" role="option" aria-selected={i === commandIndex} class:on={i === commandIndex} onclick={() => pickCommand(c.name)}>
                <span class="mono">/{c.name}</span>
                {#if c.description}<span class="muted">{c.description}</span>{/if}
              </button>
            {/each}
          </div>
        {/if}

        {#if suggestions.length && !draft.trim() && !commandMatches.length}
          <div class="suggest" role="group" aria-label="Sugestões de próximo pedido">
            {#each suggestions as text, i (text)}
              <button type="button" class="sg" style={`--i:${i}`} onclick={() => fill(text)}>
                <svg aria-hidden="true" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z" /></svg>
                {text}
              </button>
            {/each}
          </div>
        {/if}
        <div class="composer">
          {#if chat.editing}
            <div class="editing">
              <span>Editando a mensagem: ao enviar, a conversa continua em um ramo novo a partir dela.</span>
              <button type="button" class="btn small" onclick={stopEditing}>Cancelar</button>
            </div>
          {/if}
          {#if pasted.length}
            <div class="thumbs" role="list" aria-label="Imagens coladas">
              {#each pasted as image, i (image.preview)}
                <span class="thumb" role="listitem">
                  <img src={image.preview} alt={image.name} />
                  <button type="button" aria-label={`Remover a imagem ${image.name}`} title={`Remover a imagem ${image.name}`} onclick={() => removePasted(i)}>×</button>
                </span>
              {/each}
            </div>
          {/if}
          {#if attachments.length}
            <div class="files">
              {#each attachments as path (path)}
                <span class="file">
                  {baseName(path)}
                  <button type="button" aria-label={`Remover o anexo ${baseName(path)}`} title={`Remover o anexo ${baseName(path)}`} onclick={() => (attachments = attachments.filter((p) => p !== path))}>×</button>
                </span>
              {/each}
            </div>
          {/if}
          {#if rewriteError}<p class="error-text rewrite-error" role="alert">{rewriteError}</p>{/if}
          <div class="entry">
            <button type="button" class="tool" aria-label="Anexar arquivos" title="Anexar arquivos" onclick={attach}>
              <svg aria-hidden="true" width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
            </button>
            <textarea bind:this={input} aria-label="Mensagem" rows="1" placeholder="Digite / para comandos · cole imagens aqui" bind:value={draft} onkeydown={onKeydown} onpaste={onPaste} readonly={rewriting}></textarea>
            {#if canRewrite || rewriting}
              <button type="button" class="gen" onclick={rewriteDraft} disabled={rewriting}
                title="Reescreve a mensagem digitada num texto mais claro e organizado (não envia)">
                {#if rewriting}<span class="spinner" aria-hidden="true"></span>Gerando…{:else}Gerar com IA{/if}
              </button>
            {/if}
            <!-- Parar fica sempre à mão durante o trabalho; com texto digitado, Enviar aparece ao lado. -->
            {#if turn}
              <button type="button" class="send stop" aria-label="Parar a resposta" title={turn.waiting ? "Parar os agentes em segundo plano" : "Parar"} onclick={cancel}>
                <svg aria-hidden="true" width="11" height="11" viewBox="0 0 24 24" fill="currentColor"><rect x="5" y="5" width="14" height="14" rx="2.5" /></svg>
              </button>
            {/if}
            {#if canSend}
              <button type="button" class="send" aria-label="Enviar" title={turn ? "Enviar (Enter) · o agente lê no próximo passo" : "Enviar (Enter)"} onclick={submit}>
                <svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 19V5M6 11l6-6 6 6" /></svg>
              </button>
            {/if}
          </div>
        </div>

        <div class="status-line">
          {#if isAgent && conversation}
            <span class="info" title={conversation.cwd}>
              <svg aria-hidden="true" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>
              {folderName(conversation.cwd)}
            </span>
            {#if workspace.status?.branch}
              <span class="info">
                <svg aria-hidden="true" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="6" cy="6" r="2.2" /><circle cx="6" cy="18" r="2.2" /><circle cx="18" cy="8" r="2.2" /><path d="M6 8.2v7.6M18 10.2c0 4-6 3-11 5" /></svg>
                {workspace.status.branch}
              </span>
            {/if}
          {/if}
          <span class="spacer"></span>
          <button type="button" class="account" title="Conta que atende a próxima mensagem" onclick={() => onnavigate("accounts")}>
            <span class="dot" class:none={!account}></span>
            {turn?.account ?? account?.name ?? "Sem conta"}
          </button>
          {#if agents.list.length}
            <select class="pick" aria-label="Agente" title="Agente" value={agentId ?? ""} onchange={(e) => changeAgent(e.currentTarget.value)}>
              <option value="">Sem agente</option>
              {#each agents.list as a (a.id)}
                <option value={a.id}>{a.name}</option>
              {/each}
            </select>
          {/if}
          <ModelMenu model={agentModel ?? model} {effort} locked={agentModel !== null} provider={account?.provider} onmodel={changeModel} oneffort={changeEffort} />
          {#if isAgent && conversation}
            <ModeMenu mode={conversation.permission_mode} onmode={setMode} />
          {/if}
          <UsageRing {account} {onnavigate} />
        </div>
      </div>
    </div>
  </main>

  {#if openRunId}
    {#key openRunId}
      <SubagentPanel run={openRun} fallback={openRunCall} onclose={closeSubagent} />
    {/key}
  {:else if openArtifact}
    <ArtifactPanel versions={openVersions} initial={openIndex} onclose={() => (openArtifact = null)} />
  {:else if panel}
    <aside aria-label={PANELS.find((p) => p.id === panel)?.label}>
      {#if panel === "environment"}
        <EnvironmentPanel {conversation} artifacts={allArtifacts} onopenartifact={(a) => (openArtifact = a)} {onnavigate} />
      {:else if conversation && panel === "files"}
        <FilesPanel {conversation} />
      {:else if conversation && panel === "terminal"}
        {#key conversation.id}<TerminalPanel {conversation} />{/key}
      {:else if conversation}
        <ChangesPanel {conversation} />
      {/if}
    </aside>
  {/if}
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={lightbox} class="lightbox" aria-label={zoomed?.image.name ?? "Imagem"} onclose={() => (zoomed = null)} onclick={(e) => e.target === lightbox && (zoomed = null)}>
  {#if zoomed}
    {@const sent = zoomed}
    {@const image = sent.image}
    <div class="lightbox-bar">
      <span class="lightbox-name">{image.name}</span>
      <button type="button" title={imageCopied && imageCopied !== "ok" ? imageCopied : "Copiar a imagem"} onclick={() => copyImage(sent)}>{imageCopied === "ok" ? "Copiada" : imageCopied ? "Não copiou" : "Copiar"}</button>
      <button type="button" title="Fechar (Esc)" aria-label="Fechar" onclick={() => (zoomed = null)}>×</button>
    </div>
    <img src={imageSrc(image)} alt={image.name} />
  {/if}
</dialog>
</div>

<style>
  .split { flex: 1; min-height: 0; display: flex; }
  main { flex: 1 1 420px; min-width: 0; height: 100%; display: flex; flex-direction: column; }
  aside { flex: 0 0 340px; min-width: 0; height: 100%; display: flex; flex-direction: column; border-left: 1px solid var(--border); background: var(--bg-side); }

  header { position: relative; display: flex; align-items: center; gap: 10px; min-height: 48px; padding: 4px 16px 4px 20px; border-bottom: 1px solid var(--border); }
  /* Selo do que espera você: ações dos cartões "Ação sua" ou o plano para aprovar. */
  .pend { display: inline-flex; align-items: center; gap: 7px; height: 28px; padding: 0 11px; border: 1px solid var(--warn-line); border-radius: 999px; background: var(--warn-solid); color: var(--warn-text); font-size: 12px; white-space: nowrap; }
  .pend b { font-variant-numeric: tabular-nums; }
  .pend.plan { border-color: var(--acc-line); background: var(--acc-solid); color: var(--accent-text); }
  .pend.clear { border-color: var(--ok-line); background: var(--ok-solid); color: var(--ok-text); }
  .pend-dot { width: 7px; height: 7px; border-radius: 50%; background: currentColor; animation: ping 2s var(--ease) infinite; }
  @keyframes ping { 0% { box-shadow: 0 0 0 0 currentColor; } 80%, 100% { box-shadow: 0 0 0 6px transparent; } }
  /* Barra fina que corre enquanto o agente trabalha. */
  .progress { position: absolute; left: 0; right: 0; bottom: -1px; height: 2px; overflow: hidden; }
  .progress::after { content: ""; position: absolute; inset: 0; width: 40%; background: linear-gradient(90deg, transparent, var(--accent-text), var(--accent-2), transparent); animation: run 1.6s var(--inout) infinite; }
  @keyframes run { from { transform: translateX(-100%); } to { transform: translateX(260%); } }
  .crumb { padding: 0; border: 0; background: transparent; color: var(--muted); white-space: nowrap; }
  .crumb:hover { color: var(--text); }
  .slash { color: var(--faint); }
  h1 { flex: 1; min-width: 0; font-size: 13.5px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .panels { display: flex; gap: 2px; padding: 3px; border: 1px solid var(--border); border-radius: 9px; background: var(--bg-side); }
  .panels button { width: 32px; height: 28px; display: grid; place-items: center; border: 0; border-radius: 6px; background: transparent; color: var(--muted); }
  .panels button:hover { color: var(--text); }
  .panels button.on { background: var(--hover); color: #fff; }

  .stage { position: relative; flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .scroll { flex: 1; overflow-y: auto; padding: 28px 44px 20px 20px; }
  /* Com um arquivo aberto, o chat fica na faixa de baixo. */
  .stage.below { flex: 1 1 0; min-height: 120px; }
  .stage.below .scroll { padding-top: 16px; }
  .viewer { flex: 2 1 0; min-height: 0; display: flex; flex-direction: column; border-bottom: 1px solid var(--border-strong); }
  .viewer-head { display: flex; align-items: center; gap: 10px; min-height: 38px; padding: 0 8px 0 20px; border-bottom: 1px solid var(--border); background: var(--bg-side); }
  .viewer-head .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12.5px; }
  .viewer-head .info { flex-shrink: 0; font-size: 12px; }
  .no-preview { padding: 20px; }
  .thread { max-width: 720px; margin: 0 auto; display: flex; flex-direction: column; gap: 16px; }
  .blk { display: flex; flex-direction: column; min-width: 0; }
  .welcome { padding: 96px 0 0; text-align: center; display: flex; flex-direction: column; gap: 8px; }
  .welcome h2 { font-family: var(--display); font-size: 26px; font-weight: 600; letter-spacing: -0.02em; }

  .queued .user { opacity: 0.6; border-style: dashed; }
  .queue-actions { display: flex; align-items: center; gap: 6px; }
  .user-wrap { align-self: flex-end; max-width: 520px; display: flex; flex-direction: column; align-items: flex-end; gap: 4px; }
  .user { padding: 10px 14px; border: 1px solid var(--border); border-radius: 14px 14px 4px 14px; background: var(--bubble); white-space: pre-wrap; overflow-wrap: anywhere; }
  .sent-images { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; }
  .sent-image { padding: 0; border: 1px solid var(--border); border-radius: 10px; overflow: hidden; background: var(--bg); cursor: zoom-in; }
  .sent-image img { display: block; max-width: 240px; max-height: 180px; object-fit: cover; }
  .sent-image:hover { border-color: var(--border-strong); }
  .lightbox { width: 100vw; height: 100vh; max-width: none; max-height: none; margin: 0; border: 0; color: inherit; }
  .lightbox::backdrop { background: transparent; }
  .lightbox[open] { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; padding: 24px; background: rgba(0, 0, 0, 0.78); cursor: zoom-out; }
  .lightbox img { max-width: 100%; max-height: calc(100% - 52px); object-fit: contain; border-radius: 8px; box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5); cursor: default; }
  .lightbox-bar { display: flex; align-items: center; gap: 8px; max-width: 100%; cursor: default; }
  .lightbox-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12.5px; color: rgba(255, 255, 255, 0.8); }
  .lightbox-bar button { min-height: 28px; padding: 0 12px; border: 1px solid rgba(255, 255, 255, 0.25); border-radius: 8px; background: rgba(255, 255, 255, 0.1); color: #fff; font-size: 12.5px; }
  .lightbox-bar button:hover { background: rgba(255, 255, 255, 0.2); }
  .msg-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 2px; max-width: 100%; opacity: 0; transition: opacity 0.12s; }
  .user-wrap:hover .msg-actions, .user-wrap:focus-within .msg-actions, .assistant:hover .msg-actions, .assistant:focus-within .msg-actions, .msg-actions.shown { opacity: 1; }
  .msg-actions button { min-height: 24px; padding: 0 8px; border: 0; border-radius: 6px; background: transparent; font-size: 12px; color: var(--muted); }
  .msg-actions button:hover { background: var(--hover); color: var(--text); }
  .msg-actions button.on { color: var(--accent-text); }
  /* Nas respostas anteriores, as ações flutuam no canto ao passar o mouse. */
  .assistant > .msg-actions:not(.shown) { position: absolute; top: -16px; right: 4px; z-index: 3; padding: 3px; border: 1px solid var(--border-strong); border-radius: 10px; background: var(--surface-2); box-shadow: 0 12px 30px -14px rgba(0, 0, 0, 0.8); transform: translateY(4px) scale(0.97); pointer-events: none; transition: opacity 0.2s var(--ease), transform 0.3s var(--spring); }
  .assistant:hover > .msg-actions:not(.shown), .assistant:focus-within > .msg-actions:not(.shown) { transform: none; pointer-events: auto; }
  .msg-actions .when { padding: 0 8px; font-size: 12px; color: var(--muted); font-variant-numeric: tabular-nums; white-space: nowrap; transition: opacity 0.12s; }
  /* Na última resposta os botões ficam sempre à vista, mas a hora segue só ao passar o mouse. */
  .msg-actions.shown .when { opacity: 0; }
  .assistant:hover .msg-actions.shown .when, .assistant:focus-within .msg-actions.shown .when { opacity: 1; }
  .editing { display: flex; align-items: center; gap: 10px; padding: 6px 8px; border-radius: 8px; background: var(--surface); font-size: 12px; color: var(--muted); }
  .editing span { flex: 1; }
  .assistant { position: relative; display: flex; flex-direction: column; gap: 10px; min-width: 0; color: var(--text-body); border-radius: 14px; transition: box-shadow 0.4s; }
  /* A explicação entre duas sequências de ações fica na linha do tempo, sob a etapa. */
  .assistant.narr { padding-left: 36px; }
  .assistant.narr::before { content: ""; position: absolute; left: 11.5px; top: -16px; bottom: -16px; width: 1px; background: var(--rail); }
  .assistant.pinned { box-shadow: 0 0 0 1px var(--acc-line), 0 0 0 8px var(--acc-bg); }
  :global(.flash) { animation: flash 1.2s var(--ease); }
  @keyframes flash { 30% { box-shadow: 0 0 0 6px rgba(142, 153, 244, 0.25); } }
  .context { align-self: flex-start; padding: 2px 9px; border: 1px solid var(--border); border-radius: 6px; font-size: 12px; color: var(--muted); }
  .notice { display: flex; align-items: center; gap: 12px; font-size: 12px; color: var(--muted); }
  .notice span { flex: 1; height: 1px; background: var(--border); }
  .alert { padding: 14px 16px; display: flex; flex-direction: column; align-items: flex-start; gap: 6px; border-color: var(--border-strong); }
  .alert .row-w { display: flex; flex-wrap: wrap; gap: 8px; }
  .agents { display: flex; flex-direction: column; gap: 6px; }
  .agent-chip { display: flex; align-items: center; gap: 10px; min-height: 36px; padding: 0 12px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); text-align: left; color: var(--text); }
  .agent-chip:hover, .agent-chip.on { background: var(--hover); border-color: var(--border-strong); }
  .agent-chip .pulse { flex-shrink: 0; }
  .agent-name { flex-shrink: 0; max-width: 45%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; font-weight: 500; }
  .agent-activity { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; color: var(--muted); }
  .agent-chip svg { flex-shrink: 0; color: var(--muted); }
  .working { display: flex; align-items: center; gap: 8px; font-size: 12.5px; color: var(--muted); }
  .pulse { width: 7px; height: 7px; border-radius: 50%; background: var(--accent-text); animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.25; } }

  .artifact { align-self: flex-start; display: flex; align-items: center; gap: 12px; padding: 9px 14px 9px 12px; border: 1px solid var(--border-strong); border-radius: 10px; background: var(--surface); text-align: left; color: var(--text); }
  .artifact:hover { background: var(--hover); }
  .artifact span { display: flex; flex-direction: column; }
  .artifact .muted { font-size: 12px; }
  .design-link { display: flex; align-items: center; gap: 12px; }
  .design-link .lnk { font-size: 12px; color: var(--muted); background: none; border: none; padding: 0; }
  .design-link .lnk:hover { color: var(--text); text-decoration: underline; }

  .markdown { overflow-wrap: anywhere; user-select: text; }
  .markdown :global(p) { margin: 0 0 12px; }
  .markdown :global(> :last-child) { margin-bottom: 0; }
  .markdown :global(pre) { margin: 0 0 12px; padding: 12px 14px; overflow-x: auto; border: 1px solid var(--border); border-radius: 10px; background: var(--bg-side); }
  .markdown :global(pre code) { background: transparent; color: var(--code-text); }
  .markdown :global(code) { font-family: var(--mono); font-size: 12.5px; }
  .markdown :global(:not(pre) > code) { padding: 1px 5px; border-radius: 4px; background: var(--surface-2); }
  .markdown :global(ul), .markdown :global(ol) { margin: 0 0 12px; padding-left: 22px; }
  .markdown :global(h1), .markdown :global(h2), .markdown :global(h3) { margin: 16px 0 8px; font-size: 15px; font-weight: 600; color: var(--text); }
  .markdown :global(strong) { color: var(--text); }
  .markdown :global(table) { border-collapse: collapse; margin-bottom: 12px; display: block; overflow-x: auto; }
  .markdown :global(th), .markdown :global(td) { padding: 6px 10px; border: 1px solid var(--border); text-align: left; }
  .markdown :global(blockquote) { margin: 0 0 12px; padding-left: 12px; border-left: 2px solid var(--border-strong); color: var(--text-2); }
  .markdown :global(:not(pre) > code) { cursor: copy; position: relative; }
  .markdown :global(:not(pre) > code:hover) { box-shadow: 0 0 0 1px var(--acc-line); }
  .markdown :global(code.copied::after) { content: "Copiado"; position: absolute; left: 50%; bottom: calc(100% + 6px); translate: -50% 0; padding: 4px 7px; border-radius: 6px; background: var(--text); color: var(--bg); font: 500 11px/1 var(--font); white-space: nowrap; pointer-events: none; animation: tip 1.2s var(--ease) forwards; }
  @keyframes tip { 0% { opacity: 0; transform: translateY(4px); } 15%, 75% { opacity: 1; transform: none; } 100% { opacity: 0; } }

  /* Cartões de destaque ("> [!ENTREGUE]", "> [!ACAO]", "> [!FORA]", "> [!ERRO]"). */
  .markdown :global(.callout) { --tone: var(--ok); --tone-solid: var(--ok-solid); --tone-line: var(--ok-line); position: relative; isolation: isolate; margin: 4px 0 14px; padding: 14px 16px; border: 1px solid var(--tone-line); border-radius: 14px; background: linear-gradient(var(--tone-solid), var(--tone-solid)) padding-box; animation: callout-in 0.6s var(--ease) both; }
  .markdown :global(.callout.act) { --tone: var(--warn); --tone-solid: var(--warn-solid); --tone-line: var(--warn-line); }
  .markdown :global(.callout.note) { --tone: var(--note); --tone-solid: var(--note-bg); --tone-line: var(--note-line); }
  .markdown :global(.callout.bad) { --tone: var(--bad); --tone-solid: var(--bad-solid); --tone-line: var(--bad-line); }
  .markdown :global(.callout::before) { content: ""; position: absolute; inset: -1px; z-index: -1; border-radius: inherit; padding: 1px; background: conic-gradient(from var(--ang), transparent 0 70%, var(--tone) 85%, transparent 100%); -webkit-mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0); -webkit-mask-composite: xor; mask-composite: exclude; mask: linear-gradient(#000 0 0) content-box exclude, linear-gradient(#000 0 0); opacity: 0; animation: sweep 1.8s var(--ease) 0.15s; pointer-events: none; }
  .markdown :global(.co-head) { display: flex; align-items: center; gap: 12px; list-style: none; }
  .markdown :global(summary.co-head) { cursor: pointer; }
  .markdown :global(summary.co-head::-webkit-details-marker) { display: none; }
  .markdown :global(.co-ic) { width: 30px; height: 30px; flex-shrink: 0; display: grid; place-items: center; border-radius: 9px; color: var(--tone); background: color-mix(in oklab, var(--tone) 16%, transparent); }
  .markdown :global(.co-ic svg) { width: 16px; height: 16px; }
  .markdown :global(.co-tx) { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .markdown :global(.co-k) { font-size: 10.5px; font-weight: 600; line-height: 1; letter-spacing: 0.1em; text-transform: uppercase; color: var(--tone); }
  .markdown :global(.co-t) { font-family: var(--display); font-size: 16px; font-weight: 600; line-height: 1.25; color: var(--text); text-wrap: balance; }
  .markdown :global(.co-chev) { width: 16px; height: 16px; flex-shrink: 0; color: var(--muted); transition: transform 0.35s var(--spring); }
  .markdown :global(details[open] .co-chev) { transform: rotate(180deg); }
  .markdown :global(.co-body) { margin-top: 12px; }
  .markdown :global(.co-body > :last-child) { margin-bottom: 0; }
  .markdown :global(.co-count) { flex-shrink: 0; padding: 2px 9px; border-radius: 999px; font-family: var(--mono); font-size: 12px; font-weight: 600; color: var(--tone); background: color-mix(in oklab, var(--tone) 16%, transparent); font-variant-numeric: tabular-nums; transition: color 0.3s, background 0.3s; }
  .markdown :global(.co-count.all) { color: var(--ok); background: color-mix(in oklab, var(--ok) 16%, transparent); }
  .markdown :global(.co-action) { margin-top: 10px; min-height: 30px; padding: 0 12px; border: 1px solid var(--note-line); border-radius: 8px; background: var(--surface); color: var(--note); font-size: 12.5px; }
  .markdown :global(.co-action:hover) { background: var(--hover); }
  .markdown :global(.tiles) { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 8px; margin: 0 0 12px; }
  .markdown :global(.tile) { display: flex; flex-direction: column; gap: 3px; padding: 9px 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); transition: transform 0.3s var(--spring), border-color 0.2s; }
  .markdown :global(.tile:hover) { transform: translateY(-2px); border-color: var(--border-strong); }
  .markdown :global(.tl) { font-size: 11px; color: var(--muted); }
  .markdown :global(.tv) { font-size: 13.5px; font-weight: 500; color: var(--text); }
  /* Ações do usuário: cada item vira uma linha marcável. */
  .markdown :global(.callout.act .co-body > ul), .markdown :global(.callout.act .co-body > ol) { list-style: none; padding: 0; display: flex; flex-direction: column; gap: 8px; }
  .markdown :global(li.task) { position: relative; padding: 11px 12px 11px 46px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); transition: border-color 0.2s, background 0.3s; }
  .markdown :global(li.task:hover) { border-color: var(--warn-line); }
  .markdown :global(li.task.done) { background: var(--ok-solid); border-color: var(--ok-line); }
  .markdown :global(li.task.done > :not(.task-box)) { color: var(--muted); text-decoration: line-through; text-decoration-color: var(--faint); }
  .markdown :global(.task-box) { position: absolute; left: 12px; top: 11px; width: 22px; height: 22px; padding: 0; display: grid; place-items: center; border: 1.5px solid var(--border-strong); border-radius: 7px; background: transparent; color: #0c0e13; transition: background 0.25s, border-color 0.25s, transform 0.35s var(--spring); }
  .markdown :global(.task-box svg) { width: 13px; height: 13px; stroke-dasharray: 20; stroke-dashoffset: 20; transition: stroke-dashoffset 0.35s var(--ease) 0.08s; }
  .markdown :global(.task-box[aria-checked="true"]) { border-color: var(--ok); background: var(--ok); transform: scale(1.06); }
  .markdown :global(.task-box[aria-checked="true"] svg) { stroke-dashoffset: 0; }
  @keyframes callout-in { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
  @keyframes sweep { 0% { opacity: 0; --ang: 0deg; } 20%, 80% { opacity: 1; } 100% { opacity: 0; --ang: 360deg; } }

  .composer-wrap { padding: 0 20px 12px; display: flex; justify-content: center; }
  .composer-area { position: relative; width: 100%; max-width: 720px; display: flex; flex-direction: column; gap: 6px; }
  .composer { position: relative; padding: 6px 8px; border: 1px solid var(--border-strong); border-radius: 14px; background: var(--surface-2); display: flex; flex-direction: column; gap: 8px; box-shadow: 0 18px 40px -20px rgba(0, 0, 0, 0.8); }
  /* Em foco, uma luz percorre a borda do campo. */
  .composer::before { content: ""; position: absolute; inset: -1px; border-radius: inherit; padding: 1px; background: conic-gradient(from var(--ang), var(--accent), var(--accent-2), var(--ok), var(--accent)); -webkit-mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0); -webkit-mask-composite: xor; mask-composite: exclude; mask: linear-gradient(#000 0 0) content-box exclude, linear-gradient(#000 0 0); opacity: 0; transition: opacity 0.4s; pointer-events: none; }
  .composer:focus-within { border-color: transparent; }
  .composer:focus-within::before { opacity: 1; animation: orbit 4s linear infinite; }
  @keyframes orbit { to { --ang: 360deg; } }
  .suggest { display: flex; gap: 6px; overflow-x: auto; scrollbar-width: none; }
  .sg { display: inline-flex; align-items: center; gap: 7px; flex-shrink: 0; height: 30px; padding: 0 12px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface); font-size: 12.5px; color: var(--text-2); white-space: nowrap; transition: border-color 0.2s, color 0.2s, transform 0.3s var(--spring); animation: callout-in 0.5s var(--ease) both; animation-delay: calc(var(--i) * 80ms); }
  .sg:hover { border-color: var(--acc-line); color: var(--text); transform: translateY(-2px); }
  .sg svg { color: var(--accent-text); }
  .composer > .files { padding: 4px 4px 0; }
  .entry { display: flex; align-items: flex-end; gap: 6px; }
  textarea { flex: 1; min-width: 0; max-height: 240px; padding: 6px 2px; border: 0; outline: 0; resize: none; background: transparent; font-size: 14px; line-height: 1.45; field-sizing: content; }
  .spacer { flex: 1; }
  .tool { width: 32px; height: 32px; flex-shrink: 0; display: grid; place-items: center; border: 0; border-radius: 8px; background: transparent; color: var(--text-2); }
  .tool:hover, .pick:hover, .account:hover { background: var(--hover); color: var(--text); }
  .send { width: 30px; height: 30px; margin-bottom: 1px; flex-shrink: 0; display: grid; place-items: center; border: 0; border-radius: 9px; background: var(--accent); color: #fff; }
  .send:hover { background: var(--accent-hover); }
  .gen { height: 30px; margin-bottom: 1px; flex-shrink: 0; display: inline-flex; align-items: center; gap: 6px; padding: 0 10px; border: 1px solid var(--border-strong); border-radius: 9px; background: transparent; color: var(--text-2); font-size: 12.5px; white-space: nowrap; }
  .gen:hover:not(:disabled) { background: var(--hover); color: var(--text); }
  .gen:disabled { opacity: 0.8; }
  .spinner { width: 11px; height: 11px; border: 2px solid var(--border-strong); border-top-color: var(--accent); border-radius: 50%; animation: rewrite-spin 0.8s linear infinite; }
  @keyframes rewrite-spin { to { transform: rotate(360deg); } }
  .rewrite-error { margin: 0; padding: 2px 4px 0; }
  .send.stop { background: var(--hover); border: 1px solid var(--border-strong); color: var(--text); }

  .status-line { display: flex; flex-wrap: wrap; align-items: center; gap: 2px; padding: 0 2px; font-size: 12.5px; color: var(--muted); }
  .info { display: flex; align-items: center; gap: 6px; margin-right: 12px; font-size: 12px; }
  .account { display: flex; align-items: center; gap: 6px; min-height: 28px; padding: 0 8px; border: 0; border-radius: 7px; background: transparent; color: var(--text-2); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); }
  .dot.none { background: var(--warn); }
  .pick { appearance: none; min-height: 28px; max-width: 170px; padding: 0 8px; border: 0; border-radius: 7px; background: transparent; color: var(--text-2); text-overflow: ellipsis; }

  .commands { position: absolute; left: 0; right: 0; bottom: calc(100% + 8px); z-index: 20; padding: 6px; border: 1px solid var(--border-strong); border-radius: 12px; background: var(--surface-2); box-shadow: 0 16px 40px rgba(0, 0, 0, 0.5); display: flex; flex-direction: column; gap: 1px; }
  .commands button { display: flex; align-items: baseline; gap: 12px; min-height: 32px; padding: 6px 10px; border: 0; border-radius: 7px; background: transparent; text-align: left; color: var(--text); }
  .commands button:hover, .commands button.on { background: var(--hover); }
  .commands .mono { flex-shrink: 0; font-size: 12.5px; }
  .commands .muted { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }

  .thumbs { display: flex; flex-wrap: wrap; gap: 8px; padding: 4px 4px 0; }
  .thumb { position: relative; width: 72px; height: 72px; border: 1px solid var(--border-strong); border-radius: 8px; overflow: hidden; background: var(--bg); }
  .thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .thumb button { position: absolute; top: 3px; right: 3px; width: 18px; height: 18px; border: 0; border-radius: 50%; background: rgba(0, 0, 0, 0.7); color: #fff; font-size: 13px; line-height: 1; }
  .files { display: flex; flex-wrap: wrap; gap: 6px; }
  .user .files { margin-bottom: 8px; }
  .file { display: inline-flex; align-items: center; gap: 6px; padding: 2px 8px; border: 1px solid var(--border-strong); border-radius: 6px; background: var(--bg); font-size: 12px; color: var(--text-2); white-space: nowrap; }
  .file button { border: 0; padding: 0 2px; background: transparent; color: var(--muted); font-size: 14px; line-height: 1; }

</style>
