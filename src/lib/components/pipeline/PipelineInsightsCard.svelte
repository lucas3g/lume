<script lang="ts">
  import { api, errorMessage } from "../../api";
  import { formatDuration } from "../../format";
  import { STATION, usageHint, usageText } from "../../pipeline";
  import { pipelines } from "../../state/pipelines.svelte";
  import type { PipelineMetrics } from "../../types";

  interface Props {
    projectId: string;
    projectName: string;
  }
  let { projectId, projectName }: Props = $props();

  let metrics = $state<PipelineMetrics | null>(null);
  let saved = $state<string | null>(null);
  let lessons = $state("");
  let drafting = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let status = $state<string | null>(null);

  // Recarrega ao trocar de projeto e quando alguma execução muda.
  $effect(() => {
    const id = projectId;
    void pipelines.list;
    api.pipelineMetrics(id).then((m) => id === projectId && (metrics = m), (e) => (error = errorMessage(e)));
  });
  $effect(() => {
    const id = projectId;
    error = status = null;
    api.pipelineLessons(id).then(
      (text) => {
        if (id !== projectId) return;
        saved = text;
        lessons = text ?? "";
      },
      (e) => (error = errorMessage(e)),
    );
  });

  const finished = $derived(metrics ? metrics.done + metrics.failed + metrics.cancelled + metrics.needs_info : 0);
  const pct = (part: number, whole: number) => (whole ? `${Math.round((part / whole) * 100)}%` : "—");
  const SEVERITY: Record<string, string> = { alta: "alta", "média": "média", media: "média", baixa: "baixa", "": "sem gravidade" };

  async function draft() {
    drafting = true;
    error = status = null;
    try {
      lessons = await api.draftPipelineLessons(projectId);
      status = "Sugestão pronta: revise e salve para os bots passarem a seguir.";
    } catch (e) {
      error = errorMessage(e);
    } finally {
      drafting = false;
    }
  }

  async function save() {
    saving = true;
    error = status = null;
    try {
      await api.savePipelineLessons(projectId, lessons);
      saved = lessons.trim() || null;
      status = saved ? "Salvas no conhecimento do projeto." : "Lições apagadas do projeto.";
    } catch (e) {
      error = errorMessage(e);
    } finally {
      saving = false;
    }
  }
</script>

