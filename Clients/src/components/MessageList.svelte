<script lang="ts">
  import { tick } from "svelte";
  import { app, toggleSelect } from "../lib/app.svelte";
  import type { MessageView } from "../lib/core";
  import { dayKey, dayLabel } from "../lib/format";
  import Icon from "./Icon.svelte";
  import MessageItem from "./MessageItem.svelte";

  let {
    convId,
    items,
    group = false,
    hasMore = false,
    loading = false,
    loadMore,
    onedit,
    inThread = false,
    typingNames = "",
  }: {
    convId: string;
    items: MessageView[];
    group?: boolean;
    hasMore?: boolean;
    loading?: boolean;
    loadMore?: () => Promise<boolean>;
    onedit?: (m: MessageView) => void;
    inThread?: boolean;
    typingNames?: string;
  } = $props();

  let el = $state<HTMLDivElement>();
  let stick = true;
  let jump = $state(false);
  let lastKey = "";

  type Entry = { kind: "day"; key: string; label: string } | { kind: "msg"; m: MessageView; head: boolean; connector: "" | "root" | "mid" | "last" };
  const entries = $derived.by(() => {
    const out: Entry[] = [];
    let prev: MessageView | undefined;
    for (const m of items) {
      // Deleted messages are not shown at all.
      if (m.deleted) continue;
      const newDay = !prev || dayKey(prev.ts) !== dayKey(m.ts);
      if (newDay && !inThread) out.push({ kind: "day", key: dayKey(m.ts), label: dayLabel(m.ts) });
      const idx = items.indexOf(m);
      const connector = !inThread || items.length < 2 ? "" : idx === 0 ? "root" : idx === items.length - 1 ? "last" : "mid";
      const head = newDay || !prev || prev.sender_id !== m.sender_id || m.ts - prev.ts > 5 * 60_000 || prev.kind === "system" || m.kind === "system";
      out.push({ kind: "msg", m, head, connector });
      prev = m;
    }
    return out;
  });

  const nearBottom = () => !el || el.scrollHeight - el.scrollTop - el.clientHeight < 150;
  async function toBottom() {
    await tick();
    if (el) el.scrollTop = el.scrollHeight;
    stick = true;
    jump = false;
  }

  $effect(() => {
    // New conversation or a new last message: stay at the bottom when the person was there (or sent it).
    const last = items[items.length - 1];
    const k = convId + "|" + (last?.id ?? "");
    if (k === lastKey) return;
    const newConv = !lastKey.startsWith(convId + "|");
    lastKey = k;
    if (newConv || stick || last?.sender_id === app.me?.user.id) void toBottom();
    else jump = true;
  });

  // Jumping to a message (search hit, "show in chat"): scroll it to the middle and flash it.
  $effect(() => {
    const f = app.focus;
    if (!f || f.conv !== convId || !el || !items.some((m) => m.id === f.id)) return;
    void tick().then(() => {
      const node = el?.querySelector(`[data-mid="${CSS.escape(f.id)}"]`);
      if (node) {
        stick = false;
        (node as HTMLElement).scrollIntoView({ block: "center" });
        setTimeout(() => app.focus?.id === f.id && (app.focus = null), 2200);
      } else app.focus = null;
    });
  });

  // While selecting, a click anywhere on a message toggles it (and does nothing else).
  function pickClick(ev: MouseEvent, id: string) {
    if (app.select?.conv !== convId) return;
    ev.preventDefault();
    ev.stopPropagation();
    toggleSelect(convId, id);
  }
  // Desktop: Ctrl/Cmd-click starts a selection with that message.
  function modClick(ev: MouseEvent, id: string) {
    if (!(ev.ctrlKey || ev.metaKey)) return;
    ev.preventDefault();
    toggleSelect(convId, id);
  }

  async function onscroll() {
    stick = nearBottom();
    if (stick) jump = false;
    if (el && el.scrollTop < 180 && hasMore && !loading && loadMore) {
      const prevH = el.scrollHeight;
      const prevTop = el.scrollTop;
      await loadMore();
      await tick();
      if (el) el.scrollTop = el.scrollHeight - prevH + prevTop;
    }
  }
</script>

<div class="wrap">
  <div class="list scroll" bind:this={el} {onscroll} onloadcapture={() => stick && void toBottom()} role="log" aria-live="polite" aria-label="Messages">
    {#if loading && hasMore}<div class="older">Loading earlier messages…</div>{:else if !hasMore && items.length > 8 && !inThread}<div class="older">Beginning of the conversation</div>{/if}
    {#each entries as e (e.kind === "day" ? "d" + e.key : e.m.id)}
      {#if e.kind === "day"}
        <div class="day"><span>{e.label}</span></div>
      {:else}
        <div data-mid={e.m.id} class:flash={app.focus?.id === e.m.id} class:picked={app.select?.conv === convId && app.select.ids.includes(e.m.id)} onclickcapture={(ev) => pickClick(ev, e.m.id)} onclick={(ev) => modClick(ev, e.m.id)} role="presentation"><MessageItem m={e.m} {group} head={e.head} {inThread} {onedit} connector={e.connector} /></div>
      {/if}
    {/each}
    {#if typingNames}<div class="typing"><span class="dots"><i></i><i></i><i></i></span> {typingNames}</div>{/if}
    <div class="end"></div>
  </div>
  {#if jump}<button class="jump" onclick={toBottom}><Icon name="down" size={18} /> New messages</button>{/if}
</div>

<style>
  .wrap { position: relative; flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .list { flex: 1; min-height: 0; padding: 8px 0 10px; }
  .older { text-align: center; color: var(--muted); font-size: 12.5px; padding: 10px; }
  .day { display: flex; justify-content: center; margin: 14px 0 4px; }
  .day span { background: var(--surface); border: 1px solid var(--line); color: var(--text-2); font-size: 12.5px; font-weight: 600; padding: 3px 12px; border-radius: 12px; }
  .typing { padding: 8px 18px 2px; color: var(--muted); font-size: 13px; display: flex; align-items: center; gap: 8px; }
  .dots { display: inline-flex; gap: 3px; }
  .dots i { width: 5px; height: 5px; border-radius: 50%; background: var(--muted-2); animation: b 1.2s infinite; }
  .dots i:nth-child(2) { animation-delay: 0.15s; }
  .dots i:nth-child(3) { animation-delay: 0.3s; }
  @keyframes b { 0%, 60%, 100% { opacity: 0.3; transform: translateY(0); } 30% { opacity: 1; transform: translateY(-2px); } }
  .jump { position: absolute; right: 14px; bottom: 12px; background: var(--accent); color: #fff; border-radius: 18px; padding: 7px 14px 7px 10px; font-size: 13.5px; font-weight: 600; display: inline-flex; align-items: center; gap: 4px; box-shadow: var(--shadow-pop); }
  .end { height: 2px; }
  .flash > :global(.msg) { animation: flash 2.2s ease-out; }
  .picked { background: color-mix(in srgb, var(--blue) 14%, transparent); }
  @keyframes flash { 0%, 55% { background: color-mix(in srgb, var(--blue) 18%, transparent); } 100% { background: transparent; } }
</style>
