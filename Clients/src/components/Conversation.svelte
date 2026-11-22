<script lang="ts">
  import { app, backToLatest, closeConversation, convOf, ensureConv, fail, jumpTo, loadOlder, openConversation, userName } from "../lib/app.svelte";
  import { channelIcon } from "../lib/chat";
  import { core, type MessageView, type SearchResponse } from "../lib/core";
  import { clock, dayLabel } from "../lib/format";
  import { lastSeen } from "../lib/format";
  import { toPlain } from "../lib/markup";
  import Avatar from "./Avatar.svelte";
  import Composer from "./Composer.svelte";
  import Icon from "./Icon.svelte";
  import Menu from "./Menu.svelte";
  import { lockChat, unlockChat } from "../lib/lock";
  import MessageList from "./MessageList.svelte";

  let { id }: { id: string } = $props();
  const row = $derived(app.rows[id]);
  const c = $derived(convOf(id));
  const group = $derived(row?.kind === "channel" || row?.kind === "group_dm");
  let editing = $state<MessageView | null>(null);
  let finding = $state(false);
  let q = $state("");
  let menu = $state<{ x: number; y: number } | null>(null);

  $effect(() => {
    id;
    editing = null;
    finding = false;
    q = "";
  });

  const items = $derived(c.items);
  // In-conversation search asks the server (it also finds older messages that are not loaded).
  let found = $state<SearchResponse | null>(null);
  let sgen = 0;
  $effect(() => {
    const text = q.trim();
    if (text.length < 2) {
      found = null;
      return;
    }
    const my = ++sgen;
    const t = setTimeout(() => {
      core.search(text, id, 40).then((r) => my === sgen && (found = r), (e) => fail(e));
    }, 250);
    return () => clearTimeout(t);
  });
  const hits = $derived(found?.hits ?? []);
  const snippet = (s: string) => s.replace(/\[\[|\]\]/g, "");
  const pins = $derived(app.pins[id] ?? []);
  const pinned = $derived(pins.length ? (c.items.find((m) => m.id === pins[pins.length - 1]) ?? null) : null);

  const typingNames = $derived.by(() => {
    const t = app.typing[id];
    if (!t) return "";
    const names = Object.keys(t).filter((u) => t[u] > Date.now()).map(userName);
    if (!names.length) return "";
    return names.length === 1 ? `${names[0]} is typing…` : `${names.slice(0, 2).join(" and ")} are typing…`;
  });

  const subtitle = $derived.by(() => {
    if (!row) return "";
    if (typingNames) return typingNames;
    if (row.kind === "dm") {
      const u = row.peer_id ? app.users[row.peer_id] : undefined;
      if (row.presence === "online") return "Active now";
      if (row.presence === "away") return "Away";
      if (row.presence === "dnd") return "Do not disturb";
      return u?.last_active_ms ? lastSeen(u.last_active_ms) : "Offline";
    }
    if (row.kind === "self") return "Notes to self";
    const n = `${row.member_count} ${row.member_count === 1 ? "member" : "members"}`;
    return row.topic ? `${row.topic} · ${n}` : n;
  });
  const hasLocal = $derived(row?.avatar_source === "local");
  async function setLocal() {
    try {
      const [st] = await core.pickFiles("media");
      if (!st) return;
      const target = row.kind === "dm" && row.peer_id ? ({ kind: "local", what: "user", id: row.peer_id } as const) : ({ kind: "local", what: "conv", id } as const);
      app.sheet = { kind: "crop", stagedId: st.id, target };
    } catch (e) {
      fail(e);
    }
  }
  async function removeLocal() {
    try {
      if (row.kind === "dm" && row.peer_id) await core.removeLocalAvatar("user", row.peer_id);
      else await core.removeLocalAvatar("conv", id);
    } catch (e) {
      fail(e);
    }
  }
  const online = $derived(row?.kind === "dm" && row.presence === "online");

  async function toggleFav() {
    try {
      await core.setFavourite(id, !row.favourite);
    } catch (e) {
      fail(e);
    }
  }
