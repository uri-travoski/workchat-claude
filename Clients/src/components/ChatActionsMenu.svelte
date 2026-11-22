<script lang="ts">
  import { app, closeSheet, fail } from "../lib/app.svelte";
  import { core } from "../lib/core";
  import { lockChat, unlockChat } from "../lib/lock";
  import Menu from "./Menu.svelte";

  let { conv, x, y }: { conv: string; x: number; y: number } = $props();
  const row = $derived(app.rows[conv]);
</script>

{#if row}
  <Menu {x} {y} onclose={closeSheet} items={[
    { label: row.favourite ? "Unpin" : "Pin to top", icon: "pin", run: () => void core.setFavourite(conv, !row.favourite).catch(fail) },
    { label: "Mark as read", icon: "check", disabled: !row.unread && !row.marked_unread, run: () => void core.markRead(conv).catch(fail) },
    row.locked ? { label: "Unlock", icon: "lockopen", run: () => unlockChat(conv) } : { label: "Lock", icon: "lock", run: () => lockChat(conv) },
  ]} />
{/if}
