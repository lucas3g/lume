<script lang="ts" module>
  import type { JiraIssue } from "../../types";

  /** Task escolhida: a da lista ou só a chave digitada. */
  export interface PickedTask {
    key: string;
    site: string | null;
    issue: JiraIssue | null;
  }

  const KEY_RE = /^[A-Za-z][A-Za-z0-9_]*-\d+$/;

  /** "há 3 d" a partir da data ISO do Jira. */
  export function ago(iso: string | null, now = Date.now()): string {
    const t = iso ? Date.parse(iso) : NaN;
    if (Number.isNaN(t)) return "";
    const min = Math.max(0, Math.round((now - t) / 60000));
    if (min < 1) return "agora";
    if (min < 60) return `há ${min} min`;
    const h = Math.round(min / 60);
    if (h < 24) return `há ${h} h`;
    const d = Math.round(h / 24);
    if (d < 30) return `há ${d} d`;
    const m = Math.round(d / 30);
    return m < 12 ? `há ${m} m` : `há ${Math.round(m / 12)} a`;
  }

  /** Cor e forma do ícone pelo tipo da task. */
  export function typeTone(type: string | null): "bug" | "story" | "epic" | "sub" | "task" {
    const t = (type ?? "").toLowerCase();
    if (/bug|defeito|erro/.test(t)) return "bug";
    if (/epic|épico/.test(t)) return "epic";
    if (/sub/.test(t)) return "sub";
    if (/story|história|historia/.test(t)) return "story";
    return "task";
  }

  /** Nível da prioridade: -1 baixa, 0 média, 1 alta, 2 crítica; null quando não se sabe. */
  export function priorityLevel(p: string | null): number | null {
    const t = (p ?? "").toLowerCase();
    if (!t) return null;
    if (/highest|blocker|crít|crit|urgent|mais alta|bloque/.test(t)) return 2;
    if (/high|alta|major/.test(t)) return 1;
    if (/lowest|low|baixa|minor|trivial/.test(t)) return -1;
    return 0;
  }

  export function initials(name: string): string {
    const parts = name.trim().split(/\s+/);
    return ((parts[0]?.[0] ?? "") + (parts.length > 1 ? parts[parts.length - 1][0] : "")).toUpperCase();
  }

  /** Matiz estável por pessoa, para o avatar. */
  export function hue(name: string): number {
    let h = 0;
    for (const c of name) h = (h * 31 + c.charCodeAt(0)) % 360;
    return h;
  }
</script>

