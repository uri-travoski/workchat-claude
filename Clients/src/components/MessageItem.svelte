<script lang="ts">
  import { app, convOf, fail, openThread, userAvatar, userName } from "../lib/app.svelte";
  import { core, type MessageView } from "../lib/core";
  import { clock } from "../lib/format";
  import { toPlain } from "../lib/markup";
  import { initialsOf } from "../lib/people";
  import Attachments from "./Attachments.svelte";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import Rich from "./Rich.svelte";

  let {
    m,
    group = false,
    head = true,
    inThread = false,
    onedit,
    connector = "",
  }: { m: MessageView; group?: boolean; head?: boolean; inThread?: boolean; onedit?: (m: MessageView) => void; connector?: "" | "root" | "mid" | "last" } = $props();

  const own = $derived(m.sender_id === app.me?.user.id);
  // In the thread pane everything lines up on the left with an avatar, like the reference; elsewhere own messages sit right.
  const mine = $derived(own && !inThread);
  const sender = $derived(app.users[m.sender_id]);
  const system = $derived(m.kind === "system");
  const t = $derived(app.threads[m.id]);
  const row = $derived(app.rows[m.conv_id]);
  const pinned = $derived(app.pins[m.conv_id]?.includes(m.id) ?? false);
  const starred = $derived(app.saved.includes(m.id));
  const up = $derived(m.pending ? (app.uploads[m.id] ?? m.upload) : undefined);
  const upPct = $derived(up && up.total ? Math.min(100, Math.round((up.sent / up.total) * 100)) : 0);
  let pressTimer: ReturnType<typeof setTimeout> | undefined;
  let pressed = false;

  // The message this one quotes (a "quote reply"), when it is loaded.
  const quoted = $derived.by(() => {
    if (!m.reply_to) return null;
    const list = m.thread_root && !m.also_send_to_channel ? [] : convOf(m.conv_id).items;
    return list.find((x) => x.id === m.reply_to) ?? null;
  });

  // A reply that was also sent to the channel points back at its thread (the root's text when it is known).
  const rootText = $derived.by(() => {
    if (!m.thread_root) return "";
    const r = app.threads[m.thread_root]?.root ?? convOf(m.conv_id).items.find((x) => x.id === m.thread_root);
    return r ? toPlain(r.body).slice(0, 60) : "";
  });

  function openMenu(x: number, y: number) {
    if (m.pending || system) return;
    app.sheet = { kind: "msgmenu", msg: m, x, y, inThread, onedit };
  }
  function ctx(e: MouseEvent) {
    e.preventDefault();
    openMenu(e.clientX, e.clientY);
  }
  function down(e: PointerEvent) {
    if (e.pointerType !== "touch") return;
    const x = e.clientX;
    const y = e.clientY;
    pressed = false;
    clearTimeout(pressTimer);
    pressTimer = setTimeout(() => {
      pressed = true;
      openMenu(x, y);
    }, 420);
  }
  const cancel = () => clearTimeout(pressTimer);

  async function react(emoji: string) {
    const mine_ = m.reactions.find((r) => r.emoji === emoji)?.user_ids.includes(app.me!.user.id) ?? false;
    try {
      await core.react(m.id, emoji, !mine_);
    } catch (e) {
      fail(e);
    }
  }

  /** Ticks: one grey = sent, two = delivered, two light blue = read (DMs and group chats, when receipts are on). */
  const tick = $derived.by(() => {
    if (!own || m.deleted || m.pending) return null;
    const dm = row?.kind === "dm" || row?.kind === "group_dm";
    const receipts = app.me?.read_receipts !== false;
    if (dm && receipts && row) {
      const seqs = Object.values(row.readers);
      const others = Math.max(1, row.member_count - 1);
      const readBy = seqs.filter((s) => s >= m.seq).length;
      if (row.kind === "dm" ? readBy >= 1 : readBy >= others) return "read";
    }
    return m.delivered ? "delivered" : "sent";
  });
