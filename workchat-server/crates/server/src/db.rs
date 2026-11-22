//! SQLite (WAL, FTS5) access. One writer connection behind a mutex; every call runs on the
//! blocking pool. Event dispatch happens while the lock is held so per-user ordering is exact.

use crate::error::AppError;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS users(
  id TEXT PRIMARY KEY,
  username TEXT NOT NULL UNIQUE COLLATE NOCASE,
  display_name TEXT NOT NULL,
  email TEXT,
  password_hash TEXT NOT NULL,
  password_set INTEGER NOT NULL DEFAULT 1,
  role TEXT NOT NULL DEFAULT 'member',
  disabled INTEGER NOT NULL DEFAULT 0,
  deleted INTEGER NOT NULL DEFAULT 0,
  privacy_controls_enabled INTEGER NOT NULL DEFAULT 0,
  hide_last_active INTEGER NOT NULL DEFAULT 0,
  presence_pref TEXT NOT NULL DEFAULT 'auto',
  last_active_ms INTEGER,
  avatar_attachment TEXT,
  is_bot INTEGER NOT NULL DEFAULT 0,
  acked_cursor INTEGER NOT NULL DEFAULT 0,
  created_ms INTEGER NOT NULL,
  title TEXT,
  phone TEXT,
  read_receipts INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS sessions(
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  device_name TEXT NOT NULL DEFAULT '',
  client_version TEXT NOT NULL DEFAULT '',
  ip TEXT NOT NULL DEFAULT '',
  access_hash TEXT NOT NULL,
  access_exp_ms INTEGER NOT NULL,
  refresh_hash TEXT NOT NULL,
  refresh_exp_ms INTEGER NOT NULL,
  created_ms INTEGER NOT NULL,
  last_seen_ms INTEGER NOT NULL,
  revoked INTEGER NOT NULL DEFAULT 0,
  revoke_reason TEXT
);
CREATE INDEX IF NOT EXISTS sessions_access ON sessions(access_hash);
CREATE INDEX IF NOT EXISTS sessions_refresh ON sessions(refresh_hash);
CREATE INDEX IF NOT EXISTS sessions_user ON sessions(user_id);
CREATE TABLE IF NOT EXISTS login_links(
  token_hash TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  created_ms INTEGER NOT NULL,
  expires_ms INTEGER NOT NULL,
  used_ms INTEGER
);
CREATE TABLE IF NOT EXISTS conversations(
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  name TEXT,
  topic TEXT,
  archived INTEGER NOT NULL DEFAULT 0,
  created_by TEXT,
  created_ms INTEGER NOT NULL,
  dm_key TEXT UNIQUE,
  last_seq INTEGER NOT NULL DEFAULT 0,
  last_ts INTEGER,
  avatar_attachment TEXT
);
CREATE TABLE IF NOT EXISTS members(
  conv_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  role TEXT NOT NULL DEFAULT 'member',
  joined_ms INTEGER NOT NULL,
  PRIMARY KEY(conv_id, user_id)
);
CREATE INDEX IF NOT EXISTS members_user ON members(user_id);
CREATE TABLE IF NOT EXISTS messages(
  id TEXT PRIMARY KEY,
  conv_id TEXT NOT NULL,
  seq INTEGER NOT NULL,
  sender_id TEXT NOT NULL,
  client_id TEXT,
  body TEXT NOT NULL,
  kind TEXT NOT NULL DEFAULT 'text',
  ts INTEGER NOT NULL,
  version INTEGER NOT NULL DEFAULT 1,
  edited_ms INTEGER,
  deleted INTEGER NOT NULL DEFAULT 0,
  thread_root TEXT,
  reply_to TEXT,
  mentions TEXT NOT NULL DEFAULT '[]',
  delivered_ms INTEGER,
  also_channel INTEGER NOT NULL DEFAULT 0,
  UNIQUE(conv_id, seq)
);
CREATE UNIQUE INDEX IF NOT EXISTS messages_client ON messages(sender_id, client_id) WHERE client_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS messages_ts ON messages(conv_id, ts);
CREATE TABLE IF NOT EXISTS message_edits(
  message_id TEXT NOT NULL,
  version INTEGER NOT NULL,
  body TEXT NOT NULL,
  ts INTEGER NOT NULL,
  PRIMARY KEY(message_id, version)
);
CREATE TABLE IF NOT EXISTS message_hidden(
  user_id TEXT NOT NULL,
  message_id TEXT NOT NULL,
  PRIMARY KEY(user_id, message_id)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS hidden_msg ON message_hidden(message_id);
CREATE TABLE IF NOT EXISTS reactions(
  message_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  emoji TEXT NOT NULL,
  ts INTEGER NOT NULL,
  PRIMARY KEY(message_id, user_id, emoji)
);
CREATE TABLE IF NOT EXISTS attachments(
  id TEXT PRIMARY KEY,
  uploader_id TEXT NOT NULL,
  name TEXT NOT NULL,
  content_type TEXT NOT NULL,
  size INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  storage_key TEXT NOT NULL,
  purpose TEXT NOT NULL DEFAULT 'file',
  created_ms INTEGER NOT NULL,
  removed INTEGER NOT NULL DEFAULT 0,
  backup_id TEXT,
  meta TEXT
);
CREATE TABLE IF NOT EXISTS message_attachments(
  message_id TEXT NOT NULL,
  attachment_id TEXT NOT NULL,
  PRIMARY KEY(message_id, attachment_id)
);
CREATE INDEX IF NOT EXISTS ma_att ON message_attachments(attachment_id);
CREATE TABLE IF NOT EXISTS events(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id TEXT NOT NULL,
  ts INTEGER NOT NULL,
  kind TEXT NOT NULL,
  ref TEXT,
  data TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS events_user ON events(user_id, id);
CREATE TABLE IF NOT EXISTS read_state(
  user_id TEXT NOT NULL,
  conv_id TEXT NOT NULL,
  last_read_seq INTEGER NOT NULL DEFAULT 0,
  marked_unread INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(user_id, conv_id)
);
CREATE TABLE IF NOT EXISTS settings(
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS backups(
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  user_id TEXT,
  created_ms INTEGER NOT NULL,
  reason TEXT NOT NULL DEFAULT '',
  key TEXT NOT NULL,
  size INTEGER NOT NULL DEFAULT 0,
  attachments INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS pins(
  conv_id TEXT NOT NULL,
  message_id TEXT NOT NULL,
  pinned_by TEXT NOT NULL,
  ts INTEGER NOT NULL,
  PRIMARY KEY(conv_id, message_id)
);
CREATE TABLE IF NOT EXISTS bookmarks(
  id TEXT PRIMARY KEY,
  conv_id TEXT NOT NULL,
  title TEXT NOT NULL,
  url TEXT,
  attachment_id TEXT,
  position INTEGER NOT NULL DEFAULT 0,
  created_by TEXT NOT NULL,
  created_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS bookmarks_conv ON bookmarks(conv_id);
CREATE TABLE IF NOT EXISTS user_groups(
  id TEXT PRIMARY KEY,
  handle TEXT NOT NULL UNIQUE COLLATE NOCASE,
  name TEXT NOT NULL,
  created_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS user_group_members(
  group_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  PRIMARY KEY(group_id, user_id)
);
CREATE TABLE IF NOT EXISTS thread_follows(
  user_id TEXT NOT NULL,
  thread_root TEXT NOT NULL,
  conv_id TEXT NOT NULL,
  following INTEGER NOT NULL DEFAULT 1,
  last_read_seq INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(user_id, thread_root)
);
CREATE INDEX IF NOT EXISTS thread_follows_root ON thread_follows(thread_root);
CREATE TABLE IF NOT EXISTS conv_prefs(
  user_id TEXT NOT NULL,
  conv_id TEXT NOT NULL,
  favourite INTEGER NOT NULL DEFAULT 0,
  section_id TEXT,
  mute_until_ms INTEGER,
  notify TEXT NOT NULL DEFAULT 'all',
  position INTEGER,
  PRIMARY KEY(user_id, conv_id)
);
-- a private picture the viewer chose for another user (kind 'user') or a conversation (kind 'conv').
CREATE TABLE IF NOT EXISTS avatar_overrides(
  viewer_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  target TEXT NOT NULL,
  attachment_id TEXT NOT NULL,
  updated_ms INTEGER NOT NULL,
  PRIMARY KEY(viewer_id, kind, target)
);
CREATE TABLE IF NOT EXISTS sections(
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  name TEXT NOT NULL,
  position INTEGER NOT NULL DEFAULT 0,
  collapsed INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS sections_user ON sections(user_id);
CREATE TABLE IF NOT EXISTS drafts(
  user_id TEXT NOT NULL,
  conv_id TEXT NOT NULL,
  thread_root TEXT NOT NULL DEFAULT '',
  body TEXT NOT NULL,
  updated_ms INTEGER NOT NULL,
  PRIMARY KEY(user_id, conv_id, thread_root)
);
CREATE TABLE IF NOT EXISTS push_registrations(
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  session_id TEXT,
  kind TEXT NOT NULL,
  target TEXT NOT NULL,
  ntfy_url TEXT,
  device_name TEXT NOT NULL DEFAULT '',
  created_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS push_user ON push_registrations(user_id);
CREATE TABLE IF NOT EXISTS bots(
  user_id TEXT PRIMARY KEY,
  description TEXT NOT NULL DEFAULT '',
  token TEXT NOT NULL UNIQUE,
  home TEXT NOT NULL DEFAULT '{"blocks":[]}',
  webhook_url TEXT,
  webhook_secret TEXT,
  webhook_error TEXT,
  webhook_error_ms INTEGER,
  created_by TEXT,
  created_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS bot_updates(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  bot_id TEXT NOT NULL,
  data TEXT NOT NULL,
  created_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS bot_updates_bot ON bot_updates(bot_id, id);
CREATE TABLE IF NOT EXISTS form_responses(
  message_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  button_id TEXT,
  data TEXT NOT NULL,
  ts INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS form_responses_msg ON form_responses(message_id);
CREATE TABLE IF NOT EXISTS webhooks_in(
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  conv_id TEXT NOT NULL,
  secret TEXT NOT NULL,
  bot_user_id TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_by TEXT,
  created_ms INTEGER NOT NULL,
  last_used_ms INTEGER
);
CREATE TABLE IF NOT EXISTS webhooks_out(
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  url TEXT NOT NULL,
  secret TEXT NOT NULL,
  events TEXT NOT NULL DEFAULT '[]',
  conv_id TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  created_by TEXT,
  created_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS deliveries(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL,
  ref_id TEXT NOT NULL,
  url TEXT NOT NULL,
  secret TEXT,
  event TEXT NOT NULL,
  body TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  next_ms INTEGER NOT NULL,
  state TEXT NOT NULL DEFAULT 'pending',
  last_error TEXT,
  last_status INTEGER,
  created_ms INTEGER NOT NULL,
  done_ms INTEGER
);
CREATE INDEX IF NOT EXISTS deliveries_due ON deliveries(state, next_ms);
CREATE INDEX IF NOT EXISTS deliveries_ref ON deliveries(kind, ref_id);
CREATE TABLE IF NOT EXISTS scheduled(
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  conv_id TEXT NOT NULL,
  body TEXT NOT NULL,
  thread_root TEXT,
  attachment_ids TEXT NOT NULL DEFAULT '[]',
  due_ms INTEGER NOT NULL,
  created_ms INTEGER NOT NULL,
  state TEXT NOT NULL DEFAULT 'pending',
  error TEXT,
  message_id TEXT,
  done_ms INTEGER
);
CREATE INDEX IF NOT EXISTS scheduled_due ON scheduled(state, due_ms);
CREATE INDEX IF NOT EXISTS scheduled_user ON scheduled(user_id);
CREATE TABLE IF NOT EXISTS labels(
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  name TEXT NOT NULL,
  color TEXT NOT NULL,
  position INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS labels_user ON labels(user_id);
CREATE TABLE IF NOT EXISTS saved_items(
  user_id TEXT NOT NULL,
  message_id TEXT NOT NULL,
  conv_id TEXT NOT NULL,
  saved_ms INTEGER NOT NULL,
  PRIMARY KEY(user_id, message_id)
);
CREATE TABLE IF NOT EXISTS saved_labels(
  user_id TEXT NOT NULL,
  message_id TEXT NOT NULL,
  label_id TEXT NOT NULL,
  PRIMARY KEY(user_id, message_id, label_id)
);
CREATE TABLE IF NOT EXISTS activity_state(
  user_id TEXT PRIMARY KEY,
  read_ms INTEGER NOT NULL DEFAULT 0
);
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(body, content='messages', content_rowid='rowid');
CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts(rowid, body) VALUES (new.rowid, new.body);
END;
CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, body) VALUES('delete', old.rowid, old.body);
END;
CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE OF body ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, body) VALUES('delete', old.rowid, old.body);
  INSERT INTO messages_fts(rowid, body) VALUES (new.rowid, new.body);
END;
"#;

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn new_id() -> String {
    uuid::Uuid::now_v7().simple().to_string()
}

impl Db {
    pub fn open(path: &Path) -> anyhow::Result<Db> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_memory() -> anyhow::Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> anyhow::Result<Db> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "OFF")?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Db { conn: Arc::new(Mutex::new(conn)) })
    }

    /// Runs `f` on the blocking pool with exclusive access to the connection.
    pub async fn call<T, F>(&self, f: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, AppError> + Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().unwrap_or_else(|e| e.into_inner());
            f(&mut guard)
        })
        .await
        .map_err(|e| AppError::internal(format!("db task: {e}")))?
    }

    /// Synchronous access for startup code.
    pub fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T, AppError>) -> Result<T, AppError> {
        let mut guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>, AppError> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| r.get(0))
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            e => Err(e),
        })?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [key, value],
    )?;
    Ok(())
}

fn has_column(conn: &Connection, table: &str, col: &str) -> rusqlite::Result<bool> {
    let mut st = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = st.query_map([], |r| r.get::<_, String>(1))?.collect::<Result<Vec<_>, _>>()?;
    Ok(names.iter().any(|n| n == col))
}

/// Additive migrations for databases created by earlier releases.
fn migrate(conn: &Connection) -> anyhow::Result<()> {
    if !has_column(conn, "messages", "fwd")? {
        conn.execute("ALTER TABLE messages ADD COLUMN fwd TEXT", [])?;
    }
    if !has_column(conn, "messages", "form")? {
        conn.execute("ALTER TABLE messages ADD COLUMN form TEXT", [])?;
    }
    if !has_column(conn, "conv_prefs", "position")? {
        conn.execute("ALTER TABLE conv_prefs ADD COLUMN position INTEGER", [])?;
    }
    if !has_column(conn, "conversations", "avatar_attachment")? {
        conn.execute("ALTER TABLE conversations ADD COLUMN avatar_attachment TEXT", [])?;
    }
    // 1.2.0 removed public channels. The 1.1.0 `visibility` column is left in place on existing databases (dropping it
    // would make restores of old backups and downgrades riskier) but nothing reads it any more; make every row `private`
    // so even a downgrade cannot expose a channel. Databases from before 1.1.0 never get the column.
    if has_column(conn, "conversations", "visibility")? {
        conn.execute("UPDATE conversations SET visibility='private' WHERE visibility!='private'", [])?;
    }
    if !has_column(conn, "messages", "also_channel")? {
        conn.execute("ALTER TABLE messages ADD COLUMN also_channel INTEGER NOT NULL DEFAULT 0", [])?;
    }
    if !has_column(conn, "users", "title")? {
        conn.execute("ALTER TABLE users ADD COLUMN title TEXT", [])?;
    }
    if !has_column(conn, "users", "phone")? {
        conn.execute("ALTER TABLE users ADD COLUMN phone TEXT", [])?;
    }
    if !has_column(conn, "users", "read_receipts")? {
        conn.execute("ALTER TABLE users ADD COLUMN read_receipts INTEGER NOT NULL DEFAULT 1", [])?;
    }
    if !has_column(conn, "attachments", "meta")? {
        conn.execute("ALTER TABLE attachments ADD COLUMN meta TEXT", [])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn old_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE conversations(id TEXT PRIMARY KEY, kind TEXT NOT NULL, name TEXT, topic TEXT, archived INTEGER NOT NULL DEFAULT 0, created_by TEXT, created_ms INTEGER NOT NULL, dm_key TEXT UNIQUE, last_seq INTEGER NOT NULL DEFAULT 0, last_ts INTEGER);
             CREATE TABLE messages(id TEXT PRIMARY KEY, conv_id TEXT NOT NULL, seq INTEGER NOT NULL, sender_id TEXT NOT NULL, body TEXT NOT NULL, ts INTEGER NOT NULL);
             CREATE TABLE conv_prefs(user_id TEXT NOT NULL, conv_id TEXT NOT NULL);
             CREATE TABLE users(id TEXT PRIMARY KEY);
             CREATE TABLE attachments(id TEXT PRIMARY KEY);
             INSERT INTO conversations(id,kind,name,created_ms) VALUES('c1','channel','old',1);
             INSERT INTO messages(id,conv_id,seq,sender_id,body,ts) VALUES('m1','c1',1,'u','hi',1);",
        )
        .unwrap();
        conn
    }

    /// A database from before 1.1.0 (no `visibility`) is upgraded in place and keeps its channels; no column is added.
    #[test]
    fn migration_keeps_pre_1_1_channels() {
        let conn = old_db();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap(); // running it twice is harmless
        assert!(!has_column(&conn, "conversations", "visibility").unwrap());
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM conversations WHERE id='c1' AND kind='channel'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        let also: i64 = conn.query_row("SELECT also_channel FROM messages WHERE id='m1'", [], |r| r.get(0)).unwrap();
        assert_eq!(also, 0);
    }

    /// A 1.1.0 database with public channels: every channel is kept, all become private, nothing is dropped.
    #[test]
    fn migration_makes_1_1_public_channels_private() {
        let conn = old_db();
        conn.execute_batch(
            "ALTER TABLE conversations ADD COLUMN visibility TEXT NOT NULL DEFAULT 'private';
             INSERT INTO conversations(id,kind,name,created_ms,visibility) VALUES('c2','channel','pub',2,'public');",
        )
        .unwrap();
        migrate(&conn).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM conversations", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
        let pubs: i64 = conn.query_row("SELECT COUNT(*) FROM conversations WHERE visibility!='private'", [], |r| r.get(0)).unwrap();
        assert_eq!(pubs, 0);
    }
}
