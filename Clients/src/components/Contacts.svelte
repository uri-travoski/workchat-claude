<script lang="ts">
  import { app, startDm, userAvatar } from "../lib/app.svelte";
  import { core, type User } from "../lib/core";
  import { lastSeen } from "../lib/format";
  import { initialsOf } from "../lib/people";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";

  let q = $state("");
  let remote = $state<User[] | null>(null);
  // The directory comes from the server (GET /users?active=true&q=); the cached list shows at once and when offline.
  $effect(() => {
    const text = q.trim();
    const t = setTimeout(
      () => {
        core.listUsers(text || undefined).then((u) => (remote = u), () => (remote = null));
      },
      text ? 200 : 0,
    );
    return () => clearTimeout(t);
  });
  const local = $derived(
    Object.values(app.users).filter((u) => !u.disabled && (!q.trim() || u.display_name.toLowerCase().includes(q.trim().toLowerCase()) || u.username.toLowerCase().includes(q.trim().toLowerCase()))),
  );
  const people = $derived(
    (remote ?? local)
      .filter((u) => u.id !== app.me?.user.id && !u.disabled)
      .map((u) => app.users[u.id] ?? u)
      .sort((a, b) => a.display_name.localeCompare(b.display_name)),
  );
  const status = (u: User) => (u.presence === "online" ? "Active now" : u.presence === "away" ? "Away" : u.presence === "dnd" ? "Do not disturb" : u.last_active_ms ? lastSeen(u.last_active_ms) : "");
</script>

<div class="page">
  <header>{#if app.phone}<button class="back" aria-label="Back" onclick={() => (app.tab = "chats")}><Icon name="left" size={24} /></button>{/if}<h1>{app.phone ? "People" : "Contacts"}</h1><span class="count">{people.length}</span></header>
  <div class="search"><Icon name="search" size={18} /><input placeholder="Search people" bind:value={q} aria-label="Search people" /></div>
  <div class="body scroll">
    {#each people as u (u.id)}
      <div class="p">
        <button class="who" onclick={() => (app.sheet = { kind: "profile", userId: u.id })}>
          <Avatar id={userAvatar(u)} initials={initialsOf(u.display_name)} size={46} presence={u.presence ?? null} />
          <span class="t">
            <b>{u.display_name}</b>
            <span class="s">{u.title ?? "@" + u.username}</span>
            {#if status(u)}<span class="st" class:on={u.presence === "online"}>{status(u)}</span>{/if}
          </span>
        </button>
        <button class="go" aria-label={`Message ${u.display_name}`} title="Message" onclick={() => startDm(u.id)}><Icon name="chat" size={20} /></button>
      </div>
    {:else}
      <p class="empty">{q ? "Nobody matches." : "No other people on this server yet."}</p>
    {/each}
  </div>
</div>

<style>
  .page { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--page); }
  header { display: flex; align-items: baseline; gap: 10px; padding: calc(var(--safe-top) + 14px) 18px 8px; background: var(--surface); }
  h1 { margin: 0; font-size: 22px; letter-spacing: -0.02em; }
  .back { width: 40px; height: 40px; margin: -6px 0 -6px -10px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; align-self: center; }
  .back:hover { background: var(--hover); }
  .count { color: var(--muted); font-weight: 600; }
  .search { display: flex; align-items: center; gap: 8px; padding: 4px 16px 12px 18px; background: var(--surface); color: var(--muted); border-bottom: 1px solid var(--line); }
  .search input { flex: 1; border: 0; background: var(--chip); border-radius: 18px; padding: 9px 14px; outline: none; font-size: 15px; }
  .body { flex: 1; min-height: 0; background: var(--surface); }
  .p { display: flex; align-items: center; }
  .who { flex: 1; min-width: 0; display: flex; align-items: center; gap: 14px; padding: 10px 6px 10px 18px; text-align: left; }
  .who:hover { background: var(--hover); }
  .t { flex: 1; min-width: 0; display: flex; flex-direction: column; color: var(--text); }
  .t b { font-size: 16px; }
  .s { font-size: 13.5px; color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .st { font-size: 12.5px; color: var(--muted); }
  .st.on { color: var(--blue); font-weight: 600; }
  .go { width: 44px; height: 44px; margin-right: 12px; border-radius: 50%; color: var(--blue); display: inline-flex; align-items: center; justify-content: center; flex: none; }
  .go:hover { background: var(--hover); }
  .empty { text-align: center; color: var(--muted); margin: 36px 24px; }
</style>
