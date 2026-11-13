<script lang="ts">
  import type { CatalogEntry } from "../../catalog";
  import type { AccountConnector } from "../../types";
  import ConnectorIcon from "../ConnectorIcon.svelte";
  import LoginCodeField from "../LoginCodeField.svelte";
  import { logins } from "../../state/logins.svelte";
  import BrandSky from "./BrandSky.svelte";

  interface Props {
    connector: AccountConnector | null;
    entry: CatalogEntry | undefined;
    busy: boolean;
    onclose: () => void;
    onlink: (connector: AccountConnector, connect: boolean) => void;
  }
  let { connector, entry, busy, onclose, onlink }: Props = $props();

  let dialog = $state<HTMLDialogElement>();

  $effect(() => {
    if (connector && !dialog?.open) dialog?.showModal();
    if (!connector && dialog?.open) dialog.close();
  });

  function onBackdrop(event: MouseEvent) {
    if (event.target === dialog) onclose();
  }

  const connected = $derived(connector?.status === "connected");
  const lines = $derived(entry ? [...entry.can.map((text) => ({ text, asks: false })), ...(entry.asks ? [{ text: entry.asks, asks: true }] : [])] : []);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={dialog} aria-labelledby="service-title" onclose={onclose} onclick={onBackdrop}>
  {#if connector}
    <div class="content">
      <BrandSky color={entry?.color ?? null} height={112} />
      <button type="button" class="btn icon close" aria-label="Fechar detalhe" title="Fechar detalhe" onclick={onclose}>
        <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
      </button>

      <header class="head">
        <span class="lead" class:connected>
          <ConnectorIcon name={connector.display_name} endpoint={connector.endpoint} size={64} />
          {#if connected}<span class="check" aria-hidden="true"><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg></span>{/if}
        </span>
        <h2 id="service-title">{connector.display_name}</h2>
        <p class="desc">{entry?.description ?? "Serviço conectado à sua conta claude.ai."}</p>
      </header>

      <div class="body">
        {#if lines.length}
          <section class="part">
            <h3>O que o assistente poderá fazer</h3>
            <ul>
              {#each lines as line (line.text)}
                <li class:asks={line.asks}>
                  {#if line.asks}
                    <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M12 3l7 3v5c0 5-3.5 8.5-7 10-3.5-1.5-7-5-7-10V6z" /></svg>
                  {:else}
                    <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>
                  {/if}
                  <span>{line.text}</span>
                </li>
              {/each}
            </ul>
          </section>
        {/if}
        <p class="after muted">
          {connected ? "Vale nas conversas atendidas por esta conta. As ferramentas pedem sua aprovação na conversa." : "Ao vincular, vale nas conversas atendidas por esta conta. Dá para desligar depois em Meus."}
        </p>
      </div>

      <footer class="act">
        <span class="how" class:ok={connected}>
          {#if connected}
            <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg>
            <span>Já vinculado</span>
          {:else}
            <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M13 2L4 14h7l-1 8 9-12h-7z" /></svg>
            <span>Um clique: você autoriza na página do serviço</span>
          {/if}
        </span>
        {#if connected}
          <button type="button" class="btn" disabled={busy} onclick={() => onlink(connector!, false)}>Desvincular</button>
        {:else}
          <button type="button" class="btn primary" disabled={busy} onclick={() => onlink(connector!, true)}>{busy ? "Aguardando o login…" : `Vincular ${connector.display_name}`}</button>
        {/if}
      </footer>
      <!-- A janela é modal: o campo do código precisa estar nela, não na página atrás. -->
      {#if logins.connector === connector.name}
        <div class="code"><LoginCodeField scope="connector" /></div>
      {/if}
    </div>
  {/if}
</dialog>

<style>
  dialog { width: min(480px, calc(100vw - 32px)); max-height: calc(100vh - 48px); padding: 0; border: 1px solid var(--border-strong); border-radius: 18px; background: var(--surface); color: var(--text); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6); overflow: hidden; }
  dialog::backdrop { background: rgba(0, 0, 0, 0.55); }
  .content { position: relative; display: flex; flex-direction: column; max-height: calc(100vh - 50px); }
  .code { padding: 0 24px 16px; }
  .close { position: absolute; top: 14px; right: 14px; background: var(--surface); border-color: var(--border); }
  .head { display: flex; flex-direction: column; gap: 6px; margin-top: -36px; padding: 0 24px 4px; }
  .lead { position: relative; align-self: flex-start; border-radius: 19px; background: var(--surface); box-shadow: 0 0 0 4px var(--surface); }
  .check { position: absolute; right: -5px; bottom: -5px; width: 20px; height: 20px; display: grid; place-items: center; border-radius: 7px; background: var(--ok); color: var(--surface); box-shadow: 0 0 0 2px var(--surface); }
  h2 { margin-top: 8px; font-family: var(--display); font-size: 24px; font-weight: 600; letter-spacing: -0.02em; }
  .desc { color: var(--text-2); }
  .body { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 14px; padding: 16px 24px 20px; }
  .part { padding: 14px 16px; border-radius: 14px 14px 14px 4px; background: var(--bg-side); border: 1px solid var(--border); }
  h3 { margin: 0 0 8px; font-size: 12.5px; font-weight: 650; color: var(--muted); }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
  li { display: flex; align-items: center; gap: 8px; font-size: 14px; }
  li svg { flex-shrink: 0; color: var(--ok); }
  li.asks svg { color: var(--warn); }
  .after { font-size: 13px; line-height: 1.5; }
  .act { display: flex; align-items: center; justify-content: flex-end; gap: 16px; padding: 14px 24px 20px; border-top: 1px solid var(--border); }
  .how { flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px; font-size: 12.5px; color: var(--muted); }
  .how.ok svg { color: var(--ok); }
</style>
