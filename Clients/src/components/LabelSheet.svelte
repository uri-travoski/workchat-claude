<script lang="ts">
  import { untrack } from "svelte";
  import { app, closeSheet, fail } from "../lib/app.svelte";
  import { core, type MessageView } from "../lib/core";
  import { LABEL_NAME_MAX, nextColor, toggleId } from "../lib/labels";
  import Icon from "./Icon.svelte";
  import LabelColors from "./LabelColors.svelte";
  import Sheet from "./Sheet.svelte";

  // Private labels on one message. A tick applies at once; a message that is not saved yet is saved with its first label.
  let { msg }: { msg: MessageView } = $props();
  const id = untrack(() => msg.id);
  let ids = $state<string[]>(untrack(() => [...(app.savedLabels[msg.id] ?? [])]));
  let busy = $state(false);
  let adding = $state(false);
  let name = $state("");
  let color = $state("");

  async function apply(next: string[]) {
    const prev = ids;
    ids = next;
    busy = true;
    try {
      await core.saveMessageLabels(id, next);
    } catch (e) {
      ids = prev;
      fail(e);
    } finally {
      busy = false;
    }
  }
  function startAdd() {
    name = "";
    color = nextColor(app.labels);
    adding = true;
  }
  // Creates the label and puts it on this message.
  async function create() {
    const n = name.trim();
    if (!n || busy) return;
    const before = new Set(app.labels.map((l) => l.id));
    busy = true;
    let made: string | undefined;
    try {
      made = (await core.createLabel(n, color)).find((l) => !before.has(l.id))?.id;
      adding = false;
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
    if (made) await apply([...ids, made]);
  }
</script>

<Sheet title="Label message" onclose={closeSheet}>
  {#if !app.labels.length && !adding}
    <p class="hint">You have no labels yet. Labels are private to you and help you find saved messages.</p>
  {/if}
  <div class="list" role="group" aria-label="Labels">
    {#each app.labels as l (l.id)}
      {@const on = ids.includes(l.id)}
      <button class="r" role="checkbox" aria-checked={on} disabled={busy} onclick={() => apply(toggleId(ids, l.id))}>
        <span class="dot" style="background:{l.color}"></span>
        <span class="nm">{l.name}</span>
        <span class="ck" class:on>{#if on}<Icon name="check" size={16} stroke={3} />{/if}</span>
      </button>
    {/each}
  </div>
  {#if adding}
    <form class="add" onsubmit={(e) => { e.preventDefault(); void create(); }}>
      <input placeholder="Label name" maxlength={LABEL_NAME_MAX} bind:value={name} aria-label="Label name" />
      <LabelColors bind:value={color} />
      <div class="btns">
        <button type="button" class="cancel" onclick={() => (adding = false)}>Cancel</button>
        <button class="go" disabled={!name.trim() || busy}>Add and apply</button>
      </div>
    </form>
  {:else}
    <button class="r new" disabled={busy} onclick={startAdd}><span class="plus"><Icon name="plus" size={18} stroke={2.2} /></span><span class="nm">New label</span></button>
  {/if}
</Sheet>

<style>
  .hint { color: var(--muted); font-size: 14px; margin: 4px 0 10px; line-height: 1.45; }
  .r { width: 100%; display: flex; align-items: center; gap: 14px; padding: 12px 6px; text-align: left; border-radius: 10px; min-height: 52px; }
  .r:hover:not(:disabled) { background: var(--hover); }
  .r:disabled { opacity: 0.6; }
  .dot { width: 16px; height: 16px; border-radius: 50%; flex: none; }
  .nm { flex: 1; min-width: 0; font-size: 16px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ck { width: 24px; height: 24px; border-radius: 7px; border: 2px solid var(--line); display: inline-flex; align-items: center; justify-content: center; color: #fff; flex: none; }
  .ck.on { background: var(--accent); border-color: var(--accent); }
  .plus { width: 16px; display: inline-flex; justify-content: center; color: var(--blue); flex: none; }
  .new .nm { color: var(--blue); font-weight: 600; }
  .add { display: flex; flex-direction: column; gap: 14px; padding: 8px 6px 4px; }
  .add input { border: 1px solid var(--line); background: var(--surface-2); border-radius: 11px; padding: 12px 14px; font-size: 16px; outline: none; color: var(--text); }
  .add input:focus { border-color: var(--blue); }
  .btns { display: flex; gap: 10px; justify-content: flex-end; }
  .cancel { padding: 11px 16px; border-radius: 12px; font-weight: 600; color: var(--text-2); }
  .cancel:hover { background: var(--hover); }
  .go { background: linear-gradient(145deg, var(--accent), var(--accent-2)); color: #fff; font-weight: 700; border-radius: 12px; padding: 11px 18px; }
  .go:disabled { opacity: 0.5; }
</style>
