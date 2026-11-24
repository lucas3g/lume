<script lang="ts">
  import { renderMarkdown } from "../../format";
  import { choiceFindings, reviewPicks, reviewSummary } from "../../pipeline";
  import { isRecommended, optionText } from "../../tools";
  import type { ReviewPick, StageRun } from "../../types";

  /** Parada da sua escolha sobre o code review: o que corrigir e, nos achados com mais de um
   *  caminho, qual seguir. Sem nada marcado, a esteira segue para o Relator. As marcas ficam com
   *  quem abre o cartão, para o cartão Agora decidir com elas também. */
  interface Props {
    /** A parada em andamento (a entrada traz os achados). */
    stage: StageRun;
    /** A passagem do Code review que a abriu (o texto completo, para o resumo). */
    review: StageRun | null;
    busy: boolean;
    /** Achados marcados e o caminho escolhido em cada um (índice da opção). */
    marked: Record<string, boolean>;
    chosen: Record<string, number>;
    note: string;
    ondecide: (picks: ReviewPick[], note: string | null) => void;
  }
  let { stage, review, busy, marked = $bindable(), chosen = $bindable(), note = $bindable(), ondecide }: Props = $props();

  const findings = $derived(choiceFindings(stage));
  const summary = $derived(reviewSummary(review?.output ?? ""));
  let summaryOpen = $state(false);

  const decision = $derived(reviewPicks(findings, marked, chosen));
  const picks = $derived(decision.picks);
  /** Achados marcados que ainda esperam você escolher o caminho. */
  const undecided = $derived(decision.undecided);
  const decisions = $derived(findings.filter((f) => f.options.length).length);
  const allMarked = $derived(findings.length > 0 && findings.every((f) => marked[f.id]));

  function choose(id: string, option: number) {
    chosen[id] = option;
    marked[id] = true;
  }
  function toggleAll() {
    const next = !allMarked;
    marked = Object.fromEntries(findings.map((f) => [f.id, next]));
  }
  const fix = () => ondecide(picks, note.trim() || null);
  const skip = () => ondecide([], null);

  const severityTone = (s: string | null) => {
    const v = (s ?? "").toLowerCase();
    return v.startsWith("alt") || v.startsWith("high") ? "alta" : v.startsWith("m") ? "media" : "baixa";
  };
</script>

