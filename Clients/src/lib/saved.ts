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

/** Only the saved messages that carry the label (all of them when no label is chosen). `labels` maps a message id to its label ids. */
export function withLabel(list: MessageView[], labelId: string | null, labels: Record<string, string[] | undefined>): MessageView[] {
  return labelId ? list.filter((m) => labels[m.id]?.includes(labelId)) : list;
}
