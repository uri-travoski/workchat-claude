use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Milliseconds since the Unix epoch, UTC.
pub type Ts = i64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Admin,
    Member,
}

impl Role {
    pub fn is_admin(self) -> bool {
        matches!(self, Role::Owner | Role::Admin)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::Member => "member",
        }
    }
    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "owner" => Some(Role::Owner),
            "admin" => Some(Role::Admin),
            "member" => Some(Role::Member),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationKind {
    Dm,
    /// Notes to self.
    #[serde(rename = "self")]
    SelfNote,
    GroupDm,
    /// A channel: private (members only) or public (anyone can browse and join), see `Conversation.visibility`.
    Channel,
}

impl ConversationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ConversationKind::Dm => "dm",
            ConversationKind::SelfNote => "self",
            ConversationKind::GroupDm => "group_dm",
            ConversationKind::Channel => "channel",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "dm" => Some(Self::Dm),
            "self" => Some(Self::SelfNote),
            "group_dm" => Some(Self::GroupDm),
            "channel" => Some(Self::Channel),
            _ => None,
        }
    }
}

/// Who can find and join a channel. Group DMs, DMs and notes are always `private`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelVisibility {
    /// Listed by `GET /channels/browse` for people who are not in it; anyone can `POST /conversations/{id}/join`.
    Public,
    /// Members only, invisible to everyone else.
    #[default]
    Private,
}

impl ChannelVisibility {
    pub fn as_str(self) -> &'static str {
        match self {
            ChannelVisibility::Public => "public",
            ChannelVisibility::Private => "private",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "public" => Some(Self::Public),
            "private" => Some(Self::Private),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    Online,
    Away,
    Dnd,
    Offline,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct User {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    /// Attachment id of the profile photo; clients show initials when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub is_bot: bool,
    /// Never present when the user has chosen to hide it (SV-12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_active_ms: Option<Ts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence: Option<Presence>,
    /// Job title shown on the profile and in the contacts list ("Head of Product Operations").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Work phone number as typed by the person or the admin; free text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Me {
    pub user: User,
    /// True when an admin enabled the Privacy screen for this user (SV-12).
    pub privacy_controls_enabled: bool,
    pub hide_last_active: bool,
    /// What the user chose: "auto" (online while connected), "away" or "dnd".
    pub presence_pref: String,
    /// False for accounts created with a one-time link that have not set a password yet.
    pub password_set: bool,
    pub email: Option<String>,
    /// Read receipts (default on): when off, other people do not see how far you have read in DMs and group chats.
    #[serde(default = "default_true")]
    pub read_receipts: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Member {
    pub user_id: String,
    /// "owner" or "member" (conversation-local role). The conversation `owner` is the channel manager: they can rename,
    /// set topic and picture, add and remove members, archive and (always) pin. The creator of a channel or group DM gets it.
    pub role: String,
    /// Read receipts: how far this member has read (conversation sequence number). Only set in `dm` and `group_dm`
    /// conversations and only when the member has read receipts switched on. A message of yours is "read" in a DM when
    /// the other member's `last_read_seq` >= its `seq`. Kept current by the `read_receipt` event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_read_seq: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Conversation {
    pub id: String,
    pub kind: ConversationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(default)]
    pub archived: bool,
    pub created_ms: Ts,
    pub members: Vec<Member>,
    /// Highest message sequence number in this conversation.
    pub last_seq: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message_ts: Option<Ts>,
    /// The picture of a channel or group DM that everyone sees (an attachment id with purpose `avatar`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// `public` or `private` (channels only; everything else is `private`). Channels that existed before 1.1.0 are `private`.
    #[serde(default)]
    pub visibility: ChannelVisibility,
}

/// A conversation plus the calling user's own state in it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationSummary {
    #[serde(flatten)]
    pub conversation: Conversation,
    pub last_read_seq: i64,
    pub marked_unread: bool,
    pub unread: i64,
    pub mentions: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Text,
    File,
    System,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub content_type: String,
    pub size: i64,
    /// True when an admin removed the file; clients show "File removed".
    #[serde(default)]
    pub removed: bool,
    /// M-30/M-31: duration, waveform peaks, size and poster frame of voice messages and videos (set by the uploader).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<AttachmentMeta>,
}

/// Media details the sender's app computes before sending: used to draw waveforms and video posters without
/// downloading the file first. Everything is optional; the server only checks the size and field limits.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AttachmentMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    /// Loudness per slice of the recording, 0..=255, 64 values at most.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub peaks: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// A small JPEG of the first video frame, base64 (at most about 40 KB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster_b64: Option<String>,
    /// Page count of a PDF or slide deck ("4.2 MB - 28 pages" on document cards).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<u32>,
}

