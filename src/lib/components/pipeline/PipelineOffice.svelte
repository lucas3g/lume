<script lang="ts">
  import { onMount } from "svelte";
  import "../../office/office.css";
  import { officeClock } from "../../office/clock";
  import { layersSvg, OFFICE_H, OFFICE_W, officeScenery, officeTrip, officeView, spritesSvg, type OfficeScene } from "../../office/scene";
  import { passesByRole, passLabel, stationPasses, stationRole, STATIONS } from "../../pipeline";
  import { loadTierModels, stationCards, stationTime } from "../../pipeline-cards.svelte";
  import type { PipelineDetail, PipelineRole } from "../../types";

  /** Mesma interface da trilha (PipelineTrack): a execução aberta vista como um escritório isométrico em pixel art,
      com cada papel na sua mesa e quem termina levando a pasta a pé até a mesa do próximo. */
  interface Props {
    detail: PipelineDetail;
    /** Mesa aberta no painel. */
    selected: PipelineRole | null;
    /** Instante atual (s), para o tempo das etapas em andamento. */
    now: number;
    onselect: (role: PipelineRole) => void;
  }
  let { detail, selected, now, onselect }: Props = $props();

  const ORDER = STATIONS.map((s) => s.role);
  /** Mesa de um papel; a sua escolha fica na mesa do Code review. */
  const indexOf = (role: PipelineRole | undefined) => {
    const station = role ? stationRole(role) : null;
    return station ? ORDER.indexOf(station) : -1;
  };

  onMount(loadTierModels);
  const passes = $derived(passesByRole(detail));
  // Sem o relógio: a cena (e os sprites) só se refaz quando o estado das mesas muda, não a cada segundo.
  const cards = $derived(stationCards(detail, null, passes));
  const times = $derived(Object.fromEntries(ORDER.map((role) => [role, stationTime(stationPasses(detail, role), now)])) as Record<string, string>);

  /** A pasta fica na mesa da etapa mais recente; o ajuste sozinho não é mesa. */
  const lastStation = $derived(detail.stages.filter((s) => s.role !== "feedback").at(-1));

  // ---- viagem da pasta: quando uma etapa nova começa em outra mesa

  let previous = $state<number | null>(null);
  let tripId = $state(0);
  let traveling = $state(false);
  let seenRun: string | null = null;
  let seenCount = 0;
  let settle: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const count = detail.stages.length;
    if (detail.id !== seenRun) {
      seenRun = detail.id;
      seenCount = count;
      previous = null;
      traveling = false;
      return;
    }
    if (count <= seenCount) return;
    const before = detail.stages[count - 2];
    const latest = detail.stages[count - 1];
    seenCount = count;
    // O ajuste sozinho não é mesa: a pasta sai quando o Implementador começa.
    if (latest.role === "feedback") {
      seenCount = count - 1;
      return;
    }
    if (!before || indexOf(before.role) === indexOf(latest.role)) return;
    // Reaberta com o seu ajuste: a pasta sai da última mesa que trabalhou antes dele.
    const source = before.role === "feedback" ? detail.stages.slice(0, count - 2).findLast((s) => s.role !== "feedback") : before;
    const from = indexOf(source?.role);
    if (from < 0 || from === indexOf(latest.role)) return;
    previous = from;
    tripId += 1;
    traveling = true;
    clearTimeout(settle);
    // A duração depende da distância entre as mesas: calculada pela própria cena.
    queueMicrotask(() => {
      const trip = officeTrip(scene);
      settle = setTimeout(() => (traveling = false), (trip?.td ?? 0) * 1000);
    });
  });
  onMount(() => () => clearTimeout(settle));

  // ---- cena

  const scene = $derived<OfficeScene>({
    stations: cards.map((c) => ({
      role: c.station.role,
      visual: c.visual,
      label: c.station.name,
      stateText: c.label,
      activity: c.visual === "working" ? (c.activity ?? "Trabalhando") : null,
      account: c.account && c.account !== "você" ? c.account : null,
      model: c.model,
      // O tempo fica fora da cena: entra só no texto do balão.
      time: "",
      pass: c.last ? passLabel(c.last, passes[c.station.role].length - 1, detail.max_rounds) : null,
    })),
    active: lastStation ? indexOf(lastStation.role) : null,
    previous,
    selected: selected ? indexOf(selected) : null,
    tripId,
  });

  let width = $state(0);
  const scale = $derived(Math.max(1, Math.min(4, Math.floor(width / OFFICE_W))));
  // Densidade da tela (muda ao arrastar a janela para outro monitor ou com zoom): os bitmaps saem nela.
  let dpr = $state(window.devicePixelRatio || 1);
  onMount(() => {
    let query: MediaQueryList | undefined;
    const watch = () => {
      query?.removeEventListener("change", update);
      query = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
      query.addEventListener("change", update);
    };
    const update = () => {
      dpr = window.devicePixelRatio || 1;
      watch();
    };
    watch();
    return () => query?.removeEventListener("change", update);
  });
  // Só depois de medir a largura: a escala entra nos bitmaps, e montar antes geraria tudo duas vezes.
  const scenery = $derived(width ? officeScenery(scale, dpr) : null);
  const view = $derived(width ? officeView(scene, { scale, dpr, traveling }) : null);

  // Animações em passos avançadas pelo relógio da sala, só quando algum pixel muda: 12 quadros por segundo parado
  // (o ritmo da pixel art), 30 enquanto a pasta anda (passos de 2x1 px mais rápidos).
  let room: HTMLDivElement;
  let clock: ReturnType<typeof officeClock> | undefined;
  onMount(() => {
    clock = officeClock(room, 12);
    return () => clock?.destroy();
  });
  $effect(() => clock?.setFps(traveling ? 30 : 12));
  const meta = (text: string, role: string) => [text, times[role]].filter(Boolean).join(" · ");
