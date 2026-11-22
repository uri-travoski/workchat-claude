import { convertFileSrc } from "@tauri-apps/api/core";

/** URL the webview can load for an attachment; served by the core's `wcmedia` scheme (no auth headers needed). */
export const mediaUrl = (id: string, thumb = false): string => convertFileSrc(id, "wcmedia") + (thumb ? "?thumb=1" : "");

export const posterUrl = (b64?: string | null) => (b64 ? `data:image/jpeg;base64,${b64}` : undefined);
