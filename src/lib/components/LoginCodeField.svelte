<script lang="ts">
  import { api, errorMessage } from "../api";
  import type { CodeScope } from "../state/logins.svelte";

  interface Props {
    /** Login que recebe o código: o de conta ou o de conector. */
    scope: CodeScope;
  }
  let { scope }: Props = $props();
  const uid = $props.id();
  const id = `login-code-${uid}`;

  /** Às vezes o login termina numa página com um código de autenticação em vez de voltar
   * sozinho ao app. Enquanto o login espera, o código pode ser colado aqui. */
  let code = $state("");
  let sending = $state(false);
  let sent = $state(false);
  let error = $state<string | null>(null);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    error = null;
    sending = true;
    try {
      await api.submitLoginCode(scope, code);
      code = "";
      sent = true;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      sending = false;
    }
  }
</script>

<form class="login-code" onsubmit={submit}>
  <label class="muted small" for={id}>O navegador mostrou um código de autenticação? Cole aqui:</label>
  <div class="row">
    <input
      {id}
      class="input"
      placeholder="Código de autenticação"
      autocomplete="off"
      spellcheck="false"
      bind:value={code}
      disabled={sending}
      oninput={() => (sent = false)}
    />
    <button type="submit" class="btn primary" disabled={sending || !code.trim()}>{sending ? "Enviando…" : "Enviar código"}</button>
  </div>
  {#if error}<p class="error-text small" role="alert">{error}</p>{:else if sent}<p class="muted small" role="status">Código enviado. Aguardando o Claude Code confirmar…</p>{/if}
</form>

<style>
  .login-code { display: flex; flex-direction: column; gap: 6px; margin: 8px 0; }
  .login-code .input { flex: 1; min-width: 0; font-family: var(--mono); }
</style>
