<script lang="ts">
  import { onMount } from "svelte";
  import "../../app.css";
  import { api, errorMessage } from "$lib/api";
  import Logo from "$lib/components/Logo.svelte";
  import { DEFAULT_MODEL, type Account } from "$lib/types";

  let text = $state("");
  let sending = $state(false);
  let error = $state<string | null>(null);
  let account = $state<Account | undefined>();
  let input = $state<HTMLInputElement>();

  const nowSeconds = () => Math.floor(Date.now() / 1000);

  async function loadAccount() {
    const now = nowSeconds();
    const all = await api.listAccounts();
    account = all.find((a) => a.enabled && a.auth_ok && (a.limited_until === null || a.limited_until <= now));
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    const message = text.trim();
    if (!message || sending) return;
    sending = true;
    error = null;
    try {
      const conversation = await api.createConversation(DEFAULT_MODEL);
      await api.sendMessage(conversation.id, message, [], []);
      text = "";
      await api.quickEntrySent(conversation.id);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      sending = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") void api.hideQuickEntry();
  }

  onMount(() => {
    void loadAccount();
    // A janela é reaproveitada: a cada exibição, foca o campo e atualiza a conta.
    const onFocus = () => {
      input?.focus();
      void loadAccount();
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  });
</script>

<svelte:window onkeydown={onKeydown} />

<form onsubmit={submit}>
  <div class="field">
    <Logo size={22} />
    <!-- svelte-ignore a11y_autofocus -->
    <input bind:this={input} bind:value={text} aria-label="Pergunta" placeholder="Pergunte qualquer coisa" disabled={sending} autofocus />
  </div>
  <div class="foot">
    {#if error}
      <span class="error-text" role="alert">{error}</span>
    {:else}
      <span class="who"><span class="status" class:none={!account}></span>{account?.name ?? "Nenhuma conta disponível"}</span>
    {/if}
    <span class="spacer"></span>
    <span>Enter para enviar</span>
    <span>Esc para fechar</span>
  </div>
</form>

<style>
  form { height: 100vh; display: flex; flex-direction: column; background: var(--surface); border: 1px solid var(--border-strong); }
  .field { flex: 1; display: flex; align-items: center; gap: 12px; padding: 0 16px; }
  input { flex: 1; min-width: 0; min-height: 44px; border: 0; outline: 0; background: transparent; font-size: 17px; }
  .foot { display: flex; align-items: center; gap: 16px; padding: 10px 16px; border-top: 1px solid var(--border); background: var(--bg-side); font-size: 12px; color: var(--muted); }
  .who { display: flex; align-items: center; gap: 6px; }
  .status { width: 8px; height: 8px; border-radius: 50%; background: var(--ok); }
  .status.none { background: var(--warn); }
  .spacer { flex: 1; }
</style>
