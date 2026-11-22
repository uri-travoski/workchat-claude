// Desktop D1 walk: rail, resizable panes, two independent search boxes, sort menu. Checks throw (-> "error" mark) on failure.
const must = (c, m) => { if (!c) throw new Error("check failed: " + m); };
const val = (sel) => document.querySelector(sel)?.value ?? null;
await api.openChat("David Kim");
await api.mark("d1-rail-list");
// rail order and bottom Settings
const labels = [...document.querySelectorAll("nav.rail button")].map((b) => b.getAttribute("aria-label"));
must(labels.join() === "All Chats,DMs,Channels,Threads,Files,Saved,Contacts,Settings", "rail items " + labels.join());
must(!document.querySelector(".pills"), "no pills on desktop");
// resize: real mouse drag of the divider (456,400) -> (580,400), then double click resets
const w0 = document.querySelector(".side").getBoundingClientRect().width;
await api.mark("xdo:mousemove_456_400_mousedown_1_mousemove_500_400_mousemove_540_400_mousemove_580_400_mouseup_1");
await api.wait(300);
const w1 = document.querySelector(".side").getBoundingClientRect().width;
must(w1 > w0 + 100, `resized ${w0} -> ${w1}`);
must(JSON.parse(localStorage.getItem("workchat.panes")).list === Math.round(w1), "width persisted");
await api.mark("d1-resized-panes");
await api.mark("xdo:mousemove_580_400_click_--repeat_2_--delay_80_1");
await api.wait(300);
must(Math.round(document.querySelector(".side").getBoundingClientRect().width) === 384, "double click resets");
// top search is independent of the list filter
await api.type("input.listq", "elena");
await api.wait(400);
must(val("input.gsearch") === "", "top box untouched by list filter");
const names = [...document.querySelectorAll(".list .prow, .list .row")].length;
await api.mark("d1-list-filter");
await api.type("input.listq", "");
await api.type("input.gsearch", "launch");
await api.wait(2200);
must(val("input.listq") === "", "list box untouched by global search");
must(document.querySelector("#gs-panel"), "panel open");
const groups = [...document.querySelectorAll("#gs-panel .h")].map((h) => h.textContent);
await api.mark("d1-global-search");
await api.mark("xdo:key_Down");
await api.mark("xdo:key_Down");
await api.wait(200);
must(document.querySelector("#gs-panel .r.sel"), "keyboard selection");
await api.mark("d1-global-search-keys");
await api.mark("xdo:key_Escape");
await api.wait(300);
must(!document.querySelector("#gs-panel"), "Esc closes the panel");
await api.click("button[aria-label='Clear search']");
must(val("input.gsearch") === "", "clear button");
// message hit: opens the conversation and highlights the message
await api.type("input.gsearch", "sprint");
await api.wait(2200);
const hit = document.querySelector("#gs-panel .r");
must(hit, "has a hit for 'sprint'");
hit.click();
await api.wait(1500);
await api.mark("d1-jump-highlight");
// sort menu
await api.openChat("David Kim");
await api.click(".sortb");
await api.mark("d1-sort-menu");
await api.clickText("Name A to Z");
await api.wait(400);
const ts = [...document.querySelectorAll(".list .row .title")].map((e) => e.textContent.trim());
await api.mark("d1-sort-az");
must(JSON.parse(localStorage.getItem("workchat.prefs")).chatSort === "az", "sort persisted");
await api.click(".sortb");
await api.clickText("Recent activity");
// rail views
for (const id of ["dm", "channel", "threads", "files", "saved"]) {
  await api.click(`nav.rail button[data-rail=${id}]`);
  await api.wait(900);
  await api.mark("d1-rail-" + id);
}
await api.click("nav.rail button[data-rail=all]");
