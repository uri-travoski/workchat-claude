<script lang="ts">
  import { app, closeSheet, fail, startDm, userAvatar } from "../lib/app.svelte";
  import { core } from "../lib/core";
  import { lastSeen } from "../lib/format";
  import { initialsOf } from "../lib/people";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import Sheet from "./Sheet.svelte";

  let { userId }: { userId: string } = $props();
  const u = $derived(app.users[userId]);
  const mine = $derived(userId === app.me?.user.id);
  const hasLocal = $derived(!!app.localAvatars[`user:${userId}`]);
  const status = $derived(!u ? "" : u.presence === "online" ? "Active now" : u.presence === "away" ? "Away" : u.presence === "dnd" ? "Do not disturb" : u.last_active_ms ? lastSeen(u.last_active_ms) : "Offline");

  async function setLocal() {
    try {
      const [st] = await core.pickFiles("media");
      if (st) app.sheet = { kind: "crop", stagedId: st.id, target: { kind: "local", what: "user", id: userId } };
    } catch (e) {
      fail(e);
    }
  }
  async function msg() {
    const id = userId;
    closeSheet();
    await startDm(id);
  }
</script>

{#if u}
  <Sheet title="" onclose={closeSheet}>
    <div class="hero">
      <Avatar id={userAvatar(u)} initials={initialsOf(u.display_name)} size={92} presence={u.presence ?? null} />
      <h2>{u.display_name}</h2>
      {#if u.title}<p class="role">{u.title}</p>{/if}
      <p class="st" class:on={u.presence === "online"}>{status}</p>
    </div>
    {#if !mine}<button class="msg" onclick={msg}><Icon name="chat" size={20} /> Message</button>{/if}
    <dl>
      <dt>Username</dt><dd>@{u.username}</dd>
      {#if u.phone}<dt>Phone</dt><dd>{u.phone}</dd>{/if}
      {#if u.role !== "member"}<dt>Role</dt><dd>{u.role === "owner" ? "Owner" : "Admin"}</dd>{/if}
      {#if u.is_bot}<dt class="bot">bot</dt><dd class="bot">This is an automated account.</dd>{/if}
    </dl>
    {#if !mine}
      <div class="loc">
        <button onclick={setLocal}><Icon name="image" size={19} /> {hasLocal ? "Change local picture" : "Set local picture"}</button>
        {#if hasLocal}<button onclick={() => core.removeLocalAvatar("user", userId).catch(fail)}><Icon name="trash" size={19} /> Remove local picture</button>{/if}
        <small>A local picture is only on this device. Nobody else sees it.</small>
      </div>
    {/if}
  </Sheet>
{/if}

<style>
  .hero { display: flex; flex-direction: column; align-items: center; text-align: center; padding: 4px 0 12px; }
  h2 { margin: 12px 0 0; font-size: 22px; }
  .role { margin: 2px 0 0; color: var(--text-2); }
  .st { margin: 4px 0 0; color: var(--muted); font-size: 14px; }
  .st.on { color: var(--blue); font-weight: 600; }
  .msg { width: 100%; display: flex; align-items: center; justify-content: center; gap: 9px; background: linear-gradient(145deg, var(--accent), var(--accent-2)); color: #fff; font-weight: 700; border-radius: 24px; padding: 13px; margin: 6px 0 10px; font-size: 16px; }
  dl { display: grid; grid-template-columns: 96px 1fr; gap: 8px 10px; margin: 8px 2px; }
  dt { color: var(--muted); font-size: 14px; }
  dd { margin: 0; overflow-wrap: anywhere; }
  .bot { font-size: 12.5px; opacity: 0.7; }
  .loc { display: flex; flex-direction: column; gap: 2px; border-top: 1px solid var(--line); margin-top: 10px; padding-top: 8px; }
  .loc button { display: flex; align-items: center; gap: 12px; padding: 11px 4px; border-radius: 10px; text-align: left; }
  .loc button:hover { background: var(--hover); }
  .loc small { color: var(--muted); padding: 2px 4px; }
</style>
