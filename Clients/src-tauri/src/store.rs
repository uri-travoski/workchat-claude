//! Local SQLite cache: the last known state of the server for this account (conversations, messages, preferences,
//! threads), the sync cursor, the outbox of unsent messages and the media index. Everything except the outbox can be
//! rebuilt from the server; the outbox is kept across re-syncs.

use crate::error::Result;
use proto::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;
use std::sync::Mutex;

pub struct Store {
    conn: Mutex<Connection>,
}

/// A message the person sent that the server has not confirmed yet.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct OutboxItem {
    pub client_id: String,
    pub conv_id: String,
    pub thread_root: Option<String>,
    /// A thread reply that also shows in the channel ("Also send to channel"); one message on the server.
    #[serde(default)]
    pub also_send_to_channel: bool,
    pub reply_to: Option<String>,
    pub body: String,
    pub mentions: Vec<String>,
    pub ts: i64,
    /// `queued` (will be sent / retried) or `failed` (the server refused it; the person decides).
    pub state: String,
    pub attempts: i64,
    pub error: Option<String>,
    /// Files to upload before the message is sent (their local copies live in the staging folder).
    #[serde(default)]
    pub attachments: Vec<OutAtt>,
}

/// One file of an unsent message. `att_id` is set once the upload finished, `meta_set` once its details are saved.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct OutAtt {
    pub path: String,
    pub name: String,
    pub content_type: String,
    pub size: i64,
    #[serde(default)]
    pub meta: Option<AttachmentMeta>,
    #[serde(default)]
    pub att_id: Option<String>,
    #[serde(default)]
    pub meta_set: bool,
}

