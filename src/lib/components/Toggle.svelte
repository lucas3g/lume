<script lang="ts">
  interface Props {
    checked: boolean;
    label: string;
    onchange: (value: boolean) => void;
  }
  let { checked, label, onchange }: Props = $props();
</script>

<button type="button" role="switch" aria-checked={checked} aria-label={label} class:on={checked} onclick={() => onchange(!checked)}>
  <span></span>
</button>

<style>
  button {
    flex-shrink: 0;
    width: 44px;
    height: 26px;
    padding: 3px;
    border: 0;
    border-radius: 13px;
    background: var(--off);
    display: flex;
    transition: background 0.15s;
  }
  button.on { background: var(--accent); justify-content: flex-end; }
  span { width: 20px; height: 20px; border-radius: 50%; background: #fff; }
  @media (prefers-reduced-motion: reduce) { button { transition: none; } }
</style>
