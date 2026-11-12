<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage } from "../api";
  import Toggle from "../components/Toggle.svelte";
  import type { Connector } from "../types";

  let connectors = $state<Connector[]>([]);
  let adding = $state(false);
  let name = $state("");
  let config = $state('{\n  "command": "npx",\n  "args": ["-y", "pacote-do-servidor"]\n}');
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);

  const STATUS_LABEL: Record<string, string> = {
    connected: "conectado",
    failed: "falha ao iniciar",
    pending: "iniciando",
    "needs-auth": "precisa de login",
  };

  function summary(c: Connector): string {
    const kind = isRemote(c) ? "Remoto" : "Local";
    if (!c.enabled) return `${kind} · desligado`;
    return `${kind} · ${c.status ? (STATUS_LABEL[c.status] ?? c.status) : "será iniciado na próxima mensagem"}`;
  }

  function isRemote(c: Connector): boolean {
    try {
      return typeof JSON.parse(c.config).url === "string";
    } catch {
      return false;
    }
  }

  function dotClass(c: Connector): string {
    if (!c.enabled || !c.status) return "";
    return c.status === "connected" ? "ok" : "warn";
  }

  /** Executa uma ação e recarrega a lista, mostrando o erro se houver. */
  async function run(action: () => Promise<void>) {
    error = notice = null;
    try {
      await action();
      connectors = await api.listConnectors();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  const importDesktop = () =>
    run(async () => {
      const count = await api.importDesktopConnectors();
      notice = count
        ? `${count} conector(es) importado(s), desligado(s). Ligue os que quiser usar.`
        : "Nenhum conector novo encontrado no Claude Desktop.";
    });

  async function add(event: SubmitEvent) {
    event.preventDefault();
    await run(async () => {
      await api.addConnector(name, config);
      name = "";
      adding = false;
    });
  }

  onMount(() => void run(async () => {}));
</script>

<div class="page">
  <div class="page-inner">
    <header class="page-head">
      <div>
        <h1>Conectores</h1>
        <p class="muted">Servidores MCP que dão ferramentas ao assistente. Valem para todas as contas.</p>
      </div>
      <button type="button" class="btn" onclick={importDesktop}>Importar do Claude Desktop</button>
      <button type="button" class="btn primary" onclick={() => (adding = !adding)}>Adicionar conector</button>
    </header>

    <p class="card warning small">
      As ferramentas de um conector ligado são executadas sem pedir confirmação. Ligue apenas os conectores em que você confia.
    </p>

    {#if error}<p class="error-text" role="alert">{error}</p>{/if}
    {#if notice}<p class="muted small" role="status">{notice}</p>{/if}

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

    <section class="card" aria-label="Conectores instalados">
      {#each connectors as c (c.id)}
        <div class="connector">
          <span class={`dot ${dotClass(c)}`} aria-hidden="true"></span>
          <div class="info">
            <strong>{c.name}</strong>
            <div class="muted small">{summary(c)}</div>
          </div>
          <Toggle checked={c.enabled} label={`Usar o conector ${c.name}`} onchange={(v) => run(() => api.setConnectorEnabled(c.id, v))} />
          <button type="button" class="btn icon" aria-label={`Remover o conector ${c.name}`} onclick={() => run(() => api.removeConnector(c.id))}>
            <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
          </button>
        </div>
      {:else}
        <div class="empty muted">Nenhum conector ainda. Importe os do Claude Desktop ou adicione um.</div>
      {/each}
    </section>
  </div>
</div>

<style>
  .warning { padding: 10px 16px; border-color: var(--border-strong); color: var(--warn-text); background: var(--warn-bg); }
  .form { padding: 20px; display: flex; flex-direction: column; gap: 12px; }
  .form label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; font-weight: 500; }
  .form .input { font-weight: 400; }
  .form textarea { padding: 10px 12px; resize: vertical; }
  code { font-family: var(--mono); font-size: 12px; }
  .connector { display: flex; align-items: center; gap: 12px; padding: 14px 16px; border-bottom: 1px solid var(--hairline); }
  .connector:last-child { border-bottom: 0; }
  .dot { width: 8px; height: 8px; flex-shrink: 0; border-radius: 50%; background: var(--off); }
  .dot.ok { background: var(--ok); }
  .dot.warn { background: var(--warn); }
  .info { flex: 1; min-width: 0; }
  .empty { padding: 14px 16px; }
</style>