impl AttachmentMeta {
    pub const MAX_JSON_BYTES: usize = 64 * 1024;
    pub fn valid(&self) -> bool {
        self.peaks.len() <= 64 && self.poster_b64.as_ref().map(|p| p.len() <= 56 * 1024).unwrap_or(true) && self.duration_ms.map(|d| d <= 6 * 3_600_000).unwrap_or(true)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReactionSummary {
    pub emoji: String,
    pub user_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    pub id: String,
    pub conv_id: String,
    /// Conversation-local, gapless, increasing.
    pub seq: i64,
    pub sender_id: String,
    /// Client-generated id for idempotent sends (only echoed to the sender's devices).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Plain text in the small markup (see docs/API.md). Empty for tombstones.
    pub body: String,
    pub kind: MessageKind,
    pub ts: Ts,
    /// 1 for a never edited message.
    pub version: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_ms: Option<Ts>,
    /// Tombstone: shown as "Message deleted".
    #[serde(default)]
    pub deleted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    /// Structured @mentions (user ids).
    #[serde(default)]
    pub mentions: Vec<String>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub reactions: Vec<ReactionSummary>,
    /// Delivered to at least one recipient device.
    #[serde(default)]
    pub delivered: bool,
    /// M-20: set when this message was forwarded from another conversation (attribution).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forwarded: Option<ForwardInfo>,
    /// SV-17: interactive form sent by a bot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<crate::automation::Form>,
    /// Users who already submitted the form.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub responded: Vec<String>,
    /// "Also send to channel": a thread reply (`thread_root` is set) that also shows in the conversation's main timeline.
    /// It is one message, not a copy: show it in the timeline as a reply to the thread root (`thread_root`), and in the
    /// thread as usual. Absent means false. Always false for messages that are not thread replies.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub also_send_to_channel: bool,
}

/// Attribution of a forwarded message (M-20).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ForwardInfo {
    pub sender_id: String,
    pub conv_id: String,
    pub message_id: String,
    pub ts: Ts,
}

impl Message {
    pub fn is_edited(&self) -> bool {
        self.version > 1 && !self.deleted
    }
}

// ---------------------------------------------------------------- meta & auth

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ServerMeta {
    pub name: String,
    pub api_version: u32,
    pub min_client_version: String,
    pub server_version: String,
    /// True until the first owner account has been created.
    pub setup_required: bool,
    pub max_upload_bytes: u64,
    pub ai_search_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ntfy_url: Option<String>,
    #[serde(default)]
    pub fcm_enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub client_version: String,
}

/// Sign in with the one-time token from an admin-created login link.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LinkLoginRequest {
    pub token: String,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub client_version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    /// Seconds until `access_token` expires.
    pub expires_in: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LoginResponse {
    #[serde(flatten)]
    pub tokens: TokenPair,
    pub session_id: String,
    pub me: Me,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChangePasswordRequest {
    #[serde(default)]
    pub current_password: Option<String>,
    pub new_password: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct UpdateMeRequest {
    pub display_name: Option<String>,
    /// "auto", "away" or "dnd".
    pub presence_pref: Option<String>,
    /// Only honoured when `privacy_controls_enabled` is on for the user.
    pub hide_last_active: Option<bool>,
    /// Attachment id of an uploaded image; empty string clears it.
    pub avatar: Option<String>,
    /// Job title (at most 80 characters); empty string clears it.
    pub title: Option<String>,
    /// Phone number (at most 40 characters); empty string clears it.
    pub phone: Option<String>,
    /// Switch read receipts on or off.
    pub read_receipts: Option<bool>,
}

// ---------------------------------------------------------------- sync

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Bootstrap {
    /// Start `events` / the WebSocket from this cursor after applying the snapshot.
    pub cursor: i64,
    pub server_time: Ts,
    pub me: Me,
    pub users: Vec<User>,
    pub conversations: Vec<ConversationSummary>,
    /// Latest messages per conversation (conversation id -> ascending by seq).
    pub messages: BTreeMap<String, Vec<Message>>,
    /// Per conversation: true when older messages exist on the server.
    pub has_more: BTreeMap<String, bool>,
    /// Per-conversation favourites, sections, mute and notification level (NV-01..NV-07, N-03).
    #[serde(default)]
    pub prefs: Vec<ConvPref>,
    /// The caller's private pictures for other users and conversations.
    #[serde(default)]
    pub avatar_overrides: Vec<AvatarOverride>,
    #[serde(default)]
    pub sections: Vec<Section>,
    /// User groups (`@design-team`), M-18.
    #[serde(default)]
    pub groups: Vec<UserGroup>,
    /// Synced drafts (M-22).
    #[serde(default)]
    pub drafts: Vec<DraftItem>,
    /// Threads the user follows, newest activity first (M-06, M-07).
    #[serde(default)]
    pub threads: Vec<ThreadSummary>,
    /// Pins and bookmarks per conversation (SV-09).
    #[serde(default)]
    pub pins: BTreeMap<String, Vec<Pin>>,
    #[serde(default)]
    pub bookmarks: BTreeMap<String, Vec<Bookmark>>,
    /// Saved messages ("Later") and the user's labels.
    #[serde(default)]
    pub saved: Vec<SavedItem>,
    #[serde(default)]
    pub labels: Vec<Label>,
    /// Activity items at or before this time are read.
    #[serde(default)]
    pub activity_read_ms: Ts,
    /// The organisation settings (who can create channels, who can pin in channels).
    #[serde(default)]
    pub org_settings: OrgSettings,
}

/// A pinned message (C-09). At most 100 per conversation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Pin {
    pub conv_id: String,
    pub message_id: String,
    pub pinned_by: String,
    pub ts: Ts,
}

/// A channel bookmark (C-07): a link or a file shortcut shown above the timeline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Bookmark {
    pub id: String,
    pub conv_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment_id: Option<String>,
    pub position: i64,
    pub created_by: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BookmarkRequest {
    pub title: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub attachment_id: Option<String>,
    #[serde(default)]
    pub position: Option<i64>,
}

/// A user group (`@handle`) that expands to member mentions when a message is sent (M-18).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UserGroup {
    pub id: String,
    pub handle: String,
    pub name: String,
    pub member_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UserGroupRequest {
    pub handle: Option<String>,
    pub name: Option<String>,
    pub member_ids: Option<Vec<String>>,
}

/// Per-user state of one conversation (synced across devices).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConvPref {
    pub conv_id: String,
    #[serde(default)]
    pub favourite: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_id: Option<String>,
    /// Muted until this time (UTC ms); `-1` means forever. Absent: not muted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mute_until_ms: Option<Ts>,
    /// "all", "mentions" or "nothing" (N-03).
    pub notify: String,
    /// NV-04: the user's own order inside the section (lower first). Absent: not reordered, sorted by the list setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
}

impl ConvPref {
    pub fn default_for(conv_id: &str) -> ConvPref {
        ConvPref { conv_id: conv_id.to_string(), favourite: false, section_id: None, mute_until_ms: None, notify: "all".into(), position: None }
    }
    pub fn is_muted(&self, now: Ts) -> bool {
        matches!(self.mute_until_ms, Some(-1)) || self.mute_until_ms.map(|t| t > now).unwrap_or(false)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ConvPrefRequest {
    pub favourite: Option<bool>,
    /// Empty string removes the conversation from its section.
    pub section_id: Option<String>,
    /// `0` unmutes, `-1` mutes forever, otherwise mute until that time.
    pub mute_until_ms: Option<Ts>,
    pub notify: Option<String>,
}

/// A custom sidebar section (NV-04).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Section {
    pub id: String,
    pub name: String,
    pub position: i64,
    #[serde(default)]
    pub collapsed: bool,
}

impl Section {
    /// NV-04: the built-in sections (Favourites, Channels, Direct messages) are stored as order-only rows with ids
    /// `builtin:<key>:<user id>`; returns `fav`, `channels` or `dms` for them, None for a real custom section.
    pub fn builtin_key(&self) -> Option<&str> {
        self.id.strip_prefix("builtin:").and_then(|r| r.split(':').next())
    }
}

/// NV-04: the new order of some conversations in a section. An empty list clears every manual order of the user.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ConvOrderRequest {
    pub conv_ids: Vec<String>,
}

/// NV-04: the new order of the sidebar sections: custom section ids and the keys `builtin:fav`, `builtin:channels`,
/// `builtin:dms`. An empty list puts the built-in sections back in their default places (custom ones keep their relative order).
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct SectionOrderRequest {
    pub ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct SectionRequest {
    pub name: Option<String>,
    pub position: Option<i64>,
    pub collapsed: Option<bool>,
}

/// A thread the user follows (M-06).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ThreadSummary {
    pub conv_id: String,
    pub root: Message,
    pub reply_count: i64,
    pub last_reply_ts: Ts,
    pub unread: i64,
    pub following: bool,
    pub participants: Vec<String>,
    /// The newest reply (for the thread preview line); absent while there are no replies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_reply: Option<Message>,
}

/// A thread root plus its replies (M-05).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ThreadView {
    pub summary: ThreadSummary,
    pub replies: Vec<Message>,
}