<section class="choice" aria-label="Achados do code review para a sua escolha">
  <h3 class="sec-h">
    {#if findings.length === 0}O Code review terminou sem a lista de achados
    {:else}Achados do code review<span class="n">{picks.length} de {findings.length} {findings.length === 1 ? "marcado" : "marcados"}{decisions ? ` · ${decisions} com decisão sua` : ""}</span>{/if}
  </h3>
  <p class="note">
    Marque o que corrigir e, nos achados com mais de um caminho, escolha qual seguir. O Implementador corrige só isso, e o
    Revisor e o Testador conferem.
  </p>

  {#if summary}
    <details class="summary" bind:open={summaryOpen}>
      <summary class="label">Resumo do code review</summary>
      <div class="md">{@html renderMarkdown(summary)}</div>
    </details>
  {/if}

  {#if findings.length}
    <div class="list-head">
      <button type="button" class="lnk" onclick={toggleAll}>{allMarked ? "Desmarcar todos" : "Marcar todos"}</button>
    </div>
    <ul class="finds">
      {#each findings as f (f.id)}
        <li class="find" class:on={marked[f.id]}>
          <label class="f-row">
            <input type="checkbox" bind:checked={marked[f.id]} />
            <span>
              <span class="f-top">
                {#if f.severity}<span class="sev {severityTone(f.severity)}">{f.severity}</span>{/if}
                <span class="f-title">{f.id} · {f.title}</span>
                {#if f.options.length}<span class="decide">decisão sua</span>{/if}
              </span>
              {#if f.file}<span class="f-file">{f.file}{f.line ? `:${f.line}` : ""}</span>{/if}
            </span>
          </label>
          {#if f.detail}<p class="f-detail">{f.detail}</p>{/if}
          {#if f.options.length}
            <div class="opts" role="radiogroup" aria-label="Qual caminho seguir">
              {#each f.options as option, i (i)}
                <label class="opt">
                  <input type="radio" name={`opt-${stage.id}-${f.id}`} checked={chosen[f.id] === i} onchange={() => choose(f.id, i)} />
                  <span>{optionText(option)}</span>{#if isRecommended(option)}<span class="rec">Recomendado</span>{/if}
                </label>
              {/each}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {:else if review?.output}
    <div class="md raw">{@html renderMarkdown(review.output)}</div>
  {/if}

  <div class="extra">
    <label class="label" for={`choice-note-${stage.id}`}>{findings.length ? "Outras instruções (opcional)" : "O que corrigir"}</label>
    <textarea id={`choice-note-${stage.id}`} class="input area" rows="2" bind:value={note}
      placeholder={findings.length ? "Ex.: no A2, mantenha a assinatura da função pública." : "Descreva o que o Implementador deve corrigir, a partir do texto acima."}></textarea>
  </div>

  <div class="acts">
    {#if undecided.length}<span class="warn-text small">Escolha o caminho de {undecided.map((f) => f.id).join(", ")}.</span>{/if}
    <span class="grow"></span>
    <button type="button" class="btn" disabled={busy} onclick={skip} title="Nada é corrigido: a esteira segue para o Relator">Seguir sem corrigir</button>
    <button type="button" class="btn warnp" disabled={busy || undecided.length > 0 || (picks.length === 0 && !note.trim())} onclick={fix}>
      {picks.length ? `Corrigir ${picks.length === 1 ? "o escolhido" : `os ${picks.length} escolhidos`}` : "Enviar ao Implementador"}
    </button>
  </div>
</section>

<style>
  .choice { display: flex; flex-direction: column; gap: 10px; }
  .sec-h { display: flex; align-items: center; flex-wrap: wrap; gap: 4px 8px; margin: 0; font-size: 11px; font-weight: 600; letter-spacing: 0.05em; text-transform: uppercase; color: var(--muted); }
  .sec-h .n { font: 500 10.5px var(--mono); letter-spacing: 0; text-transform: none; color: var(--faint); }
  .note { font-size: 12.5px; color: var(--text-2); }
  .grow { flex: 1; }
  .summary { padding: 8px 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); }
  .summary summary { cursor: pointer; }
  .summary .md { margin-top: 8px; }
  .list-head { display: flex; justify-content: flex-end; margin-bottom: -4px; }
  .finds { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .find { padding: 9px 10px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); transition: border-color 0.25s, background-color 0.25s; }
  .find.on { border-color: var(--acc-line); background: var(--acc-bg); }
  .f-row { display: grid; grid-template-columns: 18px minmax(0, 1fr); gap: 8px; cursor: pointer; }
  .f-row input, .opt input { width: 15px; height: 15px; margin: 2px 0 0; accent-color: var(--accent); }
  .f-top { display: flex; align-items: center; flex-wrap: wrap; gap: 4px 7px; }
  .f-title { font-size: 12.5px; font-weight: 600; color: var(--text); overflow-wrap: anywhere; }
  .f-file { display: block; margin-top: 2px; font: 400 10.5px var(--mono); color: var(--muted); overflow-wrap: anywhere; }
  .f-detail { margin: 6px 0 0 26px; font-size: 12.5px; color: var(--text-2); overflow-wrap: anywhere; }
  .sev { display: inline-flex; align-items: center; height: 18px; padding: 0 7px; border-radius: 999px; font-size: 10.5px; font-weight: 700; }
  .sev.alta { background: var(--danger-bg); color: var(--danger-text); }
  .sev.media { background: var(--warn-bg); color: var(--warn-text); }
  .sev.baixa { background: var(--surface-3); color: var(--text-2); }
  .decide { display: inline-flex; align-items: center; height: 18px; padding: 0 7px; border-radius: 999px; background: var(--acc-bg); color: var(--accent-text); font-size: 10.5px; font-weight: 600; }
  .opts { display: flex; flex-direction: column; gap: 4px; margin: 8px 0 0 26px; }
  .opt { display: flex; align-items: center; flex-wrap: wrap; gap: 7px; font-size: 12px; color: var(--text-2); cursor: pointer; }
  .rec { display: inline-flex; align-items: center; height: 17px; padding: 0 6px; border-radius: 5px; background: var(--ok-bg); color: var(--ok-text); font-size: 10px; font-weight: 700; letter-spacing: 0.02em; }
  .raw { max-height: 360px; overflow: auto; }
  .extra { display: flex; flex-direction: column; gap: 6px; }
  .area { width: 100%; min-height: 60px; padding: 9px 11px; resize: vertical; line-height: 1.5; }
  .acts { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .warn-text { color: var(--warn-text); }
  .btn.warnp { border-color: transparent; background: var(--warn); color: #1b1406; font-weight: 600; }
  .btn.warnp:hover:not(:disabled) { background: var(--warn-btn-hover); }
  .lnk { padding: 0; border: 0; background: none; color: var(--accent-text); font: inherit; font-size: 12px; text-decoration: underline; text-decoration-color: var(--acc-line); text-underline-offset: 3px; cursor: pointer; }
  .md :global(:first-child) { margin-top: 0; }
  .md :global(p), .md :global(ul), .md :global(ol) { margin: 0 0 8px; }
</style>
