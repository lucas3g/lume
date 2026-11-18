<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorMessage } from "../../api";
  import { botPreview, defaultLook, officeBot } from "../../office/scene";
  import { readPastedImage } from "../../paste";
  import { STATIONS } from "../../pipeline";
  import { loadOfficeLooks, officeLooks, saveOfficeLooks } from "../../state/officeLooks.svelte";
  import type { Accessory, BotLook, HairStyle, Outfit, PipelineRole } from "../../types";

  /** Os papéis que têm mesa (e boneco) no escritório. */
  const ROLES = STATIONS.map((s) => ({ role: s.role, name: s.name }));

  const HAIR: { id: HairStyle; label: string }[] = [
    { id: "short", label: "Curto" },
    { id: "curly", label: "Cacheado" },
    { id: "long", label: "Comprido" },
    { id: "bun", label: "Coque atrás" },
    { id: "topknot", label: "Coque no alto" },
    { id: "beanie", label: "Touca" },
    { id: "fade", label: "Raspado / degradê" },
  ];
  const ACCESSORY: { id: Accessory | ""; label: string }[] = [
    { id: "", label: "Nenhum" },
    { id: "glasses", label: "Óculos escuros de armação" },
    { id: "readers", label: "Óculos de leitura" },
    { id: "headset", label: "Fone de ouvido" },
    { id: "goggles", label: "Óculos de proteção" },
  ];
  const OUTFIT: { id: Outfit; label: string }[] = [
    { id: "hoodie", label: "Moletom" },
    { id: "lanyard", label: "Crachá" },
    { id: "cardigan", label: "Casaco aberto" },
    { id: "pearls", label: "Colar" },
    { id: "coat", label: "Jaleco" },
    { id: "scarf", label: "Cachecol" },
    { id: "tie", label: "Gravata" },
  ];
  const COLORS: { key: "skin" | "hair" | "shirt" | "pants"; label: string }[] = [
    { key: "skin", label: "Pele" },
    { key: "hair", label: "Cabelo" },
    { key: "shirt", label: "Roupa" },
    { key: "pants", label: "Calça" },
  ];
  const MAX_PHOTO_BYTES = 5 * 1024 * 1024;
  /** Formatos de imagem que o Claude aceita. */
  const PHOTO_TYPES = ["image/png", "image/jpeg", "image/gif", "image/webp"];

  let error = $state<string | null>(null);
  /** Papel aberto no editor e o visual em edição (ainda não salvo). */
  let editing = $state<PipelineRole | null>(null);
  let draft = $state<BotLook | null>(null);
  let busy = $state(false);
  let generating = $state(false);
  let photo = $state<string | null>(null);
  let fileInput = $state<HTMLInputElement>();

  onMount(() => void loadOfficeLooks());

  /** Miniatura de cada papel; refeita quando o visual salvo muda. */
  const thumbs = $derived.by(() => {
    void officeLooks.version;
    return Object.fromEntries(ROLES.map((r) => [r.role, officeBot(r.role)])) as Record<string, string>;
  });
  /** Prévia do visual em edição, refeita no máximo uma vez por quadro (arrastar no seletor de cor dispara muitos eventos). */
  let preview = $state("");
  $effect(() => {
    const look = draft ? ($state.snapshot(draft) as BotLook) : null;
    if (!look) {
      preview = "";
      return;
    }
    const frame = requestAnimationFrame(() => (preview = botPreview(look)));
    return () => cancelAnimationFrame(frame);
  });

  function open(role: PipelineRole) {
    error = null;
    photo = null;
    editing = role;
    const saved = officeLooks.looks[role];
    draft = saved ? { ...saved } : (defaultLook(role) as BotLook);
  }
  function close() {
    editing = null;
    draft = null;
    photo = null;
  }

  /** Ao trocar uma cor base, as sombras voltam a ser derivadas dela. */
  function setColor(key: (typeof COLORS)[number]["key"], value: string) {
    if (!draft) return;
    draft[key] = value;
    if (key === "skin") delete draft.skin_shade;
    if (key === "hair") delete draft.hair_light;
    if (key === "shirt") delete draft.shirt_shade;
  }

  async function save() {
    if (!editing || !draft) return;
    // Sem visual salvo e sem mudança, não há o que salvar (na sua mesa, salvar trocaria o retrato desenhado à mão).
    if (!officeLooks.looks[editing] && JSON.stringify($state.snapshot(draft)) === JSON.stringify(defaultLook(editing))) {
      close();
      return;
    }
    busy = true;
    error = null;
    try {
      await saveOfficeLooks({ ...$state.snapshot(officeLooks.looks), [editing]: $state.snapshot(draft) });
      close();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  async function restore(role: PipelineRole) {
    busy = true;
    error = null;
    try {
      const next = { ...$state.snapshot(officeLooks.looks) };
      delete next[role];
      await saveOfficeLooks(next);
      if (editing === role) close();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  /** Manda a foto ao Claude e preenche o editor com o visual sugerido (nada é salvo até clicar em Salvar). */
  async function fromPhoto(file: File | undefined) {
    if (!file || !draft) return;
    if (!PHOTO_TYPES.includes(file.type)) {
      error = "Escolha uma imagem PNG, JPEG, GIF ou WebP.";
      return;
    }
    if (file.size > MAX_PHOTO_BYTES) {
      error = "A foto pode ter até 5 MB.";
      return;
    }
    error = null;
    generating = true;
    const role = editing;
    try {
      const image = await readPastedImage(file);
      photo = image.preview;
      const look = await api.draftBotLook({ name: image.name, media_type: image.media_type, data: image.data });
      // Se o editor foi fechado ou trocou de papel no meio, a sugestão não vale mais.
      if (draft && editing === role) draft = look;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      generating = false;
      if (fileInput) fileInput.value = "";
    }
  }

  function onPaste(event: ClipboardEvent) {
    const file = [...(event.clipboardData?.files ?? [])].find((f) => f.type.startsWith("image/"));
    if (!file) return;
    event.preventDefault();
    void fromPhoto(file);
  }
  function onDrop(event: DragEvent) {
    event.preventDefault();
    void fromPhoto([...(event.dataTransfer?.files ?? [])].find((f) => f.type.startsWith("image/")));
  }
</script>

<section class="card" aria-label="Visual dos bots">
  <div class="head">
    <h2 class="section-title">Visual dos bots</h2>
    <p class="muted small">
      Como cada bot aparece no escritório. Escolha as cores e as peças ou envie uma foto para o Claude montar o boneco
      parecido com a pessoa. Vale para todos os projetos.
    </p>
  </div>
  {#if error}<p class="error-text pad" role="alert">{error}</p>{/if}

  <div class="grid">
    {#each ROLES as r (r.role)}
      <button type="button" class="bot" class:on={editing === r.role} onclick={() => open(r.role)} aria-label={`Editar o visual de ${r.name}`}>
        <span class="figure">{@html thumbs[r.role]}</span>
        <span class="name">{r.name}</span>
        {#if officeLooks.looks[r.role]}<span class="muted tiny">personalizado</span>{/if}
      </button>
    {/each}
  </div>

  {#if editing && draft}
    {@const name = ROLES.find((r) => r.role === editing)?.name}
    <div class="editor" role="group" aria-label={`Visual de ${name}`} onpaste={onPaste} ondragover={(e) => e.preventDefault()} ondrop={onDrop}>
      <div class="stage">
        <span class="figure big">{@html preview}</span>
        {#if photo}<img class="photo" src={photo} alt="Foto enviada" />{/if}
      </div>
      <div class="fields">
        <h3 class="small">{name}</h3>
        <div class="colors">
          {#each COLORS as c (c.key)}
            <label class="color">
              <input type="color" value={draft[c.key]} oninput={(e) => setColor(c.key, e.currentTarget.value)} />
              <span class="small">{c.label}</span>
            </label>
          {/each}
        </div>
        <label class="row">
          <span class="small">Cabelo</span>
          <select class="input" bind:value={draft.hair_style}>
            {#each HAIR as h (h.id)}<option value={h.id}>{h.label}</option>{/each}
          </select>
        </label>
        <label class="row">
          <span class="small">Acessório</span>
          <select class="input" value={draft.accessory ?? ""} onchange={(e) => draft && (draft.accessory = (e.currentTarget.value || null) as Accessory | null)}>
            {#each ACCESSORY as a (a.id)}<option value={a.id}>{a.label}</option>{/each}
          </select>
        </label>
        <label class="row">
          <span class="small">Roupa</span>
          <select class="input" bind:value={draft.outfit}>
            {#each OUTFIT as o (o.id)}<option value={o.id}>{o.label}</option>{/each}
          </select>
        </label>
        <p class="muted tiny">
          {generating ? "O Claude está olhando a foto…" : "Também dá para colar ou arrastar uma foto aqui. A foto só vai no pedido ao Claude e não fica guardada."}
        </p>
        <div class="actions">
          <input bind:this={fileInput} type="file" accept={PHOTO_TYPES.join(",")} hidden onchange={(e) => fromPhoto(e.currentTarget.files?.[0])} />
          <button type="button" class="btn small" disabled={generating || busy} onclick={() => fileInput?.click()}>
            {generating ? "Gerando…" : "Gerar a partir de foto"}
          </button>
          <span class="spacer"></span>
          {#if officeLooks.looks[editing]}
            <button type="button" class="btn small" disabled={busy || generating} onclick={() => editing && restore(editing)}>Restaurar padrão</button>
          {/if}
          <button type="button" class="btn small" disabled={busy} onclick={close}>Cancelar</button>
          <button type="button" class="btn small primary" disabled={busy || generating} onclick={save}>Salvar</button>
        </div>
      </div>
    </div>
  {/if}
</section>

<style>
  .head { padding: 16px 20px 4px; display: flex; flex-direction: column; gap: 4px; }
  .pad { padding: 4px 20px 0; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(92px, 1fr)); gap: 8px; padding: 12px 20px 16px; }
  .bot {
    display: flex; flex-direction: column; align-items: center; gap: 4px; padding: 8px 4px;
    background: var(--surface-2); border: 1px solid var(--border); border-radius: 8px; color: inherit; cursor: pointer;
  }
  .bot:hover, .bot.on { border-color: var(--border-strong); }
  .bot.on { outline: 2px solid var(--accent, #6f7de0); outline-offset: -1px; }
  .name { font-size: 12px; text-align: center; }
  .tiny { font-size: 11px; }
  .figure { display: block; width: 36px; height: 60px; image-rendering: pixelated; }
  .figure.big { width: 72px; height: 120px; }
  .figure :global(svg) { width: 100%; height: 100%; display: block; }
  .figure :global(image) { image-rendering: pixelated; }
  .editor { display: flex; gap: 20px; padding: 4px 20px 18px; border-top: 1px solid var(--border); padding-top: 16px; }
  .stage { display: flex; flex-direction: column; align-items: center; gap: 10px; min-width: 96px; }
  .photo { width: 72px; height: 72px; object-fit: cover; border-radius: 8px; border: 1px solid var(--border); }
  .fields { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 10px; }
  .fields h3 { margin: 0; font-weight: 600; }
  .colors { display: flex; flex-wrap: wrap; gap: 12px; }
  .color { display: flex; align-items: center; gap: 6px; cursor: pointer; }
  .color input { width: 28px; height: 28px; padding: 0; border: 1px solid var(--border-strong); border-radius: 6px; background: none; cursor: pointer; }
  .row { display: grid; grid-template-columns: 80px minmax(0, 1fr); align-items: center; gap: 10px; max-width: 360px; }
  .actions { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .spacer { flex: 1; }
  @media (max-width: 640px) {
    .editor { flex-direction: column; }
  }
</style>
