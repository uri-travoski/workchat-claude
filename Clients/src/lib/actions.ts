import { core } from "./core";
import { fail } from "./app.svelte";

/** Signs out after a confirmation; the core wipes the local cache and the UI returns to the login screen. */
export async function signOutAction() {
  if (!window.confirm("Sign out of WorkChat on this device? Your chats stay on the server.")) return;
  try {
    await core.signOut();
  } catch (e) {
    fail(e);
  }
}
