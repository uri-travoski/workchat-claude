//! Notification rules. `decide` is a pure function (tested below); the engine calls it for every new message from
//! another person and hands the result to the platform notifier (desktop OS notification, Android notification).

use crate::engine::Engine;
use crate::error::Result;
use crate::view::{make_preview, RowCtx};
use proto::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NotifySettings {
    /// Master switch for this device.
    pub enabled: bool,
    pub dms: bool,
    pub channels: bool,
    pub threads: bool,
    /// In channels only notify when you are mentioned (direct messages still notify).
    pub mentions_only: bool,
    /// Mentions notify even when the type of chat is switched off or the chat is "mentions only" is not enough: they always get through (but not mute or Do Not Disturb).
    pub mention_alert: bool,
    /// `default`, `gentle` or `none`.
    pub sound: String,
    /// Play the sound for channel messages too (off: channels are silent, direct messages keep the sound).
    pub channel_sound: bool,
    pub vibration: bool,
    /// Show the sender and text; off shows "New message".
    pub show_preview: bool,
    pub dnd: bool,
    /// Do Not Disturb window, local time `HH:MM`; it may wrap midnight.
    pub dnd_from: String,
    pub dnd_to: String,
    /// Desktop: closing the window keeps WorkChat in the tray.
    pub close_to_tray: bool,
    /// Android: foreground service that keeps the connection open (for phones without a push distributor).
    pub keep_alive: bool,
}

impl Default for NotifySettings {
    fn default() -> Self {
        NotifySettings {
            enabled: true,
            dms: true,
            channels: true,
            threads: true,
            mentions_only: false,
            mention_alert: true,
            sound: "default".into(),
            channel_sound: true,
            vibration: true,
            show_preview: true,
            dnd: false,
            dnd_from: "22:00".into(),
            dnd_to: "07:00".into(),
            close_to_tray: false,
            keep_alive: false,
        }
    }
}

/// What to show. `locked` chats never reveal sender or text.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Notice {
    pub conv_id: String,
    pub title: String,
    pub body: String,
    pub sound: String,
    pub vibrate: bool,
    pub locked: bool,
    pub message_id: String,
}

pub struct Ctx<'a> {
    pub me: &'a str,
    pub me_dnd: bool,
    pub now_ms: i64,
    /// Minutes since local midnight.
    pub local_minute: u32,
    /// The window is focused and showing this conversation.
    pub viewing: bool,
    pub locked: bool,
}