</script>

{#if row}
  <section class="conv" aria-label={row.title}>
    <header>
      {#if app.phone}<button class="ib" aria-label="Back" onclick={closeConversation}><Icon name="left" size={26} stroke={2.2} /></button>{/if}
      <button class="hd" aria-label="Chat details" onclick={() => (row.kind === "dm" && row.peer_id ? (app.sheet = { kind: "profile", userId: row.peer_id }) : (app.sheet = { kind: "channelinfo", conv: id }))}>
        <Avatar id={row.avatar} initials={row.initials} size={40} tile={row.kind === "channel"} icon={channelIcon(row.title)} presence={row.presence} />
        <span class="ti">
          <h2>{#if row.kind === "channel"}# {/if}{row.title}{#if row.locked}<span class="lk" title="Locked chat, currently unlocked" aria-label="Locked chat, currently unlocked"><Icon name="lockopen" size={16} stroke={2} /></span>{/if}</h2>
          <span class="sub" class:on={online || !!typingNames}>{subtitle}</span>
        </span>
      </button>
      <button class="ib" aria-label="Search in conversation" onclick={() => (finding = !finding)}><Icon name="search" size={22} /></button>
      <button class="ib" aria-label="More" onclick={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); menu = { x: r.right, y: r.bottom + 4 }; }}><Icon name="dots" size={22} /></button>
    </header>
    {#if finding}
      <div class="find"><Icon name="search" size={17} /><!-- svelte-ignore a11y_autofocus --><input autofocus placeholder="Find in this conversation" bind:value={q} aria-label="Find in this conversation" /><span class="cnt">{q.trim().length >= 2 && found ? hits.length : ""}</span><button class="ib sm" aria-label="Close" onclick={() => { finding = false; q = ""; }}><Icon name="x" size={17} /></button></div>
    {/if}

    {#if finding && q.trim().length >= 2}
      <div class="results scroll">
        {#each hits as h (h.message.id)}
          <button class="hit" onclick={() => { finding = false; q = ""; void jumpTo(id, h.message.id, h.message.seq); }}>
            <b>{userName(h.message.sender_id)} <small>{dayLabel(h.message.ts)}, {clock(h.message.ts)}</small></b>
            <span>{snippet(h.snippet)}</span>
          </button>
        {:else}
          <p class="state">{found ? "No messages match." : "Searching…"}</p>
        {/each}
      </div>
    {:else if pins.length}
      <button class="pinbar" onclick={() => (app.sheet = { kind: "pins", conv: id })}>
        <Icon name="pin" size={15} fill /><span class="pt"><b>Pinned{pins.length > 1 ? ` (${pins.length})` : ""}</b>{#if pinned}<span>{toPlain(pinned.body) || pinned.attachments[0]?.name || "Attachment"}</span>{/if}</span><Icon name="right" size={17} />
      </button>
    {/if}
    {#if finding && q.trim().length >= 2}
      <!-- results are listed above -->
    {:else if c.error && !c.items.length}
      <div class="state"><p>{c.error}</p><button class="btn" onclick={() => { ensureConv(id).loaded = false; void openConversation(id); }}>Try again</button></div>
    {:else if !c.loaded && c.loading}
      <div class="state"><p>Loading messages…</p></div>
    {:else if !c.items.length}
      <div class="state"><p>No messages yet. Say hello!</p></div>
    {:else}
      <MessageList convId={id} {items} {group} hasMore={c.hasMore} loading={c.loading} loadMore={() => loadOlder(id)} onedit={(m) => (editing = m)} {typingNames} />
    {/if}

    {#if app.conn !== "online"}<div class="offline" role="status"><Icon name="wifi" size={15} /> {app.conn === "connecting" ? "Connecting…" : "Offline. Messages will send when you are back online."}</div>{/if}
    {#if c.detached}<button class="latest" onclick={() => backToLatest(id)}><Icon name="down" size={16} /> Jump to latest messages</button>{/if}
    <Composer convId={id} {editing} oncancelEdit={() => (editing = null)} placeholder="Message…" />
  </section>
{/if}

{#if menu && row}
  <Menu x={menu.x} y={menu.y} right onclose={() => (menu = null)} items={[
    { label: row.favourite ? "Unpin" : "Pin to top", icon: "pin", run: toggleFav },
    { label: "Mark as read", icon: "check", disabled: !row.unread, run: () => void core.markRead(id).catch(fail) },
    row.locked ? { label: "Unlock", icon: "lockopen", run: () => unlockChat(id) } : { label: "Lock", icon: "lock", run: () => lockChat(id) },
    { label: "Pinned messages", icon: "pin", disabled: !pins.length, run: () => (app.sheet = { kind: "pins", conv: id }) },
    { label: row.kind === "dm" ? "Contact info" : "Chat info", icon: "info", run: () => (row.kind === "dm" && row.peer_id ? (app.sheet = { kind: "profile", userId: row.peer_id }) : (app.sheet = { kind: "channelinfo", conv: id })) },
    { label: hasLocal ? "Change local picture" : "Set local picture", icon: "image", run: setLocal },
    ...(hasLocal ? [{ label: "Remove local picture", icon: "trash", run: removeLocal }] : []),
  ]} />
{/if}

<style>
  .conv { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--page); min-width: 0; }
  header { display: flex; align-items: center; gap: 10px; padding: calc(var(--safe-top) + 8px) 8px 8px 10px; background: var(--surface); border-bottom: 1px solid var(--line); flex: none; }
  .hd { flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; text-align: left; border-radius: 10px; padding: 2px 4px; }
  .hd:hover { background: var(--hover); }
  .ti { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  h2 { margin: 0; font-size: 17px; font-weight: 700; letter-spacing: -0.01em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .lk { display: inline-flex; vertical-align: -2px; margin-left: 6px; color: var(--blue); }
  .sub { font-size: 13px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sub.on { color: var(--blue); font-weight: 600; }
  .ib { width: 40px; height: 40px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; flex: none; }
  .ib:hover { background: var(--hover); }
  .ib.sm { width: 30px; height: 30px; }
  .find { display: flex; align-items: center; gap: 8px; padding: 6px 12px; background: var(--surface); border-bottom: 1px solid var(--line); color: var(--muted); }
  .find input { flex: 1; border: 0; background: var(--chip); border-radius: 16px; padding: 7px 12px; outline: none; font-size: 15px; }
  .pinbar { display: flex; align-items: center; gap: 10px; padding: 7px 14px; background: var(--surface); border-bottom: 1px solid var(--line); color: var(--blue); text-align: left; flex: none; }
  .pt { flex: 1; min-width: 0; display: flex; flex-direction: column; line-height: 1.25; }
  .pt b { font-size: 13px; }
  .pt span { color: var(--text-2); font-size: 13.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .results { flex: 1; min-height: 0; background: var(--surface); }
  .hit { width: 100%; display: flex; flex-direction: column; gap: 2px; text-align: left; padding: 10px 16px; border-bottom: 1px solid var(--line); }
  .hit:hover { background: var(--hover); }
  .hit small { color: var(--muted); font-weight: 400; }
  .hit span { color: var(--text-2); font-size: 14.5px; overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .latest { align-self: center; margin: 4px 0; background: var(--accent); color: #fff; border-radius: 16px; padding: 6px 14px; font-size: 13.5px; font-weight: 600; display: inline-flex; gap: 6px; align-items: center; }
  .cnt { font-size: 12.5px; font-weight: 600; }
  .state { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; color: var(--muted); padding: 24px; text-align: center; }
  .btn { background: var(--accent); color: #fff; border-radius: 12px; padding: 9px 18px; font-weight: 600; }
  .offline { background: color-mix(in srgb, #f5b73b 20%, var(--surface)); color: var(--text); font-size: 13px; padding: 6px 14px; display: flex; align-items: center; gap: 8px; flex: none; }
</style>
