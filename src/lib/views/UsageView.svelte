<script lang="ts">
  import { api, errorMessage } from "../api";
  import { formatReset } from "../format";
  import { costText, tokenCount, totalTokens, usageHint, usageText } from "../pipeline";
  import { accounts } from "../state/accounts.svelte";
  import { projectName } from "../state/projects.svelte";
  import type { TokenUsage, UsageGroup, UsageReport } from "../types";

  const PERIODS = [7, 30, 90] as const;
  const SOURCE_LABEL: Record<string, string> = { chat: "Conversas", pipeline: "Esteira de bots", task: "Tarefas agendadas" };

  let days = $state<(typeof PERIODS)[number]>(7);
  let report = $state<UsageReport | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    const period = days;
    error = null;
    api.usageReport(period).then(
      (r) => {
        if (period === days) report = r;
      },
      (e) => (error = errorMessage(e)),
    );
  });

  const dayFormat = new Intl.DateTimeFormat("pt-BR", { weekday: "short", day: "2-digit", month: "2-digit" });
  const shortDay = new Intl.DateTimeFormat("pt-BR", { day: "2-digit", month: "2-digit" });
  const dayLabel = (start: number) => dayFormat.format(new Date(start * 1000));

  const peak = $derived(Math.max(1, ...(report?.days ?? []).map((d) => totalTokens(d.usage))));
  /** Rótulos do eixo: o primeiro, o do meio e o último dia, para não amontoar em 90 dias. */
  const ticks = $derived.by(() => {
    const list = report?.days ?? [];
    return new Set([0, Math.floor((list.length - 1) / 2), list.length - 1]);
  });

  const accountName = (id: string) => (id ? (accounts.list.find((a) => a.id === id)?.name ?? "Conta removida") : "Antes do registro por conta");
  const groupLabel = {
    source: (g: UsageGroup) => SOURCE_LABEL[g.key] ?? g.key,
    project: (g: UsageGroup) => (g.key ? (projectName(g.key) ?? "Projeto removido") : "Sem projeto"),
    account: (g: UsageGroup) => accountName(g.key),
  };
  const share = (u: TokenUsage) => (report && totalTokens(report.total) ? totalTokens(u) / totalTokens(report.total) : 0);

  /** Contas que esgotam antes de a janela renovar, no ritmo de agora. */
  const warnings = $derived(
    (report?.paces ?? []).flatMap((p) => [
      ...(p.session_out_at ? [{ id: p.id, window: "a sessão de 5 h", at: p.session_out_at }] : []),
      ...(p.week_out_at ? [{ id: p.id, window: "a semana", at: p.week_out_at }] : []),
    ]),
  );
</script>

