<script lang="ts">
  import { app, closeSheet, fail, sortedRows, toast } from "../lib/app.svelte";
  import { core, type MessageView } from "../lib/core";
  import { channelIcon, matches } from "../lib/chat";
  import { toPlain } from "../lib/markup";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import Sheet from "./Sheet.svelte";

  let { msg }: { msg: MessageView } = $props();
  let q = $state("");
  let picked = $state<string[]>([]);
  let comment = $state("");
  let busy = $state(false);
  const rows = $derived(sortedRows().filter((r) => !r.archived && matches(r, q)));

  async function send() {
    if (!picked.length || busy) return;
    busy = true;
    try {
      await core.forwardMessage(msg.id, picked, comment.trim() || undefined);
      closeSheet();
      toast(picked.length === 1 ? "Forwarded" : `Forwarded to ${picked.length} chats`);
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }
  const what = $derived(toPlain(msg.body) || msg.attachments[0]?.name || "Message");
</script>

<Sheet title="Forward to…" onclose={closeSheet} tall>
  <div class="quote"><Icon name="forward" size={15} /><span>{what}</span></div>
  <div class="search"><Icon name="search" size={17} /><input placeholder="Search chats" bind:value={q} aria-label="Search chats" /></div>
  <div class="list">
    {#each rows as r (r.id)}
      <button class="p" onclick={() => (picked = picked.includes(r.id) ? picked.filter((x) => x !== r.id) : [...picked, r.id])}>
        <Avatar id={r.avatar} initials={r.initials} size={40} tile={r.kind === "channel"} icon={channelIcon(r.title)} />
        <span class="t"><b>{r.title}</b></span>
        <span class="ck" class:on={picked.includes(r.id)}>{#if picked.includes(r.id)}<Icon name="check" size={15} stroke={3} />{/if}</span>
      </button>
    {/each}
  </div>
  {#snippet footer()}
    <input class="cm" placeholder="Add a comment (optional)" bind:value={comment} aria-label="Comment" />
    <button class="send" disabled={!picked.length || busy} onclick={send}><Icon name="send" size={19} /> Send{picked.length ? ` (${picked.length})` : ""}</button>
  {/snippet}
</Sheet>

<style>
  .quote { display: flex; gap: 8px; align-items: center; background: var(--chip); border-radius: 10px; padding: 8px 12px; font-size: 14px; color: var(--text-2); margin-bottom: 8px; }
  .quote span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .search { display: flex; align-items: center; gap: 8px; color: var(--muted); background: var(--chip); border-radius: 18px; padding: 0 12px; margin-bottom: 8px; }
  .search input { flex: 1; border: 0; background: transparent; padding: 10px 0; outline: none; font-size: 15px; }
  .p { width: 100%; display: flex; align-items: center; gap: 12px; padding: 8px 4px; text-align: left; border-radius: 10px; }
  .p:hover { background: var(--hover); }
  .t { flex: 1; min-width: 0; }
  .t b { font-size: 15.5px; }
  .ck { width: 22px; height: 22px; border-radius: 50%; border: 2px solid var(--line); display: inline-flex; align-items: center; justify-content: center; color: #fff; }
  .ck.on { background: var(--accent); border-color: var(--accent); }
  .cm { flex: 1; min-width: 0; border: 1px solid var(--line); background: var(--surface-2); border-radius: 18px; padding: 9px 14px; outline: none; font-size: 14.5px; }
  .send { display: inline-flex; align-items: center; gap: 7px; background: linear-gradient(145deg, var(--accent), var(--accent-2)); color: #fff; font-weight: 700; border-radius: 20px; padding: 10px 18px; flex: none; }
  .send:disabled { opacity: 0.5; }
</style>
