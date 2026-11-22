import { test } from "node:test";
import assert from "node:assert/strict";
import { lockEntry } from "../src/lib/menu";
import { HOLD_MS, SLOP, holdStep } from "../src/lib/hold";
import { visibleSaved, withLabel } from "../src/lib/saved";
import { moveId, sortChats } from "../src/lib/chat";

const row = (o: Record<string, unknown>) => ({ id: "x", kind: "dm", title: "T", last_ts: 0, favourite: false, ...o }) as never;
const msg = (id: string, conv: string, o: Record<string, unknown> = {}) => ({ id, conv_id: conv, deleted: false, ...o }) as never;

test("lock entry: Lock while unlocked, Unlock while locked, first use sets a PIN", () => {
  assert.deepEqual(lockEntry({ pin_set: true, open: true }), { label: "Lock", act: "lock" });
  assert.deepEqual(lockEntry({ pin_set: true, open: false }), { label: "Unlock", act: "unlock" });
  assert.deepEqual(lockEntry({ pin_set: false, open: false }), { label: "Lock", act: "setup" });
});

test("sort: Recent is newest activity first, Alphabetically is A to Z; ties fall back to activity", () => {
  const rows = [row({ id: "b", title: "bravo", last_ts: 5 }), row({ id: "c", title: "Charlie", last_ts: 9 }), row({ id: "a", title: "alpha", last_ts: 1 }), row({ id: "a2", title: "Alpha", last_ts: 3 })];
  assert.deepEqual(sortChats(rows, "recent").map((r) => r.id), ["c", "b", "a2", "a"]);
  assert.deepEqual(sortChats(rows, "az").map((r) => r.id), ["a2", "a", "b", "c"]);
  const before = rows.map((r) => r.id).join();
  sortChats(rows, "az");
  assert.equal(rows.map((r) => r.id).join(), before, "the input is not modified");
});

test("sort never touches pinned chats: they are split off first and keep the manual order", () => {
  const rows = [row({ id: "z", title: "Zed", favourite: true }), row({ id: "a", title: "Ann", favourite: true }), row({ id: "m", title: "Mo" })];
  const pinned = rows.filter((r) => r.favourite);
  assert.deepEqual(sortChats(rows.filter((r) => !r.favourite), "az").map((r) => r.id), ["m"]);
  assert.deepEqual(moveId(pinned.map((r) => r.id), "a", 0), ["a", "z"]);
});

test("hold gesture: moving first scrolls, holding still lifts, then moving drags, letting go still opens the menu", () => {
  assert.equal(holdStep(false, 100, 2), "wait");
  assert.equal(holdStep(false, 100, SLOP + 1), "scroll");
  assert.equal(holdStep(false, HOLD_MS, SLOP + 1), "scroll", "moved too far even at the deadline");
  assert.equal(holdStep(false, HOLD_MS, 3), "lift");
  assert.equal(holdStep(true, HOLD_MS + 300, 3), "hold");
  assert.equal(holdStep(true, HOLD_MS + 300, SLOP + 20), "drag");
});

test("saved list: drops deleted messages, hidden chats and duplicates, keeps the server's newest-first order", () => {
  const rows = { c1: row({ id: "c1" }), c2: row({ id: "c2" }) } as never;
  const list = [msg("m3", "c1"), msg("m2", "c9"), msg("m1", "c2", { deleted: true }), msg("m3", "c1"), msg("m0", "c2")];
  assert.deepEqual(visibleSaved(list, rows).map((m) => m.id), ["m3", "m0"]);
  assert.deepEqual(visibleSaved([], rows), []);
});

test("saved by label: only messages carrying the label, everything when none is chosen", () => {
  const list = [msg("a", "c"), msg("b", "c"), msg("d", "c")];
  const labels = { a: ["l1"], b: ["l2", "l1"] };
  assert.deepEqual(withLabel(list, "l1", labels).map((m) => m.id), ["a", "b"]);
  assert.deepEqual(withLabel(list, "l2", labels).map((m) => m.id), ["b"]);
  assert.deepEqual(withLabel(list, null, labels).map((m) => m.id), ["a", "b", "d"]);
});
