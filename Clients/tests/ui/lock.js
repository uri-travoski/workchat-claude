// Lock / Unlock end to end through the keypad. Uses a chat nobody else cares about: the first DM.
window.confirm = () => true;
const wait = api.wait;
const pre = window.innerWidth < 700 ? "p" : "d";
const keys = async (pin) => { for (const d of pin) { await api.click(`.pad .keys button.k:not(.flat)`, "0123456789".indexOf(d) === 0 ? 9 : "123456789".indexOf(d)); await wait(120); } await wait(500); };
await wait(500);
const rows = Object.values(api.app.rows).filter((r) => r.kind === "dm");
const target = rows.find((r) => r.title === "Marcus Brody") ?? rows[0];
const title = target.title;
// 1. Lock asks for a new PIN first.
const before = Object.keys(api.app.rows).length;
const counts0 = JSON.stringify(api.app.counts);
api.app.tab = "chats";
await wait(300);
api.app.pad = { mode: "set", title: "Create a PIN", then: async () => { await api.core.lockChat(target.id, true); await api.refreshLock(); } };
await wait(400);
await api.mark(`${pre}-lock-1-create`);
await keys("246810");
await api.mark(`${pre}-lock-2-confirm`);
await keys("246810");
await wait(900);
if (!api.app.lock.pin_set) throw new Error("PIN not set");
// Setting the PIN leaves the area open; "Lock now" closes it and hides the chat.
await api.core.lockRelock();
await api.refreshLock();
await wait(700);
if (api.app.rows[target.id]) throw new Error("locked chat is still in the list");
if (Object.keys(api.app.rows).length !== before - 1) throw new Error("row count did not drop by one");
await api.mark(`${pre}-lock-3-hidden`);
// Search must not find it either.
const hits = await api.core.search(title, "all").catch(() => null);
if (hits && JSON.stringify(hits).includes(target.id)) throw new Error("search finds a locked chat");
// 2. Wrong PIN, then right PIN opens the locked area.
api.app.pad = { mode: "unlock", title: "Locked chats", then: () => { api.app.sheet = { kind: "lockedchats" }; } };
await wait(400);
await keys("111111");
await api.mark(`${pre}-lock-4-wrong`);
await keys("246810");
await wait(900);
if (!api.app.lock.open) throw new Error("did not unlock");
await api.mark(`${pre}-lock-5-locked-chats`);
// 3. Unlock the chat (menu says Unlock, never Unlock Chat), then relock the area.
api.app.sheet = { kind: "chatactions", conv: target.id, x: 300, y: 300 };
await wait(500);
const labels = [...document.querySelectorAll("[role=menuitem], .menu button")].map((e) => e.textContent.trim());
if (!labels.some((l) => l === "Unlock" || l.startsWith("Unlock")) || labels.some((l) => /unlock chat/i.test(l))) throw new Error("menu labels: " + labels.join("|"));
await api.mark(`${pre}-lock-6-unlock-menu`);
api.app.sheet = null;
await api.core.lockChat(target.id, false);
await api.refreshLock();
await wait(600);
if (!api.app.rows[target.id]) throw new Error("unlocked chat is not back");
await api.mark("done");
