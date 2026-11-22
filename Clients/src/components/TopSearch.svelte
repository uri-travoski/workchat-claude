<script lang="ts">
  import { app, jumpTo, userAvatar, userName } from "../lib/app.svelte";
  import { core, type FileHit, type SearchHit, type User } from "../lib/core";
  import { fileTile, isImage, isVideo } from "../lib/filetype";
  import { openFile } from "../lib/files";
  import { dayLabel, fileSize } from "../lib/format";
  import { registerLayer } from "../lib/overlays";
  import { initialsOf } from "../lib/people";
  import { mergeFiles, mergePeople, snippetParts, stepIndex } from "../lib/search";
  import { viewerItem } from "../lib/viewer";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";

  // Global search (top bar): messages, people and files from the server. Independent of the list box above the chats.
  let open = $state(false);
  let loading = $state(false);
  let error = $state("");
  let hits = $state<SearchHit[]>([]);
  let people = $state<User[]>([]);
  let files = $state<FileHit[]>([]);
  let sel = $state(-1);
  let gen = 0;
  let root = $state<HTMLDivElement>();
  let input = $state<HTMLInputElement>();

  const q = $derived(app.gsearch.trim());
  // A chat that is not in the list (locked) never shows up here.
  const visible = (conv: string) => !!app.rows[conv];
  type Row = { t: "message"; h: SearchHit } | { t: "person"; u: User } | { t: "file"; f: FileHit };
  const rowsAll = $derived<Row[]>([
    ...hits.filter((h) => visible(h.message.conv_id)).map((h): Row => ({ t: "message", h })),
    ...people.map((u): Row => ({ t: "person", u })),
    ...files.filter((f) => visible(f.conv_id)).map((f): Row => ({ t: "file", f })),
  ]);
  const msgRows = $derived(rowsAll.filter((r) => r.t === "message"));
  const personRows = $derived(rowsAll.filter((r) => r.t === "person"));
  const fileRows = $derived(rowsAll.filter((r) => r.t === "file"));
  const flat = $derived([...msgRows, ...personRows, ...fileRows]);

  $effect(() => {
    const text = q;
    if (text.length < 2) {
      hits = [];
      people = [];
      files = [];
      loading = false;
      error = "";
      sel = -1;
      return;
    }
    const my = ++gen;
    loading = true;
    const t = setTimeout(async () => {
      try {
        const [s, dir, fl] = await Promise.all([
          core.search(text, null, 20),
          core.listUsers(text).catch(() => [] as User[]),
          core.listFiles({ q: text, limit: 8 }).catch(() => [] as FileHit[]),
        ]);
        if (my !== gen) return;
        hits = s.hits;
        people = mergePeople(s.people, dir, app.me?.user.id).slice(0, 6);
        files = mergeFiles(s.files ?? [], fl).slice(0, 8);
        error = "";
        sel = -1;
      } catch (e) {
        if (my === gen) error = e instanceof Object && "message" in e ? String((e as { message: string }).message) : "Search failed.";
      } finally {
        if (my === gen) loading = false;
      }
    }, 250);
    return () => clearTimeout(t);
  });

  // Escape closes the panel (and only it); a click outside does too.
  $effect(() => (open ? registerLayer(() => (open = false)) : undefined));
  $effect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (root && !root.contains(e.target as Node)) open = false;
    };
    window.addEventListener("pointerdown", away, true);
    return () => window.removeEventListener("pointerdown", away, true);
  });

  function choose(r: Row) {
    open = false;
    if (r.t === "message") void jumpTo(r.h.message.conv_id, r.h.message.id, r.h.message.seq);
    else if (r.t === "person") app.sheet = { kind: "profile", userId: r.u.id };
    else {
      const a = r.f.attachment;
      if (isImage(a.content_type) || isVideo(a.content_type)) app.viewer = { items: [viewerItem(a, { conv_id: r.f.conv_id, id: r.f.message_id, ts: r.f.ts, sender_id: r.f.sender_id })], index: 0 };
      else void openFile(a);
    }
  }
  function key(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      open = true;
      sel = stepIndex(sel, e.key === "ArrowDown" ? 1 : -1, flat.length);
      queueMicrotask(() => root?.querySelector(".r.sel")?.scrollIntoView({ block: "nearest" }));
    } else if (e.key === "Enter") {
      const r = flat[sel] ?? flat[0];
      if (r) {
        e.preventDefault();
        choose(r);
      }
    }
  }
  function clear() {
    app.gsearch = "";
    open = false;
    input?.focus();
  }
  const conv = (id: string) => {
    const r = app.rows[id];
    return r ? r.title : "";
  };
  const idx = (r: Row) => flat.indexOf(r);
</script>

