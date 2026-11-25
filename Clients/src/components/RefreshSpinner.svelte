<script lang="ts">
  // A round spinner at the top of the screen while a pull-to-refresh is running (see refreshNow in app.svelte.ts).
</script>

<div class="spin" role="status" aria-live="polite" aria-label="Refreshing">
  <span class="ring"></span>
</div>

<style>
  .spin {
    position: fixed;
    top: calc(var(--safe-top) + 64px);
    left: 50%;
    transform: translateX(-50%);
    z-index: 60;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: var(--surface);
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.18);
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
  }
  .ring {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 3px solid var(--line);
    border-top-color: var(--blue);
    box-sizing: border-box;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .ring {
      animation-duration: 2.4s;
    }
  }
</style>
