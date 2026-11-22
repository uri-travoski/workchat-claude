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
  }: { id?: string | null; initials?: string; size?: number; tile?: boolean; icon?: string; presence?: string | null } = $props();

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
  {#if presence === "online"}<span class="dot" aria-hidden="true"></span>{/if}
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
    border-radius: inherit;
    object-fit: cover;
    display: block;
  }
  .tile-ico, .ini { display: inline-flex; align-items: center; justify-content: center; letter-spacing: 0.02em; }
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
