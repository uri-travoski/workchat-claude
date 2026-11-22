<script lang="ts">
  import { LABEL_COLORS } from "../lib/labels";
  import Icon from "./Icon.svelte";

  // The palette a label colour is picked from.
  let { value = $bindable(), label = "Colour" }: { value: string; label?: string } = $props();
  const same = (c: string) => c.toLowerCase() === value.toLowerCase();
</script>

<div class="sw" role="radiogroup" aria-label={label}>
  {#each LABEL_COLORS as c (c)}
    <button type="button" role="radio" aria-checked={same(c)} aria-label={c} style="background:{c}" onclick={() => (value = c)}>{#if same(c)}<Icon name="check" size={17} stroke={3} />{/if}</button>
  {/each}
</div>

<style>
  .sw { display: flex; flex-wrap: wrap; gap: 10px; }
  button { width: 36px; height: 36px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; color: #fff; }
  button[aria-checked="true"] { box-shadow: 0 0 0 2px var(--surface), 0 0 0 4px var(--muted-2); }
</style>
