// Lock / Unlock: the UI side of the 6-digit PIN. The core keeps the PIN hash, the throttle and the locked-chat list and
// hides locked chats from everything it sends here while the locked area is closed.
import { app, closeConversation, fail, refreshLock, toast } from "./app.svelte";
import { core } from "./core";

async function doLock(conv: string) {
  try {
    await core.lockChat(conv, true);
    await refreshLock();
    if (app.selected === conv) closeConversation();
    toast(app.lock.open ? "Chat locked." : "Chat locked. Find it under Locked chats.");
  } catch (e) {
    fail(e);
  }
}

/** "Lock" on a chat's menu. The first use asks for a new PIN. */
export function lockChat(conv: string) {
  if (!app.lock.pin_set) {
    app.pad = { mode: "set", title: "Create a PIN", then: () => doLock(conv) };
    return;
  }
  void doLock(conv);
}

/** "Unlock" on a locked chat's menu (only reachable while the locked area is open). */
export function unlockChat(conv: string) {
  const run = async () => {
    try {
      await core.lockChat(conv, false);
      await refreshLock();
      toast("Chat unlocked.");
    } catch (e) {
      fail(e);
    }
  };
  if (app.lock.open) void run();
  else app.pad = { mode: "unlock", title: "Enter your PIN", then: run };
}

/** Menu entry "Locked chats": asks for the PIN, then lists them. */
export function openLockedChats() {
  if (!app.lock.pin_set) {
    toast("No chat is locked yet. Open a chat's menu and choose Lock.");
    return;
  }
  const show = () => {
    app.sheet = { kind: "lockedchats" };
  };
  if (app.lock.open) show();
  else app.pad = { mode: "unlock", title: "Locked chats", then: show };
}

/** The phone menu's Lock / Unlock entry (see `lockEntry`): locks now, asks for the PIN, or sets a PIN up first. */
export function lockMenuAction(act: "lock" | "unlock" | "setup") {
  if (act === "lock") return void lockNow();
  if (act === "unlock") {
    app.pad = { mode: "unlock", title: "Unlock chats", then: () => toast("Locked chats are showing.") };
    return;
  }
  app.pad = { mode: "set", title: "Create a PIN", then: async () => { await refreshLock(); if (app.lock.open) await lockNow(); } };
}

export async function lockNow() {
  try {
    await core.lockRelock();
    await refreshLock();
    app.sheet = null;
  } catch (e) {
    fail(e);
  }
}

export function changePin() {
  app.pad = { mode: "change", title: "Change PIN" };
}

let started = false;
/** Auto-relock: when the app goes to the background or is minimised, and after the inactivity timeout. */
export function startLockWatch() {
  if (started) return;
  started = true;
  const tick = async () => {
    if (app.phase !== "in" || !app.lock.open) return;
    try {
      if (await core.lockTick()) await refreshLock();
    } catch {
      /* ignore */
    }
  };
  const background = async () => {
    if (app.phase !== "in" || !app.lock.open) return;
    try {
      await core.lockBackground();
      await refreshLock();
    } catch {
      /* ignore */
    }
  };
  setInterval(tick, 10_000);
  document.addEventListener("visibilitychange", () => (document.visibilityState === "hidden" ? void background() : void tick()));
  let last = 0;
  const touch = () => {
    const now = Date.now();
    if (now - last > 4000 && app.lock.open) {
      last = now;
      void core.lockTouch().catch(() => {});
    }
  };
  for (const ev of ["pointerdown", "keydown", "wheel"]) window.addEventListener(ev, touch, { passive: true });
  // Desktop: a minimised or hidden window counts as "in the background"; losing focus alone does not.
  void import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
    const w = getCurrentWindow();
    void w.onFocusChanged(({ payload: focused }) => {
      if (focused) return void tick();
      setTimeout(async () => {
        try {
          if ((await w.isMinimized()) || !(await w.isVisible())) void background();
        } catch {
          /* not available on this platform */
        }
      }, 400);
    }).catch(() => {});
  }).catch(() => {});
}
