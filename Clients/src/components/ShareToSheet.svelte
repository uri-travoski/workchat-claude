<script lang="ts">
  import { app, closeSheet, fail, openConversation, sortedRows } from "../lib/app.svelte";
  import { core } from "../lib/core";
  import { channelIcon, matches } from "../lib/chat";
  import { showPreview } from "../lib/attach";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import Sheet from "./Sheet.svelte";

  // Android "Share" from another app: pick the chat that gets the shared files or text.
  let { files, text }: { files: { path: string; name: string }[]; text: string } = $props();
  let q = $state("");
  const rows = $derived(sortedRows().filter((r) => !r.archived && matches(r, q)));
  const what = $derived(files.length ? (files.length === 1 ? files[0].name : `${files.length} files`) : text.trim());

  async function pick(id: string) {
    closeSheet();
    try {
      await openConversation(id);
      if (files.length) {
        const staged = await core.stagePaths(files.map((f) => f.path));
        showPreview(id, null, staged);
        const s = app.sheet;
        if (s?.kind === "preview" && text.trim()) s.caption = text.trim();
      } else if (text.trim()) {
        await core.sendMessage(id, text.trim());
      }
    } catch (e) {
      fail(e);
    }
  }
</script>

<Sheet title="Share to…" onclose={closeSheet} tall>
  <div class="quote"><Icon name={files.length ? "paperclip" : "chat"} size={15} /><span>{what}</span></div>
  <div class="search"><Icon name="search" size={17} /><input placeholder="Search chats" bind:value={q} aria-label="Search chats" /></div>
  <div class="list">
    {#each rows as r (r.id)}
      <button class="p" onclick={() => void pick(r.id)}>
        <Avatar id={r.avatar} initials={r.initials} size={40} tile={r.kind === "channel"} icon={channelIcon(r.title)} />
        <span class="t"><b>{r.title}</b></span>
      </button>
    {/each}
  </div>
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
</style>
