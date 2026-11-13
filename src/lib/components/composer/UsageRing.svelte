<script lang="ts">
  import { dismissable } from "../../actions";
  import { percent } from "../../format";
  import type { Account, View } from "../../types";

  interface Props {
    /** Conta que atende a próxima mensagem; `undefined` quando não há nenhuma. */
    account: Account | undefined;
    onnavigate: (view: View) => void;
  }
  let { account, onnavigate }: Props = $props();

  let open = $state(false);

  const RADIUS = 6;
  const LENGTH = 2 * Math.PI * RADIUS;
  const used = $derived(percent(account?.util_5h ?? null));
  const week = $derived(percent(account?.util_7d ?? null));
  const known = $derived(account !== undefined && account.util_5h !== null);

  const when = new Intl.DateTimeFormat("pt-BR", { weekday: "short", hour: "2-digit", minute: "2-digit" });
  const resets = (seconds: number | null | undefined) => (seconds ? `Reinicia ${when.format(new Date(seconds * 1000))}` : "");

  /** Cor conforme a proximidade do limite, como no Claude Desktop. */
  function tone(value: number): string {
    if (value >= 90) return "var(--danger-text)";
    if (value >= 70) return "var(--warn)";
    return "var(--bar)";
  }

  const summary = $derived(
    !account ? "Nenhuma conta disponível" : !known ? "Uso ainda não consultado" : `${used}% do limite de 5 horas`,
  );
  const plan = $derived(account?.plan ? ` · ${account.plan.charAt(0).toUpperCase()}${account.plan.slice(1)}` : "");
</script>

<span class="wrap" use:dismissable={() => (open = false)}>
  <button type="button" class="ring" class:on={open} aria-haspopup="dialog" aria-expanded={open} aria-label={summary} onclick={() => (open = !open)}>
    <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="8" cy="8" r={RADIUS} fill="none" stroke="var(--border-strong)" stroke-width="2" />
      <circle cx="8" cy="8" r={RADIUS} fill="none" stroke={tone(used)} stroke-width="2" stroke-linecap="round" stroke-dasharray={LENGTH} stroke-dashoffset={LENGTH * (1 - used / 100)} transform="rotate(-90 8 8)" />
    </svg>
  </button>
  <span class="tip" role="tooltip">
    <strong>{summary}</strong>
    {#if known}<span class="muted">{resets(account?.resets_5h)}</span>{/if}
  </span>

  {#if open}
    <div class="pop" role="dialog" aria-label="Limites de uso">
      <div class="head">
        <span class="muted">Seus limites de uso{plan}</span>
        <button type="button" class="all" onclick={() => ((open = false), onnavigate("accounts"))}>Ver tudo →</button>
      </div>
      {#if known}
        <div class="limit">
          <div class="row"><span>Limite de 5 horas</span><span class="muted">{resets(account?.resets_5h)}</span><span class="pct">{used}%</span></div>
          <div class="bar"><div style:width={`${used}%`} style:background={tone(used)}></div></div>
        </div>
        <div class="limit">
          <div class="row"><span>Semanal · todos os modelos</span><span class="muted">{resets(account?.resets_7d)}</span><span class="pct">{week}%</span></div>
          <div class="bar"><div style:width={`${week}%`} style:background={tone(week)}></div></div>
        </div>
      {:else}
        <p class="muted small">O uso aparece após a primeira consulta ou mensagem desta conta.</p>
      {/if}
    </div>
  {/if}
</span>

<style>
  .wrap { position: relative; display: inline-grid; place-items: center; }
  .ring { display: grid; place-items: center; width: 28px; height: 28px; border: 0; border-radius: 7px; background: transparent; }
  .ring:hover, .ring.on { background: var(--hover); }
  .tip { position: absolute; bottom: calc(100% + 8px); right: 0; z-index: 30; display: none; flex-direction: column; gap: 2px; min-width: 190px; padding: 8px 12px; border: 1px solid var(--border-strong); border-radius: 10px; background: var(--surface-2); box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5); font-size: 12.5px; white-space: nowrap; pointer-events: none; }
  .wrap:hover .tip, .ring:focus-visible + .tip { display: flex; }
  .wrap:has(.pop) .tip { display: none; }
  .pop { position: absolute; bottom: calc(100% + 8px); right: 0; z-index: 31; width: 340px; padding: 14px 16px; border: 1px solid var(--border-strong); border-radius: 14px; background: var(--surface-2); box-shadow: 0 16px 40px rgba(0, 0, 0, 0.5); display: flex; flex-direction: column; gap: 14px; font-size: 13px; text-align: left; }
  .head { display: flex; justify-content: space-between; align-items: center; gap: 12px; }
  .all { padding: 0; border: 0; background: transparent; color: var(--text); font-weight: 500; }
  .all:hover { color: var(--accent-text); }
  .limit { display: flex; flex-direction: column; gap: 6px; }
  .row { display: flex; align-items: baseline; gap: 10px; }
  .row > span:first-child { flex: 1; }
  .pct { min-width: 36px; text-align: right; color: var(--text-2); }
  .bar { height: 7px; border-radius: 4px; background: var(--border); overflow: hidden; }
  .bar div { height: 100%; border-radius: 4px; }
</style>
