<script lang="ts">
  import { onMount } from "svelte";
  import { app, fail } from "../../lib/app.svelte";
  import { core, type ServerCheck } from "../../lib/core";
  import PageHead from "./PageHead.svelte";
  import Row from "./Row.svelte";

  let srv = $state<ServerCheck | null>(null);
  onMount(() => void core.checkServer(app.server).then((s) => (srv = s)).catch(() => {}));
  const connLabel = $derived(app.conn === "online" ? "Connected" : app.conn === "connecting" ? "Connecting…" : "Offline");
</script>

<div class="st-page">
  <PageHead title="Server" />
  <div class="st-body scroll" class:desk={!app.phone}>
    <h3 class="st-cap">Workspace</h3>
    <section class="st-card">
      <Row icon="server" tone="green" title={srv?.name || "WorkChat server"} sub={app.server} value={connLabel} chev={false} />
      <Row icon="info" tone="sky" title="Server version" sub={srv ? `Version ${srv.server_version}` : "…"} chev={false} />
      <Row icon="file" tone="orange" title="Largest file" sub={`${Math.round(app.maxUpload / 1048576)} MB per upload`} chev={false} />
      <Row icon="user" tone="violet" title="Signed in as" sub={`@${app.me?.user.username ?? ""}`} chev={false} />
    </section>
    <div class="st-btns gap"><button class="st-btn" onclick={() => void core.reconnect().catch(fail)}>Reconnect now</button></div>
    {#if app.connDetail && app.conn !== "online"}<p class="st-note err">{app.connDetail}</p>{/if}
    <p class="st-note gap">To use another server, sign out and sign in with that address. http addresses are only accepted on a private network (192.168.x.x, 10.x.x.x, *.local, localhost); everything else needs https.</p>
  </div>
</div>

<style>
  .gap { margin-top: 16px; }
</style>
