<script lang="ts">
  import type { ChatRow } from "../lib/core";
  import { duration, listTime } from "../lib/format";
  import { channelIcon } from "../lib/chat";
  import { app } from "../lib/app.svelte";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";

  let {
    row,
    selected = false,
    nolong = false,
    onopen,
  }: { row: ChatRow; selected?: boolean; nolong?: boolean; onopen: () => void } = $props();

  const unread = $derived(row.unread > 0 || row.marked_unread);
  const isChannel = $derived(row.kind === "channel");
  const chip = $derived(row.peer_role === "admin" ? "Admin" : "");
  const verified = $derived(row.peer_role === "owner");
  const p = $derived(row.preview);

  // Right click or long press opens the chat's quick menu (Pin, Mark as read, Lock / Unlock).
  let press: ReturnType<typeof setTimeout> | undefined;
  let longFired = false;
  function menuAt(x: number, y: number) {
    app.sheet = { kind: "chatactions", conv: row.id, x, y };
  }
  function down(e: PointerEvent) {
    if (e.pointerType === "mouse" || nolong) return;
    longFired = false;
    const [x, y] = [e.clientX, e.clientY];
    press = setTimeout(() => {
      longFired = true;
      navigator.vibrate?.(15);
      menuAt(x, y);
    }, 520);
  }
  const cancel = () => clearTimeout(press);
</script>

<div class="row" class:selected class:unread role="presentation" oncontextmenu={(e) => { e.preventDefault(); menuAt(e.clientX, e.clientY); }}>
  <button class="main" onpointerdown={down} onpointerup={cancel} onpointerleave={cancel} onpointercancel={cancel} onclick={() => { if (longFired) { longFired = false; return; } onopen(); }} aria-label={`${row.title}${unread ? ", unread" : ""}`}>
    <Avatar id={row.avatar} initials={row.initials} size={50} tile={isChannel} bot={row.is_bot} icon={channelIcon(row.title)} presence={row.presence} />
    <span class="body">
      <span class="top">
        <span class="title">{row.title}</span>
        {#if row.locked}<span class="lk" title="Locked chat, currently unlocked" aria-label="Locked chat, currently unlocked"><Icon name="lockopen" size={15} stroke={2} /></span>{/if}
        {#if verified}<svg class="verified" width="16" height="16" viewBox="0 0 24 24" aria-label="Owner"><circle cx="12" cy="12" r="11" fill="var(--blue)" /><path d="m7.5 12.3 3 3 6-6.3" fill="none" stroke="#fff" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" /></svg>{/if}
        {#if chip}<span class="role">{chip}</span>{/if}
        {#if p.kind !== "none"}<span class="time" class:blue={unread}>{listTime(row.last_ts)}</span>{/if}
      </span>
      <span class="bottom">
        <span class="preview" class:strong={unread} class:accent={p.kind === "voice"}>
          {#if p.kind === "voice"}<Icon name="mic" size={15} />{:else if p.kind === "file"}<Icon name="paperclip" size={15} />{:else if p.kind === "image"}<Icon name="image" size={15} />{:else if p.kind === "video"}<Icon name="video" size={15} />{/if}
          {#if p.sender && p.kind !== "none" && p.kind !== "system"}<b class="who">{p.sender}:</b>{/if}
          <span class="txt">{#if p.kind === "none"}<i>No messages yet</i>{:else if p.kind === "voice"}Voice message{p.duration_ms ? ` (${duration(p.duration_ms)})` : ""}{:else}{p.text}{/if}</span>
        </span>
        <span class="meta">
          {#if row.has_pending}<span class="pend" title="Sending"><Icon name="retry" size={14} /></span>{/if}
          {#if row.last_own && !row.has_pending}<span class="ticks" class:blue={row.last_delivered}><Icon name={row.last_delivered ? "checks" : "check"} size={16} stroke={2} /></span>{/if}
          {#if row.favourite}<span class="pin"><Icon name="pin" size={15} stroke={1.6} fill /></span>{/if}
          {#if p.chip}<span class="chip">{p.chip}</span>{/if}
          {#if row.unread > 0}<span class="badge" class:muted={row.muted}>{row.unread > 99 ? "99+" : row.unread}</span>{:else if row.marked_unread}<span class="badge dotb"></span>{/if}
        </span>
      </span>
    </span>
  </button>
</div>

<style>
  .row { display: flex; align-items: center; background: var(--surface); }
  .row.selected { background: #ebf1f8; box-shadow: inset 3px 0 0 var(--navy); }
  .main { flex: 1; min-width: 0; display: flex; gap: 14px; align-items: center; padding: 10px 16px; text-align: left; }
  .main:hover { background: var(--hover); }
  .row.selected .main:hover { background: transparent; }
  .body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .top, .bottom { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .title { font-size: 17px; font-weight: 700; letter-spacing: -0.01em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .verified { flex: none; }
  .role { flex: none; font-size: 12px; font-weight: 500; color: var(--chip-text); background: var(--chip); padding: 2px 8px; border-radius: 6px; }
  .time { margin-left: auto; flex: none; font-size: 12px; font-weight: 500; color: #64748b; }
  .time.blue { color: var(--blue); }
  .preview { flex: 1; min-width: 0; display: flex; align-items: center; gap: 5px; font-size: 14.5px; color: var(--muted); }
  .preview.strong { color: var(--text); font-weight: 600; }
  .preview.accent { color: var(--blue); font-weight: 500; }
  .preview .txt { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .preview :global(svg) { flex: none; }
  .who { font-weight: 700; color: var(--text); flex: none; }
  .meta { flex: none; display: flex; align-items: center; gap: 8px; color: var(--muted); }
  .ticks.blue { color: var(--blue); }
  .lk { display: inline-flex; color: var(--accent); }
  .pend { color: var(--muted-2); display: inline-flex; }
  .pin { color: var(--muted); display: inline-flex; transform: rotate(35deg); }
  .chip { font-size: 12.5px; font-weight: 500; background: var(--chip); color: var(--chip-text); padding: 2px 9px; border-radius: 9px; }
  .badge { min-width: 22px; height: 22px; padding: 0 6px; border-radius: 11px; background: var(--badge); color: #fff; font-size: 12.5px; font-weight: 700; display: inline-flex; align-items: center; justify-content: center; }
  .badge.muted { background: var(--muted-2); }
  .badge.dotb { min-width: 12px; width: 12px; height: 12px; padding: 0; }
</style>