</script>

{#if system}
  <div class="sys"><span>{m.body}</span></div>
{:else}
  <div class="msg" class:mine class:grouped={!head} class:group class:inthread={inThread} class:c-root={connector === "root"} class:c-mid={connector === "mid"} class:c-last={connector === "last"}>
    {#if (group || inThread) && !mine}
      <span class="avwrap">{#if head || inThread}<Avatar id={userAvatar(sender)} initials={initialsOf(userName(m.sender_id))} size={inThread ? 32 : 34} bot={sender?.is_bot} />{/if}</span>
    {/if}
    <div class="col">
      <div class="bubble" class:in={!own} class:out={own} class:pending={m.pending === "sending"} class:media-only={!m.body && m.attachments.length > 0} role="presentation" oncontextmenu={ctx} onpointerdown={down} onpointerup={cancel} onpointermove={cancel} onpointercancel={cancel}>
        {#if !app.phone && !m.pending && !m.deleted}
          <span class="hover">
            <button aria-label="React" title="React" onclick={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); openMenu(r.left, r.bottom + 4); }}><Icon name="smile" size={17} /></button>
            {#if !inThread}<button aria-label="Reply in thread" title="Reply in thread" onclick={() => void openThread(m.conv_id, m.thread_root ?? m.id)}><Icon name="thread" size={17} /></button>{/if}
            <button aria-label="Message actions" title="More" onclick={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); openMenu(r.right, r.bottom + 4); }}><Icon name="dots" size={17} /></button>
          </span>
        {/if}
        {#if (group || inThread) && !mine && (head || inThread)}<div class="who">{userName(m.sender_id)}</div>{/if}
        {#if m.forwarded && !m.deleted}<div class="fwd"><Icon name="forward" size={13} /> Forwarded from {userName(m.forwarded.sender_id)}</div>{/if}
        {#if m.thread_root && m.also_send_to_channel && !inThread && !m.deleted}
          <button class="tref" onclick={() => void openThread(m.conv_id, m.thread_root!)}><Icon name="thread" size={13} /> Replied to a thread{#if rootText}: <span>{rootText}</span>{/if}</button>
        {/if}
        {#if quoted && !m.deleted}
          <div class="quote"><b>{userName(quoted.sender_id)}</b><span>{toPlain(quoted.body) || (quoted.attachments[0]?.name ?? "Attachment")}</span></div>
        {/if}
        {#if m.deleted}
          <p class="gone"><Icon name="trash" size={14} /> Message deleted</p>
        {:else}
          <Attachments {m} mine={own} />
          {#if m.body}<Rich body={m.body} />{/if}
        {/if}
        {#if up && m.attachments.length}
          <div class="upl" class:fail={m.pending === "failed"}>
            {#if m.pending === "failed"}
              <Icon name="alert" size={14} /> <span>{m.error || "Upload failed"}</span>
            {:else}
              <div class="bar"><i style="width:{up.state === "sending" ? 100 : upPct}%"></i></div>
              <span class="pc">{up.state === "waiting" ? "Waiting…" : up.state === "sending" ? "Sending…" : `${upPct}%`}</span>
              <button class="x" aria-label="Cancel upload" title="Cancel" onclick={() => void core.discardMessage(m.id)}><Icon name="x" size={15} stroke={2.2} /></button>
            {/if}
          </div>
        {/if}
        <div class="foot">
          {#if pinned}<span class="fi" title="Pinned"><Icon name="pin" size={12} fill /></span>{/if}
          {#if starred}<span class="fi" title="Saved"><Icon name="star" size={12} fill /></span>{/if}
          {#if m.edited_ms && !m.deleted}<span class="edited">edited</span>{/if}
          <span class="time">{clock(m.ts)}</span>
          {#if own && !m.deleted}
            {#if m.pending === "sending"}<Icon name="retry" size={13} />{:else if m.pending === "failed"}<Icon name="alert" size={14} />{:else if tick}<span class="tick {tick}" title={tick === "read" ? "Read" : tick === "delivered" ? "Delivered" : "Sent"}><Icon name={tick === "sent" ? "check" : "checks"} size={15} stroke={2} /></span>{/if}
          {/if}
        </div>
      </div>

      {#if m.pending === "failed"}
        <div class="fail"><Icon name="alert" size={14} /> Not sent. <button onclick={() => void core.retryMessage(m.id)}>Retry</button> · <button onclick={() => void core.discardMessage(m.id)}>Delete</button></div>
      {/if}

      {#if m.reactions.length}
        <div class="reacts">
          {#each m.reactions as r (r.emoji)}
            <button class="re" class:me={r.user_ids.includes(app.me?.user.id ?? "")} onclick={() => react(r.emoji)} aria-label={`${r.emoji} ${r.user_ids.length}`}>{r.emoji}<span>{r.user_ids.length}</span></button>
          {/each}
        </div>
      {/if}

      {#if t && !inThread}
        <button class="replies" onclick={() => void openThread(m.conv_id, m.id)}><Icon name="thread" size={16} /> {t.reply_count} {t.reply_count === 1 ? "reply" : "replies"}{#if t.unread}<i class="nb">{t.unread}</i>{/if}</button>
      {/if}
    </div>
  </div>
{/if}

<style>
  .sys { text-align: center; margin: 8px 0; }
  .sys span { font-size: 12.5px; color: var(--muted); background: var(--chip); padding: 3px 12px; border-radius: 10px; }
  .msg { position: relative; display: flex; align-items: flex-end; gap: 8px; margin-top: 10px; padding: 0 14px; }
  .msg.grouped { margin-top: 3px; }
  .msg.mine { justify-content: flex-end; }
  .avwrap { width: 34px; flex: none; display: inline-flex; }
  .col { display: flex; flex-direction: column; max-width: min(78%, 640px); min-width: 0; align-items: flex-start; }
  .mine .col { align-items: flex-end; }
  .inthread .col { max-width: 100%; flex: 1; }
  .bubble { position: relative; padding: 9px 13px 6px; border-radius: var(--radius-bubble); font-size: var(--msg-size); min-width: 78px; max-width: 100%; box-shadow: var(--shadow); }
  .bubble.media-only { padding: 5px 5px 6px; }
  .bubble.media-only .foot { padding: 0 6px; }
  .bubble.in { background: var(--bubble-in); color: var(--bubble-in-text); border-top-left-radius: 4px; }
  .bubble.out { background: linear-gradient(135deg, var(--accent), var(--accent-2)); color: var(--bubble-out-text); border-top-right-radius: 4px; }
  .grouped .bubble.in { border-top-left-radius: var(--radius-bubble); }
  .grouped .bubble.out { border-top-right-radius: var(--radius-bubble); }
  .bubble.pending { opacity: 0.8; }
  .who { font-size: 13.5px; font-weight: 700; color: var(--blue); margin-bottom: 2px; }
  .out .who { color: #cfe0ff; }
  .fwd { display: flex; align-items: center; gap: 5px; font-size: 12.5px; font-style: italic; opacity: 0.8; margin-bottom: 3px; }
  .tref { display: inline-flex; align-items: center; gap: 5px; max-width: 100%; font-size: 12.5px; font-weight: 600; color: var(--blue); margin: 0 0 4px; padding: 2px 0; }
  .tref span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 400; opacity: 0.85; }
  .out .tref { color: #cfe0ff; }
  .tref:hover { text-decoration: underline; }
  .quote { border-left: 3px solid var(--blue); background: rgba(127, 127, 127, 0.14); border-radius: 6px; padding: 3px 8px; margin: 0 0 6px; display: flex; flex-direction: column; font-size: 13px; max-width: 100%; }
  .quote b { font-size: 12.5px; color: var(--blue); }
  .out .quote { border-left-color: #a9c4f5; }
  .out .quote b { color: #cfe0ff; }
  .quote span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; opacity: 0.85; }
  .gone { margin: 0; font-style: italic; opacity: 0.75; display: flex; align-items: center; gap: 6px; }
  .foot { display: flex; align-items: center; justify-content: flex-end; gap: 5px; margin-top: 3px; font-size: 12px; color: var(--muted); }
  .out .foot { color: var(--bubble-out-time); }
  .fi { display: inline-flex; opacity: 0.85; }
  .edited { font-style: italic; }
  .tick { display: inline-flex; }
  .tick.delivered { opacity: 0.95; }
  .tick.read { color: #6fa8ff; }
  .in .tick.read { color: var(--blue); }
  .upl { display: flex; align-items: center; gap: 8px; margin: 2px 0 4px; font-size: 12.5px; }
  .upl.fail { color: #ffb4ae; }
  .in .upl.fail { color: var(--danger); }
  .bar { flex: 1; height: 5px; border-radius: 3px; background: rgba(127, 127, 127, 0.3); overflow: hidden; min-width: 110px; }
  .bar i { display: block; height: 100%; background: var(--blue); border-radius: 3px; transition: width 0.15s; }
  .out .bar i { background: #fff; }
  .pc { font-variant-numeric: tabular-nums; min-width: 38px; }
  .x { width: 24px; height: 24px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; background: rgba(127, 127, 127, 0.22); }
  .reacts { display: flex; flex-wrap: wrap; gap: 5px; margin-top: 5px; }
  .re { display: inline-flex; align-items: center; gap: 4px; background: var(--chip); border: 1px solid transparent; border-radius: 13px; padding: 2px 9px; font-size: 14px; }
  .re span { font-size: 12.5px; font-weight: 600; color: var(--text-2); }
  .re.me { background: color-mix(in srgb, var(--blue) 16%, var(--surface)); border-color: color-mix(in srgb, var(--blue) 45%, transparent); }
  .replies { display: inline-flex; align-items: center; gap: 6px; margin-top: 6px; color: var(--blue); font-weight: 600; font-size: 14.5px; padding: 2px 4px; border-radius: 6px; }
  .replies:hover { background: var(--hover); }
  .nb { font-style: normal; background: var(--badge); color: #fff; font-size: 11.5px; border-radius: 9px; padding: 0 6px; }
  .fail { font-size: 12.5px; color: var(--danger); margin-top: 3px; display: inline-flex; gap: 5px; align-items: center; }
  .fail button { color: var(--blue); font-weight: 600; text-decoration: underline; }
  /* Desktop hover actions live inside the bubble's top corner, so nothing floats in the list. */
  .hover { position: absolute; top: -16px; right: 8px; display: none; gap: 1px; background: var(--surface); color: var(--text-2); border: 1px solid var(--line); border-radius: 10px; padding: 2px; box-shadow: var(--shadow-pop); z-index: 3; }
  .mine .hover { right: auto; left: 8px; }
  .bubble:hover .hover, .hover:focus-within { display: inline-flex; }
  .hover button { width: 28px; height: 26px; border-radius: 7px; display: inline-flex; align-items: center; justify-content: center; }
  .hover button:hover { background: var(--hover); color: var(--text); }

  /* Thread pane: the vertical connector line from the root message down through the replies (desktop/2.webp). */
  .inthread { padding: 0 14px 0 14px; }
  .inthread.c-root::before, .inthread.c-mid::before, .inthread.c-last::before { content: ""; position: absolute; left: 29px; width: 2px; background: color-mix(in srgb, var(--blue) 32%, var(--line)); }
  .inthread.c-root::before { top: 44px; bottom: -10px; }
  .inthread.c-mid::before { top: -10px; bottom: -10px; }
  .inthread.c-last::before { top: -10px; height: 28px; }
</style>
