<script lang="ts">
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import { formatTime, nowSeconds, percent } from "../format";
  import { accounts, stateOf } from "../state/accounts.svelte";
  import { api, errorMessage } from "../api";
  import { chat, openConversation, removeConversation, startAgentSession, startIsolatedSession } from "../state/chat.svelte";
  import { projects } from "../state/projects.svelte";
  import { pipelines } from "../state/pipelines.svelte";
  import type { Account, Conversation, Project, View } from "../types";
  import Logo from "./Logo.svelte";
  import NewConversationDialog from "./NewConversationDialog.svelte";

  interface Props {
    view: View;
    onnavigate: (view: View) => void;
  }
  let { view, onnavigate }: Props = $props();

  let search = $state("");
  let searching = $state(false);
  let choosing = $state(false);
  /** Grupos recolhidos (chave: id do projeto; "" é "Sem projeto"). */
  let collapsed = $state<Record<string, boolean>>({});
  /** Quantas sessões cada projeto mostra antes do "Ver todas"; grupos abertos por inteiro (mesma chave). */
  const RECENT = 10;
  let expanded = $state<Record<string, boolean>>({});
  /** As mais recentes do grupo; na busca, aberto por inteiro ou com poucas, todas. A aberta, a que espera
      você e a que está respondendo aparecem mesmo além das 10. */
  function visible(id: string, list: Conversation[]): Conversation[] {
    if (search || expanded[id] || list.length <= RECENT) return list;
    return list.filter((c, i) => i < RECENT || c.id === chat.activeId || !!chat.permissions[c.id] || !!chat.turns[c.id]);
  }
  let error = $state<string | null>(null);

  /** Sessão em uma cópia isolada do repositório do projeto. */
  async function startIsolated(project: Project) {
    error = null;
    try {
      await startIsolatedSession(project.id);
      onnavigate("chat");
    } catch (e) {
      error = errorMessage(e);
    }
  }

  const NO_PROJECT = "";
  const PASTA = "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z";
  const BALAO = "M4 5h16v11H9l-5 4z";

  const SHORTCUTS: { id: View; label: string; hint: string; icon: string }[] = [
    { id: "skills", label: "Skills e agentes", hint: "Skills instaladas, agentes com instruções próprias e a loja de plugins", icon: "M5 4h11a3 3 0 0 1 3 3v13H8a3 3 0 0 1-3-3zM5 17a3 3 0 0 1 3-3h11" },
    { id: "connectors", label: "Conectores", hint: "Serviços e servidores MCP que o assistente pode usar", icon: "M9 3v5M15 3v5M6 8h12v3a6 6 0 0 1-12 0zM12 17v4" },
    { id: "pipelines", label: "Esteira de bots", hint: "Bots com papéis fixos resolvem tasks do Jira ou pedidos seus, com a sua aprovação do plano", icon: "M3 12h3M18 12h3M9 12h6M6 9h3v6H6zM15 9h3v6h-3zM10.5 5h3l-1.5-2zM12 5v4" },
    { id: "tasks", label: "Tarefas agendadas", hint: "Pedidos que rodam sozinhos em horários definidos", icon: "M8 3v3M16 3v3M4 8h16M5 5h14a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zM9 14l2 2 4-4" },
    { id: "history", label: "Histórico importado", hint: "Conversas antigas do Claude Code e do Claude Desktop usadas como contexto", icon: "M12 7v5l3 2M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0z" },
    { id: "settings", label: "Configurações", hint: "Preferências do Lume, como a forma de ver a esteira de bots", icon: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" },
  ];

  const matches = $derived(
    chat.conversations.filter((c) => c.title.toLowerCase().includes(search.trim().toLowerCase())),
  );

  type ProjectOrder = "recent" | "name" | "created";
  const ORDER_LABEL: Record<ProjectOrder, string> = { recent: "Alterados recentemente", name: "Nome (A–Z)", created: "Ordem de criação" };
  const ORDER_KEY = "lume.projectOrder";

  function isOrder(value: unknown): value is ProjectOrder {
    return value === "recent" || value === "name" || value === "created";
  }
  function savedOrder(): ProjectOrder {
    try {
      const saved = localStorage.getItem(ORDER_KEY);
      return isOrder(saved) ? saved : "created";
    } catch {
      return "created";
    }
  }
  /** Ordem dos projetos na lista, gravada nas configurações do app. */
  let order = $state<ProjectOrder>(savedOrder());
  let ordering = $state(false);
  /** Escolha feita antes de a gravada chegar não é desfeita por ela. */
  let chosen = false;

  api
    .getProjectOrder()
    .then((saved) => {
      if (chosen) return;
      if (isOrder(saved)) {
        order = saved;
        rememberLocally(saved);
      } else if (order !== "created") {
        // Escolha antiga, de quando só ficava no navegador: passa para as configurações.
        api.setProjectOrder(order).catch(() => {});
      }
    })
    .catch(() => {});

  function rememberLocally(value: ProjectOrder) {
    try {
      localStorage.setItem(ORDER_KEY, value);
    } catch {
      // Sem armazenamento local vale a cópia das configurações.
    }
  }

  function chooseOrder(next: ProjectOrder) {
    chosen = true;
    order = next;
    ordering = false;
    rememberLocally(next);
    api.setProjectOrder(next).catch(() => {});
  }

  /** Conversas agrupadas por projeto, na ordem escolhida; "Sem projeto" vem por último. */
  const groups = $derived.by(() => {
    const known = new Set(projects.list.map((p) => p.id));
    const groupOf = (c: Conversation) => (c.project_id && known.has(c.project_id) ? c.project_id : NO_PROJECT);
    const of = (id: string) => matches.filter((c) => groupOf(c) === id);
    // Conta todas as conversas do grupo, não só as que a busca mostra: o aviso não some ao buscar.
    const waitingIn = new Set(chat.conversations.filter((c) => chat.permissions[c.id]).map(groupOf));
    const named = projects.list.map((p) => ({
      id: p.id,
      name: p.name,
      icon: PASTA,
      project: p as Project | null,
      conversations: of(p.id),
      waiting: waitingIn.has(p.id),
    }));
    const latest = (g: { conversations: Conversation[] }) => Math.max(0, ...g.conversations.map((c) => c.updated_at));
    if (order === "recent") named.sort((a, b) => latest(b) - latest(a));
    if (order === "name") named.sort((a, b) => a.name.localeCompare(b.name, "pt-BR", { sensitivity: "base" }));
    const loose = {
      id: NO_PROJECT,
      name: "Sem projeto",
      icon: BALAO,
      project: null as Project | null,
      conversations: of(NO_PROJECT),
      waiting: waitingIn.has(NO_PROJECT),
    };
    const all = [...named, loose];
    return search.trim() ? all.filter((g) => g.conversations.length || g.waiting) : all;
  });

  function when(c: Conversation): string {
    const days = Math.floor((nowSeconds() - c.updated_at) / 86_400);
    if (days < 1) return formatTime(c.updated_at);
    if (days < 7) return `${days} d`;
    return new Intl.DateTimeFormat("pt-BR", { day: "numeric", month: "short" }).format(new Date(c.updated_at * 1000));
  }

  function accountNote(a: Account): string {
    const state = stateOf(a);
    if (state === "limited" && a.limited_until) return formatTime(a.limited_until);
    return { login: "login", disabled: "pausada", limited: "limite", active: "em uso", available: "na fila" }[state];
  }

  function toggleSearch() {
    searching = !searching;
    search = "";
  }

  /** Conversa comum, sem pasta e sem projeto. */
  async function startChat() {
    choosing = false;
    await open(null);
  }

  /** Escolhe uma pasta e abre uma sessão nela; a pasta vira um projeto. */
  async function startInFolder() {
    choosing = false;
    const folder = await pickFolder({ directory: true, title: "Pasta do projeto" });
    if (typeof folder !== "string") return;
    await startAgentSession(folder);
    onnavigate("chat");
  }

  /** Nova sessão dentro de um grupo: na pasta do projeto, ou uma conversa do projeto. */
  async function startIn(project: Project | null) {
    if (project?.folder) {
      await startAgentSession(project.folder);
    } else {
      projects.pendingId = project?.id ?? null;
      await openConversation(null);
    }
    onnavigate("chat");
  }

  async function open(id: string | null) {
    if (id === null) projects.pendingId = null;
    await openConversation(id);
    onnavigate("chat");
  }

  function openProject(id: string | null) {
    projects.selectedId = id;
    onnavigate("project");
  }
</script>

<nav aria-label="Navegação principal">
  <div class="brand">
    <Logo />
    <span class="name">Lume</span>
    <button type="button" class="btn icon" aria-label="Buscar conversas" title="Buscar conversas" aria-pressed={searching} onclick={toggleSearch}>
      <svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="11" cy="11" r="6.5" /><path d="M16 16l4.5 4.5" /></svg>
    </button>
  </div>

  {#if searching}
    <!-- svelte-ignore a11y_autofocus -->
    <input class="input search" type="search" aria-label="Buscar conversas" placeholder="Buscar conversas" bind:value={search} autofocus />
  {/if}

  <div class="create">
    <button type="button" class="btn grow" title="Começar uma conversa ou abrir uma pasta de projeto" aria-haspopup="dialog" onclick={() => (choosing = true)}>
      <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
      Nova conversa
    </button>
  </div>

  <div class="shortcuts">
    {#each SHORTCUTS as item (item.id)}
      <button type="button" class="item" class:active={view === item.id} aria-current={view === item.id ? "page" : undefined} title={item.hint} onclick={() => onnavigate(item.id)}>
        <svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d={item.icon} /></svg>
        {item.label}
        {#if item.id === "pipelines" && pipelines.list.some((r) => r.status === "awaiting_approval" || r.status === "awaiting_choice")}<span class="waiting-dot" title="Uma execução espera a sua decisão"></span>{/if}
      </button>
    {/each}
  </div>

  {#if error}<p class="error-text small" role="alert">{error}</p>{/if}

  <div class="projects">
    <div class="heading">
      <span class="label">Projetos</span>
      <span class="order">
        <button type="button" class="btn icon tiny" aria-label={`Ordenar projetos: ${ORDER_LABEL[order]}`} title={`Ordem: ${ORDER_LABEL[order]}`} aria-haspopup="menu" aria-expanded={ordering} onclick={() => (ordering = !ordering)}>
          <svg aria-hidden="true" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M4 7h16M4 12h10M4 17h5" /></svg>
        </button>
        {#if ordering}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="menu" role="menu" tabindex="-1" aria-label="Ordem dos projetos" onkeydown={(e) => e.key === "Escape" && (ordering = false)}>
            {#each Object.entries(ORDER_LABEL) as [key, label] (key)}
              <button type="button" role="menuitemradio" aria-checked={order === key} class:on={order === key} onclick={() => chooseOrder(key as ProjectOrder)}>
                <span class="tick" aria-hidden="true">{order === key ? "✓" : ""}</span>{label}
              </button>
            {/each}
          </div>
        {/if}
      </span>
      <button type="button" class="btn icon tiny" aria-label="Novo projeto" title="Novo projeto" onclick={() => openProject(null)}>
        <svg aria-hidden="true" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
      </button>
    </div>

    {#each groups as g (g.id)}
      <div class="group">
        <div class="group-head" class:active={view === "project" && projects.selectedId === g.id && g.id !== NO_PROJECT}>
          <button type="button" class="toggle" aria-expanded={!collapsed[g.id]} aria-label={`${collapsed[g.id] ? "Expandir" : "Recolher"} ${g.name}`} title={`${collapsed[g.id] ? "Expandir" : "Recolher"} ${g.name}`} onclick={() => (collapsed[g.id] = !collapsed[g.id])}>
            <svg aria-hidden="true" class:closed={collapsed[g.id]} width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6" /></svg>
          </button>
          {#if g.id === NO_PROJECT}
            <span class="group-name">
              <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d={g.icon} /></svg>
              <span class="ellipsis">{g.name}</span>
            </span>
          {:else}
            <button type="button" class="group-name" title="Abrir o projeto" onclick={() => openProject(g.id)}>
              <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d={g.icon} /></svg>
              <span class="ellipsis">{g.name}</span>
            </button>
          {/if}
          {#if g.waiting}
            <span class="dot warn pulse" role="img" aria-label={`Conversa em ${g.name} aguardando você`} title="Uma conversa aqui aguarda sua resposta ou aprovação"></span>
          {/if}
          <span class="count">{g.conversations.length}</span>
          {#if g.project?.folder}
            {@const project = g.project}
            <button type="button" class="btn icon tiny start" aria-label={`Nova sessão isolada em ${g.name}`} title="Sessão isolada (git worktree em um ramo novo)" onclick={() => startIsolated(project)}>
              <svg aria-hidden="true" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><circle cx="6" cy="6" r="2.2" /><circle cx="6" cy="18" r="2.2" /><circle cx="18" cy="8" r="2.2" /><path d="M6 8.2v7.6M18 10.2c0 4-6 3-11 5" /></svg>
            </button>
          {/if}
          <button type="button" class="btn icon tiny start" aria-label={g.project?.folder ? `Nova sessão em ${g.name}` : `Nova conversa em ${g.name}`} title={g.project?.folder ? "Nova sessão nesta pasta" : "Nova conversa"} onclick={() => startIn(g.project)}>
            <svg aria-hidden="true" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
          </button>
        </div>

        {#if !collapsed[g.id]}
          {#each visible(g.id, g.conversations) as c (c.id)}
            <div class="conversation" class:active={view === "chat" && chat.activeId === c.id}>
              <button type="button" class="title" onclick={() => open(c.id)}>
                {#if chat.permissions[c.id]}
                  <span class="dot warn pulse" role="img" aria-label="Aguardando você" title="Aguardando sua resposta ou aprovação"></span>
                {:else if chat.turns[c.id]}<span class="busy" role="img" aria-label="Respondendo"></span>{/if}
                {#if c.kind === "agent"}
                  <svg role="img" aria-label="Sessão de agente" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 8l4 4-4 4M12 17h7" /></svg>
                {/if}
                <span class="ellipsis">{c.title}</span>
              </button>
              <span class="when">{when(c)}</span>
              <button type="button" class="btn icon tiny remove" aria-label={`Excluir a conversa ${c.title}`} title={`Excluir a conversa ${c.title}`} onclick={() => removeConversation(c.id)}>
                <svg aria-hidden="true" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
              </button>
            </div>
          {:else}
            <div class="none">{search ? "Nada encontrado" : "Nenhuma conversa"}</div>
          {/each}
          {#if !search && g.conversations.length > RECENT}
            <button type="button" class="more" aria-expanded={!!expanded[g.id]} onclick={() => (expanded[g.id] = !expanded[g.id])}>
              {expanded[g.id] ? "Mostrar só as recentes" : `Ver todas (${g.conversations.length})`}
            </button>
          {/if}
        {/if}
      </div>
    {/each}
  </div>

  <button type="button" class="pool" class:active={view === "accounts"} title="Contas da assinatura e uso de cada uma; clique para gerenciar" onclick={() => onnavigate("accounts")}>
    <span class="pool-head"><span class="label">Contas</span><span class="manage">Gerenciar</span></span>
    {#each accounts.list as a (a.id)}
      {@const state = stateOf(a)}
      <span class="account">
        <span class="account-head">
          <span class="dot" class:ok={state === "active"} class:warn={state === "limited" || state === "login"}></span>
          <span class="who" class:current={state === "active"} title={a.name}>{a.name}</span>
          <span class="note">{accountNote(a)}</span>
        </span>
        <span class="meter" role="img" aria-label={`${a.name}: ${percent(a.util_5h)}% usado`}>
          <span class:full={state === "limited"} style:width={`${state === "limited" ? 100 : percent(a.util_5h)}%`}></span>
        </span>
      </span>
    {:else}
      <span class="none">Nenhuma conta ainda</span>
    {/each}
  </button>
</nav>

<NewConversationDialog open={choosing} onclose={() => (choosing = false)} onchat={startChat} onfolder={startInFolder} />

<style>
  nav { width: 272px; flex-shrink: 0; height: 100%; padding: 14px 10px 10px; display: flex; flex-direction: column; gap: 14px; background: var(--bg-side); border-right: 1px solid var(--border); font-size: 13px; line-height: 1.4; }
  .brand { display: flex; align-items: center; gap: 9px; padding: 2px 6px; }
  .name { flex: 1; font-family: var(--display); font-size: 16px; font-weight: 600; letter-spacing: -0.01em; }
  .search { width: 100%; min-height: 32px; }
  .create { display: flex; gap: 6px; }
  .create .btn { display: flex; align-items: center; justify-content: center; gap: 7px; padding: 0 11px; }
  .grow { flex: 1; }
  .shortcuts { display: flex; flex-direction: column; gap: 1px; }
  .item { display: flex; align-items: center; gap: 9px; min-height: 30px; padding: 0 8px; border: 0; border-radius: 6px; background: transparent; color: var(--text-2); text-align: left; }
  .item:hover, .item.active { background: var(--hover); }
  .item.active { color: #fff; }
  .waiting-dot { width: 7px; height: 7px; margin-left: auto; border-radius: 50%; background: var(--warn); }

  .projects { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 10px; }
  .heading { display: flex; align-items: center; padding: 0 4px 0 8px; }
  .heading .label { flex: 1; }
  .order { position: relative; }
  .menu { position: absolute; top: 26px; right: 0; z-index: 5; min-width: 200px; padding: 4px; border: 1px solid var(--border-strong); border-radius: 10px; background: var(--surface); box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5); display: flex; flex-direction: column; }
  .menu button { display: flex; align-items: center; gap: 6px; min-height: 28px; padding: 0 8px; border: 0; border-radius: 6px; background: transparent; color: var(--text-2); font-size: 12.5px; text-align: left; }
  .menu button:hover { background: var(--hover); color: var(--text); }
  .menu button.on { color: var(--text); font-weight: 500; }
  .menu .tick { width: 12px; color: var(--accent-text); }
  .tiny { width: 22px; min-height: 22px; }
  .group { display: flex; flex-direction: column; gap: 1px; }
  .group-head { display: flex; align-items: center; gap: 4px; min-height: 28px; padding: 0 8px 0 4px; border-radius: 6px; color: var(--text-2); }
  .group-head.active { background: var(--hover); color: #fff; }
  .toggle { width: 18px; height: 22px; display: grid; place-items: center; border: 0; border-radius: 4px; background: transparent; color: var(--muted); }
  .toggle svg { transition: transform 0.12s; }
  .toggle svg.closed { transform: rotate(-90deg); }
  .group-name { flex: 1; min-width: 0; display: flex; align-items: center; gap: 7px; padding: 0; border: 0; background: transparent; font-weight: 500; color: inherit; text-align: left; }
  button.group-name:hover { color: #fff; }
  .group-name svg { flex-shrink: 0; }
  .count { font-size: 11px; color: var(--muted); }
  .start { display: none; }
  .group-head:hover .start, .group-head:focus-within .start { display: grid; }
  .group-head:hover .count, .group-head:focus-within .count { display: none; }
  .ellipsis { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .conversation { display: flex; align-items: center; gap: 6px; min-height: 28px; margin-left: 13px; padding: 0 4px 0 10px; border-left: 1px solid var(--border); border-radius: 0 6px 6px 0; color: var(--text-2); }
  .conversation:hover, .conversation.active { background: var(--hover); }
  .conversation.active { color: #fff; }
  .title { flex: 1; min-width: 0; min-height: 28px; display: flex; align-items: center; gap: 6px; padding: 0; border: 0; background: transparent; color: inherit; text-align: left; }
  .conversation.active .title { font-weight: 500; }
  .title svg { flex-shrink: 0; color: var(--muted); }
  .busy { width: 6px; height: 6px; flex-shrink: 0; border-radius: 50%; background: var(--accent-text); }

  .when { flex-shrink: 0; font-size: 11px; color: var(--muted); }
  .remove { display: none; }
  .conversation:hover .remove, .conversation:focus-within .remove { display: grid; }
  .conversation:hover .when, .conversation:focus-within .when { display: none; }
  .none { margin-left: 13px; padding: 4px 10px; border-left: 1px solid var(--border); font-size: 12px; color: var(--muted); }
  .more { display: block; margin-left: 13px; padding: 4px 10px; border: 0; border-left: 1px solid var(--border); background: none; font-size: 12px; color: var(--accent-text); text-align: left; cursor: pointer; }
  .more:hover { color: var(--text); }

  .pool { display: flex; flex-direction: column; gap: 8px; padding: 10px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); text-align: left; }
  .pool:hover, .pool.active { border-color: var(--border-strong); }
  .pool-head { display: flex; align-items: center; }
  .pool-head .label { flex: 1; }
  .manage { font-size: 11px; color: var(--accent-text); }
  .account { display: flex; flex-direction: column; gap: 5px; }
  .account-head { display: flex; align-items: center; gap: 8px; }
  .dot { width: 7px; height: 7px; flex-shrink: 0; border-radius: 50%; background: var(--faint); }
  .dot.ok { background: var(--ok); }
  .dot.warn { background: var(--warn); }
  /* Turno parado esperando o usuário (autorização, pergunta ou plano): se destaca do "respondendo". */
  .dot.pulse { animation: pulse 1.6s ease-in-out infinite; }
  @keyframes pulse { 50% { box-shadow: 0 0 0 3px var(--warn-bg); } }
  .who { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .who.current { font-weight: 600; }
  .meter { display: block; width: 100%; height: 4px; border-radius: 2px; background: var(--border); overflow: hidden; }
  .meter span { display: block; height: 100%; border-radius: 2px; background: var(--bar); }
  .meter span.full { background: var(--warn); }
  .note { flex-shrink: 0; text-align: right; font-size: 11px; color: var(--muted); }
  .pool .none { margin: 0; padding: 0; border: 0; }
</style>
