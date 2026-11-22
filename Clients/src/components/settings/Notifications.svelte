<script lang="ts">
  import { onMount } from "svelte";
  import { app, fail, toast } from "../../lib/app.svelte";
  import { core, type NotifySettings, type NotifyStatus } from "../../lib/core";
  import { timeAgo } from "../../lib/format";
  import { syncPush } from "../../lib/push";
  import Icon from "../Icon.svelte";
  import Toggle from "../Toggle.svelte";
  import PageHead from "./PageHead.svelte";
  import Row from "./Row.svelte";

  let s = $state<NotifySettings | null>(null);
  let st = $state<NotifyStatus | null>(null);
  const os = $derived(app.info.platform);
  const mobile = $derived(os === "android" || os === "ios");

  async function load() {
    try {
      [s, st] = await Promise.all([core.getNotifySettings(), core.notifyStatus()]);
    } catch (e) {
      fail(e);
    }
  }
  onMount(load);
  async function save() {
    if (!s) return;
    try {
      await core.setNotifySettings($state.snapshot(s) as NotifySettings);
      st = await core.notifyStatus();
    } catch (e) {
      fail(e);
    }
  }
  async function allow() {
    try {
      await core.notifyRequestPermission();
      await load();
    } catch (e) {
      fail(e);
    }
  }
  async function test() {
    try {
      await core.notifyTest();
      toast("Test notification sent.");
    } catch (e) {
      fail(e);
    }
  }
  async function register() {
    try {
      await core.pushRegister();
      await load();
      toast("Push registered.");
    } catch (e) {
      await load();
      fail(e);
    }
  }
  async function unregister() {
    try {
      await core.pushUnregister();
      await load();
    } catch (e) {
      fail(e);
    }
  }
  const methodLabel = $derived(
    !st ? "" : st.method === "os" ? "System notifications (this app shows them while it is running)" : st.method === "unifiedpush" ? `UnifiedPush via ${st.distributor || "your distributor"}` : st.method === "keepalive" ? "Connection kept open by a background service" : st.method === "fcm" ? "Google (FCM)" : st.method === "apns" ? "Apple (APNs)" : "Not receiving notifications while the app is closed",
  );
  const stateLabel = $derived(!st ? "" : st.permission === "granted" ? "Allowed" : st.permission === "denied" ? "Blocked in system settings" : "Not asked yet");
</script>

