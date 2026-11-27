<script lang="ts">
  /**
   * Mapa da conversa: uma trilha ao lado com as mensagens importantes (cartões de destaque,
   * falhas, o plano esperando você, mensagens fixadas). Clicar leva até ela.
   */
  interface Props {
    scroller: HTMLElement | undefined;
    root: HTMLElement | undefined;
  }
  let { scroller, root }: Props = $props();

  interface Mark {
    top: number;
    kind: string;
    label: string;
    el: HTMLElement;
  }

  let marks = $state.raw<Mark[]>([]);
  let view = $state({ top: 0, height: 0 });
  let track = $state<HTMLElement>();

  const KIND_LABEL: Record<string, string> = { ok: "Entregue", act: "Ação sua", note: "Ficou fora", bad: "Erro" };

  function offsetOf(el: HTMLElement): number {
    if (!scroller) return 0;
    return el.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;
  }

  function measure() {
    if (!scroller || !root || !track) return;
    const total = scroller.scrollHeight;
    const height = track.clientHeight;
    const found: Mark[] = [];
    for (const el of root.querySelectorAll<HTMLElement>(".callout[data-callout], [data-mm]")) {
      const kind = el.dataset.callout ?? el.dataset.mmKind ?? "plan";
      const label = el.dataset.mm ?? el.querySelector(".co-t")?.textContent?.trim() ?? KIND_LABEL[kind] ?? "";
      found.push({ top: (offsetOf(el) / total) * height, kind, label, el });
    }
    marks = found;
    view = { top: (scroller.scrollTop / total) * height, height: Math.max(14, (scroller.clientHeight / total) * height) };
  }

  let frame = 0;
  const schedule = () => {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(measure);
  };

  $effect(() => {
    if (!scroller || !root) return;
    const resize = new ResizeObserver(schedule);
    resize.observe(root);
    resize.observe(scroller);
    const mutation = new MutationObserver(schedule);
    mutation.observe(root, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-mm", "open"] });
    scroller.addEventListener("scroll", schedule, { passive: true });
    schedule();
    return () => {
      resize.disconnect();
      mutation.disconnect();
      scroller.removeEventListener("scroll", schedule);
      cancelAnimationFrame(frame);
    };
  });

  function go(mark: Mark) {
    scroller?.scrollTo({ top: offsetOf(mark.el) - 72, behavior: "smooth" });
    mark.el.classList.remove("flash");
    void mark.el.offsetWidth;
    mark.el.classList.add("flash");
  }
</script>

<nav class="minimap" class:empty={!marks.length} aria-label="Mapa da conversa">
  <div class="track" bind:this={track}>
    <div class="view" style={`top:${view.top}px;height:${view.height}px`}></div>
    {#each marks as mark, i (i)}
      <button type="button" class={`dot ${mark.kind}`} style={`top:${mark.top}px`} aria-label={mark.label} data-label={mark.label} onclick={() => go(mark)}></button>
    {/each}
  </div>
</nav>

<style>
  .minimap { position: absolute; top: 24px; bottom: 24px; right: 10px; width: 22px; z-index: 5; }
  .minimap.empty { display: none; }
  .track { position: absolute; inset: 0 10px; width: 2px; border-radius: 2px; background: var(--border); }
  .view { position: absolute; left: -4px; width: 10px; border-radius: 5px; border: 1px solid rgba(91, 106, 217, 0.35); background: rgba(91, 106, 217, 0.14); transition: top 0.12s linear, height 0.12s linear; }
  .dot { position: absolute; left: 50%; width: 10px; height: 10px; margin: -5px 0 0 -5px; padding: 0; border: 2px solid var(--bg); border-radius: 50%; background: var(--muted); transition: transform 0.3s var(--spring), top 0.5s var(--ease); z-index: 1; }
  .dot:hover, .dot:focus-visible { transform: scale(1.5); }
  .dot.ok { background: var(--ok); }
  .dot.act { background: var(--warn); }
  .dot.note { background: var(--note); }
  .dot.bad { background: var(--bad); }
  .dot.plan { background: var(--accent-text); }
  .dot.pin { border-radius: 2px; background: var(--accent-text); transform: rotate(45deg); }
  .dot.pin:hover { transform: rotate(45deg) scale(1.5); }
  .dot::after { content: attr(data-label); position: absolute; right: calc(100% + 12px); top: 50%; translate: 0 -50%; white-space: nowrap; padding: 6px 8px; border: 1px solid var(--border-strong); border-radius: 7px; background: var(--surface-2); box-shadow: 0 12px 30px -12px rgba(0, 0, 0, 0.7); font-size: 11.5px; line-height: 1; color: var(--text); opacity: 0; pointer-events: none; transition: opacity 0.2s; }
  .dot.pin::after { rotate: -45deg; }
  .dot:hover::after, .dot:focus-visible::after { opacity: 1; }
  @media (max-width: 900px) { .minimap { display: none; } }
</style>
