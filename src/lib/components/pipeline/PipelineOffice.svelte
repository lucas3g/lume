<script lang="ts">
  import { onMount } from "svelte";
  import "../../office/office.css";
  import { backdropSvg, layersSvg, OFFICE_H, OFFICE_W, officeTrip, officeView, type OfficeScene } from "../../office/scene";
  import { passesByRole, passLabel, STATIONS } from "../../pipeline";
  import { loadTierModels, stationCards, stationTime } from "../../pipeline-cards.svelte";
  import type { PipelineDetail, PipelineRole } from "../../types";

  /** Mesma interface da trilha (PipelineTrack): a execução aberta vista como um escritório em pixel art. */
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
  const indexOf = (role: PipelineRole | undefined) => (role ? ORDER.indexOf(role) : -1);

  onMount(loadTierModels);
  const passes = $derived(passesByRole(detail));
  // Sem o relógio: a cena (e os sprites) só se refaz quando o estado das mesas muda, não a cada segundo.
  const cards = $derived(stationCards(detail, null, passes));
  const times = $derived(Object.fromEntries(ORDER.map((role) => [role, stationTime(passes[role], now)])) as Record<string, string>);

  /** A pasta fica na mesa da etapa mais recente; o ajuste sozinho não é mesa. */
  const lastStation = $derived(detail.stages.filter((s) => s.role !== "feedback").at(-1));

  // ---- viagem da pasta: quando uma etapa nova começa em outra mesa

  let previous = $state<number | null>(null);
  let tripId = $state(0);
  let traveling = $state(false);
  let seenRun: string | null = null;
  let seenCount = 0;
  let settle: ReturnType<typeof setTimeout> | undefined;
  const reduce = typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

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
    if (!before || before.role === latest.role) return;
    // Reaberta com o seu ajuste: a pasta sai da última mesa que trabalhou antes dele.
    const source = before.role === "feedback" ? detail.stages.slice(0, count - 2).findLast((s) => s.role !== "feedback") : before;
    const from = indexOf(source?.role);
    if (from < 0 || from === indexOf(latest.role)) return;
    previous = from;
    tripId += 1;
    if (reduce) return;
    traveling = true;
    clearTimeout(settle);
    // A duração depende da distância entre as mesas: calculada pela própria cena.
    queueMicrotask(() => {
      const trip = officeTrip(scene);
      settle = setTimeout(() => (traveling = false), (trip?.td ?? 0) * 1000 + 120);
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
  const view = $derived(officeView(scene, { scale, traveling }));
  const backdrop = backdropSvg();
  const meta = (text: string, role: string) => [text, times[role]].filter(Boolean).join(" · ");
</script>

<div class="office" bind:clientWidth={width}>
  <div class="office-room" style={view.roomStyle}>
    <svg class="bg" viewBox="0 0 {OFFICE_W} {OFFICE_H}" aria-hidden="true">{@html backdrop}</svg>
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
            <svg class="spr" viewBox="0 0 48 60" aria-hidden="true">{@html layersSvg(v.layers)}</svg>
            <span class="tag"><span class="name">{v.label}</span><span class="pass">{v.passText}</span></span>
          </button>
          {#if v.balloon}
            <div class={v.balloon.cls} style={v.balloon.style}>
              <span class="act">{v.balloon.activity}</span>
              {#if meta(v.balloon.meta, v.role)}<span class="meta">{meta(v.balloon.meta, v.role)}</span>{/if}
            </div>
          {/if}
        </li>
      {/each}
    </ol>
    {#if view.trip}
      {#key view.trip.id}
        <div class={view.trip.cls} style={view.trip.style} aria-hidden="true">
          <svg class="trip-spr" viewBox={view.trip.vb}>{@html layersSvg(view.trip.layers)}</svg>
        </div>
      {/key}
    {/if}
  </div>
</div>

<style>
  /* Abaixo de 360 px a sala não encolhe: rola na horizontal. */
  .office { width: 100%; overflow-x: auto; border-radius: var(--radius); }
  .office-room { border-radius: var(--radius); }
</style>
