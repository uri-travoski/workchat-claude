<script lang="ts">
  import { app } from "../../lib/app.svelte";
  import Icon from "../Icon.svelte";

  let { title }: { title: string } = $props();
</script>

{#if app.phone}
  <header class="st-sub">
    <button class="back" onclick={() => (app.spage = "")}><Icon name="left" size={20} stroke={2.2} /> Settings</button>
    <h1>{title}</h1>
    <button class="done" onclick={() => (app.spage = "")}>Done</button>
  </header>
{:else}
  <header class="dh"><h1>{title}</h1></header>
{/if}

<style>
  .dh { padding: 20px 24px 4px; }
  .dh h1 { margin: 0; font-size: 26px; letter-spacing: -0.02em; }
</style>
