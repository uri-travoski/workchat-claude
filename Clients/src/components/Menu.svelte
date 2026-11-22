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
    {#if it.heading}
      <div class="cap" role="presentation">{it.label}</div>
    {:else}
      <button role={it.checked === undefined ? "menuitem" : "menuitemradio"} aria-checked={it.checked} class:danger={it.danger} disabled={it.disabled} onclick={() => { onclose(); it.run(); }}>
        {#if it.icon}<Icon name={it.icon} size={18} />{/if}<span>{it.label}</span>{#if it.checked !== undefined}<span class="tick">{#if it.checked}<Icon name="check" size={18} stroke={2.2} />{/if}</span>{/if}
      </button>
    {/if}
  {/each}
</div>

<style>
  .backdrop { position: fixed; inset: 0; z-index: 50; }
  .menu { position: fixed; z-index: 51; min-width: 190px; background: var(--surface); border-radius: 12px; box-shadow: var(--shadow-pop); border: 1px solid var(--line); padding: 6px; display: flex; flex-direction: column; }
  button { display: flex; align-items: center; gap: 12px; padding: 10px 12px; border-radius: 8px; font-size: 15px; text-align: left; }
  button:hover:not(:disabled) { background: var(--hover); }
  button:disabled { opacity: 0.4; cursor: default; }
  .cap { margin-top: 4px; padding: 10px 12px 4px; font-size: 12px; font-weight: 700; letter-spacing: 0.07em; text-transform: uppercase; color: var(--muted); border-top: 1px solid var(--line); }
  .tick { margin-left: auto; padding-left: 14px; color: var(--blue); display: inline-flex; width: 32px; justify-content: flex-end; }
  .danger { color: var(--danger); }
</style>
