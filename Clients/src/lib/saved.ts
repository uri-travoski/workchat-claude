import type { ChatRow, MessageView } from "./core";

/** The saved list shown to the person: deleted messages and ones in chats this device cannot see (locked) are dropped. */
export function visibleSaved(list: MessageView[], rows: Record<string, ChatRow | undefined>): MessageView[] {
  const seen = new Set<string>();
  return list.filter((m) => {
    if (m.deleted || !rows[m.conv_id] || seen.has(m.id)) return false;
    seen.add(m.id);
    return true;
  });
}
