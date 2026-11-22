<script lang="ts">
  import { app, closeSheet, userAvatar, userName } from "../lib/app.svelte";
  import type { MessageView } from "../lib/core";
  import { clock, dayLabel } from "../lib/format";
  import { toPlain } from "../lib/markup";
  import { initialsOf } from "../lib/people";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import Sheet from "./Sheet.svelte";

  let { msg }: { msg: MessageView } = $props();
  const row = $derived(app.rows[msg.conv_id]);
  const receipts = $derived(app.me?.read_receipts !== false);
  // Everyone else in the chat; "read" comes from their last_read_seq (only known when they have read receipts on).
  const others = $derived(
    Object.entries(row?.readers ?? {})
      .filter(([u]) => u !== msg.sender_id && u !== app.me?.user.id)
      .map(([u, seq]) => ({ u, read: seq >= msg.seq })),
  );
  const mine = $derived(msg.sender_id === app.me?.user.id);
  const readBy = $derived(others.filter((o) => o.read));
  const notYet = $derived(others.filter((o) => !o.read));
</script>

<Sheet title="Message info" onclose={closeSheet}>
  <div class="msg"><b>{userName(msg.sender_id)}</b><p>{toPlain(msg.body) || msg.attachments[0]?.name || "Attachment"}</p><small>{dayLabel(msg.ts)}, {clock(msg.ts)}</small></div>
  {#if !mine}
    <p class="note">Read details are shown for your own messages.</p>
  {:else if !receipts}
    <p class="note"><Icon name="eyeoff" size={16} /> You turned read receipts off, so you do not see who has read your messages.</p>
  {:else}
    <h3><span class="rd"><Icon name="checks" size={16} stroke={2} /></span> Read by</h3>
    {#each readBy as o (o.u)}
      <div class="p"><Avatar id={userAvatar(app.users[o.u])} initials={initialsOf(userName(o.u))} size={36} /><span>{userName(o.u)}</span></div>
    {:else}
      <p class="none">Nobody yet</p>
    {/each}
    {#if notYet.length}
      <h3><Icon name="check" size={16} stroke={2} /> Delivered to</h3>
      {#each notYet as o (o.u)}
        <div class="p"><Avatar id={userAvatar(app.users[o.u])} initials={initialsOf(userName(o.u))} size={36} /><span>{userName(o.u)}</span></div>
      {/each}
    {/if}
  {/if}
</Sheet>

<style>
  .msg { background: var(--chip); border-radius: 12px; padding: 10px 14px; margin-bottom: 6px; }
  .msg p { margin: 2px 0 4px; overflow-wrap: anywhere; }
  .msg small { color: var(--muted); }
  h3 { display: flex; align-items: center; gap: 7px; font-size: 13px; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-2); margin: 16px 0 6px; }
  .rd { color: var(--blue); display: inline-flex; }
  .p { display: flex; align-items: center; gap: 12px; padding: 6px 0; font-size: 15.5px; }
  .none, .note { color: var(--muted); display: flex; gap: 8px; align-items: center; margin: 8px 0; }
</style>
