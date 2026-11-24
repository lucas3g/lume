<script lang="ts">
  import { api } from "../api";
  import { formatWhen } from "../format";
  import { chat, loadConversations, openConversation } from "../state/chat.svelte";
  import { openPipeline, pipelines } from "../state/pipelines.svelte";
  import { projectName, projects } from "../state/projects.svelte";
  import { DEFAULT_MODEL, type HistSession, type MessageHit, type View } from "../types";

  interface Props {
    onnavigate: (view: View) => void;
  }
  let { onnavigate }: Props = $props();

  interface Item {
    id: string;
    group: string;
    label: string;
    detail?: string;
    run: () => void | Promise<void>;
  }

  const VIEWS: { id: View; label: string }[] = [
    { id: "chat", label: "Conversas" },
    { id: "pipelines", label: "Esteira de bots" },
    { id: "usage", label: "Uso e custo" },
    { id: "accounts", label: "Contas" },
    { id: "tasks", label: "Tarefas agendadas" },
    { id: "skills", label: "Skills e agentes" },
    { id: "connectors", label: "Conectores" },
    { id: "history", label: "Histórico importado" },
    { id: "settings", label: "Configurações" },
  ];
  const GROUP_LIMIT = 6;

  let open = $state(false);
  let query = $state("");
  let selected = $state(0);
  let hits = $state<MessageHit[]>([]);
  let history = $state<HistSession[]>([]);
  let dialog = $state<HTMLDialogElement>();
  let input = $state<HTMLInputElement>();

  $effect(() => {
    if (open && !dialog?.open) {
      dialog?.showModal();
      input?.focus();
    }
    if (!open && dialog?.open) dialog.close();
  });

  function onWindowKey(event: KeyboardEvent) {
    // No terminal, Ctrl+K é do shell (apaga até o fim da linha): não abre a busca.
    if ((event.target as HTMLElement | null)?.closest?.(".xterm")) return;
    if (event.key.toLowerCase() === "k" && (event.metaKey || event.ctrlKey) && !event.altKey && !event.shiftKey) {
      event.preventDefault();
      open = !open;
      if (open) {
        query = "";
        selected = 0;
      }
    }
  }

  /** Sem diferença de maiúsculas nem acentos. */
  const fold = (text: string) => text.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
  const matches = (text: string | null | undefined, q: string) => !q || fold(text ?? "").includes(q);

  // Mensagens e histórico importado vêm do núcleo: espera uma pausa na digitação.
  $effect(() => {
    const q = query.trim();
    if (!open || q.length < 3) {
      hits = [];
      history = [];
      return;
    }
    const timer = setTimeout(() => {
      void api.searchMessages(q).then((h) => q === query.trim() && (hits = h)).catch(() => {});
      void api.historyList(q).then((h) => q === query.trim() && (history = h.slice(0, 4))).catch(() => {});
    }, 200);
    return () => clearTimeout(timer);
  });

  async function go(view: View) {
    onnavigate(view);
  }

  async function openChat(id: string | null) {
    if (id === null) projects.pendingId = null;
    await openConversation(id);
    await go("chat");
  }

  const items = $derived.by((): Item[] => {
    const q = fold(query.trim());
    const list: Item[] = [];
    const add = (group: string, entries: Item[]) => list.push(...entries.slice(0, GROUP_LIMIT).map((e) => ({ ...e, group })));
    add("Ações", [
      { id: "new-chat", group: "", label: "Nova conversa", run: () => openChat(null) },
      { id: "new-run", group: "", label: "Nova execução da esteira", run: async () => { await openPipeline(null); await go("pipelines"); } },
    ].filter((i) => matches(i.label, q)));
    add("Telas", VIEWS.filter((v) => matches(v.label, q)).map((v) => ({ id: `view-${v.id}`, group: "", label: v.label, run: () => go(v.id) })));
    add("Projetos", projects.list.filter((p) => matches(p.name, q)).map((p) => ({
      id: `project-${p.id}`, group: "", label: p.name, detail: p.folder ?? undefined,
      run: () => { projects.selectedId = p.id; return go("project"); },
    })));
    if (q) {
      add("Conversas", chat.conversations.filter((c) => matches(c.title, q)).map((c) => ({
        id: `conv-${c.id}`, group: "", label: c.title, detail: [projectName(c.project_id), formatWhen(c.updated_at)].filter(Boolean).join(" · "),
        run: () => openChat(c.id),
      })));
      add("Execuções da esteira", pipelines.list.filter((r) => matches(`${r.jira_key ?? ""} ${r.title}`, q)).map((r) => ({
        id: `run-${r.id}`, group: "", label: r.jira_key ? `${r.jira_key} · ${r.title}` : r.title, detail: projectName(r.project_id) ?? undefined,
        run: async () => { await openPipeline(r.id); await go("pipelines"); },
      })));
      const titled = new Set(list.filter((i) => i.id.startsWith("conv-")).map((i) => i.id.slice(5)));
      add("Nas mensagens", hits.filter((h) => !titled.has(h.conversation_id)).map((h) => ({
        id: `hit-${h.conversation_id}`, group: "", label: h.title, detail: h.snippet, run: () => openChat(h.conversation_id),
      })));
      add("Histórico importado", history.map((h) => ({
        id: `hist-${h.id}`, group: "", label: h.title ?? "Sessão sem título", detail: h.cwd ?? undefined,
        run: async () => {
          const conversation = await api.historyOpen(h.id, DEFAULT_MODEL);
          await loadConversations();
          await openChat(conversation.id);
        },
      })));
    }
    return list;
  });

  $effect(() => {
    if (selected >= items.length) selected = Math.max(0, items.length - 1);
  });

  async function choose(item: Item | undefined) {
    if (!item) return;
    open = false;
    await item.run();
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const n = items.length;
      if (n) selected = (selected + (event.key === "ArrowDown" ? 1 : n - 1)) % n;
      dialog?.querySelector(`[data-index="${selected}"]`)?.scrollIntoView({ block: "nearest" });
    } else if (event.key === "Enter") {
      event.preventDefault();
      void choose(items[selected]);
    }
  }

  function onBackdrop(event: MouseEvent) {
    if (event.target === dialog) open = false;
  }
