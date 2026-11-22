//! Chat lock: a 6-digit PIN, wrong-PIN throttling, the set of locked chats and the "locked area" (open or closed).
//!
//! The PIN never leaves the device: only a PBKDF2-HMAC-SHA256 hash with a random salt is stored (in the local
//! database, which is wiped on sign-out). Locked chats are hidden by the engine from snapshots, events, counts,
//! search, files, threads and notifications while the area is closed.

use crate::api::now_ms;
use crate::engine::Engine;
use crate::error::{CoreError, Result};
use crate::store::Store;
use crate::view::CoreEvent;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use proto::*;
use ring::pbkdf2;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::num::NonZeroU32;
use std::sync::atomic::Ordering;

pub const PIN_LEN: usize = 6;
pub const ITERATIONS: u32 = 210_000;
const KEY_PIN: &str = "lock_pin";
const KEY_THROTTLE: &str = "lock_throttle";
const KEY_CHATS: &str = "lock_chats";
const KEY_TIMEOUT: &str = "lock_timeout";
const KEY_FILE_ADJ: &str = "lock_file_adj";

pub fn pin_valid(pin: &str) -> bool {
    pin.len() == PIN_LEN && pin.bytes().all(|b| b.is_ascii_digit())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PinRecord {
    pub salt: String,
    pub hash: String,
    pub iterations: u32,
}

pub fn make_record(pin: &str, iterations: u32) -> PinRecord {
    let salt: [u8; 16] = rand::random();
    let mut out = [0u8; 32];
    pbkdf2::derive(pbkdf2::PBKDF2_HMAC_SHA256, NonZeroU32::new(iterations.max(1)).unwrap(), &salt, pin.as_bytes(), &mut out);
    PinRecord { salt: B64.encode(salt), hash: B64.encode(out), iterations }
}

/// Constant-time comparison (ring's `verify`).
pub fn check_record(rec: &PinRecord, pin: &str) -> bool {
    let (Ok(salt), Ok(hash)) = (B64.decode(&rec.salt), B64.decode(&rec.hash)) else { return false };
    pbkdf2::verify(pbkdf2::PBKDF2_HMAC_SHA256, NonZeroU32::new(rec.iterations.max(1)).unwrap(), &salt, pin.as_bytes(), &hash).is_ok()
}

/// Seconds to wait after the `fails`-th wrong PIN in a row: the first 4 are free, then 30 s, 1 min, 5 min, 15 min, 1 h.
pub fn penalty_secs(fails: u32) -> i64 {
    match fails {
        0..=4 => 0,
        5 => 30,
        6 => 60,
        7 => 300,
        8 => 900,
        _ => 3600,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Throttle {
    pub fails: u32,
    pub until_ms: i64,
}

impl Throttle {
    pub fn wait_secs(&self, now: i64) -> i64 {
        ((self.until_ms - now + 999) / 1000).max(0)
    }
    pub fn fail(&mut self, now: i64) {
        self.fails += 1;
        self.until_ms = now + penalty_secs(self.fails) * 1000;
    }
    pub fn attempts_left(&self) -> u32 {
        5u32.saturating_sub(self.fails)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LockStatus {
    pub pin_set: bool,
    /// The locked area is open (the PIN was entered and the area has not been locked again).
    pub open: bool,
    pub wait_secs: i64,
    pub attempts_left: u32,
    /// Seconds without use before the area locks again (0: only when the app goes to the background / never).
    pub timeout_secs: i64,
    pub on_background: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnlockResult {
    pub ok: bool,
    pub wait_secs: i64,
    pub attempts_left: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LockTimeout {
    pub secs: i64,
    pub on_background: bool,
}

impl Default for LockTimeout {
    fn default() -> Self {
        LockTimeout { secs: 300, on_background: true }
    }
}

// ---------------------------------------------------------------- store-level helpers (testable without an engine)

pub fn locked_set(store: &Store) -> HashSet<String> {
    store.kv_get::<Vec<String>>(KEY_CHATS).ok().flatten().unwrap_or_default().into_iter().collect()
}

pub fn set_locked(store: &Store, conv: &str, on: bool) -> Result<()> {
    let mut s = locked_set(store);
    if on {
        s.insert(conv.to_string());
    } else {
        s.remove(conv);
    }
    let mut v: Vec<String> = s.into_iter().collect();
    v.sort();
    store.kv_set(KEY_CHATS, &v)
}

pub fn pin_record(store: &Store) -> Option<PinRecord> {
    store.kv_get(KEY_PIN).ok().flatten()
}

pub fn throttle(store: &Store) -> Throttle {
    store.kv_get(KEY_THROTTLE).ok().flatten().unwrap_or_default()
}

/// Checks a PIN against the stored hash, applying and persisting the throttle. Never reveals more than ok / wait / attempts left.
pub fn try_pin(store: &Store, pin: &str, now: i64) -> Result<UnlockResult> {
    let rec = pin_record(store).ok_or_else(|| CoreError::local("No PIN is set yet."))?;
    let mut t = throttle(store);
    let wait = t.wait_secs(now);
    if wait > 0 {
        return Ok(UnlockResult { ok: false, wait_secs: wait, attempts_left: 0 });
    }
    if pin_valid(pin) && check_record(&rec, pin) {
        store.kv_set(KEY_THROTTLE, &Throttle::default())?;
        return Ok(UnlockResult { ok: true, wait_secs: 0, attempts_left: 5 });
    }
    t.fail(now);
    store.kv_set(KEY_THROTTLE, &t)?;
    Ok(UnlockResult { ok: false, wait_secs: t.wait_secs(now), attempts_left: t.attempts_left() })
}

pub fn timeout_settings(store: &Store) -> LockTimeout {
    store.kv_get(KEY_TIMEOUT).ok().flatten().unwrap_or_default()
}

/// The conversation a locked-chat filter applies to, for events.
fn event_conv(e: &CoreEvent) -> Option<&str> {
    match e {
        CoreEvent::Chat { row } => Some(&row.id),
        CoreEvent::Message { message, .. } => Some(&message.message.conv_id),
        CoreEvent::MessageRemove { conv_id, .. } => Some(conv_id),
        CoreEvent::Typing { conv_id, .. } => Some(conv_id),
        CoreEvent::Pins { conv_id, .. } => Some(conv_id),
        CoreEvent::Thread { thread } => Some(&thread.conv_id),
        _ => None,
    }
}

/// Per-chat file counts of the locked chats `[image, video, audio, document]`, subtracted from the pill counts while hidden.
type FileAdj = BTreeMap<String, [i64; 4]>;

pub fn adjust_counts_with(mut c: Counts, store: &Store, hidden: &HashSet<String>) -> Counts {
    if hidden.is_empty() {
        return c;
    }
    let adj: FileAdj = store.kv_get(KEY_FILE_ADJ).ok().flatten().unwrap_or_default();
    for cs in store.all_convs().unwrap_or_default().iter().filter(|cs| hidden.contains(&cs.conversation.id)) {
        let k = cs.conversation.kind;
        if cs.conversation.archived || k == ConversationKind::SelfNote {
            continue;
        }
        let unread = (cs.unread > 0 || cs.marked_unread) as i64;
        c.conversations -= 1;
        c.unread_conversations -= unread;
        if k == ConversationKind::Channel {
            c.channels -= 1;
            c.unread_channels -= unread;
        } else {
            c.dms -= 1;
            c.unread_dms -= unread;
        }
        c.unread_messages -= cs.unread;
        c.mentions -= cs.mentions;
    }
    for t in store.all_threads().unwrap_or_default().iter().filter(|t| hidden.contains(&t.conv_id)) {
        c.threads -= 1;
        if t.unread > 0 {
            c.threads_unread -= 1;
        }
    }
    for (conv, n) in adj.iter().filter(|(k, _)| hidden.contains(*k)) {
        c.files_image -= n[0];
        c.files_video -= n[1];
        c.files_audio -= n[2];
        c.files_document -= n[3];
        c.files -= n.iter().sum::<i64>();
        let _ = conv;
    }
    macro_rules! floor { ($($f:ident),*) => { $( c.$f = c.$f.max(0); )* } }
    floor!(conversations, dms, channels, unread_conversations, unread_dms, unread_channels, unread_messages, mentions, threads, threads_unread, files, files_image, files_video, files_audio, files_document);
    c
}

// ---------------------------------------------------------------- engine side

impl Engine {
    pub fn hidden_ids(&self) -> HashSet<String> {
        if self.lock_open.load(Ordering::SeqCst) {
            HashSet::new()
        } else {
            locked_set(&self.store)
        }
    }

    pub fn is_hidden(&self, conv: &str) -> bool {
        !self.lock_open.load(Ordering::SeqCst) && locked_set(&self.store).contains(conv)
    }

    pub fn is_locked(&self, conv: &str) -> bool {
        locked_set(&self.store).contains(conv)
    }

    /// Refuses to touch a hidden chat.
    pub fn guard(&self, conv: &str) -> Result<()> {
        if self.is_hidden(conv) {
            Err(CoreError::local("This chat is locked."))
        } else {
            Ok(())
        }
    }

    pub fn adjust_counts(&self, c: Counts) -> Counts {
        adjust_counts_with(c, &self.store, &self.hidden_ids())
    }

    /// Drops or rewrites events about hidden chats. Called for every event the UI would get.
    pub fn filter_event(&self, e: CoreEvent) -> Option<CoreEvent> {
        if let Some(conv) = event_conv(&e) {
            if self.is_hidden(conv) {
                return None;
            }
        }
        match e {
            CoreEvent::Counts { counts } => Some(CoreEvent::Counts { counts: self.adjust_counts(counts) }),
            CoreEvent::Saved { ids } => Some(CoreEvent::Saved { ids: self.visible_saved(ids) }),
            e => Some(e),
        }
    }

    pub fn visible_saved(&self, ids: Vec<String>) -> Vec<String> {
        let hidden = self.hidden_ids();
        if hidden.is_empty() {
            return ids;
        }
        ids.into_iter().filter(|id| self.store.get_message(id).ok().flatten().map(|m| !hidden.contains(&m.conv_id)).unwrap_or(true)).collect()
    }

    pub fn lock_status(&self) -> LockStatus {
        let now = now_ms();
        let t = timeout_settings(&self.store);
        let th = throttle(&self.store);
        LockStatus { pin_set: pin_record(&self.store).is_some(), open: self.lock_open.load(Ordering::SeqCst), wait_secs: th.wait_secs(now), attempts_left: th.attempts_left(), timeout_secs: t.secs, on_background: t.on_background }
    }

    fn set_open(&self, open: bool) {
        let was = self.lock_open.swap(open, Ordering::SeqCst);
        self.touch();
        if was != open {
            self.counts_wake.notify_one();
            self.emit(CoreEvent::Reset);
        }
    }

    pub fn touch(&self) {
        self.last_active.store(now_ms(), Ordering::SeqCst);
    }

    pub fn lock_set_pin(&self, pin: &str, confirm: &str) -> Result<()> {
        if pin_record(&self.store).is_some() {
            return Err(CoreError::local("A PIN is already set. Use Change PIN."));
        }
        if !pin_valid(pin) {
            return Err(CoreError::local("The PIN must be exactly 6 digits."));
        }
        if pin != confirm {
            return Err(CoreError::local("The two PINs do not match."));
        }
        self.store.kv_set(KEY_PIN, &make_record(pin, ITERATIONS))?;
        self.store.kv_set(KEY_THROTTLE, &Throttle::default())?;
        self.set_open(true);
        Ok(())
    }

    pub fn lock_unlock(&self, pin: &str) -> Result<UnlockResult> {
        let r = try_pin(&self.store, pin, now_ms())?;
        if r.ok {
            self.set_open(true);
        }
        Ok(r)
    }

    pub fn lock_relock(&self) {
        self.set_open(false);
    }

    pub fn lock_change_pin(&self, old: &str, new: &str, confirm: &str) -> Result<UnlockResult> {
        if !pin_valid(new) {
            return Err(CoreError::local("The new PIN must be exactly 6 digits."));
        }
        if new != confirm {
            return Err(CoreError::local("The two PINs do not match."));
        }
        let r = try_pin(&self.store, old, now_ms())?;
        if r.ok {
            self.store.kv_set(KEY_PIN, &make_record(new, ITERATIONS))?;
        }
        Ok(r)
    }

    pub fn lock_set_timeout(&self, secs: i64, on_background: bool) -> Result<()> {
        self.store.kv_set(KEY_TIMEOUT, &LockTimeout { secs: secs.clamp(0, 86_400), on_background })
    }

    /// Called by the UI on a timer and when the app comes back: locks the area after the inactivity timeout.
    pub fn lock_tick(&self) -> bool {
        if !self.lock_open.load(Ordering::SeqCst) {
            return false;
        }
        let t = timeout_settings(&self.store);
        if t.secs > 0 && now_ms() - self.last_active.load(Ordering::SeqCst) >= t.secs * 1000 {
            self.lock_relock();
            return true;
        }
        false
    }

    /// The app went to the background / was minimised.
    pub fn lock_background(&self) {
        if timeout_settings(&self.store).on_background {
            self.lock_relock();
        }
    }

    pub async fn lock_chat(&self, conv: &str, on: bool) -> Result<()> {
        if self.store.get_conv(conv)?.is_none() {
            return Err(CoreError::local("This conversation is not available."));
        }
        if on {
            if pin_record(&self.store).is_none() {
                return Err(CoreError::local("Set a PIN first."));
            }
            set_locked(&self.store, conv, true)?;
            if self.lock_open.load(Ordering::SeqCst) {
                self.emit_row(conv);
            } else {
                self.emit(CoreEvent::ChatRemove { id: conv.to_string() });
            }
            self.touch();
        } else {
            if !self.lock_open.load(Ordering::SeqCst) {
                return Err(CoreError::local("Enter your PIN to unlock."));
            }
            set_locked(&self.store, conv, false)?;
            self.emit_row(conv);
            self.touch();
        }
        self.counts_wake.notify_one();
        // Remember how many files the locked chats hold, so the Files pill can leave them out.
        if let Ok(api) = self.api() {
            self.refresh_file_adj(&api).await;
        }
        self.counts_wake.notify_one();
        Ok(())
    }

    /// Counts the files of every locked chat (`GET /files?conv=`) for the pill adjustments.
    pub async fn refresh_file_adj(&self, api: &crate::api::Api) {
        let locked = locked_set(&self.store);
        let mut adj: FileAdj = BTreeMap::new();
        for conv in locked {
            let mut n = [0i64; 4];
            let mut offset = 0;
            for _ in 0..5 {
                let q = [("conv", conv.clone()), ("limit", "200".to_string()), ("offset", offset.to_string())];
                let Ok(hits) = api.get::<Vec<FileHit>>("/files", &q).await else { break };
                for h in hits.iter().filter(|h| !h.attachment.removed) {
                    let slot = match crate::view::attachment_kind(&h.attachment) {
                        "image" => 0,
                        "video" => 1,
                        "audio" | "voice" => 2,
                        _ => 3,
                    };
                    n[slot] += 1;
                }
                if hits.len() < 200 {
                    break;
                }
                offset += 200;
            }
            adj.insert(conv, n);
        }
        let _ = self.store.kv_set(KEY_FILE_ADJ, &adj);
    }

    pub fn lock_wipe_adj(&self) {
        let _ = self.store.kv_set(KEY_FILE_ADJ, &FileAdj::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_format() {
        assert!(pin_valid("012345"));
        assert!(!pin_valid("12345"));
        assert!(!pin_valid("1234567"));
        assert!(!pin_valid("12a456"));
        assert!(!pin_valid("١٢٣٤٥٦"));
    }

    #[test]
    fn hash_roundtrip_and_salt() {
        let a = make_record("123456", 1000);
        let b = make_record("123456", 1000);
        assert_ne!(a.salt, b.salt);
        assert_ne!(a.hash, b.hash);
        assert!(check_record(&a, "123456"));
        assert!(!check_record(&a, "123457"));
        assert!(!a.hash.contains("123456"));
        let mut broken = a.clone();
        broken.hash = "!!".into();
        assert!(!check_record(&broken, "123456"));
    }

    #[test]
    fn throttle_steps() {
        assert_eq!([0, 4, 5, 6, 7, 8, 9, 20].map(penalty_secs), [0, 0, 30, 60, 300, 900, 3600, 3600]);
        let mut t = Throttle::default();
        for _ in 0..4 {
            t.fail(1_000);
        }
        assert_eq!((t.wait_secs(1_000), t.attempts_left()), (0, 1));
        t.fail(1_000);
        assert_eq!(t.wait_secs(1_000), 30);
        assert_eq!(t.wait_secs(11_000), 20);
        assert_eq!(t.wait_secs(31_000), 0);
        assert_eq!(t.attempts_left(), 0);
    }

    #[test]
    fn try_pin_persists_throttle_and_resets() {
        let s = Store::open_memory().unwrap();
        assert!(try_pin(&s, "123456", 0).is_err());
        s.kv_set(KEY_PIN, &make_record("123456", 1000)).unwrap();
        let r = try_pin(&s, "000000", 0).unwrap();
        assert_eq!((r.ok, r.attempts_left), (false, 4));
        // the right PIN works and clears the counter
        assert!(try_pin(&s, "123456", 1).unwrap().ok);
        assert_eq!(throttle(&s).fails, 0);
        for i in 0..5 {
            assert!(!try_pin(&s, "111111", 10 + i).unwrap().ok);
        }
        // the 5th failure starts the 30 s wait; even the right PIN is refused during it
        let r = try_pin(&s, "123456", 100).unwrap();
        assert_eq!((r.ok, r.wait_secs > 0), (false, true));
        assert!(try_pin(&s, "123456", 40_000).unwrap().ok);
        // not a PIN at all counts as a wrong attempt
        assert!(!try_pin(&s, "abc", 40_001).unwrap().ok);
        assert_eq!(throttle(&s).fails, 1);
    }

    #[test]
    fn locked_set_roundtrip() {
        let s = Store::open_memory().unwrap();
        set_locked(&s, "b", true).unwrap();
        set_locked(&s, "a", true).unwrap();
        set_locked(&s, "b", false).unwrap();
        assert_eq!(locked_set(&s).into_iter().collect::<Vec<_>>(), vec!["a".to_string()]);
    }

    #[test]
    fn counts_leave_locked_chats_out() {
        let s = Store::open_memory().unwrap();
        let conv = |id: &str, kind| Conversation { id: id.into(), kind, name: Some(id.into()), topic: None, archived: false, created_ms: 1, members: vec![], last_seq: 1, last_message_ts: None, avatar: None };
        for (id, kind, unread) in [("a", ConversationKind::Dm, 3), ("b", ConversationKind::Channel, 2)] {
            s.upsert_conv(&ConversationSummary { conversation: conv(id, kind), last_read_seq: 0, marked_unread: false, unread, mentions: 1 }).unwrap();
        }
        let c = Counts { conversations: 2, dms: 1, channels: 1, unread_conversations: 2, unread_dms: 1, unread_channels: 1, unread_messages: 5, mentions: 2, threads: 0, threads_unread: 0, files: 4, files_image: 2, files_video: 0, files_audio: 0, files_document: 2, saved: 0 };
        s.kv_set(KEY_FILE_ADJ, &FileAdj::from([("b".to_string(), [1, 0, 0, 1])])).unwrap();
        let hidden: HashSet<String> = ["b".to_string()].into();
        let r = adjust_counts_with(c.clone(), &s, &hidden);
        assert_eq!((r.conversations, r.channels, r.dms, r.unread_messages, r.mentions, r.files, r.files_image, r.files_document), (1, 0, 1, 3, 1, 2, 1, 1));
        assert_eq!(adjust_counts_with(c.clone(), &s, &HashSet::new()).conversations, 2);
    }
}
