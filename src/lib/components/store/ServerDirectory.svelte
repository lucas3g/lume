<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage } from "../../api";
  import type { ServerListing } from "../../types";

  interface Props {
    /** Avisa quando um conector foi adicionado. */
    onchange: () => void;
  }
  let { onchange }: Props = $props();

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

  const KIND_LABEL = { remote: "Remoto", npm: "npm", pypi: "Python", unsupported: "Sem suporte" } as const;

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

  function describe(s: ServerListing): string {
    switch (s.install.kind) {
      case "remote":
        return s.install.url;
      case "npm":
        return `npx ${s.install.package}${s.install.version ? `@${s.install.version}` : ""}`;
      case "pypi":
        return `uvx ${s.install.package}`;
      default:
        return s.install.reason;
    }
  }

  onMount(() => void search());
</script>

<div class="directory">
  <input class="input" type="search" aria-label="Buscar servidores MCP" placeholder="Buscar no diretório (ex.: github, postgres, slack)" bind:value={query} oninput={onInput} />

  {#if error}<p class="error-text" role="alert">{error}</p>{/if}

  <section class="card list" aria-label="Servidores encontrados">
    {#each servers as s (s.name)}
      <div class="server">
        <div class="line">
          <div class="info">
            <div class="row head">
              <strong>{s.title || s.name.split("/").pop()}</strong>
              <span class="pill" class:warn={s.install.kind === "unsupported"}>{KIND_LABEL[s.install.kind]}</span>
              {#if added === s.name}<span class="pill ok">Adicionado</span>{/if}
            </div>
            {#if s.description}<p class="muted small description">{s.description}</p>{/if}
            <div class="muted small meta">
              <span class="mono">{describe(s)}</span>
              {#if s.website}<a href={s.website} target="_blank" rel="noreferrer">Site</a>{/if}
            </div>
          </div>
          {#if s.install.kind !== "unsupported"}
            <button type="button" class="btn small" class:primary={open !== s.name} aria-expanded={open === s.name} onclick={() => toggle(s)}>
              {open === s.name ? "Cancelar" : "Adicionar"}
            </button>
          {/if}
        </div>

        {#if open === s.name}
          <form class="setup" onsubmit={(e) => add(s, e)}>
            {#if s.inputs.length}
              <p class="small muted">Este servidor pede os valores abaixo. Os marcados são obrigatórios.</p>
              {#each s.inputs as input (input.name)}
                <label>
                  <span>{input.name}{input.required ? " *" : ""}</span>
                  {#if input.description}<span class="muted small">{input.description}</span>{/if}
                  <input class="input" type={input.secret ? "password" : "text"} bind:value={values[input.name]} required={input.required} autocomplete="off" />
                </label>
              {/each}
            {:else}
              <p class="small muted">Este servidor não precisa de configuração.</p>
            {/if}
            <div class="row">
              <button type="submit" class="btn primary" disabled={adding}>{adding ? "Adicionando…" : "Adicionar e ligar"}</button>
            </div>
          </form>
        {/if}
      </div>
    {:else}
      <p class="muted empty">{loading ? "Buscando…" : "Nenhum servidor encontrado."}</p>
    {/each}
    {#if nextCursor}
      <button type="button" class="more" disabled={loading} onclick={() => search(true)}>{loading ? "Carregando…" : "Carregar mais"}</button>
    {/if}
  </section>
  <p class="muted small">Resultados do registro público de servidores MCP. Confira a procedência antes de adicionar: as ferramentas de um conector ligado rodam sem pedir confirmação.</p>
</div>

<style>
  .directory { display: flex; flex-direction: column; gap: 12px; }
  .directory > .input { width: 100%; }
  .server { border-bottom: 1px solid var(--hairline); }
  .server:last-of-type { border-bottom: 0; }
  .line { display: flex; align-items: flex-start; gap: 14px; padding: 14px 16px; }
  .info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .head { gap: 8px; flex-wrap: wrap; }
  .description { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .meta { display: flex; flex-wrap: wrap; gap: 12px; min-width: 0; }
  .meta .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .line .btn { flex-shrink: 0; margin-top: 2px; }
  .setup { margin: 0 16px 14px; padding: 14px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); display: flex; flex-direction: column; gap: 10px; }
  .setup label { display: flex; flex-direction: column; gap: 3px; font-size: 13px; font-weight: 500; }
  .setup .input { font-weight: 400; }
  .more { width: 100%; min-height: 40px; border: 0; border-top: 1px solid var(--hairline); background: transparent; color: var(--accent-text); }
  .more:hover:not(:disabled) { background: var(--hover); }
  .empty { padding: 16px; }
</style>
