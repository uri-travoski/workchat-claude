<script lang="ts">
  import { app } from "../lib/app.svelte";
  import type { Attachment } from "../lib/core";
  import { duration as fmt } from "../lib/format";
  import { cycleSpeed, isCurrent, isPlaying, seek, togglePlay } from "../lib/player.svelte";
  import { openFile } from "../lib/files";
  import Icon from "./Icon.svelte";

  let { a, voice = true, mine = false }: { a: Attachment; voice?: boolean; mine?: boolean } = $props();

  const BARS = 44;
  const cur = $derived(isCurrent(a.id));
  const playing = $derived(isPlaying(a.id));
  const loading = $derived(app.audio.id === a.id && app.audio.state === "loading");
  const total = $derived(cur && app.audio.dur ? app.audio.dur : (a.meta?.duration_ms ?? 0));
  const pos = $derived(cur ? Math.min(app.audio.pos, total || app.audio.pos) : 0);
  const frac = $derived(total ? Math.min(1, pos / total) : 0);

  // 64 loudness values from the sender become BARS bars; without them a calm placeholder is drawn.
  const bars = $derived.by(() => {
    const p = a.meta?.peaks ?? [];
    if (!p.length) return Array.from({ length: BARS }, (_, i) => 70 + ((i * 37) % 60));
    return Array.from({ length: BARS }, (_, i) => {
      const lo = Math.floor((i * p.length) / BARS);
      const hi = Math.max(lo + 1, Math.floor(((i + 1) * p.length) / BARS));
      let m = 0;
      for (let k = lo; k < hi; k++) m = Math.max(m, p[k]);
      return m;
    });
  });

  let waveEl = $state<HTMLDivElement>();
  let scrub = $state<number | null>(null);

  function fracAt(e: PointerEvent) {
    const r = waveEl!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
  }
  function down(e: PointerEvent) {
    if (!total) return;
    waveEl?.setPointerCapture(e.pointerId);
    scrub = fracAt(e);
  }
  function move(e: PointerEvent) {
    if (scrub !== null) scrub = fracAt(e);
  }
  function up(e: PointerEvent) {
    if (scrub === null) return;
    const f = fracAt(e);
    scrub = null;
    void seek(a.id, Math.round(f * total), total);
  }
  const shown = $derived(scrub ?? frac);
  const label = $derived(cur && (playing || app.audio.state === "paused") ? `${fmt(scrub !== null ? scrub * total : pos)}` : total ? fmt(total) : "0:00");
</script>

<div class="vb" class:mine class:voice>
  <button class="play" aria-label={playing ? "Pause" : "Play"} onclick={() => togglePlay(a.id)}>
    {#if loading}<span class="spin"></span>{:else if playing}<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="5" width="4.2" height="14" rx="1.2" /><rect x="13.8" y="5" width="4.2" height="14" rx="1.2" /></svg>{:else}<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.4v13.2a1 1 0 0 0 1.5.86l10.6-6.6a1 1 0 0 0 0-1.72L9.5 4.54A1 1 0 0 0 8 5.4Z" /></svg>{/if}
  </button>
  <div class="mid">
    {#if !voice}<span class="name">{a.name}</span>{/if}
    <div class="wave" bind:this={waveEl} role="slider" tabindex="0" aria-label="Seek" aria-valuemin="0" aria-valuemax={total} aria-valuenow={Math.round(shown * total)} onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={() => (scrub = null)}>
      {#each bars as b, i}
        <i class:on={(i + 0.5) / BARS <= shown} style="height:{4 + (b / 255) * 22}px"></i>
      {/each}
      <span class="knob" style="left:{shown * 100}%"></span>
    </div>
    <div class="meta"><span class="t">{label}</span>{#if cur && app.audio.state !== "ended"}<button class="spd" aria-label="Playback speed" onclick={cycleSpeed}>{app.audio.speed === 1 ? "1x" : app.audio.speed + "x"}</button>{/if}</div>
  </div>
  {#if voice}<span class="micico"><Icon name="mic" size={16} /></span>{:else}<button class="opn" aria-label={`Open ${a.name} in default app`} title="Open in default app" onclick={() => void openFile(a)}><Icon name="external" size={17} /></button>{/if}
</div>

<style>
  .vb { display: flex; align-items: center; gap: 10px; min-width: 232px; max-width: 100%; padding: 2px 0 0; }
  .play { width: 40px; height: 40px; border-radius: 50%; flex: none; display: inline-flex; align-items: center; justify-content: center; background: var(--accent); color: #fff; }
  .mine .play { background: rgba(255, 255, 255, 0.22); color: #fff; }
  .mid { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .opn { width: 32px; height: 32px; border-radius: 50%; flex: none; display: inline-flex; align-items: center; justify-content: center; color: inherit; opacity: 0.75; }
  .opn:hover { opacity: 1; background: rgba(127, 127, 127, 0.18); }
  .name { font-size: 14px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .wave { position: relative; display: flex; align-items: center; gap: 2px; height: 30px; cursor: pointer; touch-action: none; }
  .wave i { flex: 1; min-width: 2px; border-radius: 2px; background: color-mix(in srgb, var(--muted-2) 80%, transparent); }
  .wave i.on { background: var(--blue); }
  .mine .wave i { background: rgba(255, 255, 255, 0.4); }
  .mine .wave i.on { background: #fff; }
  .knob { position: absolute; top: 50%; width: 10px; height: 10px; margin: -5px 0 0 -5px; border-radius: 50%; background: var(--blue); pointer-events: none; }
  .mine .knob { background: #fff; }
  .meta { display: flex; align-items: center; justify-content: space-between; font-size: 12.5px; color: var(--muted); }
  .mine .meta { color: var(--bubble-out-time); }
  .t { font-variant-numeric: tabular-nums; }
  .spd { font-size: 11.5px; font-weight: 700; padding: 1px 8px; border-radius: 10px; background: color-mix(in srgb, var(--blue) 14%, transparent); color: var(--blue); }
  .mine .spd { background: rgba(255, 255, 255, 0.2); color: #fff; }
  .micico { color: var(--blue); display: inline-flex; margin-top: 2px; }
  .mine .micico { color: var(--bubble-out-time); }
  .spin { width: 18px; height: 18px; border-radius: 50%; border: 2.5px solid rgba(255, 255, 255, 0.35); border-top-color: #fff; animation: sp 0.8s linear infinite; }
  @keyframes sp { to { transform: rotate(360deg); } }
</style>