<script lang="ts">
  import { tick } from "svelte";
  import { jira } from "../../state/jira.svelte";
  import { activeAccount } from "../../state/accounts.svelte";

  interface Props {
    open: boolean;
    /** Chave já escolhida, para marcar na lista. */
    selected: string | null;
    onpick: (task: PickedTask) => void;
    /** Seguir sem task. */
    onclear: () => void;
    onclose: () => void;
    /** Leva à tela de Conectores (Atlassian não vinculado). */
    onconnectors: () => void;
  }
  let { open: isOpen, selected, onpick, onclear, onclose, onconnectors }: Props = $props();

  type Filter = "mine" | "doing" | "todo" | "done" | "all";
  const FILTERS: { id: Filter; label: string }[] = [
    { id: "mine", label: "Minhas" },
    { id: "doing", label: "Em andamento" },
    { id: "todo", label: "A fazer" },
    { id: "done", label: "Concluídas" },
    { id: "all", label: "Todas" },
  ];
  const GROUPS: { id: JiraIssue["category"]; label: string }[] = [
    { id: "doing", label: "Em andamento" },
    { id: "todo", label: "A fazer" },
    { id: "done", label: "Concluídas" },
  ];

  let dialog = $state<HTMLDialogElement>();
  let input = $state<HTMLInputElement>();
  let listEl = $state<HTMLDivElement>();
  let query = $state("");
  let filter = $state<Filter>("all");
  let project = $state("");
  let active = $state(0);
  let now = $state(Date.now());

  $effect(() => {
    if (isOpen && !dialog?.open) {
      dialog?.showModal();
      query = "";
      project = "";
      jira.clearSearch();
      jira.useAccount(activeAccount()?.id ?? null);
      filter = jira.issues.some((i) => i.mine) ? "mine" : "all";
      now = Date.now();
      void jira.ensure(activeAccount()?.id ?? null);
      void tick().then(() => input?.focus());
    }
    if (!isOpen && dialog?.open) dialog.close();
  });

  function onBackdrop(event: MouseEvent) {
    if (event.target === dialog) onclose();
  }

  // "Minhas" vazio não serve de filtro: sem tasks do usuário, mostra todas.
  $effect(() => {
    if (filter === "mine" && jira.issues.length && !jira.issues.some((i) => i.mine)) filter = "all";
  });

  const inSearch = $derived(jira.searchQuery !== null);
  const source = $derived(inSearch ? jira.searchResults : jira.issues);
  const projects = $derived([...new Set(jira.issues.map((i) => i.project).filter((p): p is string => !!p))].sort());

  const needle = $derived(query.trim().toLowerCase());
  function matches(i: JiraIssue): boolean {
    if (project && i.project !== project) return false;
    if (!needle) return true;
    return [i.key, i.summary, i.assignee ?? "", i.project ?? "", i.status].some((f) => f.toLowerCase().includes(needle));
  }
  function passes(i: JiraIssue, f: Filter): boolean {
    if (f === "mine") return i.mine;
    if (f === "all") return true;
    return i.category === f;
  }
  // A busca em todo o Jira já filtrou pelo texto (inclusive descrição e comentários): o filtro local
  // por título, chave e projeto esconderia resultados certos.
  const searched = $derived(inSearch ? source : source.filter(matches));
  const counts = $derived(Object.fromEntries(FILTERS.map((f) => [f.id, searched.filter((i) => passes(i, f.id)).length])) as Record<Filter, number>);
  const visible = $derived(inSearch ? searched : searched.filter((i) => passes(i, filter)));
  const groups = $derived(GROUPS.map((g) => ({ ...g, items: visible.filter((i) => i.category === g.id) })).filter((g) => g.items.length));
  /** Ordem da navegação pelo teclado: a mesma dos grupos. */
  const ordered = $derived(groups.flatMap((g) => g.items));

  /** Chave digitada que não está na lista: dá para usá-la mesmo assim. */
  const typedKey = $derived(KEY_RE.test(query.trim()) && !source.some((i) => i.key === query.trim().toUpperCase()) ? query.trim().toUpperCase() : null);
  const rows = $derived(ordered.length + (typedKey ? 1 : 0));

  // Filtro novo: volta para o topo.
  $effect(() => {
    void needle;
    void filter;
    void project;
    void inSearch;
    active = 0;
  });

  // A linha ativa sempre à vista.
  $effect(() => {
    const index = active;
    void tick().then(() => listEl?.querySelector(`[data-row="${index}"]`)?.scrollIntoView({ block: "nearest" }));
  });

  function rowIssue(index: number): JiraIssue | null {
    const offset = typedKey ? 1 : 0;
    return ordered[index - offset] ?? null;
  }

  function choose(index: number) {
    if (typedKey && index === 0) {
      onpick({ key: typedKey, site: jira.site, issue: null });
      return;
    }
    const issue = rowIssue(index);
    if (issue) onpick({ key: issue.key, site: jira.site, issue });
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      active = Math.min(rows - 1, active + 1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      active = Math.max(0, active - 1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (rows) choose(active);
      else if (needle && !inSearch) void jira.search(query);
    }
  }

  function backToList() {
    jira.clearSearch();
    input?.focus();
  }

  const host = $derived(jira.site?.replace(/^https?:\/\//, "") ?? null);
  const updatedLabel = $derived(jira.fetchedAt ? ago(new Date(jira.fetchedAt).toISOString(), now) : "");
  const loadingFirst = $derived(inSearch ? jira.searching : jira.loading && !jira.issues.length);
  const failure = $derived(inSearch ? jira.searchError : jira.issues.length ? null : jira.error);
  const priorityLabel = ["Baixa", "Média", "Alta", "Crítica"];
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={dialog} aria-labelledby="jira-picker-title" onclose={onclose} onclick={onBackdrop}>
  <div class="picker">
    <header class="top">
      <div class="title">
        <h2 id="jira-picker-title">Escolher task do Jira</h2>
        <p class="muted small">
          {#if host}{host} · {/if}
          {#if jira.loading}Atualizando…{:else if updatedLabel}Atualizada {updatedLabel}{:else}Pelo conector do Atlassian{/if}
        </p>
      </div>
      <button type="button" class="btn icon" aria-label="Atualizar a lista" title="Atualizar a lista" disabled={jira.loading} onclick={() => { now = Date.now(); void jira.refresh(); }}>
        <svg class:spin={jira.loading} aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 11a8 8 0 1 0-2.3 5.7" /><path d="M20 4v7h-7" /></svg>
      </button>
      <button type="button" class="btn icon" aria-label="Fechar" title="Fechar" onclick={onclose}>
        <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
      </button>
      {#if jira.loading && jira.issues.length}<span class="bar" aria-hidden="true"></span>{/if}
    </header>

    <div class="tools">
      <div class="search">
        <svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" /></svg>
        <input
          bind:this={input}
          class="input"
          type="search"
          role="combobox"
          aria-expanded="true"
          aria-controls="jira-picker-list"
          aria-activedescendant={rows ? `jira-row-${active}` : undefined}
          aria-label="Filtrar tasks por chave, título, responsável ou projeto"
          placeholder="Filtrar por chave, título, responsável ou projeto"
          bind:value={query}
          onkeydown={onKey}
        />
      </div>
      {#if inSearch}
        <div class="searchbar">
          <span class="small">Resultados em todo o Jira para <b>“{jira.searchQuery}”</b></span>
          <button type="button" class="link" onclick={backToList}>Voltar à lista</button>
        </div>
      {:else}
        <div class="filters">
          <div class="chips" role="tablist" aria-label="Filtrar por situação">
            {#each FILTERS as f (f.id)}
              <button type="button" role="tab" class="chip" class:on={filter === f.id} aria-selected={filter === f.id} onclick={() => (filter = f.id)}>
                {#if f.id !== "mine" && f.id !== "all"}<span class="dot {f.id}" aria-hidden="true"></span>{/if}
                {f.label} <span class="n">{counts[f.id]}</span>
              </button>
            {/each}
          </div>
          {#if projects.length > 1}
            <select class="input proj" aria-label="Projeto do Jira" bind:value={project}>
              <option value="">Todos os projetos</option>
              {#each projects as p (p)}<option value={p}>{p}</option>{/each}
            </select>
          {/if}
        </div>
      {/if}
    </div>

    <div class="list" id="jira-picker-list" role="listbox" aria-label="Tasks do Jira" bind:this={listEl}>
      {#if typedKey}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="row typed" id="jira-row-0" data-row="0" role="option" tabindex="-1" aria-selected={active === 0} class:active={active === 0} onmousemove={() => (active = 0)} onclick={() => choose(0)}>
          <span class="type task" aria-hidden="true">#</span>
          <span class="main"><span class="key">{typedKey}</span><span class="summary">Usar a task digitada</span></span>
          <kbd>Enter</kbd>
        </div>
      {/if}

      {#if loadingFirst}
        <p class="state muted small">{inSearch ? "Procurando em todo o Jira…" : "Consultando o Jira pelo conector… leva alguns segundos."}</p>
        {#each { length: 6 } as _, i (i)}<div class="row skeleton" aria-hidden="true"></div>{/each}
      {:else if failure}
        <div class="state error">
          <p>{failure}</p>
          <div class="actions">
            <button type="button" class="btn small" onclick={() => (inSearch ? jira.search(jira.searchQuery ?? "") : jira.refresh())}>Tentar de novo</button>
            <button type="button" class="btn small" onclick={onconnectors}>Abrir Conectores</button>
          </div>
        </div>
      {:else}
        {#each groups as group (group.id)}
          <div class="group" role="presentation">
            <div class="ghead" role="presentation"><span class="dot {group.id}" aria-hidden="true"></span>{group.label}<span class="n">{group.items.length}</span></div>
            {#each group.items as issue (issue.key)}
              {@const index = ordered.indexOf(issue) + (typedKey ? 1 : 0)}
              {@const level = priorityLevel(issue.priority)}
              {@const tone = typeTone(issue.issue_type)}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                class="row"
                id={`jira-row-${index}`}
                data-row={index}
                role="option"
                tabindex="-1"
                aria-selected={active === index}
                class:active={active === index}
                class:chosen={selected === issue.key}
                onmousemove={() => (active = index)}
                onclick={() => choose(index)}
              >
                <span class="type {tone}" title={issue.issue_type ?? "Task"} aria-label={issue.issue_type ?? "Task"}>
                  {#if tone === "bug"}<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><circle cx="5" cy="5" r="3" fill="currentColor" /></svg>
                  {:else if tone === "story"}<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M2 1h6v8L5 6.5 2 9z" fill="currentColor" /></svg>
                  {:else if tone === "epic"}<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M6 0 1.5 5.5H5L4 10l4.5-5.5H5z" fill="currentColor" /></svg>
                  {:else}<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="m2 5 2 2 4-4.5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg>{/if}
                </span>
                <span class="main">
                  <span class="line1">
                    <span class="key">{issue.key}</span>
                    {#if selected === issue.key}<span class="tag chosen-tag">Escolhida</span>{/if}
                    {#if issue.project && projects.length > 1}<span class="muted small proj-name">{issue.project}</span>{/if}
                  </span>
                  <span class="summary" title={issue.summary}>{issue.summary}</span>
                </span>
                <span class="meta">
                  <span class="status {issue.category}">{issue.status}</span>
                  <span class="sub">
                    {#if level !== null}
                      <span class="prio p{level + 1}" title={`Prioridade: ${issue.priority}`} aria-label={`Prioridade ${priorityLabel[level + 1]}`}>
                        {#if level > 0}<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M5 2 1.5 6h7z" fill="currentColor" />{#if level > 1}<path d="M5 5.5 1.5 9.5h7z" fill="currentColor" />{/if}</svg>
                        {:else if level < 0}<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M5 8 1.5 4h7z" fill="currentColor" /></svg>
                        {:else}<svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M2 3.5h6M2 6.5h6" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" /></svg>{/if}
                      </span>
                    {/if}
                    {#if issue.updated}<span class="when">{ago(issue.updated, now)}</span>{/if}
                    {#if issue.assignee}
                      <span class="avatar" class:me={issue.mine} style={`--h:${hue(issue.assignee)}`} title={`Responsável: ${issue.assignee}`}>{initials(issue.assignee)}</span>
                    {:else}
                      <span class="avatar none" title="Sem responsável" aria-label="Sem responsável">–</span>
                    {/if}
                  </span>
                </span>
              </div>
            {/each}
          </div>
        {:else}
          {#if !typedKey}
            <div class="state muted">
              {#if inSearch}
                <p>Nada no Jira com “{jira.searchQuery}”.</p>
              {:else if source.length}
                <p>Nenhuma task com esse filtro.</p>
                <button type="button" class="btn small" onclick={() => { query = ""; filter = "all"; project = ""; input?.focus(); }}>Limpar filtros</button>
              {:else}
                <p>Nenhuma task em aberto ou sua nos últimos 90 dias.</p>
              {/if}
            </div>
          {/if}
        {/each}
      {/if}

      {#if needle && !inSearch}
        <button type="button" class="deep" onclick={() => jira.search(query)}>
          <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" /></svg>
          Buscar “{query.trim()}” em todo o Jira <span class="muted">(inclui as concluídas e as de outras pessoas)</span>
        </button>
      {/if}
    </div>

    <footer class="foot">
      <span class="hints muted small"><kbd>↑</kbd><kbd>↓</kbd> navegar <kbd>Enter</kbd> escolher <kbd>Esc</kbd> fechar</span>
      <button type="button" class="btn small" onclick={onclear}>Seguir sem task</button>
    </footer>
  </div>
</dialog>

<style>
  dialog { width: min(780px, calc(100vw - 32px)); height: min(680px, calc(100vh - 48px)); padding: 0; border: 1px solid var(--border-strong); border-radius: 18px; background: var(--surface); color: var(--text); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6); overflow: hidden; }
  dialog::backdrop { background: rgba(0, 0, 0, 0.55); }
  .picker { display: flex; flex-direction: column; height: 100%; min-height: 0; }

  .top { position: relative; display: flex; align-items: flex-start; gap: 8px; padding: 18px 20px 12px; }
  .title { flex: 1; min-width: 0; }
  .title h2 { font-family: var(--display); font-size: 19px; font-weight: 600; letter-spacing: -0.01em; margin-bottom: 2px; }
  .bar { position: absolute; left: 0; right: 0; bottom: 0; height: 2px; background: linear-gradient(90deg, transparent, var(--accent), transparent); background-size: 50% 100%; background-repeat: no-repeat; animation: slide 1.1s linear infinite; }
  @keyframes slide { from { background-position: -50% 0; } to { background-position: 150% 0; } }
  .spin { animation: rot 0.9s linear infinite; }
  @keyframes rot { to { transform: rotate(360deg); } }

  .tools { display: flex; flex-direction: column; gap: 10px; padding: 0 20px 12px; border-bottom: 1px solid var(--border); }
  .search { position: relative; display: flex; align-items: center; }
  .search svg { position: absolute; left: 11px; color: var(--muted); pointer-events: none; }
  .search .input { width: 100%; padding-left: 34px; height: 38px; font-size: 14px; }
  .filters { display: flex; align-items: center; gap: 10px; }
  .chips { flex: 1; min-width: 0; display: flex; gap: 6px; overflow-x: auto; scrollbar-width: none; }
  .chip { flex-shrink: 0; display: inline-flex; align-items: center; gap: 6px; min-height: 28px; padding: 0 10px; border: 1px solid var(--border-strong); border-radius: 999px; background: transparent; color: var(--text-2); font-size: 12.5px; }
  .chip:hover { background: var(--hover); }
  .chip.on { background: var(--hover); border-color: var(--accent); color: var(--text); }
  .chip .n, .ghead .n { color: var(--muted); font-variant-numeric: tabular-nums; }
  .proj { flex: none; width: auto; max-width: 200px; height: 28px; font-size: 12.5px; }
  .searchbar { display: flex; align-items: center; justify-content: space-between; gap: 10px; min-height: 28px; }
  .link { border: 0; background: none; padding: 0; color: var(--accent-text); font-size: 12.5px; text-decoration: underline; text-underline-offset: 2px; }

  .dot { width: 8px; height: 8px; border-radius: 50%; flex: none; }
  .dot.todo { background: var(--muted); }
  .dot.doing { background: var(--bar); }
  .dot.done { background: var(--ok); }

  .list { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 10px 10px; }
  .group { display: flex; flex-direction: column; }
  .ghead { position: sticky; top: 0; z-index: 1; display: flex; align-items: center; gap: 8px; padding: 10px 10px 6px; background: var(--surface); font-size: 11.5px; font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; color: var(--text-2); }

  .row { display: flex; align-items: center; gap: 12px; padding: 9px 10px; border-radius: 10px; cursor: pointer; border: 1px solid transparent; }
  .row.active { background: var(--hover); border-color: var(--border-strong); }
  .row.chosen { box-shadow: inset 3px 0 0 var(--accent); }
  .row.typed { margin: 6px 0 4px; border-style: dashed; border-color: var(--border-strong); }
  .row.skeleton { height: 54px; margin-bottom: 4px; cursor: default; background: linear-gradient(90deg, var(--surface), var(--surface-2), var(--surface)); background-size: 200% 100%; animation: shimmer 1.4s infinite; }
  @keyframes shimmer { from { background-position: 200% 0; } to { background-position: -200% 0; } }

  .type { flex: none; width: 20px; height: 20px; display: grid; place-items: center; border-radius: 5px; color: #fff; font-size: 11px; font-weight: 700; }
  .type.task { background: #4b8fe0; }
  .type.bug { background: #d9584f; }
  .type.story { background: #5aa86b; }
  .type.epic { background: #8e6ad6; }
  .type.sub { background: #4aa3b8; }

  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .line1 { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .key { flex: none; font-family: var(--mono); font-size: 11.5px; color: var(--accent-text); padding: 0 6px; border-radius: 5px; background: rgba(91, 106, 217, 0.14); border: 1px solid rgba(91, 106, 217, 0.3); }
  .proj-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .summary { font-size: 13.5px; color: var(--text); line-height: 1.35; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .typed .main { flex-direction: row; align-items: center; gap: 10px; }
  .tag { padding: 0 7px; border-radius: 999px; font-size: 10.5px; font-weight: 600; }
  .chosen-tag { background: rgba(91, 106, 217, 0.18); color: var(--accent-text); }

  .meta { flex: none; display: flex; flex-direction: column; align-items: flex-end; gap: 4px; }
  .status { max-width: 150px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 1px 8px; border-radius: 999px; font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.02em; }
  .status.todo { background: var(--surface-2); color: var(--text-2); border: 1px solid var(--border-strong); }
  .status.doing { background: rgba(91, 106, 217, 0.18); color: var(--accent-text); }
  .status.done { background: var(--ok-bg); color: var(--ok-text); }
  .sub { display: flex; align-items: center; gap: 8px; }
  .when { font-size: 11.5px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .prio { display: grid; place-items: center; }
  .prio.p0 { color: #6b9be6; }
  .prio.p1 { color: var(--warn); }
  .prio.p2 { color: #e5893f; }
  .prio.p3 { color: #e0574c; }
  .avatar { width: 20px; height: 20px; flex: none; display: grid; place-items: center; border-radius: 50%; font-size: 9.5px; font-weight: 700; color: #fff; background: hsl(var(--h) 45% 40%); }
  .avatar.me { box-shadow: 0 0 0 2px var(--surface), 0 0 0 3px var(--accent); }
  .avatar.none { background: var(--surface-2); color: var(--muted); border: 1px dashed var(--border-strong); }

  .state { display: flex; flex-direction: column; align-items: center; gap: 10px; padding: 36px 16px; text-align: center; }
  p.state { padding: 14px 10px 8px; align-items: flex-start; text-align: left; }
  .state.error { color: var(--danger-text); }
  .actions { display: flex; gap: 8px; }

  .deep { display: flex; align-items: center; gap: 8px; width: 100%; margin-top: 8px; padding: 10px 12px; border: 1px dashed var(--border-strong); border-radius: 10px; background: none; color: var(--text-2); font-size: 13px; text-align: left; }
  .deep:hover { background: var(--hover); color: var(--text); }

  .foot { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 10px 20px; border-top: 1px solid var(--border); background: var(--bg-side); }
  .hints { display: flex; align-items: center; gap: 4px; }
  kbd { display: inline-grid; place-items: center; min-width: 20px; height: 20px; padding: 0 5px; border: 1px solid var(--border-strong); border-bottom-width: 2px; border-radius: 5px; font-family: var(--mono); font-size: 10.5px; color: var(--text-2); background: var(--surface); }
  .hints kbd:not(:first-child) { margin-left: 6px; }
  .hints kbd:nth-child(2) { margin-left: 2px; }
</style>
