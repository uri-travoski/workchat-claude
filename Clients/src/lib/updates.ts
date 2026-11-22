// The update checker's UI side. The core queries GitHub (or the test override), compares versions and picks the asset.
import { app, fail } from "./app.svelte";
import { core, errorMessage } from "./core";

/** Silent check at start, at most once a day: only sets the badge on Settings, About. */
export async function silentUpdateCheck() {
  try {
    const st = await core.updateState();
    app.upd.found = st.found;
    if (!st.due) return;
    const info = await core.checkUpdate();
    app.upd.found = info.newer ? info.latest : null;
    if (info.newer) app.upd.info = info;
  } catch {
    /* offline or no release yet: stay quiet */
  }
}

export async function checkUpdateNow() {
  app.upd.checking = true;
  app.upd.error = "";
  try {
    const info = await core.checkUpdate();
    app.upd.info = info;
    app.upd.found = info.newer ? info.latest : null;
    app.upd.state = "";
  } catch (e) {
    app.upd.info = null;
    app.upd.error = errorMessage(e);
  } finally {
    app.upd.checking = false;
  }
}

export async function runUpdate() {
  const info = app.upd.info;
  if (!info) return;
  app.upd.error = "";
  try {
    if (info.action === "page" || !info.asset) await core.openUrl(info.page);
    else await core.startUpdate(info);
  } catch (e) {
    app.upd.error = errorMessage(e);
    fail(e);
  }
}
