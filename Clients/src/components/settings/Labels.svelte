<script lang="ts">
  import { app, fail } from "../../lib/app.svelte";
  import { core, type Label } from "../../lib/core";
  import { LABEL_NAME_MAX, nextColor } from "../../lib/labels";
  import Icon from "../Icon.svelte";
  import LabelColors from "../LabelColors.svelte";
  import PageHead from "./PageHead.svelte";

  // The person's own labels (private): rename, recolour, delete, add.
  let editing = $state<string | null>(null);
  let eName = $state("");
  let eColor = $state("");
  let nName = $state("");
  let nColor = $state(nextColor(app.labels));
  let busy = $state(false);

  const uses = (id: string) => Object.values(app.savedLabels).filter((ids) => ids.includes(id)).length;
  /** Runs a core call; the result is null when it failed (the person got the message). */
  async function run<T>(p: Promise<T>): Promise<T | null> {
    busy = true;
    try {
      return await p;
    } catch (e) {
      fail(e);
      return null;
    } finally {
      busy = false;
    }
  }
  function edit(l: Label) {
    editing = l.id;
    eName = l.name;
    eColor = l.color;
  }
  async function save(l: Label) {
    const name = eName.trim();
    if (!name) return;
    const patch = { name: name !== l.name ? name : undefined, color: eColor !== l.color ? eColor : undefined };
    if (!patch.name && !patch.color) editing = null;
    else if (await run(core.updateLabel(l.id, patch))) editing = null;
  }
  async function remove(l: Label) {
    const n = uses(l.id);
    const note = n ? ` It will be taken off ${n} saved ${n === 1 ? "message" : "messages"}.` : "";
    if (!window.confirm(`Delete the label "${l.name}"?${note}`)) return;
    if (await run(core.deleteLabel(l.id)) && editing === l.id) editing = null;
  }
  async function add() {
    const name = nName.trim();
    if (!name) return;
    const list = await run(core.createLabel(name, nColor));
    if (list) {
      nName = "";
      nColor = nextColor(list);
    }
  }
</script>

<div class="st-page">
  <PageHead title="Labels" />
  <div class="st-body scroll" class:desk={!app.phone}>
    <p class="st-note">Labels are private to you. Put them on a message from its menu, then filter the Saved list by label.</p>

    <h3 class="st-cap">Your labels</h3>
    <section class="st-card">
      {#each app.labels as l (l.id)}
        {#if editing === l.id}
          <form class="st-blk" onsubmit={(e) => { e.preventDefault(); void save(l); }}>
            <input class="st-input" maxlength={LABEL_NAME_MAX} bind:value={eName} aria-label="Label name" />
            <LabelColors bind:value={eColor} />
            <div class="st-btns">
              <button class="st-btn pri" disabled={busy || !eName.trim()}>Save</button>
              <button type="button" class="st-btn" onclick={() => (editing = null)}>Cancel</button>
            </div>
          </form>
        {:else}
          {@const n = uses(l.id)}
          <div class="st-row">
            <button class="main" aria-label={`Edit label ${l.name}`} onclick={() => edit(l)}>
              <span class="dot" style="background:{l.color}"></span>
              <span class="st-tx"><b>{l.name}</b><small>{n} saved {n === 1 ? "message" : "messages"}</small></span>
              <span class="st-chev"><Icon name="pencil" size={17} /></span>
            </button>
            <button class="del" aria-label={`Delete label ${l.name}`} title="Delete" disabled={busy} onclick={() => remove(l)}><Icon name="trash" size={18} /></button>
          </div>
        {/if}
      {:else}
        <div class="st-blk"><p class="st-note">No labels yet. Add your first one below.</p></div>
      {/each}
    </section>

    <h3 class="st-cap">Add label</h3>
    <section class="st-card">
      <form class="st-blk" onsubmit={(e) => { e.preventDefault(); void add(); }}>
        <input class="st-input" placeholder="Label name" maxlength={LABEL_NAME_MAX} bind:value={nName} aria-label="New label name" />
        <LabelColors bind:value={nColor} label="New label colour" />
        <div class="st-btns"><button class="st-btn pri" disabled={busy || !nName.trim()}><Icon name="plus" size={17} stroke={2.2} /> Add label</button></div>
      </form>
    </section>
  </div>
</div>

<style>
  .st-row { padding: 0 6px 0 0; gap: 0; }
  .main { flex: 1; min-width: 0; display: flex; align-items: center; gap: 14px; padding: 13px 8px 13px 16px; text-align: left; }
  .dot { width: 18px; height: 18px; border-radius: 50%; flex: none; }
  .del { width: 40px; height: 40px; border-radius: 50%; color: var(--danger); display: inline-flex; align-items: center; justify-content: center; flex: none; }
  .del:hover:not(:disabled) { background: var(--hover); }
  .del:disabled { opacity: 0.5; }
</style>
