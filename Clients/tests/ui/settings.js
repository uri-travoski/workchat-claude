// Settings: every page, then Lock/Unlock and the update dialog. Phone and desktop sizes (the driver sets the window).
window.confirm = () => true;
const wait = api.wait;
const phone = window.innerWidth < 700;
const pre = phone ? "p" : "d";
api.app.tab = "settings";
api.app.spage = "";
await wait(800);
await api.mark(`${pre}-settings-home`);
for (const page of ["profile", "saved", "starred", "notifications", "privacy", "storage", "appearance", "prefs", "server", "devices", "about"]) {
  if (phone && page === "profile") continue;
  api.app.spage = page;
  await wait(1100);
  await api.mark(`${pre}-settings-${page}`);
}
// Edit profile and share card sheets.
api.app.sheet = { kind: "editprofile" };
await wait(600);
await api.mark(`${pre}-edit-profile`);
api.app.sheet = { kind: "sharecard" };
await wait(600);
await api.mark(`${pre}-share-card`);
api.app.sheet = null;
// Appearance really applies.
api.app.spage = "appearance";
await wait(500);
api.prefs.bubbleRadius = 22;
api.prefs.accent = "blue";
api.prefs.theme = "dark";
api.savePrefs();
await wait(600);
await api.mark(`${pre}-appearance-dark-blue-round`);
api.prefs.theme = "light";
api.prefs.accent = "navy";
api.prefs.bubbleRadius = 12;
api.savePrefs();
await wait(300);
await api.mark("done");
