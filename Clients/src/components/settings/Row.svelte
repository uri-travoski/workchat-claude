<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "../Icon.svelte";

  let { icon, tone = "", title, sub = "", value = "", badge = "", chev = true, onclick, children }: { icon: string; tone?: string; title: string; sub?: string; value?: string; badge?: string; chev?: boolean; onclick?: () => void; children?: Snippet } = $props();
</script>

{#if onclick}
  <button class="st-row" {onclick}>
    <span class="st-ic {tone}"><Icon name={icon} size={19} /></span>
    <span class="st-tx"><b>{title}</b>{#if sub}<small>{sub}</small>{/if}</span>
    {#if children}{@render children()}{/if}
    {#if value}<span class="st-val" class:mute={!!sub}>{value}</span>{/if}
    {#if badge}<span class="st-dot">{badge}</span>{/if}
    {#if chev}<span class="st-chev"><Icon name="right" size={18} /></span>{/if}
  </button>
{:else}
  <div class="st-row">
    <span class="st-ic {tone}"><Icon name={icon} size={19} /></span>
    <span class="st-tx"><b>{title}</b>{#if sub}<small>{sub}</small>{/if}</span>
    {#if children}{@render children()}{/if}
    {#if value}<span class="st-val">{value}</span>{/if}
  </div>
{/if}
