// M1 (phone layout): header with hamburger, lock / sort menu, drawer, Saved pill, pinned press-and-hold drag. Real X input.
const must = (c, m) => { if (!c) throw new Error("check failed: " + m); };
const ctr = (sel, nth = 0) => {
  const els = document.querySelectorAll(sel);
  const el = els[nth < 0 ? els.length + nth : nth];
  if (!el) throw new Error("no element " + sel);
  const r = el.getBoundingClientRect();
  return [Math.round(r.left + r.width / 2), Math.round(r.top + r.height / 2)];
};
const tap = async (sel, nth = 0) => { const [x, y] = ctr(sel, nth); await api.mark(`xdo:mousemove_${x}_${y}_click_1`); await api.wait(500); };
const tapText = async (text, scope = "body") => {
  const el = [...document.querySelectorAll(`${scope} button, ${scope} [role=menuitem], ${scope} [role=menuitemradio], ${scope} [role=tab]`)].find((e) => (e.getAttribute("aria-label") ?? e.textContent).trim().toLowerCase().startsWith(text.toLowerCase()));
  if (!el) throw new Error("no button " + text);
  const r = el.getBoundingClientRect();
  await api.mark(`xdo:mousemove_${Math.round(r.left + r.width / 2)}_${Math.round(r.top + r.height / 2)}_click_1`);
  await api.wait(500);
};
const titles = () => [...document.querySelectorAll(".body .row .title")].map((e) => e.textContent.trim());
const pinIds = () => [...document.querySelectorAll("[data-pin]")].map((e) => e.getAttribute("data-pin"));
must(api.app.phone, "phone layout");
await api.loadSnapshot();
await api.wait(500);
must(!document.querySelector("nav[aria-label=Main]"), "no bottom bar");
must(!document.body.textContent.includes("Reorder"), "no Reorder text");
must(!document.querySelector(".sortb"), "no sort/dots button next to the section heading");
await api.mark("m1-list-header");

// ---- lock label states
try { await api.core.lockSetPin("135790", "135790"); } catch { /* already set on this device */ }
await api.core.lockUnlock("135790");
await api.refreshLock();
await tap("button[aria-label=More]");
must(document.querySelector("[role=menu]").textContent.includes("Lock") && !document.querySelector("[role=menu]").textContent.includes("Unlock"), "menu says Lock while unlocked");
must(!/New message|Reconnect|Settings|Sign out/.test(document.querySelector("[role=menu]").textContent), "old items gone");
await api.mark("m1-menu-unlocked");
await tapText("Lock", "[role=menu]");
await api.wait(500);
must(!api.app.lock.open, "Lock locks immediately");
await tap("button[aria-label=More]");
must(document.querySelector("[role=menu]").textContent.includes("Unlock"), "menu says Unlock while locked");
await api.mark("m1-menu-locked");
await tapText("Unlock", "[role=menu]");
await api.wait(500);
must(api.app.pad && api.app.pad.mode === "unlock", "Unlock opens the PIN pad");
await api.mark("m1-unlock-pinpad");
await api.core.lockUnlock("135790");
await api.refreshLock();
api.app.pad = null;
await api.wait(300);
await api.core.lockRelock();
await api.refreshLock();
api.app.pad = null;

