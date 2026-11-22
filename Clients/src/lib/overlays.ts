// One place decides what Escape (and the system Back button) closes. Every overlay (viewer, sheet, modal, menu, emoji
// picker, PIN pad) registers a close function while it is open; Escape closes only the top-most one and is then
// swallowed, so it can never reach another handler (which used to close two layers at once) and never means "quit".

interface Layer {
  close: () => void;
}
const stack: Layer[] = [];

/** Registers an open overlay. Returns the function that unregisters it (call it when the overlay goes away). */
export function registerLayer(close: () => void): () => void {
  const l: Layer = { close };
  stack.push(l);
  return () => {
    const i = stack.indexOf(l);
    if (i >= 0) stack.splice(i, 1);
  };
}

export const layerCount = () => stack.length;

/** Closes the top-most overlay. Returns false when none is open. */
export function closeTopLayer(): boolean {
  const l = stack[stack.length - 1];
  if (!l) return false;
  l.close();
  return true;
}

/** Window-level Escape handler (capture phase). Returns the remover. */
export function installOverlayKeys(): () => void {
  const onKey = (e: KeyboardEvent) => {
    if (e.key !== "Escape" || e.defaultPrevented) return;
    // Escape is never a quit shortcut: stop the browser and the shell from acting on it.
    if (closeTopLayer()) {
      e.preventDefault();
      e.stopImmediatePropagation();
    }
  };
  window.addEventListener("keydown", onKey, true);
  return () => window.removeEventListener("keydown", onKey, true);
}
