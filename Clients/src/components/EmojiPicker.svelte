<script lang="ts">
  import { registerLayer } from "../lib/overlays";
  import { EMOJI } from "../lib/emoji";

  let { onpick, onclose, inline = false }: { onpick: (e: string) => void; onclose: () => void; inline?: boolean } = $props();
  let cat = $state(0);
  $effect(() => registerLayer(onclose));
</script>

{#if !inline}<div class="backdrop" role="presentation" onclick={onclose}></div>{/if}
<div class="picker" class:inline role="dialog" aria-label="Emoji">
  <div class="tabs">
    {#each EMOJI as c, i}<button class:on={cat === i} aria-label={c.name} onclick={() => (cat = i)}>{c.icon}</button>{/each}
  </div>
  <div class="grid scroll">
    {#each EMOJI[cat].items as e}<button onclick={() => onpick(e)} aria-label={e}>{e}</button>{/each}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; z-index: 40; }
  .picker { position: absolute; right: 12px; bottom: calc(100% + 8px); z-index: 41; width: min(320px, calc(100vw - 24px)); background: var(--surface); border-radius: 14px; border: 1px solid var(--line); box-shadow: var(--shadow-pop); overflow: hidden; }
  .picker.inline { position: static; width: 100%; border: 0; box-shadow: none; }
  .picker.inline .grid { height: min(300px, 46vh); grid-template-columns: repeat(8, 1fr); }
  .tabs { display: flex; border-bottom: 1px solid var(--line); }
  .tabs button { flex: 1; padding: 8px 0; font-size: 18px; opacity: 0.55; }
  .tabs button.on { opacity: 1; box-shadow: inset 0 -2px var(--blue); }
  .grid { display: grid; grid-template-columns: repeat(8, 1fr); gap: 2px; padding: 8px; height: 188px; align-content: start; }
  .grid button { font-size: 22px; padding: 3px 0; border-radius: 8px; }
  .grid button:hover { background: var(--hover); }
</style>
