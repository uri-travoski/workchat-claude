<script lang="ts">
  import { app } from "../../lib/app.svelte";
  import { signOutAction } from "../../lib/actions";
  import Icon from "../Icon.svelte";

  // Desktop: the left column of the two-column settings (desktop/5.webp).
  const items: [typeof app.spage, string, string][] = [
    ["profile", "Profile", "user"],
    ["saved", "Saved Messages", "bookmark"],
    ["starred", "Starred & Pinned", "star"],
    ["notifications", "Notifications and Sounds", "bell"],
    ["privacy", "Privacy and Security", "shield"],
    ["storage", "Data and Storage", "pie"],
    ["appearance", "Chat Settings & Theme", "brush"],
    ["prefs", "User Preferences", "sliders"],
    ["server", "Server", "server"],
    ["devices", "Devices & Sessions", "laptop"],
    ["about", "About", "info"],
  ];
  const cur = $derived(app.spage || "profile");
</script>

<nav class="nav" aria-label="Settings">
  <h1>Settings</h1>
  {#each items as [id, label, icon]}
    <button class:on={cur === id} onclick={() => (app.spage = id)}><Icon name={icon} size={18} /><span>{label}</span>{#if id === "about" && app.upd.found}<i class="st-dot">1</i>{/if}</button>
  {/each}
  <button class="out" onclick={signOutAction}><Icon name="logout" size={18} /><span>Log Out</span></button>
</nav>

<style>
  .nav { display: flex; flex-direction: column; padding: 14px 10px; gap: 2px; height: 100%; overflow-y: auto; }
  h1 { margin: 6px 10px 10px; font-size: 22px; letter-spacing: -0.02em; }
  button { display: flex; align-items: center; gap: 12px; padding: 11px 12px; border-radius: 10px; text-align: left; font-size: 15.5px; color: var(--text); }
  button:hover { background: var(--hover); }
  button.on { background: var(--surface); font-weight: 700; box-shadow: var(--shadow); }
  button span { flex: 1; }
  .out { margin-top: 12px; color: var(--danger); }
  :global(.nav .st-dot) { min-width: 18px; height: 18px; padding: 0 5px; border-radius: 9px; background: var(--badge); color: #fff; font-size: 11.5px; font-weight: 700; display: inline-flex; align-items: center; justify-content: center; font-style: normal; }
</style>
