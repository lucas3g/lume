<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage } from "../api";
  import SettingRow from "../components/SettingRow.svelte";
  import { folderName, formatWhen } from "../format";
  import { loadConversations, openConversation } from "../state/chat.svelte";
  import { DEFAULT_MODEL, type HistSession, type HistoryStatus, type View } from "../types";

  interface Props {
    onnavigate: (view: View) => void;
  }
  let { onnavigate }: Props = $props();

  let status = $state<HistoryStatus>({ sessions: 0, projects: 0, desktop_sessions: 0 });
  let sessions = $state<HistSession[]>([]);
  let query = $state("");
  let progress = $state<{ done: number; total: number } | null>(null);
  let error = $state<string | null>(null);

  const SOURCE_LABEL = { code: "Claude Code", desktop: "Claude Desktop" } as const;

  async function refresh() {
    [status, sessions] = await Promise.all([api.historyStatus(), api.historyList(query)]);
  }

  async function reindex() {
    error = null;
    progress = { done: 0, total: 0 };
    try {
      await api.historyReindex();
      await refresh();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      progress = null;
    }
  }

  async function open(session: HistSession) {
    error = null;
    try {
      const conversation = await api.historyOpen(session.id, DEFAULT_MODEL);
      await loadConversations();
      await openConversation(conversation.id);
      onnavigate("chat");
    } catch (e) {
      error = errorMessage(e);
    }
  }

  onMount(() => {
    void refresh();
    const unlisten = api.onHistoryProgress((p) => {
      if (progress) progress = p;
      if (p.done === p.total) void refresh();
    });
    return () => void unlisten.then((stop) => stop());
  });
</script>

<div class="page">
  <div class="page-inner">
    <header class="page-head">
      <div>
        <h1>Histórico</h1>
        <p class="muted">O Lume lê as conversas que você já teve neste computador e as usa como contexto nas novas. Os arquivos originais não são alterados.</p>
      </div>
      <button type="button" class="btn" disabled={progress !== null} onclick={reindex}>
        {progress ? `Verificando ${progress.done} de ${progress.total}…` : "Verificar novamente"}
      </button>
    </header>

    {#if error}<p class="error-text" role="alert">{error}</p>{/if}

    <section class="card" aria-label="Fontes de histórico">
      <div class="source">
        <div>
          <div class="row"><strong>Claude Code (terminal)</strong><span class="pill ok">Importado</span></div>
          <div class="muted small">{status.sessions} conversas em {status.projects} pastas</div>
          <div class="mono muted">~/.claude/projects</div>
        </div>
      </div>
      <div class="source">
        <div>
          <div class="row"><strong>Claude Desktop · sessões de código</strong><span class="pill ok">Importado</span></div>
          <div class="muted small">{status.desktop_sessions} das conversas acima foram feitas pelo Claude Desktop</div>
        </div>
      </div>
      <div class="source">
        <div>
          <div class="row"><strong>Claude Desktop · conversas do chat</strong><span class="pill warn">Ainda não disponível</span></div>
          <div class="muted small">Essas conversas ficam na nuvem da conta, não neste computador. A importação pelo arquivo de exportação da conta chega em uma próxima versão.</div>
        </div>
      </div>
    </section>

    <section class="card panel">
      <SettingRow flag="use_history" title="Usar conversas anteriores como contexto" description="Procura trechos relevantes no histórico antes de responder. Só os trechos encontrados são enviados ao modelo." />
    </section>

    <section class="card" aria-label="Conversas importadas">
      <div class="search">
        <input class="input" type="search" aria-label="Buscar no histórico" placeholder="Buscar no histórico" bind:value={query} oninput={refresh} />
      </div>
      {#each sessions as s (s.id)}
        <div class="session">
          <div class="info">
            <div class="title">{s.title ?? "Conversa sem título"}</div>
            <div class="muted small">{SOURCE_LABEL[s.source]} · {folderName(s.cwd)} · {formatWhen(s.mtime)} · {s.msg_count} mensagens</div>
          </div>
          <button type="button" class="btn small" onclick={() => open(s)}>Continuar</button>
        </div>
      {:else}
        <div class="empty muted">{query ? "Nenhuma conversa encontrada." : "Nenhuma conversa importada ainda."}</div>
      {/each}
    </section>
  </div>
</div>

<style>
  .source { padding: 14px 20px; border-bottom: 1px solid var(--hairline); }
  .source:last-child { border-bottom: 0; }
  .source .row { gap: 8px; flex-wrap: wrap; }
  .panel { padding: 20px; }
  .search { padding: 12px 20px; border-bottom: 1px solid var(--hairline); }
  .search .input { width: 100%; }
  .session { display: flex; align-items: center; gap: 16px; padding: 12px 20px; border-bottom: 1px solid var(--hairline); }
  .session:last-child { border-bottom: 0; }
  .info { flex: 1; min-width: 0; }
  .title { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { padding: 14px 20px; }
</style>
