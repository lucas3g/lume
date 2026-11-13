<script lang="ts">
  import { CATALOG, CATEGORY_LABEL, CATEGORY_ORDER, SUGGESTED, catalogOf, describe, isListed, matchesQuery, type CatalogEntry, type Category } from "../../catalog";
  import type { AccountConnector } from "../../types";
  import ConnectorIcon from "../ConnectorIcon.svelte";
  import BrandSky from "./BrandSky.svelte";
  import ServerDirectory from "./ServerDirectory.svelte";
  import ServiceDialog from "./ServiceDialog.svelte";

  interface Props {
    /** Conectores que o Claude Code carrega para a conta ativa. */
    account: AccountConnector[];
    /** O catálogo da conta ainda está sendo verificado. */
    loading: boolean;
    /** Conector da conta com uma operação em andamento. */
    busy: string | null;
    onlink: (connector: AccountConnector, connect: boolean) => void;
    /** Avisa quando um servidor da comunidade foi adicionado. */
    onchange: () => void;
  }
  let { account, loading, busy, onlink, onchange }: Props = $props();

  type Filter = "all" | "connected" | Category;

  interface Entry {
    key: string;
    connector: AccountConnector;
    entry: CatalogEntry | undefined;
    name: string;
    description: string;
    category: Category | null;
    connected: boolean;
  }

  let query = $state("");
  let filter = $state<Filter>("all");
  /** Conector aberto por cima do catálogo. */
  let selected = $state<string | null>(null);
  /** "Não achou?": a busca na comunidade, com o que foi digitado. */
  let community = $state(false);

  const entries = $derived<Entry[]>(
    account
      .filter(isListed)
      .map((c) => ({ key: c.name, connector: c, entry: catalogOf(c), name: c.display_name, description: describe(c), category: catalogOf(c)?.category ?? null, connected: c.status === "connected" }))
      .sort((a, b) => a.name.localeCompare(b.name, "pt-BR")),
  );

  const present = $derived(CATEGORY_ORDER.filter((c) => entries.some((e) => e.category === c)));
  const filters = $derived<Filter[]>(["all", "connected", ...present]);

  function count(f: Filter): number {
    if (f === "all") return entries.length;
    if (f === "connected") return entries.filter((e) => e.connected).length;
    return entries.filter((e) => e.category === f).length;
  }
  const label = (f: Filter) => (f === "all" ? "Todos" : f === "connected" ? "Vinculados" : CATEGORY_LABEL[f]);

  const needle = $derived(query.trim());
  const inFilter = (e: Entry) => filter === "all" || (filter === "connected" ? e.connected : e.category === filter);
  const showSuggestions = $derived(filter === "all" && !needle);
  const suggested = $derived(
    showSuggestions
      ? SUGGESTED.map((k) => entries.find((e) => e.name.toLowerCase() === k)).filter((e): e is Entry => !!e && !e.connected).slice(0, 3)
      : [],
  );
  const shown = $derived(entries.filter((e) => inFilter(e) && matchesQuery(e.connector, needle) && !suggested.includes(e)));
  const current = $derived(entries.find((e) => e.key === selected) ?? null);

  function openCommunity() {
    community = true;
  }
</script>

