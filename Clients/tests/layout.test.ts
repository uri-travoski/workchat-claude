import { test } from "node:test";
import assert from "node:assert/strict";
import { LIMITS, MAIN_MIN, defaults, fit, maxFor, resize } from "../src/lib/panes";
import { dropSlot, inKind, moveId, nameMatches, sortChats, unreadOf } from "../src/lib/chat";
import { closeTopLayer, installOverlayKeys, layerCount, registerLayer } from "../src/lib/overlays";
import { mergeFiles, mergePeople, snippetParts, stepIndex } from "../src/lib/search";

const row = (o: Record<string, unknown>) => ({ id: "x", kind: "dm", title: "T", last_ts: 0, unread: 0, marked_unread: false, muted: false, archived: false, preview: { text: "" }, ...o }) as never;

test("panes: widths stay within limits and leave room for the conversation", () => {
  const w = defaults();
  assert.equal(resize(w, "list", 10, 1400, false).list, LIMITS.list.min);
  assert.equal(resize(w, "list", 9999, 1400, false).list, LIMITS.list.max);
  assert.equal(resize(w, "list", 9999, 1000, false).list, 1000 - 72 - MAIN_MIN);
  assert.equal(maxFor("thread", { list: 400, thread: 360 }, 1300, true), 1300 - 72 - 400 - MAIN_MIN);
  assert.equal(maxFor("thread", { list: 400, thread: 360 }, 1000, true), LIMITS.thread.min, "never below the minimum");
  const small = fit({ list: 560, thread: 640 }, 1000, true);
  assert.ok(small.list + small.thread + 72 + MAIN_MIN <= 1000 || (small.list === LIMITS.list.min && small.thread === LIMITS.thread.min));
  assert.deepEqual(defaults(), { list: 384, thread: 360 });
});

test("list filter matches names only", () => {
  assert.equal(nameMatches(row({ title: "Katherine Wells", preview: { text: "lunch" } }), "kath"), true);
  assert.equal(nameMatches(row({ title: "Katherine Wells", preview: { text: "lunch" } }), "lunch"), false);
  assert.equal(nameMatches(row({ title: "x" }), "  "), true);
});

test("rail kinds: DMs are DMs and group DMs, Channels are channels, All is everything", () => {
  const [dm, g, ch, self] = [row({ kind: "dm" }), row({ kind: "group_dm" }), row({ kind: "channel" }), row({ kind: "self" })];
  assert.deepEqual([dm, g, ch, self].map((r) => inKind(r, "all")), [true, true, true, true]);
  assert.deepEqual([dm, g, ch, self].map((r) => inKind(r, "dm")), [true, true, false, true]);
  assert.deepEqual([dm, g, ch, self].map((r) => inKind(r, "channel")), [false, false, true, false]);
  assert.equal(unreadOf([row({ unread: 3 }), row({ marked_unread: true }), row({ unread: 9, muted: true })]), 4);
});

test("sorting: recent activity by default, A to Z and Z to A on request", () => {
  const rows = [row({ id: "a", title: "Bravo", last_ts: 9 }), row({ id: "b", title: "alpha", last_ts: 5 }), row({ id: "c", title: "Charlie", last_ts: 1 })];
  const ids = (s: "recent" | "az" | "za") => sortChats(rows, s).map((r) => r.id).join("");
  assert.equal(ids("recent"), "abc");
  assert.equal(ids("az"), "bac");
  assert.equal(ids("za"), "cab");
});

test("pinned drag: slot from pointer, new order", () => {
  assert.equal(dropSlot([10, 30, 50], 5), 0);
  assert.equal(dropSlot([10, 30, 50], 35), 2);
  assert.equal(dropSlot([10, 30, 50], 99), 3);
  assert.deepEqual(moveId(["a", "b", "c"], "a", 3), ["b", "c", "a"]);
  assert.deepEqual(moveId(["a", "b", "c"], "c", 0), ["c", "a", "b"]);
  assert.deepEqual(moveId(["a", "b", "c"], "b", 1), ["a", "b", "c"]);
});

test("overlays: Escape closes only the top-most layer", () => {
  const log: string[] = [];
  const off1 = registerLayer(() => log.push("viewer"));
  const off2 = registerLayer(() => log.push("dialog"));
  assert.equal(layerCount(), 2);
  assert.equal(closeTopLayer(), true);
  off2();
  assert.equal(closeTopLayer(), true);
  off1();
  assert.equal(closeTopLayer(), false);
  assert.deepEqual(log, ["dialog", "viewer"]);
});

test("overlays: the window handler swallows Escape while a layer is open", () => {
  const handlers: Record<string, (e: unknown) => void> = {};
  (globalThis as Record<string, unknown>).window = { addEventListener: (t: string, f: (e: unknown) => void) => (handlers[t] = f), removeEventListener: () => {} };
  const off = installOverlayKeys();
  let closed = 0;
  const un = registerLayer(() => closed++);
  const ev = { key: "Escape", defaultPrevented: false, prevented: false, stopped: false, preventDefault() { this.prevented = true; }, stopImmediatePropagation() { this.stopped = true; } };
  handlers.keydown(ev);
  assert.deepEqual([closed, ev.prevented, ev.stopped], [1, true, true]);
  un();
  const ev2 = { ...ev, prevented: false, stopped: false };
  handlers.keydown(ev2);
  assert.deepEqual([closed, ev2.prevented], [1, false]);
  off();
});

test("global search helpers", () => {
  assert.deepEqual(snippetParts("a [[b]] c"), [{ t: "a ", hit: false }, { t: "b", hit: true }, { t: " c", hit: false }]);
  const u = (id: string, disabled = false) => ({ id, disabled }) as never;
  assert.deepEqual(mergePeople([u("1"), u("2")], [u("2"), u("3"), u("4", true), u("me")], "me").map((x: { id: string }) => x.id), ["1", "2", "3"]);
  const f = (id: string) => ({ attachment: { id } }) as never;
  assert.equal(mergeFiles([f("a")], [f("a"), f("b")]).length, 2);
  assert.equal(stepIndex(-1, 1, 3), 0);
  assert.equal(stepIndex(-1, -1, 3), 2);
  assert.equal(stepIndex(2, 1, 3), 0);
  assert.equal(stepIndex(0, -1, 3), 2);
  assert.equal(stepIndex(0, 1, 0), -1);
});
