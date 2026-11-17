<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage } from "../../api";
  import type { AccountConnector, ConnectorBridge, ConnectorsElsewhere } from "../../types";
  import ConnectorIcon from "../ConnectorIcon.svelte";
  import Toggle from "../Toggle.svelte";

  interface Props {
    /** Conectores que o Claude Code carrega para a conta (os da conta claude.ai viram ponte). */
    account: AccountConnector[];
    /** Conectores do Lume ligados: chegam sozinhos aos outros serviços. */
    lumeOn: number;
  }
  let { account, lumeOn }: Props = $props();

  let info = $state<ConnectorsElsewhere | null>(null);
  let loading = $state(false);
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  /** Ponte com as instruções do Gemini abertas. */
  let geminiHelp = $state<string | null>(null);

  /** Só os da conta claude.ai com endereço público podem ser usados fora do Claude. */
  const candidates = $derived(account.filter((c) => c.source === "account" && c.endpoint.startsWith("https://")));
  const bridgeOf = (name: string): ConnectorBridge | undefined => info?.bridges.find((b) => b.name === name);
  /** Pontes ligadas cujo conector não aparece mais na conta (desvinculado no claude.ai). */
  const orphans = $derived((info?.bridges ?? []).filter((b) => !candidates.some((c) => c.name === b.name)));

  async function load() {
    loading = true;
    try {
      info = await api.connectorsElsewhere();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  async function toggle(c: { name: string; display_name: string; endpoint: string }, on: boolean) {
    if (busy) return;
    busy = c.name;
    error = notice = null;
    try {
      await api.setConnectorBridge(c, on);
      await load();
      if (on) notice = `${c.display_name} ligado no ChatGPT e no Gemini. Falta entrar nele em cada serviço.`;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = null;
    }
  }

  async function loginChatgpt(b: ConnectorBridge) {
    busy = b.name;
    error = null;
    notice = `Conclua o login de ${b.display_name} no navegador (uma vez para cada conta do ChatGPT).`;
    try {
      await api.loginConnectorChatgpt(b.name);
      notice = null;
      await load();
    } catch (e) {
      notice = null;
      error = errorMessage(e);
    } finally {
      busy = null;
    }
  }

  onMount(() => void load());
</script>

<section class="card elsewhere" aria-labelledby="elsewhere-title">
  <header class="top">
    <div>
      <h2 id="elsewhere-title" class="section-title">No ChatGPT e no Gemini</h2>
      <p class="muted small">
        Os conectores do Lume{lumeOn ? ` (${lumeOn} ligado${lumeOn > 1 ? "s" : ""})` : ""} valem também nas contas do ChatGPT e do Gemini.
        Os da conta claude.ai não saem dela: ligue abaixo para usar o mesmo serviço lá, com login próprio em cada um.
      </p>
    </div>
    <button type="button" class="btn icon" aria-label="Verificar de novo" title="Verificar de novo" disabled={loading} onclick={load}>
      <svg class:spin={loading} aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 12a8 8 0 1 1-2.5-5.8M20 4v5h-5" /></svg>
    </button>
  </header>

  <div class="services">
    <span class="svc" class:on={info?.chatgpt_installed}>
      <span class="dot" aria-hidden="true"></span>
      <b>ChatGPT</b>
      {#if !info}<span class="muted">…</span>
      {:else if !info.chatgpt_installed}<span class="muted">Codex não instalado</span>
      {:else if !info.chatgpt_accounts}<span class="muted">sem conta do ChatGPT</span>
      {:else}<span class="muted">pelo Codex, a cada mensagem · {info.chatgpt_accounts} conta{info.chatgpt_accounts > 1 ? "s" : ""}</span>{/if}
    </span>
    <span class="svc" class:on={info?.gemini_installed && !info?.gemini_error} class:bad={!!info?.gemini_error}>
      <span class="dot" aria-hidden="true"></span>
      <b>Gemini</b>
      {#if !info}<span class="muted">…</span>
      {:else if !info.gemini_installed}<span class="muted">Antigravity não instalado</span>
      {:else if info.gemini_error}<span>arquivo de configuração com problema</span>
      {:else}<span class="muted">pelo arquivo do Antigravity (também vale no agy fora do app)</span>{/if}
    </span>
  </div>

  {#if info?.gemini_error}<p class="warn small">{info.gemini_error}</p>{/if}
  {#if error}<p class="error-text small" role="alert">{error}</p>{/if}
  {#if notice}<p class="muted small" role="status">{notice}</p>{/if}

  {#if candidates.length || orphans.length}
    <ul class="list">
      {#each [...candidates.map((c) => ({ name: c.name, display_name: c.display_name, endpoint: c.endpoint })), ...orphans] as c (c.name)}
        {@const b = bridgeOf(c.name)}
        <li class="item">
          <div class="row1">
            <ConnectorIcon endpoint={c.endpoint} name={c.display_name} />
            <div class="text">
              <strong>{c.display_name}</strong>
              <span class="muted small mono" title={c.endpoint}>{c.endpoint}</span>
            </div>
            <Toggle checked={!!b} label={`Usar ${c.display_name} também no ChatGPT e no Gemini`} onchange={(v) => toggle(c, v)} />
          </div>
          {#if b}
            <div class="logins">
              <span class="login">
                <span class="svc-name">ChatGPT</span>
                {#if b.chatgpt === true}
                  <span class="ok">✓ entrou</span>
                {:else if info?.chatgpt_installed && info.chatgpt_accounts}
                  <button type="button" class="btn small" disabled={busy !== null} onclick={() => loginChatgpt(b)}>{busy === b.name ? "Aguardando…" : "Entrar"}</button>
                {:else}
                  <span class="muted">{info?.chatgpt_installed ? "adicione uma conta do ChatGPT" : "instale o Codex"}</span>
                {/if}
              </span>
              <span class="login">
                <span class="svc-name">Gemini</span>
                {#if b.gemini === true}
                  <span class="ok">✓ entrou</span>
                {:else if info?.gemini_installed}
                  <button type="button" class="btn small" aria-expanded={geminiHelp === b.name} onclick={() => (geminiHelp = geminiHelp === b.name ? null : b.name)}>Como entrar</button>
                {:else}
                  <span class="muted">instale o Antigravity</span>
                {/if}
              </span>
            </div>
            {#if geminiHelp === b.name}
              <ol class="help small">
                <li>No terminal, rode <code>agy</code>.</li>
                <li>Digite <code>/mcp</code> e escolha <code>{b.server}</code>.</li>
                <li>Clique em Authenticate, conclua o login no navegador e cole o código, se ele pedir.</li>
                <li>Volte aqui e clique em Verificar de novo.</li>
              </ol>
            {/if}
            {#if b.display_name.toLowerCase() === "atlassian"}
              <p class="muted small note">
                Fora do Claude, os bots leem o Jira sem pedir aprovação. Só o Relator da esteira escreve (comenta e muda o status), e só no ChatGPT:
                o Gemini não deixa liberar escrita para um único papel.
              </p>
            {/if}
          {/if}
        </li>
      {/each}
    </ul>
  {:else}
    <p class="muted small empty">Nenhum conector da conta claude.ai com endereço público. Vincule um em Descobrir para poder usá-lo aqui.</p>
  {/if}
</section>

<style>
  .elsewhere { padding: 18px 20px; display: flex; flex-direction: column; gap: 12px; }
  .top { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  .top .section-title { margin-bottom: 4px; }
  .services { display: flex; flex-wrap: wrap; gap: 8px; }
  .svc { display: inline-flex; align-items: center; gap: 7px; padding: 5px 11px; border: 1px solid var(--border-strong); border-radius: 999px; font-size: 12.5px; }
  .svc .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--off); }
  .svc.on .dot { background: var(--ok); }
  .svc.bad { border-color: rgba(217, 164, 65, 0.45); color: var(--warn-text); }
  .svc.bad .dot { background: var(--warn); }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; border-top: 1px solid var(--border); }
  .item { display: flex; flex-direction: column; gap: 8px; padding: 12px 0; border-bottom: 1px solid var(--hairline); }
  .item:last-child { border-bottom: 0; padding-bottom: 0; }
  .row1 { display: flex; align-items: center; gap: 12px; }
  .text { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .text strong { font-weight: 500; }
  .mono { font-family: var(--mono); font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .logins { display: flex; flex-wrap: wrap; gap: 8px; padding-left: 44px; }
  .login { display: inline-flex; align-items: center; gap: 8px; padding: 4px 6px 4px 11px; border: 1px solid var(--border); border-radius: 9px; background: var(--surface-2); font-size: 12.5px; min-height: 34px; }
  .svc-name { font-weight: 600; color: var(--text-2); }
  .ok { color: var(--ok-text); padding-right: 6px; }
  .help { margin: 0 0 0 44px; padding: 10px 12px 10px 30px; border: 1px solid var(--border); border-radius: 9px; background: var(--bg-side); display: flex; flex-direction: column; gap: 4px; }
  .help code, .item code { font-family: var(--mono); font-size: 11.5px; }
  .note { padding-left: 44px; }
  .warn { color: var(--warn-text); }
  .empty { padding-top: 4px; }
  .spin { animation: rot 0.9s linear infinite; }
  @keyframes rot { to { transform: rotate(360deg); } }
</style>
