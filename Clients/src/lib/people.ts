/** Up to two initials from a display name. Same rule as the core's `initials`. */
export function initialsOf(name: string): string {
  const words = name.split(/[\s._-]+/).filter(Boolean).slice(0, 2);
  const out = words.map((w) => (Array.from(w).find((c) => /[\p{L}\p{N}]/u.test(c)) ?? "").toUpperCase()).join("");
  return out || "?";
}
