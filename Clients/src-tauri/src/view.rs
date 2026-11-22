//! View models: what the UI receives. The core resolves names, avatars and previews so the UI only draws.

use crate::markup;
use crate::store::{OutboxItem, Store};
use crate::error::Result;
use proto::*;
use serde::Serialize;
use std::collections::HashMap;

/// A message as the UI sees it: the server message plus the local send state.
#[derive(Debug, Clone, Serialize)]
pub struct MessageView {
    #[serde(flatten)]
    pub message: Message,
    /// `sending` (queued in the outbox) or `failed`; absent for messages the server has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Upload progress of an unsent message with files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upload: Option<UploadInfo>,
}

/// Progress of the files of one unsent message (all files together, in bytes).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UploadInfo {
    /// Index of the file being sent.
    pub index: usize,
    pub count: usize,
    pub sent: u64,
    pub total: u64,
    /// `waiting`, `uploading`, `sending`, `failed`.
    pub state: String,
}

impl MessageView {
    pub fn server(m: Message) -> Self {
        MessageView { message: m, pending: None, error: None, upload: None }
    }
    pub fn with_upload(mut self, u: Option<UploadInfo>) -> Self {
        self.upload = u;
        self
    }
    pub fn from_outbox(o: &OutboxItem, me: &str, seq: i64) -> Self {
        MessageView {
            message: Message {
                id: o.local_id(),
                conv_id: o.conv_id.clone(),
                seq,
                sender_id: me.to_string(),
                client_id: Some(o.client_id.clone()),
                body: o.body.clone(),
                kind: MessageKind::Text,
                ts: o.ts,
                version: 1,
                edited_ms: None,
                deleted: false,
                thread_root: o.thread_root.clone(),
                also_send_to_channel: o.also_send_to_channel,
                reply_to: o.reply_to.clone(),
                mentions: vec![],
                attachments: o
                    .attachments
                    .iter()
                    .enumerate()
                    .map(|(i, a)| Attachment { id: format!("up_{}_{}", o.client_id, i), name: a.name.clone(), content_type: a.content_type.clone(), size: a.size, removed: false, meta: a.meta.clone() })
                    .collect(),
                reactions: vec![],
                delivered: false,
                forwarded: None,
                form: None,
                responded: vec![],
            },
            pending: Some(if o.state == "failed" { "failed".into() } else { "sending".into() }),
            error: o.error.clone(),
            upload: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Preview {
    /// `none`, `text`, `voice`, `audio`, `image`, `video`, `file`, `deleted`, `system`.
    pub kind: String,
    pub text: String,
    /// "Sarah" for channel rows; absent for direct messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
    /// Small grey chip at the right of the row: "Audio", "PDF", ...
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatRow {
    pub id: String,
    /// `dm`, `self`, `group_dm`, `channel`.
    pub kind: String,
    pub title: String,
    pub topic: Option<String>,
    /// Attachment id of the picture to show (private override first, then the shared one).
    pub avatar: Option<String>,
    pub initials: String,
    pub peer_id: Option<String>,
    pub peer_role: Option<String>,
    pub is_bot: bool,
    pub presence: Option<String>,
    pub member_count: usize,
    pub preview: Preview,
    pub last_ts: i64,
    pub unread: i64,
    pub mentions: i64,
    pub marked_unread: bool,
    pub favourite: bool,
    pub muted: bool,
    pub position: Option<i64>,
    /// The last message is mine (draw ticks).
    pub last_own: bool,
    pub last_delivered: bool,
    pub has_pending: bool,
    pub archived: bool,
    pub last_seq: i64,
    /// Read receipts: how far the other members have read (user id -> sequence number), DMs and group DMs only.
    pub readers: std::collections::BTreeMap<String, i64>,
    /// Where the picture comes from: `local` (set on this device), `private`, `shared`, or absent.
    pub avatar_source: Option<String>,
    /// The person may pin messages here (any member, unless the organisation limits channel pins to managers).
    pub can_pin: bool,
    /// The person manages this chat: an admin, or its owner (channel manager). Rename, topic, picture, members, archive.
    pub can_manage: bool,
    /// The chat is locked (only seen while the locked area is open).
    pub locked: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub me: Me,
    pub server: String,
    pub users: Vec<User>,
    pub rows: Vec<ChatRow>,
    pub threads: Vec<ThreadSummary>,
    pub connection: String,
    pub counts: Option<Counts>,
    pub org_settings: OrgSettings,
    /// Ids of messages saved ("starred") by the person.
    pub saved: Vec<String>,
    pub local_avatars: Vec<LocalAvatar>,
    pub max_upload_bytes: u64,
    /// Pinned message ids per conversation.
    pub pins: std::collections::BTreeMap<String, Vec<String>>,
}

/// A picture chosen on this device for a person (`kind` = "user") or a chat (`kind` = "conv"). Never sent anywhere.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct LocalAvatar {
    pub kind: String,
    pub target: String,
    /// Changes whenever the picture changes, so the webview does not show a cached older one.
    pub version: i64,
}

impl LocalAvatar {
    /// The id the UI puts into `wcmedia://localhost/<id>`.
    pub fn media_id(&self) -> String {
        format!("la_{}_{}_{}", self.kind, self.target, self.version)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoreEvent {
    Conn { state: String, detail: Option<String> },
    Chat { row: ChatRow },
    ChatRemove { id: String },
    Message { message: MessageView, replaces: Option<String> },
    MessageRemove { conv_id: String, id: String },
    User { user: User },
    Thread { thread: ThreadSummary },
    ThreadRemove { root_id: String },
    Typing { conv_id: String, user_id: String },
    Presence { user_id: String, presence: Presence, last_active_ms: Option<i64> },
    Me { me: Me },
    /// The cache was replaced: ask for a fresh snapshot.
    Reset,
    SignedOut { code: String, message: String },
    Counts { counts: Counts },
    /// Live level while recording a voice message (0..1) and the time recorded so far.
    Voice { level: f32, elapsed_ms: u32 },
    /// Playback state of the audio player (`loading`, `playing`, `paused`, `ended`, `stopped`, `error`).
    Audio { id: String, state: String, pos_ms: u32, dur_ms: u32, speed: f32, error: Option<String> },
    Saved { ids: Vec<String> },
    Pins { conv_id: String, ids: Vec<String> },
    /// Progress of the files of an unsent message.
    Upload { local_id: String, info: UploadInfo },
    LocalAvatars { items: Vec<LocalAvatar> },
    /// Progress of an app update download: `downloading`, `ready`, `error`, `needs_permission`, `installing`.
    Update { state: String, got: u64, total: u64, message: String },
    /// Organisation settings changed (who may create channels, who may pin).
    OrgSettings { settings: OrgSettings },
}

// ---------------------------------------------------------------- chat rows

/// Everything needed to build rows, loaded once per batch.
pub struct RowCtx {
    pub me: String,
    pub users: HashMap<String, User>,
    pub prefs: HashMap<String, ConvPref>,
    pub overrides: HashMap<(String, String), String>,
    pub local: HashMap<(String, String), String>,
    pub locked: std::collections::HashSet<String>,
    pub org: OrgSettings,
}

impl RowCtx {
    pub fn load(store: &Store) -> Result<RowCtx> {
        let me: String = store.kv_get("user_id")?.unwrap_or_default();
        let users = store.all_users()?.into_iter().map(|u| (u.id.clone(), u)).collect();
        let prefs = store.all_prefs()?.into_iter().map(|p| (p.conv_id.clone(), p)).collect();
        let ov: Vec<AvatarOverride> = store.kv_get("avatar_overrides")?.unwrap_or_default();
        let overrides = ov.into_iter().map(|o| ((o.kind, o.target), o.attachment)).collect();
        let la: Vec<LocalAvatar> = store.kv_get("local_avatars")?.unwrap_or_default();
        let local = la.into_iter().map(|a| ((a.kind.clone(), a.target.clone()), a.media_id())).collect();
        let locked = crate::lock::locked_set(store);
        let org = store.kv_get::<OrgSettings>("org_settings").ok().flatten().unwrap_or_default();
        Ok(RowCtx { me, users, prefs, overrides, local, locked, org })
    }

    fn name_of(&self, id: &str) -> String {
        self.users.get(id).map(|u| u.display_name.clone()).unwrap_or_else(|| "Someone".into())
    }
}

pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split(|c: char| c.is_whitespace() || c == '-' || c == '_' || c == '.').filter(|w| !w.is_empty()).collect();
    let mut out = String::new();
    for w in words.iter().take(2) {
        if let Some(c) = w.chars().find(|c| c.is_alphanumeric()) {
            out.extend(c.to_uppercase());
        }
    }
    if out.is_empty() {
        out.push('?');
    }
    out
}

/// "Audio", "PDF", ... for the grey chip of a file row.
pub fn file_chip(a: &Attachment) -> String {
    let ext = a.name.rsplit_once('.').map(|(_, e)| e.to_ascii_uppercase()).unwrap_or_default();
    if !ext.is_empty() && ext.len() <= 4 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return ext;
    }
    "File".into()
}

pub fn attachment_kind(a: &Attachment) -> &'static str {
    let ct = a.content_type.to_ascii_lowercase();
    if ct.starts_with("audio/") {
        let voice = a.meta.as_ref().map(|m| !m.peaks.is_empty()).unwrap_or(false) || a.name.to_ascii_lowercase().starts_with("voice");
        if voice {
            "voice"
        } else {
            "audio"
        }
    } else if ct.starts_with("image/") {
        "image"
    } else if ct.starts_with("video/") {
        "video"
    } else {
        "file"
    }
}

pub fn make_preview(m: &Message, ctx: &RowCtx, group: bool) -> Preview {
    let sender = if group {
        Some(if m.sender_id == ctx.me { "You".to_string() } else { ctx.name_of(&m.sender_id).split_whitespace().next().unwrap_or("Someone").to_string() })
    } else {
        None
    };
    if m.deleted {
        return Preview { kind: "deleted".into(), text: "Message deleted".into(), sender, ..Default::default() };
    }
    if m.kind == MessageKind::System {
        return Preview { kind: "system".into(), text: markup::to_plain(&m.body), ..Default::default() };
    }
    let text = markup::to_plain(&m.body);
    if let Some(a) = m.attachments.first() {
        if a.removed {
            return Preview { kind: "file".into(), text: "File removed".into(), sender, ..Default::default() };
        }
        let kind = attachment_kind(a);
        let more = if m.attachments.len() > 1 { format!(" (+{})", m.attachments.len() - 1) } else { String::new() };
        let dur = a.meta.as_ref().and_then(|x| x.duration_ms);
        return match kind {
            "voice" | "audio" => Preview {
                kind: kind.into(),
                text: if kind == "voice" { "Voice message".into() } else if text.is_empty() { a.name.clone() } else { text },
                sender,
                chip: Some("Audio".into()),
                duration_ms: dur,
            },
            "image" => Preview { kind: "image".into(), text: if text.is_empty() { format!("Photo{more}") } else { text }, sender, chip: None, duration_ms: None },
            "video" => Preview { kind: "video".into(), text: if text.is_empty() { format!("Video{more}") } else { text }, sender, chip: None, duration_ms: dur },
            _ => Preview { kind: "file".into(), text: if text.is_empty() { format!("Uploaded {}{more}", a.name) } else { text }, sender, chip: Some(file_chip(a)), duration_ms: None },
        };
    }
    Preview { kind: "text".into(), text, sender, chip: None, duration_ms: None }
}

pub fn build_row(store: &Store, ctx: &RowCtx, cs: &ConversationSummary) -> Result<ChatRow> {
    let is_admin = ctx.users.get(&ctx.me).map(|u| u.role.is_admin()).unwrap_or(false);
    let is_owner = cs.conversation.members.iter().any(|m| m.user_id == ctx.me && m.role == "owner");
    let can_manage = (is_admin || is_owner) && cs.conversation.kind != ConversationKind::Dm && cs.conversation.kind != ConversationKind::SelfNote;
    let can_pin = cs.conversation.kind != ConversationKind::Channel || ctx.org.channel_pins == ChannelPins::Everyone || is_admin || is_owner;
    let c = &cs.conversation;
    let kind = c.kind.as_str().to_string();
    let peer = if c.kind == ConversationKind::Dm { c.members.iter().find(|m| m.user_id != ctx.me).and_then(|m| ctx.users.get(&m.user_id)) } else { None };
    let title = match c.kind {
        ConversationKind::Dm => peer.map(|u| u.display_name.clone()).unwrap_or_else(|| "Direct message".into()),
        ConversationKind::SelfNote => "Notes to self".into(),
        ConversationKind::GroupDm => c.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| {
            let names: Vec<String> = c.members.iter().filter(|m| m.user_id != ctx.me).map(|m| ctx.name_of(&m.user_id).split_whitespace().next().unwrap_or("").to_string()).collect();
            names.join(", ")
        }),
        ConversationKind::Channel => c.name.clone().unwrap_or_else(|| "channel".into()),
    };
    let ck = ("conv".to_string(), c.id.clone());
    let (avatar, avatar_source) = match (c.kind, peer) {
        (ConversationKind::Dm, Some(u)) => {
            let uk = ("user".to_string(), u.id.clone());
            if let Some(l) = ctx.local.get(&ck).or_else(|| ctx.local.get(&uk)) {
                (Some(l.clone()), Some("local".to_string()))
            } else if let Some(o) = ctx.overrides.get(&ck).or_else(|| ctx.overrides.get(&uk)) {
                (Some(o.clone()), Some("private".to_string()))
            } else {
                (u.avatar.clone(), u.avatar.as_ref().map(|_| "shared".to_string()))
            }
        }
        _ => {
            if let Some(l) = ctx.local.get(&ck) {
                (Some(l.clone()), Some("local".to_string()))
            } else if let Some(o) = ctx.overrides.get(&ck) {
                (Some(o.clone()), Some("private".to_string()))
            } else {
                (c.avatar.clone(), c.avatar.as_ref().map(|_| "shared".to_string()))
            }
        }
    };
    let readers: std::collections::BTreeMap<String, i64> = if matches!(c.kind, ConversationKind::Dm | ConversationKind::GroupDm) {
        c.members.iter().filter(|m| m.user_id != ctx.me).filter_map(|m| m.last_read_seq.map(|s| (m.user_id.clone(), s))).collect()
    } else {
        Default::default()
    };
    let last = store.last_message(&c.id)?;
    let group = !matches!(c.kind, ConversationKind::Dm | ConversationKind::SelfNote);
    let preview = last.as_ref().map(|m| make_preview(m, ctx, group)).unwrap_or_else(|| Preview { kind: "none".into(), ..Default::default() });
    let pending = store.outbox_for_conv(&c.id)?;
    let pref = ctx.prefs.get(&c.id);
    let last_ts = pending.iter().map(|o| o.ts).max().or(last.as_ref().map(|m| m.ts)).or(c.last_message_ts).unwrap_or(c.created_ms);
    Ok(ChatRow {
        id: c.id.clone(),
        kind,
        initials: initials(&title),
        title,
        topic: c.topic.clone(),
        avatar,
        peer_id: peer.map(|u| u.id.clone()),
        peer_role: peer.map(|u| u.role.as_str().to_string()),
        is_bot: peer.map(|u| u.is_bot).unwrap_or(false),
        presence: peer.and_then(|u| u.presence).map(|p| serde_json::to_value(p).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()),
        member_count: c.members.len(),
        preview,
        last_ts,
        unread: cs.unread,
        mentions: cs.mentions,
        marked_unread: cs.marked_unread,
        favourite: pref.map(|p| p.favourite).unwrap_or(false),
        muted: pref.map(|p| p.is_muted(crate::api::now_ms())).unwrap_or(false),
        position: pref.and_then(|p| p.position),
        last_own: last.as_ref().map(|m| m.sender_id == ctx.me && !m.deleted && m.kind != MessageKind::System).unwrap_or(false),
        last_delivered: last.as_ref().map(|m| m.delivered).unwrap_or(false),
        has_pending: !pending.is_empty(),
        archived: c.archived,
        last_seq: c.last_seq,
        readers,
        avatar_source,
        locked: ctx.locked.contains(&c.id),
        can_pin: can_pin,
        can_manage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn att(name: &str, ct: &str, meta: Option<AttachmentMeta>) -> Attachment {
        Attachment { id: "a1".into(), name: name.into(), content_type: ct.into(), size: 10, removed: false, meta }
    }

    fn msg(body: &str, attachments: Vec<Attachment>) -> Message {
        Message {
            id: "m1".into(), conv_id: "c1".into(), seq: 1, sender_id: "u2".into(), client_id: None, body: body.into(), kind: MessageKind::Text, ts: 1,
            version: 1, edited_ms: None, deleted: false, thread_root: None, also_send_to_channel: false, reply_to: None, mentions: vec![], attachments, reactions: vec![],
            delivered: false, forwarded: None, form: None, responded: vec![],
        }
    }

    fn ctx() -> RowCtx {
        let mut users = HashMap::new();
        users.insert("u2".to_string(), User { id: "u2".into(), username: "sarah".into(), display_name: "Sarah Chen".into(), role: Role::Member, avatar: None, disabled: false, is_bot: false, last_active_ms: None, presence: None, title: None, phone: None });
        RowCtx { me: "u1".into(), users, prefs: HashMap::new(), overrides: HashMap::new(), local: HashMap::new(), locked: Default::default(), org: Default::default() }
    }

    #[test]
    fn initials_work() {
        assert_eq!(initials("Liam Chen"), "LC");
        assert_eq!(initials("Prince"), "P");
        assert_eq!(initials("anna-marie smith"), "AM");
        assert_eq!(initials(""), "?");
    }

    #[test]
    fn previews() {
        let c = ctx();
        let p = make_preview(&msg("*Hello* there", vec![]), &c, true);
        assert_eq!((p.kind.as_str(), p.text.as_str(), p.sender.as_deref()), ("text", "Hello there", Some("Sarah")));
        let p = make_preview(&msg("", vec![att("voice-1.ogg", "audio/ogg", Some(AttachmentMeta { duration_ms: Some(42_000), peaks: vec![1, 2], ..Default::default() }))]), &c, false);
        assert_eq!((p.kind.as_str(), p.chip.as_deref(), p.duration_ms, p.sender), ("voice", Some("Audio"), Some(42_000), None));
        let p = make_preview(&msg("", vec![att("contract.pdf", "application/pdf", None)]), &c, false);
        assert_eq!((p.kind.as_str(), p.text.as_str(), p.chip.as_deref()), ("file", "Uploaded contract.pdf", Some("PDF")));
        let p = make_preview(&msg("", vec![att("a.png", "image/png", None)]), &c, false);
        assert_eq!((p.kind.as_str(), p.text.as_str()), ("image", "Photo"));
        let mut m = msg("x", vec![]);
        m.deleted = true;
        assert_eq!(make_preview(&m, &c, false).kind, "deleted");
        let mut mine = msg("mine", vec![]);
        mine.sender_id = "u1".into();
        assert_eq!(make_preview(&mine, &c, true).sender.as_deref(), Some("You"));
    }

    #[test]
    fn row_for_dm_uses_peer_name_and_override_avatar() {
        let store = Store::open_memory().unwrap();
        store.kv_set("user_id", &"u1").unwrap();
        let mut c = ctx();
        c.overrides.insert(("user".into(), "u2".into()), "att9".into());
        let conv = Conversation { id: "c1".into(), kind: ConversationKind::Dm, name: None, topic: None, archived: false, created_ms: 1, members: vec![Member { user_id: "u1".into(), role: "member".into(), last_read_seq: None }, Member { user_id: "u2".into(), role: "member".into(), last_read_seq: None }], last_seq: 1, last_message_ts: Some(5), avatar: None };
        let cs = ConversationSummary { conversation: conv, last_read_seq: 0, marked_unread: false, unread: 2, mentions: 0 };
        store.upsert_message(&msg("hi", vec![])).unwrap();
        let row = build_row(&store, &c, &cs).unwrap();
        assert_eq!((row.title.as_str(), row.avatar.as_deref(), row.unread, row.initials.as_str()), ("Sarah Chen", Some("att9"), 2, "SC"));
        assert_eq!(row.preview.sender, None);
    }
}
