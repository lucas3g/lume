<script lang="ts">
  interface Props {
    open: boolean;
    onclose: () => void;
    /** Conversa comum, sem pasta. */
    onchat: () => void;
    /** Sessão em uma pasta de projeto. */
    onfolder: () => void;
  }
  let { open, onclose, onchat, onfolder }: Props = $props();

  let dialog = $state<HTMLDialogElement>();

  // O elemento nativo cuida do foco, do Esc e do fundo escurecido.
  $effect(() => {
    if (open && !dialog?.open) dialog?.showModal();
    if (!open && dialog?.open) dialog.close();
  });

  /** Clique no fundo (fora do conteúdo) fecha. */
  function onBackdrop(event: MouseEvent) {
    if (event.target === dialog) onclose();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog bind:this={dialog} aria-labelledby="new-conversation-title" onclose={onclose} onclick={onBackdrop}>
  <div class="content">
    <h2 id="new-conversation-title">Nova conversa</h2>
    <p class="muted">Como você quer começar?</p>

    <button type="button" class="option" onclick={onchat}>
      <svg aria-hidden="true" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M4 5h16v11H9l-5 4z" /></svg>
      <span>
        <strong>Só conversar</strong>
        <span class="muted">Perguntas, textos e ideias, sem acesso aos seus arquivos.</span>
      </span>
    </button>

    <button type="button" class="option" onclick={onfolder}>
      <svg aria-hidden="true" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>
      <span>
        <strong>Abrir uma pasta de projeto</strong>
        <span class="muted">O assistente trabalha nos arquivos da pasta. A pasta vira um projeto na barra lateral.</span>
      </span>
    </button>

    <button type="button" class="btn cancel" onclick={onclose}>Cancelar</button>
  </div>
</dialog>

<style>
  dialog { width: min(440px, calc(100vw - 32px)); padding: 0; border: 1px solid var(--border-strong); border-radius: 16px; background: var(--surface); color: var(--text); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6); }
  dialog::backdrop { background: rgba(0, 0, 0, 0.55); }
  .content { padding: 20px; display: flex; flex-direction: column; gap: 10px; }
  h2 { font-family: var(--display); font-size: 19px; font-weight: 600; letter-spacing: -0.01em; }
  p { margin: -6px 0 6px; }
  .option { display: flex; align-items: flex-start; gap: 14px; padding: 14px; border: 1px solid var(--border); border-radius: 12px; background: var(--bg-side); text-align: left; color: var(--text); }
  .option:hover { border-color: var(--border-strong); background: var(--hover); }
  .option svg { flex-shrink: 0; margin-top: 2px; color: var(--accent-text); }
  .option span { display: flex; flex-direction: column; gap: 2px; }
  .option .muted { font-size: 12.5px; line-height: 1.45; }
  .cancel { align-self: flex-end; margin-top: 4px; }
</style>
