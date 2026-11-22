//! Writes events into the per-user ordered log and collects them for live dispatch.

use crate::db::now_ms;
use crate::error::AppResult;
use crate::hub::Hub;
use proto::{Event, EventData};
use rusqlite::Connection;

#[derive(Default)]
pub struct Emitter {
    out: Vec<(String, Event)>,
}

impl Emitter {
    pub fn new() -> Emitter {
        Emitter::default()
    }

    pub fn emit(&mut self, conn: &Connection, user_id: &str, data: EventData) -> AppResult<i64> {
        let ts = now_ms();
        let reference = match &data {
            EventData::MessageNew { message } => Some(message.id.clone()),
            _ => None,
        };
        // The cursor is the autoincrement row id, which is also embedded in the stored JSON.
        conn.execute(
            "INSERT INTO events(user_id, ts, kind, ref, data) VALUES(?1, ?2, ?3, ?4, '')",
            rusqlite::params![user_id, ts, data.kind(), reference],
        )?;
        let cursor = conn.last_insert_rowid();
        let ev = Event { cursor, ts, event: data };
        conn.execute(
            "UPDATE events SET data=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&ev)?, cursor],
        )?;
        self.out.push((user_id.to_string(), ev));
        Ok(cursor)
    }

    pub fn emit_many<'a>(
        &mut self,
        conn: &Connection,
        users: impl IntoIterator<Item = &'a String>,
        make: impl Fn(&str) -> EventData,
    ) -> AppResult<()> {
        for u in users {
            self.emit(conn, u, make(u))?;
        }
        Ok(())
    }

    /// Hand the collected events to live connections. Call after the transaction commits,
    /// while the database lock is still held, so per-user order matches cursor order.
    pub fn flush(self, hub: &Hub) {
        hub.dispatch(self.out);
    }
}
