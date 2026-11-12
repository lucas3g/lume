<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage } from "../../api";
  import { CATEGORY_LABEL, type PluginListing } from "../../types";

  interface Props {
    /** Avisa quando a lista de skills instaladas mudou. */
    onchange: () => void;
  }
  let { onchange }: Props = $props();

  let plugins = $state<PluginListing[]>([]);
  let categories = $state<string[]>([]);
  let query = $state("");
  let category = $state<string | null>(null);
  /** Id do plugin com instalação ou remoção em andamento. */
  let busy = $state<string | null>(null);
  let refreshing = $state(false);
  let loading = $state(true);
  let error = $state<string | null>(null);

  const label = (c: string) => CATEGORY_LABEL[c] ?? c;

  async function load() {
    try {
      [plugins, categories] = await Promise.all([api.storePlugins(query, category), api.storeCategories()]);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  async function pick(next: string | null) {
    category = next;
    await load();
  }

  /** Instala ou remove e recarrega a lista; o erro aparece na tela. */
  async function act(plugin: PluginListing, install: boolean) {
    busy = plugin.id;
    error = null;
    try {
      await (install ? api.storeInstallPlugin(plugin.id) : api.storeUninstallPlugin(plugin.id));
      await load();
      onchange();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = null;
    }
  }

  async function refresh() {
    refreshing = true;
    error = null;
    try {
      await api.storeRefreshCatalog();
      await load();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      refreshing = false;
    }
  }

  onMount(load);
</script>

<div class="store">
  <div class="toolbar">
    <input class="input grow" type="search" aria-label="Buscar na loja de skills" placeholder="Buscar skills e plugins" bind:value={query} oninput={load} />
    <button type="button" class="btn small" disabled={refreshing} onclick={refresh}>{refreshing ? "Atualizando…" : "Atualizar catálogo"}</button>
  </div>

  <div class="chips" role="group" aria-label="Categoria">
    <button type="button" class="chip" class:on={category === null} aria-pressed={category === null} onclick={() => pick(null)}>Todas</button>
    {#each categories as c (c)}
      <button type="button" class="chip" class:on={category === c} aria-pressed={category === c} onclick={() => pick(c)}>{label(c)}</button>
    {/each}
  </div>

  {#if error}<p class="error-text" role="alert">{error}</p>{/if}

  <section class="card list" aria-label="Plugins disponíveis">
    {#each plugins as p (p.id)}
      <div class="plugin">
        <div class="info">
          <div class="row head">
            <strong>{p.display_name}</strong>
            {#if p.author}<span class="muted small">por {p.author}</span>{/if}
            {#if p.category}<span class="pill">{label(p.category)}</span>{/if}
            {#if p.installed}<span class="pill ok">Instalado</span>{/if}
          </div>
          <p class="muted small description">{p.description}</p>
          <div class="muted small meta">
            <span class="mono">{p.name}</span>
            {#if p.skill_count}<span>{p.skill_count} {p.skill_count === 1 ? "skill" : "skills"}</span>{/if}
            <span>{p.marketplace}</span>
            {#if p.homepage}<a href={p.homepage} target="_blank" rel="noreferrer">Site</a>{/if}
          </div>
        </div>
        {#if p.installed}
          <button type="button" class="btn small" disabled={busy !== null} onclick={() => act(p, false)}>{busy === p.id ? "Removendo…" : "Remover"}</button>
        {:else}
          <button type="button" class="btn small primary" disabled={busy !== null} onclick={() => act(p, true)}>{busy === p.id ? "Instalando…" : "Instalar"}</button>
        {/if}
      </div>
    {:else}
      <p class="muted empty">{loading ? "Carregando o catálogo…" : "Nenhum plugin encontrado."}</p>
    {/each}
  </section>
  <p class="muted small">Os plugins vêm dos marketplaces configurados no Claude Code. Instalar é o mesmo que o comando <code>claude plugin install</code>, e as skills ficam disponíveis aqui e no terminal.</p>
</div>

<style>
  .store { display: flex; flex-direction: column; gap: 12px; }
  .toolbar { display: flex; gap: 8px; }
  .grow { flex: 1; min-width: 0; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { min-height: 28px; padding: 0 11px; border: 1px solid var(--border); border-radius: 999px; background: transparent; font-size: 12.5px; color: var(--text-2); }
  .chip:hover, .chip.on { background: var(--hover); color: var(--text); }
  .chip.on { border-color: var(--border-strong); }
  .plugin { display: flex; align-items: flex-start; gap: 14px; padding: 14px 16px; border-bottom: 1px solid var(--hairline); }
  .plugin:last-child { border-bottom: 0; }
  .info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .head { gap: 8px; flex-wrap: wrap; }
  .description { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .meta { display: flex; flex-wrap: wrap; gap: 12px; }
  .plugin .btn { flex-shrink: 0; margin-top: 2px; }
  .empty { padding: 16px; }
  code { font-family: var(--mono); font-size: 12px; }
</style>
