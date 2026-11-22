//! SV-08 search: full text over messages and file names with filters, strictly limited to the
//! caller's conversations and to messages visible to the caller (per-user hide table).

use crate::auth::Auth;
use crate::error::AppResult;
use crate::queries::*;
use crate::routes::org::{like_escape, search_files};
use crate::state::St;
use axum::extract::{Query, State};
use axum::Json;
use proto::*;
use rusqlite::Connection;
use serde::Deserialize;

#[derive(Deserialize, Default, Clone, Debug)]
pub struct SearchParams {
    #[serde(default)]
    pub q: String,
    /// Sender: user id, username or display name.
    pub from: Option<String>,
    #[serde(rename = "in")]
    pub in_conv: Option<String>,
    /// UTC ms, exclusive.
    pub before: Option<i64>,
    pub after: Option<i64>,
    /// Comma separated: link, file, reaction.
    pub has: Option<String>,
    /// File type filter for the files list: image, video, audio, document.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub limit: Option<i64>,
}

/// Turns user text into a safe FTS5 query: every word quoted, prefix match on the last one.
pub fn fts_query(q: &str) -> Option<String> {
    let words: Vec<String> = q
        .split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '\'' || (!c.is_ascii() && !c.is_control())).collect::<String>())
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        return None;
    }
    let n = words.len();
    Some(
        words
            .iter()
            .enumerate()
            .map(|(i, w)| if i + 1 == n { format!("\"{w}\"*") } else { format!("\"{w}\"") })
            .collect::<Vec<_>>()
            .join(" "),
    )
}

pub async fn search(State(st): State<St>, auth: Auth, Query(p): Query<SearchParams>) -> AppResult<Json<SearchResponse>> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| run_search(c, &hub, &auth.user_id, &p)).await?))
}

pub fn run_search(c: &Connection, hub: &crate::hub::Hub, user_id: &str, p: &SearchParams) -> AppResult<SearchResponse> {
    let limit = p.limit.unwrap_or(30).clamp(1, 100);
    let fts = fts_query(&p.q);
    let has: Vec<String> = p.has.as_deref().unwrap_or("").split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect();
    let vis = visible_sql("?1");
    let mut sql = format!(
        "SELECT {MSG_COLS}, {} FROM messages m JOIN members mem ON mem.conv_id=m.conv_id AND mem.user_id=?1 ",
        if fts.is_some() { "snippet(messages_fts,0,'[[',']]','…',12)" } else { "m.body" }
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(user_id.to_string())];
    if let Some(f) = &fts {
        sql.push_str("JOIN messages_fts ON messages_fts.rowid=m.rowid ");
        sql.push_str(&format!("WHERE messages_fts MATCH ?{} ", args.len() + 1));
        args.push(Box::new(f.clone()));
    } else {
        sql.push_str("WHERE 1=1 ");
    }
    sql.push_str(&format!("AND m.deleted=0 AND {vis} "));
    if let Some(from) = &p.from {
        sql.push_str(&format!(
            "AND m.sender_id IN (SELECT id FROM users WHERE id=?{n} OR username=?{n} COLLATE NOCASE OR display_name=?{n} COLLATE NOCASE) ",
            n = args.len() + 1
        ));
        args.push(Box::new(from.trim_start_matches('@').to_string()));
    }
    if let Some(cv) = &p.in_conv {
        sql.push_str(&format!("AND m.conv_id=?{} ", args.len() + 1));
        args.push(Box::new(cv.clone()));
    }
    if let Some(b) = p.before {
        sql.push_str(&format!("AND m.ts<?{} ", args.len() + 1));
        args.push(Box::new(b));
    }
    if let Some(a) = p.after {
        sql.push_str(&format!("AND m.ts>?{} ", args.len() + 1));
        args.push(Box::new(a));
    }
    for h in &has {
        match h.as_str() {
            "file" => sql.push_str("AND EXISTS (SELECT 1 FROM message_attachments x WHERE x.message_id=m.id) "),
            "link" => sql.push_str("AND (m.body LIKE '%http://%' OR m.body LIKE '%https://%') "),
            "reaction" => sql.push_str("AND EXISTS (SELECT 1 FROM reactions x WHERE x.message_id=m.id) "),
            _ => {}
        }
    }
    let filtered = p.from.is_some() || p.in_conv.is_some() || p.before.is_some() || p.after.is_some() || !has.is_empty();
    let q = p.q.trim();
    let people = search_people(c, hub, q)?;
    let channels = search_channels(c, user_id, q)?;
    let files = if q.is_empty() && !filtered && p.kind.is_none() {
        vec![]
    } else {
        // File-name matches, honouring the same conversation / date / sender filters.
        let mut hits = search_files(c, user_id, p.in_conv.as_deref(), p.kind.as_deref(), if q.is_empty() { None } else { Some(q) }, p.before, limit, 0)?;
        if let Some(a) = p.after {
            hits.retain(|h| h.ts > a);
        }
        if let Some(f) = &p.from {
            let f = f.trim_start_matches('@').to_lowercase();
            hits.retain(|h| {
                c.query_row("SELECT 1 FROM users WHERE id=?1 AND (id=?2 OR lower(username)=?2 OR lower(display_name)=?2)", rusqlite::params![h.sender_id, f], |_| Ok(())).is_ok()
            });
        }
        hits
    };
    if fts.is_none() && !filtered {
        return Ok(SearchResponse { hits: vec![], people, files, channels });
    }
    sql.push_str(&format!("ORDER BY m.ts DESC LIMIT {limit}"));
    let mut stmt = c.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), |r| Ok((msg_base(r)?, r.get::<_, String>(18)?)))?.collect::<Result<Vec<_>, _>>()?;
    let snippets: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
    let msgs = hydrate(c, rows.into_iter().map(|r| r.0).collect())?;
    let hits = msgs.into_iter().zip(snippets).map(|(message, snippet)| SearchHit { message, snippet }).collect();
    Ok(SearchResponse { hits, people, files, channels })
}

pub fn search_people(conn: &Connection, hub: &crate::hub::Hub, q: &str) -> AppResult<Vec<User>> {
    let q = q.trim().trim_start_matches('@');
    if q.is_empty() {
        return Ok(vec![]);
    }
    let like = like_escape(q);
    let mut st = conn.prepare(&format!(
        "SELECT {USER_COLS} FROM users WHERE deleted=0 AND (username LIKE ?1 ESCAPE '\\' OR display_name LIKE ?1 ESCAPE '\\') ORDER BY display_name COLLATE NOCASE LIMIT 20"
    ))?;
    let rows = st.query_map([like], user_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows.iter().map(|r| to_user(r, hub)).collect())
}

/// SR-05: only channels and group DMs the caller belongs to.
pub fn search_channels(conn: &Connection, user_id: &str, q: &str) -> AppResult<Vec<ConversationSummary>> {
    let q = q.trim().trim_start_matches('#');
    if q.is_empty() {
        return Ok(vec![]);
    }
    let like = like_escape(q);
    let mut st = conn.prepare(
        "SELECT c.id FROM conversations c JOIN members m ON m.conv_id=c.id AND m.user_id=?1
         WHERE c.kind IN ('channel','group_dm') AND c.name LIKE ?2 ESCAPE '\\' ORDER BY c.name COLLATE NOCASE LIMIT 20",
    )?;
    let ids = st.query_map(rusqlite::params![user_id, like], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    ids.iter().map(|id| conv_summary(conn, id, user_id)).collect()
}
