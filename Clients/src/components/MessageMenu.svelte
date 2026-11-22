<script lang="ts">
  import { registerLayer } from "../lib/overlays";
  import { untrack } from "svelte";
  import { app, fail, openThread, toast } from "../lib/app.svelte";
  import { core, type MessageView } from "../lib/core";
  import { EMOJI_QUICK } from "../lib/emoji";
  import { toPlain } from "../lib/markup";
  import Icon from "./Icon.svelte";

  let { msg: msgProp, x, y, inThread, onedit }: { msg: MessageView; x: number; y: number; inThread: boolean; onedit?: (m: MessageView) => void } = $props();
  // The menu closes itself before it acts, and its props vanish with it: keep the message.
  const msg = untrack(() => msgProp);
  const edit = untrack(() => onedit);

  const mine = $derived(msg.sender_id === app.me?.user.id);
  const row = $derived(app.rows[msg.conv_id]);
  const pinned = $derived(app.pins[msg.conv_id]?.includes(msg.id) ?? false);
  const starred = $derived(app.saved.includes(msg.id));
  const receipts = $derived(row?.kind === "dm" || row?.kind === "group_dm");
  const close = () => (app.sheet = null);

  async function react(emoji: string) {
    close();
    const had = msg.reactions.find((r) => r.emoji === emoji)?.user_ids.includes(app.me!.user.id) ?? false;
    try {
      await core.react(msg.id, emoji, !had);
    } catch (e) {
      fail(e);
    }
  }
  async function attempt(p: Promise<unknown>) {
    try {
      await p;
    } catch (e) {
      fail(e);
    }
  }

  type Item = { label: string; icon: string; danger?: boolean; run: () => void };
  const items = $derived.by((): Item[] => {
    const l: Item[] = [];
    if (!inThread) l.push({ label: "Reply in thread", icon: "thread", run: () => { close(); void openThread(msg.conv_id, msg.thread_root ?? msg.id); } });
    if (msg.body) l.push({ label: "Copy text", icon: "copy", run: () => { close(); void navigator.clipboard?.writeText(toPlain(msg.body) || msg.body).then(() => toast("Copied")).catch(() => {}); } });
    if (mine && msg.body) l.push({ label: "Edit", icon: "pencil", run: () => { close(); edit?.(msg); } });
    l.push({ label: "Forward", icon: "forward", run: () => (app.sheet = { kind: "forward", msg }) });
    if (row?.can_pin) l.push({ label: pinned ? "Unpin" : "Pin to chat", icon: "pin", run: () => { close(); void attempt(core.pinMessage(msg.conv_id, msg.id, !pinned)); } });
    l.push({ label: starred ? "Unsave" : "Save", icon: "bookmark", run: () => { close(); void attempt(core.saveMessage(msg.id, !starred)); } });
    if (receipts) l.push({ label: "Info", icon: "info", run: () => (app.sheet = { kind: "info", msg }) });
    if (msg.kind !== "system") l.push({ label: "Delete for me", icon: "eyeoff", run: () => { close(); if (window.confirm("Delete this message for you? Other people in the chat will still see it.")) void attempt(core.hideMessage(msg.id)); } });
    if (mine) l.push({ label: "Delete for everyone", icon: "trash", danger: true, run: () => { close(); if (window.confirm("Delete this message for everyone?")) void attempt(core.deleteMessage(msg.id)); } });
    return l;
  });

  let el = $state<HTMLDivElement>();
  let pos = $state({ left: 0, top: 0 });
  $effect(() => {
    if (app.phone) return;
    const w = el?.offsetWidth ?? 260;
    const h = el?.offsetHeight ?? 360;
    pos = { left: Math.max(8, Math.min(x, window.innerWidth - w - 8)), top: Math.max(8, Math.min(y, window.innerHeight - h - 8)) };
  });
  $effect(() => registerLayer(close));
</script>


<div class="back" class:phone={app.phone} role="presentation" onclick={close} oncontextmenu={(e) => { e.preventDefault(); close(); }}></div>
<div class="menu" class:phone={app.phone} role="menu" bind:this={el} style={app.phone ? "" : `left:${pos.left}px;top:${pos.top}px`}>
  {#if app.phone}<span class="grab"></span>{/if}
  <div class="quick">
    {#each EMOJI_QUICK as e}<button aria-label={`React ${e}`} onclick={() => react(e)}>{e}</button>{/each}
    <button class="plus" aria-label="More reactions" onclick={() => (app.sheet = { kind: "emoji", msg })}><Icon name="plus" size={20} stroke={2.2} /></button>
  </div>
  {#each items as it}
    <button class="it" class:danger={it.danger} role="menuitem" onclick={it.run}><Icon name={it.icon} size={19} /><span>{it.label}</span></button>
  {/each}
</div>

<style>
  .back { position: fixed; inset: 0; z-index: 60; }
  .back.phone { background: rgba(12, 18, 32, 0.4); }
  .menu { position: fixed; z-index: 61; width: 256px; background: var(--surface); border: 1px solid var(--line); border-radius: 14px; box-shadow: var(--shadow-pop); padding: 6px; display: flex; flex-direction: column; }
  .menu.phone { left: 0; right: 0; bottom: 0; top: auto; width: 100%; border-radius: 20px 20px 0 0; border: 0; padding: 4px 10px calc(10px + var(--safe-bottom)); max-height: 86vh; overflow-y: auto; }
  .grab { align-self: center; width: 38px; height: 4px; border-radius: 2px; background: var(--line); margin: 6px 0 8px; }
  .quick { display: flex; align-items: center; justify-content: space-between; gap: 2px; padding: 4px 4px 8px; border-bottom: 1px solid var(--line); margin-bottom: 4px; }
  .quick button { font-size: 23px; width: 36px; height: 36px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; }
  .quick button:hover { background: var(--hover); }
  .quick .plus { background: var(--chip); color: var(--text-2); font-size: 0; }
  .it { display: flex; align-items: center; gap: 13px; padding: 11px 12px; border-radius: 9px; font-size: 15.5px; text-align: left; }
  .it:hover { background: var(--hover); }
  .danger { color: var(--danger); }
</style>
