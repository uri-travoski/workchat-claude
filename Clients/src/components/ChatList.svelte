<script lang="ts">
  import { app, closeConversation, openConversation, openThread, prefs, savePrefs, sortedRows, unreadTotal, type ChatSort } from "../lib/app.svelte";
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
  import { lockMenuAction, lockNow, openLockedChats } from "../lib/lock";
  import { lockEntry, type MenuItem } from "../lib/menu";
  import { HOLD_MS, holdStep } from "../lib/hold";

  const rows = $derived(sortedRows().filter((r) => !r.archived));
  const dms = $derived(rows.filter(isDirect));
  const channels = $derived(rows.filter((r) => r.kind === "channel"));
  const threadList = $derived(Object.values(app.threads).sort((a, b) => b.last_reply_ts - a.last_reply_ts));

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

  // Rearranging pinned chats.
  //  Desktop: drag the row with the mouse (or its grip with touch).
  //  Phone: press and hold a pinned row (HOLD_MS, finger still): it lifts with a haptic tick. Then either drag it
  //  (a line shows where it will land; the list scrolls at its top and bottom edge) or let go without moving to open
  //  the chat's actions menu. Moving before the hold is up is just scrolling, which never lifts anything.
  type Drag = { id: string; startY: number; lastX: number; startScroll: number; lastY: number; dy: number; active: boolean; lifted: boolean; slot: number; x0: number; y0: number; t0: number; pid: number };
  let drag = $state<Drag | null>(null);
  let bodyEl = $state<HTMLDivElement>();
  let holdTimer: ReturnType<typeof setTimeout> | undefined;
  let raf = 0;
  let suppressClick = false;
  let lastType = "mouse";
  const quiet = () => {
    suppressClick = true;
    setTimeout(() => (suppressClick = false), 400);
  };
  function pdown(e: PointerEvent, id: string) {
    lastType = e.pointerType;
    if (e.button !== 0 || (!app.phone && pinned.length < 2)) return;
    if (!app.phone) {
      const grip = (e.target as HTMLElement).closest(".grip");
      if (e.pointerType !== "mouse" && !grip) return;
    }
    const el = e.currentTarget as HTMLElement;
    drag = { id, startY: e.clientY, startScroll: bodyEl?.scrollTop ?? 0, lastX: e.clientX, lastY: e.clientY, dy: 0, active: false, lifted: false, slot: pinned.findIndex((r) => r.id === id), x0: e.clientX, y0: e.clientY, t0: Date.now(), pid: e.pointerId };
    if (!app.phone) return el.setPointerCapture(e.pointerId);
    clearTimeout(holdTimer);
    holdTimer = setTimeout(() => {
      const d = drag;
      if (!d || d.lifted) return;
      if (holdStep(false, Date.now() - d.t0, Math.hypot(d.lastX - d.x0, d.lastY - d.y0)) !== "lift") return;
      d.lifted = true;
      navigator.vibrate?.(18);
      try {
        el.setPointerCapture(d.pid);
      } catch {
        /* pointer already gone */
      }
    }, HOLD_MS);
  }
  function pmove(e: PointerEvent) {
    if (!drag) return;
    drag.lastY = e.clientY;
    drag.lastX = e.clientX;
    if (app.phone) {
      const step = holdStep(drag.lifted, Date.now() - drag.t0, Math.hypot(e.clientX - drag.x0, e.clientY - drag.y0));
      if (step === "scroll") {
        clearTimeout(holdTimer);
        drag = null;
        return;
      }
      if (step === "drag" && !drag.active) {
        drag.active = true;
        drag.startY = e.clientY;
        drag.startScroll = bodyEl?.scrollTop ?? 0;
        loop();
      }
    } else if (!drag.active && Math.abs(e.clientY - drag.startY) > 5) drag.active = true;
    if (drag.active) track();
  }
  /** Follows the finger (also while the list scrolls under it) and works out the drop slot. */
  function track() {
    const d = drag;
    if (!d?.active) return;
    d.dy = d.lastY - d.startY + ((bodyEl?.scrollTop ?? 0) - d.startScroll);
    const mids = [...document.querySelectorAll<HTMLElement>("[data-pin]")].map((el) => {
      const r = el.getBoundingClientRect();
      return r.top + r.height / 2 - (el.dataset.pin === d.id ? d.dy : 0);
    });
    d.slot = dropSlot(mids, d.lastY);
  }
  function loop() {
    cancelAnimationFrame(raf);
    const tick = () => {
      if (!drag?.active || !bodyEl) return;
      const r = bodyEl.getBoundingClientRect();
      const edge = 64;
      const v = drag.lastY < r.top + edge ? -((r.top + edge - drag.lastY) / edge) * 14 : drag.lastY > r.bottom - edge ? ((drag.lastY - (r.bottom - edge)) / edge) * 14 : 0;
      if (v) {
        bodyEl.scrollTop += v;
        track();
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
  }
  // A lifted row must not scroll the list: the page's touchmove has to be cancelable, so this listener is not passive.
  $effect(() => {
    const el = bodyEl;
    if (!el) return;
    const stop = (e: TouchEvent) => {
      if (drag?.lifted && e.cancelable) e.preventDefault();
    };
    el.addEventListener("touchmove", stop, { passive: false });
    return () => el.removeEventListener("touchmove", stop);
  });
  async function pup(e: PointerEvent) {
    clearTimeout(holdTimer);
    cancelAnimationFrame(raf);
    const d = drag;
    drag = null;
    if (!d) return;
    if (app.phone && d.lifted && !d.active) {
      quiet();
      app.sheet = { kind: "chatactions", conv: d.id, x: e.clientX, y: e.clientY };
      return;
    }
    if (!d.active) return;
    quiet();
    if (e.type === "pointercancel") return;
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
  let pmenu = $state<{ x: number; y: number } | null>(null);
  let sortMenu = $state<{ x: number; y: number } | null>(null);
  let searchEl = $state<HTMLInputElement>();
  $effect(() => {
    if (app.searching) searchEl?.focus();
  });
  const setSort = (s: ChatSort) => {
    prefs.chatSort = s;
    savePrefs();
  };


  const unreadAll = $derived(unreadTotal());
  const filesPage = $derived(app.phone && app.filter === "files");
  const phoneItems = $derived.by((): MenuItem[] => {
    const l = lockEntry(app.lock);
    // Choosing a label shows the Saved list narrowed to it; "All labels" clears the narrowing.
    const byLabel = (id: string | null) => {
      app.filter = "saved";
      app.labelFilter = id;
    };
    return [
      { label: l.label, icon: l.label === "Lock" ? "lock" : "lockopen", run: () => lockMenuAction(l.act) },
      { label: "Sort chats", heading: true, run: () => {} },
      { label: "Recent", checked: prefs.chatSort !== "az" && prefs.chatSort !== "za", run: () => setSort("recent") },
      { label: "Alphabetically", checked: prefs.chatSort === "az" || prefs.chatSort === "za", run: () => setSort("az") },
      { label: "Filter by label", heading: true, run: () => {} },
      { label: "All labels", checked: !app.labelFilter, run: () => { app.labelFilter = null; } },
      ...app.labels.map((lb): MenuItem => ({ label: lb.name, checked: app.labelFilter === lb.id, run: () => byLabel(lb.id) })),
      { label: "Manage labels", icon: "tag", run: () => { app.tab = "settings"; app.spage = "labels"; } },
    ];
  });
  const viewTitle = $derived(app.filter === "threads" ? "Threads" : app.filter === "files" ? "Files" : app.filter === "saved" ? "Saved" : app.filter === "dm" ? "DMs" : app.filter === "channel" ? "Channels" : "All Chats");
</script>

<div class="list">
  {#if filesPage}
  <header>
    <button class="ib back" aria-label="Back" onclick={() => (app.filter = "all")}><Icon name="left" size={24} /></button>
    <div class="ttl"><h1>Files</h1></div>
  </header>
  {:else if app.phone}
  <header>
    <button class="ib menu" aria-label="Menu" aria-haspopup="dialog" onclick={() => (app.drawer = true)}>
      <Icon name="menu" size={24} />
      {#if unreadAll > 0}<span class="mb" aria-label={`${unreadAll} unread`}>{unreadAll > 99 ? "99+" : unreadAll}</span>{/if}
    </button>
    <div class="ttl">
      <h1>{viewTitle}</h1>
    </div>
    <div class="acts">
      <button class="ib" aria-label="Search" onclick={() => (app.searching = !app.searching)}><Icon name="search" size={22} /></button>
      <button class="ib" aria-label="More" aria-haspopup="menu" onclick={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); pmenu = { x: r.right, y: r.bottom + 4 }; }}><Icon name="dots" size={22} /></button>
    </div>
  </header>
  {/if}

  {#if filesPage}{:else if app.phone ? app.searching : isChats}
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

  <div class="body" bind:this={bodyEl} class:scroll={app.filter !== "files"} class:files={app.filter === "files"}>
    {#if app.filter === "threads"}
      {#if !threadList.length}
        <p class="empty">No threads yet. Replies you follow or join show up here.</p>
      {/if}
      {#each threadList as t (t.root.id)}
        {@const r = app.rows[t.conv_id]}
        <button class="trow" onclick={() => { void openConversation(t.conv_id).then(() => openThread(t.conv_id, t.root.id)); }}>
          <Avatar id={r?.avatar} initials={r?.initials ?? "?"} size={44} tile={r?.kind === "channel"} />
          <span class="tb">
            <span class="tt"><b>{r ? r.title : "Thread"}</b><span class="tm">{listTime(t.last_reply_ts)}</span></span>
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
        <p class="empty">{app.listq ? "No chats match that name." : app.conn === "connecting" ? "Loading your chats…" : "No chats yet. Tap + to start one."}</p>
      {/if}
      {#if pinned.length}
        <div class="sec"><span><Icon name="pin" size={13} stroke={2} /> PINNED ({pinned.length})</span>{#if pinned.length > 1 && !app.phone}<small class="hint">drag to reorder</small>{/if}</div>
        {#each pinned as r, i (r.id)}
          <div
            class="prow {dropClass(i)}"
            class:dragging={drag?.active && drag.id === r.id}
            class:lifted={app.phone && drag?.lifted && drag.id === r.id}
            class:multi={pinned.length > 1 && !app.phone}
            class:hold={app.phone}
            data-pin={r.id}
            role="presentation"
            style={drag?.active && drag.id === r.id ? `transform:translateY(${drag.dy}px) scale(1.02)` : ""}
            onpointerdown={(e) => pdown(e, r.id)}
            onpointermove={pmove}
            onpointerup={pup}
            onpointercancel={pup}
            onclickcapture={(e) => { if (suppressClick) { e.stopPropagation(); e.preventDefault(); } }}
            oncontextmenucapture={(e) => { if (app.phone && lastType !== "mouse") { e.stopPropagation(); e.preventDefault(); } }}
          >
            {#if pinned.length > 1 && !app.phone}<span class="grip" aria-label="Drag to reorder" title="Drag to reorder"></span>{/if}
            <ChatRowItem row={r} selected={app.selected === r.id} nolong={app.phone} onopen={() => openConversation(r.id)} />
          </div>
        {/each}
      {/if}
      {#if rest.length}
        <div class="sec">
          <span><Icon name={kind === "channel" ? "hash" : kind === "dm" ? "chat" : "allchats"} size={13} stroke={2} /> {restTitle} ({rest.length})</span>
          {#if !app.phone}<button class="sortb" aria-label={`Sort: ${sortLabel}`} aria-haspopup="menu" onclick={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); sortMenu = { x: r.right, y: r.bottom + 4 }; }}>{sortLabel} <Icon name="down" size={13} stroke={2.2} /></button>{/if}
        </div>
        {#each rest as r (r.id)}<ChatRowItem row={r} selected={app.selected === r.id} onopen={() => openConversation(r.id)} />{/each}
      {/if}
      {#if app.phone && app.listq.trim().length >= 2}<GlobalSearch q={app.listq} />{/if}
      <div class="pad"></div>
    {/if}
  </div>

  {#if (isChats || app.phone) && !filesPage}<button class="fab" aria-label="New message" onclick={() => (app.sheet = { kind: "newchat" })}><Icon name="plus" size={26} stroke={2.2} /></button>{/if}
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

{#if pmenu}
  <Menu x={pmenu.x} y={pmenu.y} right onclose={() => (pmenu = null)} items={phoneItems} />
{/if}

{#if menu}
  <Menu
    x={menu.x}
    y={menu.y}
    right
    onclose={() => (menu = null)}
    items={[
      { label: "New message", icon: "plus", run: () => (app.sheet = { kind: "newchat" }) },
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
  .acts { display: flex; gap: 2px; }
  .ib { width: 40px; height: 40px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; color: var(--text); }
  .ib:hover { background: var(--hover); }
  header { gap: 4px; }
  header .ttl { flex: 1; min-width: 0; }
  .ib.menu, .ib.back { position: relative; margin-left: -8px; margin-right: 2px; }
  .mb { position: absolute; top: 1px; right: -1px; min-width: 18px; height: 18px; padding: 0 5px; border-radius: 9px; background: var(--badge); color: #fff; font-size: 11px; font-weight: 700; display: inline-flex; align-items: center; justify-content: center; border: 2px solid var(--surface); box-sizing: content-box; transform: scale(0.85); transform-origin: top right; }
  .ib.sm { width: 32px; height: 32px; }
  .search { display: flex; align-items: center; gap: 8px; margin: 0; padding: 4px 12px 10px 18px; background: var(--surface); color: var(--muted); }
  .search.desk { padding: 12px 12px 4px 16px; background: var(--page); }
  .search input { flex: 1; border: 0; background: var(--chip); border-radius: 18px; padding: 9px 14px; outline: none; color: var(--text); font-size: 15px; }
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
  .prow.hold { -webkit-user-select: none; user-select: none; -webkit-touch-callout: none; transition: transform 0.12s, box-shadow 0.12s; }
  .prow.lifted { z-index: 3; transform: scale(1.02); box-shadow: var(--shadow-pop); border-radius: 12px; }
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