/// A draft, per conversation or per thread (M-22). An empty body means "no draft".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DraftItem {
    pub conv_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_root: Option<String>,
    pub body: String,
    pub updated_ms: Ts,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DraftRequest {
    #[serde(default)]
    pub thread_root: Option<String>,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ForwardRequest {
    pub conv_ids: Vec<String>,
    /// Optional note posted with the forwarded message.
    #[serde(default)]
    pub comment: Option<String>,
}

/// A push registration (SV-13). `kind` is "fcm", "ntfy", "unifiedpush" or "apns".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PushRegistration {
    #[serde(default)]
    pub id: String,
    pub kind: String,
    /// FCM / APNs device token, the ntfy topic, or (kind "unifiedpush") the full endpoint URL the distributor gave the app.
    pub target: String,
    /// ntfy server URL (falls back to the server's configured one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ntfy_url: Option<String>,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub created_ms: Ts,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HistoryResponse {
    pub messages: Vec<Message>,
    pub has_more_before: bool,
    pub has_more_after: bool,
}

/// `POST /conversations`. Creating a `channel` is open to everyone unless the organisation setting `channel_creation`
/// is `admins`; creating a `group_dm` is always open to everyone. The creator joins as a member with the conversation
/// role `owner` (channel manager).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateConversation {
    /// `channel` or `group_dm`.
    pub kind: ConversationKind,
    pub name: String,
    #[serde(default)]
    pub topic: Option<String>,
    /// The other people to add (the creator is added automatically).
    #[serde(default)]
    pub members: Vec<String>,
    /// Channels only: `public` (default) or `private`. A `group_dm` is always private.
    #[serde(default)]
    pub visibility: Option<ChannelVisibility>,
    /// Admins only: set `false` to create the conversation without joining it (the admin panel does this). Default true.
    #[serde(default)]
    pub include_creator: Option<bool>,
}

