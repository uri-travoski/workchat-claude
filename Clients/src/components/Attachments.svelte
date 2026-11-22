<script lang="ts">
  import { onMount } from "svelte";
  import { app, fail, prefs, toast } from "../lib/app.svelte";
  import { core, type MessageView } from "../lib/core";
  import { docLine, fileTile, isAudio, isImage, isVideo, isVoice } from "../lib/filetype";
  import { duration, fileSize } from "../lib/format";
  import { mediaUrl, posterUrl } from "../lib/media";
  import { openMedia } from "../lib/viewer";
  import { openFile } from "../lib/files";
  import Icon from "./Icon.svelte";
  import VoiceBubble from "./VoiceBubble.svelte";

  let { m, mine = false }: { m: MessageView; mine?: boolean } = $props();

  const live = $derived(m.attachments.filter((a) => !a.removed));
  const media = $derived(live.filter((a) => isImage(a.content_type) || isVideo(a.content_type)));
  const voices = $derived(live.filter((a) => isVoice(a)));
  const audios = $derived(live.filter((a) => isAudio(a.content_type) && !isVoice(a)));
  const docs = $derived(m.attachments.filter((a) => a.removed || !/^(image|video|audio)\//.test(a.content_type)));
  const shownMedia = $derived(media.slice(0, 4));
  const extra = $derived(media.length - 4);
  let playing = $state<string | null>(null);
  let brokenThumb = $state<Record<string, boolean>>({});
  // "Auto-Download Photos" off: a photo shows a download button until it is fetched (or already cached).
  let cached = $state<Record<string, boolean>>({});
  let forced = $state<Record<string, boolean>>({});
  const shown = (id: string) => prefs.autoPhotos || id.startsWith("up_") || id.startsWith("st_") || cached[id] || forced[id];
  async function checkCache() {
    if (prefs.autoPhotos) return;
    const ids = media.filter((a) => isImage(a.content_type)).map((a) => a.id);
    if (!ids.length) return;
    try {
      for (const id of await core.mediaCached(ids)) cached[id] = true;
    } catch {
      /* show the buttons */
    }
  }
  onMount(checkCache);

  async function save(id: string, name: string) {
    try {
      const p = await core.saveAttachment(id, name);
      toast(`Saved to ${p}`);
    } catch (e) {
      fail(e);
    }
  }
  const open = (id: string, name: string) => void openFile({ id, name });
  const ratio = (a: { meta?: { width?: number | null; height?: number | null } | null }) => {
    const w = a.meta?.width;
    const h = a.meta?.height;
    return w && h ? Math.min(1.8, Math.max(0.6, w / h)) : 1.4;
  };
</script>

{#if media.length === 1}
  {@const a = media[0]}
  <div class="card media single" style="aspect-ratio:{ratio(a)}">
    {#if isImage(a.content_type)}
      {#if shown(a.id)}
        <button class="fill" aria-label={`Open ${a.name}`} onclick={() => openMedia(m, a)}>
          <img src={mediaUrl(a.id)} alt={a.name} loading="lazy" decoding="async" />
        </button>
      {:else}
        <button class="fill dlph" aria-label={`Download photo ${a.name}`} onclick={() => (forced[a.id] = true)}><Icon name="download" size={26} /><span>Download photo{a.size ? ` (${fileSize(a.size)})` : ""}</span></button>
      {/if}
    {:else if playing === a.id}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video class="fill" src={mediaUrl(a.id)} controls autoplay poster={posterUrl(a.meta?.poster_b64)}></video>
    {:else}
      <button class="fill" aria-label={`Play ${a.name}`} onclick={() => (playing = a.id)}>
        {#if a.meta?.poster_b64}<img src={posterUrl(a.meta.poster_b64)} alt="" />{:else}<span class="noposter"><Icon name="video" size={34} /></span>{/if}
        <span class="playbtn"><svg width="26" height="26" viewBox="0 0 24 24" fill="#fff"><path d="M8 5.4v13.2a1 1 0 0 0 1.5.86l10.6-6.6a1 1 0 0 0 0-1.72L9.5 4.54A1 1 0 0 0 8 5.4Z" /></svg></span>
        {#if a.meta?.duration_ms}<span class="dur">{duration(a.meta.duration_ms)}</span>{/if}
      </button>
    {/if}
    <button class="expand" aria-label="Open full screen" onclick={() => openMedia(m, a)}><Icon name="expand" size={17} stroke={2} /></button>
  </div>
{:else if media.length > 1}
  <div class="album n{Math.min(media.length, 4)}">
    {#each shownMedia as a, i (a.id)}
      <button class="cell" aria-label={`Open ${a.name}`} onclick={() => openMedia(m, a)}>
        {#if isImage(a.content_type) && !shown(a.id)}
          <span class="noposter"><Icon name="download" size={24} /></span>
        {:else if isImage(a.content_type)}
          <img src={mediaUrl(a.id)} alt={a.name} loading="lazy" decoding="async" />
        {:else if a.meta?.poster_b64}
          <img src={posterUrl(a.meta.poster_b64)} alt="" />
        {:else}
          <span class="noposter"><Icon name="video" size={26} /></span>
        {/if}
        {#if isVideo(a.content_type)}<span class="playbtn sm"><svg width="18" height="18" viewBox="0 0 24 24" fill="#fff"><path d="M8 5.4v13.2a1 1 0 0 0 1.5.86l10.6-6.6a1 1 0 0 0 0-1.72L9.5 4.54A1 1 0 0 0 8 5.4Z" /></svg></span>{/if}
        {#if i === 3 && extra > 0}<span class="more">+{extra}</span>{/if}
      </button>
    {/each}
  </div>
{/if}

{#each voices as a (a.id)}<VoiceBubble {a} voice {mine} />{/each}
{#each audios as a (a.id)}<VoiceBubble {a} voice={false} {mine} />{/each}

{#each docs as a (a.id)}
  {@const tile = fileTile(a.name, a.content_type)}
  {@const pdf = a.content_type === "application/pdf" && !a.removed && !a.id.startsWith("up_")}
  <div class="doc" class:removed={a.removed}>
    {#if pdf && !brokenThumb[a.id]}
      <button class="pdfprev" aria-label={`Preview ${a.name}`} onclick={() => open(a.id, a.name)}>
        <img src={mediaUrl(a.id, true)} alt="" loading="lazy" onerror={() => (brokenThumb[a.id] = true)} />
      </button>
    {/if}
    <div class="drow">
      <button class="dmain" disabled={a.removed || a.id.startsWith("up_")} aria-label={`Open ${a.name}`} title="Open" onclick={() => open(a.id, a.name)}>
        <span class="tile" style="color:{tile.fg};background:{tile.bg}"><Icon name="file" size={20} stroke={1.9} /><b>{tile.label}</b></span>
        <span class="dt"><b>{a.removed ? "File removed" : a.name}</b>{#if !a.removed}<small>{docLine(a.size, a.meta?.pages, fileSize)}</small>{/if}</span>
      </button>
      {#if !a.removed && !a.id.startsWith("up_")}<button class="dl" aria-label={`Download ${a.name}`} title="Save to Downloads" onclick={() => save(a.id, a.name)}><Icon name="download" size={19} stroke={2} /></button>{/if}
    </div>
  </div>
  {#if !a.removed && !a.id.startsWith("up_")}<button class="plink" onclick={() => open(a.id, a.name)}>Preview document</button>{/if}
{/each}

<style>
  .card { position: relative; border-radius: 12px; overflow: hidden; background: rgba(127, 127, 127, 0.18); margin-bottom: 6px; }
  .single { width: min(300px, 100%); max-width: 100%; }
  .fill { position: absolute; inset: 0; width: 100%; height: 100%; padding: 0; display: block; }
  .fill img, video.fill { width: 100%; height: 100%; object-fit: cover; display: block; background: #0c1220; }
  .dlph { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; background: linear-gradient(145deg, #2a3a58, #131c2e); color: #dbe5f7; font-size: 13.5px; font-weight: 600; }
  .noposter { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; color: #c9d4e8; background: linear-gradient(145deg, #2a3a58, #131c2e); }
  .playbtn { position: absolute; left: 50%; top: 50%; width: 52px; height: 52px; margin: -26px 0 0 -26px; border-radius: 50%; background: rgba(12, 18, 32, 0.62); display: inline-flex; align-items: center; justify-content: center; padding-left: 3px; }
  .playbtn.sm { width: 34px; height: 34px; margin: -17px 0 0 -17px; }
  .dur { position: absolute; left: 8px; bottom: 8px; background: rgba(12, 18, 32, 0.66); color: #fff; font-size: 12px; font-weight: 600; padding: 1px 7px; border-radius: 8px; }
  .expand { position: absolute; right: 8px; bottom: 8px; width: 30px; height: 30px; border-radius: 50%; background: rgba(12, 18, 32, 0.55); color: #fff; display: inline-flex; align-items: center; justify-content: center; }
  .expand:hover { background: rgba(12, 18, 32, 0.8); }
  .album { display: grid; gap: 3px; width: min(300px, 100%); margin-bottom: 6px; border-radius: 12px; overflow: hidden; }
  .album.n2 { grid-template-columns: 1fr 1fr; aspect-ratio: 2 / 1; }
  .album.n3 { grid-template-columns: 1fr 1fr; grid-template-rows: 1fr 1fr; aspect-ratio: 1; }
  .album.n3 .cell:first-child { grid-row: span 2; }
  .album.n4 { grid-template-columns: 1fr 1fr; grid-template-rows: 1fr 1fr; aspect-ratio: 1; }
  .cell { position: relative; overflow: hidden; background: #0c1220; padding: 0; min-height: 0; }
  .cell img { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: cover; }
  .more { position: absolute; inset: 0; background: rgba(12, 18, 32, 0.6); color: #fff; font-size: 24px; font-weight: 600; display: flex; align-items: center; justify-content: center; }
  .doc { background: rgba(127, 127, 127, 0.13); border-radius: 12px; margin-bottom: 4px; min-width: 232px; overflow: hidden; }
  .doc.removed { opacity: 0.7; }
  .pdfprev { display: block; width: 100%; height: 118px; overflow: hidden; padding: 0; background: #fff; border-bottom: 1px solid rgba(127, 127, 127, 0.2); }
  .pdfprev img { width: 100%; height: 100%; object-fit: cover; object-position: top; display: block; }
  .drow { display: flex; align-items: center; gap: 11px; padding: 9px 10px; }
  .dmain { flex: 1; min-width: 0; display: flex; align-items: center; gap: 11px; text-align: left; padding: 0; color: inherit; }
  .dmain:not(:disabled):hover .dt b { text-decoration: underline; }
  .tile { width: 42px; height: 46px; border-radius: 9px; display: inline-flex; flex-direction: column; align-items: center; justify-content: center; gap: 0; flex: none; }
  .tile b { font-size: 9.5px; letter-spacing: 0.03em; line-height: 1; margin-top: 1px; }
  .dt { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .dt b { font-size: 14.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 700; }
  .dt small { font-size: 12.5px; opacity: 0.78; }
  .dl { width: 36px; height: 36px; border-radius: 50%; background: var(--surface); color: var(--text); box-shadow: var(--shadow); display: inline-flex; align-items: center; justify-content: center; flex: none; }
  .dl:hover { background: var(--hover); }
  .plink { color: var(--blue); font-size: 12.5px; font-weight: 700; padding: 2px 0 2px 2px; margin-bottom: 3px; }
  :global(.out) .plink { color: #a9c4f5; }
</style>
