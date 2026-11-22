<script lang="ts">
  import type { Snippet } from "svelte";
  import { app } from "../lib/app.svelte";
  import Icon from "./Icon.svelte";

  // A bottom sheet on phones and a centred modal on desktop (desktop/4.webp). Escape and the backdrop close it.
  let {
    title = "",
    onclose,
    children,
    footer,
    wide = false,
    tall = false,
    plain = false,
  }: { title?: string; onclose: () => void; children: Snippet; footer?: Snippet; wide?: boolean; tall?: boolean; plain?: boolean } = $props();
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />
<div class="back" class:phone={app.phone} role="presentation" onclick={onclose}></div>
<div class="sheet" class:phone={app.phone} class:wide class:tall role="dialog" aria-modal="true" aria-label={title}>
  {#if app.phone}<span class="grab"></span>{/if}
  {#if !plain}
    <header><h2>{title}</h2><button class="x" aria-label="Close" onclick={onclose}><Icon name="x" size={20} /></button></header>
  {/if}
  <div class="body scroll" class:plain>{@render children()}</div>
  {#if footer}<footer>{@render footer()}</footer>{/if}
</div>

<style>
  .back { position: fixed; inset: 0; background: rgba(12, 18, 32, 0.45); z-index: 60; animation: fade 0.15s; }
  .sheet { position: fixed; z-index: 61; left: 50%; top: 50%; transform: translate(-50%, -50%); width: min(440px, calc(100vw - 32px)); max-height: min(680px, calc(100vh - 48px)); background: var(--surface); border-radius: 16px; box-shadow: var(--shadow-pop); display: flex; flex-direction: column; overflow: hidden; border: 1px solid var(--line); animation: pop 0.14s; }
  .sheet.wide { width: min(560px, calc(100vw - 32px)); }
  .sheet.tall { height: min(640px, calc(100vh - 48px)); }
  .sheet.phone { left: 0; right: 0; top: auto; bottom: 0; transform: none; width: 100%; max-height: 88vh; border-radius: 20px 20px 0 0; border: 0; padding-bottom: var(--safe-bottom); animation: up 0.2s ease-out; }
  .sheet.phone.tall { height: 88vh; }
  .grab { align-self: center; width: 38px; height: 4px; border-radius: 2px; background: var(--line); margin: 8px 0 2px; flex: none; }
  header { display: flex; align-items: center; justify-content: space-between; padding: 12px 14px 8px 18px; flex: none; }
  .phone header { padding-top: 6px; }
  h2 { margin: 0; font-size: 18px; letter-spacing: -0.01em; }
  .x { width: 34px; height: 34px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; color: var(--muted); }
  .x:hover { background: var(--hover); }
  .body { flex: 1; min-height: 0; padding: 4px 18px 16px; }
  .body.plain { padding: 0; }
  footer { flex: none; padding: 10px 16px 14px; border-top: 1px solid var(--line); display: flex; gap: 10px; align-items: center; }
  @keyframes fade { from { opacity: 0; } }
  @keyframes pop { from { opacity: 0; transform: translate(-50%, -48%) scale(0.97); } }
  @keyframes up { from { transform: translateY(40px); opacity: 0.6; } }
</style>
