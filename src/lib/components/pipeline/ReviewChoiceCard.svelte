<script lang="ts">
  import { renderMarkdown } from "../../format";
  import { choiceFindings, reviewSummary } from "../../pipeline";
  import { isRecommended, optionText } from "../../tools";
  import type { ReviewPick, StageRun } from "../../types";

  /** Parada da sua escolha sobre o code review: o que corrigir e, nos achados com mais de um
   *  caminho, qual seguir. Sem nada marcado, a esteira segue para o Relator. */
  interface Props {
    /** A parada em andamento (a entrada traz os achados). */
    stage: StageRun;
    /** A passagem do Code review que a abriu (o texto completo, para o resumo). */
    review: StageRun | null;
    busy: boolean;
    ondecide: (picks: ReviewPick[], note: string | null) => void;
  }
  let { stage, review, busy, ondecide }: Props = $props();

  const findings = $derived(choiceFindings(stage));
  const summary = $derived(reviewSummary(review?.output ?? ""));

  /** Achados marcados e o caminho escolhido em cada um (índice da opção). */
  let marked = $state<Record<string, boolean>>({});
  let chosen = $state<Record<string, number>>({});
  let note = $state("");
  let summaryOpen = $state(false);

  // Outra parada (ex.: um novo ciclo): começa sem nada marcado.
  $effect(() => {
    void stage.id;
    marked = {};
    chosen = {};
    note = "";
  });

  const picks = $derived(findings.filter((f) => marked[f.id]).map((f) => ({ id: f.id, option: f.options.length ? (chosen[f.id] ?? null) : null })));
  /** Achados marcados que ainda esperam você escolher o caminho. */
  const undecided = $derived(findings.filter((f) => marked[f.id] && f.options.length && chosen[f.id] === undefined));
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
    return v.startsWith("alt") || v.startsWith("high") ? "bad" : v.startsWith("m") ? "warn" : "";
  };
</script>

