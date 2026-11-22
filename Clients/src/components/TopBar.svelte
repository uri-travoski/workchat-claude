<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { initialsOf } from "../lib/people";
  import Avatar from "./Avatar.svelte";
  import TopSearch from "./TopSearch.svelte";

  // Desktop top bar: brand left, global search centred (messages, people, files), profile right. The native window frame stays.
</script>

<header class="top">
  <div class="brand"><img src="/app-icon.svg" alt="" width="26" height="26" /><b>WorkChat</b></div>
  <TopSearch />
  <button class="me" aria-label="Profile" title={app.me?.user.display_name ?? ""} onclick={() => (app.tab = "settings")}>
    <Avatar id={app.me?.user.avatar ?? null} initials={initialsOf(app.me?.user.display_name ?? "?")} size={34} />
  </button>
</header>

<style>
  .top { height: 52px; flex: none; display: grid; grid-template-columns: 1fr minmax(240px, 520px) 1fr; align-items: center; padding: 0 14px; background: var(--surface); border-bottom: 1px solid var(--line); }
  .brand { display: flex; align-items: center; gap: 9px; font-size: 16px; }
  .brand img { border-radius: 7px; }
  .me { justify-self: end; border-radius: 50%; }
</style>