/// `PATCH /conversations/{id}` (admins and the conversation's managers).
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct UpdateConversation {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub archived: Option<bool>,
    /// Channels only.
    #[serde(default)]
    pub visibility: Option<ChannelVisibility>,
}

/// Who may create channels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelCreation {
    #[default]
    Everyone,
    Admins,
}

/// Who may pin messages in channels (DMs and group DMs: always every member).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelPins {
    /// Every member of the channel.
    #[default]
    Everyone,
    /// Channel managers (conversation role `owner`) and admins only.
    Managers,
}

/// Organisation-wide settings, `GET /org-settings` (any signed-in user), changed by admins with `PUT /admin/org-settings`.
/// Also part of the bootstrap and kept current by the `org_settings_update` event.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OrgSettings {
    /// Who can create channels: `everyone` (default) or `admins`.
    #[serde(default)]
    pub channel_creation: ChannelCreation,
    /// Who can pin in channels: `everyone` (default) or `managers`.
    #[serde(default)]
    pub channel_pins: ChannelPins,
}

/// `PUT /admin/org-settings`: only the given fields change.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct UpdateOrgSettings {
    #[serde(default)]
    pub channel_creation: Option<ChannelCreation>,
    #[serde(default)]
    pub channel_pins: Option<ChannelPins>,
}

