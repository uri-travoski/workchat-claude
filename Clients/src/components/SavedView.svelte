<script lang="ts">
  import { app, fail, jumpTo } from "../lib/app.svelte";
  import { core, type MessageView } from "../lib/core";
  import { fileTile } from "../lib/filetype";
  import { clock, dayLabel, fileSize } from "../lib/format";
  import { toPlain } from "../lib/markup";
  import { initialsOf } from "../lib/people";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";

  // Saved messages (GET /saved, kept current by `saved` events): who wrote it, where, when, its files; click to jump to it.
  let items = $state<MessageView[]>([]);
  let loading = $state(true);
  let gen = 0;

  // Reloads when the list of saved ids changes (a save or unsave on any device arrives as an event).
  $effect(() => {
    void app.saved.length;
    void app.saved.join(",");
    const my = ++gen;
    loading = true;
    core.listSavedMessages().then(
      (list) => {
        if (my !== gen) return;
        items = list.filter((m) => !m.deleted && !!app.rows[m.conv_id]);
        loading = false;
      },
      (e) => {
        if (my !== gen) return;
        loading = false;
        fail(e);
      },
    );
  });

  const who = (m: MessageView) => app.users[m.sender_id];
  const where = (m: MessageView) => {
    const r = app.rows[m.conv_id];
    return r ? (r.kind === "channel" ? "# " : "") + r.title : "Chat";
  };
  async function unsave(m: MessageView) {
    try {
      await core.saveMessage(m.id, false);
      items = items.filter((x) => x.id !== m.id);
    } catch (e) {
      fail(e);
    }
  }
</script>

<div class="sv">
  {#if !items.length}
    <p class="n">{loading ? "Loading saved messages…" : "Nothing saved yet. Use Save on a message's menu."}</p>
  {/if}
  {#each items as m (m.id)}
    {@const u = who(m)}
    <div class="row">
      <button class="main" aria-label={`Open saved message from ${u?.display_name ?? "someone"} in ${where(m)}`} onclick={() => jumpTo(m.conv_id, m.id, m.seq)}>
        <Avatar id={u?.avatar ?? null} initials={initialsOf(u?.display_name ?? "?")} size={40} />
        <span class="tx">
          <span class="top"><b>{u?.display_name ?? "Someone"}</b><small>{dayLabel(m.ts)}, {clock(m.ts)}</small></span>
          <span class="in">in {where(m)}</span>
          {#if toPlain(m.body)}<span class="body">{toPlain(m.body)}</span>{/if}
          {#each m.attachments.filter((a) => !a.removed) as a (a.id)}
            {@const t = fileTile(a.name, a.content_type)}
            <span class="att"><span class="tile" style="color:{t.fg};background:{t.bg}">{t.label}</span><span class="an">{a.name}</span><small>{fileSize(a.size)}</small></span>
          {/each}
        </span>
      </button>
      <button class="un" aria-label="Unsave" title="Unsave" onclick={() => unsave(m)}><Icon name="bookmark" size={19} fill /></button>
    </div>
  {/each}
</div>

<style>
  .sv { background: var(--surface); min-height: 100%; }
  .n { text-align: center; color: var(--muted); margin: 36px 24px; }
  .row { display: flex; align-items: flex-start; border-bottom: 1px solid var(--line); }
  .main { flex: 1; min-width: 0; display: flex; gap: 12px; padding: 11px 4px 11px 16px; text-align: left; }
  .main:hover { background: var(--hover); }
  .tx { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .top { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; }
  .top b { font-size: 15px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .top small { color: var(--muted); font-size: 12px; flex: none; }
  .in { color: var(--blue); font-size: 12.5px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .body { font-size: 14px; color: var(--text-2); display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; overflow-wrap: anywhere; }
  .att { display: flex; align-items: center; gap: 8px; margin-top: 3px; font-size: 13px; }
  .tile { font-size: 9.5px; font-weight: 700; padding: 3px 5px; border-radius: 6px; flex: none; }
  .an { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
  .att small { color: var(--muted); flex: none; }
  .un { width: 40px; height: 40px; margin: 8px 8px 0 0; border-radius: 50%; color: var(--blue); display: inline-flex; align-items: center; justify-content: center; flex: none; }
  .un:hover { background: var(--hover); }
</style>
