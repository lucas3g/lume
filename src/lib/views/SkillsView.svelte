<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage } from "../api";
  import SkillStore from "../components/store/SkillStore.svelte";
  import Toggle from "../components/Toggle.svelte";
  import { agents, loadAgents } from "../state/agents.svelte";
  import { MODELS, MORE_MODELS, SCOPE_ALL, modelLabel, type Agent, type Skill } from "../types";

  let tab = $state<"installed" | "store">("installed");
  let scope = $state<string>(SCOPE_ALL);
  let skills = $state<Skill[]>([]);
  let filter = $state("");
  let error = $state<string | null>(null);
  /** Formulário do agente: `null` fechado, sem `id` é um agente novo. */
  let form = $state<{ id: string | null; name: string; instructions: string; model: string } | null>(null);

  const ORIGIN_LABEL = { personal: "Pessoal", plugin: "Plugin" } as const;
  const selected = $derived(agents.list.find((a) => a.id === scope));
  const visible = $derived(skills.filter((s) => s.name.toLowerCase().includes(filter.trim().toLowerCase())));
  const granted = (list: Skill[]) => list.filter((s) => s.enabled || s.inherited).length;

  async function loadSkills() {
    skills = await api.listSkills(scope);
  }

  async function select(next: string) {
    scope = next;
    form = null;
    await loadSkills();
  }

  async function toggle(skill: Skill, enabled: boolean) {
    skill.enabled = enabled;
    await api.setSkill(scope, skill.name, enabled);
  }

  /** Skills recomendadas pela IA; são ligadas ao agente quando ele é salvo. */
  let suggested = $state<string[]>([]);
  let drafting = $state(false);

  function edit(agent: Agent | null) {
    form = { id: agent?.id ?? null, name: agent?.name ?? "", instructions: agent?.instructions ?? "", model: agent?.model ?? "" };
    suggested = [];
  }

  /** Pede ao modelo instruções e skills a partir do nome digitado. */
  async function generate() {
    if (!form || !form.name.trim()) return;
    error = null;
    drafting = true;
    try {
      const draft = await api.draftAgent(form.name);
      form.instructions = draft.instructions;
      suggested = draft.skills;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      drafting = false;
    }
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (!form) return;
    error = null;
    try {
      const saved = await api.saveAgent({ ...form, model: form.model || null });
      for (const skill of suggested) await api.setSkill(saved.id, skill, true);
      suggested = [];
      await loadAgents();
      await select(saved.id);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function remove(agent: Agent) {
    await api.deleteAgent(agent.id);
    await loadAgents();
    await select(SCOPE_ALL);
  }

  onMount(() => void Promise.all([loadAgents(), loadSkills()]));
</script>

<div class="page">
  <div class="page-inner wide">
    <header class="page-head">
      <div>
        <h1>Skills</h1>
        <p class="muted">Instruções prontas que o assistente carrega quando a tarefa pede. Escolha quais valem para todos e quais cada agente pode usar.</p>
      </div>
      <div class="tabs" role="tablist" aria-label="Skills">
        <button type="button" role="tab" aria-selected={tab === "installed"} class:on={tab === "installed"} onclick={() => (tab = "installed")}>Instaladas</button>
        <button type="button" role="tab" aria-selected={tab === "store"} class:on={tab === "store"} onclick={() => (tab = "store")}>Loja</button>
      </div>
      {#if tab === "installed"}
        <button type="button" class="btn primary" onclick={() => edit(null)}>Novo agente</button>
      {/if}
    </header>

    {#if tab === "store"}
      <SkillStore onchange={loadSkills} />
    {:else}

    {#if error}<p class="error-text" role="alert">{error}</p>{/if}

    <div class="columns">
      <section class="card scopes" aria-label="Aplicar a">
        <div class="heading">Aplicar a</div>
        <button type="button" class="scope" class:on={scope === SCOPE_ALL} aria-pressed={scope === SCOPE_ALL} onclick={() => select(SCOPE_ALL)}>
          <strong>Todos os agentes</strong>
          <span class="muted">Vale para qualquer conversa</span>
        </button>
        {#each agents.list as a (a.id)}
          <button type="button" class="scope" class:on={scope === a.id} aria-pressed={scope === a.id} onclick={() => select(a.id)}>
            <strong>{a.name}</strong>
            <span class="muted">{a.model ? modelLabel(a.model) : "Modelo da conversa"}</span>
          </button>
        {:else}
          <p class="muted small empty">Crie agentes para dar a cada um instruções e skills próprias.</p>
        {/each}
      </section>

      <div class="main">
        {#if form}
          <form class="card agent-form" onsubmit={save}>
            <h2 class="section-title">{form.id ? "Editar agente" : "Novo agente"}</h2>
            <label>Nome
              <span class="row name-row">
                <input class="input" bind:value={form.name} placeholder="Ex.: Revisor de código" />
                <button type="button" class="btn" disabled={drafting || !form.name.trim()} onclick={generate} title="Gera as instruções e recomenda skills a partir do nome">
                  <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM19 17l.7 2 2 .7-2 .7-.7 2-.7-2-2-.7 2-.7z" /></svg>
                  {drafting ? "Gerando…" : "Gerar com IA"}
                </button>
              </span>
            </label>
            <label>Instruções
              <textarea class="input" rows="6" bind:value={form.instructions} placeholder="Como este agente deve trabalhar e responder"></textarea>
            </label>
            {#if suggested.length}
              <div class="suggested">
                <span class="muted small">Skills recomendadas (ligadas ao salvar):</span>
                {#each suggested as s (s)}
                  <span class="chip">{s}<button type="button" aria-label={`Não usar ${s}`} onclick={() => (suggested = suggested.filter((x) => x !== s))}>×</button></span>
                {/each}
              </div>
            {/if}
            <label>Modelo
              <select class="input" bind:value={form.model}>
                <option value="">O da conversa</option>
                {#each [...MODELS, ...MORE_MODELS] as m (m.id)}<option value={m.id}>{m.label}</option>{/each}
              </select>
            </label>
            <div class="row">
              <button type="submit" class="btn primary" disabled={!form.name.trim()}>Salvar</button>
              <button type="button" class="btn" onclick={() => (form = null)}>Cancelar</button>
            </div>
          </form>
        {/if}

        <section class="card" aria-label="Skills">
          <div class="scope-head">
            <div>
              <strong>{selected?.name ?? "Todos os agentes"}</strong>
              <div class="muted small">
                {selected ? "Além das skills gerais, este agente pode usar as marcadas abaixo." : "Skills disponíveis em qualquer conversa e para qualquer agente."}
                {granted(skills)} de {skills.length} disponíveis.
              </div>
            </div>
            {#if selected}
              <button type="button" class="btn small" onclick={() => edit(selected)}>Editar</button>
              <button type="button" class="btn small danger" onclick={() => remove(selected)}>Excluir</button>
            {/if}
          </div>
          <div class="search"><input class="input" type="search" aria-label="Filtrar skills" placeholder="Filtrar skills" bind:value={filter} /></div>
          {#each visible as s (s.name)}
            <div class="skill">
              <div class="info">
                <div class="row name">
                  <strong>{s.name}</strong>
                  <span class="origin">{ORIGIN_LABEL[s.origin]}</span>
                  {#if s.inherited}<span class="pill">Vale para todos</span>{/if}
                </div>
                {#if s.description}<div class="muted small description">{s.description}</div>{/if}
              </div>
              {#if !s.inherited}
                <Toggle checked={s.enabled} label={s.name} onchange={(v) => toggle(s, v)} />
              {/if}
            </div>
          {:else}
            <div class="empty muted">{skills.length ? "Nenhuma skill com esse nome." : "Nenhuma skill encontrada em ~/.claude/skills."}</div>
          {/each}
          <div class="foot muted small">Skills de plugins aparecem aqui depois da primeira mensagem enviada pelo app.</div>
        </section>
      </div>
    </div>
    {/if}
  </div>
</div>

<style>
  .wide { max-width: 980px; }
  .tabs { display: flex; padding: 3px; border-radius: 9px; background: var(--bg-side); border: 1px solid var(--border); }
  .tabs button { min-height: 30px; padding: 0 14px; border: 0; border-radius: 6px; background: transparent; font-size: 13px; color: var(--text-2); }
  .tabs button.on { background: var(--hover); color: #fff; font-weight: 500; }
  .columns { display: flex; flex-wrap: wrap; gap: 24px; align-items: flex-start; }
  .scopes { flex: 1 1 240px; padding: 8px; display: flex; flex-direction: column; gap: 2px; }
  .heading { padding: 8px 10px 6px; font-size: 12px; font-weight: 600; color: var(--muted); }
  .scope { display: flex; flex-direction: column; min-height: 52px; padding: 6px 10px; border: 0; border-radius: 6px; background: transparent; text-align: left; justify-content: center; }
  .scope:hover, .scope.on { background: var(--hover); }
  .scope strong { font-weight: 500; }
  .scope.on strong { font-weight: 600; color: #fff; }
  .scope .muted { font-size: 12px; }
  .main { flex: 2.4 1 420px; min-width: 0; display: flex; flex-direction: column; gap: 16px; }
  .agent-form { padding: 20px; display: flex; flex-direction: column; gap: 12px; }
  .agent-form label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; font-weight: 500; }
  .agent-form .input { font-weight: 400; }
  .agent-form textarea { padding: 8px 12px; resize: vertical; }
  .name-row { gap: 8px; }
  .name-row .input { flex: 1; }
  .name-row .btn { display: inline-flex; align-items: center; gap: 6px; white-space: nowrap; }
  .suggested { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .chip { display: inline-flex; align-items: center; gap: 4px; padding: 2px 4px 2px 9px; border: 1px solid var(--border-strong); border-radius: 999px; background: var(--surface-2); font-size: 12px; }
  .chip button { border: 0; padding: 0 4px; background: transparent; color: var(--muted); font-size: 13px; line-height: 1; }
  .scope-head { display: flex; align-items: center; gap: 8px; padding: 14px 16px; border-bottom: 1px solid var(--border); background: var(--bg-side); border-radius: 10px 10px 0 0; }
  .scope-head > div { flex: 1; }
  .search { padding: 12px 16px; border-bottom: 1px solid var(--hairline); }
  .search .input { width: 100%; }
  .skill { display: flex; align-items: center; gap: 14px; padding: 12px 16px; border-bottom: 1px solid var(--hairline); }
  .info { flex: 1; min-width: 0; }
  .name { gap: 8px; flex-wrap: wrap; }
  .origin { padding: 1px 8px; border: 1px solid var(--border-strong); border-radius: 999px; font-size: 12px; color: var(--text-2); }
  .description { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .empty, .foot { padding: 12px 16px; }
</style>
