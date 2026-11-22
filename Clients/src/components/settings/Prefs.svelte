<script lang="ts">
  import { app, fail, prefs, savePrefs } from "../../lib/app.svelte";
  import { core } from "../../lib/core";
  import Icon from "../Icon.svelte";
  import Toggle from "../Toggle.svelte";
  import PageHead from "./PageHead.svelte";
  import Row from "./Row.svelte";

  let receipts = $state(true);
  $effect(() => {
    receipts = app.me?.read_receipts ?? true;
  });
  async function setReceipts() {
    try {
      await core.updateProfile({ read_receipts: receipts });
    } catch (e) {
      receipts = !receipts;
      fail(e);
    }
  }
</script>

<div class="st-page">
  <PageHead title="User Preferences" />
  <div class="st-body scroll" class:desk={!app.phone}>
    <section class="st-card hero">
      <span class="st-ic blue big"><Icon name="sliders" size={26} /></span>
      <span class="st-tx"><b>Chat Ergonomics</b><small>Tailor your high-throughput work interface</small></span>
    </section>
    <h3 class="st-cap">Messaging &amp; behavior</h3>
    <section class="st-card">
      <Row icon="send" tone="" title="Send with Return Key" sub="Press Enter to dispatch messages. On desktop Enter always sends; Shift+Enter adds a line." chev={false}><Toggle bind:checked={prefs.sendWithReturn} label="Send with Return Key" onchange={savePrefs} /></Row>
      <Row icon="checks" tone="" title="Read Receipts" sub="Show double checkmarks when read" chev={false}><Toggle bind:checked={receipts} label="Read receipts" onchange={setReceipts} /></Row>
      <Row icon="eye" tone="" title="Online &amp; Last Seen Status" value="Everyone" onclick={() => (app.spage = "privacy")} />
      <Row icon="download" tone="" title="Auto-Download Media" value={prefs.autoPhotos ? "Photos" : "Off"} onclick={() => (app.spage = "storage")} />
    </section>
  </div>
</div>

<style>
  .hero { display: flex; align-items: center; gap: 14px; padding: 16px; margin-top: 4px; }
  .big { width: 52px; height: 52px; border-radius: 50%; }
</style>
