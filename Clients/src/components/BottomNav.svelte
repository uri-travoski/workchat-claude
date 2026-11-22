<script lang="ts">
  import { app, unreadTotal, type Tab } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";

  const tabs: { id: Tab; label: string; icon: string }[] = [
    { id: "chats", label: "Chats", icon: "chat" },
    { id: "contacts", label: "Contacts", icon: "users" },
    { id: "settings", label: "Settings", icon: "gear" },
  ];
  const unread = $derived(unreadTotal());
</script>

<nav aria-label="Main">
  {#each tabs as t}
    <button class:active={app.tab === t.id} onclick={() => (app.tab = t.id)} aria-current={app.tab === t.id ? "page" : undefined}>
      <span class="ico">
        <Icon name={t.icon} size={26} stroke={app.tab === t.id ? 2.1 : 1.8} />
        {#if t.id === "chats" && unread > 0}<span class="badge">{unread > 99 ? "99+" : unread}</span>{/if}
      </span>
      <span class="lbl">{t.label}</span>
    </button>
  {/each}
</nav>

<style>
  nav { display: flex; background: var(--surface); border-top: 1px solid var(--line); padding-bottom: var(--safe-bottom); flex: none; }
  button { flex: 1; display: flex; flex-direction: column; align-items: center; gap: 3px; padding: 9px 4px 8px; color: var(--text); }
  button.active { color: var(--blue); }
  .ico { position: relative; display: inline-flex; }
  .badge { position: absolute; top: -7px; right: -11px; min-width: 18px; height: 18px; padding: 0 5px; border-radius: 9px; background: var(--badge); color: #fff; font-size: 11px; font-weight: 700; display: inline-flex; align-items: center; justify-content: center; border: 2px solid var(--surface); box-sizing: content-box; }
  .lbl { font-size: 12.5px; font-weight: 500; }
  button.active .lbl { font-weight: 600; }
</style>
