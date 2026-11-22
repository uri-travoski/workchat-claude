<script lang="ts">
  import { onMount } from "svelte";
  import { app, closeSheet, fail } from "../lib/app.svelte";
  import { pickAndPreview, videoMeta } from "../lib/attach";
  import { core, type Staged } from "../lib/core";
  import { fileTile } from "../lib/filetype";
  import { duration, fileSize } from "../lib/format";
  import { mediaUrl, posterUrl } from "../lib/media";
  import Icon from "./Icon.svelte";
  import Sheet from "./Sheet.svelte";

  let { sheet }: { sheet: Extract<NonNullable<typeof app.sheet>, { kind: "preview" }> } = $props();
  let busy = $state(false);
  const files = $derived(sheet.files);
  const total = $derived(files.reduce((n, f) => n + f.size, 0));
  const single = $derived(files.length === 1 ? files[0] : null);
  const posters = $state<Record<string, string>>({});

  // Videos get a poster frame, their size and duration (the server stores these so receivers see a card at once).
  $effect(() => {
    for (const f of files) {
      if (f.kind === "video" && !(f.id in posters)) {
        posters[f.id] = "";
        void videoMeta(mediaUrl(f.id)).then(async (m) => {
          if (!m) return;
          if (m.poster_b64) posters[f.id] = posterUrl(m.poster_b64)!;
          await core.setStagedMeta(f.id, m).catch(() => {});
          f.poster_b64 = m.poster_b64 ?? f.poster_b64;
          f.width = m.width ?? f.width;
          f.height = m.height ?? f.height;
          f.duration_ms = m.duration_ms ?? f.duration_ms;
        });
      }
    }
  });

  function remove(f: Staged) {
    void core.unstage(f.id);
    sheet.files = sheet.files.filter((x) => x.id !== f.id);
    if (!sheet.files.length) closeSheet();
  }
  function cancel() {
    for (const f of sheet.files) void core.unstage(f.id).catch(() => {});
    closeSheet();
  }
  async function send() {
    if (busy) return;
    busy = true;
    try {
      await core.sendFiles(sheet.conv, sheet.files.map((f) => f.id), sheet.caption, sheet.thread);
      closeSheet();
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }
  let ta = $state<HTMLTextAreaElement>();
  onMount(() => ta?.focus());
</script>

<Sheet title={files.length === 1 ? "Send file" : `Send ${files.length} files`} onclose={cancel}>
  {#if single && (single.kind === "image" || (single.kind === "video" && posters[single.id]))}
    <div class="hero">
      {#if single.kind === "image"}<img src={mediaUrl(single.id)} alt={single.name} />{:else}<img src={posters[single.id]} alt="" /><span class="pl"><Icon name="play" size={26} fill /></span>{/if}
    </div>
    <div class="cap1"><b>{single.name}</b><span>{fileSize(single.size)}{single.duration_ms ? " · " + duration(single.duration_ms) : ""}</span></div>
  {:else}
    <div class="list">
      {#each files as f (f.id)}
        {@const tile = fileTile(f.name, f.content_type)}
        <div class="it">
          {#if f.kind === "image"}
            <img class="th" src={mediaUrl(f.id)} alt="" />
          {:else if f.kind === "video" && posters[f.id]}
            <img class="th" src={posters[f.id]} alt="" />
          {:else}
            <span class="th tile" style="color:{tile.fg};background:{tile.bg}"><Icon name={f.kind === "audio" ? "music" : f.kind === "video" ? "video" : "file"} size={20} /><b>{tile.label}</b></span>
          {/if}
          <span class="nm"><b>{f.name}</b><small>{fileSize(f.size)}{f.pages ? ` • ${f.pages} pages` : ""}{f.duration_ms ? " · " + duration(f.duration_ms) : ""}</small></span>
          <button class="rm" aria-label={`Remove ${f.name}`} onclick={() => remove(f)}><Icon name="x" size={17} /></button>
        </div>
      {/each}
    </div>
  {/if}
  <textarea bind:this={ta} bind:value={sheet.caption} rows="2" placeholder="Add a caption…" aria-label="Caption" onkeydown={(e) => { if (e.key === "Enter" && !e.shiftKey && !app.phone) { e.preventDefault(); void send(); } }}></textarea>
  {#snippet footer()}
    <button class="add" onclick={() => pickAndPreview(sheet.conv, sheet.thread, "any")}><Icon name="plus" size={18} stroke={2.2} /> Add more</button>
    <span class="tot">{fileSize(total)}</span>
    <button class="send" onclick={send} disabled={busy}><Icon name="send" size={20} /> Send</button>
  {/snippet}
</Sheet>

<style>
  .hero { position: relative; border-radius: 14px; overflow: hidden; background: #0c1220; display: flex; justify-content: center; max-height: 340px; }
  .hero img { max-width: 100%; max-height: 340px; object-fit: contain; display: block; }
  .pl { position: absolute; left: 50%; top: 50%; width: 56px; height: 56px; margin: -28px; border-radius: 50%; background: rgba(12, 18, 32, 0.6); color: #fff; display: flex; align-items: center; justify-content: center; padding-left: 3px; }
  .cap1 { display: flex; flex-direction: column; margin: 8px 2px 4px; font-size: 14px; }
  .cap1 span { color: var(--muted); font-size: 12.5px; }
  .list { display: flex; flex-direction: column; gap: 8px; max-height: 300px; overflow-y: auto; padding: 2px 0; }
  .it { display: flex; align-items: center; gap: 12px; background: var(--surface-2); border: 1px solid var(--line); border-radius: 12px; padding: 7px 8px; }
  .th { width: 50px; height: 50px; border-radius: 9px; object-fit: cover; flex: none; background: #0c1220; }
  .th.tile { display: inline-flex; flex-direction: column; align-items: center; justify-content: center; }
  .th.tile b { font-size: 9.5px; }
  .nm { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .nm b { font-size: 14.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .nm small { color: var(--muted); font-size: 12.5px; }
  .rm { width: 32px; height: 32px; border-radius: 50%; color: var(--muted); display: inline-flex; align-items: center; justify-content: center; }
  .rm:hover { background: var(--hover); }
  textarea { width: 100%; margin-top: 12px; border: 1px solid var(--line); border-radius: 14px; background: var(--surface-2); padding: 10px 12px; font-size: 15.5px; outline: none; resize: none; }
  textarea:focus { border-color: color-mix(in srgb, var(--blue) 55%, var(--line)); }
  .add { display: inline-flex; align-items: center; gap: 6px; color: var(--blue); font-weight: 700; padding: 8px 6px; }
  .tot { margin-left: auto; color: var(--muted); font-size: 13px; }
  .send { display: inline-flex; align-items: center; gap: 8px; background: linear-gradient(145deg, var(--accent), var(--accent-2)); color: #fff; font-weight: 700; border-radius: 22px; padding: 11px 22px; }
  .send:disabled { opacity: 0.6; }
</style>
