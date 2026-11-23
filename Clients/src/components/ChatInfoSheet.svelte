<script lang="ts">
  import { onMount } from "svelte";
  import { app, closeConversation, closeSheet, fail, toast, userAvatar } from "../lib/app.svelte";
  import { core, type Member } from "../lib/core";
  import { channelIcon } from "../lib/chat";
  import { initialsOf } from "../lib/people";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";
  import PeoplePick from "./PeoplePick.svelte";
  import Sheet from "./Sheet.svelte";
  type CropTarget = Extract<NonNullable<typeof app.sheet>, { kind: "crop" }>["target"];

  let { conv }: { conv: string } = $props();
  const row = $derived(app.rows[conv]);
  const group = $derived(row?.kind === "channel" || row?.kind === "group_dm");
  // Admins and the chat's own manager (the creator) manage it; the server checks again.
  const manage = $derived(group && !!row?.can_manage);
  const isAdmin = $derived(app.me?.user.role === "owner" || app.me?.user.role === "admin");
  const isLocal = $derived(row?.avatar_source === "local");
  const myId = $derived(app.me?.user.id ?? "");

  let members = $state<Member[]>([]);
  let editing = $state(false);
  let name = $state("");
  let topic = $state("");
  let adding = $state(false);
  let toAdd = $state<string[]>([]);
  let busy = $state(false);

  async function loadMembers() {
    try {
      members = await core.getMembers(conv);
    } catch {
      members = [];
    }
  }
  onMount(loadMembers);
  // The member list follows the chat (someone joined, left or was added).
  $effect(() => {
    void row?.member_count;
    void loadMembers();
  });

  async function pick(target: CropTarget) {
    try {
      const [st] = await core.pickFiles("image");
      if (st) app.sheet = { kind: "crop", stagedId: st.id, target };
    } catch (e) {
      fail(e);
    }
  }
  // A DM's local picture belongs to the other person; a channel's belongs to the chat.
  const localTarget = $derived(row?.kind === "dm" && row.peer_id ? ({ kind: "local", what: "user", id: row.peer_id } as const) : ({ kind: "local", what: "conv", id: conv } as const));
  async function removeLocal() {
    try {
      if (row?.kind === "dm" && row.peer_id) await core.removeLocalAvatar("user", row.peer_id);
      else await core.removeLocalAvatar("conv", conv);
    } catch (e) {
      fail(e);
    }
  }
  async function run(p: Promise<unknown>, done?: string) {
    busy = true;
    try {
      await p;
      if (done) toast(done);
      return true;
    } catch (e) {
      fail(e);
      return false;
    } finally {
      busy = false;
    }
  }
  function startEdit() {
    name = row?.title ?? "";
    topic = row?.topic ?? "";
    editing = true;
  }
  async function saveEdit() {
    if (!name.trim()) return fail({ message: "The name cannot be empty." });
    if (await run(core.updateConversation(conv, { name: name.trim(), topic: topic.trim() }), "Saved.")) editing = false;
  }
  async function archive() {
    const on = !row?.archived;
    if (on && !window.confirm("Archive this channel? It becomes read-only and leaves the channel list.")) return;
    await run(core.updateConversation(conv, { archived: on }), on ? "Channel archived." : "Channel restored.");
  }
  async function addPeople() {
    if (!toAdd.length) return;
    if (await run(core.addMembers(conv, toAdd), `Added ${toAdd.length} ${toAdd.length === 1 ? "person" : "people"}.`)) {
      toAdd = [];
      adding = false;
      await core.getMembers(conv).then((m) => (members = m)).catch(() => {});
    }
  }
  async function remove(id: string) {
    const u = app.users[id];
    if (!window.confirm(`Remove ${u?.display_name ?? "this person"} from the ${row?.kind === "channel" ? "channel" : "group"}?`)) return;
    if (await run(core.removeMember(conv, id), "Removed.")) await loadMembers();
  }
  async function leave() {
    if (!window.confirm(`Leave ${row?.title}? You will need to be added again to come back.`)) return;
    if (await run(core.leaveConversation(conv))) {
      closeSheet();
      closeConversation();
    }
  }
  const canRemove = (m: Member) => manage && m.user_id !== myId && (m.role !== "owner" || isAdmin);
  const sorted = $derived([...members].sort((a, b) => (a.role === "owner" ? 0 : 1) - (b.role === "owner" ? 0 : 1) || (app.users[a.user_id]?.display_name ?? "").localeCompare(app.users[b.user_id]?.display_name ?? "")));
</script>

