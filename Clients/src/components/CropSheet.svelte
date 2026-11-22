<script lang="ts">
  import { app, closeSheet, fail, toast } from "../lib/app.svelte";
  import { core } from "../lib/core";
  import { mediaUrl } from "../lib/media";
  import Sheet from "./Sheet.svelte";

  let { sheet }: { sheet: Extract<NonNullable<typeof app.sheet>, { kind: "crop" }> } = $props();
  const V = 260; // the circle's diameter in CSS pixels
  let img = $state<HTMLImageElement>();
  let iw = $state(0);
  let ih = $state(0);
  let zoom = $state(1);
  let ox = $state(0);
  let oy = $state(0);
  let busy = $state(false);
  let drag: { x: number; y: number; ox: number; oy: number } | null = null;

  const base = $derived(iw && ih ? V / Math.min(iw, ih) : 1);
  const k = $derived(base * zoom);
  function clamp() {
    const mx = Math.max(0, (iw * k - V) / 2);
    const my = Math.max(0, (ih * k - V) / 2);
    ox = Math.max(-mx, Math.min(mx, ox));
    oy = Math.max(-my, Math.min(my, oy));
  }
  $effect(() => {
    zoom;
    iw;
    clamp();
  });
  function down(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { x: e.clientX, y: e.clientY, ox, oy };
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    ox = drag.ox + e.clientX - drag.x;
    oy = drag.oy + e.clientY - drag.y;
    clamp();
  }
  const up = () => (drag = null);
  function wheel(e: WheelEvent) {
    e.preventDefault();
    zoom = Math.max(1, Math.min(5, zoom * (e.deltaY < 0 ? 1.08 : 0.93)));
  }

  async function done() {
    if (!img || busy) return;
    busy = true;
    try {
      const out = 512;
      const size = V / k; // the visible square in source pixels
      const sx = iw / 2 - ox / k - size / 2;
      const sy = ih / 2 - oy / k - size / 2;
      const c = document.createElement("canvas");
      c.width = c.height = out;
      const g = c.getContext("2d")!;
      g.fillStyle = "#fff";
      g.fillRect(0, 0, out, out);
      g.drawImage(img, sx, sy, size, size, 0, 0, out, out);
      const blob: Blob = await new Promise((res, rej) => c.toBlob((b) => (b ? res(b) : rej(new Error("Could not prepare the picture."))), "image/jpeg", 0.88));
      const st = await core.stageBlob(blob, "avatar.jpg");
      const t = sheet.target;
      if (t.kind === "me") await core.setAvatar("me", null, st.id);
      else if (t.kind === "conv") await core.setAvatar("conv", t.id, st.id);
      else if (t.kind === "local") await core.setLocalAvatar(t.what === "user" ? "user" : "conv", t.id, st.id);
      else {
        app.chatDraft.avatarId = st.id;
        void core.unstage(sheet.stagedId).catch(() => {});
        app.sheet = { kind: "newchat", step: "channel" };
        return;
      }
      void core.unstage(sheet.stagedId).catch(() => {});
      void core.unstage(st.id).catch(() => {});
      toast(t.kind === "local" ? "Local picture set. Only you see it." : "Picture updated");
      closeSheet();
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }
  function cancel() {
    void core.unstage(sheet.stagedId).catch(() => {});
    if (sheet.target.kind === "newchat") app.sheet = { kind: "newchat", step: "channel" };
    else closeSheet();
  }
</script>

<Sheet title="Crop picture" onclose={cancel}>
  <div class="stage" role="presentation" style="--v:{V}px" onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up} onwheel={wheel}>
    <img bind:this={img} crossorigin="anonymous" src={mediaUrl(sheet.stagedId)} alt="" draggable="false" onload={(e) => { const i = e.currentTarget as HTMLImageElement; iw = i.naturalWidth; ih = i.naturalHeight; }} style="width:{iw * k}px;height:{ih * k}px;transform:translate(calc(-50% + {ox}px),calc(-50% + {oy}px))" />
    <span class="ring"></span>
  </div>
  <label class="z">Zoom <input type="range" min="1" max="5" step="0.01" bind:value={zoom} aria-label="Zoom" /></label>
  {#snippet footer()}
    <button class="ghost" onclick={cancel}>Cancel</button>
    <button class="ok" onclick={done} disabled={busy || !iw}>{busy ? "Saving…" : "Use picture"}</button>
  {/snippet}
</Sheet>

<style>
  .stage { position: relative; width: var(--v); height: var(--v); margin: 8px auto 4px; overflow: hidden; border-radius: 50%; background: #0c1220; touch-action: none; cursor: grab; }
  .stage img { position: absolute; left: 50%; top: 50%; max-width: none; user-select: none; }
  .ring { position: absolute; inset: 0; border-radius: 50%; box-shadow: inset 0 0 0 2px rgba(255, 255, 255, 0.85); pointer-events: none; }
  .z { display: flex; align-items: center; gap: 12px; margin: 12px 8px 0; color: var(--muted); font-size: 14px; }
  .z input { flex: 1; }
  .ghost { padding: 10px 16px; font-weight: 600; color: var(--text-2); margin-left: auto; }
  .ok { background: linear-gradient(145deg, var(--accent), var(--accent-2)); color: #fff; font-weight: 700; border-radius: 20px; padding: 10px 22px; }
  .ok:disabled { opacity: 0.55; }
</style>
