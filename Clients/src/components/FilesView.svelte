<script lang="ts">
  import { app, fail, jumpTo, userName } from "../lib/app.svelte";
  import { core, type FileHit } from "../lib/core";
  import { openFile } from "../lib/files";
  import { docLine, fileTile, isImage, isVideo } from "../lib/filetype";
  import { dayLabel, duration, fileSize } from "../lib/format";
  import { mediaUrl, posterUrl } from "../lib/media";
  import { viewerItem } from "../lib/viewer";
  import { togglePlay, isPlaying } from "../lib/player.svelte";
  import Icon from "./Icon.svelte";

  type Tab = "media" | "docs" | "audio";
  let tab = $state<Tab>("media");
  let q = $state("");
  let items = $state<FileHit[]>([]);
  let loading = $state(false);
  let done = $state(false);
  let error = $state("");
  let gen = 0;
  const PAGE = 60;
  const offs: Record<string, number> = {};
  const kinds = $derived(tab === "media" ? ["image", "video"] : tab === "docs" ? ["document"] : ["audio"]);

  async function load(reset: boolean) {
    if (loading || (done && !reset)) return;
    if (reset) {
      gen++;
      items = [];
      done = false;
      for (const k of Object.keys(offs)) delete offs[k];
    }
    const my = gen;
    loading = true;
    error = "";
    try {
      const parts = await Promise.all(
        kinds.map(async (k) => {
          const rows = await core.listFiles({ kind: k, q: q.trim() || undefined, offset: offs[k] ?? 0, limit: PAGE });
          offs[k] = (offs[k] ?? 0) + rows.length;
          return rows;
        }),
      );
      if (my !== gen) return;
      const got = parts.flat();
      if (parts.every((p) => p.length < PAGE)) done = true;
      const seen = new Set(items.map((i) => i.attachment.id));
      items = [...items, ...got.filter((g) => !seen.has(g.attachment.id))].sort((a, b) => b.ts - a.ts);
    } catch (e) {
      if (my === gen) error = e instanceof Object && "message" in e ? String((e as { message: string }).message) : "Could not load files.";
    } finally {
      if (my === gen) loading = false;
    }
  }
  $effect(() => {
    tab;
    const t = setTimeout(() => void load(true), q ? 250 : 0);
    q;
    return () => clearTimeout(t);
  });
  function scroll(e: Event) {
    const el = e.currentTarget as HTMLElement;
    if (el.scrollHeight - el.scrollTop - el.clientHeight < 400) void load(false);
  }
  const mediaItems = $derived(items.filter((i) => isImage(i.attachment.content_type) || isVideo(i.attachment.content_type)));
  function open(f: FileHit) {
    const list = mediaItems.map((h) => viewerItem(h.attachment, { conv_id: h.conv_id, id: h.message_id, ts: h.ts, sender_id: h.sender_id }));
    app.viewer = { items: list, index: Math.max(0, mediaItems.indexOf(f)) };
  }
  const count = (n: number | undefined) => (n === undefined ? "" : String(n));
  const c = $derived(app.counts);
  const tabs = $derived([
    { id: "media" as Tab, label: "Media", n: c ? c.files_image + c.files_video : undefined },
    { id: "docs" as Tab, label: "Documents", n: c?.files_document },
    { id: "audio" as Tab, label: "Audio", n: c?.files_audio },
  ]);
  async function save(f: FileHit) {
    try {
      await core.saveAttachment(f.attachment.id, f.attachment.name);
    } catch (e) {
      fail(e);
    }
  }
</script>

