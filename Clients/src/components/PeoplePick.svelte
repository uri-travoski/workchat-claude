<script lang="ts">
  import { app, userAvatar } from "../lib/app.svelte";
  import { initialsOf } from "../lib/people";
  import Avatar from "./Avatar.svelte";
  import Icon from "./Icon.svelte";

  // A searchable list of the server's people; with `multi` each row has a check.
  let { selected = $bindable([]), multi = false, onpick, exclude = [] }: { selected?: string[]; multi?: boolean; onpick?: (id: string) => void; exclude?: string[] } = $props();
  let q = $state("");
  const people = $derived(
    Object.values(app.users)
      .filter((u) => u.id !== app.me?.user.id && !u.disabled && !u.is_bot && !exclude.includes(u.id))
      .filter((u) => !q.trim() || u.display_name.toLowerCase().includes(q.trim().toLowerCase()) || u.username.toLowerCase().includes(q.trim().toLowerCase()))
      .sort((a, b) => a.display_name.localeCompare(b.display_name)),
  );
  function tap(id: string) {
    if (multi) selected = selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id];
    else onpick?.(id);
  }
</script>

<div class="search"><Icon name="search" size={17} /><input placeholder="Search people" bind:value={q} aria-label="Search people" /></div>
<div class="list">
  {#each people as u (u.id)}
    <button class="p" onclick={() => tap(u.id)}>
      <Avatar id={userAvatar(u)} initials={initialsOf(u.display_name)} size={40} presence={u.presence ?? null} bot={u.is_bot} />
      <span class="t"><b>{u.display_name}</b><small>{u.title ?? "@" + u.username}</small></span>
      {#if multi}<span class="ck" class:on={selected.includes(u.id)}>{#if selected.includes(u.id)}<Icon name="check" size={15} stroke={3} />{/if}</span>{/if}
    </button>
  {:else}
    <p class="empty">Nobody matches.</p>
  {/each}
</div>

<style>
  .search { display: flex; align-items: center; gap: 8px; color: var(--muted); background: var(--chip); border-radius: 18px; padding: 0 12px; margin: 2px 0 8px; }
  .search input { flex: 1; border: 0; background: transparent; padding: 10px 0; outline: none; font-size: 15px; }
  .p { width: 100%; display: flex; align-items: center; gap: 12px; padding: 8px 4px; text-align: left; border-radius: 10px; }
  .p:hover { background: var(--hover); }
  .t { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .t b { font-size: 15.5px; }
  .t small { color: var(--muted); font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ck { width: 22px; height: 22px; border-radius: 50%; border: 2px solid var(--line); display: inline-flex; align-items: center; justify-content: center; color: #fff; }
  .ck.on { background: var(--accent); border-color: var(--accent); }
  .empty { color: var(--muted); text-align: center; margin: 20px; }
</style>
