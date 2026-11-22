<script lang="ts">
  import { app, closeConversation, fail, openConversation, openThread, sortedRows } from "../lib/app.svelte";
  import { core, type ChatRow } from "../lib/core";
  import { byActivity, isDirect, matches } from "../lib/chat";
  import { listTime } from "../lib/format";
  import { toPlain } from "../lib/markup";
  import Avatar from "./Avatar.svelte";
  import ChatRowItem from "./ChatRowItem.svelte";
  import Icon from "./Icon.svelte";
  import Menu from "./Menu.svelte";
  import FilesView from "./FilesView.svelte";
  import GlobalSearch from "./GlobalSearch.svelte";
  import { signOutAction } from "../lib/actions";
  import { lockNow, openLockedChats } from "../lib/lock";

  const rows = $derived(sortedRows().filter((r) => !r.archived));
  const dms = $derived(rows.filter(isDirect));
  const channels = $derived(rows.filter((r) => r.kind === "channel"));
  const threadList = $derived(Object.values(app.threads).sort((a, b) => b.last_reply_ts - a.last_reply_ts));
  const anyUnread = $derived(rows.some((r) => r.unread > 0 || r.marked_unread));

  const shown = $derived.by(() => {
    const base = app.filter === "dm" ? dms : app.filter === "channel" ? channels : rows;
    return base.filter((r) => matches(r, app.query));
  });
  const pinned = $derived(shown.filter((r) => r.favourite).sort(byActivity));
  const directs = $derived(shown.filter((r) => !r.favourite && isDirect(r)).sort(byActivity));
  const chans = $derived(shown.filter((r) => !r.favourite && r.kind === "channel").sort(byActivity));

  let reorder = $state(false);
  let order = $state<string[]>([]);
  const pinnedView = $derived(reorder ? order.map((id) => app.rows[id]).filter(Boolean) : pinned);

  function startReorder() {
    order = pinned.map((r) => r.id);
    reorder = true;
  }
  async function move(id: string, dir: -1 | 1) {
    const i = order.indexOf(id);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= order.length) return;
    [order[i], order[j]] = [order[j], order[i]];
  }
  async function doneReorder() {
    reorder = false;
    try {
      await core.orderConversations(order);
    } catch (e) {
      fail(e);
    }
  }

  let menu = $state<{ x: number; y: number } | null>(null);
  let searchEl = $state<HTMLInputElement>();
  $effect(() => {
    if (app.searching) searchEl?.focus();
  });

  const connLabel = $derived(app.conn === "online" ? "Online" : app.conn === "connecting" ? "Connecting…" : "Offline");

  function pill(f: typeof app.filter) {
    app.filter = f;
  }
  const nDms = $derived(app.counts?.dms ?? dms.length);
  const nChans = $derived(app.counts?.channels ?? channels.length);
  const nThreads = $derived(app.counts?.threads ?? threadList.length);
  const nFiles = $derived(app.counts?.files ?? 0);

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

  {#if app.searching || !app.phone}
    <div class="search" class:desk={!app.phone}>
      <Icon name="search" size={18} />
      <input bind:this={searchEl} placeholder="Search chats" bind:value={app.query} aria-label="Search chats" />
      {#if app.phone}
        <button class="ib sm" aria-label="Close search" onclick={() => { app.searching = false; app.query = ""; }}><Icon name="x" size={18} /></button>
      {:else}
        {#if app.query}<button class="ib sm" aria-label="Clear search" onclick={() => (app.query = "")}><Icon name="x" size={18} /></button>{/if}
        <button class="ib sm" aria-label="More" onclick={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); menu = { x: r.right, y: r.bottom + 4 }; }}><Icon name="dots" size={20} /></button>
      {/if}
    </div>
  {/if}

  {#if app.lock.open}
    <div class="lockbar" role="status"><Icon name="lock" size={15} /><span>Locked chats are showing</span><button onclick={lockNow}>Lock now</button></div>
  {/if}
  <div class="pills" role="tablist" aria-label="Filter chats">
    <button role="tab" aria-selected={app.filter === "all"} class:on={app.filter === "all"} onclick={() => pill("all")}>All{#if anyUnread}<i class="udot"></i>{/if}</button>
    <button role="tab" aria-selected={app.filter === "dm"} class:on={app.filter === "dm"} onclick={() => pill("dm")}>DMs{#if nDms}<em>{nDms}</em>{/if}</button>
    <button role="tab" aria-selected={app.filter === "channel"} class:on={app.filter === "channel"} onclick={() => pill("channel")}>Channels{#if nChans}<em>{nChans}</em>{/if}</button>
    <button role="tab" aria-selected={app.filter === "threads"} class:on={app.filter === "threads"} onclick={() => pill("threads")}>Threads{#if nThreads}<em>{nThreads}</em>{/if}{#if (app.counts?.threads_unread ?? 0) > 0}<i class="udot"></i>{/if}</button>
    <button role="tab" aria-selected={app.filter === "files"} class:on={app.filter === "files"} onclick={() => pill("files")}>Files{#if nFiles}<em>{nFiles}</em>{/if}</button>
  </div>

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
    {:else}
      {#if !shown.length && app.query.trim().length < 2}
        <p class="empty">{app.query ? "No chats match your search." : app.conn === "connecting" ? "Loading your chats…" : "No chats yet. Tap the pencil to start one."}</p>
      {/if}
      {#if pinned.length || reorder}
        <div class="sec"><span><Icon name="pin" size={13} stroke={2} /> PINNED ({pinned.length})</span>
          {#if reorder}<button class="act" onclick={doneReorder}>Done</button>{:else if pinned.length > 1}<button class="act" onclick={startReorder}>Reorder</button>{/if}</div>
        {#each pinnedView as r, i (r.id)}
          <ChatRowItem row={r} selected={app.selected === r.id} onopen={() => openConversation(r.id)} {reorder} first={i === 0} last={i === pinnedView.length - 1} onmove={(d) => move(r.id, d)} />
        {/each}
      {/if}
      {#if directs.length}
        <div class="sec"><span><Icon name="user" size={13} stroke={2} /> DIRECT MESSAGES ({directs.length})</span><span class="end"><Icon name="dots" size={16} /></span></div>
        {#each directs as r (r.id)}<ChatRowItem row={r} selected={app.selected === r.id} onopen={() => openConversation(r.id)} />{/each}
      {/if}
      {#if chans.length}
        <div class="sec"><span><Icon name="megaphone" size={13} stroke={2} /> CHANNELS ({chans.length})</span><span class="end"><Icon name="hash" size={15} /></span></div>
        {#each chans as r (r.id)}<ChatRowItem row={r} selected={app.selected === r.id} onopen={() => openConversation(r.id)} />{/each}
      {/if}
      {#if app.query.trim().length >= 2}<GlobalSearch q={app.query} />{/if}
      <div class="pad"></div>
    {/if}
  </div>

  {#if app.filter !== "files"}<button class="fab" aria-label="New message" onclick={() => (app.sheet = { kind: "newchat" })}><Icon name="pencil" size={24} /></button>{/if}
</div>

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
  .sec .end { color: var(--muted); }
  .act { color: var(--blue); font-weight: 600; font-size: 13px; letter-spacing: 0; }
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
