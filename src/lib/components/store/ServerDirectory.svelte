<script lang="ts">
  import { api, errorMessage } from "../../api";
  import type { ServerListing } from "../../types";
  import ConnectorIcon from "../ConnectorIcon.svelte";

  interface Props {
    /** Diálogo aberto por cima do catálogo ("Não achou?"). */
    open: boolean;
    /** O que o usuário já tinha digitado no catálogo. */
    initialQuery: string;
    onclose: () => void;
    /** Avisa quando um conector foi adicionado. */
    onchange: () => void;
  }
  let { open: isOpen, initialQuery, onclose, onchange }: Props = $props();

  let dialog = $state<HTMLDialogElement>();

  // Ao abrir, repete a busca com o texto do catálogo; o elemento nativo cuida do foco e do Esc.
  $effect(() => {
    if (isOpen && !dialog?.open) {
      dialog?.showModal();
      query = initialQuery;
      void search();
    }
    if (!isOpen && dialog?.open) dialog.close();
  });

  function onBackdrop(event: MouseEvent) {
    if (event.target === dialog) onclose();
  }

  let servers = $state<ServerListing[]>([]);
  let query = $state("");
  let nextCursor = $state<string | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  /** Servidor com o formulário de valores aberto, e os valores digitados. */
  let open = $state<string | null>(null);
  let values = $state<Record<string, string>>({});
  let adding = $state(false);
  let added = $state<string | null>(null);

  const KIND_LABEL = { remote: "Conexão remota", npm: "Roda localmente (npm)", pypi: "Roda localmente (Python)", unsupported: "Sem suporte" } as const;

  /** Nova busca: descarta os resultados anteriores; `more` acrescenta a próxima página. */
  async function search(more = false) {
    loading = true;
    error = null;
    try {
      const page = await api.storeServers(query, more ? nextCursor : null);
      servers = more ? [...servers, ...page.servers] : page.servers;
      nextCursor = page.next_cursor;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  function onInput() {
    clearTimeout(timer);
    timer = setTimeout(() => void search(), 350);
  }

  function toggle(s: ServerListing) {
    open = open === s.name ? null : s.name;
    values = {};
    added = null;
  }

  async function add(s: ServerListing, event: SubmitEvent) {
    event.preventDefault();
    adding = true;
    error = null;
    try {
      await api.storeAddServer(s, values);
      open = null;
      added = s.name;
      onchange();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      adding = false;
    }
  }

  const title = (s: ServerListing) => s.title || s.name.split("/").pop() || s.name;

  /** Quem publicou: o domínio do site ou o dono no identificador (`io.github.dono/x`). */
  function author(s: ServerListing): string {
    const host = s.website?.match(/^https?:\/\/([^/]+)/)?.[1];
    if (host && !host.includes("github.com")) return host.replace(/^www\./, "");
    const owner = s.name.split("/")[0]?.split(".").pop();
    return owner ?? "";
  }

  /** Endereço de onde vem o ícone: o site do serviço, ou o próprio servidor remoto. */
  function iconEndpoint(s: ServerListing): string {
    if (s.website) return s.website;
    return s.install.kind === "remote" ? s.install.url : "";
  }

</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={dialog} aria-labelledby="community-title" onclose={onclose} onclick={onBackdrop}>
<div class="directory">
  <header class="section-head">
    <div>
      <h2 id="community-title">Outro serviço</h2>
      <p class="muted small">Registro público de servidores MCP. Qualquer serviço que ofereça conexão para agentes entra pelo endereço dele.</p>
    </div>
    <button type="button" class="btn icon" aria-label="Fechar" title="Fechar" onclick={onclose}>
      <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
    </button>
  </header>

  <input class="input" type="search" aria-label="Procurar conector" placeholder="Procurar conector (ex.: GitHub, Postgres, Slack)" bind:value={query} oninput={onInput} />

  {#if error}<p class="error-text" role="alert">{error}</p>{/if}

  <div class="grid" role="list">
    {#each servers as s (s.name)}
      <article class="card item" class:wide={open === s.name} role="listitem">
        <div class="body">
          <ConnectorIcon endpoint={iconEndpoint(s)} name={title(s)} size={40} />
          <div class="text">
            <div class="row name">
              <strong>{title(s)}</strong>
              {#if s.install.kind !== "remote" && s.install.kind !== "unsupported"}<span class="tag">Local</span>{/if}
              {#if added === s.name}<span class="pill ok">Adicionado</span>{/if}
            </div>
            <p class="muted small description" title={s.description}>{s.description || KIND_LABEL[s.install.kind]}</p>
            <span class="muted small">por {author(s) || "autor não informado"}</span>
          </div>
          {#if s.install.kind === "unsupported"}
            <span class="tag" title={s.install.reason}>Sem suporte</span>
          {:else}
            <button type="button" class="plus" aria-label={open === s.name ? `Cancelar ${title(s)}` : `Adicionar ${title(s)}`} title={open === s.name ? `Cancelar ${title(s)}` : `Adicionar ${title(s)}`} aria-expanded={open === s.name} onclick={() => toggle(s)}>
              {#if open === s.name}
                <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
              {:else}
                <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
              {/if}
            </button>
          {/if}
        </div>

        {#if open === s.name}
          <form class="setup" onsubmit={(e) => add(s, e)}>
            <p class="small muted">{KIND_LABEL[s.install.kind]}{s.inputs.length ? ". Preencha o que o serviço pede; os marcados são obrigatórios." : ". Não precisa de configuração."}</p>
            {#each s.inputs as input (input.name)}
              <label>
                <span>{input.name}{input.required ? " *" : ""}</span>
                {#if input.description}<span class="muted small">{input.description}</span>{/if}
                <input class="input" type={input.secret ? "password" : "text"} bind:value={values[input.name]} required={input.required} autocomplete="off" />
              </label>
            {/each}
            <div class="row">
              <button type="submit" class="btn primary" disabled={adding}>{adding ? "Adicionando…" : "Adicionar e ligar"}</button>
            </div>
          </form>
        {/if}
      </article>
    {:else}
      <p class="muted empty">{loading ? "Procurando…" : "Nenhum conector encontrado."}</p>
    {/each}
  </div>

  {#if nextCursor}
    <button type="button" class="btn more" disabled={loading} onclick={() => search(true)}>{loading ? "Carregando…" : "Mostrar mais"}</button>
  {/if}
  <p class="muted small">Confira a procedência antes de adicionar: as ferramentas de um conector pedem sua aprovação na conversa, mas o servidor recebe o que o assistente enviar a ele.</p>
</div>
</dialog>

<style>
  dialog { width: min(820px, calc(100vw - 32px)); max-height: calc(100vh - 48px); padding: 0; border: 1px solid var(--border-strong); border-radius: 18px; background: var(--surface); color: var(--text); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6); }
  dialog::backdrop { background: rgba(0, 0, 0, 0.55); }
  .directory { display: flex; flex-direction: column; gap: 14px; padding: 20px; }
  .directory > .input { width: 100%; }
  .section-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  .section-head h2 { font-family: var(--display); font-size: 19px; font-weight: 600; letter-spacing: -0.01em; margin-bottom: 4px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(340px, 100%), 1fr)); gap: 12px; }
  .item { display: flex; flex-direction: column; }
  .item.wide { grid-column: 1 / -1; }
  .body { display: flex; align-items: flex-start; gap: 12px; padding: 14px; }
  .text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .name { gap: 8px; flex-wrap: wrap; }
  .description { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .tag { padding: 0 7px; border: 1px solid var(--border-strong); border-radius: 999px; font-size: 11px; color: var(--muted); white-space: nowrap; }
  .plus { width: 30px; height: 30px; flex-shrink: 0; display: grid; place-items: center; border: 1px solid var(--border-strong); border-radius: 8px; background: var(--surface-2); color: var(--text); }
  .plus:hover { background: var(--hover); }
  .setup { margin: 0 14px 14px; padding: 14px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); display: flex; flex-direction: column; gap: 10px; }
  .setup label { display: flex; flex-direction: column; gap: 3px; font-size: 13px; font-weight: 500; }
  .setup .input { font-weight: 400; }
  .more { align-self: center; }
  .empty { grid-column: 1 / -1; padding: 16px; }
</style>
