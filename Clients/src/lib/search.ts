// Pure helpers of the top-bar global search.
import type { FileHit, User } from "./api-types";

/** Splits a server snippet on its `[[match]]` markers. */
export function snippetParts(s: string): { t: string; hit: boolean }[] {
  const out: { t: string; hit: boolean }[] = [];
  let i = 0;
  for (const m of s.matchAll(/\[\[(.*?)\]\]/g)) {
    if (m.index! > i) out.push({ t: s.slice(i, m.index), hit: false });
    out.push({ t: m[1], hit: true });
    i = m.index! + m[0].length;
  }
  if (i < s.length) out.push({ t: s.slice(i), hit: false });
  return out;
}

/** Search hits plus directory matches, without duplicates, the search's own order first. Disabled accounts and `selfId` are left out. */
export function mergePeople(a: User[], b: User[], selfId?: string): User[] {
  const seen = new Set<string>();
  const out: User[] = [];
  for (const u of [...a, ...b]) {
    if (seen.has(u.id) || u.disabled || u.id === selfId) continue;
    seen.add(u.id);
    out.push(u);
  }
  return out;
}

export function mergeFiles(a: FileHit[], b: FileHit[]): FileHit[] {
  const seen = new Set<string>();
  return [...a, ...b].filter((f) => !seen.has(f.attachment.id) && !!seen.add(f.attachment.id));
}

export type GItem = { kind: "message"; key: string } | { kind: "person"; key: string } | { kind: "file"; key: string };

/** Moves the highlighted row by `d` (wraps around). -1 means nothing is highlighted yet. */
export function stepIndex(cur: number, d: number, n: number): number {
  if (n <= 0) return -1;
  if (cur < 0) return d > 0 ? 0 : n - 1;
  return (cur + d + n) % n;
}
