<script lang="ts">
  import { app, closeConversation, fail, openConversation, openThread, prefs, savePrefs, sortedRows, type ChatSort } from "../lib/app.svelte";
  import { core, type ChatRow } from "../lib/core";
  import { byActivity, dropSlot, inKind, isDirect, moveId, nameMatches, sortChats } from "../lib/chat";
  import { listTime } from "../lib/format";
  import { toPlain } from "../lib/markup";
  import Avatar from "./Avatar.svelte";
  import ChatRowItem from "./ChatRowItem.svelte";
  import Icon from "./Icon.svelte";
  import Menu from "./Menu.svelte";
  import FilesView from "./FilesView.svelte";
  import SavedView from "./SavedView.svelte";
  import GlobalSearch from "./GlobalSearch.svelte";
  import { signOutAction } from "../lib/actions";
  import { lockNow, openLockedChats } from "../lib/lock";

  const rows = $derived(sortedRows().filter((r) => !r.archived));
  const dms = $derived(rows.filter(isDirect));
  const channels = $derived(rows.filter((r) => r.kind === "channel"));
  const threadList = $derived(Object.values(app.threads).sort((a, b) => b.last_reply_ts - a.last_reply_ts));
  const anyUnread = $derived(rows.some((r) => r.unread > 0 || r.marked_unread));

  // The chat list: what the rail item (desktop) or pill (phone) selects, narrowed by the list box (names only).
  const kind = $derived(app.filter === "dm" ? "dm" : app.filter === "channel" ? "channel" : "all");
  const isChats = $derived(app.filter === "all" || app.filter === "dm" || app.filter === "channel");
  const shown = $derived(rows.filter((r) => inKind(r, kind) && nameMatches(r, app.listq)));

  // Pinned chats keep the order the person gave them (server position, else the one saved on this device).
  const LOCAL_KEY = "workchat.pinorder";
  let localOrder = $state<string[]>((() => {
    try {
      return JSON.parse(localStorage.getItem(LOCAL_KEY) ?? "[]");
    } catch {
      return [];
    }
  })());
  const pinRank = (r: ChatRow) => r.position ?? (localOrder.includes(r.id) ? 1e9 + localOrder.indexOf(r.id) : Number.MAX_SAFE_INTEGER);
  const pinned = $derived(shown.filter((r) => r.favourite).sort((a, b) => pinRank(a) - pinRank(b) || byActivity(a, b)));
  const rest = $derived(sortChats(shown.filter((r) => !r.favourite), prefs.chatSort));
  const restTitle = $derived(kind === "dm" ? "DIRECT MESSAGES" : kind === "channel" ? "CHANNELS" : "ALL CHATS");
  const sortLabel = $derived(prefs.chatSort === "az" ? "A to Z" : prefs.chatSort === "za" ? "Z to A" : "Recent");

  // Drag a pinned chat (whole row with the mouse, the grip with touch) to a new place; a line shows where it will land.
  let drag = $state<{ id: string; startY: number; dy: number; active: boolean; slot: number } | null>(null);
  let suppressClick = false;
  function pdown(e: PointerEvent, id: string) {
    if (e.button !== 0 || pinned.length < 2) return;
    const grip = (e.target as HTMLElement).closest(".grip");
    if (e.pointerType !== "mouse" && !grip) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { id, startY: e.clientY, dy: 0, active: false, slot: pinned.findIndex((r) => r.id === id) };
  }
  function pmove(e: PointerEvent) {
    if (!drag) return;
    drag.dy = e.clientY - drag.startY;
    if (!drag.active && Math.abs(drag.dy) > 5) drag.active = true;
    if (drag.active) {
      const mids = [...document.querySelectorAll<HTMLElement>("[data-pin]")].map((el) => {
        const r = el.getBoundingClientRect();
        return r.top + r.height / 2;
      });
      drag.slot = dropSlot(mids, e.clientY);
    }
  }
  async function pup() {
    const d = drag;
    drag = null;
    if (!d?.active) return;
    suppressClick = true;
    setTimeout(() => (suppressClick = false), 0);
    const ids = pinned.map((r) => r.id);
    const next = moveId(ids, d.id, d.slot);
    if (next.join() === ids.join()) return;
    localOrder = next;
    try {
      await core.orderConversations(next);
    } catch {
      // The server has no order endpoint (or refused): keep the order on this device.
      try {
        localStorage.setItem(LOCAL_KEY, JSON.stringify(next));
      } catch {
        /* private mode */
      }
    }
  }
  const dropClass = (i: number) => (drag?.active && drag.slot === i ? "drop-top" : drag?.active && i === pinned.length - 1 && drag.slot === pinned.length ? "drop-bottom" : "");

  let menu = $state<{ x: number; y: number } | null>(null);
  let sortMenu = $state<{ x: number; y: number } | null>(null);
  let searchEl = $state<HTMLInputElement>();
  $effect(() => {
    if (app.searching) searchEl?.focus();
  });
  const setSort = (s: ChatSort) => {
    prefs.chatSort = s;
    savePrefs();
  };

  const connLabel = $derived(app.conn === "online" ? "Online" : app.conn === "connecting" ? "Connecting…" : "Offline");

  function pill(f: typeof app.filter) {
    app.filter = f;
  }
  const nDms = $derived(app.counts?.dms ?? dms.length);
  const nChans = $derived(app.counts?.channels ?? channels.length);
  const nThreads = $derived(app.counts?.threads ?? threadList.length);
  const nFiles = $derived(app.counts?.files ?? 0);
  const viewTitle = $derived(app.filter === "threads" ? "Threads" : app.filter === "files" ? "Files" : app.filter === "saved" ? "Saved" : app.filter === "dm" ? "DMs" : app.filter === "channel" ? "Channels" : "All Chats");
