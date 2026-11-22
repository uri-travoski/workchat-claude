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

/** The list box filters by conversation name only (not by message text), case-insensitively. */
export function nameMatches(r: ChatRow, q: string): boolean {
  const n = q.trim().toLowerCase();
  return !n || r.title.toLowerCase().includes(n);
}

export type ListKind = "all" | "dm" | "channel";

/** Which chats a rail item shows: All Chats everything, DMs direct and group DMs, Channels channels. */
export function inKind(r: ChatRow, k: ListKind): boolean {
  return k === "all" || (k === "dm" ? isDirect(r) : r.kind === "channel");
}

/** Unpinned chats: newest activity first (default), or by name A to Z / Z to A. */
export function sortChats(rows: ChatRow[], sort: "recent" | "az" | "za"): ChatRow[] {
  const by = (a: ChatRow, b: ChatRow) => a.title.localeCompare(b.title, undefined, { sensitivity: "base" });
  const out = [...rows];
  if (sort === "az") return out.sort((a, b) => by(a, b) || b.last_ts - a.last_ts);
  if (sort === "za") return out.sort((a, b) => by(b, a) || b.last_ts - a.last_ts);
  return out.sort((a, b) => b.last_ts - a.last_ts);
}

/** Moves `id` so it ends up before the item at `to` (0..length, an insertion slot), returning the new id order. */
export function moveId(ids: string[], id: string, to: number): string[] {
  const from = ids.indexOf(id);
  if (from < 0) return ids;
  const out = ids.filter((x) => x !== id);
  out.splice(to > from ? to - 1 : to, 0, id);
  return out;
}

/** The insertion slot for a pointer at `y`, given each row's vertical middle (top to bottom). */
export function dropSlot(mids: number[], y: number): number {
  let i = 0;
  while (i < mids.length && y > mids[i]) i++;
  return i;
}

/** Name or last-message text matches (forward picker). */
export function matches(r: ChatRow, q: string): boolean {
  const n = q.trim().toLowerCase();
  if (!n) return true;
  return r.title.toLowerCase().includes(n) || r.preview.text.toLowerCase().includes(n);
}

/** Unread count of a set of chats (muted chats and marked-unread dots count as in the bottom bar). */
export const unreadOf = (rows: ChatRow[]) => rows.filter((r) => !r.muted && !r.archived).reduce((n, r) => n + (r.unread > 0 ? r.unread : r.marked_unread ? 1 : 0), 0);