<div class="page">
  <div class="page-inner">
    <header class="page-head">
      <div>
        <h1>Uso e custo</h1>
        <p>Tokens gastos pelas conversas, pela esteira e pelas tarefas, com o custo equivalente na API (com assinatura não há cobrança; serve para comparar).</p>
      </div>
      <div class="seg" role="group" aria-label="Período">
        {#each PERIODS as p (p)}
          <button type="button" aria-pressed={days === p} onclick={() => (days = p)}>{p} dias</button>
        {/each}
      </div>
    </header>

    {#if error}<p class="error-text" role="alert">Não deu para carregar o uso: {error}</p>{/if}

    {#if report}
      <section class="stats" aria-label="Resumo do período">
        <div class="card stat" title={usageHint(report.total)}>
          <span class="muted small">Tokens</span>
          <strong>{tokenCount(totalTokens(report.total))}</strong>
        </div>
        <div class="card stat">
          <span class="muted small">Custo equivalente na API</span>
          <strong>{costText(report.total) ?? "—"}</strong>
        </div>
        <div class="card stat">
          <span class="muted small">Trocas de conta</span>
          <strong>{report.switches}</strong>
        </div>
      </section>

      {#if warnings.length}
        <section class="card panel" aria-labelledby="pace-title">
          <h2 class="section-title" id="pace-title">No ritmo de agora</h2>
          <ul class="pace">
            {#each warnings as w (w.id + w.window)}
              <li><b>{accountName(w.id)}</b> esgota {w.window} {formatReset(w.at)}, antes de o limite renovar.</li>
            {/each}
          </ul>
        </section>
      {/if}

      <section class="card panel" aria-labelledby="days-title">
        <h2 class="section-title" id="days-title">Tokens por dia</h2>
        <div class="chart" role="list" aria-label="Tokens por dia">
          {#each report.days as d, i (d.start)}
            {@const value = totalTokens(d.usage)}
            <div class="col" role="listitem" aria-label={`${dayLabel(d.start)}: ${usageText(d.usage)}`}>
              <div class="bar" style:height={`${(value / peak) * 100}%`} class:empty={!value}></div>
              <div class="tip" role="tooltip"><b>{dayLabel(d.start)}</b><span>{usageText(d.usage)}</span></div>
              {#if ticks.has(i)}<span class="tick">{shortDay.format(new Date(d.start * 1000))}</span>{/if}
            </div>
          {/each}
        </div>
      </section>

      {#each [{ id: "source", title: "Por origem", list: report.by_source }, { id: "project", title: "Por projeto", list: report.by_project }, { id: "account", title: "Por conta", list: report.by_account }] as const as group (group.id)}
        <section class="card panel" aria-labelledby={`g-${group.id}`}>
          <h2 class="section-title" id={`g-${group.id}`}>{group.title}</h2>
          {#if group.list.length}
            <table>
              <tbody>
                {#each group.list as g (g.key)}
                  <tr title={usageHint(g.usage)}>
                    <th scope="row">{groupLabel[group.id](g)}</th>
                    <td class="meter"><span style:width={`${share(g.usage) * 100}%`}></span></td>
                    <td class="num">{tokenCount(totalTokens(g.usage))}</td>
                    <td class="num muted">{costText(g.usage) ?? "—"}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {:else}
            <p class="muted small">Nada gasto no período.</p>
          {/if}
        </section>
      {/each}
    {/if}
  </div>
</div>

<style>
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: 6px; overflow: hidden; }
  .seg button { border: 0; border-left: 1px solid var(--border); background: transparent; color: var(--muted); min-height: 28px; padding: 0 10px; font-size: 12.5px; cursor: pointer; }
  .seg button:first-child { border-left: 0; }
  .seg button[aria-pressed="true"] { background: var(--surface-2); color: var(--text); }
  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; }
  .stat { padding: 14px 16px; display: flex; flex-direction: column; gap: 4px; }
  .stat strong { font-family: var(--display); font-size: 24px; font-weight: 600; color: var(--text); }
  .panel { padding: 16px 20px; display: flex; flex-direction: column; gap: 12px; }
  .pace { margin: 0; padding-left: 18px; color: var(--warn-text); font-size: 13px; display: flex; flex-direction: column; gap: 4px; }
  .pace b { color: var(--text); font-weight: 500; }
  .chart { display: flex; align-items: flex-end; gap: 2px; height: 160px; padding-bottom: 18px; border-bottom: 1px solid var(--hairline); }
  .col { position: relative; flex: 1; min-width: 0; height: 100%; display: flex; align-items: flex-end; justify-content: center; }
  .bar { width: 100%; max-width: 28px; min-height: 2px; background: var(--bar); border-radius: 4px 4px 0 0; }
  .bar.empty { background: var(--border); }
  .col:hover .bar { background: var(--accent-text); }
  .tip { display: none; position: absolute; bottom: calc(100% + 6px); left: 50%; transform: translateX(-50%); z-index: 2; padding: 6px 9px; border: 1px solid var(--border-strong); border-radius: 6px; background: var(--surface-2); color: var(--text-body); font-size: 12px; white-space: nowrap; flex-direction: column; gap: 2px; pointer-events: none; }
  .col:hover .tip { display: flex; }
  .col:first-child .tip { left: 0; transform: none; }
  .col:last-child .tip { left: auto; right: 0; transform: none; }
  .tick { position: absolute; top: calc(100% + 4px); font-size: 11px; color: var(--muted); white-space: nowrap; }
  .col:first-child .tick { left: 0; }
  .col:last-child .tick { right: 0; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  th { text-align: left; font-weight: 400; color: var(--text-body); padding: 6px 12px 6px 0; width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 0; }
  td { padding: 6px 0 6px 12px; }
  tr + tr th, tr + tr td { border-top: 1px solid var(--hairline); }
  .meter span { display: block; height: 6px; min-width: 2px; border-radius: 3px; background: var(--bar); }
  .num { text-align: right; white-space: nowrap; font-variant-numeric: tabular-nums; width: 1%; }
</style>
