<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { initialsOf } from "../lib/people";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";

  // Desktop top bar: brand left, global search centred, profile right. The native window frame stays.
  function onsearch() {
    app.tab = "chats";
  }
</script>

<header class="top">
  <div class="brand"><img src="/app-icon.svg" alt="" width="26" height="26" /><b>WorkChat</b></div>
  <label class="gs">
    <Icon name="search" size={18} />
    <input placeholder="Search" aria-label="Search" bind:value={app.query} oninput={onsearch} />
    {#if app.query}<button type="button" aria-label="Clear search" onclick={() => (app.query = "")}><Icon name="x" size={16} /></button>{/if}
  </label>
  <button class="me" aria-label="Profile" title={app.me?.user.display_name ?? ""} onclick={() => (app.tab = "settings")}>
    <Avatar id={app.me?.user.avatar ?? null} initials={initialsOf(app.me?.user.display_name ?? "?")} size={34} />
  </button>
</header>

<style>
  .top { height: 52px; flex: none; display: grid; grid-template-columns: 1fr minmax(240px, 520px) 1fr; align-items: center; padding: 0 14px; background: var(--surface); border-bottom: 1px solid var(--line); }
  .brand { display: flex; align-items: center; gap: 9px; font-size: 16px; }
  .brand img { border-radius: 7px; }
  .gs { display: flex; align-items: center; gap: 8px; background: var(--chip); border-radius: 18px; padding: 0 12px; height: 36px; color: var(--muted); }
  .gs input { flex: 1; min-width: 0; border: 0; background: transparent; outline: none; color: var(--text); font-size: 15px; }
  .gs button { display: inline-flex; color: var(--muted); }
  .me { justify-self: end; border-radius: 50%; }
</style>