<section class="card choice" aria-label="Achados do code review para a sua escolha">
  <div class="c-top">
    <div class="c-title">
      <div class="row-w">
        <span class="pill warn"><span class="dot warn pulse"></span>Aguardando sua escolha</span>
        <h2>
          {#if findings.length === 0}O Code review terminou sem a lista de achados
          {:else}O Code review encontrou {findings.length} {findings.length === 1 ? "achado" : "achados"}{decisions ? `, ${decisions} com decisão sua` : ""}{/if}
        </h2>
      </div>
      <p class="note">
        Marque o que corrigir e, nos achados com mais de um caminho, escolha qual seguir. O Implementador corrige só isso, e o
        Revisor e o Testador conferem. A espera não conta no orçamento.
      </p>
    </div>
  </div>

  {#if summary}
    <details class="summary" bind:open={summaryOpen}>
      <summary class="label">Resumo do code review</summary>
      <div class="md">{@html renderMarkdown(summary)}</div>
    </details>
  {/if}

  {#if findings.length}
    <div class="list-head">
      <span class="muted small">{picks.length} de {findings.length} {findings.length === 1 ? "marcado" : "marcados"}</span>
      <button type="button" class="lnk" onclick={toggleAll}>{allMarked ? "Desmarcar todos" : "Marcar todos"}</button>
    </div>
    <ul class="findings">
      {#each findings as f (f.id)}
        <li class="finding" class:on={marked[f.id]}>
          <label class="f-head">
            <input type="checkbox" bind:checked={marked[f.id]} />
            <span class="f-id mono">{f.id}</span>
            <span class="f-title">{f.title}</span>
            {#if f.severity}<span class="pill sev {severityTone(f.severity)}">{f.severity}</span>{/if}
            {#if f.options.length}<span class="pill decide">decisão sua</span>{/if}
          </label>
          {#if f.file}<code class="f-place">{f.file}{f.line ? `:${f.line}` : ""}</code>{/if}
          {#if f.detail}<p class="f-detail">{f.detail}</p>{/if}
          {#if f.options.length}
            <fieldset class="options">
              <legend class="label">Qual caminho seguir</legend>
              {#each f.options as option, i (i)}
                <label class="option">
                  <input type="radio" name={`opt-${stage.id}-${f.id}`} checked={chosen[f.id] === i} onchange={() => choose(f.id, i)} />
                  <span>{optionText(option)}{#if isRecommended(option)}<span class="tag rec">Recomendado</span>{/if}</span>
                </label>
              {/each}
            </fieldset>
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

  <div class="row-w acts">
    {#if undecided.length}<span class="warn-text small">Escolha o caminho de {undecided.map((f) => f.id).join(", ")}.</span>{/if}
    <span class="grow"></span>
    <button type="button" class="btn" disabled={busy} onclick={skip} title="Nada é corrigido: a esteira segue para o Relator">Seguir sem corrigir</button>
    <button type="button" class="btn primary" disabled={busy || undecided.length > 0 || (picks.length === 0 && !note.trim())} onclick={fix}>
      {picks.length ? `Corrigir ${picks.length === 1 ? "o escolhido" : `os ${picks.length} escolhidos`}` : "Enviar ao Implementador"}
    </button>
  </div>
</section>

<style>
  .choice { display: flex; flex-direction: column; gap: 12px; padding: 14px 16px 16px; border-color: rgba(217, 164, 65, 0.45); }
  .c-title h2 { font-family: var(--display); font-size: 16px; font-weight: 600; }
  .note { margin-top: 6px; color: var(--text-2); font-size: 13px; }
  .row-w { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 12px; }
  .grow { flex: 1; }
  .summary { border: 1px solid var(--border); border-radius: var(--radius); padding: 8px 12px; background: var(--surface-2); }
  .summary summary { cursor: pointer; }
  .summary .md { margin-top: 8px; }
  .list-head { display: flex; align-items: center; justify-content: space-between; }
  .findings { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
  .finding { border: 1px solid var(--border); border-radius: var(--radius); padding: 10px 12px; display: flex; flex-direction: column; gap: 6px; background: var(--surface); }
  .finding.on { border-color: var(--accent); background: rgba(91, 106, 217, 0.06); }
  .f-head { display: flex; align-items: center; flex-wrap: wrap; gap: 6px 10px; cursor: pointer; }
  .f-head input { accent-color: var(--accent); width: 15px; height: 15px; margin: 0; }
  .f-id { font-size: 12px; color: var(--muted); }
  .f-title { font-weight: 600; flex: 1 1 240px; min-width: 0; overflow-wrap: anywhere; }
  .pill.sev, .pill.decide { font-size: 11px; }
  .pill.bad { color: var(--danger-text); background: var(--danger-bg); }
  .pill.decide { color: var(--accent-text); background: rgba(91, 106, 217, 0.14); }
  .f-place { align-self: flex-start; padding: 0 6px; border: 1px solid var(--border); border-radius: 5px; background: var(--surface-2); font-family: var(--mono); font-size: 12px; color: var(--text-2); overflow-wrap: anywhere; }
  .f-detail { color: var(--text-2); font-size: 13px; overflow-wrap: anywhere; }
  .options { border: 0; margin: 2px 0 0; padding: 8px 10px; border-radius: 6px; background: var(--surface-2); display: flex; flex-direction: column; gap: 6px; }
  .options legend { float: left; margin-bottom: 4px; }
  .option { display: flex; align-items: flex-start; gap: 8px; cursor: pointer; font-size: 13px; }
  .option input { accent-color: var(--accent); margin: 3px 0 0; }
  .tag.rec { margin-left: 8px; padding: 0 7px; border: 1px solid currentColor; border-radius: 999px; font-size: 11px; color: var(--accent-text); white-space: nowrap; }
  .raw { max-height: 360px; overflow: auto; }
  .extra { display: flex; flex-direction: column; gap: 6px; }
  .acts { justify-content: flex-end; }
  .warn-text { color: var(--warn-text); }
  .lnk { background: none; border: 0; padding: 0; color: var(--accent-text); font: inherit; font-size: 12.5px; cursor: pointer; }
  .lnk:hover { text-decoration: underline; }
</style>
