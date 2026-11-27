<script lang="ts">
  import { slide } from "svelte/transition";
  import { renderMarkdown } from "../format";
  import { parsePlan, stepTitle } from "../plan";
  import type { PermissionRequest } from "../state/chat.svelte";
  import { parseInput } from "../tools";

  interface Props {
    request: PermissionRequest;
    /** Quantas vezes o plano já foi apresentado nesta conversa, contando este. */
    version?: number;
    /**
     * Aprovado: o modo com que a sessão segue. `null` pede mudanças: o agente continua
     * planejando, com `feedback` dizendo o que mudar.
     */
    ondecide: (nextMode: string | null, feedback?: string) => void | Promise<void>;
  }
  let { request, version = 1, ondecide }: Props = $props();

  const plan = $derived(parsePlan(String(parseInput(request.input).plan ?? "")));

  /** Comentários do usuário por passo (posição no plano). */
  let comments = $state<Record<number, string[]>>({});
  let commenting = $state<number | null>(null);
  let commentText = $state("");
  let asking = $state(false);
  let note = $state("");
  let sent = $state(false);

  const commentCount = $derived(Object.values(comments).reduce((n, list) => n + list.length, 0));
  const canSendChanges = $derived(commentCount > 0 || note.trim().length > 0);

  function addComment() {
    const text = commentText.trim();
    if (commenting === null || !text) return;
    comments[commenting] = [...(comments[commenting] ?? []), text];
    commenting = null;
    commentText = "";
  }

  function removeComment(step: number, i: number) {
    comments[step] = comments[step].filter((_, k) => k !== i);
    if (!comments[step].length) delete comments[step];
  }

  function feedback(): string {
    const lines = Object.entries(comments).flatMap(([i, list]) =>
      list.map((c) => `- Passo ${Number(i) + 1} (${stepTitle(plan.steps[Number(i)]?.text ?? "")}): ${c}`),
    );
    const parts = ["O usuário pediu ajustes no plano. Revise o plano com base nisto e apresente-o de novo."];
    if (lines.length) parts.push(`Comentários nos passos:\n${lines.join("\n")}`);
    if (note.trim()) parts.push(note.trim());
    return parts.join("\n\n");
  }

  function decide(mode: string | null) {
    if (sent) return;
    if (mode === null && !canSendChanges) {
      asking = true;
      return;
    }
    sent = true;
    // Se a resposta não chegar ao agente, o pedido continua na tela e volta a aceitar a decisão.
    Promise.resolve(ondecide(mode, mode === null ? feedback() : undefined)).catch(() => (sent = false));
  }

  function onKeydown(event: KeyboardEvent) {
    // Num campo de texto (o comentário, os ajustes, o composer) o atalho é de quem digita.
    const target = event.target as HTMLElement | null;
    if (event.defaultPrevented || target?.closest("input, textarea, [contenteditable]")) return;
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey) && !event.isComposing) {
      event.preventDefault();
      decide("acceptEdits");
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<section class="plan" id="plano-pendente" role="group" aria-label="Plano para aprovação" data-mm="Plano aguardando aprovação" data-mm-kind="plan">
  <div class="head">
    <span class="ic" aria-hidden="true">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 4L3 6v14l6-2 6 2 6-2V4l-6 2zM9 4v14M15 6v14" /></svg>
    </span>
    <div class="tx">
      <span class="kicker">Plano · aguardando sua aprovação</span>
      <h3>{plan.title}</h3>
    </div>
    <span class="ver" title="Versão do plano nesta conversa">v{version}</span>
  </div>

  {#if plan.steps.length || plan.files.length}
    <div class="meta">
      {#if plan.steps.length}<span>{plan.steps.length === 1 ? "1 passo" : `${plan.steps.length} passos`}</span>{/if}
      {#if plan.files.length}<span>{plan.files.length === 1 ? "1 arquivo citado" : `${plan.files.length} arquivos citados`}</span>{/if}
    </div>
  {/if}

  {#each plan.sections as section, s (s)}
    <div class="sec">
      {#if section.heading}<h4>{section.heading}</h4>{/if}
      {#each section.parts as part, p (p)}
        {#if part.kind === "md"}
          {#if part.raw.trim()}<div class="markdown">{@html renderMarkdown(part.raw)}</div>{/if}
        {:else}
          <ol class="steps">
            {#each part.items as step (step.index)}
              <li class="step" style={`--i:${step.index}`}>
                <span class="num">{step.index + 1}</span>
                <div class="body">
                  <div class="markdown">{@html renderMarkdown(step.text)}</div>
                  {#each comments[step.index] ?? [] as comment, k (k)}
                    <div class="comment" transition:slide={{ duration: 200 }}>
                      <span class="you">Você</span>
                      <span class="ct">{comment}</span>
                      <button type="button" aria-label="Remover comentário" title="Remover" onclick={() => removeComment(step.index, k)}>×</button>
                    </div>
                  {/each}
                  {#if commenting === step.index}
                    <form class="comment-form" transition:slide={{ duration: 200 }} onsubmit={(e) => (e.preventDefault(), addComment())}>
                      <!-- svelte-ignore a11y_autofocus -->
                      <input class="input" type="text" placeholder="Comentário sobre este passo" aria-label="Comentário sobre este passo" bind:value={commentText} autofocus onkeydown={(e) => e.key === "Escape" && (commenting = null)} />
                      <button type="submit" class="btn small">Adicionar</button>
                    </form>
                  {/if}
                </div>
                <button type="button" class="comment-btn" onclick={() => ((commenting = step.index), (commentText = ""))}>
                  <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M4 5h16v11H9l-5 4z" /></svg>
                  Comentar
                </button>
              </li>
            {/each}
          </ol>
        {/if}
      {/each}
    </div>
  {/each}

  {#if plan.files.length}
    <div class="sec">
      <h4>Arquivos citados</h4>
      <div class="files">{#each plan.files as file (file)}<code>{file}</code>{/each}</div>
    </div>
  {/if}

  {#if asking}
    <div class="changes" transition:slide={{ duration: 260 }}>
      <!-- svelte-ignore a11y_autofocus -->
      <textarea class="input" placeholder="O que você quer diferente no plano?" aria-label="Mudanças no plano" bind:value={note} autofocus></textarea>
      <div class="changes-row">
        <span class="muted small">{commentCount ? `${commentCount === 1 ? "1 comentário nos passos vai" : `${commentCount} comentários nos passos vão`} junto.` : "Escreva o que quer diferente ou comente nos passos."}</span>
        <button type="button" class="btn small" onclick={() => (asking = false)}>Cancelar</button>
        <button type="button" class="btn small primary" disabled={!canSendChanges || sent} onclick={() => decide(null)}>Enviar ajustes</button>
      </div>
    </div>
  {/if}

  <div class="actions">
    <button type="button" class="btn approve" disabled={sent} onclick={() => decide("acceptEdits")}>Aprovar e aceitar edições <kbd>⌘↵</kbd></button>
    <button type="button" class="btn" disabled={sent} onclick={() => decide("auto")}>Aprovar, perguntando antes de agir</button>
    <button type="button" class="btn ghost" disabled={sent} onclick={() => (asking ? decide(null) : (asking = true))}>
      Pedir mudanças
      {#if commentCount}<span class="count">{commentCount}</span>{/if}
    </button>
  </div>
</section>

<style>
  .plan { position: relative; padding: 16px 18px 12px; border: 1px solid rgba(91, 106, 217, 0.4); border-radius: 14px; background: linear-gradient(rgba(91, 106, 217, 0.07), rgba(91, 106, 217, 0.03)), var(--surface); display: flex; flex-direction: column; gap: 4px; animation: glow 2s var(--ease) both; }
  .head { display: flex; align-items: center; gap: 12px; }
  .ic { width: 32px; height: 32px; flex-shrink: 0; display: grid; place-items: center; border-radius: 10px; color: var(--accent-text); background: var(--acc-bg); }
  .tx { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .kicker { font-size: 10.5px; font-weight: 600; letter-spacing: 0.1em; text-transform: uppercase; color: var(--accent-text); }
  h3 { margin: 0; font-family: var(--display); font-size: 17px; font-weight: 600; line-height: 1.2; color: var(--text); text-wrap: balance; }
  .ver { flex-shrink: 0; padding: 2px 8px; border-radius: 999px; font-family: var(--mono); font-size: 11.5px; font-weight: 600; color: var(--accent-text); background: var(--acc-bg); }
  .meta { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 10px; }
  .meta span { padding: 2px 9px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface); font-size: 12px; color: var(--text-2); }

  .sec { margin-top: 14px; }
  h4 { margin: 0 0 8px; font-size: 10.5px; font-weight: 600; letter-spacing: 0.1em; text-transform: uppercase; color: var(--muted); }
  .markdown { font-size: 13.5px; color: var(--text-body); overflow-wrap: anywhere; }
  .markdown :global(p) { margin: 0 0 8px; }
  .markdown :global(> :last-child) { margin-bottom: 0; }
  .markdown :global(ul) { margin: 0 0 8px; padding-left: 20px; }
  .markdown :global(code) { font-family: var(--mono); font-size: 12px; padding: 1px 5px; border-radius: 4px; background: var(--surface-2); }
  .markdown :global(pre) { margin: 0 0 8px; padding: 10px 12px; overflow-x: auto; border: 1px solid var(--border); border-radius: 8px; background: var(--bg-side); }
  .markdown :global(pre code) { padding: 0; background: transparent; }
  .markdown :global(strong) { color: var(--text); }

  .steps { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 2px; }
  .step { position: relative; display: grid; grid-template-columns: 26px minmax(0, 1fr) auto; gap: 12px; margin: 0 -10px; padding: 10px; border-radius: 10px; transition: background 0.2s; animation: rise 0.5s var(--ease) both; animation-delay: calc(var(--i) * 70ms); }
  .step:hover, .step:focus-within { background: rgba(255, 255, 255, 0.025); }
  .step:not(:last-child)::before { content: ""; position: absolute; left: 22.5px; top: 40px; bottom: -6px; width: 1px; background: rgba(91, 106, 217, 0.3); }
  .num { position: relative; z-index: 1; width: 26px; height: 26px; display: grid; place-items: center; border: 1px solid rgba(91, 106, 217, 0.4); border-radius: 8px; background: var(--acc-solid); font-family: var(--mono); font-size: 12px; font-weight: 600; color: var(--accent-text); }
  .body { min-width: 0; }
  .body .markdown :global(p:first-child) { color: var(--text); font-weight: 500; }
  .comment-btn { align-self: start; display: inline-flex; align-items: center; gap: 6px; height: 26px; padding: 0 8px; border: 0; border-radius: 7px; background: transparent; font-size: 12px; color: var(--muted); opacity: 0; transition: opacity 0.2s, background 0.2s; }
  .step:hover .comment-btn, .step:focus-within .comment-btn { opacity: 1; }
  .comment-btn:hover { background: var(--hover); color: var(--text); }
  .comment { display: flex; align-items: flex-start; gap: 8px; margin-top: 8px; padding: 7px 10px; border: 1px solid var(--border-strong); border-radius: 9px; background: var(--surface-2); font-size: 13px; color: var(--text); }
  .you { flex-shrink: 0; padding: 3px 6px; border-radius: 6px; background: var(--accent); color: #fff; font-size: 10.5px; font-weight: 600; line-height: 1; margin-top: 1px; }
  .ct { flex: 1; min-width: 0; }
  .comment button { width: 20px; height: 20px; flex-shrink: 0; border: 0; border-radius: 5px; background: transparent; color: var(--muted); font-size: 15px; line-height: 1; }
  .comment button:hover { background: var(--hover); color: var(--text); }
  .comment-form { display: flex; gap: 6px; margin-top: 8px; }
  .comment-form .input { flex: 1; min-width: 0; min-height: 32px; font-size: 13px; }

  .files { display: flex; flex-wrap: wrap; gap: 5px; }
  .files code { padding: 2px 7px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface-2); font-family: var(--mono); font-size: 11.5px; color: var(--text-2); }

  .changes { margin-top: 14px; display: flex; flex-direction: column; gap: 8px; }
  .changes textarea { min-height: 72px; padding: 10px 12px; resize: vertical; font-size: 13.5px; line-height: 1.5; }
  .changes-row { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .changes-row .muted { flex: 1; }

  /* A decisão fica à mão enquanto você lê um plano longo. */
  .actions { position: sticky; bottom: 8px; z-index: 3; display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 14px -8px 0; padding: 8px; border: 1px solid var(--border); border-radius: 12px; background: rgba(17, 20, 26, 0.82); backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px); }
  .actions .btn { min-height: 36px; }
  .approve { border: 0; color: #fff; font-weight: 600; background: linear-gradient(135deg, var(--accent), var(--accent-2)); box-shadow: 0 6px 18px -8px var(--accent); }
  .approve:hover:not(:disabled) { background: linear-gradient(135deg, var(--accent-hover), var(--accent-2)); filter: brightness(1.06); }
  kbd { margin-left: 6px; padding: 1px 5px; border-radius: 5px; background: rgba(0, 0, 0, 0.2); font-family: var(--mono); font-size: 11px; }
  .ghost { border-color: transparent; background: transparent; color: var(--text-2); }
  .count { display: inline-grid; place-items: center; min-width: 18px; height: 18px; margin-left: 6px; padding: 0 5px; border-radius: 999px; background: var(--note); color: #17122a; font-family: var(--mono); font-size: 11px; font-weight: 600; }

  @keyframes rise { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
  @keyframes glow { 0% { box-shadow: 0 0 0 0 rgba(91, 106, 217, 0); } 30% { box-shadow: 0 0 0 6px rgba(91, 106, 217, 0.18); } 100% { box-shadow: 0 0 0 0 rgba(91, 106, 217, 0); } }
</style>