/// A public channel as listed by `GET /channels/browse`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelBrowseItem {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    /// Attachment id of the channel picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    pub member_count: i64,
    pub created_ms: Ts,
}

/// `GET /channels/browse?q=&limit=&offset=`: public, not archived channels the caller is not in, by name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelBrowsePage {
    pub channels: Vec<ChannelBrowseItem>,
    /// All matching channels, not just this page.
    pub total: i64,
    pub has_more: bool,
}

/// `PUT /conversations/{id}/avatar`. Owners and admins set the picture everyone in the channel or group sees.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetConvAvatar {
    /// Attachment id of an uploaded image (`purpose=avatar`); an empty string removes the picture.
    pub avatar: String,
}

/// A private picture the caller chose for another user (`kind` = "user") or a conversation (`kind` = "conv").
/// Only the caller ever receives it; it wins over the person's or channel's own picture on the caller's devices.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AvatarOverride {
    pub kind: String,
    pub target: String,
    /// Attachment id (purpose `avatar_private`).
    pub attachment: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetAvatarOverride {
    /// Attachment id of an uploaded image (`purpose=avatar_private`).
    pub attachment: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateDm {
    pub user_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SendMessage {
    /// Idempotency key generated by the client; retries never duplicate.
    pub client_id: String,
    pub body: String,
    #[serde(default)]
    pub thread_root: Option<String>,
    #[serde(default)]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub mentions: Vec<String>,
    #[serde(default)]
    pub attachment_ids: Vec<String>,
    /// With `thread_root`: also show this reply in the conversation's main timeline ("Also send to channel"). Still one message.
    #[serde(default)]
    pub also_send_to_channel: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EditMessage {
    pub body: String,
    #[serde(default)]
    pub mentions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReactRequest {
    pub emoji: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct MarkReadRequest {
    /// Mark everything up to and including this sequence number as read.
    pub seq: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UploadResponse {
    pub attachment: Attachment,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SearchHit {
    pub message: Message,
    /// Snippet with matches wrapped in `[[` and `]]`.
    pub snippet: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    pub people: Vec<User>,
    /// Attachments whose file name matches (SR-03).
    #[serde(default)]
    pub files: Vec<FileHit>,
    /// Channels and group DMs of the caller whose name matches (SR-05).
    #[serde(default)]
    pub channels: Vec<ConversationSummary>,
}

/// A file in the Files browser / file search (F-05, SR-03).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FileHit {
    pub attachment: Attachment,
    pub message_id: String,
    pub conv_id: String,
    pub sender_id: String,
    pub ts: Ts,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionInfo {
    pub id: String,
    pub user_id: String,
    #[serde(default)]
    pub username: String,
    pub device_name: String,
    pub client_version: String,
    pub ip: String,
    pub created_ms: Ts,
    pub last_seen_ms: Ts,
    pub current: bool,
    pub revoked: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EventsResponse {
    pub events: Vec<crate::events::Event>,
    /// Cursor to continue from (the last returned event, or the request cursor if none).
    pub cursor: i64,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SetupRequest {
    pub username: String,
    pub display_name: String,
    pub password: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub storage: Option<crate::admin::StorageConfig>,
}

// ---------------------------------------------------------------- saved items and labels

/// A label the user created for organising saved messages. Private to the user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Label {
    pub id: String,
    pub name: String,
    /// `#rrggbb`.
    pub color: String,
    pub position: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct LabelRequest {
    pub name: Option<String>,
    pub color: Option<String>,
    pub position: Option<i64>,
}

/// A message the user saved for later. The message itself is read from the normal message cache.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SavedItem {
    pub message_id: String,
    pub conv_id: String,
    pub saved_ms: Ts,
    #[serde(default)]
    pub label_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct SaveRequest {
    /// Optional labels to set while saving.
    #[serde(default)]
    pub label_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SavedLabelsRequest {
    pub label_ids: Vec<String>,
}

/// One entry of the Activity feed: a mention, a reaction to your message or a reply in a thread you are in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ActivityItem {
    /// Stable id: `<kind>:<message id>[:<user id>:<emoji>]`.
    pub id: String,
    /// `mention`, `reaction` or `thread_reply`.
    pub kind: String,
    pub ts: Ts,
    pub conv_id: String,
    /// The message the item is about (your message for a reaction, the mentioning or replying message otherwise).
    pub message_id: String,
    pub actor_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_root: Option<String>,
    pub unread: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ActivityFeed {
    pub items: Vec<ActivityItem>,
    pub read_ms: Ts,
    pub unread: i64,
}

/// `GET /me/counts`: the numbers behind the filter pills (All / DMs / Channels / Threads / Files). Cheap to compute;
/// clients refresh it on start and whenever a `thread_update`, `conversation_upsert` or `message_new` event arrives.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Counts {
    /// Conversations you are in, excluding archived ones and notes to self.
    pub conversations: i64,
    /// Direct messages and group DMs.
    pub dms: i64,
    pub channels: i64,
    /// Conversations with unread messages or marked unread.
    pub unread_conversations: i64,
    pub unread_dms: i64,
    pub unread_channels: i64,
    /// Unread messages across all conversations (thread replies included).
    pub unread_messages: i64,
    pub mentions: i64,
    /// Threads you take part in (follow).
    pub threads: i64,
    /// Threads with replies you have not read.
    pub threads_unread: i64,
    /// Files in your conversations (not removed, not deleted messages).
    pub files: i64,
    pub files_image: i64,
    pub files_video: i64,
    pub files_audio: i64,
    pub files_document: i64,
    /// Saved messages.
    pub saved: i64,
}

/// `PUT /admin/messages/{id}/ts` (owner or admin): sets the time a message shows. For importing history and for the seed tool.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct AdminSetMessageTs {
    /// UTC ms. Must not be in the future and must stay between the neighbouring messages' times of the conversation.
    pub ts: Ts,
}

/// `POST /messages/{id}/hide` ("Delete for me"): the message is hidden for the caller only, on all their devices.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HideMessageResponse {
    pub message_id: String,
    pub conv_id: String,
}