impl OutboxItem {
    pub fn local_id(&self) -> String {
        format!("local:{}", self.client_id)
    }
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS kv (k TEXT PRIMARY KEY, v TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS convs (id TEXT PRIMARY KEY, json TEXT NOT NULL, has_more_before INTEGER NOT NULL DEFAULT 1);
CREATE TABLE IF NOT EXISTS messages (
  id TEXT PRIMARY KEY, conv_id TEXT NOT NULL, seq INTEGER NOT NULL, ts INTEGER NOT NULL,
  thread_root TEXT, client_id TEXT, json TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS messages_conv ON messages(conv_id, thread_root, seq);
CREATE INDEX IF NOT EXISTS messages_root ON messages(thread_root, seq);
CREATE TABLE IF NOT EXISTS prefs (conv_id TEXT PRIMARY KEY, json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS threads (root_id TEXT PRIMARY KEY, last_ts INTEGER NOT NULL, json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS outbox (
  client_id TEXT PRIMARY KEY, conv_id TEXT NOT NULL, thread_root TEXT, reply_to TEXT, body TEXT NOT NULL,
  mentions TEXT NOT NULL, ts INTEGER NOT NULL, state TEXT NOT NULL DEFAULT 'queued', attempts INTEGER NOT NULL DEFAULT 0, error TEXT, attachments TEXT NOT NULL DEFAULT '[]', also_channel INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS media (id TEXT PRIMARY KEY, kind TEXT NOT NULL DEFAULT 'file', content_type TEXT NOT NULL, size INTEGER NOT NULL, path TEXT NOT NULL, cached_ms INTEGER NOT NULL);
";

fn to_json<T: Serialize>(v: &T) -> Result<String> {
    Ok(serde_json::to_string(v)?)
}
fn from_json<T: DeserializeOwned>(s: &str) -> Result<T> {
    Ok(serde_json::from_str(s)?)
}

/// kv keys that belong to this device, not to the server's data (quoted for SQL).
const LOCAL_KEYS: &str = "'lock_pin','lock_throttle','lock_chats','lock_timeout','lock_file_adj','notify','tz_offset','push','push_target','update_api','update_checked_ms','update_found','local_avatars','hidden_msgs'";

impl Store {
    pub fn open(path: &Path) -> Result<Store> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn)
    }

    pub fn open_memory() -> Result<Store> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Store> {
        conn.execute_batch(SCHEMA)?;
        // Databases made by 1.0 have no attachments column on the outbox.
        let _ = conn.execute("ALTER TABLE outbox ADD COLUMN attachments TEXT NOT NULL DEFAULT '[]'", []);
        let _ = conn.execute("ALTER TABLE outbox ADD COLUMN also_channel INTEGER NOT NULL DEFAULT 0", []);
        Ok(Store { conn: Mutex::new(conn) })
    }

    fn c(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Forget everything (sign-out, kicked, other account). Includes the outbox: unsent messages of a
    /// signed-out account must never be sent by the next one.
    pub fn wipe(&self) -> Result<()> {
        let c = self.c();
        c.execute_batch("DELETE FROM kv; DELETE FROM users; DELETE FROM convs; DELETE FROM messages; DELETE FROM prefs; DELETE FROM threads; DELETE FROM outbox; DELETE FROM media;")?;
        Ok(())
    }

    // ---------------------------------------------------------------- key/value

    pub fn kv_get<T: DeserializeOwned>(&self, k: &str) -> Result<Option<T>> {
        let c = self.c();
        let v: Option<String> = c.query_row("SELECT v FROM kv WHERE k=?1", [k], |r| r.get(0)).optional()?;
        v.map(|s| from_json(&s)).transpose()
    }
    pub fn kv_set<T: Serialize>(&self, k: &str, v: &T) -> Result<()> {
        self.c().execute("INSERT INTO kv(k,v) VALUES(?1,?2) ON CONFLICT(k) DO UPDATE SET v=excluded.v", params![k, to_json(v)?])?;
        Ok(())
    }
    pub fn cursor(&self) -> Option<i64> {
        self.kv_get::<i64>("cursor").ok().flatten()
    }
    pub fn set_cursor(&self, c: i64) -> Result<()> {
        self.kv_set("cursor", &c)
    }
    /// Forces a fresh bootstrap on the next sync round.
    pub fn clear_cursor(&self) -> Result<()> {
        self.c().execute("DELETE FROM kv WHERE k='cursor'", [])?;
        Ok(())
    }

    // ---------------------------------------------------------------- bootstrap

    /// Replaces the whole cache with a server snapshot (keeps the outbox and the media index).
    pub fn replace_bootstrap(&self, b: &Bootstrap) -> Result<()> {
        let mut c = self.c();
        let tx = c.transaction()?;
        // Server data is replaced; settings that only exist on this device (PIN and locked chats, notification and update
        // settings, push registration, private pictures) survive a re-sync.
        tx.execute_batch(&format!("DELETE FROM kv WHERE k NOT IN ({LOCAL_KEYS}); DELETE FROM users; DELETE FROM convs; DELETE FROM messages; DELETE FROM prefs; DELETE FROM threads;"))?;
        for u in &b.users {
            tx.execute("INSERT OR REPLACE INTO users(id,json) VALUES(?1,?2)", params![u.id, to_json(u)?])?;
        }
        for cs in &b.conversations {
            let more = b.has_more.get(&cs.conversation.id).copied().unwrap_or(true);
            tx.execute("INSERT OR REPLACE INTO convs(id,json,has_more_before) VALUES(?1,?2,?3)", params![cs.conversation.id, to_json(cs)?, more as i64])?;
        }
        for ms in b.messages.values() {
            for m in ms {
                insert_message(&tx, m)?;
            }
        }
        for p in &b.prefs {
            tx.execute("INSERT OR REPLACE INTO prefs(conv_id,json) VALUES(?1,?2)", params![p.conv_id, to_json(p)?])?;
        }
        for t in &b.threads {
            tx.execute("INSERT OR REPLACE INTO threads(root_id,last_ts,json) VALUES(?1,?2,?3)", params![t.root.id, t.last_reply_ts, to_json(t)?])?;
        }
        let put = |k: &str, v: String| tx.execute("INSERT OR REPLACE INTO kv(k,v) VALUES(?1,?2)", params![k, v]);
        put("me", to_json(&b.me)?)?;
        put("user_id", to_json(&b.me.user.id)?)?;
        put("sections", to_json(&b.sections)?)?;
        put("groups", to_json(&b.groups)?)?;
        put("avatar_overrides", to_json(&b.avatar_overrides)?)?;
        put("pins", to_json(&b.pins)?)?;
        put("saved", to_json(&b.saved)?)?;
        put("labels", to_json(&b.labels)?)?;
        put("cursor", to_json(&b.cursor)?)?;
        put("org_settings", to_json(&b.org_settings)?)?;
        tx.commit()?;
        Ok(())
    }

    // ---------------------------------------------------------------- users

    pub fn upsert_user(&self, u: &User) -> Result<()> {
        self.c().execute("INSERT OR REPLACE INTO users(id,json) VALUES(?1,?2)", params![u.id, to_json(u)?])?;
        Ok(())
    }
    pub fn get_user(&self, id: &str) -> Result<Option<User>> {
        let c = self.c();
        let s: Option<String> = c.query_row("SELECT json FROM users WHERE id=?1", [id], |r| r.get(0)).optional()?;
        s.map(|s| from_json(&s)).transpose()
    }
    pub fn all_users(&self) -> Result<Vec<User>> {
        let c = self.c();
        let mut st = c.prepare("SELECT json FROM users")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?.collect::<std::result::Result<Vec<_>, _>>()?;
        rows.iter().map(|s| from_json(s)).collect()
    }

    // ---------------------------------------------------------------- conversations

    pub fn upsert_conv(&self, cs: &ConversationSummary) -> Result<()> {
        self.c().execute(
            "INSERT INTO convs(id,json) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET json=excluded.json",
            params![cs.conversation.id, to_json(cs)?],
        )?;
        Ok(())
    }
    pub fn get_conv(&self, id: &str) -> Result<Option<ConversationSummary>> {
        let c = self.c();
        let s: Option<String> = c.query_row("SELECT json FROM convs WHERE id=?1", [id], |r| r.get(0)).optional()?;
        s.map(|s| from_json(&s)).transpose()
    }
    pub fn all_convs(&self) -> Result<Vec<ConversationSummary>> {
        let c = self.c();
        let mut st = c.prepare("SELECT json FROM convs")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?.collect::<std::result::Result<Vec<_>, _>>()?;
        rows.iter().map(|s| from_json(s)).collect()
    }
    pub fn remove_conv(&self, id: &str) -> Result<()> {
        let c = self.c();
        c.execute("DELETE FROM convs WHERE id=?1", [id])?;
        c.execute("DELETE FROM messages WHERE conv_id=?1", [id])?;
        c.execute("DELETE FROM prefs WHERE conv_id=?1", [id])?;
        c.execute("DELETE FROM threads WHERE json_extract(json,'$.conv_id')=?1", [id])?;
        c.execute("DELETE FROM outbox WHERE conv_id=?1", [id])?;
        Ok(())
    }
    pub fn has_more_before(&self, conv: &str) -> bool {
        let c = self.c();
        c.query_row("SELECT has_more_before FROM convs WHERE id=?1", [conv], |r| r.get::<_, i64>(0)).map(|v| v != 0).unwrap_or(false)
    }
    pub fn set_has_more_before(&self, conv: &str, more: bool) -> Result<()> {
        self.c().execute("UPDATE convs SET has_more_before=?2 WHERE id=?1", params![conv, more as i64])?;
        Ok(())
    }

    // ---------------------------------------------------------------- messages

    pub fn upsert_message(&self, m: &Message) -> Result<()> {
        let c = self.c();
        insert_message(&c, m)
    }
    pub fn get_message(&self, id: &str) -> Result<Option<Message>> {
        let c = self.c();
        let s: Option<String> = c.query_row("SELECT json FROM messages WHERE id=?1", [id], |r| r.get(0)).optional()?;
        s.map(|s| from_json(&s)).transpose()
    }
    pub fn delete_messages(&self, ids: &[String]) -> Result<()> {
        let c = self.c();
        for id in ids {
            c.execute("DELETE FROM messages WHERE id=?1", [id])?;
        }
        Ok(())
    }
    /// The main timeline of a conversation (thread replies excluded unless they were also sent to the channel), ascending by seq. `before` is exclusive.
    pub fn messages_page(&self, conv: &str, before: Option<i64>, limit: i64) -> Result<Vec<Message>> {
        let c = self.c();
        let mut st = c.prepare("SELECT json FROM messages WHERE conv_id=?1 AND (thread_root IS NULL OR json_extract(json,'$.also_send_to_channel')=1) AND seq<?2 ORDER BY seq DESC LIMIT ?3")?;
        let rows = st.query_map(params![conv, before.unwrap_or(i64::MAX), limit], |r| r.get::<_, String>(0))?.collect::<std::result::Result<Vec<_>, _>>()?;
        let mut v: Vec<Message> = rows.iter().map(|s| from_json(s)).collect::<Result<_>>()?;
        v.reverse();
        Ok(v)
    }
    pub fn thread_messages(&self, root: &str) -> Result<Vec<Message>> {
        let c = self.c();
        let mut st = c.prepare("SELECT json FROM messages WHERE thread_root=?1 ORDER BY seq")?;
        let rows = st.query_map([root], |r| r.get::<_, String>(0))?.collect::<std::result::Result<Vec<_>, _>>()?;
        rows.iter().map(|s| from_json(s)).collect()
    }
    pub fn last_message(&self, conv: &str) -> Result<Option<Message>> {
        let c = self.c();
        let s: Option<String> = c
            .query_row("SELECT json FROM messages WHERE conv_id=?1 AND (thread_root IS NULL OR json_extract(json,'$.also_send_to_channel')=1) ORDER BY seq DESC LIMIT 1", [conv], |r| r.get(0))
            .optional()?;
        s.map(|s| from_json(&s)).transpose()
    }
    pub fn min_seq(&self, conv: &str) -> Option<i64> {
        let c = self.c();
        c.query_row("SELECT MIN(seq) FROM messages WHERE conv_id=?1 AND (thread_root IS NULL OR json_extract(json,'$.also_send_to_channel')=1)", [conv], |r| r.get::<_, Option<i64>>(0)).ok().flatten()
    }

    // ---------------------------------------------------------------- prefs and threads

    pub fn upsert_pref(&self, p: &ConvPref) -> Result<()> {
        self.c().execute("INSERT OR REPLACE INTO prefs(conv_id,json) VALUES(?1,?2)", params![p.conv_id, to_json(p)?])?;
        Ok(())
    }
    pub fn all_prefs(&self) -> Result<Vec<ConvPref>> {
        let c = self.c();
        let mut st = c.prepare("SELECT json FROM prefs")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?.collect::<std::result::Result<Vec<_>, _>>()?;
        rows.iter().map(|s| from_json(s)).collect()
    }
    pub fn get_pref(&self, conv: &str) -> Result<Option<ConvPref>> {
        let c = self.c();
        let s: Option<String> = c.query_row("SELECT json FROM prefs WHERE conv_id=?1", [conv], |r| r.get(0)).optional()?;
        s.map(|s| from_json(&s)).transpose()
    }
    pub fn upsert_thread(&self, t: &ThreadSummary) -> Result<()> {
        self.c().execute("INSERT OR REPLACE INTO threads(root_id,last_ts,json) VALUES(?1,?2,?3)", params![t.root.id, t.last_reply_ts, to_json(t)?])?;
        Ok(())
    }
    pub fn remove_thread(&self, root: &str) -> Result<()> {
        self.c().execute("DELETE FROM threads WHERE root_id=?1", [root])?;
        Ok(())
    }
    pub fn all_threads(&self) -> Result<Vec<ThreadSummary>> {
        let c = self.c();
        let mut st = c.prepare("SELECT json FROM threads ORDER BY last_ts DESC")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?.collect::<std::result::Result<Vec<_>, _>>()?;
        rows.iter().map(|s| from_json(s)).collect()
    }

    // ---------------------------------------------------------------- outbox

    pub fn outbox_add(&self, o: &OutboxItem) -> Result<()> {
        self.c().execute(
            "INSERT OR REPLACE INTO outbox(client_id,conv_id,thread_root,reply_to,body,mentions,ts,state,attempts,error,attachments,also_channel) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![o.client_id, o.conv_id, o.thread_root, o.reply_to, o.body, to_json(&o.mentions)?, o.ts, o.state, o.attempts, o.error, to_json(&o.attachments)?, o.also_send_to_channel as i64],
        )?;
        Ok(())
    }
    fn outbox_query(&self, filter: &str, arg: Option<&str>) -> Result<Vec<OutboxItem>> {
        let c = self.c();
        let sql = format!("SELECT client_id,conv_id,thread_root,reply_to,body,mentions,ts,state,attempts,error,attachments,also_channel FROM outbox {filter} ORDER BY ts, rowid");
        let mut st = c.prepare(&sql)?;
        let map = |r: &rusqlite::Row| {
            Ok((
                OutboxItem {
                    client_id: r.get(0)?,
                    conv_id: r.get(1)?,
                    thread_root: r.get(2)?,
                    also_send_to_channel: r.get::<_, i64>(11)? != 0,
                    reply_to: r.get(3)?,
                    body: r.get(4)?,
                    mentions: vec![],
                    ts: r.get(6)?,
                    state: r.get(7)?,
                    attempts: r.get(8)?,
                    error: r.get(9)?,
                    attachments: vec![],
                },
                r.get::<_, String>(5)?,
                r.get::<_, String>(10)?,
            ))
        };
        let rows = match arg {
            Some(a) => st.query_map([a], map)?.collect::<std::result::Result<Vec<_>, _>>()?,
            None => st.query_map([], map)?.collect::<std::result::Result<Vec<_>, _>>()?,
        };
        rows.into_iter()
            .map(|(mut o, m, a)| {
                o.mentions = from_json(&m)?;
                o.attachments = from_json(&a).unwrap_or_default();
                Ok(o)
            })
            .collect()
    }
    pub fn outbox_queued(&self) -> Result<Vec<OutboxItem>> {
        self.outbox_query("WHERE state='queued'", None)
    }
    pub fn outbox_for_conv(&self, conv: &str) -> Result<Vec<OutboxItem>> {
        self.outbox_query("WHERE conv_id=?1", Some(conv))
    }
    pub fn outbox_get(&self, client_id: &str) -> Result<Option<OutboxItem>> {
        Ok(self.outbox_query("WHERE client_id=?1", Some(client_id))?.into_iter().next())
    }
    pub fn outbox_delete(&self, client_id: &str) -> Result<bool> {
        Ok(self.c().execute("DELETE FROM outbox WHERE client_id=?1", [client_id])? > 0)
    }
    pub fn outbox_set_attachments(&self, client_id: &str, a: &[OutAtt]) -> Result<()> {
        self.c().execute("UPDATE outbox SET attachments=?2 WHERE client_id=?1", params![client_id, to_json(&a)?])?;
        Ok(())
    }
    pub fn outbox_all(&self) -> Result<Vec<OutboxItem>> {
        self.outbox_query("", None)
    }
    pub fn outbox_mark(&self, client_id: &str, state: &str, error: Option<&str>, bump: bool) -> Result<()> {
        self.c().execute("UPDATE outbox SET state=?2, error=?3, attempts=attempts+?4 WHERE client_id=?1", params![client_id, state, error, bump as i64])?;
        Ok(())
    }

    // ---------------------------------------------------------------- media index

    pub fn media_clear(&self) -> Result<()> {
        self.c().execute("DELETE FROM media", [])?;
        Ok(())
    }
    pub fn media_has(&self, key: &str) -> bool {
        self.media_get(key).map(|(_, _, p)| std::path::Path::new(&p).exists()).unwrap_or(false)
    }
    /// Rewrites the database file without free pages and folds the write-ahead log into it.
    pub fn compact(&self) -> Result<()> {
        let c = self.c();
        c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    /// `key` is the attachment id, or `<id>:thumb` for a generated thumbnail.
    pub fn media_get(&self, key: &str) -> Option<(String, i64, String)> {
        let c = self.c();
        c.query_row("SELECT content_type,size,path FROM media WHERE id=?1", params![key], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).ok()
    }
    pub fn media_put(&self, key: &str, content_type: &str, size: i64, path: &str) -> Result<()> {
        self.c().execute(
            "INSERT OR REPLACE INTO media(id,kind,content_type,size,path,cached_ms) VALUES(?1,'file',?2,?3,?4,?5)",
            params![key, content_type, size, path, crate::api::now_ms()],
        )?;
        Ok(())
    }
}

fn insert_message(c: &Connection, m: &Message) -> Result<()> {
    c.execute(
        "INSERT OR REPLACE INTO messages(id,conv_id,seq,ts,thread_root,client_id,json) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![m.id, m.conv_id, m.seq, m.ts, m.thread_root, m.client_id, to_json(m)?],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, atts: Vec<OutAtt>) -> OutboxItem {
        OutboxItem { client_id: id.into(), conv_id: "c1".into(), thread_root: None, also_send_to_channel: false, reply_to: None, body: "caption".into(), mentions: vec![], ts: 5, state: "queued".into(), attempts: 0, error: None, attachments: atts }
    }
    fn out_att(name: &str) -> OutAtt {
        OutAtt { path: format!("/staging/{name}"), name: name.into(), content_type: "image/jpeg".into(), size: 1234, meta: None, att_id: None, meta_set: false }
    }

    #[test]
    fn outbox_keeps_attachments_and_their_upload_progress() {
        let s = Store::open_memory().unwrap();
        s.outbox_add(&item("k1", vec![out_att("a.jpg"), out_att("b.jpg")])).unwrap();
        let got = s.outbox_get("k1").unwrap().unwrap();
        assert_eq!(got.attachments.len(), 2);
        assert!(got.attachments.iter().all(|a| a.att_id.is_none()));
        // The first file finished uploading and its details were saved: that must survive a restart.
        let mut a = got.attachments.clone();
        a[0].att_id = Some("att-77".into());
        a[0].meta_set = true;
        s.outbox_set_attachments("k1", &a).unwrap();
        let again = s.outbox_get("k1").unwrap().unwrap();
        assert_eq!(again.attachments[0].att_id.as_deref(), Some("att-77"));
        assert!(again.attachments[0].meta_set);
        assert!(again.attachments[1].att_id.is_none());
        assert_eq!(s.outbox_queued().unwrap().len(), 1);
        assert!(s.outbox_delete("k1").unwrap());
        assert!(s.outbox_get("k1").unwrap().is_none());
    }

    #[test]
    fn plain_text_outbox_items_have_no_attachments() {
        let s = Store::open_memory().unwrap();
        s.outbox_add(&item("t1", vec![])).unwrap();
        assert!(s.outbox_get("t1").unwrap().unwrap().attachments.is_empty());
    }
}
