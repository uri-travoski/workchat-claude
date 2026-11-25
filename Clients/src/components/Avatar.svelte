<script lang="ts">
  import { mediaUrl } from "../lib/media";
  import Icon from "./Icon.svelte";

  let {
    id = null,
    initials = "?",
    size = 52,
    tile = false,
    icon = "",
    presence = null,
    bot = false,
    conn = null,
  }: { id?: string | null; initials?: string; size?: number; tile?: boolean; icon?: string; presence?: string | null; bot?: boolean; conn?: "online" | "offline" | null } = $props();
  // A channel (tile) or a bot account shows a small badge in the bottom corner.
  // Channels already show "#" in the avatar, so only bots get a corner badge.
  const badge = $derived(bot ? "bot" : "");

  let broken = $state(false);
  $effect(() => {
    id;
    broken = false;
  });
</script>

<span class="av" class:tile style="--s:{size}px">
  {#if id && !broken}
    <img src={mediaUrl(id)} alt="" loading="lazy" onerror={() => (broken = true)} />
  {:else if tile}
    <span class="tile-ico"><Icon name={icon || "hash"} size={Math.round(size * 0.46)} stroke={1.7} /></span>
  {:else}
    <span class="ini" style="font-size:{Math.round(size * 0.36)}px">{initials}</span>
  {/if}
  {#if badge}<span class="badge" class:bot={badge === "bot"} aria-hidden="true" title={badge === "bot" ? "Bot" : "Channel"}><Icon name={badge} size={Math.max(9, Math.round(size * 0.22))} /></span>{/if}
  {#if presence === "online" && !badge}<span class="dot" aria-hidden="true"></span>{/if}
  {#if conn && !badge}<span class="dot" class:off={conn === "offline"} title={conn === "online" ? "Connected to the server" : "Offline"} aria-hidden="true"></span>{/if}
</span>

<style>
  .av {
    position: relative;
    display: inline-flex;
    flex: none;
    width: var(--s);
    height: var(--s);
    border-radius: 50%;
    background: var(--initials-bg);
    color: var(--initials-text);
    align-items: center;
    justify-content: center;
    font-weight: 600;
  }
  .av.tile {
    border-radius: calc(var(--s) * 0.27);
    background: linear-gradient(145deg, var(--accent), var(--accent-2));
    color: #fff;
  }
  img {
    width: 100%;
    height: 100%;
    border-radius: 50%;
    object-fit: cover;
    display: block;
  }
  .av.tile img { border-radius: calc(var(--s) * 0.27); }
  .tile-ico, .ini { display: inline-flex; align-items: center; justify-content: center; letter-spacing: 0.02em; }
  .badge { position: absolute; right: -3px; bottom: -3px; width: calc(var(--s) * 0.38); height: calc(var(--s) * 0.38); min-width: 15px; min-height: 15px; border-radius: 50%; background: var(--surface); color: var(--navy); border: 1.5px solid var(--line); display: inline-flex; align-items: center; justify-content: center; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.18); }
  .badge.bot { color: var(--accent); }
  .dot.off { background: #e5484d; }
  .dot {
    position: absolute;
    right: -1px;
    bottom: -1px;
    width: calc(var(--s) * 0.26);
    height: calc(var(--s) * 0.26);
    border-radius: 50%;
    background: var(--online);
    border: 2px solid var(--surface);
  }
</style>
