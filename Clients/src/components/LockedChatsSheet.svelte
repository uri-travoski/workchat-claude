<script lang="ts">
  import { app, closeSheet, openConversation } from "../lib/app.svelte";
  import { channelIcon } from "../lib/chat";
  import { lockNow, unlockChat } from "../lib/lock";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import Sheet from "./Sheet.svelte";

  const rows = $derived(Object.values(app.rows).filter((r) => r.locked));
  async function open(id: string) {
    closeSheet();
    app.tab = "chats";
    await openConversation(id);
  }
</script>

<Sheet title="Locked chats" onclose={closeSheet}>
  <p class="note">These chats are hidden from your lists, search, counts and notifications until you unlock this area with your PIN. It locks again when you leave the app or after a while without use.</p>
  {#each rows as r (r.id)}
    <div class="r">
      <button class="open" onclick={() => open(r.id)}>
        <Avatar id={r.avatar} initials={r.initials} size={42} tile={r.kind === "channel"} icon={channelIcon(r.title)} />
        <span class="t"><b>{r.kind === "channel" ? "# " : ""}{r.title}</b><small>{r.preview.kind === "none" ? "No messages yet" : r.preview.text}</small></span>
      </button>
      <button class="un" onclick={() => unlockChat(r.id)}><Icon name="lockopen" size={17} /> Unlock</button>
    </div>
  {:else}
    <p class="empty">No chats are locked. Open a chat's menu and choose Lock.</p>
  {/each}
  {#snippet footer()}<button class="relock" onclick={lockNow}><Icon name="lock" size={17} /> Lock now</button>{/snippet}
</Sheet>

<style>
  .note { color: var(--muted); font-size: 13.5px; margin: 0 0 10px; }
  .empty { color: var(--muted); text-align: center; margin: 24px 0; }
  .r { display: flex; align-items: center; gap: 8px; border-bottom: 1px solid var(--line); padding: 6px 0; }
  .open { flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; text-align: left; padding: 4px; border-radius: 10px; }
  .open:hover { background: var(--hover); }
  .t { display: flex; flex-direction: column; min-width: 0; }
  .t b { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t small { color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .un { display: inline-flex; align-items: center; gap: 6px; color: var(--blue); font-weight: 600; font-size: 14px; padding: 8px 10px; border-radius: 10px; }
  .un:hover { background: var(--hover); }
  .relock { display: inline-flex; align-items: center; gap: 8px; background: var(--accent); color: #fff; font-weight: 600; padding: 11px 18px; border-radius: 12px; width: 100%; justify-content: center; }
</style>
