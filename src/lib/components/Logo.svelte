<script lang="ts">
  interface Props {
    size?: number;
  }
  let { size = 24 }: Props = $props();
</script>

<!-- Marca do Lume: um ponto de luz dentro de um anel. -->
<svg aria-hidden="true" width={size} height={size} viewBox="0 0 24 24">
  <rect width="24" height="24" rx="6.5" fill="#1D1F24" />
  <circle cx="12" cy="12" r="6.4" fill="none" stroke="#3A5F58" stroke-width="1.6" />
  <circle cx="12" cy="12" r="3.2" fill="#7CCFBD" />
</svg>
