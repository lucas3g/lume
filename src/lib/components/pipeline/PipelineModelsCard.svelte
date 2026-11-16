<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage } from "../../api";
  import { TIERS } from "../../pipeline";
  import { accounts } from "../../state/accounts.svelte";
  import type { Provider, TierModels } from "../../types";

  let models = $state<TierModels>({ claude: {}, chatgpt: {}, gemini: {} });
  let error = $state<string | null>(null);
  let saved = $state(false);
  let busy = $state(false);

  const SERVICES: { provider: Provider; label: string; placeholder: Record<string, string>; hint: string }[] = [
    { provider: "claude", label: "Claude", placeholder: { fast: "haiku", balanced: "sonnet", strong: "opus" },
      hint: "Vazio usa os apelidos haiku, sonnet e opus, sempre na versão mais recente." },
    { provider: "chatgpt", label: "ChatGPT", placeholder: { fast: "padrão do Codex", balanced: "padrão do Codex", strong: "padrão do Codex" },
      hint: "Use os nomes que o Codex aceita em --model. Vazio usa o modelo configurado no Codex." },
    { provider: "gemini", label: "Gemini", placeholder: { fast: "padrão do Antigravity", balanced: "padrão do Antigravity", strong: "padrão do Antigravity" },
      hint: "Rode `agy models` no terminal para ver os nomes. Vazio usa o modelo configurado no Antigravity." },
  ];

  /** Serviços com conta cadastrada; o Claude aparece sempre. */
  const visible = $derived(SERVICES.filter((s) => s.provider === "claude" || accounts.list.some((a) => a.provider === s.provider)));

  onMount(() => {
    void api.pipelineModels().then((m) => (models = m), (e) => (error = errorMessage(e)));
  });

  async function save(event: SubmitEvent) {
    event.preventDefault();
    error = null;
    busy = true;
    try {
      models = await api.savePipelineModels(models);
      saved = true;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }
</script>

<form class="card" aria-label="Modelos por serviço" onsubmit={save}>
  <div class="head">
    <h2 class="section-title">Modelos por serviço</h2>
    <p class="muted small">
      Cada papel usa um nível de modelo: rápido (Triagem e Relator), equilibrado (Testador) ou forte (Planejador,
      Implementador e Revisor). Se a conta trocar para outro serviço no meio de uma etapa, ela segue com o modelo desse serviço.
    </p>
  </div>
  {#if error}<p class="error-text pad" role="alert">{error}</p>{/if}
  <div class="table">
    <span></span>
    {#each TIERS as t (t.tier)}<span class="label">{t.label}</span>{/each}
    {#each visible as s (s.provider)}
      <span class="service">{s.label}</span>
      {#each TIERS as t (t.tier)}
        <input class="input mono-in" aria-label={`${s.label}, modelo ${t.label.toLowerCase()}`} placeholder={s.placeholder[t.tier]}
          value={models[s.provider][t.tier] ?? ""} oninput={(e) => { models[s.provider][t.tier] = e.currentTarget.value; saved = false; }} />
      {/each}
      <span></span><span class="muted small hint">{s.hint}</span>
    {/each}
  </div>
  <div class="row foot">
    <button type="submit" class="btn primary small" disabled={busy}>Salvar modelos</button>
    {#if saved}<span class="muted small" role="status">Salvo. Vale para as próximas etapas.</span>{/if}
  </div>
</form>

<style>
  .head { padding: 16px 20px 4px; display: flex; flex-direction: column; gap: 4px; }
  .pad { padding: 4px 20px 0; }
  .table { display: grid; grid-template-columns: 90px repeat(3, minmax(0, 1fr)); gap: 8px 10px; align-items: center; padding: 12px 20px; }
  .service { font-weight: 500; }
  .mono-in { width: 100%; min-width: 0; font-family: var(--mono); font-size: 12.5px; }
  .hint { grid-column: 2 / -1; margin: -4px 0 6px; }
  .foot { padding: 0 20px 16px; }
  @media (max-width: 640px) {
    .table { grid-template-columns: 1fr; }
    .table > span:empty, .table > .label { display: none; }
    .hint { grid-column: auto; }
  }
</style>
