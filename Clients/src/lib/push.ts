// Phones: asks the platform for a push endpoint and keeps the server's registration current.
import { app } from "./app.svelte";
import { core } from "./core";

export const isPhoneOs = () => platform === "android" || platform === "ios";
export let platform = "linux";
export const setPlatform = (p: string) => (platform = p);

/** At start and after sign-in: register for push when a distributor / token is available. Never throws. */
export async function syncPush() {
  if (!isPhoneOs() || app.phase !== "in") return;
  try {
    const s = await core.getNotifySettings();
    if (!s.enabled) return;
    await core.pushRegister();
  } catch {
    /* no distributor installed yet: Settings, Notifications explains */
  }
}
