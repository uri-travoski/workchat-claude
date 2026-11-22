<script lang="ts">
  import { app, jumpTo, userAvatar, userName } from "../lib/app.svelte";
  import { core, type SearchResponse } from "../lib/core";
  import { fileTile, isImage, isVideo } from "../lib/filetype";
  import { openFile } from "../lib/files";
  import { viewerItem } from "../lib/viewer";
  import { dayLabel, fileSize } from "../lib/format";
  import { initialsOf } from "../lib/people";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";

  let { q }: { q: string } = $props();
  let res = $state<SearchResponse | null>(null);
  let loading = $state(false);
  let error = $state("");
  let gen = 0;

  $effect(() => {
    const text = q.trim();
    if (text.length < 2) {
      res = null;
      return;
    }
    const my = ++gen;
    loading = true;
    const t = setTimeout(async () => {
      try {
        const r = await core.search(text, null, 30);
        if (my === gen) {
          res = r;
          error = "";
        }
      } catch (e) {
        if (my === gen) error = e instanceof Object && "message" in e ? String((e as { message: string }).message) : "Search failed.";
      } finally {
        if (my === gen) loading = false;
      }
    }, 280);
    return () => clearTimeout(t);
  });

  /** Splits a server snippet on its `[[match]]` markers. */
  function parts(s: string): { t: string; hit: boolean }[] {
    const out: { t: string; hit: boolean }[] = [];
    let i = 0;
    for (const m of s.matchAll(/\[\[(.*?)\]\]/g)) {
      if (m.index! > i) out.push({ t: s.slice(i, m.index), hit: false });
      out.push({ t: m[1], hit: true });
      i = m.index! + m[0].length;
    }
    if (i < s.length) out.push({ t: s.slice(i), hit: false });
    return out;
  }
  const convTitle = (id: string) => {
    const r = app.rows[id];
    return r ? r.title : "";
  };
</script>

{#if q.trim().length >= 2}
  <div class="gs">
    {#if error}<p class="n">{error}</p>{/if}
    {#if res?.people.length}
      <div class="h">People</div>
      {#each res.people.slice(0, 5) as u (u.id)}
        <button class="r" onclick={() => (app.sheet = { kind: "profile", userId: u.id })}>
          <Avatar id={userAvatar(u)} initials={initialsOf(u.display_name)} size={38} presence={u.presence ?? null} bot={u.is_bot} />
          <span class="t"><b>{u.display_name}</b><small>{u.title ?? "@" + u.username}</small></span>
        </button>
      {/each}
    {/if}
    {#if res?.hits.length}
      <div class="h">Messages</div>
      {#each res.hits as h (h.message.id)}
        <button class="r" onclick={() => jumpTo(h.message.conv_id, h.message.id, h.message.seq)}>
          <span class="ic"><Icon name="chat" size={18} /></span>
          <span class="t"><b>{userName(h.message.sender_id)} <small>in {convTitle(h.message.conv_id)} · {dayLabel(h.message.ts)}</small></b>
            <span class="sn">{#each parts(h.snippet) as p}{#if p.hit}<mark>{p.t}</mark>{:else}{p.t}{/if}{/each}</span></span>
        </button>
      {/each}
    {/if}
    {#if res?.files?.length}
      <div class="h">Files</div>
      {#each res.files.slice(0, 8) as f (f.attachment.id)}
        {@const t = fileTile(f.attachment.name, f.attachment.content_type)}
        <button class="r" onclick={() => (isImage(f.attachment.content_type) || isVideo(f.attachment.content_type) ? (app.viewer = { items: [viewerItem(f.attachment, { conv_id: f.conv_id, id: f.message_id, ts: f.ts, sender_id: f.sender_id })], index: 0 }) : void openFile(f.attachment))}>
          <span class="tile" style="color:{t.fg};background:{t.bg}"><b>{t.label}</b></span>
          <span class="t"><b>{f.attachment.name}</b><small>{fileSize(f.attachment.size)} · {convTitle(f.conv_id)}</small></span>
        </button>
      {/each}
    {/if}
    {#if res && !error && !res.hits.length && !res.people.length && !res.files?.length}<p class="n">No messages, people or files match "{q.trim()}".</p>
    {:else if loading && !res}<p class="n">Searching…</p>{/if}
  </div>
{/if}

<style>
  .gs { background: var(--surface); }
  .h { padding: 12px 18px 6px; font-size: 12px; font-weight: 700; letter-spacing: 0.07em; color: var(--text-2); text-transform: uppercase; background: var(--page); }
  .r { width: 100%; display: flex; align-items: center; gap: 12px; padding: 9px 16px; text-align: left; border-bottom: 1px solid var(--line); }
  .r:hover { background: var(--hover); }
  .t { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .t b { font-size: 15px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t small { color: var(--muted); font-weight: 400; font-size: 12.5px; }
  .sn { font-size: 14px; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  mark { background: color-mix(in srgb, #f5b73b 40%, transparent); color: inherit; border-radius: 3px; padding: 0 1px; }
  .ic { width: 38px; height: 38px; border-radius: 50%; background: var(--chip); color: var(--blue); display: inline-flex; align-items: center; justify-content: center; flex: none; }
  .tile { width: 38px; height: 42px; border-radius: 8px; display: inline-flex; align-items: center; justify-content: center; flex: none; }
  .tile b { font-size: 9.5px; }
  .n { text-align: center; color: var(--muted); margin: 24px 20px; }
</style>