<section class="card" aria-labelledby="insights-title">
  <div class="head">
    <h2 class="section-title" id="insights-title">Métricas e lições · {projectName}</h2>
    <span class="muted small">O que as execuções deste projeto mostram e o que os bots devem parar de errar.</span>
  </div>

  {#if metrics && metrics.runs}
    <dl class="stats">
      <div><dt>Execuções</dt><dd>{metrics.runs}</dd></div>
      <div title="Concluídas sobre as que terminaram (concluídas, com falha, canceladas ou com a task vaga)"><dt>Concluídas</dt><dd>{pct(metrics.done, finished)}</dd></div>
      <div title="Concluídas sem nenhuma devolução do Revisor nem do Testador"><dt>Aprovadas de primeira</dt><dd>{pct(metrics.first_pass, metrics.done)}</dd></div>
      <div title="Devoluções do Revisor e do Testador por execução terminada"><dt>Devoluções por execução</dt><dd>{finished ? ((metrics.reviewer_returns + metrics.tester_returns) / finished).toLocaleString("pt-BR", { maximumFractionDigits: 1 }) : "—"}</dd></div>
      <div title="Concluídas que você reabriu com um ajuste"><dt>Reabertas</dt><dd>{metrics.reopened}</dd></div>
      {#if metrics.ci_fixes}<div title="Vezes que o CI do pull request falhou e a esteira corrigiu sozinha"><dt>Correções do CI</dt><dd>{metrics.ci_fixes}</dd></div>{/if}
      <div title={usageHint(metrics.usage)}><dt>Gasto</dt><dd class="small-dd">{usageText(metrics.usage)}</dd></div>
    </dl>
    <div class="cols">
      {#if metrics.roles.length}
        <div>
          <h3>Tempo médio por etapa</h3>
          <ul>
            {#each metrics.roles as r (r.role)}
              <li><span>{STATION[r.role].name}</span><span class="muted">{formatDuration(r.avg_seconds * 1000)} · {r.passes} {r.passes === 1 ? "passagem" : "passagens"}</span></li>
            {/each}
          </ul>
        </div>
      {/if}
      <div>
        <h3>Devoluções e achados</h3>
        <ul>
          <li><span>Revisor devolveu</span><span class="muted">{metrics.reviewer_returns}</span></li>
          <li><span>Testador devolveu</span><span class="muted">{metrics.tester_returns}</span></li>
          {#each Object.entries(metrics.findings) as [severity, count] (severity)}
            <li><span>Code review, {SEVERITY[severity] ?? severity}</span><span class="muted">{count}</span></li>
          {/each}
        </ul>
      </div>
    </div>
  {:else if metrics}
    <p class="pad muted small">Nenhuma execução neste projeto ainda.</p>
  {/if}

  <div class="lessons">
    <div class="lhead">
      <h3>Lições para os bots</h3>
      <button type="button" class="btn small" disabled={drafting || !metrics?.runs} onclick={draft}
        title="Lê o que o Revisor e o Testador devolveram, as correções do code review e os seus ajustes, e sugere lições (junta com as já salvas)">
        {#if drafting}<span class="spinner" aria-hidden="true"></span>Gerando…{:else}Gerar do histórico{/if}
      </button>
    </div>
    <textarea class="input" rows={Math.min(12, Math.max(4, lessons.split("\n").length + 1))} bind:value={lessons}
      placeholder="- Rode o pnpm check antes de entregar.&#10;- Textos da interface em português, com acentos."></textarea>
    <div class="lfoot">
      <span class="muted small">Ficam no arquivo <code>licoes-da-esteira.md</code> do conhecimento do projeto: todas as sessões dele, inclusive os bots, passam a ler.</span>
      <button type="button" class="btn primary small" disabled={saving || lessons.trim() === (saved ?? "")} onclick={save}>Salvar no projeto</button>
    </div>
    {#if status}<p class="muted small" role="status">{status}</p>{/if}
    {#if error}<p class="error-text small" role="alert">{error}</p>{/if}
  </div>
</section>

<style>
  .head { padding: 16px 20px 4px; display: flex; flex-direction: column; gap: 4px; }
  .pad { padding: 4px 20px 0; }
  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(130px, 1fr)); gap: 10px; padding: 12px 20px 4px; margin: 0; }
  .stats div { display: flex; flex-direction: column; gap: 2px; }
  dt { font-size: 12px; color: var(--muted); }
  dd { margin: 0; font-family: var(--display); font-size: 20px; font-weight: 600; color: var(--text); }
  dd.small-dd { font-family: var(--font); font-size: 13px; font-weight: 500; }
  .cols { display: grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); gap: 16px; padding: 12px 20px 4px; }
  h3 { font-size: 12.5px; font-weight: 600; color: var(--text-2); margin-bottom: 6px; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; font-size: 13px; }
  li { display: flex; justify-content: space-between; gap: 12px; padding: 4px 0; border-top: 1px solid var(--hairline); }
  li:first-child { border-top: 0; }
  .lessons { padding: 14px 20px 18px; display: flex; flex-direction: column; gap: 8px; border-top: 1px solid var(--hairline); margin-top: 12px; }
  .lhead, .lfoot { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
  .lhead h3 { margin: 0; }
  textarea { width: 100%; resize: vertical; font-size: 13px; }
  .spinner { display: inline-block; width: 11px; height: 11px; margin-right: 6px; vertical-align: -1px; border: 2px solid var(--border-strong); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
