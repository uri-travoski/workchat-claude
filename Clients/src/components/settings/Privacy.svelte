<script lang="ts">
  import { app, fail, refreshLock, toast } from "../../lib/app.svelte";
  import { core } from "../../lib/core";
  import { changePin, lockNow, openLockedChats } from "../../lib/lock";
  import Toggle from "../Toggle.svelte";
  import PageHead from "./PageHead.svelte";
  import Row from "./Row.svelte";

  const me = $derived(app.me);
  let receipts = $state(true);
  let showSeen = $state(true);
  $effect(() => {
    receipts = me?.read_receipts ?? true;
    showSeen = !(me?.hide_last_active ?? false);
  });
  const timeouts: [number, string][] = [[0, "Never"], [60, "1 minute"], [300, "5 minutes"], [900, "15 minutes"], [3600, "1 hour"]];

  async function setReceipts() {
    try {
      await core.updateProfile({ read_receipts: receipts });
    } catch (e) {
      receipts = !receipts;
      fail(e);
    }
  }
  async function setSeen() {
    try {
      await core.updateProfile({ hide_last_active: !showSeen });
    } catch (e) {
      showSeen = !showSeen;
      fail(e);
    }
  }
  async function setTimeout_(secs: number) {
    try {
      await core.lockSetTimeout(secs, app.lock.on_background);
      await refreshLock();
    } catch (e) {
      fail(e);
    }
  }
  async function setBg() {
    try {
      await core.lockSetTimeout(app.lock.timeout_secs, app.lock.on_background);
      await refreshLock();
    } catch (e) {
      fail(e);
    }
  }
  function setPin() {
    app.pad = { mode: "set", title: "Create a PIN", then: () => toast("PIN set. Use Lock on a chat's menu to hide chats.") };
  }
</script>

<div class="st-page">
  <PageHead title="Privacy and Security" />
  <div class="st-body scroll" class:desk={!app.phone}>
    <h3 class="st-cap">Chat lock</h3>
    <section class="st-card">
      <Row icon="lock" tone="green" title="Locked chats" sub={app.lock.open ? "Showing now" : "Hidden until you enter your PIN"} onclick={openLockedChats} />
      {#if app.lock.pin_set}
        <Row icon="key" tone="blue" title="Change PIN" sub="6 digits, stored only on this device" onclick={changePin} />
      {:else}
        <Row icon="key" tone="blue" title="Set a PIN" sub="Needed to lock chats" onclick={setPin} />
      {/if}
      {#if app.lock.pin_set}
        <div class="st-blk">
          <div class="st-lrow"><b class="st-lbl">Lock again after</b><span class="st-val">{timeouts.find((t) => t[0] === app.lock.timeout_secs)?.[1] ?? `${Math.round(app.lock.timeout_secs / 60)} min`} without use</span></div>
          <div class="st-seg" role="radiogroup" aria-label="Lock timeout">
            {#each timeouts as [secs, label]}
              <button role="radio" aria-checked={app.lock.timeout_secs === secs} class:on={app.lock.timeout_secs === secs} onclick={() => setTimeout_(secs)}>{label}</button>
            {/each}
          </div>
        </div>
        <Row icon="smartphone" tone="" title="Lock when the app is in the background" sub="Also when the window is minimised" chev={false}><Toggle bind:checked={app.lock.on_background} label="Lock in background" onchange={setBg} /></Row>
        {#if app.lock.open}
          <div class="st-blk"><button class="st-btn pri" onclick={lockNow}><span>Lock now</span></button></div>
        {/if}
      {/if}
      <div class="st-blk"><p class="st-note">Locked chats are hidden from chat lists, counts, search, files and notification previews. The PIN is saved as a salted hash on this device and is never sent anywhere. If you forget it, the only way out is to sign out, which erases the local data (your chats stay on the server).</p></div>
    </section>

    <h3 class="st-cap">Who sees what</h3>
    <section class="st-card">
      <Row icon="checks" tone="sky" title="Read Receipts" sub="Show double checkmarks when read" chev={false}><Toggle bind:checked={receipts} label="Read receipts" onchange={setReceipts} /></Row>
      {#if me?.privacy_controls_enabled}
        <Row icon="eye" tone="violet" title="Online &amp; Last Seen Status" sub={showSeen ? "Everyone can see when you were last active" : "Your last active time is hidden"} chev={false}><Toggle bind:checked={showSeen} label="Show last seen" onchange={setSeen} /></Row>
      {:else}
        <Row icon="eye" tone="violet" title="Online &amp; Last Seen Status" sub="Everyone. Your administrator controls this setting." value="Everyone" chev={false} />
      {/if}
    </section>
  </div>
</div>
