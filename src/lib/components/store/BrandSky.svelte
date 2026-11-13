<script lang="ts">
  /** Faixa com o "céu" da marca: um degradê suave na cor do serviço com textura de pixels. */
  interface Props {
    color: string | null;
    height?: number;
  }
  let { color, height = 72 }: Props = $props();
  const tone = $derived(color ?? "var(--accent)");
</script>

<span class="sky" aria-hidden="true" style:height={`${height}px`} style:--tone={tone}></span>

<style>
  .sky {
    display: block;
    position: relative;
    overflow: hidden;
    background:
      repeating-linear-gradient(0deg, transparent 0 3px, rgba(0, 0, 0, 0.12) 3px 4px),
      repeating-linear-gradient(90deg, transparent 0 3px, rgba(0, 0, 0, 0.12) 3px 4px),
      radial-gradient(120% 90% at 85% 100%, color-mix(in srgb, var(--tone) 52%, var(--bg-side)) 0%, transparent 60%),
      radial-gradient(80% 70% at 15% 20%, color-mix(in srgb, var(--tone) 24%, var(--bg-side)) 0%, transparent 70%),
      linear-gradient(180deg, color-mix(in srgb, var(--tone) 10%, var(--bg-side)), color-mix(in srgb, var(--tone) 34%, var(--bg-side)));
  }
  .sky::after { content: ""; position: absolute; inset: 0; background: linear-gradient(180deg, transparent 55%, var(--surface) 100%); opacity: 0.35; }
</style>
