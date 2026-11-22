// Server phase S2 in the app: channel creation (public/private), browse and join, manager rights, delete for me,
// also send to channel. Needs the seeded server plus a public channel "c3-open-demo" made by david.kim.
window.confirm = () => true;
const wait = api.wait;
api.app.sheet = { kind: "newchat", step: "menu" };
await api.mark("s2-newchat-menu");
api.app.sheet = { kind: "newchat", step: "channel" };
await wait(300);
await api.type('input[placeholder^="e.g. product"]', "s2-launch-" + (Date.now() % 100000));
await api.type('input[placeholder^="What is this channel"]', "Made in the S2 test");
await api.mark("s2-newchannel-form");
await api.clickText("Private");
await api.mark("s2-newchannel-private");
await api.clickText("Public");
await api.clickText("Create channel");
for (let i = 0; i < 30 && !api.app.selected; i++) await wait(200);
const conv = api.app.selected;
const row = api.app.rows[conv];
if (!row || row.visibility !== "public" || !row.can_manage) throw new Error("new channel is not a public channel I manage: " + JSON.stringify(row));
await wait(500);
// Hide: send two messages, delete one for me.
await api.core.sendMessage(conv, "keep this one");
await api.core.sendMessage(conv, "delete this one for me");
await wait(1200);
await api.contextMenu(".bubble.out");
await api.mark("s2-message-menu");
await api.clickText("Delete for me");
await wait(800);
const texts = [...document.querySelectorAll(".msg")].map((e) => e.textContent);
if (texts.some((t) => t.includes("delete this one for me"))) throw new Error("hidden message still shown");
await api.mark("s2-after-delete-for-me");
// Also send to channel: thread reply visible in the channel.
const first = (await api.core.openConversation(conv)).messages.find((m) => m.body === "keep this one");
await api.core.sendMessage(conv, "reply that also goes to the channel", first.id, null, true);
await wait(1500);
const m2 = [...document.querySelectorAll(".msg")].map((e) => e.textContent);
if (!m2.some((t) => t.includes("reply that also goes to the channel"))) throw new Error("broadcast reply not in the channel timeline");
if (!m2.some((t) => t.includes("Replied to a thread"))) throw new Error("no thread reference");
await api.mark("s2-also-send");
// Manager sheet.
api.app.sheet = { kind: "channelinfo", conv };
await wait(900);
await api.mark("s2-manage-channel");
await api.clickText("Add people");
await wait(300);
await api.mark("s2-add-people");
api.app.sheet = null;
// Browse and join.
api.app.sheet = { kind: "newchat", step: "browse" };
await wait(1200);
await api.mark("s2-browse");
await api.clickText("Join");
for (let i = 0; i < 30 && !Object.values(api.app.rows).some((r) => r.title === "c3-open-demo"); i++) await wait(200);
if (!Object.values(api.app.rows).some((r) => r.title === "c3-open-demo")) throw new Error("join did not add the channel");
await wait(800);
await api.mark("s2-joined");
// Leave it again.
const demo = Object.values(api.app.rows).find((r) => r.title === "c3-open-demo");
await api.core.leaveConversation(demo.id);
await wait(800);
if (api.app.rows[demo.id]) throw new Error("left channel still listed");
// Clean up the test channel (archive).
await api.core.updateConversation(conv, { archived: true });
await wait(500);
await api.mark("done");