</script>

<div class="list">
  {#if app.phone}
  <header>
    <div class="ttl">
      <h1>Chats</h1>
      <span class="conn" class:off={app.conn !== "online"}><i></i>{connLabel}</span>
    </div>
    <div class="acts">
      <button class="ib" aria-label="Search" onclick={() => (app.searching = !app.searching)}><Icon name="search" size={22} /></button>
      <button class="ib" aria-label="More" onclick={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); menu = { x: r.right, y: r.bottom + 4 }; }}><Icon name="dots" size={22} /></button>
    </div>
  </header>
  {/if}

  {#if app.phone ? app.searching : isChats}
    <div class="search" class:desk={!app.phone}>
      <Icon name="search" size={18} />
      <input
        bind:this={searchEl}
        class="listq"
        placeholder={app.phone ? "Search chats" : `Filter ${viewTitle === "All Chats" ? "chats" : viewTitle}`}
        bind:value={app.listq}
        aria-label="Filter chats by name"
        onkeydown={(e) => { if (e.key === "Escape" && app.listq) { e.stopPropagation(); app.listq = ""; } }}
      />
      {#if app.phone}
        <button class="ib sm" aria-label="Close search" onclick={() => { app.searching = false; app.listq = ""; }}><Icon name="x" size={18} /></button>
      {:else}
        {#if app.listq}<button class="ib sm" aria-label="Clear filter" onclick={() => (app.listq = "")}><Icon name="x" size={18} /></button>{/if}
        <button class="ib sm" aria-label="More" onclick={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); menu = { x: r.right, y: r.bottom + 4 }; }}><Icon name="dots" size={20} /></button>
      {/if}
    </div>
  {:else if !app.phone}
    <div class="vhead"><h2>{viewTitle}</h2></div>
  {/if}

  {#if app.lock.open}
    <div class="lockbar" role="status"><Icon name="lockopen" size={15} /><span>Locked chats are showing</span><button onclick={lockNow}>Lock now</button></div>
  {/if}
  {#if app.phone}
  <div class="pills" role="tablist" aria-label="Filter chats">
    <button role="tab" aria-selected={app.filter === "all"} class:on={app.filter === "all"} onclick={() => pill("all")}>All{#if anyUnread}<i class="udot"></i>{/if}</button>
    <button role="tab" aria-selected={app.filter === "dm"} class:on={app.filter === "dm"} onclick={() => pill("dm")}>DMs{#if nDms}<em>{nDms}</em>{/if}</button>
    <button role="tab" aria-selected={app.filter === "channel"} class:on={app.filter === "channel"} onclick={() => pill("channel")}>Channels{#if nChans}<em>{nChans}</em>{/if}</button>
    <button role="tab" aria-selected={app.filter === "threads"} class:on={app.filter === "threads"} onclick={() => pill("threads")}>Threads{#if nThreads}<em>{nThreads}</em>{/if}{#if (app.counts?.threads_unread ?? 0) > 0}<i class="udot"></i>{/if}</button>
    <button role="tab" aria-selected={app.filter === "files"} class:on={app.filter === "files"} onclick={() => pill("files")}>Files{#if nFiles}<em>{nFiles}</em>{/if}</button>
  </div>
  {/if}

  <div class="body" class:scroll={app.filter !== "files"} class:files={app.filter === "files"}>
    {#if app.filter === "threads"}
      {#if !threadList.length}
        <p class="empty">No threads yet. Replies you follow or join show up here.</p>
      {/if}
      {#each threadList as t (t.root.id)}
        {@const r = app.rows[t.conv_id]}
        <button class="trow" onclick={() => { void openConversation(t.conv_id).then(() => openThread(t.conv_id, t.root.id)); }}>
          <Avatar id={r?.avatar} initials={r?.initials ?? "?"} size={44} tile={r?.kind === "channel"} />
          <span class="tb">
            <span class="tt"><b>{r ? (r.kind === "channel" ? "# " : "") + r.title : "Thread"}</b><span class="tm">{listTime(t.last_reply_ts)}</span></span>
            <span class="tp">{app.users[t.root.sender_id]?.display_name ?? ""}: {toPlain(t.root.body) || "Attachment"}</span>
            <span class="tr"><Icon name="thread" size={14} /> {t.reply_count} {t.reply_count === 1 ? "reply" : "replies"}{#if t.unread}<span class="badge">{t.unread}</span>{/if}</span>
          </span>
        </button>
      {/each}
    {:else if app.filter === "files"}
      <FilesView />
    {:else if app.filter === "saved"}
      <SavedView />
    {:else}
      {#if !shown.length && !(app.phone && app.listq.trim().length >= 2)}
        <p class="empty">{app.listq ? "No chats match that name." : app.conn === "connecting" ? "Loading your chats…" : "No chats yet. Tap the pencil to start one."}</p>
      {/if}
      {#if pinned.length}
        <div class="sec"><span><Icon name="pin" size={13} stroke={2} /> PINNED ({pinned.length})</span>{#if pinned.length > 1}<small class="hint">drag to reorder</small>{/if}</div>
        {#each pinned as r, i (r.id)}
          <div
            class="prow {dropClass(i)}"
            class:dragging={drag?.active && drag.id === r.id}
            class:multi={pinned.length > 1}
            data-pin={r.id}
            role="presentation"
            style={drag?.active && drag.id === r.id ? `transform:translateY(${drag.dy}px)` : ""}
            onpointerdown={(e) => pdown(e, r.id)}
            onpointermove={pmove}
            onpointerup={pup}
            onpointercancel={pup}
            onclickcapture={(e) => { if (suppressClick) { e.stopPropagation(); e.preventDefault(); } }}
          >
            {#if pinned.length > 1}<span class="grip" aria-label="Drag to reorder" title="Drag to reorder"></span>{/if}
            <ChatRowItem row={r} selected={app.selected === r.id} onopen={() => openConversation(r.id)} />
          </div>
        {/each}
      {/if}
      {#if rest.length}
        <div class="sec">
          <span><Icon name={kind === "channel" ? "hash" : kind === "dm" ? "user" : "chat"} size={13} stroke={2} /> {restTitle} ({rest.length})</span>
          <button class="sortb" aria-label={`Sort: ${sortLabel}`} aria-haspopup="menu" onclick={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); sortMenu = { x: r.right, y: r.bottom + 4 }; }}>{sortLabel} <Icon name="down" size={13} stroke={2.2} /></button>
        </div>
        {#each rest as r (r.id)}<ChatRowItem row={r} selected={app.selected === r.id} onopen={() => openConversation(r.id)} />{/each}
      {/if}
      {#if app.phone && app.listq.trim().length >= 2}<GlobalSearch q={app.listq} />{/if}
      <div class="pad"></div>
    {/if}
  </div>

  {#if isChats || app.phone}<button class="fab" aria-label="New message" onclick={() => (app.sheet = { kind: "newchat" })}><Icon name="pencil" size={24} /></button>{/if}
</div>

{#if sortMenu}
  <Menu
    x={sortMenu.x}
    y={sortMenu.y}
    right
    onclose={() => (sortMenu = null)}
    items={[
      { label: "Recent activity", icon: prefs.chatSort === "recent" ? "check" : undefined, run: () => setSort("recent") },
      { label: "Name A to Z", icon: prefs.chatSort === "az" ? "check" : undefined, run: () => setSort("az") },
      { label: "Name Z to A", icon: prefs.chatSort === "za" ? "check" : undefined, run: () => setSort("za") },
    ]}
  />
{/if}

{#if menu}
  <Menu
    x={menu.x}
    y={menu.y}
    right
    onclose={() => (menu = null)}
    items={[
      { label: "New message", icon: "pencil", run: () => (app.sheet = { kind: "newchat" }) },
      { label: "Locked chats", icon: "lock", run: openLockedChats },
      { label: "Reconnect", icon: "retry", run: () => void core.reconnect() },
      { label: "Settings", icon: "gear", run: () => { closeConversation(); app.tab = "settings"; } },
      { label: "Sign out", icon: "logout", danger: true, run: () => void signOutAction() },
    ]}
  />
{/if}

<style>
  .list { position: relative; display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--page); }
  header { display: flex; align-items: center; justify-content: space-between; padding: calc(var(--safe-top) + 12px) 12px 8px 18px; background: var(--surface); }
  h1 { margin: 0; font-size: 22px; font-weight: 700; letter-spacing: -0.02em; line-height: 1.15; }
  .conn { display: inline-flex; align-items: center; gap: 6px; font-size: 12.5px; font-weight: 600; color: var(--blue); }
  .conn i { width: 8px; height: 8px; border-radius: 50%; background: var(--blue); }
  .conn.off { color: var(--muted); }
  .conn.off i { background: var(--muted-2); }
  .acts { display: flex; gap: 2px; }
  .ib { width: 40px; height: 40px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; color: var(--text); }
  .ib:hover { background: var(--hover); }
  .ib.sm { width: 32px; height: 32px; }
  .search { display: flex; align-items: center; gap: 8px; margin: 0; padding: 4px 12px 10px 18px; background: var(--surface); color: var(--muted); }
  .search.desk { padding: 12px 12px 4px 16px; background: var(--page); }
  .search input { flex: 1; border: 0; background: var(--chip); border-radius: 18px; padding: 9px 14px; outline: none; color: var(--text); font-size: 15px; }
  /* Five pills must fit 360 to 412 px wide phones: they share the row and shrink their padding. */
  .lockbar { display: flex; align-items: center; gap: 8px; margin: 6px 12px 0; padding: 7px 12px; border-radius: 12px; background: color-mix(in srgb, var(--blue) 12%, var(--surface)); color: var(--text-2); font-size: 13.5px; font-weight: 600; }
  .lockbar span { flex: 1; }
  .lockbar button { color: var(--blue); font-weight: 700; }
  .pills { display: flex; gap: 6px; padding: 10px 12px 12px; background: var(--page); }
  .pills button { flex: 1 1 auto; min-width: 0; height: 36px; padding: 0 6px; border-radius: 18px; background: var(--pill); border: 1px solid var(--pill-line); font-size: 13px; font-weight: 600; display: inline-flex; align-items: center; justify-content: center; gap: 4px; box-shadow: var(--shadow); color: var(--text); white-space: nowrap; }
  .pills button.on { background: var(--accent); border-color: var(--accent); color: #fff; }
  .pills em { font-style: normal; font-weight: 600; color: var(--muted); font-size: 11.5px; }
  .pills .on em { color: #c9d6ee; }
  .udot { width: 7px; height: 7px; flex: none; border-radius: 50%; background: #5b93e8; }
  .body { flex: 1; min-height: 0; }
  .body.files { display: flex; flex-direction: column; overflow: hidden; }
  .sec { display: flex; align-items: center; justify-content: space-between; padding: 14px 18px 7px; font-size: 12px; font-weight: 700; letter-spacing: 0.07em; color: var(--text-2); background: var(--page); }
  .sec span { display: inline-flex; align-items: center; gap: 7px; }
  .hint { font-size: 11.5px; font-weight: 500; letter-spacing: 0; color: var(--muted); }
  .sortb { display: inline-flex; align-items: center; gap: 4px; color: var(--blue); font-weight: 600; font-size: 12.5px; letter-spacing: 0; padding: 2px 6px; border-radius: 8px; }
  .sortb:hover { background: var(--hover); }
  .vhead { padding: 16px 18px 8px; background: var(--page); }
  .vhead h2 { margin: 0; font-size: 18px; letter-spacing: -0.01em; }
  .prow { position: relative; background: var(--surface); }
  .prow.multi { cursor: grab; }
  .prow.dragging { z-index: 3; opacity: 0.88; box-shadow: var(--shadow-pop); cursor: grabbing; }
  .prow.drop-top::before, .prow.drop-bottom::after { content: ""; position: absolute; left: 12px; right: 12px; height: 3px; border-radius: 2px; background: var(--blue); z-index: 4; }
  .prow.drop-top::before { top: -2px; }
  .prow.drop-bottom::after { bottom: -2px; }
  .grip { position: absolute; left: 3px; top: 50%; width: 10px; height: 18px; margin-top: -9px; z-index: 2; opacity: 0; transition: opacity 0.12s; touch-action: none; cursor: grab; background: radial-gradient(circle, var(--muted-2) 1.3px, transparent 1.6px) 0 0 / 5px 6px; }
  .prow:hover .grip, .prow.dragging .grip { opacity: 1; }
  @media (pointer: coarse) { .grip { opacity: 0.8; width: 22px; left: 0; background-position: 6px 0; } }
  .empty { margin: 36px 24px; text-align: center; color: var(--muted); }
  .pad { height: 96px; }
  .fab { position: absolute; right: 18px; bottom: 18px; width: 58px; height: 58px; border-radius: 50%; background: linear-gradient(145deg, var(--accent), var(--accent-2)); color: #fff; display: inline-flex; align-items: center; justify-content: center; box-shadow: 0 6px 18px rgba(25, 39, 63, 0.38); }
  .fab:hover { filter: brightness(1.12); }
  .trow { width: 100%; display: flex; gap: 13px; align-items: center; padding: 11px 16px; text-align: left; background: var(--surface); border-bottom: 1px solid var(--line); }
  .trow:hover { background: var(--hover); }
  .tb { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .tt { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; }
  .tt b { font-size: 15.5px; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tm { font-size: 12.5px; color: var(--muted); flex: none; }
  .tp { font-size: 14px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tr { display: inline-flex; align-items: center; gap: 6px; font-size: 13.5px; color: var(--blue); font-weight: 600; }
  .badge { min-width: 20px; height: 20px; padding: 0 6px; border-radius: 10px; background: var(--badge); color: #fff; font-size: 12px; display: inline-flex; align-items: center; justify-content: center; }
</style>
