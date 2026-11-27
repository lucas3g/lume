<script lang="ts">
  import { slide } from "svelte/transition";

  /** Resumo da conversa para quem volta a ela: o que foi feito, o que falta e o que mudou. */
  interface Props {
    /** Título do último cartão "Entregue", se houver. */
    delivered: string | null;
    steps: number;
    actions: number;
    /** Ações do usuário ainda não marcadas. */
    pending: number;
    files: string[];
    onpending: () => void;
  }
  let { delivered, steps, actions, pending, files, onpending }: Props = $props();

  let open = $state(false);
  const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;
  const summary = $derived(
    [delivered, pending ? (pending === 1 ? "1 pendência sua" : `${pending} pendências suas`) : null, `${steps} etapas`].filter(Boolean).join(" · "),
  );
</script>

<section class="recap" class:open aria-label="Resumo da conversa">
  <div class="card">
    <button type="button" class="head" aria-expanded={open} onclick={() => (open = !open)}>
      <span class="spark" aria-hidden="true">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z" /></svg>
      </span>
      <span class="lbl">Retomar daqui</span>
      <span class="sum">{summary}</span>
      <svg class="chev" aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6" /></svg>
    </button>
    {#if open}
      <div class="grid" transition:slide={{ duration: 280 }}>
        <div>
          <h4>Feito</h4>
          <span class="big">{steps} etapas</span>
          <p>{actions} ações{delivered ? ` · ${delivered}` : ""}</p>
        </div>
        <div>
          <h4>Falta do seu lado</h4>
          <span class="big">{pending ? (pending === 1 ? "1 ação" : `${pending} ações`) : "Nada"}</span>
          {#if pending}<button type="button" class="lnk" onclick={onpending}>Ver o que falta</button>{/if}
        </div>
        <div>
          <h4>Arquivos alterados</h4>
          <span class="big">{files.length}</span>
          <div class="files">
            {#each files.slice(0, 3) as file (file)}<code title={file}>{baseName(file)}</code>{/each}
            {#if files.length > 3}<code>+{files.length - 3}</code>{/if}
          </div>
        </div>
      </div>
    {/if}
  </div>
</section>

<style>
  .recap { position: sticky; top: 0; z-index: 4; margin: -14px -8px 0; }
  .card { border: 1px solid var(--border); border-radius: 12px; background: rgba(17, 20, 26, 0.8); backdrop-filter: blur(14px) saturate(1.3); -webkit-backdrop-filter: blur(14px) saturate(1.3); overflow: hidden; }
  .head { width: 100%; display: flex; align-items: center; gap: 10px; padding: 9px 14px; border: 0; background: transparent; text-align: left; font-size: 13px; color: var(--text); }
  .spark { width: 22px; height: 22px; flex-shrink: 0; display: grid; place-items: center; border-radius: 7px; color: #fff; background: linear-gradient(135deg, var(--accent), var(--accent-2)); }
  .lbl { font-family: var(--display); font-weight: 600; white-space: nowrap; }
  .sum { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--muted); }
  .chev { flex-shrink: 0; color: var(--muted); transition: transform 0.35s var(--ease); }
  .open .chev { transform: rotate(180deg); }
  .grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 1px; border-top: 1px solid var(--border); background: var(--border); }
  .grid > div { padding: 12px 14px; display: flex; flex-direction: column; gap: 5px; background: var(--surface); }
  h4 { margin: 0; font-size: 10.5px; font-weight: 600; letter-spacing: 0.09em; text-transform: uppercase; color: var(--muted); }
  .big { font-family: var(--display); font-size: 19px; font-weight: 600; color: var(--text); font-variant-numeric: tabular-nums; }
  p { font-size: 12.5px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lnk { align-self: flex-start; padding: 0; border: 0; background: none; font-size: 12.5px; color: var(--warn-text); }
  .lnk:hover { text-decoration: underline; }
  .files { display: flex; flex-wrap: wrap; gap: 4px; }
  .files code { max-width: 100%; overflow: hidden; text-overflow: ellipsis; padding: 1px 6px; border: 1px solid var(--border); border-radius: 5px; background: var(--surface-2); font-family: var(--mono); font-size: 11px; color: var(--text-2); }
  @media (max-width: 640px) { .grid { grid-template-columns: 1fr; } }
</style>
