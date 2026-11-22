import type { Label } from "./core";

/** The colours offered for a label (the server takes any #rrggbb). */
export const LABEL_COLORS = ["#E5484D", "#F76B15", "#F5B400", "#30A46C", "#0091FF", "#8E4EC6", "#E93D82", "#6E7E92"];

export const LABEL_NAME_MAX = 30;

/** The labels of a saved message, in the person's own label order; ids that no longer exist are skipped. */
export function labelsOf(ids: string[] | undefined, all: Label[]): Label[] {
  return ids?.length ? all.filter((l) => ids.includes(l.id)) : [];
}

/** `ids` with `id` added or removed. */
export const toggleId = (ids: string[], id: string): string[] => (ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id]);

/** The next unused palette colour, so new labels differ from the last ones. */
export function nextColor(all: Label[]): string {
  return LABEL_COLORS.find((c) => !all.some((l) => l.color.toLowerCase() === c.toLowerCase())) ?? LABEL_COLORS[all.length % LABEL_COLORS.length];
}
