/** Pull down from the top of a view to reconnect to the server and reload. Works on touch screens. */
const PULL_PX = 70;

/** The nearest scrolling ancestor of `el`, up to `root`, or null when nothing scrolls. */
function scroller(el: Element | null, root: HTMLElement): HTMLElement | null {
  for (let cur: Element | null = el; cur && cur !== root.parentElement; cur = cur.parentElement) {
    const s = getComputedStyle(cur);
    if (/(auto|scroll)/.test(s.overflowY) && cur.scrollHeight > cur.clientHeight) return cur as HTMLElement;
  }
  return null;
}

export function pullRefresh(node: HTMLElement, run: () => unknown) {
  let startY = 0;
  let armed = false;
  let busy = false;

  function down(e: TouchEvent) {
    if (busy || e.touches.length !== 1) return;
    const sc = scroller(e.target as Element, node);
    armed = !sc || sc.scrollTop <= 0;
    startY = e.touches[0].clientY;
  }

  function up(e: TouchEvent) {
    if (!armed || busy) return;
    armed = false;
    const dy = (e.changedTouches[0]?.clientY ?? startY) - startY;
    if (dy < PULL_PX) return;
    busy = true;
    Promise.resolve()
      .then(run)
      .catch(() => {})
      .finally(() => (busy = false));
  }

  node.addEventListener("touchstart", down, { passive: true });
  node.addEventListener("touchend", up, { passive: true });
  return {
    destroy() {
      node.removeEventListener("touchstart", down);
      node.removeEventListener("touchend", up);
    },
  };
}
