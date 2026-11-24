<script lang="ts" module>
  /** O trabalho passando de uma estação a outra, enquanto o pacote anda. */
  export interface Transit {
    from: string;
    to: string;
    label: string;
    back: boolean;
  }
</script>

<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { chat } from "../../state/chat.svelte";
  import {
    icon, isActive, isMarker, isUserStep, outputSummary, passesByRole, returnArc, roleIcon, STATION, stationPasses, stationRole, STATIONS, VISUAL, type Visual,
  } from "../../pipeline";
  import { liveTick, loadTierModels, stationCards, type StationCard } from "../../pipeline-cards.svelte";
  import type { PipelineDetail, PipelineRole } from "../../types";

  interface Props {
    detail: PipelineDetail;
    /** Estação aberta no painel. */
    selected: PipelineRole | null;
    /** Instante atual (s), para o tempo das etapas em andamento. */
    now: number;
    onselect: (role: PipelineRole) => void;
    /** Avisado quando o pacote sai (com o que leva) e quando chega (`null`). */
    ontransit?: (transit: Transit | null) => void;
  }
  let { detail, selected, now, onselect, ontransit }: Props = $props();

  const ORDER = STATIONS.map((s) => s.role);
  const FORWARD_LABEL: Partial<Record<PipelineRole, string>> = {
    triage: "task", planner: "task triada", approval: "plano", implementer: "plano aprovado",
    reviewer: "diff", tester: "diff aprovado", code_review: "verificado", reporter: "revisado",
  };

  const passes = $derived(passesByRole(detail));

  onMount(loadTierModels);
  const cards = $derived(stationCards(detail, now, passes));

  const returnedBy = (role: PipelineRole) => passes[role].filter((s) => s.status === "returned").length;
  /** Devoluções do Revisor ou do Testador na contagem atual de voltas (o ajuste e a retomada a recomeçam). */
  const returnedSinceMarker = (role: PipelineRole) => {
    const start = detail.stages.findLastIndex((s) => isMarker(s.role)) + 1;
    return detail.stages.slice(start).filter((s) => s.role === role && s.status === "returned").length;
  };
  const arcs = $derived({
    // O ajuste no design volta à mesma estação (o Designer fica na do Planejador): a mesma seta.
    plan: returnedBy("approval") + returnedBy("design_approval"),
    review: returnedSinceMarker("reviewer"),
    test: returnedSinceMarker("tester"),
    fix: returnedBy("review_choice"),
  });
  /** Trilho até a estação já percorrido. */
  const reached = (role: PipelineRole): boolean => {
    if (stationPasses(detail, role).length > 0) return true;
    // Estação desligada: o trilho conta como percorrido quando o trabalho já passou adiante.
    const i = ORDER.indexOf(role);
    return detail.skipped.includes(role) && (detail.status === "done" || ORDER.slice(i + 1).some((r) => passes[r].length > 0));
  };
  /** Etapa que entregou o trabalho à de índice `i` (a retomada e o ajuste não contam). */
  const deliveredBy = (i: number) => detail.stages.slice(0, i).findLast((s) => !isMarker(s.role)) ?? null;
  const stationIndex = (role: PipelineRole) => ORDER.indexOf(stationRole(role) ?? role);

  /** A seta de devolução em uso agora: o trabalho voltando por ela, a etapa que ela trouxe de volta ou o
   *  limite de voltas. */
  const hotArc = $derived.by(() => {
    if (flowArc) return flowArc;
    const stages = detail.stages;
    if (detail.status === "failed" && detail.round_limit) {
      const by = stages.findLast((s) => (s.role === "reviewer" || s.role === "tester") && s.status === "returned");
      return by ? returnArc(by.role) : null;
    }
    const cur = stages.at(-1);
    if (!cur || cur.status !== "running" || isMarker(cur.role) || stages.at(-2)?.role === "feedback") return null;
    const before = deliveredBy(stages.length - 1);
    return before?.status === "returned" && stationIndex(before.role) > stationIndex(cur.role) ? returnArc(before.role) : null;
  });

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
    fix: { from: "code_review", to: "implementer", dx: 0, h: 60 },
  } as const;
  const arcD = (k: keyof typeof ARCS) => arcPath(geo.x[ARCS[k].from], geo.x[ARCS[k].to] + ARCS[k].dx, ARCS[k].h);
  const arcMid = (k: keyof typeof ARCS) => ({
    x: (geo.x[ARCS[k].from] + geo.x[ARCS[k].to] + ARCS[k].dx) / 2,
    y: geo.y - 14 - ARCS[k].h,
  });

  /** Cometa que corre pelo trilho da estação anterior até a etapa que trabalha agora. */
  const comet = $derived.by(() => {
    const stages = detail.stages;
    const cur = stages.at(-1);
    if (!geo.w || flowing || !cur || cur.status !== "running" || isMarker(cur.role)) return null;
    const to = geo.x[stationRole(cur.role) ?? cur.role];
    const before = deliveredBy(stages.length - 1);
    let d: string;
    if (stages.at(-2)?.role === "feedback") d = line(geo.w - 6, to);
    else if (!before) d = line(6, to);
    else if (stationIndex(before.role) < stationIndex(cur.role)) d = line(geo.x[ORDER[stationIndex(before.role)]], to);
    else return null;
    return { d, you: isUserStep(cur.role) };
  });

  // ---- pacote: leva o trabalho de uma estação à outra quando uma etapa nova começa

  let movePath = $state<SVGPathElement>();
  let packet = $state<HTMLDivElement>();
  let packetLabel = $state("");
  let packetBack = $state(false);
  let flowing = $state<string | null>(null);
  let flowArc = $state<keyof typeof ARCS | null>(null);
  let flowMs = $state(900);
  let flowSeq = $state(0);
  /** Chegadas em cada estação: cada uma dispara a onda em volta do nó. */
  let bursts = $state<Record<string, number>>({});
  let seenRun: string | null = null;
  let seenCount = 0;

  function arrive(station: PipelineRole) {
    untrack(() => (bursts[station] = (bursts[station] ?? 0) + 1));
  }

  $effect(() => {
    const count = detail.stages.length;
    const runId = detail.id;
    if (runId !== seenRun) {
      seenRun = runId;
      seenCount = count;
      return;
    }
    if (count <= seenCount) return;
    const latest = detail.stages[count - 1];
    seenCount = count;
    // O ajuste e a retomada não são estação: o pacote sai quando o Implementador começa.
    if (isMarker(latest.role)) {
      seenCount = count - 1;
      return;
    }
    const station = stationRole(latest.role) ?? latest.role;
    // Retomada no limite de voltas: o trabalho volta de quem devolveu, pela seta de sempre.
    const before = detail.stages[count - 2]?.role === "resume" ? detail.stages.slice(0, count - 2).findLast((s) => !isMarker(s.role)) : detail.stages[count - 2];
    // A sua escolha fica na estação do Code review: passar de um para o outro não move o pacote.
    if (!geo.w || (before && stationRole(before.role) === station)) {
      arrive(station);
      return;
    }
    const fromIdx = before ? ORDER.indexOf(stationRole(before.role) ?? before.role) : -1;
    const toIdx = ORDER.indexOf(station);
    let d: string;
    let back = false;
    let arc: keyof typeof ARCS | null = null;
    let label = fromIdx < 0 ? "task" : (FORWARD_LABEL[station] ?? "");
    let from = fromIdx < 0 ? "Início da esteira" : STATION[ORDER[fromIdx]].name;
    if (before?.role === "feedback") {
      // Reaberta com o seu ajuste: o trabalho volta do fim da esteira ao Implementador.
      back = true;
      d = line(geo.w - 6, geo.x[station]);
      label = "seu ajuste";
      from = "Fim da esteira";
    } else if (fromIdx > toIdx && before) {
      back = true;
      arc = returnArc(before.role);
      d = arcD(arc);
      label = { plan: "ajustes", test: "falhas", fix: "correções", review: "problemas" }[arc];
    } else {
      d = line(fromIdx < 0 ? 6 : geo.x[ORDER[fromIdx]], geo.x[station]);
    }
    void travel(d, label, back, arc, { from, to: STATION[station].name }, station);
  });

  async function travel(d: string, label: string, back: boolean, arc: keyof typeof ARCS | null, ends: { from: string; to: string }, station: PipelineRole) {
    if (!movePath || !packet) return;
    movePath.setAttribute("d", d);
    packetLabel = label;
    packetBack = back;
    flowArc = arc;
    const length = movePath.getTotalLength();
    const ms = Math.round(Math.min(1900, Math.max(900, 640 + length * 1.1)));
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
    ontransit?.({ ...ends, label, back });
    // Sem atalho de movimento reduzido: o Windows o liga sozinho com "Efeitos de animação" desligado (comum em PC
    // sem placa de vídeo) e o pacote pulava direto para o destino. São três animações curtas, leves em qualquer máquina.
    await animate(170, (p) => place(0, 0.6 + 0.4 * p, p));
    flowMs = ms;
    flowing = d;
    flowSeq += 1;
    await animate(ms, (p) => place(ease(p), 1, 1));
    arrive(station);
    await animate(170, (p) => place(1, 1 - 0.4 * p, 1 - p));
    el.style.opacity = "0";
    flowArc = null;
    ontransit?.(null);
    // O rastro some sozinho logo depois da chegada.
    await animate(450, () => {});
    if (flowing === d) flowing = null;
  }

  const glyphFor = (visual: Visual, gate: boolean) => (gate && visual === "waiting" ? "you" : VISUAL[visual].glyph);

  // ---- estações

  const selStation = $derived(selected ? stationRole(selected) : null);
  const lensIdx = $derived(selStation ? ORDER.indexOf(selStation) : -1);
  const active = $derived(isActive(detail.status));

  /** A quinta linha do cartão: o que faz agora ou o que entregou. */
  function actOf(c: StationCard): { text: string; tone: string; key: string; title?: string } {
    const last = c.last;
    if (!last) return { text: c.off ? "Desligada neste projeto" : active ? "Ainda não recebeu o trabalho" : "Não rodou nesta execução", tone: "dim", key: "none" };
    if (last.status === "running") {
      if (isUserStep(last.role)) {
        const text = last.role === "review_choice" ? "Escolha o que corrigir dos achados" : last.role === "design_approval" ? "O design espera a sua decisão" : "O plano espera a sua decisão";
        return { text, tone: "you", key: last.id };
      }
      if (chat.permissions[last.conversation_id]) return { text: "Pede sua autorização para continuar", tone: "you", key: `${last.id}-ask` };
      const t = liveTick(last.conversation_id);
      const text = t.target ? `${t.verb} ${t.target}` : t.verb;
      return { text, tone: "live", key: `${last.id}-${text}`, title: t.title };
    }
    let text = outputSummary(last);
    if (last.role === "review_choice") {
      const review = passes.code_review.at(-1);
      if (review) text = `${outputSummary(review)} · ${text}`;
    }
    return { text, tone: c.visual === "returned" || c.visual === "failed" ? "bad" : "", key: `${last.id}-${last.status}` };
  }
  const acts = $derived(cards.map(actOf));
  const counts = $derived(Object.fromEntries(ORDER.map((r) => [r, stationPasses(detail, r).length])) as Record<PipelineRole, number>);