pub fn parse_hm(s: &str) -> Option<u32> {
    let (h, m) = s.split_once(':')?;
    let (h, m): (u32, u32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// True when `minute` falls inside `[from, to)`, wrapping over midnight; `from == to` means all day.
pub fn in_window(minute: u32, from: u32, to: u32) -> bool {
    if from == to {
        true
    } else if from < to {
        minute >= from && minute < to
    } else {
        minute >= from || minute < to
    }
}

pub fn dnd_active(st: &NotifySettings, minute: u32) -> bool {
    st.dnd && matches!((parse_hm(&st.dnd_from), parse_hm(&st.dnd_to)), (Some(f), Some(t)) if in_window(minute, f, t))
}

/// Decides whether `m` (a new message from someone else) should notify, and with what text.
/// `title`/`body` are the plain-text pieces to show when the chat is not locked.
pub fn decide(m: &Message, kind: ConversationKind, pref: Option<&ConvPref>, st: &NotifySettings, ctx: &Ctx, title: &str, body: &str) -> Option<Notice> {
    if !st.enabled || m.sender_id == ctx.me || m.kind == MessageKind::System || m.deleted || kind == ConversationKind::SelfNote {
        return None;
    }
    if ctx.viewing {
        return None;
    }
    if ctx.me_dnd || dnd_active(st, ctx.local_minute) {
        return None;
    }
    let mentioned = m.mentions.iter().any(|u| u == ctx.me);
    let level = pref.map(|p| p.notify.as_str()).unwrap_or("all");
    if pref.map(|p| p.is_muted(ctx.now_ms)).unwrap_or(false) || level == "nothing" {
        return None;
    }
    let is_dm = matches!(kind, ConversationKind::Dm | ConversationKind::GroupDm);
    let in_thread = m.thread_root.is_some() && !m.also_send_to_channel;
    // Mentions always get through the per-type switches when the mention alert is on.
    let breaks_through = mentioned && st.mention_alert;
    if !breaks_through {
        if level == "mentions" && !mentioned {
            return None;
        }
        if in_thread && !st.threads {
            return None;
        }
        if is_dm && !st.dms {
            return None;
        }
        if !is_dm {
            if !st.channels {
                return None;
            }
            if st.mentions_only && !mentioned {
                return None;
            }
        }
    }
    let silent = st.sound == "none" || (!is_dm && !st.channel_sound && !mentioned);
    let (title, body) = if ctx.locked || !st.show_preview {
        ("WorkChat".to_string(), "New message".to_string())
    } else {
        (title.to_string(), body.to_string())
    };
    Some(Notice { conv_id: m.conv_id.clone(), title, body, sound: if silent { "none".into() } else { st.sound.clone() }, vibrate: st.vibration && !silent, locked: ctx.locked, message_id: m.id.clone() })
}

impl Engine {
    pub fn notify_settings(&self) -> NotifySettings {
        self.store.kv_get::<NotifySettings>("notify").ok().flatten().unwrap_or_default()
    }

    pub fn set_notify_settings(&self, s: &NotifySettings) -> Result<()> {
        self.store.kv_set("notify", s)
    }

    /// The window state reported by the UI: focused or not, the open chat, and the time zone (minutes east of UTC).
    pub fn set_ui_state(&self, focused: bool, conv: Option<String>, tz_offset_min: Option<i32>) {
        *self.ui.lock().unwrap_or_else(|e| e.into_inner()) = (focused, conv);
        if let Some(z) = tz_offset_min {
            self.tz_offset_min.store(z, std::sync::atomic::Ordering::Relaxed);
            let _ = self.store.kv_set("tz_offset", &z);
        }
    }

    pub fn local_minute(&self) -> u32 {
        let off = self.tz_offset_min.load(std::sync::atomic::Ordering::Relaxed) as i64;
        (((crate::api::now_ms() / 60_000 + off) % 1440 + 1440) % 1440) as u32
    }

    /// Called for each new message from another person after it was stored.
    pub fn notify_new(&self, m: &Message) {
        let Some(hook) = self.notifier.read().unwrap_or_else(|e| e.into_inner()).clone() else { return };
        let Ok(Some(cs)) = self.store.get_conv(&m.conv_id) else { return };
        let Ok(ctx) = RowCtx::load(&self.store) else { return };
        let group = !matches!(cs.conversation.kind, ConversationKind::Dm | ConversationKind::SelfNote);
        let pv = make_preview(m, &ctx, group);
        let (title, body) = match cs.conversation.kind {
            ConversationKind::Dm => (ctx.users.get(&m.sender_id).map(|u| u.display_name.clone()).unwrap_or_else(|| "New message".into()), pv.text.clone()),
            _ => {
                let name = cs.conversation.name.clone().unwrap_or_default();
                let who = ctx.users.get(&m.sender_id).map(|u| u.display_name.clone()).unwrap_or_else(|| "Someone".into());
                let t = if cs.conversation.kind == ConversationKind::Channel { format!("#{name}") } else if name.is_empty() { who.clone() } else { name };
                (t, format!("{}: {}", who.split_whitespace().next().unwrap_or(&who), pv.text))
            }
        };
        let (focused, open) = self.ui.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let me_dnd = self.store.kv_get::<Me>("me").ok().flatten().map(|me| me.presence_pref == "dnd").unwrap_or(false);
        let pref = ctx.prefs.get(&m.conv_id);
        let c = Ctx { me: &ctx.me, me_dnd, now_ms: crate::api::now_ms(), local_minute: self.local_minute(), viewing: focused && open.as_deref() == Some(&m.conv_id), locked: self.is_locked(&m.conv_id) };
        if let Some(n) = decide(m, cs.conversation.kind, pref, &self.notify_settings(), &c, &title, &body) {
            hook(n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(sender: &str, mentions: &[&str]) -> Message {
        Message {
            id: "m1".into(), conv_id: "c1".into(), seq: 1, sender_id: sender.into(), client_id: None, body: "hello".into(), kind: MessageKind::Text, ts: 1,
            version: 1, edited_ms: None, deleted: false, thread_root: None, also_send_to_channel: false, reply_to: None, mentions: mentions.iter().map(|s| s.to_string()).collect(), attachments: vec![], reactions: vec![],
            delivered: false, forwarded: None, form: None, responded: vec![],
        }
    }
    fn ctx(minute: u32) -> Ctx<'static> {
        Ctx { me: "me", me_dnd: false, now_ms: 1_000, local_minute: minute, viewing: false, locked: false }
    }
    fn run(m: &Message, kind: ConversationKind, pref: Option<&ConvPref>, st: &NotifySettings, c: &Ctx) -> Option<Notice> {
        decide(m, kind, pref, st, c, "Sarah", "hello")
    }

    #[test]
    fn basics() {
        let st = NotifySettings::default();
        let n = run(&msg("u2", &[]), ConversationKind::Dm, None, &st, &ctx(600)).unwrap();
        assert_eq!((n.title.as_str(), n.body.as_str(), n.sound.as_str(), n.vibrate), ("Sarah", "hello", "default", true));
        assert!(run(&msg("me", &[]), ConversationKind::Dm, None, &st, &ctx(600)).is_none(), "own messages");
        let mut sys = msg("u2", &[]);
        sys.kind = MessageKind::System;
        assert!(run(&sys, ConversationKind::Channel, None, &st, &ctx(600)).is_none());
        assert!(run(&msg("u2", &[]), ConversationKind::SelfNote, None, &st, &ctx(600)).is_none());
        let mut c = ctx(600);
        c.viewing = true;
        assert!(run(&msg("u2", &[]), ConversationKind::Dm, None, &st, &c).is_none(), "the chat is open and focused");
        let off = NotifySettings { enabled: false, ..Default::default() };
        assert!(run(&msg("u2", &[]), ConversationKind::Dm, None, &off, &ctx(600)).is_none());
    }

    #[test]
    fn mute_and_levels() {
        let st = NotifySettings::default();
        let mut p = ConvPref::default_for("c1");
        p.mute_until_ms = Some(-1);
        assert!(run(&msg("u2", &["me"]), ConversationKind::Channel, Some(&p), &st, &ctx(600)).is_none(), "muted, even for a mention");
        p.mute_until_ms = Some(500);
        assert!(run(&msg("u2", &[]), ConversationKind::Channel, Some(&p), &st, &ctx(600)).is_some(), "mute expired");
        p.mute_until_ms = None;
        p.notify = "mentions".into();
        assert!(run(&msg("u2", &[]), ConversationKind::Channel, Some(&p), &st, &ctx(600)).is_none());
        assert!(run(&msg("u2", &["me"]), ConversationKind::Channel, Some(&p), &st, &ctx(600)).is_some());
        p.notify = "nothing".into();
        assert!(run(&msg("u2", &["me"]), ConversationKind::Channel, Some(&p), &st, &ctx(600)).is_none());
    }

    #[test]
    fn type_switches_and_mentions() {
        let st = NotifySettings { channels: false, mentions_only: false, ..Default::default() };
        assert!(run(&msg("u2", &[]), ConversationKind::Channel, None, &st, &ctx(600)).is_none());
        assert!(run(&msg("u2", &["me"]), ConversationKind::Channel, None, &st, &ctx(600)).is_some(), "mention alert breaks through");
        let no_alert = NotifySettings { channels: false, mention_alert: false, ..Default::default() };
        assert!(run(&msg("u2", &["me"]), ConversationKind::Channel, None, &no_alert, &ctx(600)).is_none());
        let mo = NotifySettings { mentions_only: true, ..Default::default() };
        assert!(run(&msg("u2", &[]), ConversationKind::Channel, None, &mo, &ctx(600)).is_none());
        assert!(run(&msg("u2", &[]), ConversationKind::Dm, None, &mo, &ctx(600)).is_some(), "DMs are not affected by mentions only");
        let mut th = msg("u2", &[]);
        th.thread_root = Some("r".into());
        let no_threads = NotifySettings { threads: false, ..Default::default() };
        assert!(run(&th, ConversationKind::Dm, None, &no_threads, &ctx(600)).is_none());
    }

    #[test]
    fn dnd_window() {
        assert_eq!(parse_hm("07:30"), Some(450));
        assert_eq!(parse_hm("24:00"), None);
        assert_eq!(parse_hm("7"), None);
        assert!(in_window(23 * 60, 22 * 60, 7 * 60));
        assert!(in_window(3 * 60, 22 * 60, 7 * 60));
        assert!(!in_window(12 * 60, 22 * 60, 7 * 60));
        assert!(in_window(100, 600, 600), "equal bounds mean all day");
        let st = NotifySettings { dnd: true, ..Default::default() };
        assert!(run(&msg("u2", &["me"]), ConversationKind::Dm, None, &st, &ctx(23 * 60)).is_none(), "DND blocks even mentions");
        assert!(run(&msg("u2", &[]), ConversationKind::Dm, None, &st, &ctx(12 * 60)).is_some());
        let mut c = ctx(12 * 60);
        c.me_dnd = true;
        assert!(run(&msg("u2", &[]), ConversationKind::Dm, None, &NotifySettings::default(), &c).is_none(), "server-side do not disturb");
    }

    #[test]
    fn locked_and_preview_hidden() {
        let st = NotifySettings::default();
        let mut c = ctx(600);
        c.locked = true;
        let n = run(&msg("u2", &[]), ConversationKind::Dm, None, &st, &c).unwrap();
        assert_eq!((n.title.as_str(), n.body.as_str(), n.locked), ("WorkChat", "New message", true));
        assert!(!n.title.contains("Sarah") && !n.body.contains("hello"));
        let np = NotifySettings { show_preview: false, ..Default::default() };
        assert_eq!(run(&msg("u2", &[]), ConversationKind::Dm, None, &np, &ctx(600)).unwrap().body, "New message");
    }

    #[test]
    fn sounds() {
        let st = NotifySettings { channel_sound: false, ..Default::default() };
        let n = run(&msg("u2", &[]), ConversationKind::Channel, None, &st, &ctx(600)).unwrap();
        assert_eq!((n.sound.as_str(), n.vibrate), ("none", false));
        assert_eq!(run(&msg("u2", &["me"]), ConversationKind::Channel, None, &st, &ctx(600)).unwrap().sound, "default");
        let none = NotifySettings { sound: "none".into(), ..Default::default() };
        assert_eq!(run(&msg("u2", &[]), ConversationKind::Dm, None, &none, &ctx(600)).unwrap().sound, "none");
    }
}
