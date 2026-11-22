import type { ChatRow } from "./core";

/** A deterministic icon for a channel tile, picked from its name (channels have no icon of their own). */
export function channelIcon(name: string): string {
  const n = name.toLowerCase();
  if (/announce|news|town|broadcast/.test(n)) return "megaphone";
  if (/deploy|engineer|dev|code|ops|infra|build|release/.test(n)) return "terminal";
  if (/product|strategy|roadmap|sales|metric|growth|finance|report/.test(n)) return "trend";
  return "hash";
}

export const isDirect = (r: ChatRow) => r.kind === "dm" || r.kind === "self" || r.kind === "group_dm";

/** Order inside a section: the person's manual order first, then newest activity. */
export function byActivity(a: ChatRow, b: ChatRow): number {
  const pa = a.position ?? Number.MAX_SAFE_INTEGER;
  const pb = b.position ?? Number.MAX_SAFE_INTEGER;
  if (pa !== pb) return pa - pb;
  return b.last_ts - a.last_ts;
}

export function matches(r: ChatRow, q: string): boolean {
  const n = q.trim().toLowerCase();
  if (!n) return true;
  return r.title.toLowerCase().includes(n) || r.preview.text.toLowerCase().includes(n);
}
