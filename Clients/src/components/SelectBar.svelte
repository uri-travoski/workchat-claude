<script lang="ts">
  import { app, convOf, fail, toast } from "../lib/app.svelte";
  import { core, type MessageView } from "../lib/core";
  import { toPlain } from "../lib/markup";
  import Icon from "./Icon.svelte";

  let { convId }: { convId: string } = $props();

  // The picked messages, in the order they appear in the chat.
  const picked = $derived.by((): MessageView[] => {
    const ids = app.select?.conv === convId ? app.select.ids : [];
    return convOf(convId).items.filter((m) => ids.includes(m.id));
  });
  const mine = $derived(picked.filter((m) => m.sender_id === app.me?.user.id));
  const count = $derived(app.select?.conv === convId ? app.select.ids.length : 0);

  function done() {
    app.select = null;
  }

  async function copyText() {
    const text = picked
      .filter((m) => m.body)
      .map((m) => toPlain(m.body) || m.body)
      .join("\n");
    if (!text) return toast("Nothing to copy");
    try {
      await navigator.clipboard?.writeText(text);
      toast("Copied");
      done();
    } catch (e) {
      fail(e);
    }
  }

  async function deleteFor(list: MessageView[], everyone: boolean) {
    const msg = everyone
      ? `Delete ${list.length === 1 ? "this message" : `these ${list.length} messages`} for everyone?`
      : `Delete ${list.length === 1 ? "this message" : `these ${list.length} messages`} for you? Other people in the chat will still see ${list.length === 1 ? "it" : "them"}.`;
    if (!window.confirm(msg)) return;
    try {
      await Promise.all(list.map((m) => (everyone ? core.deleteMessage(m.id) : core.hideMessage(m.id))));
      done();
    } catch (e) {
      fail(e);
    }
  }
</script>

<div class="bar" role="toolbar" aria-label="Selected messages">
  <button class="ib" aria-label="Cancel selection" onclick={done}><Icon name="x" size={20} /></button>
  <span class="n">{count} selected</span>
  <button class="ib" aria-label="Copy text" disabled={!picked.some((m) => m.body)} onclick={copyText}><Icon name="copy" size={19} /></button>
  <button class="ib" aria-label="Delete for me" disabled={!count} onclick={() => deleteFor(picked, false)}><Icon name="eyeoff" size={19} /></button>
  {#if mine.length}
    <button class="ib danger" aria-label="Delete for everyone" onclick={() => deleteFor(mine, true)}><Icon name="trash" size={19} /></button>
  {/if}
</div>

<style>
  .bar { display: flex; align-items: center; gap: 4px; padding: 8px 10px; background: var(--surface); border-top: 1px solid var(--line); min-height: 56px; box-sizing: border-box; }
  .n { flex: 1; font-size: 15px; font-weight: 600; color: var(--text); padding-left: 4px; }
  .ib { width: 40px; height: 40px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; color: var(--text-2); }
  .ib:hover:not(:disabled) { background: var(--hover); }
  .ib:disabled { opacity: 0.35; }
  .ib.danger { color: #d93a3a; }
</style>
