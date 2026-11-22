<script lang="ts">
  // A draggable divider between two panes. `edge` says which side the controlled pane is on: dragging right grows a
  // "left" pane and shrinks a "right" one. Arrow keys nudge it, double click resets to the default width.
  let { value, min, max, edge, label, onchange, onreset, ondone }: { value: number; min: number; max: number; edge: "left" | "right"; label: string; onchange: (v: number) => void; onreset: () => void; ondone: () => void } = $props();
  let drag = $state<{ x: number; v: number } | null>(null);

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault(); // no text selection while dragging
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { x: e.clientX, v: value };
    document.body.classList.add("resizing");
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    const dx = e.clientX - drag.x;
    onchange(drag.v + (edge === "left" ? dx : -dx));
  }
  function up() {
    if (!drag) return;
    drag = null;
    document.body.classList.remove("resizing");
    ondone();
  }
  function key(e: KeyboardEvent) {
    const step = e.shiftKey ? 40 : 12;
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      e.preventDefault();
      const d = (e.key === "ArrowRight" ? 1 : -1) * (edge === "left" ? 1 : -1);
      onchange(value + d * step);
      ondone();
    } else if (e.key === "Home" || e.key === "Enter") {
      e.preventDefault();
      onreset();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="split"
  class:drag={!!drag}
  class:at-left={edge === "left"}
  class:at-right={edge === "right"}
  role="separator"
  aria-orientation="vertical"
  aria-label={label}
  aria-valuenow={Math.round(value)}
  aria-valuemin={min}
  aria-valuemax={max}
  tabindex="0"
  title="Drag to resize, double-click to reset"
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  ondblclick={onreset}
  onkeydown={key}
></div>

<style>
  .split { position: relative; width: 0; z-index: 5; cursor: col-resize; touch-action: none; outline: none; }
  /* A wide invisible hit area centred on the divider line, plus a thin highlight when hovered or dragged. */
  .split::before { content: ""; position: absolute; top: 0; bottom: 0; left: -4px; width: 9px; }
  .split::after { content: ""; position: absolute; top: 0; bottom: 0; left: -1px; width: 3px; background: transparent; transition: background 0.12s; }
  .split:hover::after, .split:focus-visible::after, .split.drag::after { background: var(--blue); opacity: 0.7; }
</style>
