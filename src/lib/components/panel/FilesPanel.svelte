<script lang="ts">
  import { untrack } from "svelte";
  import { api, errorMessage } from "../../api";
  import { describeFile, folderName } from "../../format";
  import { workspace } from "../../state/workspace.svelte";
  import type { Conversation, FileEntry, FileView } from "../../types";
  import CodeView from "../CodeView.svelte";

  interface Props {
    conversation: Conversation;
  }
  let { conversation }: Props = $props();

  /** Conteúdo já carregado de cada pasta (chave: caminho relativo; "" é a raiz). */
  let folders = $state<Record<string, FileEntry[]>>({});
  let expanded = $state<Record<string, boolean>>({});
  let file = $state<FileView | null>(null);
  let error = $state<string | null>(null);

  const PASTA = "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z";
  const ARQUIVO = "M6 3h9l4 4v14H6zM14 3v5h5";

  /** Situação no git de cada arquivo alterado, para marcar na árvore. */
  const changed = $derived(new Map((workspace.status?.files ?? []).map((f) => [f.path, f.status])));

  /** Árvore achatada na ordem de exibição, só com as pastas abertas. */
  const rows = $derived.by(() => {
    const out: { entry: FileEntry; depth: number }[] = [];
    const walk = (dir: string, depth: number) => {
      for (const entry of folders[dir] ?? []) {
        out.push({ entry, depth });
        if (entry.is_dir && expanded[entry.path]) walk(entry.path, depth + 1);
      }
    };
    walk("", 0);
    return out;
  });

  /** Só a troca de sessão recarrega a árvore, não cada atualização da conversa. */
  const sessionId = $derived(conversation.id);

  async function load(dir: string) {
    const session = sessionId;
    try {
      const entries = await api.workspaceEntries(session, dir);
      // A resposta de uma sessão que já foi trocada é descartada.
      if (session === sessionId) folders[dir] = entries;
    } catch (e) {
      if (session === sessionId) error = errorMessage(e);
    }
  }

  /** Um clique: abre a pasta ou mostra a prévia do arquivo aqui no painel. */
  async function open(entry: FileEntry) {
    error = null;
    if (entry.is_dir) {
      expanded[entry.path] = !expanded[entry.path];
      if (expanded[entry.path] && !folders[entry.path]) await load(entry.path);
      return;
    }
    try {
      file = await api.workspaceRead(sessionId, entry.path);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  /** Dois cliques: abre o arquivo grande, no centro, acima do chat. */
  async function openLarge(entry: FileEntry) {
    if (entry.is_dir) return;
    try {
      workspace.openFile = await api.workspaceRead(sessionId, entry.path);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  // Ao trocar de sessão, recomeça da raiz da nova pasta.
  $effect(() => {
    void sessionId;
    // Sem rastrear o que é lido aqui dentro: o efeito escreve nesses mesmos
    // estados e, rastreando-os, ele se reexecutaria sem parar.
    untrack(() => {
      folders = {};
      expanded = {};
      file = null;
      error = null;
      void load("");
    });
  });
</script>

<div class="panel">
  <div class="tree" role="tree" aria-label={`Arquivos de ${folderName(conversation.cwd)}`}>
    <h2 class="label">Arquivos · {folderName(conversation.cwd)}</h2>
    {#each rows as { entry, depth } (entry.path)}
      <button
        type="button"
        role="treeitem"
        aria-selected={file?.path === entry.path}
        aria-expanded={entry.is_dir ? !!expanded[entry.path] : undefined}
        class="node"
        class:dir={entry.is_dir}
        class:on={file?.path === entry.path}
        style:padding-left={`${8 + depth * 14}px`}
        title={entry.is_dir ? undefined : "Dois cliques abrem o arquivo no centro"}
        onclick={() => open(entry)}
        ondblclick={() => openLarge(entry)}
      >
        <svg aria-hidden="true" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d={entry.is_dir ? PASTA : ARQUIVO} /></svg>
        <span class="name">{entry.name}</span>
        {#if changed.has(entry.path)}<span class="mark" title="Alterado">{changed.get(entry.path)}</span>{/if}
      </button>
    {:else}
      <p class="muted small empty">{error ?? "Pasta vazia."}</p>
    {/each}
  </div>

  {#if file}
    <div class="file-head">
      <span class="mono path">{file.path}</span>
      <span class="muted info">{describeFile(file)}</span>
      <button type="button" class="btn icon" aria-label="Abrir o arquivo no centro" title="Abrir no centro" onclick={() => (workspace.openFile = file)}>
        <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 4H4v5M15 20h5v-5M4 4l6 6M20 20l-6-6" /></svg>
      </button>
    </div>
    {#if file.text !== null}
      <CodeView text={file.text} path={file.path} />
    {:else}
      <div class="no-preview">
        <strong>Sem prévia de texto</strong>
        <span class="muted">Este arquivo não é texto ou é grande demais para exibir aqui.</span>
      </div>
    {/if}
  {:else if rows.length}
    <p class="muted small empty">{error ?? "Um clique mostra o arquivo aqui; dois cliques abrem no centro."}</p>
  {/if}
</div>

<style>
  .panel { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .tree { flex: 0 1 auto; max-height: 45%; min-height: 120px; overflow-y: auto; padding: 14px 8px 10px; display: flex; flex-direction: column; gap: 1px; border-bottom: 1px solid var(--border); }
  .label { margin: 0 8px 8px; }
  .node { display: flex; align-items: center; gap: 7px; min-height: 26px; padding-right: 8px; border: 0; border-radius: 6px; background: transparent; font-size: 12.5px; text-align: left; color: var(--text-body); user-select: none; }
  .node.dir { color: var(--text-2); }
  .node:hover, .node.on { background: var(--hover); }
  .node.on { color: #fff; }
  .node svg { flex-shrink: 0; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mark { font-family: var(--mono); font-size: 11px; color: var(--warn); }
  .empty { padding: 12px 16px; }
  .file-head { display: flex; align-items: center; gap: 8px; min-height: 36px; padding: 0 6px 0 14px; border-bottom: 1px solid var(--border); }
  .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .info { flex-shrink: 0; font-size: 11.5px; }
  .no-preview { padding: 20px 16px; display: flex; flex-direction: column; gap: 6px; }
</style>
