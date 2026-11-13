<script lang="ts">
  import { api } from "../api";
  import { CATALOG } from "../catalog";

  interface Props {
    name: string;
    /** Endereço do serviço, para o favicon quando a marca não está no catálogo; comandos locais não têm. */
    endpoint?: string;
    size?: number;
  }
  let { name, endpoint = "", size = 32 }: Props = $props();

  /** Marca do catálogo: ícone de uma cor pintado com a cor da marca, ou nas cores do fabricante. */
  const brand = $derived(CATALOG[name.trim().toLowerCase()]);
  const brandSrc = $derived(brand?.icon ? `/connectors/${brand.icon}.svg` : null);

  let favicon = $state<string | null>(null);

  /** Ícones já resolvidos nesta sessão, por endereço. */
  const cache = new Map<string, Promise<string | null>>();

  function load(target: string) {
    if (!cache.has(target)) cache.set(target, api.connectorIcon(target).catch(() => null));
    return cache.get(target)!;
  }

  $effect(() => {
    const target = endpoint;
    favicon = null;
    if (brandSrc || !target.startsWith("http")) return;
    void load(target).then((url) => {
      if (target === endpoint) favicon = url;
    });
  });

  const letter = $derived((name.trim()[0] ?? "?").toUpperCase());
  /** Cor estável por nome, para os serviços sem ícone. */
  const hue = $derived([...name].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7));
  const radius = $derived(Math.max(8, Math.round(size * 0.28)));
</script>

<span
  class="icon"
  class:brand={!!brandSrc || !!favicon}
  style:width={`${size}px`}
  style:height={`${size}px`}
  style:border-radius={`${radius}px ${radius}px ${radius}px ${Math.round(radius / 3)}px`}
  style:font-size={`${Math.round(size * 0.42)}px`}
  style:background={brandSrc || favicon ? undefined : `hsl(${hue} 30% 26%)`}
>
  {#if brandSrc && brand?.own}
    <img src={brandSrc} alt="" style:width="60%" style:height="60%" />
  {:else if brandSrc}
    <span class="mark" style:--mark={`url("${brandSrc}")`} style:background={brand?.color ?? "var(--text)"}></span>
  {:else if favicon}
    <img src={favicon} alt="" style:width="62%" style:height="62%" onerror={() => (favicon = null)} />
  {:else}
    <span aria-hidden="true">{letter}</span>
  {/if}
</span>

<style>
  .icon { flex-shrink: 0; display: grid; place-items: center; border: 1px solid var(--border); font-weight: 650; color: #fff; overflow: hidden; }
  .icon.brand { background: var(--surface-2); border-color: var(--border-strong); }
  .mark { width: 58%; height: 58%; -webkit-mask: var(--mark) center / contain no-repeat; mask: var(--mark) center / contain no-repeat; }
  img { object-fit: contain; }
</style>
