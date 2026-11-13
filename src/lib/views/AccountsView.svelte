<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage } from "../api";
  import SettingRow from "../components/SettingRow.svelte";
  import Toggle from "../components/Toggle.svelte";
  import { formatTime, formatWhen, nowSeconds, percent } from "../format";
  import { accounts, loadAccounts, moveAccount, stateOf, type AccountState } from "../state/accounts.svelte";
  import type { Account } from "../types";

  const LOGIN_WARNING_DAYS = 10;

  let newName = $state("");
  let refreshing = $state(false);

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

  onMount(() => void refreshUsage());
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

  async function add(event: SubmitEvent) {
    event.preventDefault();
    await run(async () => {
      await api.addAccount(newName);
      newName = "";
      adding = false;
    });
  }
</script>

<div class="page">
  <div class="page-inner">
    <header class="page-head">
      <div>
        <h1>Contas</h1>
        <p class="muted">Quando o limite de uma conta acaba, a conversa continua na próxima da lista, sem perder o contexto.</p>
      </div>
      <div class="row">
        <button type="button" class="btn" disabled={refreshing} onclick={refreshUsage}>{refreshing ? "Consultando…" : "Atualizar uso"}</button>
        <button type="button" class="btn primary" disabled={busy} onclick={() => (adding = !adding)}>Adicionar conta</button>
      </div>
    </header>

    {#if adding}
      <form class="card add" onsubmit={add}>
        <label for="account-name">Nome da conta</label>
        <div class="row">
          <input id="account-name" class="input" placeholder="Ex.: Trabalho" bind:value={newName} disabled={busy} />
          <button type="submit" class="btn primary" disabled={busy || !newName.trim()}>
            {busy ? "Aguardando o login…" : "Entrar com o Claude"}
          </button>
        </div>
        <p class="muted small">O login oficial do Claude abre no navegador. Entre com a conta que deseja adicionar e volte para cá.</p>
      </form>
    {/if}

    {#if error}<p class="error-text" role="alert">{error}</p>{/if}

    <section class="card" aria-label="Contas cadastradas">
      {#each accounts.list as a, i (a.id)}
        {@const state = stateOf(a)}
        <div class="account">
          <div class="order">
            <button type="button" class="btn icon" aria-label={`Subir ${a.name} na prioridade`} disabled={i === 0 || busy} onclick={() => run(() => moveAccount(a.id, -1))}>
              <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 15l6-6 6 6" /></svg>
            </button>
            <span>{i + 1}</span>
            <button type="button" class="btn icon" aria-label={`Descer ${a.name} na prioridade`} disabled={i === accounts.list.length - 1 || busy} onclick={() => run(() => moveAccount(a.id, 1))}>
              <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6" /></svg>
            </button>
          </div>

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
            {:else}
              <div class="muted small">Uso aparece após a primeira mensagem</div>
            {/if}
          </div>

          <div class="state">
            <span class={`pill ${STATE_CLASS[state]}`}>{STATE_LABEL[state]}</span>
            <span class="muted">{detail(a)}</span>
          </div>

          <div class="row actions">
            {#if (state === "login" || loginWarning(a)) && a.config_dir}
              <button type="button" class="btn small" disabled={busy} onclick={() => run(() => api.loginAccountAgain(a.id))}>Entrar</button>
            {/if}
            <Toggle checked={a.enabled} label={`Usar a conta ${a.name}`} onchange={(v) => run(() => api.setAccountEnabled(a.id, v))} />
            {#if a.config_dir}
              <button type="button" class="btn icon" aria-label={`Remover a conta ${a.name}`} disabled={busy} onclick={() => run(() => api.removeAccount(a.id))}>
                <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
              </button>
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

<style>
  .add { padding: 16px 20px; display: flex; flex-direction: column; gap: 8px; }
  .add label { font-weight: 500; }
  .add .input { flex: 1; }
  .account { display: flex; flex-wrap: wrap; align-items: center; gap: 16px; padding: 14px 20px; border-bottom: 1px solid var(--hairline); }
  .order { display: flex; flex-direction: column; align-items: center; font-size: 13px; font-weight: 600; color: var(--muted); }
  .order .btn { width: 24px; min-height: 20px; }
  .identity { flex: 1 1 180px; min-width: 0; }
  .name { gap: 8px; }
  .name .edit { width: 24px; min-height: 24px; opacity: 0; transition: opacity 0.15s; }
  .account:hover .name .edit, .name .edit:focus-visible { opacity: 1; }
  .rename .input { flex: 1; min-width: 0; min-height: 30px; }
  .plan { padding: 1px 8px; border: 1px solid var(--border-strong); border-radius: 999px; font-size: 12px; color: var(--text-2); text-transform: capitalize; }
  .usage { flex: 1 1 200px; display: flex; flex-direction: column; gap: 6px; }
  .usage-label { display: flex; justify-content: space-between; font-size: 12px; }
  .bar { height: 6px; border-radius: 3px; background: var(--border); overflow: hidden; }
  .bar div { height: 100%; border-radius: 3px; background: var(--bar); }
  .bar div.full { background: var(--warn); }
  .week { font-size: 12px; }
  .warn { color: var(--warn); }
  .state { flex: 0 1 170px; display: flex; flex-direction: column; align-items: flex-start; gap: 2px; font-size: 12px; }
  .actions { gap: 8px; }
  .empty, .foot { padding: 14px 20px; }
  .columns { display: flex; flex-wrap: wrap; gap: 24px; align-items: flex-start; }
  .panel { flex: 1 1 380px; padding: 20px; display: flex; flex-direction: column; gap: 16px; }
  .switch { display: flex; gap: 12px; }
  .switch .muted { width: 110px; flex-shrink: 0; }
</style>