</script>

<div class="scroll">
  <div class="stage" bind:this={stage}>
    <span class="beam" aria-hidden="true"><i></i></span>
    <svg class="rails" aria-hidden="true">
      {#if geo.w}
        <path class="rail" class:done={reached("triage")} d={line(6, geo.x.triage)} pathLength="1" style:animation-delay="220ms" />
        {#each ORDER.slice(1) as role, i (role)}
          {@const a = geo.x[ORDER[i]]}
          {@const b = geo.x[role]}
          {@const m = (a + b) / 2}
          <path class="rail" class:done={reached(role)} d={line(a, b)} pathLength="1" style:animation-delay={`${310 + i * 90}ms`} />
          <path class="chev" class:done={reached(role)} d={`M${f(m - 2.5)} ${f(geo.y - 4)}L${f(m + 1.5)} ${f(geo.y)}L${f(m - 2.5)} ${f(geo.y + 4)}`} style:animation-delay={`${610 + i * 90}ms`} />
        {/each}
        <path class="rail" class:done={detail.status === "done"} d={line(geo.x.reporter, geo.w - 6)} pathLength="1" style:animation-delay="940ms" />
        <circle class="end" cx="6" cy={f(geo.y)} r="2.5" />
        <circle class="end" cx={f(geo.w - 6)} cy={f(geo.y)} r="2.5" />
        {#each ["plan", "review", "test", "fix"] as const as k (k)}
          <path class="arc" class:used={arcs[k] > 0} class:hot={hotArc === k} d={arcD(k)} />
          <path class="tip" class:used={arcs[k] > 0} class:hot={hotArc === k} d={arrowHead(geo.x[ARCS[k].to] + ARCS[k].dx)} />
        {/each}
        {#if comet}
          <path class="comet-g" class:you={comet.you} d={comet.d} pathLength="1" />
          <path class="comet" class:you={comet.you} d={comet.d} pathLength="1" />
        {/if}
        {#if flowing}
          {#key flowSeq}<path class="trail" class:back={packetBack} d={flowing} pathLength="1" style:--ms={`${flowMs}ms`} />{/key}
        {/if}
      {/if}
      <path bind:this={movePath} class="hidden-path" d="M0 0" />
    </svg>
    {#if geo.w}
      {#each [["plan", arcs.plan ? `ajustes ${arcs.plan}` : "ajustes"], ["review", `voltas ${detail.rounds}/${detail.max_rounds}`], ["test", "reprova"], ["fix", arcs.fix ? `correções ${arcs.fix}` : "correções"]] as const as [k, text] (k)}
        {@const p = arcMid(k)}
        <span class="arc-label" class:used={arcs[k] > 0} class:hot={hotArc === k} style:left={`${f(p.x)}px`} style:top={`${f(p.y)}px`}>{#key text}<span class="al-t">{text}</span>{/key}</span>
      {/each}
      <span class="end-label" style:left="0px" style:top={`${f(geo.y - 19)}px`}>{detail.jira_key ?? "pedido"}</span>
      <span class="end-label" style:right="0px" style:top={`${f(geo.y - 19)}px`}>relatório</span>
    {/if}
    <span class="lens l-{lensIdx >= 0 ? cards[lensIdx].visual : 'waiting'}" class:hide={lensIdx < 0} style:--li={Math.max(0, lensIdx)} aria-hidden="true"><i></i></span>

    <div class="grid">
      {#each cards as c, i (c.station.role)}
        {@const role = c.station.role}
        {@const act = acts[i]}
        <button
          type="button"
          class="station s-{c.visual}"
          class:sel={lensIdx === i}
          aria-label={`${c.station.name}: ${c.label}. Abrir detalhes`}
          aria-pressed={lensIdx === i}
          onclick={() => onselect(role)}
        >
          <span class="node" class:gate={c.station.gate} style:animation-delay={`${300 + i * 90}ms`} bind:this={nodes[role]}>
            {@html c.visual === "working" ? roleIcon(role, 16, 2) : icon(glyphFor(c.visual, !!c.station.gate), c.station.gate ? 13 : 15, 2.4)}
            {#key bursts[role]}{#if bursts[role]}<span class="burst"></span>{/if}{/key}
          </span>
          <span class="sc" style:animation-delay={`${380 + i * 70}ms`}>
            <span class="sc-h">
              <span class="ri">{@html roleIcon(role, 15)}</span>
              <span class="sc-name">{c.station.name}</span>
              {#if counts[role] > 1}{#key counts[role]}<span class="sc-count" title={`${counts[role]} passagens`}>{counts[role]}×</span>{/key}{/if}
            </span>
            {#if c.agent}
              <span class="sc-perm agent" title={`Agente ${c.agent} · ${c.station.perm}`}>Agente {c.agent}</span>
            {:else}
              <span class="sc-perm" title={c.station.model ? `${c.station.model} · ${c.station.perm}` : c.station.perm}>{c.station.perm.charAt(0).toUpperCase() + c.station.perm.slice(1)}</span>
            {/if}
            <span class="sc-model" title={c.model ? `Modelo: ${c.model}${c.effort ? ` · esforço ${c.effort}` : ""}` : undefined}>{c.model ?? " "}{c.effort ? ` · ${c.effort}` : ""}</span>
            <span class="chip c-{c.visual}"><i></i><span class="cl">{c.label}</span></span>
            <span class="sc-act">{#key act.key}<span class="a-line {act.tone}" title={act.title ?? act.text}>{act.text}</span>{/key}</span>
            <span class="sc-foot">
              <span class="acct">{#if c.account}{#if c.account !== "você"}<span class="adot"></span>{/if}<span class="ell">{c.account}</span>{/if}</span>
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
  .scroll { position: relative; padding: 0 16px 8px; overflow-x: auto; overflow-y: hidden; }
  .stage { position: relative; min-width: 1060px; padding: 96px 0 12px; }
  .rails { position: absolute; left: 0; top: 0; z-index: 1; width: 100%; height: 100%; pointer-events: none; overflow: visible; }
  .rail { fill: none; stroke: var(--border-strong); stroke-width: 2; stroke-linecap: round; transition: stroke 0.6s, opacity 0.6s; animation: rail-in 0.8s var(--ease) backwards; }
  .rail.done { stroke: var(--ok); opacity: 0.6; }
  .chev { fill: none; stroke: var(--border-strong); stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; transition: stroke 0.6s, opacity 0.6s; animation: fade-in 0.4s ease backwards; }
  .chev.done { stroke: var(--ok); opacity: 0.75; }
  .end { fill: var(--border-strong); }
  .arc { fill: none; stroke: var(--border-strong); stroke-width: 1.5; stroke-linecap: round; stroke-dasharray: 3 5; transition: stroke 0.5s, opacity 0.5s, stroke-width 0.5s; animation: fade-in 0.7s ease 1.05s backwards; }
  .arc.used { stroke: var(--bad); stroke-dasharray: none; opacity: 0.5; }
  .arc.hot { stroke: var(--bad); stroke-dasharray: none; opacity: 1; stroke-width: 2.2; }
  .tip { fill: var(--border-strong); transition: fill 0.5s, opacity 0.5s; animation: fade-in 0.7s ease 1.05s backwards; }
  .tip.used { fill: var(--bad); opacity: 0.65; }
  .tip.hot { fill: var(--bad); opacity: 1; }
  .comet, .comet-g { fill: none; stroke-linecap: round; stroke-dasharray: 0.16 1.84; stroke-dashoffset: 0.16; animation: comet 2.4s var(--inout) infinite; }
  .comet { stroke: var(--acc-2); stroke-width: 2.6; }
  .comet-g { stroke: var(--accent); stroke-width: 9; opacity: 0.22; }
  .comet.you, .comet-g.you { stroke: var(--warn); }
  .trail { fill: none; stroke: var(--acc-2); stroke-width: 2.4; stroke-linecap: round; animation: rail-in var(--ms) var(--inout) backwards, fade-out 0.6s ease calc(var(--ms) + 120ms) forwards; }
  .trail.back { stroke: var(--bad); }
  .hidden-path { fill: none; stroke: none; }
  .arc-label { position: absolute; z-index: 3; transform: translate(-50%, -50%); padding: 0 8px; border: 1px solid var(--border); border-radius: 999px; background: var(--bg-side); font: 500 10.5px/1.7 var(--mono); color: var(--muted); white-space: nowrap; transition: color 0.4s, border-color 0.4s, background-color 0.45s; animation: fade-in 0.6s ease 1.2s backwards; }
  .arc-label.used { color: var(--danger-text); border-color: var(--bad-line); }
  .arc-label.hot { color: var(--danger-text); border-color: var(--bad); background: var(--bad-solid); }
  .al-t { display: inline-block; animation: bump 0.55s var(--spring); }
  .end-label { position: absolute; z-index: 3; transform: translateY(-50%); padding: 0 4px; background: var(--bg-side); font: 500 10.5px/1.6 var(--mono); color: var(--muted); }

  /* Facho do cartão Agora até a estação ativa, e a lente sob a estação aberta no painel (só brilho: a borda é a do cartão). */
  .beam { position: absolute; left: 0; top: 0; z-index: 0; width: 12.5%; height: 96px; display: flex; justify-content: center; pointer-events: none; transform: translateX(calc(var(--bi, 0) * 100%)); transition: transform var(--bms, 700ms) var(--inout); }
  .beam i { display: block; width: 72px; height: 100%; background: linear-gradient(to bottom, var(--hc-bg, transparent), transparent 92%); clip-path: polygon(40% 0, 60% 0, 100% 100%, 0 100%); }
  .lens { position: absolute; left: 0; top: 140px; bottom: 12px; z-index: 0; width: 12.5%; padding: 0 3px; pointer-events: none; transform: translateX(calc(var(--li) * 100%)); transition: transform 0.6s var(--spring), opacity 0.3s; --lg: transparent; }
  .lens i { display: block; height: 100%; border-radius: 15px; box-shadow: 0 16px 36px -14px var(--lg); transition: box-shadow 0.45s; }
  .lens.hide { opacity: 0; }
  .lens.l-working { --lg: var(--acc-glow); }
  .lens.l-you { --lg: var(--warn-glow); }
  .lens.l-done { --lg: var(--ok-glow); }
  .lens.l-returned, .lens.l-failed { --lg: var(--bad-glow); }

  .grid { position: relative; z-index: 2; display: grid; grid-template-columns: repeat(8, minmax(0, 1fr)); }
  .station { position: relative; display: flex; flex-direction: column; align-items: center; gap: 12px; min-width: 0; padding: 0 6px; border: 0; background: none; color: inherit; text-align: left; }
  .station:focus-visible { outline: none; }
  .station:focus-visible .sc { outline: 2px solid var(--accent-text); outline-offset: 2px; }
  .node { position: relative; z-index: 2; width: 32px; height: 32px; flex: none; display: grid; place-items: center; border: 1.5px solid var(--border-strong); border-radius: 50%; background: var(--bg-side); color: var(--faint); transition: background-color 0.45s, border-color 0.45s, color 0.45s, box-shadow 0.45s; animation: pop 0.55s var(--spring) backwards; }
  .node :global(svg) { display: block; }
  .node.gate { width: 25px; height: 25px; margin: 3.5px; border-radius: 7px; transform: rotate(45deg); animation-name: pop-gate; }
  .node.gate :global(svg) { transform: rotate(-45deg); }
  .s-done .node { background: var(--ok-solid); border-color: var(--ok-line); color: var(--ok-text); }
  .s-returned .node { background: var(--bad-solid); border-color: var(--bad-line); color: var(--danger-text); }
  .s-failed .node { background: var(--bad-solid); border-color: var(--bad); color: var(--danger-text); }
  .s-skipped .node { border-style: dashed; }
  .s-you .node { background: var(--warn-solid); border-color: var(--warn); color: var(--warn-text); }
  .s-you .node::after { content: ""; position: absolute; inset: -2px; border-radius: inherit; animation: halo-you 1.8s ease-in-out infinite; }
  .s-working .node { background: var(--acc-solid); border-color: var(--accent); color: var(--accent-text); box-shadow: 0 0 22px -2px var(--acc-glow); }
  .s-working .node::before { content: ""; position: absolute; inset: -7px; padding: 2px; border-radius: 50%; background: conic-gradient(from var(--ang), transparent 0 55%, var(--acc-2) 85%, transparent 100%); -webkit-mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0); -webkit-mask-composite: xor; mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0); mask-composite: exclude; animation: spin-ang 1.5s linear infinite; }
  .s-working .node::after { content: ""; position: absolute; inset: -2px; border-radius: 50%; animation: halo 2.4s var(--ease) infinite; }
  .burst { position: absolute; inset: -2px; border: 2px solid var(--acc-2); border-radius: inherit; pointer-events: none; animation: burst 0.9s var(--ease) both; }
  .s-you .burst { border-color: var(--warn); }
  .s-returned .burst, .s-failed .burst { border-color: var(--bad); }

  .sc { position: relative; flex: 1; width: 100%; min-width: 0; display: flex; flex-direction: column; align-items: flex-start; padding: 10px 10px 9px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); transition: border-color 0.4s, background-color 0.45s, transform 0.35s var(--ease), box-shadow 0.4s; animation: rise 0.6s var(--ease) backwards; }
  .station:hover .sc { transform: translateY(-2px); border-color: var(--border-strong); }
  .s-working .sc { border-color: var(--acc-line); box-shadow: 0 16px 36px -20px var(--acc-glow); }
  .s-working .sc::before { content: ""; position: absolute; inset: -1px; padding: 1.5px; border-radius: inherit; background: conic-gradient(from var(--ang), transparent 0 72%, var(--acc-2) 86%, var(--accent-text) 90%, transparent 100%); -webkit-mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0); -webkit-mask-composite: xor; mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0); mask-composite: exclude; animation: spin-ang 3.6s linear infinite; pointer-events: none; }
  .s-you .sc { border-color: var(--warn-line); box-shadow: 0 16px 36px -22px var(--warn-glow); }
  .s-failed .sc { border-color: var(--bad-line); }
  .s-skipped .sc { border-style: dashed; opacity: 0.62; }
  /* A borda da estação aberta é a do próprio cartão, na cor do estado (a lente atrás só brilha). */
  .station.sel .sc { border-color: var(--border-strong); box-shadow: 0 0 0 0.5px var(--border-strong); }
  .station.sel.s-working .sc { border-color: var(--acc-2); box-shadow: 0 0 0 0.5px var(--acc-2), 0 16px 36px -20px var(--acc-glow); }
  .station.sel.s-you .sc { border-color: var(--warn); box-shadow: 0 0 0 0.5px var(--warn), 0 16px 36px -22px var(--warn-glow); }
  .station.sel.s-done .sc { border-color: var(--ok-line); box-shadow: 0 0 0 0.5px var(--ok-line); }
  .station.sel.s-returned .sc, .station.sel.s-failed .sc { border-color: var(--bad-line); box-shadow: 0 0 0 0.5px var(--bad-line); }
  .sc-h { display: flex; align-items: center; gap: 7px; width: 100%; min-width: 0; }
  .ri { flex: none; display: grid; place-items: center; color: var(--muted); transition: color 0.4s; }
  .ri :global(svg) { display: block; }
  .s-working .ri { color: var(--accent-text); }
  .s-you .ri { color: var(--warn-text); }
  .sc-name { min-width: 0; font: 600 13.5px/1.3 var(--display); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .s-waiting .sc-name { color: var(--text-2); }
  .sc-count { margin-left: auto; flex: none; padding: 0 5px; border: 1px solid var(--border); border-radius: 5px; background: var(--surface-2); font: 500 10px/1.6 var(--mono); color: var(--muted); animation: bump 0.55s var(--spring); }
  .sc-perm { margin-top: 5px; max-width: 100%; font-size: 11.5px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sc-perm.agent { color: var(--accent-text); }
  .sc-model { margin-top: 1px; max-width: 100%; font: 400 10.5px/1.5 var(--mono); color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .chip { display: inline-flex; align-items: center; gap: 5px; max-width: 100%; margin-top: 8px; padding: 1px 8px 1px 7px; border: 1px solid transparent; border-radius: 999px; font-size: 11px; font-weight: 600; line-height: 1.55; white-space: nowrap; overflow: hidden; transition: background-color 0.4s, color 0.4s, border-color 0.4s; }
  .chip i { flex: none; width: 6px; height: 6px; border-radius: 50%; background: currentColor; }
  .cl { overflow: hidden; text-overflow: ellipsis; }
  .c-waiting { color: var(--muted); padding-left: 0; }
  .c-waiting i { background: transparent; box-shadow: inset 0 0 0 1.25px var(--faint); }
  .c-working { color: var(--accent-text); background: var(--acc-bg); }
  .c-working i { animation: blink 1.2s ease-in-out infinite; }
  .c-done { color: var(--ok-text); background: var(--ok-bg); }
  .c-returned { color: var(--danger-text); border-color: var(--bad-line); }
  .c-failed { color: var(--danger-text); background: var(--danger-bg); }
  .c-skipped { color: var(--muted); border: 1px dashed var(--border-strong); }
  .c-you { color: var(--warn-text); background: var(--warn-bg); }
  .c-you i { background: var(--warn); animation: blink 1.4s ease-in-out infinite; }
  .sc-act { position: relative; width: 100%; height: 34px; margin-top: 8px; overflow: hidden; }
  .a-line { position: absolute; inset: 0; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; overflow-wrap: anywhere; font-size: 11.5px; line-height: 1.45; color: var(--text-2); }
  .a-line.live { color: var(--accent-text); animation: tick-in 0.45s var(--ease) both; }
  .a-line.you { color: var(--warn-text); }
  .a-line.bad { color: var(--danger-text); }
  .a-line.dim { color: var(--muted); }
  .sc-foot { display: flex; align-items: center; justify-content: space-between; gap: 6px; width: 100%; margin-top: auto; padding-top: 8px; font-size: 11.5px; color: var(--muted); }
  .acct { display: inline-flex; align-items: center; gap: 6px; min-width: 0; overflow: hidden; white-space: nowrap; }
  .adot { flex: none; width: 6px; height: 6px; border-radius: 50%; background: var(--ok); }
  .ell { overflow: hidden; text-overflow: ellipsis; }
  .time { flex: none; font: 500 11px var(--mono); font-variant-numeric: tabular-nums; }

  .packet { position: absolute; left: 0; top: 0; z-index: 6; display: inline-flex; align-items: center; gap: 5px; height: 23px; padding: 0 9px 0 7px; border-radius: 7px; background: var(--accent); color: #fff; font: 500 10.5px/1 var(--mono); white-space: nowrap; pointer-events: none; opacity: 0; box-shadow: 0 6px 18px -4px var(--acc-glow); }
  .packet :global(svg) { display: block; flex: none; }
  .packet.back { background: var(--bad-solid); color: var(--danger-text); box-shadow: inset 0 0 0 1px var(--bad-line), 0 6px 18px -4px var(--bad-glow); }

  @keyframes spin-ang { to { --ang: 360deg; } }
  @keyframes halo { 0% { box-shadow: 0 0 0 0 var(--acc-glow); } 75%, 100% { box-shadow: 0 0 0 16px rgba(0, 0, 0, 0); } }
  @keyframes halo-you { 0%, 100% { box-shadow: 0 0 0 0 var(--warn-glow); } 50% { box-shadow: 0 0 0 7px var(--warn-glow); } }
  @keyframes rail-in { from { stroke-dasharray: 0 1; } to { stroke-dasharray: 1 0; } }
  @keyframes comet { from { stroke-dashoffset: 0.16; } to { stroke-dashoffset: -1; } }
  @keyframes fade-in { from { opacity: 0; } to { opacity: 1; } }
  @keyframes fade-out { to { opacity: 0; } }
  @keyframes burst { from { opacity: 0.9; transform: scale(1); } to { opacity: 0; transform: scale(2.6); } }
  @keyframes rise { from { opacity: 0; transform: translateY(12px); } to { opacity: 1; transform: none; } }
  @keyframes pop { 0% { opacity: 0; transform: scale(0.4); } 70% { opacity: 1; transform: scale(1.12); } 100% { opacity: 1; transform: scale(1); } }
  @keyframes pop-gate { 0% { opacity: 0; transform: rotate(45deg) scale(0.4); } 70% { opacity: 1; transform: rotate(45deg) scale(1.12); } 100% { opacity: 1; transform: rotate(45deg) scale(1); } }
  @keyframes tick-in { from { opacity: 0; transform: translateY(100%); } to { opacity: 1; transform: none; } }
  @keyframes blink { 50% { opacity: 0.35; } }
  @keyframes bump { 0% { transform: scale(1); } 40% { transform: scale(1.3); } 100% { transform: scale(1); } }

  @media (max-width: 1100px) {
    .beam { display: none; }
  }
</style>
