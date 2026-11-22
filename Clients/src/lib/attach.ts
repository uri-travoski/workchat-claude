import { app, fail, toast } from "./app.svelte";
import { core, type Staged } from "./core";
import { fileSize } from "./format";

/** Shows the preview sheet (caption + send) for freshly staged files of a conversation or thread. */
export function showPreview(conv: string, thread: string | null, files: Staged[]) {
  if (!files.length) return;
  if (app.sheet?.kind === "preview" && app.sheet.conv === conv) {
    app.sheet.files = [...app.sheet.files, ...files];
    return;
  }
  app.sheet = { kind: "preview", conv, thread, files, caption: "" };
}

/** System file picker: photos and videos, or any file. */
export async function pickAndPreview(conv: string, thread: string | null, kind: "media" | "any") {
  try {
    showPreview(conv, thread, await core.pickFiles(kind));
  } catch (e) {
    fail(e);
  }
}

/** Files from a drop, a paste or a camera `<input>` arrive as browser `File`s; hand them to the core. */
export async function stageBrowserFiles(files: File[], conv: string, thread: string | null) {
  const out: Staged[] = [];
  for (const f of files) {
    if (f.size > app.maxUpload) {
      toast(`${f.name} is ${fileSize(f.size)}; this server accepts up to ${fileSize(app.maxUpload)}.`, "error");
      continue;
    }
    try {
      out.push(await core.stageBlob(f, f.name || defaultName(f)));
    } catch (e) {
      fail(e);
    }
  }
  showPreview(conv, thread, out);
}

function defaultName(f: File) {
  const ext = f.type.split("/")[1]?.split(";")[0] ?? "bin";
  return `${f.type.startsWith("image/") ? "image" : f.type.startsWith("video/") ? "video" : "file"}-${Date.now()}.${ext === "jpeg" ? "jpg" : ext}`;
}

/** Makes a small JPEG poster (and reads size and duration) of a video the webview can decode. Resolves to nothing when it cannot. */
export function videoMeta(url: string): Promise<{ poster_b64?: string; width?: number; height?: number; duration_ms?: number } | null> {
  return new Promise((resolve) => {
    const v = document.createElement("video");
    v.muted = true;
    v.preload = "auto";
    v.crossOrigin = "anonymous";
    v.playsInline = true;
    const done = (r: Awaited<ReturnType<typeof videoMeta>>) => {
      v.removeAttribute("src");
      v.load();
      resolve(r);
    };
    const timer = setTimeout(() => done(null), 6000);
    v.onerror = () => {
      clearTimeout(timer);
      done(null);
    };
    v.onloadedmetadata = () => {
      v.currentTime = Math.min(0.5, (v.duration || 1) / 4);
    };
    v.onseeked = () => {
      clearTimeout(timer);
      try {
        const max = 320;
        const s = Math.min(1, max / Math.max(v.videoWidth, v.videoHeight, 1));
        const c = document.createElement("canvas");
        c.width = Math.max(1, Math.round(v.videoWidth * s));
        c.height = Math.max(1, Math.round(v.videoHeight * s));
        c.getContext("2d")!.drawImage(v, 0, 0, c.width, c.height);
        let q = 0.7;
        let data = c.toDataURL("image/jpeg", q);
        while (data.length > 50_000 && q > 0.3) data = c.toDataURL("image/jpeg", (q -= 0.1));
        done({ poster_b64: data.split(",")[1], width: v.videoWidth, height: v.videoHeight, duration_ms: Math.round((v.duration || 0) * 1000) || undefined });
      } catch {
        done({ width: v.videoWidth, height: v.videoHeight, duration_ms: Math.round((v.duration || 0) * 1000) || undefined });
      }
    };
    v.src = url;
  });
}
