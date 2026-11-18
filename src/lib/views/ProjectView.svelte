<script lang="ts">
  import { open as pickFiles } from "@tauri-apps/plugin-dialog";
  import { api, errorMessage } from "../api";
  import { formatWhen } from "../format";
  import { chat, loadConversations, openConversation, startAgentSession } from "../state/chat.svelte";
  import { loadProjects, projects } from "../state/projects.svelte";
  import type { GitInfo, MemoryFile, ProjectFile, View } from "../types";
  import Toggle from "../components/Toggle.svelte";
  import PipelineConfigCard from "../components/pipeline/PipelineConfigCard.svelte";

  interface Props {
    onnavigate: (view: View) => void;
  }
  let { onnavigate }: Props = $props();

  let name = $state("");
  let instructions = $state("");
  let files = $state<ProjectFile[]>([]);
  let error = $state<string | null>(null);
  let saved = $state(false);
  /** Memória do Claude Code na pasta do projeto. */
  let memory = $state<MemoryFile[]>([]);
  let memoryOpen = $state<string | null>(null);
  let memoryText = $state("");
  let memorySaved = $state(false);
  let newMemoryName = $state("");
  /** Git da pasta do projeto; nulo enquanto carrega. */
  let git = $state<GitInfo | null>(null);

  const project = $derived(projects.list.find((p) => p.id === projects.selectedId));
  const conversations = $derived(chat.conversations.filter((c) => project && c.project_id === project.id));
  const totalSize = $derived(files.reduce((sum, f) => sum + f.size, 0));

  // Carrega o formulário sempre que o projeto em foco muda.
  $effect(() => {
    const id = projects.selectedId;
    const current = projects.list.find((p) => p.id === id);
    name = current?.name ?? "";
    instructions = current?.instructions ?? "";
    error = null;
    saved = false;
    files = [];
    memory = [];
    memoryOpen = null;
    git = null;
    if (id) {
      void api.listProjectFiles(id).then((list) => (files = list));
      if (current?.folder) {
        void api.projectMemoryList(id).then((list) => (memory = list));
        void api.projectGitInfo(id).then((info) => {
          if (projects.selectedId === id) git = info;
        });
      }
    }
  });

  const openMemory = (name: string) =>
    run(async () => {
      if (!project) return;
      memoryText = await api.projectMemoryRead(project.id, name);
      memoryOpen = name;
      memorySaved = false;
    });

  const saveMemory = () =>
    run(async () => {
      if (!project || !memoryOpen) return;
      await api.projectMemoryWrite(project.id, memoryOpen, memoryText);
      memory = await api.projectMemoryList(project.id);
      memorySaved = true;
    });

  const removeMemory = (name: string) =>
    run(async () => {
      if (!project) return;
      await api.projectMemoryRemove(project.id, name);
      memory = await api.projectMemoryList(project.id);
      if (memoryOpen === name) memoryOpen = null;
    });

  const createMemory = (event: SubmitEvent) => {
    event.preventDefault();
    return run(async () => {
      if (!project) return;
      const name = newMemoryName.trim().endsWith(".md") ? newMemoryName.trim() : `${newMemoryName.trim()}.md`;
      await api.projectMemoryWrite(project.id, name, "");
      memory = await api.projectMemoryList(project.id);
      newMemoryName = "";
      await openMemory(name);
    });
  };

  /** Executa uma ação mostrando o erro, se houver. */
  async function run(action: () => Promise<void>) {
    error = null;
    try {
      await action();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  const save = (event: SubmitEvent) => {
    event.preventDefault();
    return run(async () => {
      const result = await api.saveProject({ id: project?.id ?? null, name, instructions });
      await loadProjects();
      projects.selectedId = result.id;
      saved = true;
    });
  };

  const setIsolate = (isolate: boolean) =>
    run(async () => {
      if (!project) return;
      await api.setProjectIsolate(project.id, isolate);
      await loadProjects();
    });

  const remove = () =>
    run(async () => {
      if (!project) return;
      await api.deleteProject(project.id);
      await Promise.all([loadProjects(), loadConversations()]);
      projects.selectedId = null;
      onnavigate("chat");
    });

  const addFiles = () =>
    run(async () => {
      if (!project) return;
      const picked = await pickFiles({ multiple: true, title: "Arquivos de conhecimento do projeto" });
      if (!picked) return;
      try {
        await api.addProjectFiles(project.id, Array.isArray(picked) ? picked : [picked]);
      } finally {
        files = await api.listProjectFiles(project.id);
      }
    });

  const removeFile = (file: ProjectFile) =>
    run(async () => {
      if (!project) return;
      await api.removeProjectFile(project.id, file.id);
      files = files.filter((f) => f.id !== file.id);
    });

  /** Projeto com pasta abre uma sessão de agente nela; sem pasta, uma conversa comum. */
  async function newConversation() {
    if (!project) return;
    if (project.folder) {
      await startAgentSession(project.folder);
    } else {
      projects.pendingId = project.id;
      await openConversation(null);
    }
    onnavigate("chat");
  }

  async function open(id: string) {
    await openConversation(id);
    onnavigate("chat");
  }
</script>

<div class="page">
  <div class="page-inner">
    <header class="page-head">
      <div>
        <h1>{project?.name ?? "Novo projeto"}</h1>
        <p class="muted">As instruções e os arquivos do projeto acompanham todas as conversas feitas nele.</p>
        {#if project?.folder}<p class="mono muted folder" title="Pasta do projeto">{project.folder}</p>{/if}
      </div>
      {#if project}
        <button type="button" class="btn primary" onclick={newConversation}>{project.folder ? "Nova sessão nesta pasta" : "Nova conversa neste projeto"}</button>
      {/if}
    </header>

    {#if error}<p class="error-text" role="alert">{error}</p>{/if}

    <form class="card panel" onsubmit={save}>
      <label>Nome<input class="input" bind:value={name} placeholder="Ex.: Portal do cliente" oninput={() => (saved = false)} /></label>
      <label>Instruções
        <textarea class="input" rows="5" bind:value={instructions} placeholder="O que o assistente deve saber e como deve responder neste projeto" oninput={() => (saved = false)}></textarea>
      </label>
      <div class="row">
        <button type="submit" class="btn primary" disabled={!name.trim()}>{project ? "Salvar" : "Criar projeto"}</button>
        {#if saved}<span class="muted small" role="status">Salvo.</span>{/if}
        <span class="spacer"></span>
        {#if project}<button type="button" class="btn danger" onclick={remove}>Excluir projeto</button>{/if}
      </div>
    </form>

    {#if project}
      <section class="card" aria-label="Arquivos de conhecimento">
        <div class="head">
          <div>
            <h2 class="section-title">Conhecimento</h2>
            <div class="muted small">Arquivos de texto que o assistente lê quando a pergunta precisa deles · {totalSize.toLocaleString("pt-BR")} de 150.000 caracteres</div>
          </div>
          <button type="button" class="btn small" onclick={addFiles}>Adicionar arquivos</button>
        </div>
        {#each files as f (f.id)}
          <div class="line">
            <span class="grow">{f.name}</span>
            <span class="muted small">{f.size.toLocaleString("pt-BR")} caracteres</span>
            <button type="button" class="btn icon" aria-label={`Remover ${f.name}`} title={`Remover ${f.name}`} onclick={() => removeFile(f)}>
              <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
            </button>
          </div>
        {:else}
          <div class="line muted">Nenhum arquivo ainda.</div>
        {/each}
      </section>

      {#if project.folder}
        <section class="card" aria-label="Versionamento">
          <div class="head">
            <div>
              <h2 class="section-title">Versionamento</h2>
              <div class="muted small">
                {#if !git}
                  Verificando o git da pasta…
                {:else if !git.is_repo}
                  A pasta não é um repositório git: a esteira de bots altera os arquivos direto nela, e no fim não há commit a fazer.
                {:else}
                  Repositório git detectado{git.branch ? `, no ramo ${git.branch}` : ""}{git.remote ? `, com o remoto ${git.remote}` : ", sem remoto (sem push nem pull request)"}.
                {/if}
              </div>
            </div>
          </div>
          {#if git?.is_repo}
            <div class="line tall">
              <div class="grow">
                <div>Trabalhar numa cópia isolada (worktree)</div>
                <div class="muted small">
                  {project.isolate
                    ? "Os bots trabalham numa cópia da pasta, ao lado dela, num ramo novo: a pasta principal fica intacta e você continua trabalhando nela. No fim, a cópia vai para a main ou vira um pull request."
                    : "Os bots alteram os arquivos direto na pasta principal, no ramo em que ela estiver. Evite mexer na pasta enquanto a esteira roda."}
                </div>
              </div>
              <Toggle checked={project.isolate} label="Trabalhar numa cópia isolada (worktree)" onchange={setIsolate} />
            </div>
          {/if}
        </section>

        <PipelineConfigCard projectId={project.id} {git} {onnavigate} />

        <section class="card" aria-label="Memória do projeto">
          <div class="head">
            <div>
              <h2 class="section-title">Memória</h2>
              <div class="muted small">O que o assistente aprendeu trabalhando nesta pasta. Ele lê estes arquivos em toda sessão; edite ou apague o que não deve lembrar.</div>
            </div>
            <form class="row" onsubmit={createMemory}>
              <input class="input small-input" placeholder="nova-nota.md" bind:value={newMemoryName} aria-label="Nome do novo arquivo de memória" />
              <button type="submit" class="btn small" disabled={!newMemoryName.trim()}>Criar</button>
            </form>
          </div>
          {#each memory as m (m.name)}
            <div class="line" class:on={memoryOpen === m.name}>
              <button type="button" class="grow link-text" onclick={() => openMemory(m.name)}>{m.name}</button>
              <span class="muted small">{m.modified ? formatWhen(m.modified) : ""}</span>
              <button type="button" class="btn icon" aria-label={`Apagar ${m.name}`} title={`Apagar ${m.name}`} onclick={() => removeMemory(m.name)}>
                <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
              </button>
            </div>
          {:else}
            <div class="line muted">Nada memorizado ainda.</div>
          {/each}
          {#if memoryOpen}
            <div class="editor">
              <textarea class="input mono" rows="12" bind:value={memoryText} oninput={() => (memorySaved = false)} aria-label={`Conteúdo de ${memoryOpen}`}></textarea>
              <div class="row">
                <button type="button" class="btn primary" onclick={saveMemory}>Salvar {memoryOpen}</button>
                {#if memorySaved}<span class="muted small" role="status">Salvo.</span>{/if}
                <span class="spacer"></span>
                <button type="button" class="btn" onclick={() => (memoryOpen = null)}>Fechar</button>
              </div>
            </div>
          {/if}
        </section>
      {/if}

      <section class="card" aria-label="Conversas do projeto">
        <div class="head"><h2 class="section-title">Conversas</h2></div>
        {#each conversations as c (c.id)}
          <button type="button" class="line link" onclick={() => open(c.id)}>
            <span class="grow">{c.title}</span>
            <span class="muted small">{formatWhen(c.updated_at)}</span>
          </button>
        {:else}
          <div class="line muted">Nenhuma conversa neste projeto ainda.</div>
        {/each}
      </section>
    {/if}
  </div>
</div>

<style>
  .folder { margin-top: 6px; }
  .panel { padding: 20px; display: flex; flex-direction: column; gap: 12px; }
  .panel label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; font-weight: 500; }
  .panel .input { font-weight: 400; }
  .panel textarea { padding: 8px 12px; resize: vertical; }
  .spacer, .grow { flex: 1; min-width: 0; }
  .grow { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .head { display: flex; align-items: center; gap: 12px; padding: 14px 16px; border-bottom: 1px solid var(--hairline); }
  .head > div { flex: 1; }
  .line { display: flex; align-items: center; gap: 12px; width: 100%; min-height: 44px; padding: 0 16px; border: 0; border-bottom: 1px solid var(--hairline); background: transparent; text-align: left; }
  .line:last-child { border-bottom: 0; }
  .line.tall { padding: 10px 16px; }
  .link:hover { background: var(--hover); }
  .line.on { background: var(--hover); }
  .link-text { padding: 0; border: 0; background: transparent; color: var(--text); text-align: left; min-height: 44px; }
  .link-text:hover { color: var(--accent-text); }
  .small-input { min-height: 30px; width: 170px; font-size: 12.5px; }
  .editor { padding: 12px 16px 16px; display: flex; flex-direction: column; gap: 10px; border-top: 1px solid var(--hairline); }
  .editor textarea { width: 100%; padding: 10px 12px; resize: vertical; font-size: 12.5px; line-height: 1.5; }
</style>
