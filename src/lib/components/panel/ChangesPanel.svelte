<script lang="ts">
  import { api, errorMessage } from "../../api";
  import { refreshStatus, workspace } from "../../state/workspace.svelte";
  import type { ChangedFile, Conversation } from "../../types";

  interface Props {
    conversation: Conversation;
  }
  let { conversation }: Props = $props();

  let selected = $state<string | null>(null);
  let diff = $state("");
  let message = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let done = $state(false);

  const status = $derived(workspace.status);
  const files = $derived(status?.files ?? []);

  /** Linha do diff com um comentário em edição e o texto digitado. */
  let commentAt = $state<number | null>(null);
  let comment = $state("");

  /** Linhas do diff, sem o cabeçalho do git, com o tipo e o número no arquivo novo. */
  const lines = $derived.by(() => {
    let next = 0;
    return diff
      .split("\n")
      .filter((l) => !/^(diff --git|index |--- |\+\+\+ |new file|deleted file|similarity|rename )/.test(l))
      .map((text, index) => {
        const hunk = text.match(/^@@ -\d+(?:,\d+)? \+(\d+)/);
        if (hunk) next = Number(hunk[1]);
        const kind = text.startsWith("+") ? "add" : text.startsWith("-") ? "del" : hunk ? "hunk" : "";
        const number = kind === "del" || kind === "hunk" ? null : next++;
        return { index, text, kind, number };
      });
  });

  function toggleComment(index: number) {
    commentAt = commentAt === index ? null : index;
    comment = "";
  }

  /** Manda o comentário para o campo de mensagem, com o arquivo, a linha e o trecho. */
  function sendComment(event: SubmitEvent) {
    event.preventDefault();
    const line = lines.find((l) => l.index === commentAt);
    if (!line || !selected || !comment.trim()) return;
    const where = line.number ? `linha ${line.number}` : "trecho removido";
    workspace.composerInsert = `Em \`${selected}\` (${where}):\n\`\`\`\n${line.text.slice(1)}\n\`\`\`\n${comment.trim()}`;
    commentAt = null;
    comment = "";
  }

  async function show(file: ChangedFile) {
    selected = file.path;
    error = null;
    try {
      diff = await api.workspaceDiff(conversation.id, file.path);
    } catch (e) {
      diff = "";
      error = errorMessage(e);
    }
  }

  async function commit(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    error = null;
    try {
      await api.workspaceCommit(conversation.id, message);
      message = "";
      selected = null;
      diff = "";
      done = true;
      await refreshStatus(conversation.id, true);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  // Mostra o primeiro arquivo quando a lista muda e nada está selecionado.
  $effect(() => {
    if (files.length && !files.some((f) => f.path === selected)) void show(files[0]);
  });
</script>

<div class="panel">
  {#if !status?.is_repo}
    <p class="muted empty">Esta pasta não é um repositório git.</p>
  {:else if !files.length}
    <p class="muted empty" role="status">{done ? "Commit feito. " : ""}Nada alterado desde o último commit.</p>
  {:else}
    <div class="list">
      <div class="head">
        <h2 class="label">Alterações{status.branch ? ` · ${status.branch}` : ""}</h2>
        <span class="mono add">+{status.added.toLocaleString("pt-BR")}</span>
        <span class="mono del">−{status.removed.toLocaleString("pt-BR")}</span>
      </div>
      {#each files as f (f.path)}
        <button type="button" class="file" class:on={selected === f.path} aria-pressed={selected === f.path} onclick={() => show(f)}>
          <span class="mono status">{f.status}</span>
          <span class="mono path">{f.path}</span>
          <span class="mono add">+{f.added}</span>
          <span class="mono del">−{f.removed}</span>
        </button>
      {/each}
    </div>

    <div class="diff" aria-label="Diff; clique em uma linha para comentar">
      {#each lines as line (line.index)}
        <button type="button" class={`line ${line.kind}`} class:on={commentAt === line.index} onclick={() => toggleComment(line.index)}>
          <span class="num">{line.number ?? ""}</span><span class="code">{line.text || " "}</span>
        </button>
        {#if commentAt === line.index}
          <form class="comment" onsubmit={sendComment}>
            <!-- svelte-ignore a11y_autofocus -->
            <input class="input" bind:value={comment} placeholder="O que o agente deve mudar aqui?" autofocus />
            <button type="submit" class="btn small primary" disabled={!comment.trim()}>Enviar ao agente</button>
          </form>
        {/if}
      {/each}
    </div>

    <form onsubmit={commit}>
      <label for="commit-message" class="small muted">Mensagem do commit</label>
      <input id="commit-message" class="input" bind:value={message} placeholder="O que mudou e por quê" disabled={busy} />
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
      <button type="submit" class="btn primary" disabled={busy || !message.trim()}>
        {busy ? "Registrando…" : "Fazer commit das alterações"}
      </button>
    </form>
  {/if}
</div>

<style>
  .panel { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .empty { padding: 16px; }
  .list { flex: 0 1 auto; max-height: 38%; overflow-y: auto; padding: 14px 8px 10px; display: flex; flex-direction: column; gap: 1px; border-bottom: 1px solid var(--border); }
  .head { display: flex; align-items: center; gap: 8px; margin: 0 8px 8px; }
  .head .label { flex: 1; margin: 0; }
  .file { display: flex; align-items: center; gap: 8px; min-height: 28px; padding: 0 8px; border: 0; border-radius: 6px; background: transparent; text-align: left; }
  .file:hover, .file.on { background: var(--hover); }
  .status { width: 14px; flex-shrink: 0; font-size: 11px; color: var(--warn); }
  .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }
  .add { color: var(--ok-text); font-size: 11.5px; }
  .del { color: var(--danger-text); font-size: 11.5px; }
  .diff { flex: 1; min-height: 0; margin: 0; padding: 10px 0; overflow: auto; background: var(--bg); font-family: var(--mono); font-size: 12px; line-height: 1.75; user-select: text; }
  .line { display: flex; width: 100%; min-width: max-content; padding: 0 14px 0 0; border: 0; background: transparent; text-align: left; font: inherit; color: var(--text-2); cursor: pointer; }
  .line:hover, .line.on { outline: 1px solid var(--border-strong); outline-offset: -1px; }
  .num { width: 44px; flex-shrink: 0; padding-right: 8px; text-align: right; font-size: 11px; color: var(--faint); user-select: none; }
  .code { white-space: pre; }
  .line.add { background: var(--ok-bg); color: var(--ok-text); }
  .line.del { background: var(--danger-bg); color: var(--danger-text); }
  .line.hunk { color: var(--muted); }
  .comment { display: flex; gap: 6px; padding: 6px 14px 8px 44px; background: var(--surface); font-family: var(--font); }
  .comment .input { flex: 1; min-height: 30px; font-size: 12.5px; }
  form { padding: 12px; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 8px; }
  form .input { width: 100%; font-size: 12.5px; }
</style>
