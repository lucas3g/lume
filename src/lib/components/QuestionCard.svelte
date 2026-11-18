<script lang="ts">
  import type { Question } from "../types";
  import { isRecommended, withoutRecommended } from "../tools";

  interface Props {
    questions: Question[];
    onanswer: (answers: [string, string][]) => void;
    oncancel: () => void;
    title?: string;
    submitLabel?: string;
    cancelLabel?: string;
    /** Deixa enviar com perguntas em branco (a resposta fica vazia para completar depois). */
    allowPartial?: boolean;
  }
  let {
    questions, onanswer, oncancel,
    title = "O agente precisa da sua resposta", submitLabel = "Responder", cancelLabel = "Não responder", allowPartial = false,
  }: Props = $props();


  /** Opções marcadas por pergunta, e texto livre ("Outro") por pergunta. */
  let chosen = $state<Record<string, string[]>>({});
  let other = $state<Record<string, string>>({});

  function toggle(q: Question, label: string) {
    const current = chosen[q.question] ?? [];
    if (q.multiSelect) {
      chosen[q.question] = current.includes(label) ? current.filter((l) => l !== label) : [...current, label];
    } else {
      chosen[q.question] = [label];
    }
  }

  function answerOf(q: Question): string {
    const picked = chosen[q.question] ?? [];
    const free = other[q.question]?.trim();
    return [...picked, ...(free ? [free] : [])].join(", ");
  }

  const complete = $derived(
    questions.length > 0 && (allowPartial ? questions.some((q) => answerOf(q) !== "") : questions.every((q) => answerOf(q) !== "")),
  );

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!complete) return;
    onanswer(questions.map((q) => [q.question, answerOf(q)]));
  }
</script>

<form class="card" onsubmit={submit} aria-label={title}>
  <strong>{title}</strong>
  {#each questions as q (q.question)}
    <fieldset>
      <legend>
        {#if q.header}<span class="tag">{q.header}</span>{/if}
        {q.question}
        {#if q.multiSelect}<span class="muted small">(pode marcar mais de uma)</span>{/if}
      </legend>
      {#if q.options.length}<div class="options">
        {#each q.options as o (o.label)}
          <label class="option" class:on={(chosen[q.question] ?? []).includes(o.label)}>
            <input type={q.multiSelect ? "checkbox" : "radio"} name={q.question} value={o.label} checked={(chosen[q.question] ?? []).includes(o.label)} onchange={() => toggle(q, o.label)} />
            <span>
              <span class="label">{withoutRecommended(o.label)}{#if isRecommended(o.label)}<span class="tag rec">Recomendado</span>{/if}</span>
              {#if o.description}<span class="muted small">{o.description}</span>{/if}
            </span>
          </label>
        {/each}
      </div>{/if}
      <input class="input" placeholder={q.options.length ? "Outra resposta (opcional)" : "Sua resposta"} bind:value={other[q.question]} aria-label={`Outra resposta para: ${q.question}`} />
    </fieldset>
  {/each}
  <div class="actions">
    <button type="submit" class="btn primary" disabled={!complete}>{submitLabel}</button>
    <button type="button" class="btn" onclick={oncancel}>{cancelLabel}</button>
  </div>
</form>

<style>
  .card { padding: 16px; border-color: var(--border-strong); display: flex; flex-direction: column; gap: 14px; }
  fieldset { margin: 0; padding: 0; border: 0; display: flex; flex-direction: column; gap: 8px; }
  legend { margin-bottom: 8px; font-weight: 500; display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .tag { padding: 0 7px; border: 1px solid var(--border-strong); border-radius: 999px; font-size: 11px; color: var(--muted); }
  .options { display: flex; flex-direction: column; gap: 4px; }
  .option { display: flex; align-items: flex-start; gap: 10px; padding: 8px 10px; border: 1px solid var(--border); border-radius: 8px; cursor: pointer; }
  .option:hover, .option.on { background: var(--hover); border-color: var(--border-strong); }
  .option input { margin-top: 3px; }
  .option > span { display: flex; flex-direction: column; gap: 2px; }
  .label { font-weight: 500; display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .tag.rec { color: var(--accent, var(--muted)); border-color: currentColor; font-weight: 400; }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; }
  .actions .btn { min-height: 40px; }
</style>
