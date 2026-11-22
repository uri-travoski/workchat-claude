<script lang="ts">
  import { app, fail, jumpTo, toast, userName } from "../lib/app.svelte";
  import { core } from "../lib/core";
  import { isImage } from "../lib/filetype";
  import { dayLabel, clock, fileSize } from "../lib/format";
  import { openFile } from "../lib/files";
  import { mediaUrl } from "../lib/media";
  import { registerLayer } from "../lib/overlays";
  import Icon from "./Icon.svelte";

  const v = $derived(app.viewer);
  const item = $derived(v ? v.items[v.index] : null);
  const count = $derived(v?.items.length ?? 0);

  let scale = $state(1);
  let tx = $state(0);
  let ty = $state(0);
  let chrome = $state(true);
  let stage = $state<HTMLDivElement>();
  const pointers = new Map<number, { x: number; y: number }>();
  let startDist = 0;
  let startScale = 1;
  let drag: { x: number; y: number; tx: number; ty: number } | null = null;
  let swipeX = $state(0);

  $effect(() => {
    item?.id;
    scale = 1;
    tx = ty = 0;
  });

  const close = () => (app.viewer = null);
  $effect(() => (v ? registerLayer(close) : undefined));
  function go(d: number) {
    if (!v) return;
    const i = v.index + d;
    if (i >= 0 && i < v.items.length) v.index = i;
  }
  function clamp() {
    if (scale <= 1) {
      tx = ty = 0;
      return;
    }
    const r = stage?.getBoundingClientRect();
    if (!r) return;
    const mx = (r.width * (scale - 1)) / 2;
    const my = (r.height * (scale - 1)) / 2;
    tx = Math.max(-mx, Math.min(mx, tx));
    ty = Math.max(-my, Math.min(my, ty));
  }
  function zoom(f: number) {
    scale = Math.max(1, Math.min(6, scale * f));
    clamp();
  }
  function wheel(e: WheelEvent) {
    if (!item || !isImage(item.content_type)) return;
    e.preventDefault();
    zoom(e.deltaY < 0 ? 1.15 : 1 / 1.15);
  }
  function dbl() {
    if (scale > 1) {
      scale = 1;
      tx = ty = 0;
    } else scale = 2.5;
  }
  function down(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      startDist = Math.hypot(a.x - b.x, a.y - b.y);
      startScale = scale;
      drag = null;
    } else {
      drag = { x: e.clientX, y: e.clientY, tx, ty };
      swipeX = 0;
    }
  }
  function move(e: PointerEvent) {
    if (!pointers.has(e.pointerId)) return;
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      scale = Math.max(1, Math.min(6, (startScale * Math.hypot(a.x - b.x, a.y - b.y)) / (startDist || 1)));
      clamp();
    } else if (drag) {
      const dx = e.clientX - drag.x;
      if (scale > 1) {
        tx = drag.tx + dx;
        ty = drag.ty + (e.clientY - drag.y);
        clamp();
      } else swipeX = dx;
    }
  }
  function up(e: PointerEvent) {
    pointers.delete(e.pointerId);
    if (pointers.size === 0 && drag) {
      if (scale <= 1 && Math.abs(swipeX) > 70) go(swipeX < 0 ? 1 : -1);
      else if (Math.abs(swipeX) < 6 && Math.abs(e.clientY - drag.y) < 6) {
        const t = e.target as HTMLElement;
        // A tap on the empty backdrop closes the viewer; a tap on the picture shows or hides the toolbar.
        if (t === stage) close();
        else if (!t.closest("video")) chrome = !chrome;
      }
      drag = null;
      swipeX = 0;
    }
  }
  function key(e: KeyboardEvent) {
    if (!v) return;
    if (e.key === "ArrowLeft") go(-1);
    else if (e.key === "ArrowRight") go(1);
    else if (e.key === "+" || e.key === "=") zoom(1.25);
    else if (e.key === "-") zoom(0.8);
  }

  async function save() {
    if (!item) return;
    try {
      toast(`Saved to ${await core.saveAttachment(item.id, item.name)}`);
    } catch (e) {
      fail(e);
    }
  }
  async function open() {
    if (item) await openFile(item);
  }
</script>

