//! Applying the server's ordered event log to the local cache. Pure with respect to the network: it only reads and
//! writes the [`Store`] and returns the [`CoreEvent`]s the UI should get, so it can be tested without a server.

use crate::error::Result;
use crate::store::Store;
use crate::view::{build_row, saved_event, CoreEvent, MessageView, RowCtx};
use proto::*;
use std::collections::BTreeSet;

#[derive(Default)]
pub struct Applied {
    pub events: Vec<CoreEvent>,
    /// Conversations whose summary should be re-read from the server (a new message arrived from someone else).
    pub refresh: Vec<String>,
    /// The server asked for a full re-sync.
    pub resync: bool,
    pub cursor: Option<i64>,
    /// Something happened that changes the numbers on the filter pills: refresh `GET /me/counts`.
    pub counts_dirty: bool,
    /// Messages from other people that arrived as new (for notifications).
    pub new_messages: Vec<Message>,
}

/// Messages the person deleted for themselves (or an admin cleared): a pin may still point at one.
pub fn remember_hidden(store: &Store, ids: &[String]) {
    let mut set: Vec<String> = store.kv_get("hidden_msgs").ok().flatten().unwrap_or_default();
    for id in ids {
        if !set.contains(id) {
            set.push(id.clone());
        }
    }
    let n = set.len();
    if n > 2000 {
        set.drain(0..n - 2000);
    }
    let _ = store.kv_set("hidden_msgs", &set);
}

pub fn hidden_messages(store: &Store) -> Vec<String> {
    store.kv_get("hidden_msgs").ok().flatten().unwrap_or_default()
}

