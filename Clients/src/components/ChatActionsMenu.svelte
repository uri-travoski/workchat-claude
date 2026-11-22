<script lang="ts">
  import { app, closeSheet, fail } from "../lib/app.svelte";
  import { core } from "../lib/core";
  import { lockChat, unlockChat } from "../lib/lock";
  import Menu from "./Menu.svelte";

  let { conv, x, y }: { conv: string; x: number; y: number } = $props();
  // The menu closes itself before an entry runs, and that unmounts this component (the sheet is gone), so the entries
  // must not read the props afterwards: take the chat id now.
  // svelte-ignore state_referenced_locally
  const id = conv;
  const row = $derived(app.rows[id]);
</script>

{#if row}
  <Menu {x} {y} onclose={closeSheet} items={[
    { label: row.favourite ? "Unpin" : "Pin to top", icon: "pin", run: () => void core.setFavourite(id, !row.favourite).catch(fail) },
    { label: "Mark as read", icon: "check", disabled: !row.unread && !row.marked_unread, run: () => void core.markRead(id).catch(fail) },
    row.locked ? { label: "Unlock", icon: "lockopen", run: () => unlockChat(id) } : { label: "Lock", icon: "lock", run: () => lockChat(id) },
  ]} />
{/if}
