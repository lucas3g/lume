<script lang="ts">
  import { onMount } from "svelte";
  import { chat } from "../../state/chat.svelte";
  import { accounts, activeAccount } from "../../state/accounts.svelte";
  import { api } from "../../api";
  import { pipelines } from "../../state/pipelines.svelte";
  import {
    duration, icon, isActive, passesByRole, plannedModelText, stageAccountId, stageModelText, stageVisual, STATIONS, VISUAL,
    type Visual,
  } from "../../pipeline";
  import { EFFORTS, type PipelineDetail, type PipelineRole, type Provider, type TierModels } from "../../types";

  interface Props {
    detail: PipelineDetail;
    /** Estação aberta no painel. */
    selected: PipelineRole | null;
    /** Instante atual (s), para o tempo das etapas em andamento. */
    now: number;
    onselect: (role: PipelineRole) => void;
  }
  let { detail, selected, now, onselect }: Props = $props();

  const ORDER = STATIONS.map((s) => s.role);
  const FORWARD_LABEL: Partial<Record<PipelineRole, string>> = {
    triage: "task", planner: "task triada", approval: "plano", implementer: "plano aprovado",
    reviewer: "diff", tester: "diff aprovado", reporter: "verificado",
  };

  const passes = $derived(passesByRole(detail));

  /** Modelos de cada nível, para as estações que ainda não rodaram. */
  let tierModels = $state<TierModels | null>(null);
  onMount(() => {
    api.pipelineModels().then((m) => (tierModels = m), () => {});
  });
  /** Serviço da conta (pelo id ou pelo nome); sem conta ainda, o da conta que atende agora. */
  const providerOf = (account: { id?: string | null; name?: string | null }): Provider =>
    accounts.list.find((a) => (account.id && a.id === account.id) || (account.name && a.name === account.name))?.provider ??
    activeAccount()?.provider ??
    "claude";
  const finished = $derived(!isActive(detail.status));

  /** Estado de cada estação: o da passagem mais recente dela. */
  const cards = $derived(
    STATIONS.map((station) => {
      const list = passes[station.role];
      const last = list.at(-1);
      const off = detail.skipped.includes(station.role);
      let visual: Visual = finished || off ? "skipped" : "waiting";
      let label = off ? "Desligada" : VISUAL[visual].label;
      let account: string | null = null;
      if (last) {
        const asking = !!chat.permissions[last.conversation_id];
        ({ visual, label } = stageVisual(last, asking));
        if (last.status === "running" && last.conversation_id) account = chat.turns[last.conversation_id]?.account ?? null;
        if (!account && last.conversation_id) {
          const id = stageAccountId(pipelines.messages[last.conversation_id] ?? []);
          account = accounts.list.find((a) => a.id === id)?.name ?? null;
        }
        if (station.gate) account = "você";
      }
      const agentInfo = detail.agents[station.role] ?? null;
      let model: string | null = null;
      if (!station.gate) {
        if (last?.conversation_id) {
          const id = stageAccountId(pipelines.messages[last.conversation_id] ?? []);
          const live = last.status === "running" ? chat.turns[last.conversation_id]?.account : null;
          model = stageModelText(detail, last, providerOf(live ? { name: live } : { id }));
        }
        model ??= plannedModelText(station.role, providerOf({}), tierModels, agentInfo?.model ?? null);
      }
      const spent = list.reduce((sum, s) => sum + ((s.finished_at ?? now) - s.started_at), 0);
      const agent = agentInfo?.name ?? null;
      const effortId = detail.efforts[station.role];
      const effort = effortId ? (EFFORTS.find((e) => e.id === effortId)?.label.toLowerCase() ?? effortId) : null;
      return { station, visual, label, account, agent, model, effort, off, time: list.length ? duration(spent) : "" };
    }),
  );

  const returnedBy = (role: PipelineRole) => passes[role].filter((s) => s.status === "returned").length;
  const arcs = $derived({
    plan: returnedBy("approval"),
    review: returnedBy("reviewer"),
    test: returnedBy("tester"),
  });
  /** Trilho até a estação já percorrido. */
  const reached = (role: PipelineRole): boolean => {
    if (passes[role].length > 0) return true;
    // Estação desligada: o trilho conta como percorrido quando o trabalho já passou adiante.
    const i = ORDER.indexOf(role);
    return detail.skipped.includes(role) && (detail.status === "done" || ORDER.slice(i + 1).some((r) => passes[r].length > 0));
  };

  // ---- geometria

  let stage = $state<HTMLDivElement>();
  let nodes: Record<string, HTMLSpanElement> = {};
  let geo = $state({ w: 0, y: 0, x: {} as Record<string, number> });

  function measure() {
    if (!stage) return;
    const box = stage.getBoundingClientRect();
    if (!box.width) return;
    const x: Record<string, number> = {};
    let y = 0;
    for (const role of ORDER) {
      const r = nodes[role]?.getBoundingClientRect();
      if (!r) return;
      x[role] = r.left + r.width / 2 - box.left;
      y = r.top + r.height / 2 - box.top;
    }
    geo = { w: stage.clientWidth, y, x };
  }

  onMount(() => {
    measure();
    const observer = new ResizeObserver(measure);
    if (stage) observer.observe(stage);
    return () => observer.disconnect();
  });

  const f = (n: number) => n.toFixed(1);
  const line = (a: number, b: number) => `M${f(a)} ${f(geo.y)}H${f(b)}`;
  function arcPath(from: number, to: number, h: number): string {
    const y = geo.y, top = y - 14 - h, r = 9;
    return `M${f(from)} ${f(y)}V${f(top + r)}Q${f(from)} ${f(top)} ${f(from - r)} ${f(top)}H${f(to + r)}Q${f(to)} ${f(top)} ${f(to)} ${f(top + r)}V${f(y)}`;
  }
  const arrowHead = (to: number) => `M${f(to - 4)} ${f(geo.y - 23)}L${f(to + 4)} ${f(geo.y - 23)}L${f(to)} ${f(geo.y - 16)}Z`;
  const ARCS = {
    plan: { from: "approval", to: "planner", dx: 0, h: 24 },
    review: { from: "reviewer", to: "implementer", dx: 7, h: 24 },
    test: { from: "tester", to: "implementer", dx: -7, h: 42 },
  } as const;
  const arcD = (k: keyof typeof ARCS) => arcPath(geo.x[ARCS[k].from], geo.x[ARCS[k].to] + ARCS[k].dx, ARCS[k].h);
  const arcMid = (k: keyof typeof ARCS) => ({
    x: (geo.x[ARCS[k].from] + geo.x[ARCS[k].to] + ARCS[k].dx) / 2,
    y: geo.y - 14 - ARCS[k].h,
  });

  // ---- pacote: leva o trabalho de uma estação à outra quando uma etapa nova começa

  let movePath = $state<SVGPathElement>();
  let packet = $state<HTMLDivElement>();
  let packetLabel = $state("");
  let packetBack = $state(false);
  let flowing = $state<string | null>(null);
  let seenRun: string | null = null;
  let seenCount = 0;
  const reduce = typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  $effect(() => {
    const count = detail.stages.length;
    const runId = detail.id;
    if (runId !== seenRun) {
      seenRun = runId;
      seenCount = count;
      return;
    }
    if (count <= seenCount) return;
    const before = detail.stages[count - 2];
    const latest = detail.stages[count - 1];
    seenCount = count;
    // O ajuste sozinho não é estação: o pacote sai dele quando o Implementador começa.
    if (latest.role === "feedback") {
      seenCount = count - 1;
      return;
    }
    if (!geo.w || (before && before.role === latest.role)) return;
    const fromIdx = before ? ORDER.indexOf(before.role) : -1;
    const toIdx = ORDER.indexOf(latest.role);
    let d: string;
    let back = false;
    let label = fromIdx < 0 ? "task" : (FORWARD_LABEL[latest.role] ?? "");
    if (before?.role === "feedback") {
      // Reaberta com o seu ajuste: o trabalho volta do fim da esteira ao Implementador.
      back = true;
      d = line(geo.w - 6, geo.x[latest.role]);
      label = "seu ajuste";
    } else if (fromIdx > toIdx && before) {
      back = true;
      const key = before.role === "approval" ? "plan" : before.role === "tester" ? "test" : "review";
      d = arcD(key);
      label = key === "plan" ? "ajustes" : key === "test" ? "falhas" : "problemas";
    } else {
      d = line(fromIdx < 0 ? 6 : geo.x[ORDER[fromIdx]], geo.x[latest.role]);
    }
    void travel(d, label, back);
  });

  async function travel(d: string, label: string, back: boolean) {
    if (!movePath || !packet) return;
    movePath.setAttribute("d", d);
    packetLabel = label;
    packetBack = back;
    flowing = d;
    const length = movePath.getTotalLength();
    const el = packet;
    const half = () => el.offsetWidth / 2;
    const place = (p: number, scale: number, opacity: number) => {
      const pt = movePath!.getPointAtLength(p * length);
      const x = Math.max(half() + 2, Math.min(geo.w - half() - 2, pt.x));
      el.style.transform = `translate(${f(x - half())}px, ${f(pt.y - 11)}px) scale(${scale.toFixed(3)})`;
      el.style.opacity = String(opacity);
    };
    const ease = (p: number) => (p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2);
    const animate = (ms: number, step: (p: number) => void) =>
      new Promise<void>((done) => {
        const t0 = performance.now();
        const frame = (t: number) => {
          const p = Math.min(1, (t - t0) / ms);
          step(p);
          if (p < 1) requestAnimationFrame(frame);
          else done();
        };
        requestAnimationFrame(frame);
      });
    if (reduce) {
      place(1, 1, 1);
      await new Promise((r) => setTimeout(r, 600));
    } else {
      await animate(170, (p) => place(0, 0.6 + 0.4 * p, p));
      await animate(480 + length * 2, (p) => place(ease(p), 1, 1));
      await animate(170, (p) => place(1, 1 - 0.4 * p, 1 - p));
    }
    el.style.opacity = "0";
    flowing = null;
  }

  const glyphFor = (visual: Visual, gate: boolean) => (gate && visual === "waiting" ? "you" : VISUAL[visual].glyph);
