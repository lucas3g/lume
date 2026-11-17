<script lang="ts">
  import type { Todo } from "../tools";

  let { todos }: { todos: Todo[] } = $props();

  const STATES = { completed: "feita", in_progress: "em andamento", pending: "pendente" } as const;
</script>

<ul class="todos" aria-label="Tarefas do agente">
  {#each todos as todo, i (i)}
    <li class={todo.status}>
      {#if todo.status === "completed"}
        <svg aria-hidden="true" width="18" height="18" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" fill="currentColor" /><path d="M7.5 12.5l3 3 6-6.5" fill="none" stroke="var(--surface)" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" /></svg>
      {:else if todo.status === "in_progress"}
        <span class="spin" aria-hidden="true"></span>
      {:else}
        <svg aria-hidden="true" width="18" height="18" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-dasharray="3 3" /></svg>
      {/if}
      <span class="text">{todo.text}</span>
      <span class="sr-only">({STATES[todo.status]})</span>
    </li>
  {/each}
</ul>

<style>
  .todos { margin: 0; padding: 14px 20px; list-style: none; border: 1px solid var(--border); border-radius: 14px; background: var(--surface); display: flex; flex-direction: column; gap: 10px; }
  li { display: flex; align-items: center; gap: 10px; font-size: 14px; color: var(--text); }
  li svg { flex-shrink: 0; color: var(--muted); }
  li.completed .text { color: var(--muted); }
  .spin { flex-shrink: 0; width: 14px; height: 14px; margin: 2px; border: 2px solid var(--border-strong); border-top-color: var(--text-2); border-radius: 50%; animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spin { animation: none; } }
  .sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
</style>
