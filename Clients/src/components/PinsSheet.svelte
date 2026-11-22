<script lang="ts">
  import { app, closeSheet, fail, jumpTo, userName } from "../lib/app.svelte";
  import { core, type MessageView } from "../lib/core";
  import { clock, dayLabel } from "../lib/format";
  import { toPlain } from "../lib/markup";
  import Icon from "./Icon.svelte";
  import Sheet from "./Sheet.svelte";

  let { conv }: { conv: string } = $props();
  let items = $state<MessageView[] | null>(null);
  const row = $derived(app.rows[conv]);
  $effect(() => {
    app.pins[conv]; // reload when the pinned set changes
    core.getPins(conv).then((p) => (items = p), (e) => { fail(e); items = []; });
  });
</script>

<Sheet title="Pinned messages" onclose={closeSheet}>
  {#if items === null}<p class="n">Loading…</p>
  {:else if !items.length}<p class="n">Nothing is pinned in this chat.{row?.can_pin ? " Use a message's menu to pin it." : ""}</p>
  {:else}
    {#each items as m (m.id)}
      <div class="it">
        <button class="go" onclick={() => jumpTo(m.conv_id, m.id, m.seq)}>
          <span class="pi"><Icon name="pin" size={15} fill /></span>
          <span class="tx"><b>{userName(m.sender_id)} <small>{dayLabel(m.ts)}, {clock(m.ts)}</small></b><span>{toPlain(m.body) || m.attachments[0]?.name || "Attachment"}</span></span>
        </button>
        {#if row?.can_pin}<button class="un" aria-label="Unpin" title="Unpin" onclick={() => core.pinMessage(conv, m.id, false).catch(fail)}><Icon name="x" size={17} /></button>{/if}
      </div>
    {/each}
  {/if}
</Sheet>

<style>
  .n { color: var(--muted); text-align: center; margin: 24px 8px; }
  .it { display: flex; align-items: center; border-bottom: 1px solid var(--line); }
  .go { flex: 1; min-width: 0; display: flex; gap: 12px; text-align: left; padding: 12px 2px; align-items: flex-start; }
  .pi { color: var(--blue); padding-top: 3px; }
  .tx { display: flex; flex-direction: column; min-width: 0; }
  .tx b small { color: var(--muted); font-weight: 400; margin-left: 4px; }
  .tx span { color: var(--text-2); overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; }
  .un { width: 34px; height: 34px; border-radius: 50%; color: var(--muted); display: inline-flex; align-items: center; justify-content: center; }
  .un:hover { background: var(--hover); }
</style>
