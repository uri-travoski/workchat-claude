<script lang="ts">
  import { app, type Filter, type Tab } from "../lib/app.svelte";
  import { inKind, isDirect, unreadOf } from "../lib/chat";
  import Icon from "./Icon.svelte";

  // Desktop navigation rail (no app tile, no avatar). All Chats, DMs, Channels, Threads, Files, Saved, Contacts; Settings sits at the bottom.
  interface Item {
    id: string;
    label: string;
    icon: string;
    tab: Tab;
    filter?: Filter;
    badge?: number;
  }
  const rows = $derived(Object.values(app.rows).filter((r) => !r.archived));
  const items = $derived<Item[]>([
    { id: "all", label: "All Chats", icon: "chat", tab: "chats", filter: "all", badge: unreadOf(rows) },
    { id: "dm", label: "DMs", icon: "user", tab: "chats", filter: "dm", badge: unreadOf(rows.filter(isDirect)) },
    { id: "channel", label: "Channels", icon: "hash", tab: "chats", filter: "channel", badge: unreadOf(rows.filter((r) => inKind(r, "channel"))) },
    { id: "threads", label: "Threads", icon: "thread", tab: "chats", filter: "threads", badge: app.counts?.threads_unread ?? 0 },
    { id: "files", label: "Files", icon: "paperclip", tab: "chats", filter: "files" },
    { id: "saved", label: "Saved", icon: "bookmark", tab: "chats", filter: "saved" },
    { id: "contacts", label: "Contacts", icon: "users", tab: "contacts" },
  ]);
  const settings: Item = { id: "settings", label: "Settings", icon: "gear", tab: "settings" };
  const isActive = (it: Item) => app.tab === it.tab && (it.filter === undefined || app.filter === it.filter);
  function go(it: Item) {
    app.tab = it.tab;
    if (it.filter) app.filter = it.filter;
  }
</script>

{#snippet btn(it: Item)}
  <button class:active={isActive(it)} aria-label={it.label} aria-current={isActive(it) ? "page" : undefined} data-rail={it.id} onclick={() => go(it)}>
    <Icon name={it.icon} size={24} stroke={isActive(it) ? 2.1 : 1.8} />
    {#if it.badge}<span class="badge">{it.badge > 99 ? "99+" : it.badge}</span>{/if}
    <span class="tip" role="tooltip">{it.label}</span>
  </button>
{/snippet}

<nav class="rail" aria-label="Main">
  <div class="tabs">
    {#each items as it (it.id)}{@render btn(it)}{/each}
  </div>
  <div class="bottom">{@render btn(settings)}</div>
</nav>

<style>
  .rail { position: relative; z-index: 10; width: 72px; flex: none; display: flex; flex-direction: column; align-items: center; padding: 14px 0; background: linear-gradient(180deg, var(--navy) 0%, var(--navy-dark) 100%); }
  .tabs { display: flex; flex-direction: column; gap: 6px; flex: 1; align-items: center; }
  .bottom { display: flex; flex-direction: column; align-items: center; }
  button { position: relative; width: 48px; height: 48px; border-radius: 14px; display: inline-flex; align-items: center; justify-content: center; color: rgba(255, 255, 255, 0.72); }
  button:hover { background: rgba(255, 255, 255, 0.1); color: #fff; }
  button.active { background: rgba(255, 255, 255, 0.18); color: #fff; }
  button.active::before { content: ""; position: absolute; left: -12px; top: 12px; bottom: 12px; width: 3px; border-radius: 0 3px 3px 0; background: #fff; }
  .badge { position: absolute; top: 2px; right: 0; min-width: 18px; height: 18px; padding: 0 5px; border-radius: 9px; background: #fff; color: var(--navy); font-size: 11px; font-weight: 700; display: inline-flex; align-items: center; justify-content: center; box-sizing: border-box; }
  /* Label tooltip to the right of the rail on hover or keyboard focus. */
  .tip { position: absolute; left: calc(100% + 10px); top: 50%; transform: translateY(-50%); background: #1b2230; color: #fff; font-size: 13px; font-weight: 600; padding: 5px 10px; border-radius: 8px; white-space: nowrap; pointer-events: none; opacity: 0; transition: opacity 0.12s; box-shadow: var(--shadow-pop); }
  button:hover .tip, button:focus-visible .tip { opacity: 1; transition-delay: 0.25s; }
</style>
