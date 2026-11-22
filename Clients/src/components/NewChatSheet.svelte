<script lang="ts">
  import { app, closeSheet, fail, openConversation, startDm, toast } from "../lib/app.svelte";
  import { core } from "../lib/core";
  import { mediaUrl } from "../lib/media";
  import Icon from "./Icon.svelte";
  import PeoplePick from "./PeoplePick.svelte";
  import Sheet from "./Sheet.svelte";

  let { step = "menu" }: { step?: "menu" | "dm" | "group" | "channel" } = $props();
  // svelte-ignore state_referenced_locally
  let cur = $state<typeof step>(step);
  let busy = $state(false);
  const isAdmin = $derived(app.me?.user.role === "owner" || app.me?.user.role === "admin");
  // The organisation decides who may create channels (live); groups are open to everyone.
  const canChannel = $derived(isAdmin || app.org.channel_creation === "everyone");
  const d = app.chatDraft;
  let name = $state(d.name);
  let topic = $state(d.topic);
  let members = $state<string[]>([...d.members]);

  async function pickPicture() {
    app.chatDraft = { name, topic, members, avatarId: d.avatarId };
    try {
      const [st] = await core.pickFiles("media");
      if (st) app.sheet = { kind: "crop", stagedId: st.id, target: { kind: "newchat" } };
    } catch (e) {
      fail(e);
    }
  }
  async function create(kind: "channel" | "group_dm") {
    if (busy) return;
    busy = true;
    try {
      const id = await core.createConversation(kind, name.trim(), members, topic.trim() || undefined, app.chatDraft.avatarId || null);
      app.chatDraft = { name: "", topic: "", members: [], avatarId: "" };
      closeSheet();
      await openConversation(id);
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }
  const title = $derived({ menu: "New chat", dm: "New message", group: "New group", channel: "New channel" }[cur]);
  const hasAvatar = $derived(!!app.chatDraft.avatarId);
</script>

<Sheet {title} onclose={closeSheet} tall={cur !== "menu"}>
  {#if cur === "menu"}
    <button class="opt" onclick={() => (cur = "dm")}><span class="oi"><Icon name="user" size={22} /></span><span><b>New message</b><small>Chat with one person</small></span></button>
    <button class="opt" onclick={() => (cur = "group")}><span class="oi"><Icon name="users" size={22} /></span><span><b>New group</b><small>A private chat for several people</small></span></button>
    {#if canChannel}<button class="opt" onclick={() => (cur = "channel")}><span class="oi"><Icon name="hash" size={22} /></span><span><b>New channel</b><small>A private space with a name and topic</small></span></button>{/if}
  {:else if cur === "dm"}
    <PeoplePick onpick={(id) => { closeSheet(); void startDm(id); }} />
  {:else}
    <div class="form">
      {#if cur === "channel"}
        <button class="pic" onclick={pickPicture} aria-label="Choose a picture">
          {#if hasAvatar}<img src={mediaUrl(app.chatDraft.avatarId)} alt="" />{:else}<Icon name="camera" size={26} />{/if}
        </button>
      {/if}
      <label>Name<input bind:value={name} placeholder={cur === "channel" ? "e.g. product-launch" : "Group name"} maxlength="80" /></label>
      {#if cur === "channel"}<label>Topic<input bind:value={topic} placeholder="What is this channel for? (optional)" maxlength="250" /></label>{/if}
    </div>
    <h4>Members {members.length ? `(${members.length})` : ""}</h4>
    <PeoplePick multi bind:selected={members} />
  {/if}
  {#snippet footer()}
    {#if cur === "channel" || cur === "group"}
      <button class="ghost" onclick={() => (cur = "menu")}>Back</button>
      <button class="ok" disabled={busy || !name.trim() || (cur === "group" && !members.length)} onclick={() => create(cur === "channel" ? "channel" : "group_dm")}>{busy ? "Creating…" : cur === "channel" ? "Create channel" : "Create group"}</button>
    {:else if cur !== "menu"}
      <button class="ghost" onclick={() => (cur = "menu")}>Back</button>
    {/if}
  {/snippet}
</Sheet>

<style>
  .opt { width: 100%; display: flex; align-items: center; gap: 14px; padding: 11px 4px; text-align: left; border-radius: 12px; }
  .opt:hover { background: var(--hover); }
  .opt span:last-child { display: flex; flex-direction: column; min-width: 0; }
  .opt b { font-size: 16.5px; }
  .opt small { color: var(--muted); font-size: 13.5px; }
  .oi { width: 44px; height: 44px; border-radius: 50%; background: var(--chip); color: var(--blue); display: inline-flex; align-items: center; justify-content: center; flex: none; }

  .form { display: flex; flex-direction: column; gap: 10px; align-items: stretch; }
  .pic { align-self: center; width: 84px; height: 84px; border-radius: 22px; background: var(--chip); color: var(--muted); display: inline-flex; align-items: center; justify-content: center; overflow: hidden; }
  .pic img { width: 100%; height: 100%; object-fit: cover; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; color: var(--muted); font-weight: 600; }
  input { border: 1px solid var(--line); background: var(--surface-2); border-radius: 12px; padding: 10px 12px; font-size: 15.5px; outline: none; color: var(--text); font-weight: 400; }
  input:focus { border-color: color-mix(in srgb, var(--blue) 55%, var(--line)); }
  h4 { margin: 14px 0 4px; font-size: 13px; color: var(--text-2); letter-spacing: 0.06em; text-transform: uppercase; }
  .ghost { padding: 10px 16px; font-weight: 600; color: var(--text-2); }
  .ok { margin-left: auto; background: linear-gradient(145deg, var(--accent), var(--accent-2)); color: #fff; font-weight: 700; border-radius: 20px; padding: 10px 22px; }
  .ok:disabled { opacity: 0.5; }
</style>
