<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { openWith, saveAs, showInFolder } from "../lib/files";
  import { registerLayer } from "../lib/overlays";
  import Icon from "./Icon.svelte";

  // Shown when no application could open a downloaded file. Above everything else (also above the media viewer).
  const f = $derived(app.openFail);
  const close = () => (app.openFail = null);
  $effect(() => (f ? registerLayer(close) : undefined));

  async function run(fn: (x: { id: string; name: string }) => Promise<boolean>, closeAfter: boolean) {
    if (!f) return;
    const ok = await fn(f);
    if (ok && closeAfter) close();
  }
</script>

{#if f}
  <div class="back" role="presentation" onclick={close}></div>
  <div class="dlg" role="alertdialog" aria-modal="true" aria-labelledby="ofd-t" aria-describedby="ofd-d">
    <h2 id="ofd-t">Can't open this file</h2>
    <p id="ofd-d"><b>{f.name}</b><br />No application is set up to open it on this computer. Choose what to do instead.</p>
    {#if f.reason}<p class="why">{f.reason}</p>{/if}
    <div class="acts">
      <button class="primary" onclick={() => run(openWith, true)}><Icon name="external" size={18} /> Open with…</button>
      <button onclick={() => run(saveAs, true)}><Icon name="download" size={18} /> Save as…</button>
      <button onclick={() => run(showInFolder, false)}><Icon name="folder" size={18} /> Show in folder</button>
    </div>
    <button class="cancel" onclick={close}>Cancel</button>
  </div>
{/if}

<style>
  .back { position: fixed; inset: 0; z-index: 95; background: rgba(12, 18, 32, 0.55); }
  .dlg { position: fixed; z-index: 96; left: 50%; top: 50%; transform: translate(-50%, -50%); width: min(400px, calc(100vw - 32px)); background: var(--surface); border: 1px solid var(--line); border-radius: 16px; box-shadow: var(--shadow-pop); padding: 20px 20px 14px; display: flex; flex-direction: column; gap: 10px; }
  h2 { margin: 0; font-size: 18px; letter-spacing: -0.01em; }
  p { margin: 0; color: var(--text-2); font-size: 14.5px; line-height: 1.4; overflow-wrap: anywhere; }
  p b { color: var(--text); }
  .why { color: var(--muted); font-size: 12.5px; }
  .acts { display: flex; flex-direction: column; gap: 8px; margin-top: 6px; }
  .acts button { display: flex; align-items: center; gap: 10px; padding: 11px 14px; border-radius: 12px; background: var(--chip); font-weight: 600; font-size: 15px; text-align: left; }
  .acts button:hover { background: var(--hover); }
  .acts .primary { background: var(--accent); color: #fff; }
  .acts .primary:hover { filter: brightness(1.12); }
  .cancel { align-self: center; color: var(--muted); font-weight: 600; padding: 6px 12px; }
</style>