<div class="wrap" bind:this={root}>
  <label class="gs">
    <Icon name="search" size={18} />
    <input
      bind:this={input}
      class="gsearch"
      placeholder="Search messages, people and files"
      aria-label="Search messages, people and files"
      role="combobox"
      aria-expanded={open && q.length >= 2}
      aria-controls="gs-panel"
      autocomplete="off"
      bind:value={app.gsearch}
      onfocus={() => (open = true)}
      oninput={() => (open = true)}
      onkeydown={key}
    />
    {#if app.gsearch}<button type="button" aria-label="Clear search" onclick={clear}><Icon name="x" size={16} /></button>{/if}
  </label>
  {#if open && q.length >= 2}
    <div class="panel scroll" id="gs-panel" role="listbox" aria-label="Search results">
      {#if error}<p class="n">{error}</p>{/if}
      {#if msgRows.length}
        <div class="h">Messages</div>
        {#each msgRows as r (r.t === "message" ? r.h.message.id : "")}
          {#if r.t === "message"}
            <button class="r" class:sel={idx(r) === sel} role="option" aria-selected={idx(r) === sel} onmousemove={() => (sel = idx(r))} onclick={() => choose(r)}>
              <span class="ic"><Icon name="chat" size={18} /></span>
              <span class="t"><b>{userName(r.h.message.sender_id)} <small>in {conv(r.h.message.conv_id)} · {dayLabel(r.h.message.ts)}</small></b>
                <span class="sn">{#each snippetParts(r.h.snippet) as p}{#if p.hit}<mark>{p.t}</mark>{:else}{p.t}{/if}{/each}</span></span>
            </button>
          {/if}
        {/each}
      {/if}
      {#if personRows.length}
        <div class="h">People</div>
        {#each personRows as r (r.t === "person" ? r.u.id : "")}
          {#if r.t === "person"}
            <button class="r" class:sel={idx(r) === sel} role="option" aria-selected={idx(r) === sel} onmousemove={() => (sel = idx(r))} onclick={() => choose(r)}>
              <Avatar id={userAvatar(app.users[r.u.id] ?? r.u)} initials={initialsOf(r.u.display_name)} size={36} presence={(app.users[r.u.id] ?? r.u).presence ?? null} />
              <span class="t"><b>{r.u.display_name}</b><small>{r.u.title ?? "@" + r.u.username}</small></span>
            </button>
          {/if}
        {/each}
      {/if}
      {#if fileRows.length}
        <div class="h">Files</div>
        {#each fileRows as r (r.t === "file" ? r.f.attachment.id : "")}
          {#if r.t === "file"}
            {@const t = fileTile(r.f.attachment.name, r.f.attachment.content_type)}
            <button class="r" class:sel={idx(r) === sel} role="option" aria-selected={idx(r) === sel} onmousemove={() => (sel = idx(r))} onclick={() => choose(r)}>
              <span class="tile" style="color:{t.fg};background:{t.bg}"><b>{t.label}</b></span>
              <span class="t"><b>{r.f.attachment.name}</b><small>{fileSize(r.f.attachment.size)} · {conv(r.f.conv_id)}</small></span>
            </button>
          {/if}
        {/each}
      {/if}
      {#if !flat.length && !error}<p class="n">{loading ? "Searching…" : `No messages, people or files match "${q}".`}</p>{/if}
    </div>
  {/if}
</div>

<style>
  .wrap { position: relative; }
  .gs { display: flex; align-items: center; gap: 8px; background: var(--chip); border-radius: 18px; padding: 0 12px; height: 36px; color: var(--muted); }
  .gs input { flex: 1; min-width: 0; border: 0; background: transparent; outline: none; color: var(--text); font-size: 15px; }
  .gs button { display: inline-flex; color: var(--muted); }
  .panel { position: absolute; z-index: 40; top: 42px; left: 0; right: 0; max-height: min(520px, calc(100vh - 90px)); background: var(--surface); border: 1px solid var(--line); border-radius: 14px; box-shadow: var(--shadow-pop); overflow-y: auto; padding-bottom: 6px; }
  .h { padding: 10px 16px 5px; font-size: 11.5px; font-weight: 700; letter-spacing: 0.07em; color: var(--text-2); text-transform: uppercase; }
  .r { width: 100%; display: flex; align-items: center; gap: 12px; padding: 7px 16px; text-align: left; }
  .r.sel { background: color-mix(in srgb, var(--blue) 12%, var(--surface)); }
  .t { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .t b { font-size: 14.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t small { color: var(--muted); font-weight: 400; font-size: 12.5px; }
  .sn { font-size: 13.5px; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  mark { background: color-mix(in srgb, #f5b73b 40%, transparent); color: inherit; border-radius: 3px; padding: 0 1px; }
  .ic { width: 36px; height: 36px; border-radius: 50%; background: var(--chip); color: var(--blue); display: inline-flex; align-items: center; justify-content: center; flex: none; }
  .tile { width: 36px; height: 40px; border-radius: 8px; display: inline-flex; align-items: center; justify-content: center; flex: none; }
  .tile b { font-size: 9.5px; }
  .n { text-align: center; color: var(--muted); margin: 22px 20px; }
</style>
