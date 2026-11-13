<script lang="ts">
  import { FitAddon } from "@xterm/addon-fit";
  import { Terminal } from "@xterm/xterm";
  import "@xterm/xterm/css/xterm.css";
  import { onMount } from "svelte";
  import { api, errorMessage } from "../../api";
  import type { Conversation } from "../../types";

  interface Props {
    conversation: Conversation;
  }
  let { conversation }: Props = $props();

  let host = $state<HTMLElement>();
  let error = $state<string | null>(null);

  const decoder = new TextDecoder();

  function decode(base64: string): Uint8Array {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    return bytes;
  }

  onMount(() => {
    if (!host) return;
    const term = new Terminal({
      fontFamily: '"JetBrains Mono", ui-monospace, Menlo, monospace',
      fontSize: 12.5,
      lineHeight: 1.25,
      cursorBlink: true,
      allowProposedApi: true,
      theme: { background: "#0e0f11", foreground: "#d7d9de", cursor: "#a6b0f7", selectionBackground: "#33364e" },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    fit.fit();

    const sessionId = conversation.id;
    let terminalId: string | null = null;
    let closed = false;

    const stop = api.onTerminalOutput(({ id, data }) => {
      if (id === terminalId) term.write(decoder.decode(decode(data)));
    });
    void api
      .terminalOpen(sessionId, term.cols, term.rows)
      .then((id) => {
        if (closed) return void api.terminalClose(id);
        terminalId = id;
        term.focus();
      })
      .catch((e) => (error = errorMessage(e)));

    const input = term.onData((data) => {
      if (terminalId) void api.terminalWrite(terminalId, data);
    });
    const observer = new ResizeObserver(() => {
      fit.fit();
      if (terminalId) void api.terminalResize(terminalId, term.cols, term.rows);
    });
    observer.observe(host);

    return () => {
      closed = true;
      observer.disconnect();
      input.dispose();
      void stop.then((off) => off());
      if (terminalId) void api.terminalClose(terminalId);
      term.dispose();
    };
  });
</script>

<div class="panel">
  {#if error}<p class="error-text" role="alert">{error}</p>{/if}
  <div class="host" bind:this={host} aria-label="Terminal na pasta da sessão"></div>
</div>

<style>
  .panel { flex: 1; min-height: 0; display: flex; flex-direction: column; background: #0e0f11; }
  .host { flex: 1; min-height: 0; padding: 8px 0 8px 8px; }
  .error-text { padding: 12px 16px; }
  .host :global(.xterm) { height: 100%; }
</style>
