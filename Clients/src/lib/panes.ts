// Resizable desktop panes: the chat list column and the thread pane. Widths are remembered on this device.
export const RAIL = 72;
export const LIMITS = {
  list: { min: 260, max: 560, def: 384 },
  thread: { min: 280, max: 640, def: 360 },
} as const;
/** The conversation always keeps at least this much room. */
export const MAIN_MIN = 380;

export type Pane = keyof typeof LIMITS;
export interface Widths {
  list: number;
  thread: number;
}
const KEY = "workchat.panes";

export const clamp = (v: number, min: number, max: number) => Math.max(min, Math.min(max, v));

/** Largest width `pane` may take in a window `win` wide, with the other pane (when shown) at its current width. */
export function maxFor(pane: Pane, w: Widths, win: number, threadShown: boolean): number {
  const other = pane === "list" ? (threadShown ? w.thread : 0) : w.list;
  const room = win - RAIL - other - MAIN_MIN;
  return Math.max(LIMITS[pane].min, Math.min(LIMITS[pane].max, room));
}

/** Sets one pane's width within its limits and the room that is left. */
export function resize(w: Widths, pane: Pane, want: number, win: number, threadShown: boolean): Widths {
  const v = clamp(Math.round(want), LIMITS[pane].min, maxFor(pane, w, win, threadShown));
  return { ...w, [pane]: v };
}

/** Shrinks stored widths so they fit a small window (keeps the minimums). */
export function fit(w: Widths, win: number, threadShown: boolean): Widths {
  let out = { ...w };
  if (threadShown) out = resize(out, "thread", out.thread, win, true);
  out = resize(out, "list", out.list, win, threadShown);
  return out;
}

export const defaults = (): Widths => ({ list: LIMITS.list.def, thread: LIMITS.thread.def });

export function load(): Widths {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (raw && typeof raw.list === "number" && typeof raw.thread === "number") {
      return { list: clamp(raw.list, LIMITS.list.min, LIMITS.list.max), thread: clamp(raw.thread, LIMITS.thread.min, LIMITS.thread.max) };
    }
  } catch {
    /* none saved */
  }
  return defaults();
}

export function save(w: Widths) {
  try {
    localStorage.setItem(KEY, JSON.stringify(w));
  } catch {
    /* private mode */
  }
}