</script>

<div class="office" bind:clientWidth={width}>
  <div class="office-room" style="--s:{scale}" bind:this={room}>
    {#if view && scenery}
      <svg class="bg" viewBox="0 0 {OFFICE_W} {OFFICE_H}" aria-hidden="true">{@html scenery.backdrop}</svg>
      {#each scenery.props as prop, i (i)}
        <div class="it" style={prop.style}><svg viewBox={prop.vb} aria-hidden="true">{@html prop.svg}</svg></div>
      {/each}
      <ol class="row">
        {#each view.stations as v (v.role)}
          <li class={v.cls} style={v.style}>
            <button
              type="button"
              class="desk"
              aria-label={`${v.aria}. Abrir detalhes`}
              aria-pressed={v.pressed}
              onclick={() => onselect(v.role as PipelineRole)}
            >
              <svg viewBox={v.vb} aria-hidden="true">{@html layersSvg(v.layers)}</svg>
            </button>
            <span class="tag"><span class="name">{v.label}</span><span class="pass">{v.passText}</span></span>
            {#if v.balloon}
              <div class={v.balloon.cls} style={v.balloon.style}>
                <span class="act">{v.balloon.activity}</span>
                {#if meta(v.balloon.meta, v.role)}<span class="meta">{meta(v.balloon.meta, v.role)}</span>{/if}
              </div>
            {/if}
          </li>
        {/each}
      </ol>
      <div class="trip" aria-hidden="true">
        {#if view.trip}
          {#key view.trip.id}
            {#each view.trip.legs as leg, i (i)}
              <div class={leg.cls} style={leg.style}><svg viewBox={leg.vb}>{@html spritesSvg(leg.sprites)}</svg></div>
            {/each}
          {/key}
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  /* Abaixo de 360 px a sala não encolhe: rola na horizontal. */
  .office { width: 100%; overflow-x: auto; border-radius: var(--radius); }
  .office-room { border-radius: var(--radius); }
</style>
