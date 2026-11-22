<script lang="ts">
  import { registerLayer } from "../lib/overlays";
  import Icon from "./Icon.svelte";

  import type { MenuItem } from "../lib/menu";
  let { items, x = 0, y = 0, right = false, onclose }: { items: MenuItem[]; x?: number; y?: number; right?: boolean; onclose: () => void } = $props();

  let el = $state<HTMLDivElement>();
  let pos = $state({ left: 0, top: 0 });
  $effect(() => {
    // Keep the menu inside the window.
    const w = el?.offsetWidth ?? 200;
    const h = el?.offsetHeight ?? 200;
    const left = right ? x - w : x;
    pos = { left: Math.max(8, Math.min(left, window.innerWidth - w - 8)), top: Math.max(8, Math.min(y, window.innerHeight - h - 8)) };
  });
  $effect(() => registerLayer(onclose));
</script>

<div class="backdrop" role="presentation" onclick={onclose} oncontextmenu={(e) => { e.preventDefault(); onclose(); }}></div>
<div class="menu" role="menu" bind:this={el} style="left:{pos.left}px;top:{pos.top}px">
  {#each items as it}
    <button role="menuitem" class:danger={it.danger} disabled={it.disabled} onclick={() => { onclose(); it.run(); }}>
      {#if it.icon}<Icon name={it.icon} size={18} />{/if}<span>{it.label}</span>
    </button>
  {/each}
</div>

<style>
  .backdrop { position: fixed; inset: 0; z-index: 50; }
  .menu { position: fixed; z-index: 51; min-width: 190px; background: var(--surface); border-radius: 12px; box-shadow: var(--shadow-pop); border: 1px solid var(--line); padding: 6px; display: flex; flex-direction: column; }
  button { display: flex; align-items: center; gap: 12px; padding: 10px 12px; border-radius: 8px; font-size: 15px; text-align: left; }
  button:hover:not(:disabled) { background: var(--hover); }
  button:disabled { opacity: 0.4; cursor: default; }
  .danger { color: var(--danger); }
</style>