<div class="fv">
  <div class="sub" role="tablist" aria-label="File type">
    {#each tabs as t}<button role="tab" aria-selected={tab === t.id} class:on={tab === t.id} onclick={() => (tab = t.id)}>{t.label}{#if t.n !== undefined}<em>{count(t.n)}</em>{/if}</button>{/each}
  </div>
  <div class="srch"><Icon name="search" size={17} /><input placeholder="Search files" bind:value={q} aria-label="Search files" />{#if q}<button aria-label="Clear" onclick={() => (q = "")}><Icon name="x" size={16} /></button>{/if}</div>
  <div class="scroll body" onscroll={scroll}>
    {#if error}<p class="n">{error}</p>
    {:else if !items.length && !loading}<p class="n">{q ? "No files match." : "No files shared yet."}</p>{/if}
    {#if tab === "media"}
      <div class="grid">
        {#each mediaItems as f (f.attachment.id)}
          <button class="cell" aria-label={f.attachment.name} onclick={() => open(f)}>
            {#if isImage(f.attachment.content_type)}<img src={mediaUrl(f.attachment.id)} alt="" loading="lazy" decoding="async" />
            {:else if f.attachment.meta?.poster_b64}<img src={posterUrl(f.attachment.meta.poster_b64)} alt="" />
            {:else}<span class="np"><Icon name="video" size={24} /></span>{/if}
            {#if isVideo(f.attachment.content_type)}<span class="vb"><Icon name="play" size={12} fill /> {f.attachment.meta?.duration_ms ? duration(f.attachment.meta.duration_ms) : ""}</span>{/if}
          </button>
        {/each}
      </div>
    {:else if tab === "docs"}
      {#each items as f (f.attachment.id)}
        {@const t = fileTile(f.attachment.name, f.attachment.content_type)}
        <div class="row">
          <button class="main" title="Open" onclick={() => void openFile(f.attachment)}>
            <span class="tile" style="color:{t.fg};background:{t.bg}"><Icon name="file" size={19} /><b>{t.label}</b></span>
            <span class="tx"><b>{f.attachment.name}</b><small>{docLine(f.attachment.size, f.attachment.meta?.pages, fileSize)} · {userName(f.sender_id)} · {dayLabel(f.ts)}</small></span>
          </button>
          <button class="dl" aria-label="Show in chat" title="Show in chat" onclick={() => jumpTo(f.conv_id, f.message_id)}><Icon name="chat" size={19} /></button>
          <button class="dl" aria-label="Save to Downloads" title="Save to Downloads" onclick={() => save(f)}><Icon name="download" size={19} /></button>
        </div>
      {/each}
    {:else}
      {#each items as f (f.attachment.id)}
        <div class="row">
          <button class="pl" aria-label={isPlaying(f.attachment.id) ? "Pause" : "Play"} onclick={() => togglePlay(f.attachment.id)}><Icon name={isPlaying(f.attachment.id) ? "pause" : "play"} size={18} fill /></button>
          <button class="main" onclick={() => jumpTo(f.conv_id, f.message_id)}>
            <span class="tx"><b>{f.attachment.name}</b><small>{f.attachment.meta?.duration_ms ? duration(f.attachment.meta.duration_ms) + " · " : ""}{userName(f.sender_id)} · {dayLabel(f.ts)}</small></span>
          </button>
          <button class="dl" aria-label="Open in default app" title="Open in default app" onclick={() => void openFile(f.attachment)}><Icon name="external" size={19} /></button>
        </div>
      {/each}
    {/if}
    {#if loading}<p class="n">Loading…</p>{/if}
    <div class="pad"></div>
  </div>
</div>

<style>
  .fv { display: flex; flex-direction: column; flex: 1; min-height: 0; }
  .sub { display: flex; gap: 8px; padding: 0 16px 8px; }
  .sub button { padding: 7px 14px; border-radius: 16px; font-size: 14px; font-weight: 600; color: var(--text-2); display: inline-flex; gap: 6px; align-items: baseline; }
  .sub button.on { background: color-mix(in srgb, var(--blue) 14%, var(--surface)); color: var(--blue); }
  .sub em { font-style: normal; font-size: 12px; opacity: 0.8; }
  .srch { display: flex; align-items: center; gap: 8px; margin: 0 16px 8px; background: var(--chip); border-radius: 18px; padding: 0 12px; color: var(--muted); }
  .srch input { flex: 1; border: 0; background: transparent; padding: 9px 0; outline: none; font-size: 15px; }
  .body { flex: 1; min-height: 0; background: var(--surface); }
  .n { text-align: center; color: var(--muted); margin: 28px 20px; }
  .grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 2px; }
  .cell { position: relative; aspect-ratio: 1; overflow: hidden; background: var(--checker); padding: 0; }
  .cell img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .np { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; color: #c9d4e8; }
  .vb { position: absolute; left: 5px; bottom: 5px; background: rgba(12, 18, 32, 0.66); color: #fff; font-size: 11.5px; font-weight: 600; padding: 1px 7px 1px 5px; border-radius: 8px; display: inline-flex; gap: 3px; align-items: center; }
  .row { display: flex; align-items: center; border-bottom: 1px solid var(--line); }
  .main { flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; padding: 10px 6px 10px 16px; text-align: left; }
  .main:hover { background: var(--hover); }
  .tile { width: 42px; height: 46px; border-radius: 9px; display: inline-flex; flex-direction: column; align-items: center; justify-content: center; flex: none; }
  .tile b { font-size: 9.5px; line-height: 1; }
  .tx { min-width: 0; display: flex; flex-direction: column; }
  .tx b { font-size: 15px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tx small { color: var(--muted); font-size: 12.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .dl { width: 40px; height: 40px; margin-right: 8px; border-radius: 50%; color: var(--text-2); display: inline-flex; align-items: center; justify-content: center; }
  .dl:hover { background: var(--hover); }
  .pl { width: 40px; height: 40px; margin-left: 14px; border-radius: 50%; background: var(--accent); color: #fff; display: inline-flex; align-items: center; justify-content: center; flex: none; }
  .pad { height: 90px; }
</style>