</script>

<div class="scroll">
  <div class="stage" bind:this={stage}>
    <svg class="rails" aria-hidden="true">
      {#if geo.w}
        <path class="rail" class:done={reached("triage")} d={line(6, geo.x.triage)} />
        {#each ORDER.slice(1) as role, i (role)}
          {@const a = geo.x[ORDER[i]]}
          {@const b = geo.x[role]}
          {@const m = (a + b) / 2}
          <path class="rail" class:done={reached(role)} d={line(a, b)} />
          <path class="chev" class:done={reached(role)} d={`M${f(m - 2.5)} ${f(geo.y - 4)}L${f(m + 1.5)} ${f(geo.y)}L${f(m - 2.5)} ${f(geo.y + 4)}`} />
        {/each}
        <path class="rail" class:done={detail.status === "done"} d={line(geo.x.reporter, geo.w - 6)} />
        <circle class="end" cx="6" cy={f(geo.y)} r="2.5" />
        <circle class="end" cx={f(geo.w - 6)} cy={f(geo.y)} r="2.5" />
        {#each ["plan", "review", "test"] as const as k (k)}
          <path class="arc" class:used={arcs[k] > 0} d={arcD(k)} />
          <path class="tip" class:used={arcs[k] > 0} d={arrowHead(geo.x[ARCS[k].to] + ARCS[k].dx)} />
        {/each}
        {#if flowing}<path class="flow" class:back={packetBack} d={flowing} />{/if}
      {/if}
      <path bind:this={movePath} class="hidden-path" d="M0 0" />
    </svg>
    {#if geo.w}
      {#each [["plan", arcs.plan ? `ajustes ${arcs.plan}` : "ajustes"], ["review", `voltas ${Math.min(arcs.review + arcs.test, detail.max_rounds)}/${detail.max_rounds}`], ["test", "reprova"]] as const as [k, text] (k)}
        {@const p = arcMid(k)}
        <span class="arc-label" class:used={arcs[k] > 0} style:transform={`translate(calc(${f(p.x)}px - 50%), calc(${f(p.y)}px - 50%))`}>{text}</span>
      {/each}
      <span class="arc-label end-label" style:transform={`translate(0px, calc(${f(geo.y - 19)}px - 50%))`}>{detail.jira_key ?? "pedido"}</span>
      <span class="arc-label end-label" style:transform={`translate(calc(${f(geo.w)}px - 100%), calc(${f(geo.y - 19)}px - 50%))`}>relatório</span>
    {/if}

    <div class="grid">
      {#each cards as c (c.station.role)}
        <button
          type="button"
          class="station s-{c.visual}"
          class:sel={selected === c.station.role}
          aria-label={`${c.station.name}: ${c.label}. Abrir detalhes`}
          onclick={() => onselect(c.station.role)}
        >
          <span class="node" class:gate={c.station.gate} bind:this={nodes[c.station.role]}>
            {@html icon(glyphFor(c.visual, !!c.station.gate), c.station.gate ? 13 : 14, 2.4)}
          </span>
          <span class="card-s">
            <span class="name">{c.station.name}</span>
            {#if c.agent}
              <span class="role agent" title={`Agente ${c.agent} · ${c.station.perm}`}>Agente {c.agent}</span>
            {:else}
              <span class="role" title={c.station.model ? `${c.station.model} · ${c.station.perm}` : c.station.perm}>{c.station.perm.charAt(0).toUpperCase() + c.station.perm.slice(1)}</span>
            {/if}
            <span class="model" title={c.model ? `Modelo: ${c.model}${c.effort ? ` · esforço ${c.effort}` : ""}` : undefined}>{c.model ?? "\u00a0"}{c.effort ? ` · ${c.effort}` : ""}</span>
            <span class="chip c-{c.visual}"><i></i><span class="ctx">{c.label}</span></span>
            <span class="foot">
              <span class="acct">{#if c.account}{#if c.account !== "você"}<span class="dot ok"></span>{/if}<span class="ellipsis">{c.account}</span>{/if}</span>
              <span class="time">{c.time}</span>
            </span>
          </span>
        </button>
      {/each}
    </div>

    <div class="packet" class:back={packetBack} bind:this={packet} aria-hidden="true">
      {@html icon(packetBack ? "back" : "doc", 11, 2.2)}<span>{packetLabel}</span>
    </div>
  </div>
</div>

<style>
  .scroll { overflow-x: auto; overflow-y: hidden; }
  .stage { position: relative; min-width: 940px; padding: 58px 0 12px; }
  .rails { position: absolute; inset: 0; width: 100%; height: 100%; pointer-events: none; overflow: visible; }
  .rail { fill: none; stroke: var(--border-strong); stroke-width: 2; stroke-linecap: round; transition: stroke 0.3s, opacity 0.3s; }
  .rail.done { stroke: var(--ok); opacity: 0.55; }
  .chev { fill: none; stroke: var(--border-strong); stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  .chev.done { stroke: var(--ok); opacity: 0.7; }
  .end { fill: var(--border-strong); }
  .arc { fill: none; stroke: var(--border-strong); stroke-width: 1.5; stroke-linecap: round; stroke-dasharray: 3 5; }
  .arc.used { stroke: var(--danger-text); stroke-dasharray: none; opacity: 0.5; }
  .tip { fill: var(--border-strong); }
  .tip.used { fill: var(--danger-text); opacity: 0.6; }
  .flow { fill: none; stroke: var(--accent-text); stroke-width: 2; stroke-linecap: round; stroke-dasharray: 6 8; animation: march 0.5s linear infinite; }
  .flow.back { stroke: var(--danger-text); }
  .hidden-path { fill: none; stroke: none; }
  @keyframes march { to { stroke-dashoffset: -14; } }
  .arc-label { position: absolute; left: 0; top: 0; z-index: 1; padding: 0 6px; border-radius: 4px; background: var(--bg-side); font-family: var(--mono); font-size: 10.5px; line-height: 1.5; color: var(--faint); white-space: nowrap; }
  .arc-label.used { color: var(--danger-text); }

  .grid { position: relative; z-index: 1; display: grid; grid-template-columns: repeat(7, minmax(0, 1fr)); gap: 12px; }
  .station { display: flex; flex-direction: column; align-items: center; gap: 10px; min-width: 0; padding: 0; border: 0; border-radius: 10px; background: none; color: inherit; text-align: left; }
  .node { position: relative; z-index: 2; display: grid; place-items: center; width: 28px; height: 28px; border: 1.5px solid var(--border-strong); border-radius: 50%; background: var(--bg-side); color: var(--faint); transition: background 0.25s, border-color 0.25s, color 0.25s; }
  .node :global(svg) { display: block; }
  .node.gate { width: 23px; height: 23px; margin: 2.5px; border-radius: 6px; transform: rotate(45deg); }
  .node.gate :global(svg) { transform: rotate(-45deg); }
  .s-working .node { border-color: transparent; color: var(--accent-text); }
  .s-working .node::before { content: ""; position: absolute; inset: -1.5px; border: 2px solid var(--border-strong); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
  .s-working .node::after { content: ""; position: absolute; inset: -1.5px; border-radius: 50%; animation: halo 1.6s ease-out infinite; }
  .s-done .node { background: var(--ok-bg); border-color: rgba(88, 184, 138, 0.7); color: var(--ok-text); }
  .s-returned .node { border-color: rgba(239, 164, 155, 0.75); color: var(--danger-text); }
  .s-failed .node { background: var(--danger-bg); border-color: var(--danger-text); color: var(--danger-text); }
  .s-skipped .node { border-style: dashed; border-color: var(--faint); }
  .s-you .node { background: var(--warn-bg); border-color: var(--warn); color: var(--warn-text); }
  .s-you .node::after { content: ""; position: absolute; inset: -1.5px; border-radius: inherit; animation: halo-you 1.6s ease-in-out infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @keyframes halo { 0% { box-shadow: 0 0 0 0 rgba(91, 106, 217, 0.5); } 100% { box-shadow: 0 0 0 10px rgba(91, 106, 217, 0); } }
  @keyframes halo-you { 50% { box-shadow: 0 0 0 5px rgba(217, 164, 65, 0.22); } }

  .card-s { position: relative; display: flex; flex-direction: column; align-items: flex-start; width: 100%; min-width: 0; padding: 9px 9px 8px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); transition: border-color 0.2s, background 0.2s; }
  .station:hover .card-s { border-color: var(--border-strong); background: var(--surface-2); }
  .s-working .card-s { border-color: rgba(91, 106, 217, 0.7); background: var(--surface-2); }
  .s-you .card-s { border-color: rgba(217, 164, 65, 0.55); }
  .s-failed .card-s { border-color: rgba(239, 164, 155, 0.45); }
  .s-skipped .card-s { border-style: dashed; opacity: 0.6; }
  .station.sel .card-s { border-color: var(--muted); }
  .station.sel.s-working .card-s { border-color: var(--accent-text); }
  .station.sel.s-you .card-s { border-color: var(--warn); }
  .station.sel .card-s::after { content: ""; position: absolute; left: 50%; bottom: -5px; width: 8px; height: 8px; margin-left: -4px; border-right: 1px solid; border-bottom: 1px solid; border-color: inherit; background: inherit; transform: rotate(45deg); }
  .name { max-width: 100%; font-family: var(--display); font-size: 13px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .s-waiting .name { color: var(--text-2); }
  .role.agent { color: var(--accent-text); }
  .model { max-width: 100%; margin: -4px 0 6px; font-family: var(--mono); font-size: 10.5px; color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .role { max-width: 100%; margin-bottom: 6px; font-size: 11.5px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .chip { display: inline-flex; align-items: center; gap: 5px; max-width: 100%; padding: 1px 7px 1px 6px; border: 1px solid transparent; border-radius: 999px; font-size: 11px; font-weight: 600; line-height: 1.5; white-space: nowrap; overflow: hidden; }
  .chip i { flex: none; width: 6px; height: 6px; border-radius: 50%; background: currentColor; }
  .ctx { overflow: hidden; text-overflow: ellipsis; }
  .c-waiting { color: var(--muted); padding-left: 0; }
  .c-waiting i { background: transparent; box-shadow: inset 0 0 0 1.25px var(--faint); }
  .c-working { color: var(--accent-text); background: rgba(91, 106, 217, 0.16); }
  .c-working i { animation: blink 1.2s ease-in-out infinite; }
  .c-done { color: var(--ok-text); background: var(--ok-bg); }
  .c-returned { color: var(--danger-text); border-color: rgba(239, 164, 155, 0.45); }
  .c-failed { color: var(--danger-text); background: var(--danger-bg); }
  .c-skipped { color: var(--muted); border: 1px dashed var(--border-strong); }
  .c-you { color: var(--warn-text); background: var(--warn-bg); }
  .c-you i { background: var(--warn); }
  @keyframes blink { 50% { opacity: 0.35; } }
  .foot { display: flex; align-items: center; justify-content: space-between; gap: 6px; width: 100%; min-height: 18px; margin-top: 6px; font-size: 11.5px; color: var(--muted); }
  .acct { display: inline-flex; align-items: center; gap: 6px; min-width: 0; overflow: hidden; white-space: nowrap; }
  .ellipsis { overflow: hidden; text-overflow: ellipsis; }
  .dot { width: 7px; height: 7px; flex: none; border-radius: 50%; background: var(--ok); }
  .time { flex: none; font-family: var(--mono); font-size: 11px; font-variant-numeric: tabular-nums; }

  .packet { position: absolute; left: 0; top: 0; z-index: 3; display: inline-flex; align-items: center; gap: 5px; height: 22px; padding: 0 8px 0 6px; border-radius: 6px; background: var(--accent); color: #fff; font-family: var(--mono); font-size: 10.5px; font-weight: 500; white-space: nowrap; pointer-events: none; opacity: 0; box-shadow: 0 2px 8px rgba(0, 0, 0, 0.45); }
  .packet :global(svg) { display: block; flex: none; }
  .packet.back { background: var(--danger-bg); color: var(--danger-text); box-shadow: inset 0 0 0 1px rgba(239, 164, 155, 0.6), 0 2px 8px rgba(0, 0, 0, 0.45); }

  @media (prefers-reduced-motion: reduce) {
    .stage *, .stage *::before, .stage *::after { animation: none !important; transition: none !important; }
  }
</style>