{#if row}
  <Sheet title="" onclose={closeSheet} tall={adding}>
    {#if adding}
      <h4 class="first">Add people</h4>
      <PeoplePick multi bind:selected={toAdd} exclude={members.map((m) => m.user_id)} />
      <div class="row2"><button class="ghost" onclick={() => { adding = false; toAdd = []; }}>Cancel</button><button class="ok" disabled={!toAdd.length || busy} onclick={addPeople}>Add {toAdd.length || ""}</button></div>
    {:else}
      <div class="hero">
        <Avatar id={row.avatar} initials={row.initials} size={88} tile={row.kind === "channel"} bot={row.is_bot} icon={channelIcon(row.title)} presence={row.presence} />
        {#if editing}
          <label class="f">Name<input bind:value={name} maxlength="80" /></label>
          {#if row.kind === "channel"}<label class="f">Topic<input bind:value={topic} maxlength="250" placeholder="What is this channel for?" /></label>{/if}
          <div class="row2"><button class="ghost" onclick={() => (editing = false)}>Cancel</button><button class="ok" disabled={busy} onclick={saveEdit}>Save</button></div>
        {:else}
          <h2>{row.title}</h2>
          {#if row.topic}<p class="topic">{row.topic}</p>{/if}
          {#if group}<p class="sub">{row.member_count} {row.member_count === 1 ? "member" : "members"}{row.kind === "channel" ? " · Private channel" : ""}{row.archived ? " · Archived" : ""}</p>{/if}
          {#if isLocal}<p class="loc">Showing your local picture</p>{/if}
        {/if}
      </div>
      <div class="acts">
        {#if manage && !editing}<button onclick={startEdit}><Icon name="pencil" size={19} /> Rename{row.kind === "channel" ? " or change topic" : ""}</button>{/if}
        {#if manage}<button onclick={() => pick({ kind: "conv", id: conv })}><Icon name="camera" size={19} /> Change {row.kind === "channel" ? "channel" : "group"} picture</button>{/if}
        {#if manage && row.kind === "channel"}
          <button onclick={archive} disabled={busy}><Icon name="archive" size={19} /> {row.archived ? "Restore channel" : "Archive channel"}</button>
        {/if}
        <button onclick={() => pick(localTarget)}><Icon name="image" size={19} /> {isLocal ? "Change local picture" : "Set local picture"}</button>
        {#if isLocal}<button onclick={removeLocal}><Icon name="trash" size={19} /> Remove local picture</button>{/if}
        <small>A local picture is only on this device. Nobody else sees it.</small>
        {#if group}<button class="danger" onclick={leave} disabled={busy}><Icon name="logout" size={19} /> Leave {row.kind === "channel" ? "channel" : "group"}</button>{/if}
      </div>
      {#if group}
        <div class="mh"><h4>Members ({members.length || row.member_count})</h4>{#if manage}<button class="add" onclick={() => (adding = true)}><Icon name="plus" size={16} stroke={2.4} /> Add people</button>{/if}</div>
        {#each sorted as mem (mem.user_id)}
          {@const u = app.users[mem.user_id]}
          {#if u}
            <div class="m">
              <button class="mm" onclick={() => (app.sheet = { kind: "profile", userId: mem.user_id })}>
                <Avatar id={userAvatar(u)} initials={initialsOf(u.display_name)} size={38} presence={u.presence ?? null} bot={u.is_bot} />
                <span><b>{u.display_name}{#if mem.role === "owner"} <i class="mgr">Manager</i>{/if}</b><small>{u.title ?? "@" + u.username}</small></span>
              </button>
              {#if canRemove(mem)}<button class="rm" aria-label={`Remove ${u.display_name}`} title="Remove" onclick={() => remove(mem.user_id)}><Icon name="x" size={17} /></button>{/if}
            </div>
          {/if}
        {/each}
      {/if}
    {/if}
  </Sheet>
{/if}

<style>
  .hero { display: flex; flex-direction: column; align-items: center; text-align: center; padding-bottom: 10px; gap: 4px; }
  h2 { margin: 12px 0 0; font-size: 21px; }
  .topic { margin: 4px 8px 0; color: var(--text-2); }
  .sub, .loc { margin: 4px 0 0; color: var(--muted); font-size: 14px; }
  .f { align-self: stretch; display: flex; flex-direction: column; gap: 4px; text-align: left; font-size: 13px; font-weight: 600; color: var(--muted); margin-top: 8px; }
  .f input { border: 1px solid var(--line); background: var(--surface-2); border-radius: 12px; padding: 10px 12px; font-size: 15.5px; outline: none; color: var(--text); font-weight: 400; }
  .row2 { display: flex; justify-content: flex-end; gap: 10px; margin-top: 10px; align-self: stretch; }
  .ghost { padding: 10px 16px; font-weight: 600; color: var(--text-2); }
  .ok { background: linear-gradient(145deg, var(--accent), var(--accent-2)); color: #fff; font-weight: 700; border-radius: 20px; padding: 10px 22px; }
  .ok:disabled { opacity: 0.5; }
  .acts { display: flex; flex-direction: column; border-top: 1px solid var(--line); padding-top: 6px; }
  .acts button { display: flex; align-items: center; gap: 12px; padding: 11px 4px; border-radius: 10px; text-align: left; }
  .acts button:hover { background: var(--hover); }
  .acts button.danger { color: var(--danger); }
  .acts small { color: var(--muted); padding: 2px 4px; }
  h4 { margin: 0; font-size: 13px; color: var(--text-2); letter-spacing: 0.06em; text-transform: uppercase; }
  h4.first { margin-bottom: 8px; }
  .mh { display: flex; align-items: center; justify-content: space-between; margin: 16px 0 4px; }
  .add { display: inline-flex; align-items: center; gap: 5px; color: var(--blue); font-weight: 700; font-size: 14px; padding: 4px 8px; border-radius: 12px; }
  .add:hover { background: var(--hover); }
  .m { display: flex; align-items: center; }
  .mm { flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; padding: 7px 4px; text-align: left; border-radius: 10px; }
  .mm:hover { background: var(--hover); }
  .mm span { display: flex; flex-direction: column; min-width: 0; }
  .mm small { color: var(--muted); }
  .mgr { font-style: normal; font-size: 11.5px; font-weight: 700; color: var(--blue); background: var(--chip); border-radius: 8px; padding: 1px 7px; margin-left: 4px; }
  .rm { width: 34px; height: 34px; border-radius: 50%; color: var(--muted); display: inline-flex; align-items: center; justify-content: center; }
  .rm:hover { background: var(--hover); color: var(--danger); }
</style>