/// Applies `events` strictly in cursor order, skipping duplicates and replays, and stores the new cursor.
pub fn apply_events(store: &Store, events: &[Event]) -> Result<Applied> {
    let mut out = Applied::default();
    let mut cursor = store.cursor().unwrap_or(0);
    let me: String = store.kv_get("user_id")?.unwrap_or_default();
    let mut touched: BTreeSet<String> = BTreeSet::new();
    let mut touched_users: BTreeSet<String> = BTreeSet::new();
    let mut refresh: BTreeSet<String> = BTreeSet::new();
    let mut all_rows = false;

    for ev in events {
        if !should_apply(cursor, ev.cursor) {
            continue;
        }
        cursor = ev.cursor;
        out.cursor = Some(cursor);
        if matches!(
            ev.event,
            EventData::MessageNew { .. }
                | EventData::MessageDelete { .. }
                | EventData::ThreadUpdate { .. }
                | EventData::ConversationUpsert { .. }
                | EventData::ConversationRemove { .. }
                | EventData::AttachmentRemoved { .. }
                | EventData::ReadUpdate { .. }
                | EventData::SavedUpdate { .. }
                | EventData::SavedRemove { .. }
        ) {
            out.counts_dirty = true;
        }
        match &ev.event {
            EventData::MessageNew { message } => {
                let replaces = message.client_id.as_ref().filter(|_| message.sender_id == me).map(|cid| {
                    let _ = store.outbox_delete(cid);
                    format!("local:{cid}")
                });
                store.upsert_message(message)?;
                match store.get_conv(&message.conv_id)? {
                    Some(mut cs) => {
                        cs.conversation.last_seq = cs.conversation.last_seq.max(message.seq);
                        cs.conversation.last_message_ts = Some(cs.conversation.last_message_ts.unwrap_or(0).max(message.ts));
                        if message.sender_id != me && message.thread_root.is_none() && message.kind != MessageKind::System {
                            cs.unread += 1;
                            if message.mentions.contains(&me) {
                                cs.mentions += 1;
                            }
                            refresh.insert(message.conv_id.clone());
                        }
                        store.upsert_conv(&cs)?;
                    }
                    None => {
                        refresh.insert(message.conv_id.clone());
                    }
                }
                touched.insert(message.conv_id.clone());
                if message.sender_id != me && message.kind != MessageKind::System && !message.deleted {
                    out.new_messages.push(message.clone());
                }
                out.events.push(CoreEvent::Message { message: MessageView::server(message.clone()), replaces });
            }
            EventData::MessageEdit { message } | EventData::MessageDelete { message } => {
                store.upsert_message(message)?;
                touched.insert(message.conv_id.clone());
                out.events.push(CoreEvent::Message { message: MessageView::server(message.clone()), replaces: None });
            }
            EventData::MessageHide { conv_id, message_ids } => {
                store.delete_messages(message_ids)?;
                remember_hidden(store, message_ids);
                touched.insert(conv_id.clone());
                for id in message_ids {
                    out.events.push(CoreEvent::MessageRemove { conv_id: conv_id.clone(), id: id.clone() });
                }
            }
            EventData::ReactionUpdate { conv_id, message_id, reactions } => {
                if let Some(mut m) = store.get_message(message_id)? {
                    m.reactions = reactions.clone();
                    store.upsert_message(&m)?;
                    touched.insert(conv_id.clone());
                    out.events.push(CoreEvent::Message { message: MessageView::server(m), replaces: None });
                }
            }
            EventData::MessageDelivered { conv_id, message_id } => {
                if let Some(mut m) = store.get_message(message_id)? {
                    if !m.delivered {
                        m.delivered = true;
                        store.upsert_message(&m)?;
                        touched.insert(conv_id.clone());
                        out.events.push(CoreEvent::Message { message: MessageView::server(m), replaces: None });
                    }
                }
            }
            EventData::ConversationUpsert { conversation } => {
                store.upsert_conv(conversation)?;
                touched.insert(conversation.conversation.id.clone());
            }
            EventData::ConversationRemove { conv_id } => {
                store.remove_conv(conv_id)?;
                touched.remove(conv_id);
                out.events.push(CoreEvent::ChatRemove { id: conv_id.clone() });
            }
            EventData::ReadUpdate { conv_id, last_read_seq, marked_unread, unread, mentions } => {
                if let Some(mut cs) = store.get_conv(conv_id)? {
                    cs.last_read_seq = *last_read_seq;
                    cs.marked_unread = *marked_unread;
                    cs.unread = *unread;
                    cs.mentions = *mentions;
                    store.upsert_conv(&cs)?;
                    touched.insert(conv_id.clone());
                }
            }
            EventData::UserUpsert { user } => {
                store.upsert_user(user)?;
                touched_users.insert(user.id.clone());
                out.events.push(CoreEvent::User { user: user.clone() });
            }
            EventData::MeUpdate { me: m } => {
                store.kv_set("me", m)?;
                store.upsert_user(&m.user)?;
                out.events.push(CoreEvent::Me { me: m.clone() });
            }
            EventData::ThreadUpdate { thread } => {
                if thread.following {
                    store.upsert_thread(thread)?;
                    out.events.push(CoreEvent::Thread { thread: thread.clone() });
                } else {
                    store.remove_thread(&thread.root.id)?;
                    out.events.push(CoreEvent::ThreadRemove { root_id: thread.root.id.clone() });
                }
            }
            EventData::PrefUpdate { pref } => {
                store.upsert_pref(pref)?;
                touched.insert(pref.conv_id.clone());
            }
            EventData::AvatarOverrideUpdate { kind, target, attachment } => {
                let mut list: Vec<AvatarOverride> = store.kv_get("avatar_overrides")?.unwrap_or_default();
                list.retain(|o| !(o.kind == *kind && o.target == *target));
                if let Some(a) = attachment {
                    list.push(AvatarOverride { kind: kind.clone(), target: target.clone(), attachment: a.clone() });
                }
                store.kv_set("avatar_overrides", &list)?;
                all_rows = true;
            }
            EventData::PinsUpdate { conv_id, pins } => {
                let mut map: std::collections::BTreeMap<String, Vec<Pin>> = store.kv_get("pins")?.unwrap_or_default();
                map.insert(conv_id.clone(), pins.clone());
                store.kv_set("pins", &map)?;
                let hidden = hidden_messages(store);
                out.events.push(CoreEvent::Pins { conv_id: conv_id.clone(), ids: pins.iter().map(|p| p.message_id.clone()).filter(|id| !hidden.contains(id)).collect() });
            }
            EventData::ReadReceipt { conv_id, user_id, last_read_seq } => {
                if let Some(mut cs) = store.get_conv(conv_id)? {
                    for m in cs.conversation.members.iter_mut().filter(|m| m.user_id == *user_id) {
                        m.last_read_seq = Some(m.last_read_seq.unwrap_or(0).max(*last_read_seq));
                    }
                    store.upsert_conv(&cs)?;
                    touched.insert(conv_id.clone());
                }
            }
            EventData::SavedUpdate { item } => {
                let mut list: Vec<SavedItem> = store.kv_get("saved")?.unwrap_or_default();
                list.retain(|s| s.message_id != item.message_id);
                list.push(item.clone());
                store.kv_set("saved", &list)?;
                out.events.push(saved_event(&list));
            }
            EventData::SavedRemove { message_id } => {
                let mut list: Vec<SavedItem> = store.kv_get("saved")?.unwrap_or_default();
                list.retain(|s| s.message_id != *message_id);
                store.kv_set("saved", &list)?;
                out.events.push(saved_event(&list));
            }
            EventData::LabelsUpdate { labels } => {
                store.kv_set("labels", labels)?;
                out.events.push(CoreEvent::Labels { labels: labels.clone() });
            }
            EventData::SectionsUpdate { sections } => store.kv_set("sections", sections)?,
            EventData::GroupsUpdate { groups } => store.kv_set("groups", groups)?,
            EventData::OrgSettingsUpdate { settings } => {
                store.kv_set("org_settings", settings)?;
                out.events.push(CoreEvent::OrgSettings { settings: settings.clone() });
                // Who may pin here depends on it.
                all_rows = true;
            }
            EventData::Resync {} => out.resync = true,
            // Not shown in C1 (bookmarks, drafts, scheduled, activity, removed files).
            _ => {}
        }
    }
    if let Some(c) = out.cursor {
        store.set_cursor(c)?;
    }
    if out.resync {
        return Ok(out);
    }

    // Rows are built once at the end from the final state.
    let ctx = RowCtx::load(store)?;
    if !touched_users.is_empty() {
        for cs in store.all_convs()? {
            if cs.conversation.members.iter().any(|m| touched_users.contains(&m.user_id)) {
                touched.insert(cs.conversation.id.clone());
            }
        }
    }
    if all_rows {
        for cs in store.all_convs()? {
            touched.insert(cs.conversation.id.clone());
        }
    }
    for id in &touched {
        if let Some(cs) = store.get_conv(id)? {
            out.events.push(CoreEvent::Chat { row: build_row(store, &ctx, &cs)? });
        }
    }
    out.refresh = refresh.into_iter().collect();
    Ok(out)
}

