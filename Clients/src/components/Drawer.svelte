<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { registerLayer } from "../lib/overlays";
  import { initialsOf } from "../lib/people";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";

  // Phone: the menu behind the hamburger icon. Slides in from the left over a scrim; a tap on the scrim, a swipe to the
  // left, Escape or the system Back button closes it. Files, People and Settings open as full screens with a back arrow.
  const me = $derived(app.me?.user);
  const close = () => (app.drawer = false);
  $effect(() => registerLayer(close));

  type F = "all" | "dm" | "channel" | "threads" | "saved" | "files";
  function go(tab: "chats" | "contacts" | "settings", filter?: F) {
    app.drawer = false;
    app.tab = tab;
    if (tab === "settings") app.spage = "";
    if (filter) app.filter = filter;
    else if (app.filter === "files") app.filter = "all";
  }
  const views: { f: F; label: string; icon: string }[] = [
    { f: "all", label: "All Chats", icon: "allchats" },
    { f: "dm", label: "DMs", icon: "chat" },
    { f: "channel", label: "Channels", icon: "hash" },
    { f: "threads", label: "Threads", icon: "thread" },
    { f: "saved", label: "Saved", icon: "bookmark" },
  ];

  let dx = $state(0);
  let sx: number | null = null;
  let sy = 0;
  let horizontal = false;
  function tstart(e: TouchEvent) {
    sx = e.touches[0].clientX;
    sy = e.touches[0].clientY;
    horizontal = false;
  }
  function tmove(e: TouchEvent) {
    if (sx === null) return;
    const x = e.touches[0].clientX - sx;
    const y = e.touches[0].clientY - sy;
    if (!horizontal && Math.abs(x) > 10 && Math.abs(x) > Math.abs(y)) horizontal = true;
    if (horizontal) dx = Math.min(0, x);
  }
  function tend() {
    if (horizontal && dx < -70) close();
    dx = 0;
    sx = null;
  }
</script>

<div class="scrim" role="presentation" onclick={close}></div>
<aside aria-label="Menu" style={dx ? `transform:translateX(${dx}px);transition:none` : ""} ontouchstart={tstart} ontouchmove={tmove} ontouchend={tend} ontouchcancel={tend}>
  {#if me}
    <button class="me" onclick={() => go("settings")} aria-label="Your profile and settings">
      <Avatar id={me.avatar ?? null} initials={initialsOf(me.display_name)} size={64} presence={app.conn === "online" ? "online" : null} />
      <b>{me.display_name}</b>
      <span>{me.title || "@" + me.username}</span>
    </button>
  {/if}
  <nav>
    {#each views as v (v.f)}
      <button class:on={app.tab === "chats" && app.filter === v.f} onclick={() => go("chats", v.f)}><Icon name={v.icon} size={22} /><span>{v.label}</span></button>
    {/each}
    <button class:on={app.tab === "chats" && app.filter === "files"} onclick={() => go("chats", "files")}><Icon name="folder" size={22} /><span>Files</span></button>
    <button onclick={() => go("contacts")}><Icon name="users" size={22} /><span>People</span></button>
    <button onclick={() => go("settings")}><Icon name="gear" size={22} /><span>Settings</span>{#if app.upd.found}<i class="udot"></i>{/if}</button>
  </nav>
  <p class="ver">WorkChat {app.info.version}</p>
</aside>

<style>
  .scrim { position: fixed; inset: 0; z-index: 60; background: rgba(15, 24, 40, 0.5); animation: fade 0.18s ease-out; }
  aside { position: fixed; top: 0; bottom: 0; left: 0; z-index: 61; width: min(300px, 82vw); display: flex; flex-direction: column; background: var(--surface); box-shadow: var(--shadow-pop); padding: var(--safe-top) 0 var(--safe-bottom); animation: slide 0.2s ease-out; touch-action: pan-y; }
  .me { display: flex; flex-direction: column; align-items: flex-start; gap: 3px; padding: 22px 20px 18px; text-align: left; border-bottom: 1px solid var(--line); background: var(--page); }
  .me b { margin-top: 10px; font-size: 18px; letter-spacing: -0.01em; }
  .me span { color: var(--muted); font-size: 14px; }
  nav { padding: 8px 10px; display: flex; flex-direction: column; gap: 2px; }
  nav button { display: flex; align-items: center; gap: 16px; padding: 13px 12px; border-radius: 12px; font-size: 16.5px; font-weight: 600; color: var(--text); text-align: left; }
  nav button:hover, nav button:active { background: var(--hover); }
  nav button.on { background: var(--hover); color: var(--blue); }
  nav :global(svg) { color: var(--blue); }
  .udot { width: 8px; height: 8px; border-radius: 50%; background: #5b93e8; }
  .ver { margin: auto 0 0; padding: 14px 20px; color: var(--muted); font-size: 13px; }
  @keyframes fade { from { opacity: 0; } }
  @keyframes slide { from { transform: translateX(-100%); } }
</style>
