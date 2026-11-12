<script lang="ts">
  import { highlightFile } from "../highlight";

  interface Props {
    text: string;
    /** Caminho do arquivo, usado para escolher a linguagem. */
    path: string;
    /** Texto maior, para a leitura no centro da tela. */
    large?: boolean;
  }
  let { text, path, large = false }: Props = $props();

  const lines = $derived(highlightFile(text, path));
</script>

<pre class="code hljs" class:large>{#each lines as line, i}<span class="line"><span class="n" aria-hidden="true">{i + 1}</span><span class="t">{@html line || " "}</span></span>{/each}</pre>

<style>
  .code { flex: 1; min-height: 0; margin: 0; padding: 10px 0; overflow: auto; font-family: var(--mono); font-size: 12px; font-weight: 700; line-height: 1.75; user-select: text; }
  .code.large { font-size: 13px; padding: 14px 0; }
  .line { display: flex; gap: 16px; padding-right: 16px; }
  .n { font-weight: 400; width: 42px; flex-shrink: 0; text-align: right; color: var(--code-comment); opacity: 0.75; user-select: none; }
  .t { white-space: pre; }
</style>
