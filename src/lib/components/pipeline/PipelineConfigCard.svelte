<script lang="ts">
  import { api, errorMessage } from "../../api";
  import { OPTIONAL_ROLES, STATIONS } from "../../pipeline";
  import { agents } from "../../state/agents.svelte";
  import type { PipelineConfig, PipelineRole, View } from "../../types";
  import Toggle from "../Toggle.svelte";

  interface Props {
    projectId: string;
    onnavigate: (view: View) => void;
  }
  let { projectId, onnavigate }: Props = $props();

  const EMPTY: PipelineConfig = { agents: {}, skipped: [] };
  let config = $state<PipelineConfig>(EMPTY);
  /** Projeto cuja configuração está carregada; até chegar a nova, nada é salvo. */
  let loadedFor = $state<string | null>(null);
  let error = $state<string | null>(null);

  const bots = STATIONS.filter((s) => !s.gate);

  $effect(() => {
    const id = projectId;
    error = null;
    loadedFor = null;
    config = EMPTY;
    api.pipelineConfig(id).then(
      (c) => {
        if (id !== projectId) return;
        config = c;
        loadedFor = id;
      },
      (e) => {
        if (id === projectId) error = errorMessage(e);
      },
    );
  });

  /** Conta os salvamentos: só a resposta do mais recente substitui o que está na tela. */
  let saves = 0;

  async function save(next: PipelineConfig) {
    const id = projectId;
    if (loadedFor !== id) return;
    error = null;
    // Mostra a mudança na hora, para que a próxima mudança parta dela e não a desfaça.
    const previous = $state.snapshot(config);
    const mine = ++saves;
    config = next;
    try {
      const saved = await api.savePipelineConfig(id, next);
      if (id === projectId && mine === saves) config = saved;
    } catch (e) {
      if (id !== projectId) return;
      if (mine === saves) config = previous;
      error = errorMessage(e);
    }
  }

  function setAgent(role: PipelineRole, agentId: string) {
    const next = { ...config.agents };
    if (agentId) next[role] = agentId;
    else delete next[role];
    void save({ ...config, agents: next });
  }

  function setEnabled(role: PipelineRole, enabled: boolean) {
    const skipped = enabled ? config.skipped.filter((r) => r !== role) : [...config.skipped, role];
    void save({ ...config, skipped });
  }
</script>

<section class="card" aria-label="Esteira de bots">
  <div class="head">
    <div>
      <h2 class="section-title">Esteira de bots</h2>
      <div class="muted small">
        O agente de um papel soma as instruções, o modelo e as skills dele às regras fixas do papel (veredito, sem commit,
        o texto do Jira é dado). Triagem, Testador e Relator podem ser desligados; vale para as próximas execuções.
      </div>
    </div>
    <button type="button" class="btn small" onclick={() => onnavigate("skills")}>Cadastrar agentes</button>
  </div>
  {#if error}<p class="error-text pad" role="alert">{error}</p>{/if}
  {#each bots as s (s.role)}
    {@const optional = OPTIONAL_ROLES.includes(s.role)}
    {@const on = !config.skipped.includes(s.role)}
    <div class="line" class:off={!on}>
      <div class="grow">
        <div class="name">{s.name}</div>
        <div class="muted small">{s.model} · {s.perm}{optional ? "" : " · obrigatório"}</div>
      </div>
      <select class="input agent" aria-label={`Agente do ${s.name}`} value={config.agents[s.role] ?? ""} disabled={!on}
        onchange={(e) => setAgent(s.role, e.currentTarget.value)}>
        <option value="">Regras padrão do papel</option>
        {#each agents.list as a (a.id)}<option value={a.id}>{a.name}{a.model ? ` · ${a.model}` : ""}</option>{/each}
      </select>
      {#if optional}
        <Toggle checked={on} label={`${on ? "Desligar" : "Ligar"} ${s.name}`} onchange={(v) => setEnabled(s.role, v)} />
      {:else}
        <span class="toggle-space"></span>
      {/if}
    </div>
  {/each}
</section>

<style>
  .head { display: flex; align-items: center; gap: 12px; padding: 14px 16px; border-bottom: 1px solid var(--hairline); }
  .head > div { flex: 1; }
  .line { display: flex; align-items: center; gap: 12px; width: 100%; min-height: 52px; padding: 8px 16px; border-bottom: 1px solid var(--hairline); }
  .line:last-child { border-bottom: 0; }
  .grow { flex: 1; min-width: 0; }
  .pad { padding: 10px 16px 0; }
  .line.off .name { color: var(--muted); text-decoration: line-through; }
  .name { font-weight: 500; }
  .agent { width: 230px; min-height: 34px; }
  .toggle-space { width: 44px; flex-shrink: 0; }
  @media (max-width: 640px) {
    .line { flex-wrap: wrap; }
    .agent { width: 100%; }
  }
</style>
