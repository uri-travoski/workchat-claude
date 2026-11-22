<script lang="ts">
  import { onMount } from "svelte";
  import { app, fail, prefs, savePrefs, toast } from "../../lib/app.svelte";
  import { core, type StorageInfo } from "../../lib/core";
  import { fileSize } from "../../lib/format";
  import Toggle from "../Toggle.svelte";
  import PageHead from "./PageHead.svelte";
  import Row from "./Row.svelte";

  let info = $state<StorageInfo | null>(null);
  let busy = $state("");
  const load = () => core.storageInfo().then((i) => (info = i)).catch(fail);
  onMount(load);

  async function clear() {
    if (!window.confirm("Clear the cache? Downloaded photos, videos and files are removed from this device. Messages stay, and files download again when you open them.")) return;
    busy = "clear";
    try {
      const before = info?.total ?? 0;
      info = await core.clearCache();
      toast(`Cache cleared. Freed ${fileSize(Math.max(0, before - info.total))}.`);
    } catch (e) {
      fail(e);
    } finally {
      busy = "";
    }
  }
  async function compact() {
    busy = "compact";
    try {
      const before = info?.database ?? 0;
      info = await core.compactDb();
      toast(`Database compacted: ${fileSize(before)} to ${fileSize(info.database)}.`);
    } catch (e) {
      fail(e);
    } finally {
      busy = "";
    }
  }
  const pct = (n: number) => (info && info.total ? Math.max(2, Math.round((n / info.total) * 100)) : 0);
</script>

<div class="st-page">
  <PageHead title="Data and Storage" />
  <div class="st-body scroll" class:desk={!app.phone}>
    <h3 class="st-cap">Database &amp; cache</h3>
    <section class="st-card">
      <div class="st-blk">
        <div class="st-lrow"><b class="st-lbl">Used on this device</b><span class="st-val">{info ? fileSize(info.total) : "…"}</span></div>
        {#if info}
          <div class="st-bar"><i style="width:{Math.min(100, pct(info.media + info.other))}%"></i></div>
          <p class="st-note">Database (messages): <b>{fileSize(info.database)}</b><br />Downloaded media: <b>{fileSize(info.media)}</b> in {info.files} files<br />Temporary copies: <b>{fileSize(info.other)}</b></p>
        {/if}
        <div class="st-btns">
          <button class="st-btn" disabled={!!busy} onclick={compact}>Compact Database</button>
          <button class="st-btn danger" disabled={!!busy} onclick={clear}>Clear Cache</button>
        </div>
        <p class="st-note">Compact rewrites the local database without free space. Clearing the cache removes downloaded files only; your messages and sign-in stay.</p>
      </div>
    </section>

    <h3 class="st-cap">Downloads</h3>
    <section class="st-card">
      <Row icon="image" tone="blue" title="Auto-Download Photos" sub={prefs.autoPhotos ? "Photos in chats load by themselves" : "Photos show a download button first"} chev={false}><Toggle bind:checked={prefs.autoPhotos} label="Auto-download photos" onchange={savePrefs} /></Row>
      <div class="st-blk"><p class="st-note">Videos and files always download when you play or open them. Saved files go to {app.info.platform === "android" ? "the Downloads folder" : "your Downloads folder"}.</p></div>
    </section>
    {#if app.info.data_dir}<p class="st-note path">Data folder: {app.info.data_dir}{app.info.portable ? " (portable mode)" : ""}</p>{/if}
  </div>
</div>

<style>
  .path { margin: 14px 6px 0; word-break: break-all; }
</style>
