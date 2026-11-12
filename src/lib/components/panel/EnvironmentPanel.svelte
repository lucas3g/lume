<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../../api";
  import type { Artifact } from "../../artifacts";
  import { folderName } from "../../format";
  import { activeAccount } from "../../state/accounts.svelte";
  import { workspace } from "../../state/workspace.svelte";
  import type { Connector, Conversation, View } from "../../types";

  interface Props {
    conversation: Conversation | undefined;
    artifacts: Artifact[];
    onopenartifact: (artifact: Artifact) => void;
    onnavigate: (view: View) => void;
  }
  let { conversation, artifacts, onopenartifact, onnavigate }: Props = $props();

  let connectors = $state<Connector[]>([]);

  const isAgent = $derived(conversation?.kind === "agent");
  const status = $derived(workspace.status);
  const account = $derived(activeAccount());
  const context = $derived(conversation?.context_percent ?? null);
  const active = $derived(connectors.filter((c) => c.enabled));
  const broken = $derived(active.filter((c) => c.status && c.status !== "connected"));
  /** Uma entrada por artifact, ficando com a versão mais recente de cada título. */
  const outputs = $derived([...new Map(artifacts.map((a) => [`${a.kind}:${a.title}`, a])).values()]);

  /** Pasta que contém a da sessão, com o diretório pessoal abreviado. */
  function parentOf(path: string): string {
    const parent = path.replace(/[\\/][^\\/]+[\\/]?$/, "");
    return parent.replace(/^\/Users\/[^/]+/, "~").replace(/^[A-Z]:\\Users\\[^\\]+/, "~");
  }

  function connectorSummary(): string {
    if (broken.length) return `${broken.length} para reconectar`;
    return active.length ? `${active.length} ${active.length === 1 ? "ativo" : "ativos"}` : "Nenhum ativo";
  }

  onMount(async () => {
    connectors = await api.listConnectors();
  });
</script>

<div class="panel">
  <section>
    <h2 class="label">Ambiente</h2>
    <div class="line">
      <svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 5h16v11H4zM2 19h20" /></svg>
      Este computador
    </div>
    {#if isAgent && conversation}
      <div class="line" title={conversation.cwd}>
        <svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>
        <span>{folderName(conversation.cwd)}</span>
        <span class="muted ellipsis">{parentOf(conversation.cwd)}</span>
      </div>
      {#if status?.branch}
        <div class="line">
          <svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="6" cy="6" r="2.2" /><circle cx="6" cy="18" r="2.2" /><circle cx="18" cy="8" r="2.2" /><path d="M6 8.2v7.6M18 10.2c0 4-6 3-11 5" /></svg>
          {status.branch}
        </div>
      {/if}
    {/if}
  </section>

  <hr />

  <section>
    <button type="button" class="line link" onclick={() => onnavigate("accounts")}>
      <span class="grow">Conta em uso</span>
      <span class="dot" class:none={!account}></span>
      <strong>{account?.name ?? "Nenhuma"}</strong>
    </button>
    {#if context !== null}
      <div class="line">
        <span class="grow">Contexto</span>
        <span class="muted">{context}%</span>
        <span class="meter" role="img" aria-label={`${context}% do contexto usado`}><span style:width={`${context}%`}></span></span>
      </div>
    {/if}
    <button type="button" class="line link" onclick={() => onnavigate("connectors")}>
      <span class="grow">Conectores</span>
      <span class={broken.length ? "warn" : "muted"}>{connectorSummary()}</span>
    </button>
  </section>

  {#if outputs.length}
    <hr />
    <section>
      <h2 class="label">Saídas</h2>
      {#each outputs as artifact (artifact.kind + artifact.title)}
        <button type="button" class="output" onclick={() => onopenartifact(artifact)}>
          <svg aria-hidden="true" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="var(--accent-text)" stroke-width="1.8" stroke-linejoin="round"><path d="M6 3h9l4 4v14H6zM14 3v5h5" /></svg>
          <span class="grow ellipsis">{artifact.title}</span>
          <span class="muted">{artifact.kind === "html" ? "Página" : "Imagem"}</span>
        </button>
      {/each}
    </section>
  {/if}

  {#if status?.is_repo}
    <hr />
    <section>
      <h2 class="label">Alterações</h2>
      {#if status.files.length}
        <div class="line">
          <span class="mono add">+{status.added.toLocaleString("pt-BR")}</span>
          <span class="mono del">−{status.removed.toLocaleString("pt-BR")}</span>
          <span class="grow muted">em {status.files.length} {status.files.length === 1 ? "arquivo" : "arquivos"}</span>
          <button type="button" class="btn small" onclick={() => (workspace.panel = "changes")}>Revisar e fazer commit</button>
        </div>
      {:else}
        <div class="line muted">Nada alterado desde o último commit.</div>
      {/if}
    </section>
  {/if}
</div>

<style>
  .panel { padding: 16px; display: flex; flex-direction: column; gap: 16px; overflow-y: auto; }
  section { display: flex; flex-direction: column; gap: 10px; }
  hr { width: 100%; height: 1px; margin: 0; border: 0; background: var(--border); }
  .line { display: flex; align-items: center; gap: 10px; min-height: 22px; color: var(--text); }
  .line svg { flex-shrink: 0; color: var(--text-2); }
  .link { width: 100%; padding: 0; border: 0; background: transparent; text-align: left; }
  .link:hover .grow { color: var(--accent-text); }
  .grow { flex: 1; min-width: 0; }
  .ellipsis { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); }
  .dot.none { background: var(--warn); }
  .warn { color: var(--warn); }
  .meter { width: 56px; height: 5px; border-radius: 3px; background: var(--border); overflow: hidden; }
  .meter span { display: block; height: 100%; border-radius: 3px; background: var(--bar); }
  .output { display: flex; align-items: center; gap: 10px; min-height: 36px; padding: 0 10px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); text-align: left; }
  .output:hover { background: var(--hover); }
  .add { color: var(--ok-text); font-size: 12.5px; }
  .del { color: var(--danger-text); font-size: 12.5px; }
</style>
