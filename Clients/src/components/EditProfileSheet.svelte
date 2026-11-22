<script lang="ts">
  import { app, closeSheet, fail, toast } from "../lib/app.svelte";
  import { core } from "../lib/core";
  import { initialsOf } from "../lib/people";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import Sheet from "./Sheet.svelte";

  const me = app.me!.user;
  let name = $state(me.display_name);
  let title = $state(me.title ?? "");
  let phone = $state(me.phone ?? "");
  let busy = $state(false);
  const changed = $derived(name.trim() !== me.display_name || title.trim() !== (me.title ?? "") || phone.trim() !== (me.phone ?? ""));

  async function save() {
    if (!name.trim()) return fail({ message: "Your name cannot be empty." });
    busy = true;
    try {
      await core.updateProfile({ display_name: name.trim(), title: title.trim(), phone: phone.trim() });
      toast("Profile saved.");
      closeSheet();
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }
  async function picture() {
    try {
      const [st] = await core.pickFiles("image");
      if (st) app.sheet = { kind: "crop", stagedId: st.id, target: { kind: "me" } };
    } catch (e) {
      fail(e);
    }
  }
  async function removePicture() {
    try {
      await core.removeAvatar("me", null);
    } catch (e) {
      fail(e);
    }
  }
</script>

<Sheet title="Edit Profile" onclose={closeSheet}>
  <div class="av">
    <Avatar id={app.me?.user.avatar ?? null} initials={initialsOf(name || me.display_name)} size={76} />
    <div class="ab">
      <button class="st-btn" onclick={picture}><Icon name="camera" size={16} /> Change picture</button>
      {#if app.me?.user.avatar}<button class="st-btn" onclick={removePicture}>Remove</button>{/if}
    </div>
  </div>
  <label class="st-field"><span>Name</span><input class="st-input" bind:value={name} maxlength="80" autocomplete="name" /></label>
  <label class="st-field"><span>Title</span><input class="st-input" bind:value={title} maxlength="80" placeholder="Head of Operations" /></label>
  <label class="st-field"><span>Phone</span><input class="st-input" bind:value={phone} maxlength="40" inputmode="tel" placeholder="+1 555 0100" /></label>
  <p class="st-note">Your username <b>@{me.username}</b> is set by your administrator. Name, title, phone and picture are visible to everyone in the workspace.</p>
  {#snippet footer()}<button class="st-btn" onclick={closeSheet}>Cancel</button><button class="st-btn pri" disabled={!changed || busy} onclick={save}>Save</button>{/snippet}
</Sheet>

<style>
  .av { display: flex; align-items: center; gap: 14px; margin-bottom: 14px; }
  .ab { display: flex; gap: 8px; flex-wrap: wrap; }
  :global(.st-field) { margin-bottom: 12px; }
</style>