<svelte:window onkeydown={key} />
{#if v && item}
  <div class="viewer" role="dialog" aria-modal="true" aria-label="Media viewer">
    {#if chrome}
      <header>
        {#if app.phone}<button class="ib" aria-label="Close" onclick={close}><Icon name="left" size={26} stroke={2.2} /></button>{/if}
        <div class="ti">
          <b>{item.sender_id ? userName(item.sender_id) : item.name}</b>
          <span>{item.ts ? `${dayLabel(item.ts)}, ${clock(item.ts)}` : fileSize(item.size)} · {item.name}</span>
        </div>
        <span class="cnt">{v.index + 1} / {count}</span>
        {#if isImage(item.content_type) && !app.phone}
          <button class="ib" aria-label="Zoom in" onclick={() => zoom(1.4)}><Icon name="zoomin" size={22} /></button>
          <button class="ib" aria-label="Zoom out" onclick={() => zoom(1 / 1.4)}><Icon name="zoomout" size={22} /></button>
        {/if}
        {#if item.message_id && item.conv_id}<button class="ib" aria-label="Show in chat" title="Show in chat" onclick={() => jumpTo(item.conv_id!, item.message_id!)}><Icon name="chat" size={22} /></button>{/if}
        <button class="ib" aria-label="Save" title="Save to Downloads" onclick={save}><Icon name="download" size={22} /></button>
        <button class="ib" aria-label="Open in default app" title="Open in default app" onclick={open}><Icon name="external" size={22} /></button>
        {#if !app.phone}<button class="ib" aria-label="Close" title="Close" onclick={close}><Icon name="x" size={26} stroke={2.2} /></button>{/if}
      </header>
    {/if}
    <div class="stage" bind:this={stage} role="presentation" onwheel={wheel} ondblclick={dbl} onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up}>
      {#if isImage(item.content_type)}
        {#key item.id}
          <img src={mediaUrl(item.id)} alt={item.name} draggable="false" style="transform:translate({tx + (scale <= 1 ? swipeX : 0)}px,{ty}px) scale({scale})" />
        {/key}
      {:else}
        {#key item.id}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video src={mediaUrl(item.id)} controls autoplay poster={item.poster}></video>
        {/key}
      {/if}
    </div>
    {#if chrome && count > 1}
      {#if v.index > 0}<button class="nav l" aria-label="Previous" onclick={() => go(-1)}><Icon name="left" size={30} stroke={2.2} /></button>{/if}
      {#if v.index < count - 1}<button class="nav r" aria-label="Next" onclick={() => go(1)}><Icon name="right" size={30} stroke={2.2} /></button>{/if}
    {/if}
  </div>
{/if}

<style>
  .viewer { position: fixed; inset: 0; z-index: 80; background: #05080f; color: #fff; display: flex; flex-direction: column; }
  header { position: absolute; z-index: 2; left: 0; right: 0; top: 0; display: flex; align-items: center; gap: 2px; padding: calc(var(--safe-top) + 8px) 8px 18px 6px; background: linear-gradient(#05080fe6, #05080f00); }
  .ib { width: 42px; height: 42px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; color: #fff; flex: none; }
  .ib:hover { background: rgba(255, 255, 255, 0.14); }
  .ti { flex: 1; min-width: 0; display: flex; flex-direction: column; line-height: 1.25; padding-left: 4px; }
  .ti b { font-size: 16px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ti span { font-size: 12.5px; opacity: 0.75; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cnt { font-size: 13px; opacity: 0.8; padding: 0 8px; font-variant-numeric: tabular-nums; }
  .stage { flex: 1; min-height: 0; display: flex; align-items: center; justify-content: center; overflow: hidden; touch-action: none; user-select: none; }
  .stage img { max-width: 100%; max-height: 100%; object-fit: contain; transition: transform 0.05s linear; will-change: transform; }
  .stage video { max-width: 100%; max-height: 100%; outline: none; }
  .nav { position: absolute; top: 50%; width: 50px; height: 50px; margin-top: -25px; border-radius: 50%; background: rgba(255, 255, 255, 0.14); color: #fff; display: inline-flex; align-items: center; justify-content: center; z-index: 2; }
  .nav:hover { background: rgba(255, 255, 255, 0.28); }
  .nav.l { left: 14px; }
  .nav.r { right: 14px; }
</style>
