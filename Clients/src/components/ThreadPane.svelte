<script lang="ts">
  import { app, fail, threadMsgs, toast, userName } from "../lib/app.svelte";
  import { core, type MessageView } from "../lib/core";
  import Composer from "./Composer.svelte";
  import Icon from "./Icon.svelte";
  import Menu from "./Menu.svelte";
  import MessageList from "./MessageList.svelte";

  let { conv, root }: { conv: string; root: string } = $props();
  const t = $derived(app.threads[root]);
  const rootMsg = $derived(t?.root);
  const replies = $derived(threadMsgs[root] ?? []);
  const items = $derived<MessageView[]>(rootMsg ? [rootMsg as MessageView, ...replies] : replies);
  let editing = $state<MessageView | null>(null);
  const row = $derived(app.rows[conv]);
  const following = $derived(t?.following ?? false);
  let menu = $state<{ x: number; y: number } | null>(null);

  // Opening a thread marks it read.
  $effect(() => {
    const r = root;
    if (t && t.unread > 0) void core.readThread(r).catch(() => {});
  });

  async function follow(on: boolean) {
    try {
      await core.followThread(root, on);
      if (t) t.following = on;
      toast(on ? "Following this thread" : "Unfollowed this thread");
    } catch (e) {
      fail(e);
    }
  }
</script>

<aside class="pane" aria-label="Thread">
  <header>
    {#if app.phone}<button class="ib" aria-label="Back" onclick={() => (app.thread = null)}><Icon name="left" size={26} stroke={2.2} /></button>{/if}
    <div class="ti"><h2>Thread</h2><span>{row ? (row.kind === "channel" ? "# " : "") + row.title : ""}</span></div>
    <button class="ib" aria-label="Thread options" onclick={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); menu = { x: r.right, y: r.bottom + 4 }; }}><Icon name="dots" size={22} /></button>
    {#if !app.phone}<button class="ib" aria-label="Close thread" onclick={() => (app.thread = null)}><Icon name="x" size={22} /></button>{/if}
  </header>
  <MessageList convId={"thread:" + root} {items} group={row?.kind === "channel" || row?.kind === "group_dm"} inThread onedit={(m) => (editing = m)} />
  <Composer convId={conv} threadRoot={root} {editing} oncancelEdit={() => (editing = null)} alsoChannel={row?.kind === "channel" || row?.kind === "group_dm"} placeholder={rootMsg ? `Reply to ${userName(rootMsg.sender_id)}…` : "Reply…"} />
</aside>

{#if menu}
  <Menu x={menu.x} y={menu.y} right onclose={() => (menu = null)} items={[
    { label: following ? "Unfollow thread" : "Follow thread", icon: "bell", run: () => void follow(!following) },
    { label: "Mark as read", icon: "check", disabled: !t?.unread, run: () => void core.readThread(root).catch(fail) },
    { label: "Show in chat", icon: "chat", run: () => { if (app.phone) app.thread = null; } },
  ]} />
{/if}

<style>
  .pane { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--page); min-width: 0; }
  header { display: flex; align-items: center; gap: 4px; padding: calc(var(--safe-top) + 8px) 8px 8px 14px; background: var(--surface); border-bottom: 1px solid var(--line); flex: none; }
  .ti { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  h2 { margin: 0; font-size: 17px; }
  .ti span { font-size: 13px; color: var(--muted); }
  .ib { width: 40px; height: 40px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; }
  .ib:hover { background: var(--hover); }
</style>
