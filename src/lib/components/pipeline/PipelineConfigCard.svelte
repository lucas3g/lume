<script lang="ts">
  import { api, errorMessage } from "../../api";
  import { OPTIONAL_ROLES, STATIONS } from "../../pipeline";
  import { agents } from "../../state/agents.svelte";
  import { EFFORTS, type PipelineConfig, type PipelineRole, type View } from "../../types";
  import Toggle from "../Toggle.svelte";

  interface Props {
    projectId: string;
    onnavigate: (view: View) => void;
  }
  let { projectId, onnavigate }: Props = $props();

  const EMPTY: PipelineConfig = { agents: {}, skipped: [], jira_template: null, default_jira_template: null, efforts: {} };
  /** Texto do campo do modelo de task, salvo ao sair do campo. */
  let template = $state("");
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
        template = c.jira_template ?? "";
        defaultTemplate = c.default_jira_template ?? "";
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

  function saveTemplate() {
    const value = template.trim() || null;
    if (value !== (config.jira_template ?? null)) void save({ ...config, jira_template: value });
  }

  /** Texto do campo do modelo padrão (de todos os projetos), salvo ao sair do campo. */
  let defaultTemplate = $state("");
  function saveDefaultTemplate() {
    const value = defaultTemplate.trim() || null;
    if (value === (config.default_jira_template ?? null)) return;
    error = null;
    api.savePipelineDefaultTemplate(value).then(
      (saved) => {
        config = { ...config, default_jira_template: saved };
        defaultTemplate = saved ?? "";
      },
      (e) => (error = errorMessage(e)),
    );
  }

  function setEffort(role: PipelineRole, effort: string) {
    const next = { ...config.efforts };
    if (effort) next[role] = effort;
    else delete next[role];
    void save({ ...config, efforts: next });
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
        Cada papel pode ter um agente e um nível de esforço. O agente soma as instruções, as skills e o modelo dele (só nas contas Claude) às regras fixas do papel (veredito, sem commit,
        o texto do Jira é dado). Triagem, Testador, Code review e Relator podem ser desligados; vale para as próximas execuções.
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
      <select class="input effort" aria-label={`Esforço do ${s.name}`} title="Esforço de raciocínio do modelo nesta etapa"
        value={config.efforts[s.role] ?? ""} disabled={!on} onchange={(e) => setEffort(s.role, e.currentTarget.value)}>
        <option value="">Esforço padrão</option>
        {#each EFFORTS as e (e.id)}<option value={e.id}>Esforço {e.label.toLowerCase()}{e.warn ? " (consome mais)" : ""}</option>{/each}
      </select>
      {#if optional}
        <Toggle checked={on} label={`${on ? "Desligar" : "Ligar"} ${s.name}`} onchange={(v) => setEnabled(s.role, v)} />
      {:else}
        <span class="toggle-space"></span>
      {/if}
    </div>
  {/each}
  <div class="template">
    <label class="grow" for={`tpl-${projectId}`}>
      <span class="name">Modelo de task do Jira</span>
      <span class="muted small">
        Quando a execução começa sem task escolhida, o Relator cria uma no fim, copiando o projeto, o tipo, os campos e a
        estrutura da descrição deste modelo, e mostra o link no relatório. Precisa do conector do Atlassian (Jira) na conta.
        Vazio: usa o modelo padrão.
      </span>
    </label>
    <input id={`tpl-${projectId}`} class="input tpl" bind:value={template} placeholder={config.default_jira_template ?? "https://empresa.atlassian.net/browse/EI-5198"}
      disabled={config.skipped.includes("reporter")} onblur={saveTemplate}
      onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()} />
  </div>
  <div class="template sub">
    <label class="grow" for={`tpl-default-${projectId}`}>
      <span class="name">Modelo padrão (todos os projetos)</span>
      <span class="muted small">Vale para os projetos sem modelo próprio. Vazio em ambos: o Relator não cria task.</span>
    </label>
    <input id={`tpl-default-${projectId}`} class="input tpl" bind:value={defaultTemplate} placeholder="https://empresa.atlassian.net/browse/EI-5198"
      disabled={config.skipped.includes("reporter")} onblur={saveDefaultTemplate}
      onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()} />
  </div>
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
  .agent { width: 210px; min-height: 34px; }
  .effort { width: 170px; min-height: 34px; }
  .toggle-space { width: 44px; flex-shrink: 0; }
  .template { display: flex; align-items: center; gap: 12px; padding: 12px 16px 16px; border-top: 1px solid var(--hairline); }
  .template.sub { padding-top: 0; border-top: 0; }
  .template label { display: flex; flex-direction: column; gap: 2px; }
  .tpl { width: 330px; min-height: 34px; font-family: var(--mono); font-size: 12px; }
  @media (max-width: 640px) {
    .line { flex-wrap: wrap; }
    .agent, .effort, .tpl { width: 100%; }
    .template { flex-wrap: wrap; }
  }
</style>
