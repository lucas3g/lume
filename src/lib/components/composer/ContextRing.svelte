<script lang="ts">
  interface Props {
    /** Percentual do contexto usado; `null` antes da primeira resposta. */
    percent: number | null;
  }
  let { percent }: Props = $props();

  const RADIUS = 6;
  const LENGTH = 2 * Math.PI * RADIUS;
  const used = $derived(percent ?? 0);
  const label = $derived(percent === null ? "Contexto: ainda sem uso medido" : `Contexto: ${percent}% usado`);
</script>

<span class="ring" role="img" aria-label={label} title={label}>
  <svg width="16" height="16" viewBox="0 0 16 16">
    <circle cx="8" cy="8" r={RADIUS} fill="none" stroke="var(--border-strong)" stroke-width="2" />
    <circle
      cx="8"
      cy="8"
      r={RADIUS}
      fill="none"
      stroke={used >= 80 ? "var(--warn)" : "var(--bar)"}
      stroke-width="2"
      stroke-linecap="round"
      stroke-dasharray={LENGTH}
      stroke-dashoffset={LENGTH * (1 - used / 100)}
      transform="rotate(-90 8 8)"
    />
  </svg>
</span>

<style>
  .ring { display: grid; place-items: center; width: 24px; height: 28px; }
</style>
