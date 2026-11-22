<script lang="ts">
  import { app } from "../../lib/app.svelte";
  import About from "./About.svelte";
  import Account from "./Account.svelte";
  import Appearance from "./Appearance.svelte";
  import Devices from "./Devices.svelte";
  import Notifications from "./Notifications.svelte";
  import Prefs from "./Prefs.svelte";
  import Privacy from "./Privacy.svelte";
  import ProfileCard from "./ProfileCard.svelte";
  import Server from "./Server.svelte";
  import Storage from "./Storage.svelte";
  import PageHead from "./PageHead.svelte";

  const page = $derived(app.spage || "profile");
</script>

{#if page === "profile"}
  <div class="st-page">
    <PageHead title="Profile" />
    <div class="st-body scroll" class:desk={!app.phone}><div class="w"><ProfileCard /></div></div>
  </div>
{:else if page === "account" || page === "saved"}<Account kind="saved" />
{:else if page === "starred"}<Account kind="starred" />
{:else if page === "notifications"}<Notifications />
{:else if page === "privacy"}<Privacy />
{:else if page === "storage"}<Storage />
{:else if page === "appearance"}<Appearance />
{:else if page === "prefs"}<Prefs />
{:else if page === "server"}<Server />
{:else if page === "devices"}<Devices />
{:else if page === "about"}<About />
{/if}

<style>
  .w { max-width: 460px; }
</style>
