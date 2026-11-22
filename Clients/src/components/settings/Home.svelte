<script lang="ts">
  import { app, prefs } from "../../lib/app.svelte";
  import { core } from "../../lib/core";
  import { signOutAction } from "../../lib/actions";
  import { platformLabel } from "../../lib/format";
  import Icon from "../Icon.svelte";
  import ProfileCard from "./ProfileCard.svelte";
  import Row from "./Row.svelte";

  let { notif, storage, sessions }: { notif: string; storage: string; sessions: string } = $props();
  const go = (p: typeof app.spage) => (app.spage = p);
  const accent = $derived(({ navy: "Navy Classic", blue: "Blue", slate: "Slate", deep: "Deep navy" } as Record<string, string>)[prefs.accent]);
  function openFiles() {
    app.tab = "chats";
    app.filter = "files";
  }
</script>

<div class="st-page">
  <header class="st-head">
    <button class="hb" aria-label="Back" onclick={() => (app.tab = "chats")}><Icon name="left" size={24} /></button>
    <h1>Settings<span class="sub" class:off={app.conn !== "online"}>● {app.conn === "online" ? "Online" : "Offline"}</span></h1>
  </header>
  <div class="st-body scroll">
    <ProfileCard />

    <h3 class="st-cap">Account &amp; profile</h3>
    <section class="st-card">
      <Row icon="bookmark" tone="blue" title="Saved Messages" sub="Messages you saved for later" onclick={() => go("saved")} />
      <Row icon="star" tone="yellow" title="Starred &amp; Pinned" sub="Pinned chats and pinned messages" onclick={() => go("starred")} />
      <Row icon="folder" tone="sky" title="Recent Files &amp; Shared Media" sub="Photos, documents and audio" onclick={openFiles} />
    </section>

    <h3 class="st-cap">Settings &amp; configuration</h3>
    <section class="st-card">
      <Row icon="bell" tone="red" title="Notifications and Sounds" value={notif} onclick={() => go("notifications")} />
      <Row icon="shield" tone="green" title="Privacy and Security" value={app.lock.pin_set ? "PIN set" : "Off"} onclick={() => go("privacy")} />
      <Row icon="pie" tone="violet" title="Data and Storage" value={storage} onclick={() => go("storage")} />
      <Row icon="brush" tone="navy" title="Chat Settings &amp; Theme" onclick={() => go("appearance")}><span class="st-chip">{accent}</span></Row>
      <Row icon="sliders" tone="navy" title="User Preferences" sub="Return key, receipts, downloads" value="Configure" onclick={() => go("prefs")} />
    </section>

    <h3 class="st-cap">Workspace</h3>
    <section class="st-card">
      <Row icon="server" tone="green" title="Server" sub={app.server} onclick={() => go("server")} />
      <Row icon="laptop" tone="violet" title="Devices &amp; Sessions" value={sessions} onclick={() => go("devices")} />
      <Row icon="help" tone="orange" title="Help &amp; Support" sub="github.com/uri-travoski/workchat-claude" onclick={() => void core.openUrl("https://github.com/uri-travoski/workchat-claude").catch(() => {})} />
      <Row icon="info" tone="sky" title="About" sub="Version {app.info.version}" badge={app.upd.found ? "1" : ""} onclick={() => go("about")} />
    </section>

    <button class="logout" onclick={signOutAction}>
      <span class="st-ic red"><Icon name="logout" size={19} /></span>
      <span class="st-tx"><b>Log Out</b><small>End session on this device</small></span>
      <Icon name="right" size={18} />
    </button>
    <p class="foot">WorkChat for {platformLabel(app.info.platform)} • Version {app.info.version}</p>
  </div>
</div>

<style>
  .hb { width: 40px; height: 40px; margin: -4px 0 -4px -8px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; flex: none; }
  .hb:hover { background: var(--hover); }
  .logout { margin-top: 20px; display: flex; align-items: center; gap: 14px; padding: 13px 16px; width: 100%; text-align: left; background: var(--surface); border: 1px solid #f1caca; border-radius: 16px; color: var(--danger); }
  .logout b { color: var(--danger); font-size: 15.5px; }
  .logout small { color: var(--muted); font-size: 13px; }
  .foot { text-align: center; color: var(--muted); font-size: 13px; margin: 18px 0 0; }
</style>
