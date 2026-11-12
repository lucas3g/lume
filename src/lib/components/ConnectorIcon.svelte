<script lang="ts">
  import { api } from "../api";

  interface Props {
    /** Endereço do serviço; comandos locais não têm ícone. */
    endpoint: string;
    name: string;
    size?: number;
  }
  let { endpoint, name, size = 32 }: Props = $props();

  let src = $state<string | null>(null);

  /** Ícones já resolvidos nesta sessão, por endereço. */
  const cache = new Map<string, Promise<string | null>>();

  function load(target: string) {
    if (!cache.has(target)) cache.set(target, api.connectorIcon(target).catch(() => null));
    return cache.get(target)!;
  }

  $effect(() => {
    const target = endpoint;
    src = null;
    if (!target.startsWith("http")) return;
    void load(target).then((url) => {
      if (target === endpoint) src = url;
    });
  });

  const letter = $derived((name.trim()[0] ?? "?").toUpperCase());
  /** Cor estável por nome, para os serviços sem ícone. */
  const hue = $derived([...name].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7));
</script>

<span class="icon" style:width={`${size}px`} style:height={`${size}px`} style:font-size={`${Math.round(size * 0.44)}px`} style:background={src ? "#fff" : `hsl(${hue} 32% 28%)`}>
  {#if src}
    <img {src} alt="" width={Math.round(size * 0.62)} height={Math.round(size * 0.62)} onerror={() => (src = null)} />
  {:else}
    <span aria-hidden="true">{letter}</span>
  {/if}
</span>

<style>
  .icon { flex-shrink: 0; display: grid; place-items: center; border-radius: 8px; border: 1px solid var(--border); font-weight: 600; color: #fff; overflow: hidden; }
  img { object-fit: contain; }
</style>
