<script lang="ts">
  import { onMount } from "svelte";
  import { app, fail, toast } from "../../lib/app.svelte";
  import { core, type SessionItem } from "../../lib/core";
  import { timeAgo } from "../../lib/format";
  import PageHead from "./PageHead.svelte";
  import Row from "./Row.svelte";

  let list = $state<SessionItem[] | null>(null);
  let err = $state("");
  async function load() {
    try {
      list = await core.listSessions();
      err = "";
    } catch (e) {
      err = (e as { message?: string }).message ?? "Could not load the sessions.";
    }
  }
  onMount(load);
  async function revoke(s: SessionItem) {
    if (!window.confirm(`Sign out "${s.device_name}"? It will need to sign in again.`)) return;
    try {
      await core.revokeSession(s.id);
      toast("Device signed out.");
      await load();
    } catch (e) {
      fail(e);
    }
  }
  async function revokeOthers() {
    if (!window.confirm("Sign out every other device?")) return;
    try {
      await core.revokeOtherSessions();
      toast("Other devices signed out.");
      await load();
    } catch (e) {
      fail(e);
    }
  }
  const icon = (n: string) => (/android|ios|iphone|phone/i.test(n) ? "smartphone" : "laptop");
  // Plain members may sign devices out only when the administrator allows it (Privacy options); everyone sees the list.
  const canRevoke = $derived(app.me?.privacy_controls_enabled ?? false);
  const others = $derived((list ?? []).filter((s) => !s.current));
</script>

<div class="st-page">
  <PageHead title="Devices &amp; Sessions" />
  <div class="st-body scroll" class:desk={!app.phone}>
    {#if err}<p class="st-note err">{err}</p>{/if}
    <section class="st-card">
      {#each list ?? [] as s (s.id)}
        <Row icon={icon(s.device_name)} tone={s.current ? "green" : "violet"} title={s.device_name || "Unknown device"} sub={`${s.current ? "This device · " : ""}${s.client_version ? "v" + s.client_version + " · " : ""}active ${timeAgo(s.last_seen_ms)}${s.ip ? " · " + s.ip : ""}`} chev={false}>
          {#if !s.current && canRevoke}<button class="st-btn danger sm" onclick={() => revoke(s)}>Sign out</button>{/if}
        </Row>
      {:else}
        <div class="st-blk"><p class="st-note">{list ? "No sessions." : "Loading…"}</p></div>
      {/each}
    </section>
    {#if others.length && canRevoke}<div class="st-btns gap"><button class="st-btn danger" onclick={revokeOthers}>Sign out all other devices</button></div>{/if}
    <p class="st-note gap">Each place you are signed in is a session.{canRevoke ? " Signing one out takes effect at once." : " Your administrator controls signing devices out."}</p>
  </div>
</div>

<style>
  .gap { margin-top: 16px; }
  .sm { padding: 6px 12px; font-size: 13.5px; }
</style>
