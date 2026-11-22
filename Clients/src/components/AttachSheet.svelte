<script lang="ts">
  import { app, closeSheet } from "../lib/app.svelte";
  import { pickAndPreview, stageBrowserFiles } from "../lib/attach";
  import Icon from "./Icon.svelte";
  import Sheet from "./Sheet.svelte";

  let { conv, thread }: { conv: string; thread: string | null } = $props();
  let cam = $state<HTMLInputElement>();
  let vid = $state<HTMLInputElement>();

  async function fromInput(e: Event) {
    const el = e.currentTarget as HTMLInputElement;
    const files = Array.from(el.files ?? []);
    el.value = "";
    const c = conv;
    const t = thread;
    closeSheet();
    await stageBrowserFiles(files, c, t);
  }
  async function pick(kind: "media" | "any") {
    // The sheet's props vanish with it, so read them first.
    const c = conv;
    const t = thread;
    closeSheet();
    await pickAndPreview(c, t, kind);
  }
</script>

<Sheet title="Attach" onclose={closeSheet}>
  <div class="grid">
    <button onclick={() => pick("media")}><span class="ic" style="--c:#1e5bb8;--b:#dbe8ff"><Icon name="image" size={26} /></span>Photos &amp; videos</button>
    <button onclick={() => pick("any")}><span class="ic" style="--c:#c2570f;--b:#ffe5d0"><Icon name="file" size={26} /></span>Document</button>
    {#if app.phone}
      <button onclick={() => cam?.click()}><span class="ic" style="--c:#1a7f4b;--b:#d8f2e4"><Icon name="camera" size={26} /></span>Camera photo</button>
      <button onclick={() => vid?.click()}><span class="ic" style="--c:#c4261f;--b:#fde4e4"><Icon name="video" size={26} /></span>Camera video</button>
    {/if}
  </div>
  <p class="hint">Any file type, up to {Math.round(app.maxUpload / 1048576)} MB each. You can also drag files into the window or paste an image.</p>
  <input bind:this={cam} type="file" accept="image/*" capture="environment" hidden onchange={fromInput} />
  <input bind:this={vid} type="file" accept="video/*" capture="environment" hidden onchange={fromInput} />
</Sheet>

<style>
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(120px, 1fr)); gap: 12px; padding-top: 6px; }
  .grid button { display: flex; flex-direction: column; align-items: center; gap: 9px; padding: 14px 6px; border-radius: 14px; background: var(--surface-2); border: 1px solid var(--line); font-weight: 600; font-size: 14.5px; }
  .grid button:hover { background: var(--hover); }
  .ic { width: 54px; height: 54px; border-radius: 16px; background: var(--b); color: var(--c); display: inline-flex; align-items: center; justify-content: center; }
  .hint { color: var(--muted); font-size: 13px; text-align: center; margin: 14px 4px 0; }
</style>
