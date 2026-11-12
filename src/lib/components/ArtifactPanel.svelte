<script lang="ts">
  import { save as pickDestination } from "@tauri-apps/plugin-dialog";
  import { api, errorMessage } from "../api";
  import { fileNameFor, previewDocument, type Artifact } from "../artifacts";

  interface Props {
    /** Versões do artifact, da mais antiga para a mais nova. */
    versions: Artifact[];
    /** Versão exibida ao abrir. */
    initial: number;
    onclose: () => void;
  }
  let { versions, initial, onclose }: Props = $props();

  let tab = $state<"preview" | "code">("preview");
  let index = $state(0);
  let feedback = $state<string | null>(null);

  // Ao abrir outro artifact (ou outra versão pela conversa), mostra a versão pedida.
  $effect(() => {
    index = Math.min(initial, versions.length - 1);
  });

  const artifact = $derived(versions[Math.min(index, versions.length - 1)]);

  function notify(message: string) {
    feedback = message;
    setTimeout(() => (feedback = null), 2000);
  }

  async function copy() {
    await navigator.clipboard.writeText(artifact.code);
    notify("Copiado");
  }

  async function exportFile() {
    const path = await pickDestination({ defaultPath: fileNameFor(artifact), title: "Salvar artifact" });
    if (!path) return;
    try {
      await api.saveTextFile(path, artifact.code);
      notify("Salvo");
    } catch (e) {
      notify(errorMessage(e));
    }
  }
</script>

<section aria-label={`Artifact: ${artifact.title}`}>
  <header>
    <div class="tabs" role="tablist" aria-label="Modo de exibição">
      <button type="button" role="tab" aria-selected={tab === "preview"} class:on={tab === "preview"} onclick={() => (tab = "preview")}>Prévia</button>
      <button type="button" role="tab" aria-selected={tab === "code"} class:on={tab === "code"} onclick={() => (tab = "code")}>Código</button>
    </div>
    <span class="title" role="status">{feedback ?? artifact.title}</span>
    {#if versions.length > 1}
      <div class="versions">
        <button type="button" class="btn icon" aria-label="Versão anterior" disabled={index === 0} onclick={() => index--}>
          <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 6l-6 6 6 6" /></svg>
        </button>
        <span>Versão {index + 1} de {versions.length}</span>
        <button type="button" class="btn icon" aria-label="Próxima versão" disabled={index === versions.length - 1} onclick={() => index++}>
          <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 6l6 6-6 6" /></svg>
        </button>
      </div>
    {/if}
    <button type="button" class="btn small" onclick={copy}>Copiar</button>
    <button type="button" class="btn small" onclick={exportFile}>Exportar</button>
    <button type="button" class="btn icon" aria-label="Fechar o artifact" onclick={onclose}>
      <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
    </button>
  </header>

  {#if tab === "preview"}
    <!-- Isolado: sem acesso ao app nem aos dados do usuário. -->
    {#key artifact.code}
      <iframe title={artifact.title} sandbox="allow-scripts" srcdoc={previewDocument(artifact)}></iframe>
    {/key}
  {:else}
    <pre><code>{artifact.code}</code></pre>
  {/if}
</section>

<style>
  section { flex: 1.2 1 440px; min-width: 0; height: 100%; display: flex; flex-direction: column; border-left: 1px solid var(--border); background: var(--bg-side); }
  header { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; min-height: 52px; padding: 4px 12px 4px 16px; border-bottom: 1px solid var(--border); }
  .tabs { display: flex; padding: 3px; border-radius: 8px; background: var(--bg); }
  .tabs button { min-height: 30px; padding: 0 14px; border: 0; border-radius: 6px; background: transparent; font-size: 13px; color: var(--text-2); }
  .tabs button.on { background: var(--hover); color: #fff; font-weight: 500; }
  .title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; color: var(--muted); }
  .versions { display: flex; align-items: center; gap: 2px; font-size: 13px; color: var(--text-2); white-space: nowrap; }
  .versions .btn { width: 26px; min-height: 26px; }
  iframe { flex: 1; width: 100%; border: 0; background: #fff; }
  pre { flex: 1; margin: 0; padding: 16px; overflow: auto; font-family: var(--mono); font-size: 12.5px; line-height: 1.6; }
</style>
