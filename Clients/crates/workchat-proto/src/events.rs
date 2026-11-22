use crate::automation::ScheduledMessage;
use crate::types::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One entry of a user's ordered event log. `cursor` increases monotonically per user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    pub cursor: i64,
    pub ts: Ts,
    pub event: EventData,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventData {
    MessageNew { message: Message },
    /// Edit applied; `message` carries the new body and version.
    MessageEdit { message: Message },
    /// Tombstone (the sender deleted the message, or an admin deleted it for everyone).
    MessageDelete { message: Message },
    /// The messages vanish from this user's view only: "Delete for me" (`POST /messages/{id}/hide`, sent to all of that user's
    /// devices) and admin clear-history / date-range delete. Drop them from the cache and the lists.
    MessageHide { conv_id: String, message_ids: Vec<String> },
    ReactionUpdate { conv_id: String, message_id: String, reactions: Vec<ReactionSummary> },
    MessageDelivered { conv_id: String, message_id: String },
    ConversationUpsert { conversation: ConversationSummary },
    /// Removed from the conversation, or it was deleted.
    ConversationRemove { conv_id: String },
    /// Someone else read up to `last_read_seq` in a DM or group DM (read receipts); sent only when that person has them on.
    ReadReceipt { conv_id: String, user_id: String, last_read_seq: i64 },
    ReadUpdate { conv_id: String, last_read_seq: i64, marked_unread: bool, unread: i64, mentions: i64 },
    UserUpsert { user: User },
    /// An admin removed a file; the attachment shows "File removed".
    AttachmentRemoved { attachment_id: String },
    /// Own profile / settings changed on another device.
    MeUpdate { me: Me },
    /// A followed thread changed (new reply, follow/unfollow, read). `following=false` removes it.
    ThreadUpdate { thread: ThreadSummary },
    PinsUpdate { conv_id: String, pins: Vec<Pin> },
    BookmarksUpdate { conv_id: String, bookmarks: Vec<Bookmark> },
    GroupsUpdate { groups: Vec<UserGroup> },
    /// Favourite / section / mute / notification level changed on another device.
    PrefUpdate { pref: ConvPref },
    SectionsUpdate { sections: Vec<Section> },
    /// A private picture for another user or a conversation was set (`attachment` = Some) or removed (None).
    /// Sent only to the person who chose it.
    AvatarOverrideUpdate { kind: String, target: String, attachment: Option<String> },
    DraftUpdate { draft: DraftItem },
    /// A scheduled message was created, sent, cancelled or failed (SV-10).
    ScheduledUpdate { item: ScheduledMessage },
    /// A message was saved for later or its labels changed.
    SavedUpdate { item: SavedItem },
    SavedRemove { message_id: String },
    LabelsUpdate { labels: Vec<Label> },
    /// Activity was marked read (on any device).
    ActivityRead { read_ms: Ts },
    /// The organisation settings changed (sent to everyone).
    OrgSettingsUpdate { settings: OrgSettings },
    /// A public channel appeared, changed or went away for people who are NOT in it (they only learn it exists through
    /// `GET /channels/browse`). `channel` is `None` when it is no longer browsable (deleted, archived or made private).
    /// Sent on create, rename, topic, picture, archive and visibility changes, not on every join or leave.
    ChannelDirectoryUpdate { conv_id: String, channel: Option<ChannelBrowseItem> },
    /// The server restored data for this user; discard the cache and bootstrap again.
    Resync {},
}

impl EventData {
    pub fn kind(&self) -> &'static str {
        match self {
            EventData::MessageNew { .. } => "message_new",
            EventData::MessageEdit { .. } => "message_edit",
            EventData::MessageDelete { .. } => "message_delete",
            EventData::MessageHide { .. } => "message_hide",
            EventData::ReactionUpdate { .. } => "reaction_update",
            EventData::MessageDelivered { .. } => "message_delivered",
            EventData::ConversationUpsert { .. } => "conversation_upsert",
            EventData::ConversationRemove { .. } => "conversation_remove",
            EventData::ReadReceipt { .. } => "read_receipt",
            EventData::ReadUpdate { .. } => "read_update",
            EventData::UserUpsert { .. } => "user_upsert",
            EventData::AttachmentRemoved { .. } => "attachment_removed",
            EventData::MeUpdate { .. } => "me_update",
            EventData::ThreadUpdate { .. } => "thread_update",
            EventData::PinsUpdate { .. } => "pins_update",
            EventData::BookmarksUpdate { .. } => "bookmarks_update",
            EventData::GroupsUpdate { .. } => "groups_update",
            EventData::PrefUpdate { .. } => "pref_update",
            EventData::SectionsUpdate { .. } => "sections_update",
            EventData::AvatarOverrideUpdate { .. } => "avatar_override_update",
            EventData::DraftUpdate { .. } => "draft_update",
            EventData::ScheduledUpdate { .. } => "scheduled_update",
            EventData::SavedUpdate { .. } => "saved_update",
            EventData::SavedRemove { .. } => "saved_remove",
            EventData::LabelsUpdate { .. } => "labels_update",
            EventData::ActivityRead { .. } => "activity_read",
            EventData::OrgSettingsUpdate { .. } => "org_settings_update",
            EventData::ChannelDirectoryUpdate { .. } => "channel_directory_update",
            EventData::Resync {} => "resync",
        }
    }
}

/// Decides whether an event with cursor `incoming` must be applied given the client's stored cursor.
/// Events are applied strictly in increasing cursor order; duplicates and replays are skipped.
pub fn should_apply(stored_cursor: i64, incoming: i64) -> bool {
    incoming > stored_cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_handling() {
        assert!(should_apply(0, 1));
        assert!(should_apply(10, 11));
        assert!(!should_apply(10, 10));
        assert!(!should_apply(10, 3));
    }

    #[test]
    fn event_json_shape() {
        let e = Event {
            cursor: 7,
            ts: 1,
            event: EventData::ConversationRemove { conv_id: "c1".into() },
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["event"]["type"], "conversation_remove");
        let back: Event = serde_json::from_value(v).unwrap();
        assert_eq!(back, e);
    }
}
