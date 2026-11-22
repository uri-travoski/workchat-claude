// Opening attachments. Photos and videos open in the in-app viewer (see viewer.ts); everything else (documents, PDFs,
// archives, audio files, ...) is downloaded to the cache by the core and opened with the system's default application.
// When that does not work the user always gets a dialog with Open with / Save as / Show in folder, never silence.
import { app, fail, toast } from "./app.svelte";
import { core, errorMessage } from "./core";

export interface FileRef {
  id: string;
  name: string;
}

/** Opens a file in its default application; shows the fallback dialog when no application opens it. */
export async function openFile(f: FileRef): Promise<boolean> {
  try {
    const r = await core.openAttachment(f.id, f.name);
    if (r.opened) return true;
    app.openFail = { id: f.id, name: f.name, reason: r.reason };
    return false;
  } catch (e) {
    // The download itself failed (offline, file gone): that is an error, not a missing handler.
    fail(e);
    return false;
  }
}

/** The three actions of the fallback dialog. Each one reports what happened. */
export async function openWith(f: FileRef): Promise<boolean> {
  try {
    return await core.openAttachmentWith(f.id, f.name);
  } catch (e) {
    toast(errorMessage(e), "error");
    return false;
  }
}

export async function saveAs(f: FileRef): Promise<boolean> {
  try {
    const p = await core.saveAttachmentAs(f.id, f.name);
    if (p) toast(`Saved to ${p}`);
    return !!p;
  } catch (e) {
    toast(errorMessage(e), "error");
    return false;
  }
}

export async function showInFolder(f: FileRef): Promise<boolean> {
  try {
    await core.showAttachmentInFolder(f.id, f.name);
    return true;
  } catch (e) {
    toast(errorMessage(e), "error");
    return false;
  }
}
