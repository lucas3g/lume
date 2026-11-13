<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage } from "../api";
  import ConnectorIcon from "../components/ConnectorIcon.svelte";
  import LoginCodeField from "../components/LoginCodeField.svelte";
  import { logins } from "../state/logins.svelte";
  import { activeAccount } from "../state/accounts.svelte";
  import { connectors, isLinked } from "../state/connectors.svelte";
  import ConnectorCatalog from "../components/store/ConnectorCatalog.svelte";
  import Toggle from "../components/Toggle.svelte";
  import type { AccountConnector, Connector } from "../types";

  let tab = $state<"mine" | "discover">("mine");
  /** Conectores que o Claude Code carrega para a conta ativa (web, terminal e plugins), guardados entre visitas. */
  const account = $derived(connectors.account);
  const checking = $derived(connectors.checking);
  /** Conectores cadastrados no próprio Lume. */
  let mine = $state<Connector[]>([]);
  let adding = $state(false);
  let name = $state("");
  let config = $state('{\n  "command": "npx",\n  "args": ["-y", "pacote-do-servidor"]\n}');
  let filter = $state("");
  /** Nome do conector com login ou remoção em andamento. */
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);

  /** Espera pelo vínculo de um conector da conta; cancelada pelo usuário ou ao sair da tela. */
  let linkWait = $state<AbortController | null>(null);
  /** Intervalo entre verificações: começa curto e cresce até o máximo (cada uma testa todos os servidores). */
  const LINK_POLL_FIRST_MS = 4000;
  const LINK_POLL_MAX_MS = 15_000;
  const LINK_TIMEOUT_MS = 5 * 60_000;

  const SOURCE_TAG = { account: "Conta", user: "Claude Code", plugin: "Plugin" } as const;
  const STATUS_LABEL: Record<string, string> = { connected: "conectado", failed: "falha ao iniciar", pending: "iniciando", "needs-auth": "precisa de login" };

  const query = $derived(filter.trim().toLowerCase());
  const accountRows = $derived(account.filter((c) => c.display_name.toLowerCase().includes(query)));
  const mineRows = $derived(mine.filter((c) => c.name.toLowerCase().includes(query)));
  /** Conector do Lume que repete um servidor já configurado no Claude Code: os dois chegariam ao assistente. */
  const duplicated = (c: Connector) => account.some((a) => a.source === "user" && a.name.toLowerCase() === c.name.toLowerCase());

  function isRemote(c: Connector): boolean {
    try {
      return typeof JSON.parse(c.config).url === "string";
    } catch {
      return false;
    }
  }

  /** Endereço do conector do Lume, para o ícone; comandos locais não têm. */
  function mineEndpoint(c: Connector): string {
    try {
      const url = JSON.parse(c.config).url;
      return typeof url === "string" ? url : "";
    } catch {
      return "";
    }
  }

  function mineStatus(c: Connector): string {
    if (!c.enabled) return "desligado";
    return c.status ? (STATUS_LABEL[c.status] ?? c.status) : "inicia na próxima mensagem";
  }

  async function loadMine() {
    mine = await api.listConnectors();
  }

  /** A situação dos conectores da conta vem do Claude Code e leva alguns segundos; a última lista fica na tela enquanto isso. */
  async function loadAccount() {
    await connectors.ensure(activeAccount()?.id ?? null);
    if (connectors.error) error = connectors.error;
  }

  /** Executa uma ação mostrando o erro, se houver, e recarrega os conectores do Lume. */
  async function run(action: () => Promise<void>) {
    error = notice = null;
    try {
      await action();
      await loadMine();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  /** Vincula ou desvincula um conector da conta; o login abre no navegador. */
  async function link(c: AccountConnector, connect: boolean) {
    busy = c.name;
    error = notice = null;
    if (connect) notice = `Conclua o login de ${c.display_name} no navegador.`;
    else if (c.source === "account") notice = `Desvincule ${c.display_name} na página de conectores do claude.ai, aberta no navegador.`;
    try {
      if (connect) {
        // O vínculo fica marcado fora da tela: o campo do código volta se o usuário sair e retornar.
        const started = await logins.connectorLogin(c.name, () => api.linkConnector(c.name));
        if (!started) {
          notice = null;
          error = "Outro conector ainda espera o login. Conclua-o antes de vincular este.";
          return;
        }
      } else {
        await api.unlinkConnector(c.name);
      }
      // Num conector da conta, vincular e desvincular só abrem a página: a mudança é feita
      // no claude.ai, depois. Verifica de novo até ela aparecer na lista.
      if (c.source === "account" && !(await waitAccountChange(c.name, connect))) {
        notice = null;
        return;
      }
      notice = null;
      await loadAccount();
    } catch (e) {
      notice = null;
      error = errorMessage(e);
    } finally {
      busy = null;
    }
  }

  /** Espera, com Cancelar à mão, o conector da conta ficar vinculado (ou desvinculado). */
  async function waitAccountChange(name: string, connect: boolean): Promise<boolean> {
    const wait = new AbortController();
    linkWait = wait;
    try {
      return await waitStatus(name, wait.signal, connect ? linkedStatus : unlinkedStatus);
    } finally {
      if (linkWait === wait) linkWait = null;
    }
  }

  /** Vinculado: o conector aparece na lista e não pede mais login. */
  const linkedStatus = (status: string | undefined) => status !== undefined && status !== "needs_auth";
  /** Desvinculado: pede login de novo ou saiu da lista. */
  const unlinkedStatus = (status: string | undefined) => status === undefined || status === "needs_auth";

  /** Para de esperar o vínculo (o usuário desistiu da página no navegador). */
  function cancelLinkWait() {
    linkWait?.abort();
    notice = null;
  }

  /** Pausa até o intervalo passar, a janela do app voltar ao foco (o usuário voltou do
   * navegador) ou a espera ser cancelada. */
  function pause(ms: number, signal: AbortSignal): Promise<void> {
    return new Promise((resolve) => {
      const done = () => {
        clearTimeout(timer);
        window.removeEventListener("focus", done);
        signal.removeEventListener("abort", done);
        resolve();
      };
      const timer = setTimeout(done, ms);
      window.addEventListener("focus", done);
      signal.addEventListener("abort", done);
    });
  }

  /** Espera a situação do conector da conta satisfazer `done`, verificando a lista de tempos em
   * tempos. Devolve `false` se a espera foi cancelada ou a conta ativa mudou. */
  async function waitStatus(name: string, signal: AbortSignal, done: (status: string | undefined) => boolean): Promise<boolean> {
    const accountId = activeAccount()?.id ?? null;
    const deadline = Date.now() + LINK_TIMEOUT_MS;
    let delay = LINK_POLL_FIRST_MS;
    for (;;) {
      await pause(delay, signal);
      if (signal.aborted || (activeAccount()?.id ?? null) !== accountId) return false;
      await connectors.ensure(accountId);
      if (signal.aborted) return false;
      // Uma verificação que falhou não decide nada.
      if (!connectors.error && done(connectors.account.find((a) => a.name === name)?.status)) return true;
      if (Date.now() > deadline) throw new Error("A mudança não apareceu em 5 minutos. Clique em Verificar de novo depois de concluí-la.");
      delay = Math.min(delay * 1.5, LINK_POLL_MAX_MS);
    }
  }

  async function toggleAccount(c: AccountConnector, enabled: boolean) {
    c.enabled = enabled;
    await api.setServerEnabled(c.name, enabled);
  }

  const importDesktop = () =>
    run(async () => {
      // Servidores que o Claude Code já carrega não entram de novo.
      const count = await api.importDesktopConnectors(account.filter((c) => c.source === "user").map((c) => c.name));
      notice = count ? `${count} conector(es) importado(s), desligado(s). Ligue os que quiser usar.` : "Nenhum conector novo encontrado no Claude Desktop.";
    });

  async function add(event: SubmitEvent) {
    event.preventDefault();
    await run(async () => {
      await api.addConnector(name, config);
      name = "";
      adding = false;
    });
  }

  onMount(() => {
    void loadMine();
    void loadAccount();
    return () => linkWait?.abort();
  });
</script>

<div class="page">
  <div class="page-inner">
    <header class="page-head">
      <div>
        <h1>Conectores</h1>
        <p class="muted">Serviços e ferramentas que o assistente pode usar. Os da conta acompanham a conta que atende a próxima mensagem.</p>
      </div>
      <div class="tabs" role="tablist" aria-label="Conectores">
        <button type="button" role="tab" aria-selected={tab === "mine"} class:on={tab === "mine"} title="Conectores da conta ativa, do Claude Code e do Lume" onclick={() => (tab = "mine")}>Meus</button>
        <button type="button" role="tab" aria-selected={tab === "discover"} class:on={tab === "discover"} title="Catálogo de serviços para vincular à conta" onclick={() => (tab = "discover")}>Descobrir</button>
      </div>
    </header>

    {#if error}<p class="error-text" role="alert">{error}</p>{/if}
    {#if notice}
      <p class="muted small notice" role="status">
        {notice}
        {#if linkWait}<button type="button" class="btn small" title="Parar de esperar a página no navegador" onclick={cancelLinkWait}>Cancelar</button>{/if}
      </p>
    {/if}
    {#if logins.connector}<LoginCodeField scope="connector" />{/if}

    {#if tab === "discover"}
      <ConnectorCatalog {account} loading={checking} {busy} onlink={(c, connect) => void link(c, connect)} oncancel={linkWait ? cancelLinkWait : undefined} onchange={() => void loadMine()} />
    {:else}
      <div class="toolbar">
        <input class="input grow" type="search" aria-label="Procurar conector" placeholder="Procurar" bind:value={filter} />
        <button type="button" class="btn" title="Traz os servidores MCP configurados no Claude Desktop; entram desligados" onclick={importDesktop}>Importar do Claude Desktop</button>
        <button type="button" class="btn primary" title="Cadastrar um servidor MCP pela configuração (comando local ou endereço)" onclick={() => (adding = !adding)}>Adicionar</button>
        <button type="button" class="btn icon" aria-label="Verificar de novo" title="Verificar de novo" disabled={checking} onclick={loadAccount}>
          <svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 12a8 8 0 1 1-2.5-5.8M20 4v5h-5" /></svg>
        </button>
      </div>

      {#if adding}
        <form class="card form" onsubmit={add}>
          <label>Nome<input class="input" bind:value={name} placeholder="Ex.: Arquivos locais" /></label>
          <label>Configuração (JSON)
            <textarea class="input mono" rows="6" spellcheck="false" bind:value={config}></textarea>
          </label>
          <p class="muted small">Servidor local: <code>command</code>, <code>args</code> e <code>env</code>. Servidor remoto: <code>type</code> e <code>url</code>.</p>
          <div class="row">
            <button type="submit" class="btn primary" disabled={!name.trim()}>Adicionar</button>
            <button type="button" class="btn" onclick={() => (adding = false)}>Cancelar</button>
          </div>
        </form>
      {/if}

      <section class="card" aria-label="Conectores">
        <div class="grid head">
          <span>Conector</span><span>Tipo</span><span class="center">Status</span><span class="sr-only">Ações</span>
        </div>

        {#each accountRows as c (c.name)}
          <div class="grid item" class:off={!c.enabled}>
            <span class="cell name">
              <ConnectorIcon endpoint={c.endpoint} name={c.display_name} />
              <span class="name-text">
                <strong>{c.display_name}</strong>
                {#if c.source === "plugin"}<span class="muted small">Fornecido por plugin</span>{/if}
              </span>
            </span>
            <span class="cell">
              {c.source === "account" || c.endpoint.startsWith("http") ? "Web" : "Desktop"}
              <span class="tag">{SOURCE_TAG[c.source]}</span>
            </span>
            <span class="cell status">
              {#if c.status === "connected"}
                <svg role="img" aria-label="Conectado" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="var(--ok)" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>
              {:else if c.status === "needs_auth"}
                <button type="button" class="btn small" disabled={busy !== null} onclick={() => link(c, true)}>{busy === c.name ? "Aguardando…" : "Vincular"}</button>
              {:else if c.status === "pending"}
                <span class="muted">iniciando</span>
              {:else}
                <span class="warn">falha</span>
              {/if}
            </span>
            <span class="cell actions">
              {#if isLinked(c) && c.source !== "user"}
                <button type="button" class="btn icon" aria-label={`Desvincular ${c.display_name}`} title="Desvincular" disabled={busy !== null} onclick={() => link(c, false)}>
                  <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1M4 4l16 16" /></svg>
                </button>
              {:else}
                <span class="slot" aria-hidden="true"></span>
              {/if}
              <Toggle checked={c.enabled} label={`Usar ${c.display_name} nas conversas`} onchange={(v) => toggleAccount(c, v)} />
            </span>
          </div>
        {/each}

        {#each mineRows as c (c.id)}
          <div class="grid item" class:off={!c.enabled}>
            <span class="cell name">
              <ConnectorIcon endpoint={mineEndpoint(c)} name={c.name} />
              <span class="name-text">
                <strong>{c.name}</strong>
                {#if duplicated(c)}<span class="warn small">Repetido: o Claude Code já carrega este servidor. Remova um dos dois.</span>{/if}
              </span>
            </span>
            <span class="cell">{isRemote(c) ? "Web" : "Desktop"}<span class="tag">Lume</span></span>
            <span class="cell status">
              {#if c.enabled && c.status === "connected"}
                <svg role="img" aria-label="Conectado" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="var(--ok)" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>
              {:else}
                <span class="muted">{mineStatus(c)}</span>
              {/if}
            </span>
            <span class="cell actions">
              <button type="button" class="btn icon" aria-label={`Remover o conector ${c.name}`} title="Remover" onclick={() => run(() => api.removeConnector(c.id))}>
                <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
              </button>
              <Toggle checked={c.enabled} label={`Usar o conector ${c.name}`} onchange={(v) => run(() => api.setConnectorEnabled(c.id, v))} />
            </span>
          </div>
        {/each}

        {#if checking && !connectors.loaded}
          {#each [1, 2, 3, 4] as i (i)}
            <div class="grid item skeleton" aria-hidden="true"><span class="bone icon-bone"></span><span class="bone"></span><span class="bone short"></span><span></span></div>
          {/each}
          <p class="muted empty" role="status">Verificando os conectores da conta…</p>
        {:else if !accountRows.length && !mineRows.length}
          <p class="muted empty">{query ? "Nenhum conector com esse nome." : "Nenhum conector ainda. Procure em Descobrir ou adicione um."}</p>
        {/if}
      </section>

      <p class="muted small">Vincular abre o login do serviço no navegador. As ferramentas dos conectores pedem sua aprovação na conversa, como as ações do agente.</p>
    {/if}
  </div>
</div>

<style>
  .notice { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .tabs { display: flex; padding: 3px; border-radius: 9px; background: var(--bg-side); border: 1px solid var(--border); }
  .tabs button { min-height: 30px; padding: 0 14px; border: 0; border-radius: 6px; background: transparent; font-size: 13px; color: var(--text-2); }
  .tabs button.on { background: var(--hover); color: #fff; font-weight: 500; }
  .toolbar { display: flex; gap: 8px; align-items: center; }
  .grow { flex: 1; min-width: 0; }
  .form { padding: 20px; display: flex; flex-direction: column; gap: 12px; }
  .form label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; font-weight: 500; }
  .form .input { font-weight: 400; }
  .form textarea { padding: 10px 12px; resize: vertical; }
  code { font-family: var(--mono); font-size: 12px; }

  .grid { display: grid; grid-template-columns: minmax(0, 1.6fr) minmax(0, 1fr) minmax(0, 0.9fr) 86px; align-items: center; gap: 12px; padding: 0 16px; }
  .center, .status { justify-content: center; text-align: center; }
  .slot { width: 28px; height: 28px; }
  .head { min-height: 36px; border-bottom: 1px solid var(--border); font-size: 12px; font-weight: 600; letter-spacing: 0.02em; color: var(--muted); }
  .item { min-height: 52px; border-bottom: 1px solid var(--hairline); }
  .item:last-of-type { border-bottom: 0; }
  .item.off .name, .item.off .cell:not(.actions) { opacity: 0.55; }
  .cell { display: flex; align-items: center; gap: 8px; min-width: 0; color: var(--text-2); }
  .name { gap: 12px; color: var(--text); }
  .name-text { display: flex; flex-direction: column; min-width: 0; }
  .name strong { font-weight: 500; }
  .tag { padding: 0 7px; border: 1px solid var(--border-strong); border-radius: 999px; font-size: 11px; color: var(--muted); white-space: nowrap; }
  .actions { justify-content: flex-end; gap: 6px; }
  .warn { color: var(--warn-text); }
  .empty { padding: 14px 16px; }
  .skeleton .bone { display: block; height: 12px; border-radius: 6px; background: linear-gradient(90deg, var(--surface-2), var(--hover), var(--surface-2)); background-size: 200% 100%; animation: shimmer 1.4s infinite; }
  .skeleton .icon-bone { width: 32px; height: 32px; border-radius: 9px; }
  .skeleton .short { width: 60%; }
  @keyframes shimmer { from { background-position: 200% 0; } to { background-position: -200% 0; } }
  .sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
  @media (max-width: 760px) { .grid { grid-template-columns: minmax(0, 1fr) auto; } .head span:nth-child(2), .head span:nth-child(3), .item .cell:nth-child(2), .item .status { display: none; } }
</style>