// ---- sort
const orig = api.prefs.chatSort;
await tap("button[aria-label=More]");
await api.mark("m1-menu-sort-recent");
await tapText("Alphabetically", "[role=menu]");
await api.wait(500);
const t = titles();
const unpinnedFirst = [...document.querySelectorAll(".sec")].find((e) => /CHATS|DIRECT|CHANNELS/.test(e.textContent));
const after = [...document.querySelectorAll(".body .row")].filter((r) => !r.closest("[data-pin]")).map((r) => r.querySelector(".title").textContent.trim().replace(/^#\s*/, ""));
const sorted = [...after].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" }));
must(after.join("|") === sorted.join("|"), "unpinned sorted A to Z: " + after.join(","));
must(JSON.parse(localStorage.getItem("workchat.prefs")).chatSort === "az", "sort persisted");
await tap("button[aria-label=More]");
await api.mark("m1-menu-sort-alpha");
await api.click("[role=menuitemradio]", 0); // Recent again (default)
must(api.prefs.chatSort === "recent", "back to Recent");
await api.click("body", 0).catch(() => {});
await tap("button[aria-label=More]");
await tapText("Alphabetically", "[role=menu]");
await api.wait(400);
await api.mark("m1-sorted-alphabetically");

// ---- drawer
await tap("button[aria-label=Menu]");
must(document.querySelector("aside[aria-label=Menu]"), "drawer open");
await api.mark("m1-drawer");
must(window.__wcBack() === true && !api.app.drawer, "Back closes the drawer");
await tap("button[aria-label=Menu]");
await api.mark("xdo:mousemove_380_500_click_1"); // scrim
await api.wait(400);
must(!api.app.drawer, "scrim closes the drawer");
await tap("button[aria-label=Menu]");
await tapText("People", "aside");
must(api.app.tab === "contacts", "People view");
await api.mark("m1-people");
must(window.__wcBack() === true && api.app.tab === "chats", "Back leaves People");
await api.wait(400);
await tap("button[aria-label=Menu]");
await tapText("Settings", "aside");
must(api.app.tab === "settings", "Settings view");
await api.mark("m1-settings");
await tap("button[aria-label=Back]");
must(api.app.tab === "chats", "back arrow leaves Settings");
await api.wait(400);
await tap("button[aria-label=Menu]");
await tapText("Files", "aside");
must(api.app.filter === "files", "Files view");
await api.wait(1200);
await api.mark("m1-files");
must(window.__wcBack() === true && api.app.filter === "all", "Back leaves Files");
await api.wait(400);
must(window.__wcBack() === false, "Back at the list leaves the app");

// ---- Save / Unsave in the message menu, and the Saved pill
if (api.app.saved.includes("01a20193ea3970f79ca9df59691edc96")) await api.core.saveMessage("01a20193ea3970f79ca9df59691edc96", false); // left by a debug run
await api.wait(800);
const startSaved = [...api.app.saved];
const dk = await api.openChat("David Kim");
await api.wait(800);
const c = api.msgs[dk];
const mine = c.items.filter((m) => m.kind !== "system" && m.body && !m.deleted && !startSaved.includes(m.id)).slice(-2);
must(mine.length === 2, "two messages to save");
await api.contextMenu(".bubble", -1);
await api.wait(400);
await api.clickText("Save");
await api.wait(1200);
const lastId = [...document.querySelectorAll("[data-mid]")].pop()?.getAttribute("data-mid");
const nowSaved = api.app.saved.filter((i) => !startSaved.includes(i));
must(nowSaved.length === 1, "Save from the long-press menu saved one message " + JSON.stringify(api.app.toast) + " " + api.app.saved.length + "/" + startSaved.length + " sheet " + api.app.sheet?.kind);
await api.contextMenu(".bubble", -1);
await api.wait(400);
must([...document.querySelectorAll("button")].some((b) => b.textContent.trim() === "Unsave"), "menu now says Unsave");
await api.mark("m1-message-menu-unsave");
await api.clickText("Unsave");
await api.wait(1200);
must(api.app.saved.filter((i) => !startSaved.includes(i)).length === 0, "Unsave from the menu");
await api.click("button[aria-label='Back']");
await api.wait(500);
for (const m of mine) await api.core.saveMessage(m.id, true);
await api.wait(1500);
await tapText("Saved", ".pills");
await api.wait(1500);
const rowsN = () => document.querySelectorAll(".sv .row").length;
must(rowsN() >= 2, "saved list shows rows");
const pillCount = () => document.querySelector(".pills [aria-selected=true] em")?.textContent;
const before = Number(pillCount());
must(before === api.app.saved.length, `pill count ${before} = saved ${api.app.saved.length}`);
await api.mark("m1-saved");
// touch long press on the first row (synthetic touch pointer events): menu with Open in chat / Unsave
await api.pointer(".sv .row .main", "pointerdown");
await api.wait(800);
await api.wait(400);
must([...document.querySelectorAll("[role=menuitem]")].some((b) => b.textContent.includes("Unsave")), "row menu has Unsave");
await api.mark("m1-saved-longpress-menu");
await api.clickText("Unsave", "[role=menu]");
await api.wait(1500);
must(api.app.saved.length === before - 1, `menu Unsave removed one: ${api.app.saved.length} vs ${before} toast ${JSON.stringify(api.app.toast)} rows ${rowsN()}`);
// the visible Unsave button on the row
await tap(".sv .row .un", 0);
await api.wait(1500);
must(api.app.saved.length === before - 2, "row button removed another");
must(Number(pillCount() ?? 0) === before - 2 || !pillCount(), `pill count updated: ${pillCount()}`);
await api.mark("m1-saved-after-unsave");
for (const id of startSaved) if (!api.app.saved.includes(id)) await api.core.saveMessage(id, true);
await tapText("All", ".pills");

// ---- pinned: hold and drag
const origPins = pinIds();
let made = [];
if (origPins.length < 3) {
  const cand = Object.values(api.app.rows).filter((r) => !r.favourite && !r.archived && !r.locked).slice(0, 3 - origPins.length);
  for (const r of cand) { await api.core.setFavourite(r.id, true); made.push(r.id); }
  await api.wait(1200);
}
const pins = pinIds();
must(pins.length >= 3, "three pinned chats");
document.querySelector(".body").scrollTop = 0;
// a quick drag is a scroll: nothing lifts
{
  const [x, y] = ctr("[data-pin]", 0);
  await api.mark(`xdo:mousemove_${x}_${y}_mousedown_1_mousemove_${x}_${y + 20}_mousemove_${x}_${y + 60}_mouseup_1`);
  await api.wait(300);
  must(pinIds().join() === pins.join() && !document.querySelector(".lifted"), "quick move does not reorder");
}
// hold still, release: actions menu
{
  const [x, y] = ctr("[data-pin]", 1);
  await api.mark(`xdo:mousemove_${x}_${y}_mousedown_1_sleep_0.8`);
  must(document.querySelector(".prow.lifted"), "row lifted after holding");
  await api.mark("m1-pinned-lifted");
  await api.mark("xdo:mouseup_1");
  await api.wait(500);
  must(api.app.sheet?.kind === "chatactions", "hold still then release opens the actions menu");
  await api.mark("m1-pinned-actions");
  api.app.sheet = null;
  await api.wait(300);
}
// hold, then drag the first pinned chat below the third
{
  const [x, y] = ctr("[data-pin]", 0);
  const [, y3] = ctr("[data-pin]", 2);
  await api.mark(`xdo:mousemove_${x}_${y}_mousedown_1_sleep_0.8_mousemove_${x}_${y + 15}_mousemove_${x}_${y + 60}_mousemove_${x}_${y3 + 20}`);
  must(document.querySelector(".prow.dragging"), "dragging");
  await api.mark("m1-pinned-drag");
  await api.mark("xdo:mouseup_1");
  await api.wait(1500);
  const now = pinIds();
  must(now.join() !== pins.join() && now[2] === pins[0], "first pinned chat dropped after the third: " + now.join());
  await api.mark("m1-pinned-dropped");
}
await api.core.orderConversations(pins).catch(() => {});
try { localStorage.removeItem("workchat.pinorder"); } catch {}
for (const id of made) await api.core.setFavourite(id, false);
api.prefs.chatSort = orig; api.savePrefs();
await api.wait(1000);
