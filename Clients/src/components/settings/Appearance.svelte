<script lang="ts">
  import { app, prefs, savePrefs } from "../../lib/app.svelte";
  import Icon from "../Icon.svelte";
  import PageHead from "./PageHead.svelte";

  const accents: { id: typeof prefs.accent; label: string; color: string }[] = [
    { id: "navy", label: "Navy Classic", color: "#21375b" },
    { id: "blue", label: "Blue", color: "#1e5bb8" },
    { id: "slate", label: "Slate", color: "#4a5f85" },
    { id: "deep", label: "Deep navy", color: "#1c3a55" },
  ];
  const accentLabel = $derived(accents.find((a) => a.id === prefs.accent)?.label ?? "");
  const radiusLabel = $derived(prefs.bubbleRadius <= 6 ? "Small" : prefs.bubbleRadius <= 14 ? "Medium" : "Large");
</script>

<div class="st-page">
  <PageHead title="Chat Settings &amp; Theme" />
  <div class="st-body scroll" class:desk={!app.phone}>
    <h3 class="st-cap">Appearance &amp; theme</h3>
    <section class="st-card">
      <div class="st-blk">
        <b class="st-lbl">Theme Mode</b>
        <div class="st-seg" role="radiogroup" aria-label="Theme mode">
          {#each [["light", "Light"], ["dark", "Dark"], ["system", "System"]] as [id, label]}
            <button role="radio" aria-checked={prefs.theme === id} class:on={prefs.theme === id} onclick={() => { prefs.theme = id as typeof prefs.theme; savePrefs(); }}>{#if prefs.theme === id}<Icon name="check" size={14} stroke={2.6} />{/if}{label}</button>
          {/each}
        </div>
      </div>
      <div class="st-blk">
        <div class="st-lrow"><b class="st-lbl">Primary Accent Color</b><span class="st-val mute">{accentLabel}</span></div>
        <div class="sw">
          {#each accents as a}
            <button class="swatch" class:on={prefs.accent === a.id} style="--c:{a.color}" aria-label={a.label} aria-pressed={prefs.accent === a.id} onclick={() => { prefs.accent = a.id; savePrefs(); }}>{#if prefs.accent === a.id}<Icon name="check" size={20} stroke={2.6} />{/if}</button>
          {/each}
        </div>
      </div>
      <div class="st-blk">
        <div class="st-lrow"><b class="st-lbl">Chat Bubble Corner Radius</b><span class="st-val mute">{radiusLabel} ({prefs.bubbleRadius}px)</span></div>
        <input type="range" min="2" max="22" step="1" bind:value={prefs.bubbleRadius} oninput={savePrefs} aria-label="Bubble corner radius" />
        <div class="demo"><span class="in">Is the sprint plan ready?</span><span class="out" style="font-size:{prefs.textSize}px">Executive sprint sync at 2 PM.</span></div>
      </div>
      <div class="st-blk">
        <div class="st-lrow"><b class="st-lbl">Message Text Size</b><span class="st-val mute">{prefs.textSize} pt{prefs.textSize === 16 ? " (Default)" : ""}</span></div>
        <div class="szrow"><span class="a sm">A</span><input type="range" min="13" max="22" step="1" bind:value={prefs.textSize} oninput={savePrefs} aria-label="Message text size" /><span class="a lg">A</span></div>
      </div>
    </section>
    <p class="st-note rst"><button class="st-btn" onclick={() => { prefs.theme = "system"; prefs.accent = "navy"; prefs.bubbleRadius = 12; prefs.textSize = 16; savePrefs(); }}>Reset to defaults</button></p>
  </div>
</div>

<style>
  .sw { display: flex; justify-content: space-between; padding: 2px 4px; max-width: 340px; }
  .swatch { width: 44px; height: 44px; border-radius: 50%; background: var(--c); color: #fff; display: inline-flex; align-items: center; justify-content: center; }
  .swatch.on { outline: 2px solid var(--c); outline-offset: 3px; }
  input[type="range"] { width: 100%; accent-color: var(--accent); }
  .demo { background: var(--surface-2); border-radius: 10px; padding: 10px; display: flex; flex-direction: column; gap: 6px; align-items: flex-end; }
  .demo span { padding: 9px 14px; border-radius: var(--radius-bubble); font-size: 14.5px; }
  .demo .out { background: linear-gradient(135deg, var(--accent), var(--accent-2)); color: #fff; }
  .demo .in { background: var(--bubble-in); color: var(--bubble-in-text); align-self: flex-start; box-shadow: var(--shadow); }
  .szrow { display: flex; align-items: center; gap: 12px; }
  .a { font-weight: 800; color: var(--text-2); }
  .a.sm { font-size: 13px; }
  .a.lg { font-size: 22px; color: var(--text); }
  .rst { text-align: center; margin-top: 16px; }
</style>