<div class="st-page">
  <PageHead title="Notifications and Sounds" />
  <div class="st-body scroll" class:desk={!app.phone}>
    {#if s && st}
      <h3 class="st-cap">Status</h3>
      <section class="st-card">
        <Row icon="bell" tone="red" title="Notifications" sub={methodLabel} value={st.method === "none" ? "" : stateLabel} chev={false} />
        <div class="st-blk">
          <div class="st-btns">
            {#if st.permission !== "granted" && os !== "linux"}<button class="st-btn pri" onclick={allow}>Allow notifications</button>{/if}
            <button class="st-btn" onclick={test}><Icon name="bell" size={16} /> Send test notification</button>
          </div>
          {#if st.permission === "denied"}<p class="st-note err">Notifications are blocked for WorkChat. Turn them on in the {os === "android" ? "phone's app settings" : "system settings"}.</p>{/if}
          {#if os === "linux"}<p class="st-note">Desktop notifications use your system's notification service. Clicking one opens that chat.</p>{/if}
        </div>
        <Row icon="bell" tone="blue" title="Notifications on this device" chev={false}><Toggle bind:checked={s.enabled} label="Notifications on this device" onchange={save} /></Row>
      </section>

      <h3 class="st-cap">What notifies you</h3>
      <section class="st-card">
        <Row icon="user" tone="blue" title="Direct messages" sub="One to one and group chats" chev={false}><Toggle bind:checked={s.dms} label="Direct messages" onchange={save} /></Row>
        <Row icon="hash" tone="violet" title="Channels" sub="Messages in channels" chev={false}><Toggle bind:checked={s.channels} label="Channels" onchange={save} /></Row>
        <Row icon="thread" tone="sky" title="Thread replies" sub="Replies in threads you follow" chev={false}><Toggle bind:checked={s.threads} label="Thread replies" onchange={save} /></Row>
        <Row icon="at" tone="orange" title="Mentions only in channels" sub="Direct messages still notify" chev={false}><Toggle bind:checked={s.mentions_only} label="Mentions only in channels" onchange={save} /></Row>
        <Row icon="at" tone="red" title="Direct Mentions Alert" sub="@mentions always get through, even when a type is off" chev={false}><Toggle bind:checked={s.mention_alert} label="Direct mentions alert" onchange={save} /></Row>
        <Row icon="eye" tone="green" title="Show message preview" sub="Off shows only &quot;New message&quot;. Locked chats never show a preview." chev={false}><Toggle bind:checked={s.show_preview} label="Show message preview" onchange={save} /></Row>
      </section>

      <h3 class="st-cap">Sounds</h3>
      <section class="st-card">
        <div class="st-blk">
          <b class="st-lbl">Notification sound</b>
          <div class="st-seg" role="radiogroup" aria-label="Notification sound">
            {#each [["default", "WorkChat Chime"], ["gentle", "Gentle"], ["none", "None"]] as [id, label]}
              <button role="radio" aria-checked={s.sound === id} class:on={s.sound === id} onclick={() => { s!.sound = id as NotifySettings["sound"]; void save(); }}>{label}</button>
            {/each}
          </div>
        </div>
        <Row icon="volumeoff" tone="" title="Channel Broadcasts Sound" sub={s.channel_sound ? "Channels play the sound" : "Silent notification delivery"} chev={false}><Toggle bind:checked={s.channel_sound} label="Channel broadcasts sound" onchange={save} /></Row>
        {#if mobile}<Row icon="vibrate" tone="" title="Vibration" sub="Haptic response for new messages" chev={false}><Toggle bind:checked={s.vibration} label="Vibration" onchange={save} /></Row>{/if}
      </section>

      <h3 class="st-cap">Do Not Disturb</h3>
      <section class="st-card">
        <Row icon="moon" tone="" title="Do Not Disturb Schedule" sub={s.dnd ? `Every day ${s.dnd_from} to ${s.dnd_to}` : "Off"} chev={false}><Toggle bind:checked={s.dnd} label="Do not disturb schedule" onchange={save} /></Row>
        {#if s.dnd}
          <div class="st-blk times">
            <label class="st-field"><span>From</span><input class="st-input" type="time" bind:value={s.dnd_from} onchange={save} /></label>
            <label class="st-field"><span>Until</span><input class="st-input" type="time" bind:value={s.dnd_to} onchange={save} /></label>
          </div>
        {/if}
      </section>

      {#if !mobile}
        <h3 class="st-cap">Tray</h3>
        <section class="st-card">
          <Row icon="laptop" tone="" title="Close to tray" sub={st.tray ? "Closing the window keeps WorkChat running for notifications" : "No tray is available on this desktop"} chev={false}><Toggle bind:checked={s.close_to_tray} label="Close to tray" onchange={save} /></Row>
        </section>
      {:else if os === "android"}
        <h3 class="st-cap">Push (while the app is closed)</h3>
        <section class="st-card">
          {#if st.distributors.length}
            <Row icon="server" tone="green" title="UnifiedPush" sub={st.push.kind === "unifiedpush" && !st.push.error ? `Registered with ${st.push.target_host || "the server"}, ${timeAgo(st.push.registered_ms)}` : st.push.error || `Distributor: ${st.distributor || st.distributors[0]}`} chev={false} />
            <div class="st-blk">
              <div class="st-btns">
                <button class="st-btn pri" onclick={register}>{st.push.kind === "unifiedpush" ? "Register again" : "Register for push"}</button>
                {#if st.push.kind === "unifiedpush"}<button class="st-btn" onclick={unregister}>Remove</button>{/if}
              </div>
              <p class="st-note">Push wake-ups carry no message text. WorkChat fetches the messages itself and shows the notification.</p>
            </div>
          {:else}
            <div class="st-blk">
              <b class="st-lbl">No push app installed</b>
              <p class="st-note">To get notifications when WorkChat is closed, install a UnifiedPush distributor such as the <b>ntfy</b> app (F-Droid or Google Play), open it and set it to your server's ntfy address, then come back and tap Register.</p>
              <div class="st-btns">
                <button class="st-btn" onclick={() => void core.openUrl("https://docs.ntfy.sh/subscribe/phone/")}><Icon name="external" size={15} /> How to install ntfy</button>
                <button class="st-btn" onclick={load}><Icon name="refresh" size={15} /> Check again</button>
              </div>
            </div>
          {/if}
          <Row icon="wifi" tone="orange" title="Keep connection alive" sub="Fallback without a push app: a notification stays in the status bar and WorkChat keeps listening" chev={false}><Toggle bind:checked={s.keep_alive} label="Keep connection alive" onchange={save} /></Row>
          {#if st.fcm}<Row icon="bell" tone="blue" title="Google push (FCM)" sub="Built into this version" chev={false} />{/if}
        </section>
      {:else}
        <h3 class="st-cap">Push (while the app is closed)</h3>
        <section class="st-card">
          <Row icon="bell" tone="blue" title="Apple push (APNs)" sub={st.push.kind === "apns" && !st.push.error ? `Registered ${timeAgo(st.push.registered_ms)}` : st.push.error || "Not registered. Needs a build signed with a developer account."} chev={false} />
          <div class="st-blk"><button class="st-btn pri" onclick={register}>Register for push</button></div>
        </section>
      {/if}
    {:else}
      <p class="st-note">Loading…</p>
    {/if}
  </div>
</div>

<style>
  .times { flex-direction: row; gap: 12px; }
  .times label { flex: 1; }
</style>