/// A message of mine that the server confirmed over HTTP (the WebSocket echo follows later and is idempotent).
pub fn apply_sent(store: &Store, m: &Message) -> Result<Vec<CoreEvent>> {
    if let Some(cid) = &m.client_id {
        store.outbox_delete(cid)?;
    }
    store.upsert_message(m)?;
    let mut out = vec![CoreEvent::Message { message: MessageView::server(m.clone()), replaces: m.client_id.as_ref().map(|c| format!("local:{c}")) }];
    if let Some(mut cs) = store.get_conv(&m.conv_id)? {
        cs.conversation.last_seq = cs.conversation.last_seq.max(m.seq);
        cs.conversation.last_message_ts = Some(cs.conversation.last_message_ts.unwrap_or(0).max(m.ts));
        store.upsert_conv(&cs)?;
        let ctx = RowCtx::load(store)?;
        out.push(CoreEvent::Chat { row: build_row(store, &ctx, &cs)? });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::OutboxItem;
    use std::collections::BTreeMap;

    fn user(id: &str, name: &str) -> User {
        User { id: id.into(), username: name.to_lowercase(), display_name: name.into(), role: Role::Member, avatar: None, disabled: false, is_bot: false, last_active_ms: None, presence: None, title: None, phone: None }
    }

    fn me_struct() -> Me {
        Me { user: user("u1", "Me"), privacy_controls_enabled: false, hide_last_active: false, presence_pref: "auto".into(), password_set: true, email: None, read_receipts: true }
    }

    fn conv(id: &str, kind: ConversationKind, other: Option<&str>) -> ConversationSummary {
        let mut members = vec![Member { user_id: "u1".into(), role: "member".into(), last_read_seq: None }];
        if let Some(o) = other {
            members.push(Member { user_id: o.into(), role: "member".into(), last_read_seq: None });
        }
        ConversationSummary {
            conversation: Conversation { id: id.into(), kind, name: if kind == ConversationKind::Channel { Some("general".into()) } else { None }, topic: None, archived: false, created_ms: 1, members, last_seq: 0, last_message_ts: None, avatar: None },
            last_read_seq: 0,
            marked_unread: false,
            unread: 0,
            mentions: 0,
        }
    }

    fn message(id: &str, conv: &str, seq: i64, sender: &str, body: &str) -> Message {
        Message {
            id: id.into(), conv_id: conv.into(), seq, sender_id: sender.into(), client_id: None, body: body.into(), kind: MessageKind::Text, ts: 1000 + seq, version: 1,
            edited_ms: None, deleted: false, thread_root: None, also_send_to_channel: false, reply_to: None, mentions: vec![], attachments: vec![], reactions: vec![], delivered: false, forwarded: None, form: None, responded: vec![],
        }
    }

    fn boot() -> Bootstrap {
        let mut messages = BTreeMap::new();
        messages.insert("c1".to_string(), vec![message("m1", "c1", 1, "u2", "hello")]);
        Bootstrap {
            cursor: 10, server_time: 0, me: me_struct(), users: vec![user("u1", "Me"), user("u2", "Sarah Chen")],
            conversations: vec![conv("c1", ConversationKind::Dm, Some("u2")), conv("c2", ConversationKind::Channel, None)],
            messages, has_more: BTreeMap::new(), prefs: vec![], avatar_overrides: vec![], sections: vec![], groups: vec![], drafts: vec![], threads: vec![],
            pins: BTreeMap::new(), bookmarks: BTreeMap::new(), saved: vec![], labels: vec![], activity_read_ms: 0, org_settings: Default::default(),
        }
    }

    fn ev(cursor: i64, e: EventData) -> Event {
        Event { cursor, ts: cursor, event: e }
    }

    fn store() -> Store {
        let s = Store::open_memory().unwrap();
        s.replace_bootstrap(&boot()).unwrap();
        s
    }

    #[test]
    fn bootstrap_sets_cursor_and_me() {
        let s = store();
        assert_eq!(s.cursor(), Some(10));
        assert_eq!(s.kv_get::<String>("user_id").unwrap().as_deref(), Some("u1"));
        assert_eq!(s.all_convs().unwrap().len(), 2);
        assert_eq!(s.messages_page("c1", None, 10).unwrap().len(), 1);
    }

    #[test]
    fn replays_and_old_events_are_skipped() {
        let s = store();
        let m = message("m2", "c1", 2, "u2", "second");
        let a = apply_events(&s, &[ev(9, EventData::MessageNew { message: m.clone() }), ev(10, EventData::MessageNew { message: m.clone() })]).unwrap();
        assert!(a.events.is_empty());
        assert_eq!(s.messages_page("c1", None, 10).unwrap().len(), 1);
        let a = apply_events(&s, &[ev(11, EventData::MessageNew { message: m.clone() }), ev(11, EventData::MessageNew { message: m })]).unwrap();
        assert_eq!(a.cursor, Some(11));
        assert_eq!(s.messages_page("c1", None, 10).unwrap().len(), 2);
        assert_eq!(s.cursor(), Some(11));
    }

    #[test]
    fn incoming_message_bumps_unread_and_asks_for_refresh() {
        let s = store();
        let a = apply_events(&s, &[ev(11, EventData::MessageNew { message: message("m2", "c1", 2, "u2", "second") })]).unwrap();
        let cs = s.get_conv("c1").unwrap().unwrap();
        assert_eq!((cs.unread, cs.conversation.last_seq), (1, 2));
        assert_eq!(a.refresh, vec!["c1".to_string()]);
        let row = a.events.iter().find_map(|e| if let CoreEvent::Chat { row } = e { Some(row.clone()) } else { None }).unwrap();
        assert_eq!((row.title.as_str(), row.unread, row.preview.text.as_str()), ("Sarah Chen", 1, "second"));
        // my own message does not count as unread
        apply_events(&s, &[ev(12, EventData::MessageNew { message: message("m3", "c1", 3, "u1", "mine") })]).unwrap();
        assert_eq!(s.get_conv("c1").unwrap().unwrap().unread, 1);
        // read update from the server is authoritative
        apply_events(&s, &[ev(13, EventData::ReadUpdate { conv_id: "c1".into(), last_read_seq: 3, marked_unread: false, unread: 0, mentions: 0 })]).unwrap();
        assert_eq!(s.get_conv("c1").unwrap().unwrap().unread, 0);
    }

    #[test]
    fn echo_of_my_send_replaces_the_outbox_entry() {
        let s = store();
        s.outbox_add(&OutboxItem { client_id: "abc".into(), conv_id: "c1".into(), thread_root: None, also_send_to_channel: false, reply_to: None, body: "hi".into(), mentions: vec![], ts: 5, state: "queued".into(), attempts: 0, error: None, attachments: vec![] }).unwrap();
        let mut m = message("m9", "c1", 2, "u1", "hi");
        m.client_id = Some("abc".into());
        let a = apply_events(&s, &[ev(11, EventData::MessageNew { message: m })]).unwrap();
        assert!(s.outbox_get("abc").unwrap().is_none());
        let replaces = a.events.iter().find_map(|e| if let CoreEvent::Message { replaces, .. } = e { replaces.clone() } else { None });
        assert_eq!(replaces.as_deref(), Some("local:abc"));
    }

    #[test]
    fn edit_delete_reaction_hide() {
        let s = store();
        let mut edited = message("m1", "c1", 1, "u2", "hello edited");
        edited.version = 2;
        edited.edited_ms = Some(5);
        apply_events(&s, &[ev(11, EventData::MessageEdit { message: edited })]).unwrap();
        assert_eq!(s.get_message("m1").unwrap().unwrap().body, "hello edited");
        apply_events(&s, &[ev(12, EventData::ReactionUpdate { conv_id: "c1".into(), message_id: "m1".into(), reactions: vec![ReactionSummary { emoji: "👍".into(), user_ids: vec!["u1".into()] }] })]).unwrap();
        assert_eq!(s.get_message("m1").unwrap().unwrap().reactions.len(), 1);
        let mut gone = message("m1", "c1", 1, "u2", "");
        gone.deleted = true;
        apply_events(&s, &[ev(13, EventData::MessageDelete { message: gone })]).unwrap();
        assert!(s.get_message("m1").unwrap().unwrap().deleted);
        let a = apply_events(&s, &[ev(14, EventData::MessageHide { conv_id: "c1".into(), message_ids: vec!["m1".into()] })]).unwrap();
        assert!(s.get_message("m1").unwrap().is_none());
        assert!(a.events.iter().any(|e| matches!(e, CoreEvent::MessageRemove { .. })));
    }

    #[test]
    fn org_settings_event_updates_the_cache_and_rows() {
        let s = store();
        let a = apply_events(&s, &[ev(11, EventData::OrgSettingsUpdate { settings: OrgSettings { channel_creation: ChannelCreation::Admins, channel_pins: ChannelPins::Managers } })]).unwrap();
        assert!(a.events.iter().any(|e| matches!(e, CoreEvent::OrgSettings { settings } if settings.channel_pins == ChannelPins::Managers)));
        // Every row is rebuilt: a plain member can no longer pin in the channel.
        let row = a.events.iter().find_map(|e| match e { CoreEvent::Chat { row } if row.id == "c2" => Some(row.clone()), _ => None }).unwrap();
        assert!(!row.can_pin);
        let dm = a.events.iter().find_map(|e| match e { CoreEvent::Chat { row } if row.id == "c1" => Some(row.clone()), _ => None }).unwrap();
        assert!(dm.can_pin, "direct messages are not limited");
        assert_eq!(s.kv_get::<OrgSettings>("org_settings").unwrap().unwrap().channel_creation, ChannelCreation::Admins);
    }

    #[test]
    fn a_broadcast_thread_reply_is_in_the_main_timeline() {
        let s = store();
        let mut plain = message("t1", "c2", 5, "u2", "only in the thread");
        plain.thread_root = Some("m1".into());
        let mut both = message("t2", "c2", 6, "u2", "also in the channel");
        both.thread_root = Some("m1".into());
        both.also_send_to_channel = true;
        apply_events(&s, &[ev(11, EventData::MessageNew { message: plain }), ev(12, EventData::MessageNew { message: both })]).unwrap();
        let ids: Vec<String> = s.messages_page("c2", None, 50).unwrap().into_iter().map(|m| m.id).collect();
        assert!(ids.contains(&"t2".to_string()) && !ids.contains(&"t1".to_string()), "{ids:?}");
        assert_eq!(s.last_message("c2").unwrap().unwrap().id, "t2");
        assert_eq!(s.thread_messages("m1").unwrap().len(), 2);
    }

    #[test]
    fn hidden_messages_are_remembered_for_pins() {
        let s = store();
        apply_events(&s, &[ev(11, EventData::MessageHide { conv_id: "c1".into(), message_ids: vec!["m9".into()] })]).unwrap();
        assert_eq!(hidden_messages(&s), vec!["m9".to_string()]);
        let a = apply_events(&s, &[ev(12, EventData::PinsUpdate { conv_id: "c1".into(), pins: vec![Pin { conv_id: "c1".into(), message_id: "m9".into(), pinned_by: "u1".into(), ts: 1 }, Pin { conv_id: "c1".into(), message_id: "m8".into(), pinned_by: "u1".into(), ts: 2 }] })]).unwrap();
        assert!(a.events.iter().any(|e| matches!(e, CoreEvent::Pins { ids, .. } if ids == &vec!["m8".to_string()])));
    }

    #[test]
    fn a_resync_keeps_what_belongs_to_this_device() {
        let s = store();
        s.kv_set("lock_pin", &"hash").unwrap();
        s.kv_set("notify", &"settings").unwrap();
        s.replace_bootstrap(&boot()).unwrap();
        assert_eq!(s.kv_get::<String>("lock_pin").unwrap().as_deref(), Some("hash"));
        assert_eq!(s.kv_get::<String>("notify").unwrap().as_deref(), Some("settings"));
        s.wipe().unwrap();
        assert!(s.kv_get::<String>("lock_pin").unwrap().is_none(), "signing out forgets the PIN");
    }

    #[test]
    fn conversation_remove_and_resync() {
        let s = store();
        let a = apply_events(&s, &[ev(11, EventData::ConversationRemove { conv_id: "c2".into() })]).unwrap();
        assert!(s.get_conv("c2").unwrap().is_none());
        assert!(a.events.iter().any(|e| matches!(e, CoreEvent::ChatRemove { id } if id == "c2")));
        let a = apply_events(&s, &[ev(12, EventData::Resync {})]).unwrap();
        assert!(a.resync);
    }

    #[test]
    fn threads_follow_and_unfollow() {
        let s = store();
        let root = message("r1", "c1", 1, "u2", "root");
        let t = ThreadSummary { conv_id: "c1".into(), root: root.clone(), reply_count: 3, last_reply_ts: 50, unread: 1, following: true, participants: vec![], last_reply: None };
        apply_events(&s, &[ev(11, EventData::ThreadUpdate { thread: t.clone() })]).unwrap();
        assert_eq!(s.all_threads().unwrap().len(), 1);
        let mut u = t;
        u.following = false;
        apply_events(&s, &[ev(12, EventData::ThreadUpdate { thread: u })]).unwrap();
        assert!(s.all_threads().unwrap().is_empty());
    }

    #[test]
    fn wipe_clears_everything_including_the_outbox() {
        let s = store();
        s.outbox_add(&OutboxItem { client_id: "x".into(), conv_id: "c1".into(), thread_root: None, also_send_to_channel: false, reply_to: None, body: "b".into(), mentions: vec![], ts: 1, state: "queued".into(), attempts: 0, error: None, attachments: vec![] }).unwrap();
        s.wipe().unwrap();
        assert!(s.all_convs().unwrap().is_empty());
        assert!(s.outbox_queued().unwrap().is_empty());
        assert_eq!(s.cursor(), None);
    }

    #[test]
    fn rebootstrap_keeps_outbox() {
        let s = store();
        s.outbox_add(&OutboxItem { client_id: "x".into(), conv_id: "c1".into(), thread_root: None, also_send_to_channel: false, reply_to: None, body: "b".into(), mentions: vec![], ts: 1, state: "queued".into(), attempts: 0, error: None, attachments: vec![] }).unwrap();
        s.replace_bootstrap(&boot()).unwrap();
        assert_eq!(s.outbox_queued().unwrap().len(), 1);
    }

    #[test]
    fn labels_and_saved_label_ids_are_kept() {
        let s = store();
        let l = Label { id: "l1".into(), name: "Urgent".into(), color: "#E5484D".into(), position: 0 };
        let item = SavedItem { message_id: "m1".into(), conv_id: "c1".into(), saved_ms: 5, label_ids: vec!["l1".into()] };
        let out = apply_events(&s, &[ev(11, EventData::LabelsUpdate { labels: vec![l.clone()] }), ev(12, EventData::SavedUpdate { item })]).unwrap();
        assert_eq!(s.kv_get::<Vec<Label>>("labels").unwrap(), Some(vec![l]));
        let saved: Vec<SavedItem> = s.kv_get("saved").unwrap().unwrap();
        assert_eq!(saved[0].label_ids, vec!["l1".to_string()]);
        assert!(out.events.iter().any(|e| matches!(e, CoreEvent::Labels { labels } if labels.len() == 1)));
        assert!(out.events.iter().any(|e| matches!(e, CoreEvent::Saved { labels, .. } if labels.get("m1").map(|v| v.len()) == Some(1))));
    }
}
