<script lang="ts">
  import { accounts, setFlag } from "../state/accounts.svelte";
  import type { FlagKey } from "../types";
  import Toggle from "./Toggle.svelte";

  interface Props {
    flag: FlagKey;
    title: string;
    description: string;
  }
  let { flag, title, description }: Props = $props();
</script>

<div class="row">
  <div class="text">
    <div class="title">{title}</div>
    <div class="muted small">{description}</div>
  </div>
  <Toggle checked={accounts.flags[flag]} label={title} onchange={(value) => setFlag(flag, value)} />
</div>

<style>
  .row { gap: 16px; }
  .text { flex: 1; }
  .title { font-weight: 500; }
</style>
