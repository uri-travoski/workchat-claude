<script lang="ts">
  import { app, unreadTotal, type Tab } from "../lib/app.svelte";
  import { initialsOf } from "../lib/people";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";

  // Desktop navigation rail. No Calls entry anywhere.
  const tabs: { id: Tab; label: string; icon: string }[] = [
    { id: "chats", label: "Chats", icon: "chat" },
    { id: "contacts", label: "Contacts", icon: "users" },
    { id: "settings", label: "Settings", icon: "gear" },
  ];
  const unread = $derived(unreadTotal());
</script>

<nav class="rail" aria-label="Main">
  <img class="logo" src="/app-icon.svg" alt="WorkChat" width="36" height="36" />
  <div class="tabs">
    {#each tabs as t}
      <button class:active={app.tab === t.id} title={t.label} aria-label={t.label} aria-current={app.tab === t.id ? "page" : undefined} onclick={() => (app.tab = t.id)}>
        <Icon name={t.icon} size={24} stroke={app.tab === t.id ? 2.1 : 1.8} />
        {#if t.id === "chats" && unread > 0}<span class="badge">{unread > 99 ? "99+" : unread}</span>{/if}
      </button>
    {/each}
  </div>
  <button class="me" title={app.me?.user.display_name ?? "Profile"} aria-label="Profile and settings" onclick={() => (app.tab = "settings")}>
    <Avatar id={app.me?.user.avatar ?? null} initials={initialsOf(app.me?.user.display_name ?? "?")} size={40} />
  </button>
</nav>

<style>
  .rail { width: 72px; flex: none; display: flex; flex-direction: column; align-items: center; gap: 18px; padding: 14px 0; background: linear-gradient(180deg, var(--navy) 0%, var(--navy-dark) 100%); }
  .logo { border-radius: 11px; background: #fff; padding: 3px; box-sizing: content-box; }
  .tabs { display: flex; flex-direction: column; gap: 6px; flex: 1; align-items: center; margin-top: 8px; }
  .tabs button { position: relative; width: 48px; height: 48px; border-radius: 14px; display: inline-flex; align-items: center; justify-content: center; color: rgba(255, 255, 255, 0.72); }
  .tabs button:hover { background: rgba(255, 255, 255, 0.1); color: #fff; }
  .tabs button.active { background: rgba(255, 255, 255, 0.18); color: #fff; }
  .badge { position: absolute; top: 2px; right: 0; min-width: 18px; height: 18px; padding: 0 5px; border-radius: 9px; background: #fff; color: var(--navy); font-size: 11px; font-weight: 700; display: inline-flex; align-items: center; justify-content: center; box-sizing: border-box; }
  .me { border-radius: 50%; border: 2px solid rgba(255, 255, 255, 0.5); }
</style>
