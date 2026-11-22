<script lang="ts">
  import { onMount } from "svelte";
  import { app, fail, jumpTo, openConversation } from "../../lib/app.svelte";
  import { core, type MessageView } from "../../lib/core";
  import { dayLabel } from "../../lib/format";
  import { toPlain } from "../../lib/markup";
  import Avatar from "../Avatar.svelte";
  import Icon from "../Icon.svelte";
  import PageHead from "./PageHead.svelte";

  let { kind }: { kind: "saved" | "starred" } = $props();
  let saved = $state<MessageView[]>([]);
  let pinned = $state<MessageView[]>([]);
  let loading = $state(true);

  async function load() {
    loading = true;
    try {
      if (kind === "saved") {
        const out: MessageView[] = [];
        for (const id of app.saved) {
          try {
            out.push(await core.fetchMessage(id));
          } catch {
            /* a chat that is locked or gone */
          }
        }
        saved = out.sort((a, b) => b.ts - a.ts);
      } else {
        const out: MessageView[] = [];
        for (const conv of Object.keys(app.pins)) if (app.pins[conv]?.length && app.rows[conv]) out.push(...(await core.getPins(conv).catch(() => [])));
        pinned = out.sort((a, b) => b.ts - a.ts);
      }
    } catch (e) {
      fail(e);
    } finally {
      loading = false;
    }
  }
  onMount(load);
  const favs = $derived(Object.values(app.rows).filter((r) => r.favourite));
  const where = (m: MessageView) => app.rows[m.conv_id]?.title ?? "Chat";
  const who = (m: MessageView) => app.users[m.sender_id]?.display_name ?? "Someone";
  async function unsave(m: MessageView) {
    try {
      await core.saveMessage(m.id, false);
      saved = saved.filter((x) => x.id !== m.id);
    } catch (e) {
      fail(e);
    }
  }
</script>

<div class="st-page">
  <PageHead title={kind === "saved" ? "Saved Messages" : "Starred & Pinned"} />
  <div class="st-body scroll" class:desk={!app.phone}>
    {#if kind === "starred"}
      <h3 class="st-cap">Pinned chats</h3>
      <section class="st-card">
        {#each favs as r (r.id)}
          <button class="st-row" onclick={() => { app.tab = "chats"; void openConversation(r.id); }}>
            <Avatar id={r.avatar} initials={r.initials} size={38} tile={r.kind === "channel"} />
            <span class="st-tx"><b>{r.kind === "channel" ? "# " : ""}{r.title}</b></span><Icon name="right" size={18} />
          </button>
        {:else}
          <div class="st-blk"><p class="st-note">No pinned chats. Use Pin to top on a chat's menu.</p></div>
        {/each}
      </section>
      <h3 class="st-cap">Pinned messages</h3>
    {/if}
    <section class="st-card">
      {#each kind === "saved" ? saved : pinned as m (m.id)}
        <div class="st-row">
          <button class="item" onclick={() => void jumpTo(m.conv_id, m.id, m.seq)}>
            <span class="st-tx"><b>{who(m)} <small class="where">in {where(m)} · {dayLabel(m.ts)}</small></b><small class="txt">{toPlain(m.body) || m.attachments[0]?.name || "Attachment"}</small></span>
          </button>
          {#if kind === "saved"}<button class="st-btn sm" onclick={() => unsave(m)}>Remove</button>{/if}
        </div>
      {:else}
        <div class="st-blk"><p class="st-note">{loading ? "Loading…" : kind === "saved" ? "Nothing saved yet. Open a message's menu and choose Save." : "No pinned messages in your chats."}</p></div>
      {/each}
    </section>
  </div>
</div>

<style>
  .item { flex: 1; min-width: 0; text-align: left; }
  .where { font-weight: 400; }
  .txt { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .sm { padding: 6px 12px; font-size: 13.5px; }
</style>
