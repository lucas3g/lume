<script lang="ts">
  import { onMount } from "svelte";
  import { flip } from "svelte/animate";
  import { api, errorMessage } from "../api";
  import LimitsDialog from "../components/LimitsDialog.svelte";
  import LoginCodeField from "../components/LoginCodeField.svelte";
  import { logins } from "../state/logins.svelte";
  import SettingRow from "../components/SettingRow.svelte";
  import Toggle from "../components/Toggle.svelte";
  import { formatTime, formatWhen, nowSeconds, percent } from "../format";
  import { accounts, loadAccounts, moveAccount, reorderAccounts, stateOf, type AccountState } from "../state/accounts.svelte";
  import { PROVIDER_CLI, PROVIDER_LABEL, type Account, type Provider, type ProviderInfo } from "../types";

  const LOGIN_WARNING_DAYS = 10;

  let newName = $state("");
  let refreshing = $state(false);
  /** Conta com o diálogo de limites aberto. */
  let limitsOf = $state<Account | null>(null);

  /** Conta sendo arrastada pela alça e a ordem que ela formaria se fosse solta agora. */
  let dragging = $state<string | null>(null);
  let dragOrder = $state<string[] | null>(null);
  const rowEls: Record<string, HTMLElement> = {};
  const shown = $derived(
    dragOrder ? dragOrder.map((id) => accounts.list.find((a) => a.id === id)).filter((a): a is Account => !!a) : accounts.list,
  );

  function startDrag(event: PointerEvent, id: string) {
    if (busy || event.button !== 0) return;
    event.preventDefault();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    dragging = id;
    dragOrder = accounts.list.map((a) => a.id);
  }

  /** A conta arrastada entra depois de todas as linhas cujo meio já ficou acima do ponteiro. */
  function moveDrag(event: PointerEvent) {
    if (!dragging || !dragOrder) return;
    const others = dragOrder.filter((id) => id !== dragging);
    const index = others.filter((id) => {
      const rect = rowEls[id]?.getBoundingClientRect();
      return rect && rect.top + rect.height / 2 < event.clientY;
    }).length;
    const next = [...others.slice(0, index), dragging, ...others.slice(index)];
    if (next.join() !== dragOrder.join()) dragOrder = next;
  }

  function endDrag() {
    const order = dragOrder;
    dragging = null;
    dragOrder = null;
    if (order && order.join() !== accounts.list.map((a) => a.id).join()) void run(() => reorderAccounts(order));
  }

  /** Pelo teclado, as setas sobem e descem a conta com a alça em foco. */
  function dragKey(event: KeyboardEvent, id: string) {
    const direction = event.key === "ArrowUp" ? -1 : event.key === "ArrowDown" ? 1 : 0;
    if (!direction) return;
    event.preventDefault();
    void run(() => moveAccount(id, direction));
  }

  /** Consulta o uso atual de cada conta no provedor (um turno mínimo por conta). */
  async function refreshUsage() {
    refreshing = true;
    try {
      await api.refreshUsage();
      await loadAccounts();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      refreshing = false;
    }
  }

  /** Tipos de conta e se o CLI de cada um está instalado. */
  let providers = $state<ProviderInfo[]>([]);
  let newProvider = $state<Provider>("claude");
  const chosen = $derived(providers.find((p) => p.id === newProvider));

  /** Como a conta do tipo escolhido entra: cada CLI tem o seu jeito de fazer login. */
  const ADD_HINT: Record<Provider, string> = {
    claude: "O login oficial do Claude abre no navegador. Entre com a conta que deseja adicionar e volte para cá.",
    chatgpt: "Se o Codex já tem um login neste computador, o Lume usa esse login. Senão, o login do ChatGPT abre no navegador.",
    gemini: "Usa o login que o Antigravity (agy) já tem neste computador. O Antigravity guarda um único login por computador.",
  };

  /** O Antigravity não informa o uso da conta fora do terminal. */
  function reportsUsage(a: Account): boolean {
    return a.provider !== "gemini";
  }

  onMount(() => {
    void refreshUsage();
    void api.listProviders().then((list) => (providers = list));
  });
  let adding = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const STATE_LABEL: Record<AccountState, string> = {
    login: "Login necessário",
    disabled: "Desativada",
    limited: "Limite atingido",
    active: "Em uso",
    available: "Disponível",
  };
  const STATE_CLASS: Record<AccountState, string> = {
    login: "warn",
    disabled: "",
    limited: "warn",
    active: "ok",
    available: "",
  };

  /** Dias até o login vencer; só vale a pena avisar nos últimos dias. */
  function loginDaysLeft(a: Account): number | null {
    if (a.login_expires_at === null) return null;
    return Math.floor((a.login_expires_at - nowSeconds()) / 86_400);
  }

  function loginWarning(a: Account): string | null {
    const days = loginDaysLeft(a);
    if (days === null || days > LOGIN_WARNING_DAYS) return null;
    if (days < 0) return "O login venceu: entre novamente";
    if (days === 0) return "O login vence hoje: entre novamente";
    return `O login vence em ${days} ${days === 1 ? "dia" : "dias"}`;
  }

  function detail(a: Account): string {
    const state = stateOf(a);
    if (state === "limited" && a.limited_until) return `Volta às ${formatTime(a.limited_until)}`;
    if (state === "login") return "Entre novamente para usar";
    if (state === "active") return "Atende a próxima mensagem";
    if (state === "available") return "Na fila";
    return "Fora da fila";
  }

  /** Executa uma ação de conta mostrando o erro, se houver. */
  /** Conta com o nome em edição e o texto digitado. */
  let renaming = $state<string | null>(null);
  let renameTo = $state("");

  function startRename(a: Account) {
    renaming = a.id;
    renameTo = a.name;
  }

  async function saveRename(event: SubmitEvent) {
    event.preventDefault();
    const id = renaming;
    if (!id) return;
    await run(async () => {
      await api.renameAccount(id, renameTo);
      renaming = null;
    });
  }

  async function run(action: () => Promise<void>) {
    error = null;
    busy = true;
    try {
      await action();
      await loadAccounts();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  /** Executa um login; o do Claude mostra o campo do código de autenticação enquanto espera. */
  function login(provider: Provider, action: () => Promise<void>) {
    return logins.accountLogin(provider === "claude", () => run(action));
  }

  async function add(event: SubmitEvent) {
    event.preventDefault();
    await login(newProvider, async () => {
      await api.addAccount(newName, newProvider);
      newName = "";
      adding = false;
    });
  }
</script>

<div class="page">
  <div class="page-inner wide">
    <header class="page-head">
      <div>
        <h1>Contas</h1>
        <p class="muted">Quando o limite de uma conta acaba, a conversa continua na próxima da lista, sem perder o contexto.</p>
      </div>
      <div class="row">
        <button type="button" class="btn" title="Consulta no provedor o uso atual de cada conta" disabled={refreshing} onclick={refreshUsage}>{refreshing ? "Consultando…" : "Atualizar uso"}</button>
        <button type="button" class="btn primary" title="Entra com outra assinatura; o login abre no navegador" disabled={busy} onclick={() => (adding = !adding)}>Adicionar conta</button>
      </div>
    </header>

    {#if adding}
      <form class="card add" onsubmit={add}>
        <fieldset class="providers" disabled={busy}>
          <legend>Tipo de conta</legend>
          {#each providers as p (p.id)}
            <label class="provider" class:on={newProvider === p.id} class:off={!p.installed}>
              <input type="radio" name="provider" value={p.id} bind:group={newProvider} />
              <strong>{p.label}</strong>
              <span class="muted small">{p.installed ? `pelo ${p.cli}` : `${p.cli} não instalado`}</span>
            </label>
          {/each}
        </fieldset>
        <label for="account-name">Nome da conta</label>
        <div class="row">
          <input id="account-name" class="input" placeholder="Ex.: Trabalho" bind:value={newName} disabled={busy} />
          <button type="submit" class="btn primary" disabled={busy || !newName.trim() || !chosen?.installed}>
            {busy ? "Aguardando o login…" : `Entrar com o ${PROVIDER_LABEL[newProvider]}`}
          </button>
        </div>
        {#if chosen && !chosen.installed}
          <p class="warn small">Instale o {chosen.cli} neste computador para adicionar uma conta do {chosen.label}.</p>
        {:else}
          <p class="muted small">{ADD_HINT[newProvider]}</p>
        {/if}
      </form>
    {/if}

    {#if logins.account}<LoginCodeField scope="account" />{/if}
    {#if error}<p class="error-text" role="alert">{error}</p>{/if}

    <section class="card" aria-label="Contas cadastradas">
      {#each shown as a, i (a.id)}
        {@const state = stateOf(a)}
        <div class="account" class:dragging={dragging === a.id} bind:this={rowEls[a.id]} animate:flip={{ duration: 150 }}>
          <button
            type="button"
            class="order"
            aria-label={`Prioridade ${i + 1} de ${a.name}: segure e arraste para mudar, ou use as setas`}
            title="Segure e arraste para mudar a prioridade"
            disabled={busy}
            onpointerdown={(e) => startDrag(e, a.id)}
            onpointermove={moveDrag}
            onpointerup={endDrag}
            onpointercancel={endDrag}
            onkeydown={(e) => dragKey(e, a.id)}
          >
            <svg aria-hidden="true" width="12" height="16" viewBox="0 0 12 16" fill="currentColor"><circle cx="3" cy="3" r="1.4" /><circle cx="9" cy="3" r="1.4" /><circle cx="3" cy="8" r="1.4" /><circle cx="9" cy="8" r="1.4" /><circle cx="3" cy="13" r="1.4" /><circle cx="9" cy="13" r="1.4" /></svg>
            <span>{i + 1}</span>
          </button>

          <div class="identity">
            {#if renaming === a.id}
              <form class="row name rename" onsubmit={saveRename}>
                <!-- svelte-ignore a11y_autofocus -->
                <input class="input" aria-label="Novo nome da conta" bind:value={renameTo} maxlength="40" autofocus onkeydown={(e) => e.key === "Escape" && (renaming = null)} />
                <button type="submit" class="btn small primary" disabled={!renameTo.trim() || busy}>Salvar</button>
                <button type="button" class="btn small" onclick={() => (renaming = null)}>Cancelar</button>
              </form>
            {:else}
              <div class="row name">
                <strong>{a.name}</strong>
                <span class="plan service" title={`Atendida pelo ${PROVIDER_CLI[a.provider]}`}>{PROVIDER_LABEL[a.provider]}</span>
                {#if a.plan}<span class="plan">{a.plan}</span>{/if}
                <button type="button" class="btn icon edit" aria-label={`Renomear a conta ${a.name}`} title="Renomear" disabled={busy} onclick={() => startRename(a)}>
                  <svg aria-hidden="true" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20h4l10.5-10.5a2.1 2.1 0 0 0-3-3L5 17z" /><path d="M13.5 6.5l3 3" /></svg>
                </button>
              </div>
            {/if}
            <div class="muted small">{a.email ?? "E-mail não informado"}</div>
            {#if loginWarning(a)}<div class="warn small">{loginWarning(a)}</div>{/if}
          </div>

          <div class="usage">
            {#if a.util_5h !== null}
              <div class="usage-label muted"><span>Uso na janela de 5 h{a.resets_5h ? ` · zera às ${formatTime(a.resets_5h)}` : ""}</span><span>{percent(a.util_5h)}%</span></div>
              <div class="bar" role="img" aria-label={`${percent(a.util_5h)}% usado na janela de 5 horas`}>
                <div class:full={state === "limited"} style:width={`${percent(a.util_5h)}%`}></div>
              </div>
              {#if a.util_7d !== null}<div class="muted week">Semana: {percent(a.util_7d)}%{a.resets_7d ? ` · zera ${formatWhen(a.resets_7d)}` : ""}</div>{/if}
            {:else if reportsUsage(a)}
              <div class="muted small">Uso aparece após a primeira mensagem</div>
            {:else}
              <div class="muted small">O Antigravity não informa o uso fora do terminal</div>
            {/if}
            {#if a.auth_ok && reportsUsage(a)}
              <button type="button" class="btn small limits" title="Mostra o limite da sessão, o da semana e o de cada modelo" onclick={() => (limitsOf = a)}>Ver limites</button>
            {/if}
          </div>

          <div class="state">
            <span class={`pill ${STATE_CLASS[state]}`}>{STATE_LABEL[state]}</span>
            <span class="muted">{detail(a)}</span>
            {#if (state === "login" || loginWarning(a)) && a.config_dir}
              <button type="button" class="btn small enter" disabled={busy} onclick={() => login(a.provider, () => api.loginAccountAgain(a.id))}>Entrar</button>
            {:else if state === "login" && a.provider === "claude"}
              <button type="button" class="btn small enter" title="O login deste computador agora é de outra conta: entre com o login desta para ela ter um perfil próprio" disabled={busy} onclick={() => login(a.provider, () => api.loginAccountAgain(a.id))}>Entrar</button>
            {:else if state === "login" && a.provider !== "claude"}
              <button type="button" class="btn small enter" title={`Refaça o login pelo ${PROVIDER_CLI[a.provider]} no terminal e confira aqui`} disabled={busy} onclick={() => login(a.provider, () => api.loginAccountAgain(a.id))}>Conferir login</button>
            {/if}
          </div>

          <div class="row actions">
            <Toggle checked={a.enabled} label={`Usar a conta ${a.name}`} onchange={(v) => run(() => api.setAccountEnabled(a.id, v))} />
            {#if a.config_dir || a.provider !== "claude"}
              <button type="button" class="btn icon" aria-label={`Remover a conta ${a.name}`} title={`Remover a conta ${a.name}`} disabled={busy} onclick={() => run(() => api.removeAccount(a.id))}>
                <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
              </button>
            {:else}
              <span class="slot" aria-hidden="true"></span>
            {/if}
          </div>
        </div>
      {:else}
        <div class="empty muted">Nenhuma conta ainda. Adicione uma para começar.</div>
      {/each}
      <div class="foot muted small">A conta 1 é usada primeiro. Use as setas para mudar a ordem.</div>
    </section>

    <div class="columns">
      <section class="card panel">
        <h2 class="section-title">Troca automática</h2>
        <SettingRow flag="auto_switch" title="Trocar de conta ao atingir o limite" description="Passa para a próxima conta disponível e reenvia a última mensagem." />
        <SettingRow flag="switch_early" title="Poupar a conta quase no limite" description="Acima de 95% da janela de 5 h, usa outra conta livre antes de o limite estourar." />
        <SettingRow flag="notify_switch" title="Avisar na conversa" description="Mostra uma linha discreta indicando a conta que assumiu." />
      </section>

      <section class="card panel">
        <h2 class="section-title">Avisos</h2>
        <SettingRow flag="notify_desktop" title="Avisar pelo sistema" description="Com o Lume em segundo plano, avisa quando uma resposta termina ou o agente precisa de você." />
      </section>

      <section class="card panel">
        <h2 class="section-title">Trocas recentes</h2>
        {#each accounts.switches as s (s.at + s.from_name + s.to_name)}
          <div class="switch small">
            <span class="muted">{formatWhen(s.at)}</span>
            <span>{s.from_name} → {s.to_name}, em “{s.conversation_title}”</span>
          </div>
        {:else}
          <p class="muted small">Nenhuma troca até agora.</p>
        {/each}
      </section>
    </div>
  </div>
</div>

<LimitsDialog account={limitsOf} onclose={() => (limitsOf = null)} />

<style>
  .add { padding: 16px 20px; display: flex; flex-direction: column; gap: 8px; }
  .add label { font-weight: 500; }
  .add .input { flex: 1; }
  .providers { display: flex; flex-wrap: wrap; gap: 8px; margin: 0 0 6px; padding: 0; border: 0; }
  .providers legend { margin-bottom: 8px; padding: 0; font-weight: 500; }
  .provider { flex: 1 1 150px; display: flex; flex-direction: column; gap: 2px; padding: 10px 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--bg-side); cursor: pointer; }
  .provider:hover { border-color: var(--border-strong); }
  .provider.on { border-color: var(--accent-text); background: var(--hover); }
  .provider.off { opacity: 0.6; }
  .provider:focus-within { outline: 2px solid var(--accent-text); outline-offset: 2px; }
  .provider input { position: absolute; opacity: 0; pointer-events: none; }
  .plan.service { text-transform: none; }
  /* Colunas fixas: ordem, identidade, uso, situação e ações ficam alinhadas entre as linhas. */
  .wide { max-width: 1160px; }
  .account { display: grid; grid-template-columns: 28px minmax(0, 1.25fr) minmax(0, 1.35fr) 190px 108px; align-items: center; gap: 20px; padding: 14px 20px; border-bottom: 1px solid var(--hairline); }
  .order { display: flex; flex-direction: column; align-items: center; gap: 4px; padding: 6px 0; border: 0; border-radius: 6px; background: none; font: inherit; font-size: 13px; font-weight: 600; color: var(--muted); cursor: grab; touch-action: none; user-select: none; }
  .order:hover:not(:disabled) { background: var(--hover); color: var(--text-2); }
  .order:disabled { cursor: default; }
  .account.dragging { position: relative; z-index: 1; background: var(--hover); box-shadow: 0 6px 18px rgb(0 0 0 / 0.35); }
  .account.dragging .order { cursor: grabbing; color: var(--text-2); }
  .identity { min-width: 0; }
  .identity strong, .identity .small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .name { gap: 8px; }
  .name .edit { width: 24px; min-height: 24px; opacity: 0; transition: opacity 0.15s; }
  .account:hover .name .edit, .name .edit:focus-visible { opacity: 1; }
  .rename .input { flex: 1; min-width: 0; min-height: 30px; }
  .plan { padding: 1px 8px; border: 1px solid var(--border-strong); border-radius: 999px; font-size: 12px; color: var(--text-2); text-transform: capitalize; }
  .usage { min-width: 0; display: flex; flex-direction: column; gap: 6px; }
  .usage-label { display: flex; justify-content: space-between; font-size: 12px; }
  .bar { height: 6px; border-radius: 3px; background: var(--border); overflow: hidden; }
  .bar div { height: 100%; border-radius: 3px; background: var(--bar); }
  .bar div.full { background: var(--warn); }
  .week { font-size: 12px; }
  .limits { align-self: flex-start; }
  .warn { color: var(--warn); }
  .state { min-width: 0; display: flex; flex-direction: column; align-items: flex-start; gap: 2px; font-size: 12px; }
  .state .muted { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  .state .enter { margin-top: 4px; }
  .actions { justify-content: flex-end; gap: 8px; }
  .actions .slot { width: 28px; height: 28px; }
  @media (max-width: 900px) {
    .account { grid-template-columns: 28px minmax(0, 1fr) 108px; }
    .usage { grid-column: 2 / 3; }
    .state { grid-column: 2 / 3; flex-direction: row; align-items: center; gap: 8px; }
  }
  .empty, .foot { padding: 14px 20px; }
  .columns { display: flex; flex-wrap: wrap; gap: 24px; align-items: flex-start; }
  .panel { flex: 1 1 380px; padding: 20px; display: flex; flex-direction: column; gap: 16px; }
  .switch { display: flex; gap: 12px; }
  .switch .muted { width: 110px; flex-shrink: 0; }
</style>
