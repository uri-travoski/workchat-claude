// Window and system hooks: what the core needs to know for notification rules (focus, open chat, time zone),
// notification clicks, and deep links (workchat://... and https://<server>/l/<token>).
import { app, closeConversation, fail, jumpTo, onThemeApplied, openConversation, toast } from "./app.svelte";
import { core } from "./core";
import { openLockedChats } from "./lock";
import { parseAppLink } from "./links";
import { isPhoneOs } from "./push";

export function reportUi() {
  const focused = document.visibilityState === "visible" && document.hasFocus();
  void core.uiState(focused, app.selected, -new Date().getTimezoneOffset()).catch(() => {});
}

/** Opens a chat from a notification or link; a locked one asks for the PIN first. */
export async function openChatFrom(conv: string) {
  if (!conv || app.phase !== "in") return;
  app.tab = "chats";
  app.sheet = null;
  if (app.rows[conv]) return void (await openConversation(conv));
  if (app.lock.pin_set && !app.lock.open) {
    app.pad = { mode: "unlock", title: "Locked chats", then: async () => (app.rows[conv] ? void (await openConversation(conv)) : toast("That chat is not available.")) };
  }
}

export async function handleLink(raw: string) {
  const link = parseAppLink(raw);
  let isLogin = false;
  if (link) isLogin = link.host === "login";
  else {
    try {
      isLogin = /^\/l\/[^/]+/.test(new URL(raw).pathname);
    } catch {
      return;
    }
  }
  if (isLogin) {
    if (app.phase === "out") app.pendingLink = raw;
    else toast("You are already signed in.");
    return;
  }
  if (!link || app.phase !== "in") return;
  if (link.host === "chat") return void (await openChatFrom(link.path));
  if (link.host === "locked") return void openLockedChats();
  if (link.host === "m") {
    const [c, m] = [link.params.get("c"), link.params.get("m")];
    if (c && m) await jumpTo(c, m).catch(fail);
  }
}

/** The system Back button (Android): closes the top-most layer. Returns true when it did something, false to leave the app. */
export function handleBack(): boolean {
  if (app.pad) {
    app.pad = null;
    return true;
  }
  if (app.viewer) {
    app.viewer = null;
    return true;
  }
  if (app.sheet) {
    app.sheet = null;
    return true;
  }
  if (app.thread) {
    app.thread = null;
    return true;
  }
  if (app.phase !== "in") return false;
  if (app.selected) {
    closeConversation();
    return true;
  }
  if (app.tab === "settings" && app.spage) {
    app.spage = "";
    return true;
  }
  if (app.tab !== "chats") {
    app.tab = "chats";
    return true;
  }
  return false;
}

/** Status and navigation bars take the page colour and matching icons (Android). */
export function syncBars() {
  if (!isPhoneOs()) return;
  const bg = getComputedStyle(document.body).backgroundColor;
  const m = bg.match(/\d+(\.\d+)?/g);
  if (!m || m.length < 3) return;
  const [r, g, b] = m.slice(0, 3).map((x) => Math.round(Number(x)));
  const hex = "#" + [r, g, b].map((x) => x.toString(16).padStart(2, "0")).join("");
  const dark = 0.299 * r + 0.587 * g + 0.114 * b < 140;
  void core.setSystemBars(hex, dark).catch(() => {});
}

export async function startShell(): Promise<() => void> {
  const offs: (() => void)[] = [];
  (window as unknown as { __wcBack: () => boolean }).__wcBack = handleBack;
  onThemeApplied(syncBars);
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const bars = () => setTimeout(syncBars, 60);
  media.addEventListener("change", bars);
  offs.push(() => media.removeEventListener("change", bars));
  bars();
  const rep = () => reportUi();
  window.addEventListener("focus", rep);
  window.addEventListener("blur", rep);
  document.addEventListener("visibilitychange", rep);
  offs.push(() => {
    window.removeEventListener("focus", rep);
    window.removeEventListener("blur", rep);
    document.removeEventListener("visibilitychange", rep);
  });
  reportUi();
  try {
    const { listen } = await import("@tauri-apps/api/event");
    offs.push(await listen<string>("wc_open_chat", (e) => void openChatFrom(e.payload)));
  } catch {
    /* not in Tauri */
  }
  try {
    const dl = await import("@tauri-apps/plugin-deep-link");
    const first = await dl.getCurrent().catch(() => null);
    for (const u of first ?? []) void handleLink(u);
    offs.push(await dl.onOpenUrl((urls) => urls.forEach((u) => void handleLink(u))));
  } catch {
    /* no deep links here */
  }
  return () => offs.forEach((f) => f());
}