<div class="catalog">
  <div class="tools">
    <input class="input search" type="search" aria-label="Buscar conector" placeholder="Buscar por nome ou o que faz" bind:value={query} />
    <div class="chips" role="tablist" aria-label="Categorias">
      {#each filters as f (f)}
        <button type="button" role="tab" class="chip" aria-selected={filter === f} class:on={filter === f} onclick={() => (filter = f)}>{label(f)} <span class="n">{count(f)}</span></button>
      {/each}
    </div>
  </div>

  <div class="body">
    <nav class="cats" aria-label="Categorias">
      {#each filters as f (f)}
        <button type="button" class="cat" class:rule={f === "connected"} aria-pressed={filter === f} onclick={() => (filter = f)}>
          <span class="cat-name">{label(f)}</span>
          <span class="cat-count">{count(f)}</span>
        </button>
      {/each}
    </nav>

    <div class="list" role="region" aria-label="Conectores disponíveis">
      {#if loading && !entries.length}
        <ul class="grid" aria-busy="true">
          {#each [1, 2, 3, 4, 5, 6] as i (i)}<li class="tile skeleton"></li>{/each}
        </ul>
      {:else if !shown.length && (needle || filter !== "all")}
        <div class="empty">
          <svg aria-hidden="true" width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><circle cx="11" cy="11" r="6.5" /><path d="M16 16l4.5 4.5" /></svg>
          <strong>{needle ? `Nenhum conector com “${needle}”` : "Nenhum conector nesta categoria ainda"}</strong>
          <p class="muted">Dá para procurar na comunidade ou conectar qualquer serviço pelo endereço dele.</p>
          <button type="button" class="btn" onclick={openCommunity}>Procurar na comunidade</button>
        </div>
      {:else}
        {#if suggested.length}
          <section class="section" aria-labelledby="catalog-suggested">
            <h3 id="catalog-suggested" class="section-title">Sugeridos para você</h3>
            <ul class="feat">
              {#each suggested as e (e.key)}
                <li>
                  <button type="button" class="feat-card" aria-label={e.name} aria-haspopup="dialog" onclick={() => (selected = e.key)}>
                    <BrandSky color={e.entry?.color ?? null} />
                    <span class="feat-icon"><ConnectorIcon name={e.name} endpoint={e.connector.endpoint} size={44} /></span>
                    <span class="name">{e.name}</span>
                    <span class="desc">{e.description}</span>
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        <section class="section" aria-labelledby={suggested.length ? "catalog-more" : undefined}>
          {#if suggested.length}<h3 id="catalog-more" class="section-title">Mais serviços</h3>{/if}
          <ul class="grid">
            {#each shown as e (e.key)}
              <li>
                <button type="button" class="tile" aria-haspopup="dialog" aria-label={e.connected ? `${e.name}, vinculado` : e.name} onclick={() => (selected = e.key)}>
                  <span class="tile-head">
                    <span class="tile-icon">
                      <ConnectorIcon name={e.name} endpoint={e.connector.endpoint} size={36} />
                      {#if e.connected}<span class="check" aria-hidden="true"><svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg></span>{/if}
                    </span>
                    <span class="name">{e.name}</span>
                  </span>
                  <span class="desc">{e.description}</span>
                </button>
              </li>
            {/each}
            <li>
              <button type="button" class="tile other" aria-haspopup="dialog" onclick={openCommunity}>
                <span class="tile-head">
                  <span class="mono-icon" aria-hidden="true"><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" /></svg></span>
                  <span class="name">Não achou?</span>
                </span>
                <span class="desc">Procure na comunidade ou conecte qualquer serviço pelo endereço dele.</span>
              </button>
            </li>
          </ul>
        </section>
      {/if}
    </div>
  </div>
</div>

<ServiceDialog connector={current?.connector ?? null} entry={current?.entry} busy={busy !== null} onclose={() => (selected = null)} {onlink} />

<ServerDirectory open={community} initialQuery={needle} onclose={() => (community = false)} {onchange} />

<style>
  .catalog { display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: 14px; background: var(--surface); overflow: hidden; }
  .tools { display: flex; flex-direction: column; gap: 12px; padding: 16px 18px; }
  .search { max-width: 360px; }
  .chips { display: none; gap: 6px; overflow-x: auto; }
  .chip { flex-shrink: 0; min-height: 28px; padding: 0 10px; border: 1px solid var(--border-strong); border-radius: 999px; background: transparent; color: var(--text-2); font-size: 12.5px; }
  .chip.on { background: var(--hover); color: var(--text); }
  .chip .n { color: var(--muted); }
  .body { display: grid; grid-template-columns: 200px minmax(0, 1fr); border-top: 1px solid var(--border); }
  .cats { display: flex; flex-direction: column; gap: 2px; padding: 16px 10px; border-right: 1px solid var(--border); }
  .cat { display: flex; align-items: center; gap: 10px; min-height: 34px; padding: 0 10px; border: 0; border-radius: 10px 10px 10px 3px; background: none; color: var(--text-2); font-size: 13.5px; text-align: left; }
  .cat:hover { background: var(--hover); }
  .cat[aria-pressed="true"] { background: var(--surface-2); color: var(--text); font-weight: 600; }
  .cat.rule { position: relative; margin-bottom: 13px; }
  .cat.rule::after { content: ""; position: absolute; left: 10px; right: 10px; bottom: -8px; height: 1px; background: var(--border); }
  .cat-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cat-count { color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; }
  .list { min-width: 0; display: flex; flex-direction: column; gap: 22px; padding: 20px 18px 22px; }
  .section { display: flex; flex-direction: column; gap: 10px; }
  .feat, .grid { list-style: none; margin: 0; padding: 0; display: grid; }
  .feat { grid-template-columns: repeat(auto-fill, minmax(min(100%, 200px), 1fr)); gap: 14px; }
  .grid { grid-template-columns: repeat(auto-fill, minmax(min(100%, 196px), 1fr)); grid-auto-rows: 1fr; gap: 12px; }
  .feat > li, .grid > li { display: flex; min-width: 0; }
  .feat-card, .tile { flex: 1; min-width: 0; box-sizing: border-box; display: flex; flex-direction: column; font: inherit; color: inherit; text-align: left; border: 1px solid var(--border); background: var(--bg-side); cursor: pointer; transition: transform 0.2s, border-color 0.15s; }
  .feat-card:hover, .tile:hover { transform: translateY(-1px); border-color: var(--border-strong); }
  .feat-card:active, .tile:active { transform: scale(0.98); }
  .feat-card { position: relative; overflow: hidden; border-radius: 16px; padding: 0 16px 16px; }
  .feat-card :global(.sky) { margin: 0 -16px; }
  .feat-icon { position: relative; align-self: flex-start; margin-top: -22px; margin-bottom: 10px; border-radius: 13px; box-shadow: 0 0 0 3px var(--bg-side); }
  .name { font-weight: 600; font-size: 14px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .feat-card .name { font-size: 15px; }
  .desc { margin-top: 2px; font-size: 12.5px; line-height: 18px; height: 36px; color: var(--muted); display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .tile { gap: 10px; padding: 16px; border-radius: 16px; }
  .tile.skeleton { min-height: 104px; background: linear-gradient(90deg, var(--bg-side), var(--surface-2), var(--bg-side)); background-size: 200% 100%; animation: shimmer 1.4s infinite; }
  @keyframes shimmer { from { background-position: 200% 0; } to { background-position: -200% 0; } }
  .tile-head { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .tile-head .name { flex: 1; min-width: 0; }
  .tile-icon { position: relative; display: inline-flex; flex: none; }
  .check { position: absolute; right: -4px; bottom: -4px; width: 18px; height: 18px; display: grid; place-items: center; border-radius: 7px; background: var(--ok); color: var(--bg-side); box-shadow: 0 0 0 2px var(--bg-side); }
  .tile.other { background: transparent; border: 1.5px dashed var(--border-strong); }
  .tile.other:hover { border-color: var(--text-2); }
  .mono-icon { width: 36px; height: 36px; flex: none; display: grid; place-items: center; border-radius: 10px 10px 10px 3px; background: var(--surface-2); color: var(--text-2); border: 1px solid var(--border); }
  .empty { margin: auto; display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 36px 16px; text-align: center; color: var(--text-2); }
  .empty svg { color: var(--muted); margin-bottom: 6px; }
  .empty p { max-width: 360px; }
  .empty .btn { margin-top: 6px; }
  @media (max-width: 760px) {
    .body { grid-template-columns: 1fr; }
    .cats { display: none; }
    .chips { display: flex; }
  }
</style>
