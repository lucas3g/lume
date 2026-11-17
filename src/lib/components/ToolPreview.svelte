<script lang="ts">
  import { highlight } from "../highlight";
  import { editDiff } from "../tools";

  interface Props {
    input: Record<string, unknown>;
    output?: string | null;
  }
  let { input, output = null }: Props = $props();

  const diff = $derived(editDiff(input));
  const command = $derived(typeof input.command === "string" ? input.command : null);
  const lines = (text: string) => text.split("\n");
</script>

{#if diff}
  <pre class="diff">{#each lines(diff.before) as line}{#if diff.before}<span class="del">- {line}</span>{/if}{/each}{#each lines(diff.after) as line}<span class="add">+ {line}</span>{/each}</pre>
{:else if command}
  <pre class="command hljs"><span class="prompt">$ </span>{@html highlight(command, "bash")}</pre>
{:else if Object.keys(input).length}
  <pre>{JSON.stringify(input, null, 2)}</pre>
{/if}
{#if output}
  <pre class="output">{output}</pre>
{/if}

<style>
  pre {
    margin: 0;
    padding: 10px 12px;
    max-height: 280px;
    overflow: auto;
    border: 1px solid var(--hairline);
    border-radius: 6px;
    background: var(--bg);
    font-family: var(--mono);
    font-size: 12.5px;
    line-height: 1.6;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .diff { padding: 6px 0; }
  .diff span { display: block; padding: 0 12px; }
  .del { background: #3a2020; color: #f0a39a; }
  .add { background: var(--ok-bg); color: #8fd9ae; }
  .output { color: var(--text-2); }
  .command { font-size: 13px; }
  .prompt { color: var(--muted); user-select: none; }
</style>
