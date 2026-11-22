<script lang="ts">
  import { app, fail, jumpTo } from "../lib/app.svelte";
  import { core, type MessageView } from "../lib/core";
  import { fileTile } from "../lib/filetype";
  import { clock, dayLabel, fileSize } from "../lib/format";
  import { toPlain } from "../lib/markup";
  import { initialsOf } from "../lib/people";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import Menu from "./Menu.svelte";
  import { labelsOf } from "../lib/labels";
  import { visibleSaved, withLabel } from "../lib/saved";

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
        items = visibleSaved(list, app.rows);
        loading = false;
      },
      (e) => {
        if (my !== gen) return;
        loading = false;
        fail(e);
      },
    );
  });

  // Narrowed to one label when the person chose it (desktop chips here, phone top-right menu).
  const shown = $derived(withLabel(items, app.labelFilter, app.savedLabels));
  // Desktop: every label as a chip. Phone: just the active one, to clear it.
  const chips = $derived(app.phone ? app.labels.filter((l) => l.id === app.labelFilter) : app.labels);
  const pick = (id: string) => (app.labelFilter = app.labelFilter === id ? null : id);

  const who = (m: MessageView) => app.users[m.sender_id];
  const where = (m: MessageView) => {
    const r = app.rows[m.conv_id];
    return r ? r.title : "Chat";
  };
  // Long press (or right click) on a row: Open in chat, Unsave.
  let menu = $state<{ m: MessageView; x: number; y: number } | null>(null);
  let press: ReturnType<typeof setTimeout> | undefined;
  let fired = false;
  function down(e: PointerEvent, m: MessageView) {
    if (e.pointerType === "mouse") return;
    fired = false;
    const [x, y] = [e.clientX, e.clientY];
    clearTimeout(press);
    press = setTimeout(() => {
      fired = true;
      navigator.vibrate?.(15);
      menu = { m, x, y };
    }, 520);
  }
  const cancel = () => clearTimeout(press);
  // Built with the message itself: the menu clears `menu` before it runs an item.
  const rowItems = (m: MessageView) => [
    { label: "Open in chat", icon: "chat", run: () => void jumpTo(m.conv_id, m.id, m.seq) },
    { label: "Unsave", icon: "bookmark", run: () => void unsave(m) },
  ];
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
  {#if chips.length}
    <div class="chips" role="group" aria-label="Filter by label">
      {#if !app.phone}<button class="chip" class:on={!app.labelFilter} aria-pressed={!app.labelFilter} onclick={() => (app.labelFilter = null)}>All</button>{/if}
      {#each chips as l (l.id)}
        <button class="chip" class:on={app.labelFilter === l.id} aria-pressed={app.labelFilter === l.id} onclick={() => pick(l.id)}>
          <i style="background:{l.color}"></i>{l.name}{#if app.phone}<Icon name="x" size={14} stroke={2.2} />{/if}
        </button>
      {/each}
    </div>
  {/if}
  {#if !shown.length}
    <p class="n">{loading ? "Loading saved messages…" : items.length && app.labelFilter ? "No messages with this label." : "Nothing saved yet. Use Save on a message's menu."}</p>
  {/if}
  {#each shown as m (m.id)}
    {@const u = who(m)}
    <div class="row" role="presentation" oncontextmenu={(e) => { e.preventDefault(); menu = { m, x: e.clientX, y: e.clientY }; }}>
      <button class="main" onpointerdown={(e) => down(e, m)} onpointerup={cancel} onpointerleave={cancel} onpointercancel={cancel} aria-label={`Open saved message from ${u?.display_name ?? "someone"} in ${where(m)}`} onclick={() => { if (fired) { fired = false; return; } void jumpTo(m.conv_id, m.id, m.seq); }}>
        <Avatar id={u?.avatar ?? null} initials={initialsOf(u?.display_name ?? "?")} size={40} />
        <span class="tx">
          <span class="top"><b>{u?.display_name ?? "Someone"}</b><small>{dayLabel(m.ts)}, {clock(m.ts)}</small></span>
          <span class="in">in {where(m)}</span>
          {#if toPlain(m.body)}<span class="body">{toPlain(m.body)}</span>{/if}
          {#each m.attachments.filter((a) => !a.removed) as a (a.id)}
            {@const t = fileTile(a.name, a.content_type)}
            <span class="att"><span class="tile" style="color:{t.fg};background:{t.bg}">{t.label}</span><span class="an">{a.name}</span><small>{fileSize(a.size)}</small></span>
          {/each}
          {#if app.savedLabels[m.id]?.length}
            <span class="lbs">{#each labelsOf(app.savedLabels[m.id], app.labels) as l (l.id)}<span class="lb"><i style="background:{l.color}"></i>{l.name}</span>{/each}</span>
          {/if}
        </span>
      </button>
      <button class="un" aria-label="Unsave" title="Unsave" onclick={() => unsave(m)}><Icon name="bookmark" size={19} fill /></button>
    </div>
  {/each}
</div>

{#if menu}
  <Menu x={menu.x} y={menu.y} onclose={() => (menu = null)} items={rowItems(menu.m)} />
{/if}

<style>
  .sv { background: var(--surface); min-height: 100%; }
  .chips { display: flex; flex-wrap: wrap; gap: 8px; padding: 10px 16px; border-bottom: 1px solid var(--line); }
  .chip { display: inline-flex; align-items: center; gap: 6px; padding: 6px 12px; border-radius: 16px; border: 1px solid var(--line); background: var(--chip); color: var(--text-2); font-size: 13.5px; font-weight: 600; }
  .chip:hover { background: var(--hover); }
  .chip.on { background: var(--accent); border-color: var(--accent); color: #fff; }
  .chip i, .lb i { width: 10px; height: 10px; border-radius: 50%; flex: none; }
  .n { text-align: center; color: var(--muted); margin: 36px 24px; }
  .row { display: flex; align-items: flex-start; border-bottom: 1px solid var(--line); }
  .main { -webkit-touch-callout: none; flex: 1; min-width: 0; display: flex; gap: 12px; padding: 11px 4px 11px 16px; text-align: left; }
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
  .lbs { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 4px; }
  .lb { display: inline-flex; align-items: center; gap: 5px; padding: 2px 9px; border-radius: 10px; background: var(--chip); color: var(--text-2); font-size: 12px; font-weight: 600; max-width: 100%; }
  .un { width: 40px; height: 40px; margin: 8px 8px 0 0; border-radius: 50%; color: var(--blue); display: inline-flex; align-items: center; justify-content: center; flex: none; }
  .un:hover { background: var(--hover); }
</style>
