<script lang="ts">
  import { app, closeSheet, fail, openConversation, startDm, toast } from "../lib/app.svelte";
  import { core, type ChannelBrowseItem } from "../lib/core";
  import { channelIcon } from "../lib/chat";
  import { mediaUrl } from "../lib/media";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import PeoplePick from "./PeoplePick.svelte";
  import Sheet from "./Sheet.svelte";

  let { step = "menu" }: { step?: "menu" | "dm" | "group" | "channel" | "browse" } = $props();
  // svelte-ignore state_referenced_locally
  let cur = $state<typeof step>(step);
  let busy = $state(false);
  const isAdmin = $derived(app.me?.user.role === "owner" || app.me?.user.role === "admin");
  // The organisation decides who may create channels (live); groups are open to everyone.
  const canChannel = $derived(isAdmin || app.org.channel_creation === "everyone");
  // svelte-ignore state_referenced_locally
  let visibility = $state<"public" | "private">(app.chatDraft.visibility);
  const d = app.chatDraft;
  let name = $state(d.name);
  let topic = $state(d.topic);
  let members = $state<string[]>([...d.members]);
  const channels = $derived(Object.values(app.rows).filter((r) => r.kind === "channel" && !r.archived).sort((a, b) => a.title.localeCompare(b.title)));

  async function pickPicture() {
    app.chatDraft = { name, topic, members, avatarId: d.avatarId, visibility };
    try {
      const [st] = await core.pickFiles("media");
      if (st) app.sheet = { kind: "crop", stagedId: st.id, target: { kind: "newchat" } };
    } catch (e) {
      fail(e);
    }
  }
  // Browse: public channels the person is not in (server side), searched as they type.
  let found = $state<ChannelBrowseItem[]>([]);
  let total = $state(0);
  let q = $state("");
  let loading = $state(false);
  let joining = $state("");
  let more = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  async function loadBrowse(append = false) {
    loading = true;
    try {
      const page = await core.browseChannels(q, 50, append ? found.length : 0);
      found = append ? [...found, ...page.channels] : page.channels;
      total = page.total;
      more = page.has_more;
    } catch (e) {
      fail(e);
    } finally {
      loading = false;
    }
  }
  $effect(() => {
    if (cur !== "browse") return;
    void q;
    void app.directoryTick;
    clearTimeout(timer);
    timer = setTimeout(() => void loadBrowse(), 200);
    return () => clearTimeout(timer);
  });
  async function join(c: ChannelBrowseItem) {
    joining = c.id;
    try {
      const id = await core.joinChannel(c.id);
      closeSheet();
      await openConversation(id);
    } catch (e) {
      fail(e);
      await loadBrowse();
    } finally {
      joining = "";
    }
  }
  async function create(kind: "channel" | "group_dm") {
    if (busy) return;
    busy = true;
    try {
      const id = await core.createConversation(kind, name.trim(), members, topic.trim() || undefined, app.chatDraft.avatarId || null, kind === "channel" ? visibility : undefined);
      app.chatDraft = { name: "", topic: "", members: [], avatarId: "", visibility: "public" };
      closeSheet();
      await openConversation(id);
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }
  const title = $derived({ menu: "New chat", dm: "New message", group: "New group", channel: "New channel", browse: "Channels" }[cur]);
  const hasAvatar = $derived(!!app.chatDraft.avatarId);
</script>

<Sheet {title} onclose={closeSheet} tall={cur !== "menu"}>
  {#if cur === "menu"}
    <button class="opt" onclick={() => (cur = "dm")}><span class="oi"><Icon name="user" size={22} /></span><span><b>New message</b><small>Chat with one person</small></span></button>
    <button class="opt" onclick={() => (cur = "group")}><span class="oi"><Icon name="users" size={22} /></span><span><b>New group</b><small>A private chat for several people</small></span></button>
    {#if canChannel}<button class="opt" onclick={() => (cur = "channel")}><span class="oi"><Icon name="hash" size={22} /></span><span><b>New channel</b><small>A shared space with a name and topic</small></span></button>{/if}
    <button class="opt" onclick={() => (cur = "browse")}><span class="oi"><Icon name="megaphone" size={22} /></span><span><b>Browse channels</b><small>Find and join public channels</small></span></button>
  {:else if cur === "dm"}
    <PeoplePick onpick={(id) => { closeSheet(); void startDm(id); }} />
  {:else if cur === "browse"}
    <input class="search" type="search" placeholder="Search public channels" bind:value={q} aria-label="Search channels" />
    <h4>Public channels you can join {total ? `(${total})` : ""}</h4>
    {#each found as c (c.id)}
      <div class="opt">
        <Avatar id={c.avatar ?? null} initials={c.name.slice(0, 2).toUpperCase()} size={42} tile icon={channelIcon(c.name)} />
        <span class="grow"><b># {c.name}</b><small>{c.topic || ""}{c.topic ? " · " : ""}{c.member_count} {c.member_count === 1 ? "member" : "members"}</small></span>
        <button class="join" disabled={joining === c.id} onclick={() => join(c)}>{joining === c.id ? "Joining…" : "Join"}</button>
      </div>
    {:else}
      <p class="none">{loading ? "Searching…" : q.trim() ? "No public channel matches." : "There are no public channels to join right now."}</p>
    {/each}
    {#if more}<button class="ghost wide" disabled={loading} onclick={() => loadBrowse(true)}>Show more</button>{/if}
    {#if channels.length}
      <h4>Your channels</h4>
      {#each channels as r (r.id)}
        <button class="opt" onclick={() => { closeSheet(); void openConversation(r.id); }}>
          <Avatar id={r.avatar} initials={r.initials} size={42} tile icon={channelIcon(r.title)} />
          <span><b># {r.title}</b><small>{r.topic || `${r.member_count} members`}</small></span>
        </button>
      {/each}
    {/if}
  {:else}
    <div class="form">
      {#if cur === "channel"}
        <button class="pic" onclick={pickPicture} aria-label="Choose a picture">
          {#if hasAvatar}<img src={mediaUrl(app.chatDraft.avatarId)} alt="" />{:else}<Icon name="camera" size={26} />{/if}
        </button>
      {/if}
      <label>Name<input bind:value={name} placeholder={cur === "channel" ? "e.g. product-launch" : "Group name"} maxlength="80" /></label>
      {#if cur === "channel"}<label>Topic<input bind:value={topic} placeholder="What is this channel for? (optional)" maxlength="250" /></label>
        <div class="vis" role="radiogroup" aria-label="Who can find this channel">
          <button role="radio" aria-checked={visibility === "public"} class:on={visibility === "public"} onclick={() => (visibility = "public")}><Icon name="globe" size={18} /><span><b>Public</b><small>Anyone in the workspace can find and join it</small></span></button>
          <button role="radio" aria-checked={visibility === "private"} class:on={visibility === "private"} onclick={() => (visibility = "private")}><Icon name="lock" size={18} /><span><b>Private</b><small>Only people you add can see it</small></span></button>
        </div>{/if}
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
  .opt .grow { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .join { background: linear-gradient(145deg, var(--accent), var(--accent-2)); color: #fff; font-weight: 700; border-radius: 18px; padding: 7px 16px; flex: none; }
  .join:disabled { opacity: 0.6; }
  .search { margin-bottom: 4px; }
  .wide { width: 100%; text-align: center; }
  .vis { display: flex; flex-direction: column; gap: 8px; margin-top: 2px; }
  .vis button { display: flex; align-items: center; gap: 12px; text-align: left; padding: 10px 12px; border-radius: 12px; border: 1.5px solid var(--line); color: var(--text); }
  .vis button.on { border-color: var(--blue); background: color-mix(in srgb, var(--blue) 9%, var(--surface)); }
  .vis button span { display: flex; flex-direction: column; }
  .vis small { color: var(--muted); font-size: 13px; font-weight: 400; }
  .none { color: var(--muted); text-align: center; margin: 14px 6px; font-size: 14px; }

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
