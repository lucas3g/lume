<script lang="ts">
  /** Número que rola dígito a dígito quando muda (tokens, custo). */
  let { value }: { value: string } = $props();
  const DIGITS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
</script>

<span class="odo"><span class="sr">{value}</span><span class="cols" aria-hidden="true">{#each value.split("") as ch, i (i)}{#if /\d/.test(ch)}<span class="oc"><span class="os" style:--d={ch}>{#each DIGITS as d (d)}<span>{d}</span>{/each}</span></span>{:else}<span>{ch}</span>{/if}{/each}</span></span>

<style>
  .odo { position: relative; display: inline-flex; }
  .cols { display: inline-flex; height: 1.35em; line-height: 1.35em; overflow: hidden; }
  .oc { display: inline-block; height: 1.35em; overflow: hidden; }
  .os { display: flex; flex-direction: column; transform: translateY(calc(var(--d) * -1.35em)); transition: transform 0.9s cubic-bezier(0.2, 0.9, 0.22, 1.06); }
  .os span { display: block; height: 1.35em; text-align: center; }
  .sr { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
</style>
