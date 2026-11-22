import { app, msgs, type ViewerItem } from "./app.svelte";
import type { Attachment, MessageView } from "./core";
import { isImage, isVideo } from "./filetype";

export const isMedia = (a: Attachment) => !a.removed && (isImage(a.content_type) || isVideo(a.content_type));

export function viewerItem(a: Attachment, m: Pick<MessageView, "conv_id" | "id" | "ts" | "sender_id">): ViewerItem {
  return {
    id: a.id,
    name: a.name,
    content_type: a.content_type,
    size: a.size,
    poster: a.meta?.poster_b64 ? `data:image/jpeg;base64,${a.meta.poster_b64}` : undefined,
    conv_id: m.conv_id,
    message_id: m.id,
    ts: m.ts,
    sender_id: m.sender_id,
  };
}

/** Opens the viewer on `a`, with every photo and video of the same conversation (that is loaded) to swipe through. */
export function openMedia(m: MessageView, a: Attachment) {
  const list: ViewerItem[] = [];
  const items = m.thread_root && !m.also_send_to_channel ? [m] : (msgs[m.conv_id]?.items ?? [m]);
  for (const x of items) {
    if (x.deleted) continue;
    for (const att of x.attachments) if (isMedia(att)) list.push(viewerItem(att, x));
  }
  let index = list.findIndex((i) => i.id === a.id);
  if (index < 0) {
    list.push(viewerItem(a, m));
    index = list.length - 1;
  }
  app.viewer = { items: list, index };
}
