<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage } from "../../api";
  import { SERVICE, TIERS } from "../../pipeline";
  import { accounts } from "../../state/accounts.svelte";
  import { MODELS, MORE_MODELS, type Provider, type Tier, type TierModels } from "../../types";

  const CLAUDE = [...MODELS, ...MORE_MODELS].map((m) => ({ id: m.id as string, label: m.label as string }));
  let models = $state<TierModels>({ claude: {}, chatgpt: {}, gemini: {} });
  let error = $state<string | null>(null);
  /** Modelos que o CLI de cada serviço listou; `null` enquanto carrega. */
  let listed = $state<Record<Provider, { id: string; label: string }[] | null>>({ claude: CLAUDE, chatgpt: null, gemini: null });
  let listError = $state<Partial<Record<Provider, string>>>({});

  const SERVICES = (["claude", "chatgpt", "gemini"] as const).map((provider) => ({ provider: provider as Provider, ...SERVICE[provider] }));
  const CLAUDE_DEFAULT: Record<Tier, string> = { fast: "Haiku", balanced: "Sonnet", strong: "Opus" };

  /** Serviços com conta conectada; o Claude aparece sempre. */
  const visible = $derived(SERVICES.filter((s) => s.provider === "claude" || accounts.list.some((a) => a.provider === s.provider)));

  onMount(() => {
    void api.pipelineModels().then((m) => (models = m), (e) => (error = errorMessage(e)));
  });

  /** Serviços cuja lista já foi pedida (fora do estado: pedir não deve rodar o efeito de novo). */
  const requested = new Set<Provider>();

  // Pede a lista ao CLI de cada serviço conectado, uma vez.
  $effect(() => {
    for (const s of visible) {
      if (s.provider === "claude" || requested.has(s.provider)) continue;
      const provider = s.provider;
      requested.add(provider);
      api.cliModels(provider).then(
        (ids) => (listed[provider] = ids.map((id) => ({ id, label: id }))),
        (e) => {
          listError[provider] = errorMessage(e);
          listed[provider] = [];
        },
      );
    }
  });

  /** Opções do nível: as listadas e, se o salvo não estiver entre elas, o salvo. */
  function options(provider: Provider, tier: Tier) {
    const list = listed[provider] ?? [];
    const current = models[provider][tier];
    return current && !list.some((m) => m.id === current) ? [...list, { id: current, label: `${current} (salvo)` }] : list;
  }

  const defaultLabel = (s: (typeof SERVICES)[number], tier: Tier) =>
    s.provider === "claude" ? `Padrão (${CLAUDE_DEFAULT[tier]} mais recente)` : `Padrão do ${s.cli}`;

  async function choose(provider: Provider, tier: Tier, value: string) {
    error = null;
    const previous = $state.snapshot(models);
    const next: TierModels = $state.snapshot(models);
    if (value) next[provider][tier] = value;
    else delete next[provider][tier];
    models = next;
    try {
      models = await api.savePipelineModels(next);
    } catch (e) {
      models = previous;
      error = errorMessage(e);
    }
  }
</script>

<section class="card" aria-label="Modelos por serviço">
  <div class="head">
    <h2 class="section-title">Modelos por serviço</h2>
    <p class="muted small">
      Cada papel usa um nível de modelo: rápido (Triagem e Relator), equilibrado (Testador) ou forte (Planejador,
      Implementador e Revisor). Se a conta trocar para outro serviço no meio de uma etapa, ela segue com o modelo desse
      serviço. A escolha é salva na hora e vale para as próximas etapas.
    </p>
  </div>
  {#if error}<p class="error-text pad" role="alert">{error}</p>{/if}
  <div class="table">
    <span></span>
    {#each TIERS as t (t.tier)}<span class="label">{t.label}</span>{/each}
    {#each visible as s (s.provider)}
      <span class="service">{s.label}</span>
      {#each TIERS as t (t.tier)}
        <select class="input" aria-label={`${s.label}, modelo ${t.label.toLowerCase()}`} value={models[s.provider][t.tier] ?? ""}
          disabled={listed[s.provider] === null} onchange={(e) => choose(s.provider, t.tier, e.currentTarget.value)}>
          <option value="">{listed[s.provider] === null ? "Carregando modelos…" : defaultLabel(s, t.tier)}</option>
          {#each options(s.provider, t.tier) as m (m.id)}<option value={m.id}>{m.label}</option>{/each}
        </select>
      {/each}
      {#if listError[s.provider]}
        <span></span><span class="small hint warn">Não foi possível listar os modelos do {s.cli}: {listError[s.provider]} Fica o padrão do CLI.</span>
      {:else if s.provider !== "claude" && listed[s.provider]?.length === 0}
        <span></span><span class="muted small hint">O {s.cli} não informou modelos; fica o padrão do CLI.</span>
      {/if}
    {/each}
  </div>
</section>

<style>
  .head { padding: 16px 20px 4px; display: flex; flex-direction: column; gap: 4px; }
  .pad { padding: 4px 20px 0; }
  .table { display: grid; grid-template-columns: 90px repeat(3, minmax(0, 1fr)); gap: 8px 10px; align-items: center; padding: 12px 20px 18px; }
  .table select { width: 100%; min-width: 0; }
  .service { font-weight: 500; }
  .hint { grid-column: 2 / -1; margin: -4px 0 6px; }
  .warn { color: var(--warn-text); }
  @media (max-width: 640px) {
    .table { grid-template-columns: 1fr; }
    .table > span:empty, .table > .label { display: none; }
    .hint { grid-column: auto; }
  }
</style>
