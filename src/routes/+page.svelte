<script lang="ts">
  import { onMount } from "svelte";
  import "../app.css";
  import { api } from "$lib/api";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import UpdateBanner from "$lib/components/UpdateBanner.svelte";
  import { activeAccount, loadAccounts, loadFlags } from "$lib/state/accounts.svelte";
  import { connectors } from "$lib/state/connectors.svelte";
  import { loadAgents } from "$lib/state/agents.svelte";
  import { loadProjects } from "$lib/state/projects.svelte";
  import { handleChatEvent, loadConversations, openConversation } from "$lib/state/chat.svelte";
  import type { View } from "$lib/types";
  import AccountsView from "$lib/views/AccountsView.svelte";
  import ChatView from "$lib/views/ChatView.svelte";
  import ConnectorsView from "$lib/views/ConnectorsView.svelte";
  import ProjectView from "$lib/views/ProjectView.svelte";
  import SkillsView from "$lib/views/SkillsView.svelte";
  import HistoryView from "$lib/views/HistoryView.svelte";
  import TasksView from "$lib/views/TasksView.svelte";
  import PipelinesView from "$lib/views/PipelinesView.svelte";
  import SettingsView from "$lib/views/SettingsView.svelte";
  import { loadUiPrefs } from "$lib/state/ui.svelte";
  import { handlePipelineChanged, isStageConversation, loadPipelines, loadStageMessages } from "$lib/state/pipelines.svelte";
  import type { ChatEvent } from "$lib/types";

  let view = $state<View>("chat");
  let cliFound = $state(true);

  onMount(() => {
    // Os conectores da conta demoram para verificar: começam a carregar já na abertura.
    void Promise.all([loadAccounts(), loadFlags(), loadConversations(), loadAgents(), loadProjects()]).then(() => connectors.ensure(activeAccount()?.id ?? null));
    void api.environment().then((env) => (cliFound = env.cli_found));
    // Uso das contas direto do provedor, para a barra lateral e o anel do compositor abrirem atualizados.
    const usageTimer = setTimeout(() => void api.refreshUsage().then(loadAccounts).catch(() => {}), 3000);
    // Mensagem enviada pela entrada rápida: abre a conversa recém-criada.
    const showConversation = async (id: string) => {
      await loadConversations();
      await openConversation(id);
      view = "chat";
    };
    void loadPipelines();
    void loadUiPrefs();
    // O que as etapas da esteira gravam aparece no log da tela de acompanhamento.
    const STAGE_REFRESH = new Set<ChatEvent["kind"]>(["step", "done", "waiting", "notice", "error", "exhausted", "cancelled"]);
    const onChat = (event: ChatEvent) => {
      handleChatEvent(event);
      if (STAGE_REFRESH.has(event.kind) && isStageConversation(event.conversation_id)) void loadStageMessages(event.conversation_id);
    };
    const listeners = [
      api.onChat(onChat),
      api.onPipelineChanged((p) => void handlePipelineChanged(p.id)),
      api.onAccountsChanged(() => void loadAccounts()),
      api.onOpenConversation((id) => void showConversation(id)),
    ];
    return () => {
      clearTimeout(usageTimer);
      listeners.forEach((l) => void l.then((stop) => stop()));
    };
  });
</script>

<div class="app">
  <Sidebar {view} onnavigate={(v) => (view = v)} />
  <div class="content">
    <UpdateBanner />
    {#if !cliFound}
      <div class="banner" role="alert">
        O Claude Code não foi encontrado neste computador. Instale-o e abra o Lume de novo para conversar.
      </div>
    {/if}
    {#if view === "chat"}
      <ChatView onnavigate={(v) => (view = v)} />
    {:else if view === "project"}
      <ProjectView onnavigate={(v) => (view = v)} />
    {:else if view === "skills"}
      <SkillsView />
    {:else if view === "connectors"}
      <ConnectorsView />
    {:else if view === "history"}
      <HistoryView onnavigate={(v) => (view = v)} />
    {:else if view === "pipelines"}
      <PipelinesView onnavigate={(v) => (view = v)} />
    {:else if view === "tasks"}
      <TasksView onnavigate={(v) => (view = v)} />
    {:else if view === "settings"}
      <SettingsView />
    {:else}
      <AccountsView />
    {/if}
  </div>
</div>

<style>
  /* Presa às bordas da janela, e não a 100vh: a webview pode manter a altura antiga ao
     redimensionar, o que deixava o app menor que a janela e a janela inteira rolando. */
  .app { position: fixed; inset: 0; display: flex; overflow: hidden; }
  .content { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .content :global(.split), .content :global(.page) { flex: 1; min-height: 0; }
  .banner { padding: 10px 24px; background: var(--warn-bg); color: var(--warn-text); font-size: 13px; }
</style>
