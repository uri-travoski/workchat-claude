/** Coloured tile of a document card: PDF red, Word blue, Excel green, PowerPoint orange, archives violet, and so on. */
export interface FileTile {
  label: string;
  fg: string;
  bg: string;
}

const T = (label: string, fg: string, bg: string): FileTile => ({ label, fg, bg });

const RED = T("PDF", "#c4261f", "#fde4e4");
const BLUE = (l: string) => T(l, "#1e5bb8", "#dbe8ff");
const GREEN = (l: string) => T(l, "#1a7f4b", "#d8f2e4");
const ORANGE = (l: string) => T(l, "#c2570f", "#ffe5d0");
const VIOLET = (l: string) => T(l, "#6a4fd1", "#e7e2fb");
const GREY = (l: string) => T(l, "#566176", "#e6e9ef");

export function extOf(name: string): string {
  const i = name.lastIndexOf(".");
  return i > 0 && i < name.length - 1 ? name.slice(i + 1).toLowerCase() : "";
}

export function fileTile(name: string, contentType = ""): FileTile {
  const e = extOf(name);
  switch (e) {
    case "pdf":
      return RED;
    case "doc":
    case "docx":
    case "odt":
    case "rtf":
      return BLUE(e === "doc" ? "DOC" : e === "docx" ? "DOCX" : e.toUpperCase());
    case "xls":
    case "xlsx":
    case "ods":
    case "csv":
      return GREEN(e.toUpperCase());
    case "ppt":
    case "pptx":
    case "odp":
    case "key":
      return ORANGE(e.toUpperCase());
    case "zip":
    case "rar":
    case "7z":
    case "gz":
    case "tar":
    case "tgz":
      return VIOLET(e.toUpperCase());
    case "apk":
      return T("APK", "#2e7d32", "#dcf0dc");
    case "txt":
    case "md":
    case "log":
      return GREY("TXT");
    case "json":
    case "xml":
    case "html":
      return GREY(e.toUpperCase());
  }
  if (contentType === "application/pdf") return RED;
  return GREY(e ? e.toUpperCase().slice(0, 4) : "FILE");
}

/** "4.2 MB · 28 pages" style line of a document card. */
export function docLine(size: number, pages?: number | null, bytes: (n: number) => string = String): string {
  const s = bytes(size);
  return pages && pages > 0 ? `${s} • ${pages} ${pages === 1 ? "page" : "pages"}` : s;
}

/** True for attachments the app plays itself (voice notes and audio files). */
export const isAudio = (ct: string) => ct.startsWith("audio/");
export const isImage = (ct: string) => ct.startsWith("image/");
export const isVideo = (ct: string) => ct.startsWith("video/");
/** A voice message: audio with a waveform in its details, or named like the apps name recordings. */
export const isVoice = (a: { name: string; content_type: string; meta?: { peaks?: number[] } | null }) =>
  isAudio(a.content_type) && ((a.meta?.peaks?.length ?? 0) > 0 || /^voice[-_]/i.test(a.name));
