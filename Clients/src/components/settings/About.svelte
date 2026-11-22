<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../../lib/app.svelte";
  import { core, type ServerCheck } from "../../lib/core";
  import { fileSize, platformLabel } from "../../lib/format";
  import { checkUpdateNow, runUpdate } from "../../lib/updates";
  import Icon from "../Icon.svelte";
  import PageHead from "./PageHead.svelte";
  import Row from "./Row.svelte";

  let srv = $state<ServerCheck | null>(null);
  let taps = $state(0);
  let dev = $state(false);
  let custom = $state("");
  onMount(() => {
    void core.checkServer(app.server).then((s) => (srv = s)).catch(() => {});
    void core.updateState().then((u) => (custom = u.custom_api ? u.api : "")).catch(() => {});
  });
  const info = $derived(app.upd.info);
  const u = $derived(app.upd);
  const busy = $derived(u.state === "downloading");
  const pct = $derived(u.total ? Math.min(100, Math.round((u.got / u.total) * 100)) : 0);
  const verb = $derived(!info ? "" : info.action === "install" ? "Download and install" : info.action === "installer" ? "Download and run installer" : info.action === "portable" ? "Download and restart" : info.action === "appimage" ? "Download and restart" : info.action === "package" ? "Download and open" : "Open release page");

  function tapVersion() {
    taps += 1;
    if (taps >= 7) dev = true;
  }
  async function saveApi() {
    await core.setUpdateApi(custom.trim());
    await checkUpdateNow();
  }
</script>

<div class="st-page">
  <PageHead title="About" />
  <div class="st-body scroll" class:desk={!app.phone}>
    <section class="st-card">
      <button class="st-row" onclick={tapVersion}>
        <span class="st-ic navy"><img src="/app-icon.svg" alt="" width="38" height="38" /></span>
        <span class="st-tx"><b>WorkChat for {platformLabel(app.info.platform)}</b><small>Version {app.info.version}{app.info.portable ? " (portable)" : ""} · {app.info.arch}</small></span>
      </button>
      <Row icon="server" tone="green" title="Server version" sub={srv ? srv.server_version : "…"} chev={false} />
    </section>

    <h3 class="st-cap">Updates</h3>
    <section class="st-card">
      <div class="st-blk">
        <div class="st-btns">
          <button class="st-btn pri" disabled={u.checking || busy} onclick={checkUpdateNow}><Icon name="refresh" size={16} /> {u.checking ? "Checking…" : "Check for updates"}</button>
        </div>
        {#if u.error}<p class="st-note err">{u.error}</p>{/if}
        {#if info && !info.newer}
          <p class="up"><Icon name="check" size={18} stroke={2.4} /> You're up to date <small>(version {info.current})</small></p>
        {:else if info}
          <p class="up new"><Icon name="arrowup" size={18} stroke={2.4} /> Version {info.latest} available <small>(you have {info.current})</small></p>
          {#if info.notes}<div class="st-notes">{info.notes}</div>{/if}
          {#if info.asset}<p class="st-note">{info.asset.name}{info.asset.size ? ` · ${fileSize(info.asset.size)}` : ""}</p>{:else if info.action === "page"}<p class="st-note">There is no installer for this device in the release. The release page has the downloads.</p>{/if}
          {#if busy}
            <div class="st-bar" role="progressbar" aria-valuenow={pct}><i style="width:{pct}%"></i></div>
            <p class="st-note">Downloading… {fileSize(u.got)}{u.total ? ` of ${fileSize(u.total)} (${pct}%)` : ""}</p>
            <button class="st-btn" onclick={() => void core.cancelUpdate()}>Cancel</button>
          {:else}
            <div class="st-btns">
              <button class="st-btn pri" onclick={runUpdate}><Icon name={info.action === "page" ? "external" : "download"} size={16} /> {u.state === "needs_permission" ? "Install" : verb}</button>
              {#if info.action !== "page"}<button class="st-btn" onclick={() => void core.openUrl(info.page)}><Icon name="external" size={15} /> Release page</button>{/if}
            </div>
          {/if}
          {#if u.state === "needs_permission"}<p class="st-note err">{u.message || "Allow WorkChat to install apps, then tap Install again."}</p>
          {:else if u.state === "ready" || u.state === "installing"}<p class="st-note">{u.message}</p>
          {:else if u.state === "error"}<p class="st-note err">{u.message}</p>{/if}
        {:else if app.upd.found && !u.checking}
          <p class="up new"><Icon name="arrowup" size={18} stroke={2.4} /> Version {app.upd.found} available</p>
        {:else if !u.error}
          <p class="st-note">WorkChat checks GitHub for new releases about once a day, and when you tap the button.</p>
        {/if}
      </div>
    </section>

    {#if dev}
      <h3 class="st-cap">Developer</h3>
      <section class="st-card"><div class="st-blk">
        <label class="st-field"><span>Update server (empty: GitHub)</span><input class="st-input" placeholder="http://10.0.2.2:8099/latest.json" bind:value={custom} /></label>
        <button class="st-btn" onclick={saveApi}>Save and check</button>
      </div></section>
    {/if}

    <h3 class="st-cap">Help</h3>
    <section class="st-card">
      <Row icon="help" tone="orange" title="Help &amp; Support" sub="github.com/uri-travoski/workchat-claude" onclick={() => void core.openUrl("https://github.com/uri-travoski/workchat-claude")} />
      <Row icon="file" tone="sky" title="Release notes" sub="All versions on GitHub" onclick={() => void core.openUrl("https://github.com/uri-travoski/workchat-claude/releases")} />
    </section>
    <p class="foot">WorkChat for {platformLabel(app.info.platform)} • Version {app.info.version}</p>
  </div>
</div>

<style>
  .up { margin: 0; display: flex; align-items: center; gap: 8px; font-weight: 700; color: #0c8f67; font-size: 15.5px; flex-wrap: wrap; }
  .up small { color: var(--muted); font-weight: 500; }
  .up.new { color: var(--blue); }
  .st-ic img { border-radius: 11px; }
  .foot { text-align: center; color: var(--muted); font-size: 13px; margin: 18px 0 0; }
</style>
