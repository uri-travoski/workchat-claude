<script lang="ts">
  import { app, fail } from "../../lib/app.svelte";
  import { core } from "../../lib/core";
  import { initialsOf } from "../../lib/people";
  import Avatar from "../Avatar.svelte";
  import Icon from "../Icon.svelte";

  const me = $derived(app.me?.user);
  async function changePicture() {
    try {
      const [st] = await core.pickFiles("image");
      if (st) app.sheet = { kind: "crop", stagedId: st.id, target: { kind: "me" } };
    } catch (e) {
      fail(e);
    }
  }
</script>

{#if me}
  <section class="card">
    <button class="av" aria-label="Change your picture" title="Change your picture" onclick={changePicture}>
      <Avatar id={me.avatar ?? null} initials={initialsOf(me.display_name)} size={84} presence={app.conn === "online" ? "online" : null} /><span class="cam"><Icon name="camera" size={15} /></span>
    </button>
    <h2>{me.display_name}</h2>
    <span class="st-pill" class:off={app.conn !== "online"}><i></i>{app.conn === "online" ? "Online" : app.conn === "connecting" ? "Connecting" : "Offline"}</span>
    <p class="role">{me.title || (me.role === "owner" ? "Owner" : me.role === "admin" ? "Admin" : "Member")}</p>
    {#if me.phone}<p class="line"><Icon name="phone" size={15} /> {me.phone}</p>{/if}
    <p class="line"><Icon name="at" size={15} /> {me.username}</p>
    <div class="btns">
      <button class="st-btn pri" onclick={() => (app.sheet = { kind: "editprofile" })}><Icon name="pencil" size={16} /> Edit Profile</button>
      <button class="st-btn" onclick={() => (app.sheet = { kind: "sharecard" })}><Icon name="qr" size={16} /> Share Card</button>
    </div>
  </section>
{/if}

<style>
  .card { background: var(--surface); border-radius: 20px; padding: 22px 16px 18px; display: flex; flex-direction: column; align-items: center; box-shadow: var(--shadow); border: 1px solid var(--line); }
  .av { position: relative; border-radius: 50%; }
  .cam { position: absolute; right: -2px; bottom: -2px; width: 28px; height: 28px; border-radius: 50%; background: var(--accent); color: #fff; border: 2px solid var(--surface); display: inline-flex; align-items: center; justify-content: center; }
  h2 { margin: 12px 0 6px; font-size: 20px; }
  .role { margin: 8px 0 0; color: var(--blue); font-weight: 600; font-size: 16px; text-align: center; }
  .line { margin: 5px 0 0; color: var(--text-2); display: inline-flex; align-items: center; gap: 7px; font-size: 14.5px; }
  .btns { display: flex; gap: 10px; margin-top: 16px; }
</style>
