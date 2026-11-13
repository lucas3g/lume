<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage } from "../api";
  import Toggle from "../components/Toggle.svelte";
  import { formatWhen } from "../format";
  import { loadConversations, openConversation } from "../state/chat.svelte";
  import { projects } from "../state/projects.svelte";
  import { DEFAULT_MODEL, describeRepeat, MODELS, type Repeat, type Task, type View } from "../types";

  interface Props {
    onnavigate: (view: View) => void;
  }
  let { onnavigate }: Props = $props();

  let tasks = $state<Task[]>([]);
  let editing = $state<string | null | undefined>(undefined);
  let error = $state<string | null>(null);
  let busy = $state(false);

  // Formulário.
  let name = $state("");
  let prompt = $state("");
  let projectId = $state<string>("");
  let model = $state<string>(DEFAULT_MODEL);
  let kind = $state<Repeat["kind"]>("daily");
  let minutes = $state(60);
  let time = $state("09:00");
  let weekday = $state(1);

  const WEEKDAYS = ["Domingo", "Segunda", "Terça", "Quarta", "Quinta", "Sexta", "Sábado"];

  function repeatFromForm(): Repeat {
    // Campo de hora vazio: o núcleo recusa a cadência com uma mensagem clara em vez de receber NaN.
    const [h = -1, m = -1] = time ? time.split(":").map(Number) : [];
    if (kind === "every_minutes") return { kind, minutes: Number(minutes) };
    if (kind === "daily") return { kind, hour: h, minute: m };
    return { kind, weekday: Number(weekday), hour: h, minute: m };
  }

  function startNew() {
    editing = null;
    name = "";
    prompt = "";
    projectId = "";
    model = DEFAULT_MODEL;
    kind = "daily";
    minutes = 60;
    time = "09:00";
    weekday = 1;
  }

  function startEdit(task: Task) {
    editing = task.id;
    name = task.name;
    prompt = task.prompt;
    projectId = task.project_id ?? "";
    model = task.model;
    kind = task.repeat.kind;
    const two = (n: number) => String(n).padStart(2, "0");
    if (task.repeat.kind === "every_minutes") minutes = task.repeat.minutes;
    else time = `${two(task.repeat.hour)}:${two(task.repeat.minute)}`;
    if (task.repeat.kind === "weekly") weekday = task.repeat.weekday;
  }

  async function run(action: () => Promise<void>) {
    error = null;
    busy = true;
    try {
      await action();
      tasks = await api.listTasks();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  const save = (event: SubmitEvent) => {
    event.preventDefault();
    return run(async () => {
      await api.saveTask({
        id: editing ?? null,
        name,
        prompt,
        project_id: projectId || null,
        model,
        repeat: repeatFromForm(),
        tz_offset_minutes: -new Date().getTimezoneOffset(),
      });
      editing = undefined;
    });
  };

  const runNow = (task: Task) =>
    run(async () => {
      const conversation = await api.runTask(task.id);
      await loadConversations();
      await openConversation(conversation.id);
      onnavigate("chat");
    });

  onMount(() => void run(async () => {}));
</script>

<div class="page">
  <div class="page-inner">
    <header class="page-head">
      <div>
        <h1>Tarefas agendadas</h1>
        <p class="muted">Prompts que rodam sozinhos enquanto o Lume estiver aberto, cada rodada em uma conversa nova, com a troca de contas de sempre.</p>
      </div>
      <button type="button" class="btn primary" title="Agendar um pedido para rodar sozinho em uma conversa nova" onclick={startNew}>Nova tarefa</button>
    </header>

    {#if error}<p class="error-text" role="alert">{error}</p>{/if}

    {#if editing !== undefined}
      <form class="card panel" onsubmit={save}>
        <label>Nome<input class="input" bind:value={name} placeholder="Ex.: Resumo das issues abertas" /></label>
        <label>O que fazer
          <textarea class="input" rows="4" bind:value={prompt} placeholder="O prompt enviado em cada rodada"></textarea>
        </label>
        <div class="grid">
          <label>Projeto
            <select class="input" bind:value={projectId}>
              <option value="">Sem projeto (conversa comum)</option>
              {#each projects.list as p (p.id)}
                <option value={p.id}>{p.name}{p.folder ? " · na pasta" : ""}</option>
              {/each}
            </select>
          </label>
          <label>Modelo
            <select class="input" bind:value={model}>
              {#each MODELS as m (m.id)}<option value={m.id}>{m.label}</option>{/each}
            </select>
          </label>
          <label>Cadência
            <select class="input" bind:value={kind}>
              <option value="every_minutes">A cada N minutos</option>
              <option value="daily">Todo dia</option>
              <option value="weekly">Toda semana</option>
            </select>
          </label>
          {#if kind === "every_minutes"}
            <label>Minutos<input class="input" type="number" min="5" step="5" bind:value={minutes} /></label>
          {:else}
            {#if kind === "weekly"}
              <label>Dia
                <select class="input" bind:value={weekday}>
                  {#each WEEKDAYS as d, i (d)}<option value={i}>{d}</option>{/each}
                </select>
              </label>
            {/if}
            <label>Horário<input class="input" type="time" bind:value={time} /></label>
          {/if}
        </div>
        <p class="muted small">Projetos com pasta rodam como sessão de agente aceitando edições de arquivo; o resto pede aprovação na conversa.</p>
        <div class="row">
          <button type="submit" class="btn primary" disabled={busy || !name.trim() || !prompt.trim()}>{editing ? "Salvar" : "Criar tarefa"}</button>
          <button type="button" class="btn" onclick={() => (editing = undefined)}>Cancelar</button>
        </div>
      </form>
    {/if}

    <section class="card" aria-label="Tarefas">
      {#each tasks as t (t.id)}
        <div class="task">
          <div class="text">
            <div class="row name"><strong>{t.name}</strong><span class="muted small">{describeRepeat(t.repeat)}</span></div>
            <div class="muted small ellipsis">{t.prompt}</div>
            <div class="muted small">
              {t.enabled ? `Próxima: ${formatWhen(t.next_run_at)}` : "Pausada"}{t.last_run_at ? ` · última: ${formatWhen(t.last_run_at)}` : ""}
            </div>
          </div>
          <div class="row actions">
            <button type="button" class="btn small" disabled={busy} onclick={() => runNow(t)}>Rodar agora</button>
            <button type="button" class="btn small" onclick={() => startEdit(t)}>Editar</button>
            <Toggle checked={t.enabled} label={`Ativar ${t.name}`} onchange={(v) => run(() => api.setTaskEnabled(t.id, v))} />
            <button type="button" class="btn icon" aria-label={`Excluir ${t.name}`} title={`Excluir ${t.name}`} onclick={() => run(() => api.deleteTask(t.id))}>
              <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
            </button>
          </div>
        </div>
      {:else}
        <div class="empty muted">Nenhuma tarefa ainda. Crie uma para o Lume rodar sozinho.</div>
      {/each}
    </section>
  </div>
</div>

<style>
  .panel { padding: 20px; display: flex; flex-direction: column; gap: 12px; }
  .panel label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; font-weight: 500; }
  .panel .input { font-weight: 400; }
  .panel textarea { padding: 8px 12px; resize: vertical; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; }
  .task { display: flex; flex-wrap: wrap; align-items: center; gap: 16px; padding: 14px 20px; border-bottom: 1px solid var(--hairline); }
  .task:last-child { border-bottom: 0; }
  .text { flex: 1 1 260px; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .name { gap: 10px; }
  .ellipsis { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .actions { gap: 8px; }
  .empty { padding: 16px 20px; }
</style>
