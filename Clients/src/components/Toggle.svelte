<script lang="ts">
  let { checked = $bindable(false), label, onchange }: { checked?: boolean; label: string; onchange?: () => void } = $props();
</script>

<button class="tg" role="switch" aria-checked={checked} aria-label={label} class:on={checked} onclick={() => { checked = !checked; onchange?.(); }}><i></i></button>

<style>
  .tg { width: 52px; height: 30px; border-radius: 15px; background: var(--chip); border: 1px solid var(--line); position: relative; flex: none; transition: background 0.15s; }
  .tg.on { background: var(--accent); border-color: var(--accent); }
  i { position: absolute; top: 2px; left: 2px; width: 24px; height: 24px; border-radius: 50%; background: #fff; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3); transition: transform 0.15s; }
  .tg.on i { transform: translateX(22px); }
</style>