</script>

<svelte:window onkeydown={onWindowKey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={dialog} aria-label="Buscar" onclose={() => (open = false)} onclick={onBackdrop}>
  <div class="content">
    <input bind:this={input} class="search" type="search" placeholder="Buscar conversas, mensagens, execuções, projetos e telas…"
      bind:value={query} onkeydown={onKey} oninput={() => (selected = 0)}
      role="combobox" aria-expanded="true" aria-controls="palette-list" aria-activedescendant={items[selected] ? `palette-${selected}` : undefined} />
    <ul id="palette-list" role="listbox" aria-label="Resultados">
      {#each items as item, i (item.id)}
        {#if i === 0 || items[i - 1].group !== item.group}
          <li class="group" role="presentation">{item.group}</li>
        {/if}
        <li id={`palette-${i}`} data-index={i} role="option" aria-selected={i === selected} class:on={i === selected}
          onmousemove={() => (selected = i)} onclick={() => void choose(item)}>
          <span class="label">{item.label}</span>
          {#if item.detail}<span class="detail">{item.detail}</span>{/if}
        </li>
      {:else}
        <li class="empty" role="presentation">{query.trim().length < 3 ? "Nada com esse nome. Digite ao menos 3 letras para buscar também nas mensagens." : "Nada encontrado."}</li>
      {/each}
    </ul>
    <div class="foot muted"><span>↑↓ escolher</span><span>Enter abrir</span><span>Esc fechar</span></div>
  </div>
</dialog>

<style>
  dialog { width: min(620px, calc(100vw - 32px)); margin-top: 12vh; padding: 0; border: 1px solid var(--border-strong); border-radius: 14px; background: var(--surface); color: var(--text); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6); }
  dialog::backdrop { background: rgba(0, 0, 0, 0.5); }
  .content { display: flex; flex-direction: column; max-height: 70vh; }
  .search { border: 0; border-bottom: 1px solid var(--border); background: transparent; color: var(--text); font-size: 15px; padding: 14px 16px; outline: none; }
  ul { list-style: none; margin: 0; padding: 6px; overflow-y: auto; flex: 1; }
  .group { padding: 10px 10px 4px; font-size: 11.5px; font-weight: 600; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; }
  [role="option"] { display: flex; flex-direction: column; gap: 1px; padding: 7px 10px; border-radius: 8px; cursor: pointer; }
  [role="option"].on { background: var(--hover); }
  .label { font-size: 13.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .detail { font-size: 12px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { padding: 16px 10px; font-size: 13px; color: var(--muted); }
  .foot { display: flex; gap: 16px; padding: 8px 16px; border-top: 1px solid var(--border); font-size: 11.5px; }
</style>
