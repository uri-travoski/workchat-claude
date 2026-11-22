//! Admin API for bots (SV-15), webhooks (SV-16) and AI settings (SV-21).

use crate::auth::{random_token, Admin};
use crate::bots;
use crate::db::{new_id, now_ms};
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::queries::*;
use crate::state::St;
use axum::extract::{Path, State};
use axum::Json;
use proto::*;
use rusqlite::{params, Connection, OptionalExtension};

type J<T> = AppResult<Json<T>>;

// ------------------------------------------------------------------ bots

pub async fn list_bots(State(st): State<St>, Admin(_a): Admin) -> J<Vec<AdminBot>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let mut stmt = c.prepare("SELECT b.user_id FROM bots b JOIN users u ON u.id=b.user_id WHERE u.deleted=0 ORDER BY u.display_name COLLATE NOCASE")?;
                let ids = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                ids.iter().map(|id| bots::admin_bot(c, &hub, id)).collect()
            })
            .await?,
    ))
}

pub async fn create_bot(State(st): State<St>, Admin(a): Admin, Json(req): Json<CreateBotRequest>) -> J<AdminBot> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let tx = c.transaction()?;
                let mut em = Emitter::new();
                let row = bots::create_bot_user(&tx, &hub, req.username.trim(), &req.display_name, &mut em)?;
                if req.description.chars().count() > 500 {
                    return Err(AppError::validation("description is too long"));
                }
                tx.execute(
                    "INSERT INTO bots(user_id,description,token,created_by,created_ms) VALUES(?1,?2,?3,?4,?5)",
                    params![row.id, req.description.trim(), bots::new_token(), a.user_id, now_ms()],
                )?;
                for conv in &req.conv_ids {
                    bots::add_to_conversation(&tx, &hub, conv, &row.id, &mut em)?;
                }
                let out = bots::admin_bot(&tx, &hub, &row.id)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(out)
            })
            .await?,
    ))
}

pub async fn get_bot(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>) -> J<AdminBot> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| bots::admin_bot(c, &hub, &id)).await?))
}

pub async fn update_bot(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>, Json(req): Json<UpdateBotRequest>) -> J<AdminBot> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                bots::admin_bot(c, &hub, &id)?;
                let tx = c.transaction()?;
                if let Some(n) = &req.display_name {
                    let n = n.trim();
                    if n.is_empty() || n.chars().count() > 80 {
                        return Err(AppError::validation("display name must be 1-80 characters"));
                    }
                    tx.execute("UPDATE users SET display_name=?2 WHERE id=?1", params![id, n])?;
                }
                if let Some(d) = &req.description {
                    if d.chars().count() > 500 {
                        return Err(AppError::validation("description is too long"));
                    }
                    tx.execute("UPDATE bots SET description=?2 WHERE user_id=?1", params![id, d.trim()])?;
                }
                if let Some(d) = req.disabled {
                    tx.execute("UPDATE users SET disabled=?2 WHERE id=?1", params![id, d as i64])?;
                }
                if req.title.is_some() {
                    tx.execute("UPDATE users SET title=?2 WHERE id=?1", params![id, crate::users::profile_field(&req.title, 80, "title")?])?;
                }
                if let Some(a) = &req.avatar {
                    if a.is_empty() {
                        tx.execute("UPDATE users SET avatar_attachment=NULL WHERE id=?1", [&id])?;
                    } else {
                        let ok: i64 = tx.query_row("SELECT COUNT(*) FROM attachments WHERE id=?1 AND purpose='avatar' AND removed=0", [a], |r| r.get(0))?;
                        if ok == 0 {
                            return Err(AppError::validation("unknown avatar upload"));
                        }
                        tx.execute("UPDATE users SET avatar_attachment=?2 WHERE id=?1", params![id, a])?;
                    }
                }
                let mut em = Emitter::new();
                crate::users::announce_user(&tx, &hub, &id, &mut em)?;
                let out = bots::admin_bot(&tx, &hub, &id)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(out)
            })
            .await?,
    ))
}

pub async fn regenerate_token(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>) -> J<AdminBot> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let n = c.execute("UPDATE bots SET token=?2 WHERE user_id=?1", params![id, bots::new_token()])?;
                if n == 0 {
                    return Err(AppError::not_found("bot not found"));
                }
                bots::admin_bot(c, &hub, &id)
            })
            .await?,
    ))
}

pub async fn delete_bot(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>) -> J<serde_json::Value> {
    let hub = st.hub.clone();
    st.db
        .call(move |c| {
            bots::admin_bot(c, &hub, &id)?;
            let tx = c.transaction()?;
            let convs: Vec<String> = {
                let mut s = tx.prepare("SELECT conv_id FROM members WHERE user_id=?1")?;
                let v = s.query_map([&id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                v
            };
            tx.execute("DELETE FROM members WHERE user_id=?1", [&id])?;
            tx.execute("DELETE FROM bots WHERE user_id=?1", [&id])?;
            tx.execute("DELETE FROM bot_updates WHERE bot_id=?1", [&id])?;
            tx.execute("DELETE FROM deliveries WHERE kind='bot' AND ref_id=?1", [&id])?;
            tx.execute("UPDATE users SET deleted=1, disabled=1 WHERE id=?1", [&id])?;
            let mut em = Emitter::new();
            for cv in convs {
                crate::routes::chat::upsert_for_members(&tx, &mut em, &cv)?;
            }
            crate::users::announce_user(&tx, &hub, &id, &mut em)?;
            tx.commit()?;
            em.flush(&hub);
            Ok(())
        })
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

pub async fn add_bot_to_conv(State(st): State<St>, Admin(_a): Admin, Path((id, conv)): Path<(String, String)>) -> J<AdminBot> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                bots::admin_bot(c, &hub, &id)?;
                let tx = c.transaction()?;
                let mut em = Emitter::new();
                bots::add_to_conversation(&tx, &hub, &conv, &id, &mut em)?;
                let out = bots::admin_bot(&tx, &hub, &id)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(out)
            })
            .await?,
    ))
}

pub async fn remove_bot_from_conv(State(st): State<St>, Admin(_a): Admin, Path((id, conv)): Path<(String, String)>) -> J<AdminBot> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                bots::admin_bot(c, &hub, &id)?;
                let tx = c.transaction()?;
                let mut em = Emitter::new();
                let cr = conv_row(&tx, &conv)?;
                if !matches!(cr.kind, ConversationKind::Channel | ConversationKind::GroupDm) {
                    return Err(AppError::validation("bots are removed from DMs by disabling them"));
                }
                // The conversation disappears for the bot, the others see the member list change.
                crate::routes::chat::upsert_for_members(&tx, &mut em, &conv)?;
                tx.execute("DELETE FROM members WHERE conv_id=?1 AND user_id=?2", params![conv, id])?;
                crate::routes::chat::upsert_for_members(&tx, &mut em, &conv)?;
                let out = bots::admin_bot(&tx, &hub, &id)?;
                tx.commit()?;
                em.flush(&hub);
                Ok(out)
            })
            .await?,
    ))
}

// ------------------------------------------------------------------ webhooks

fn incoming_row(conn: &Connection, id: &str) -> AppResult<(IncomingWebhook, String)> {
    conn.query_row(
        "SELECT w.id, w.name, w.conv_id, COALESCE(c.name,''), w.secret, w.bot_user_id, w.enabled, w.created_ms, w.last_used_ms
         FROM webhooks_in w LEFT JOIN conversations c ON c.id=w.conv_id WHERE w.id=?1",
        [id],
        |r| {
            let secret: String = r.get(4)?;
            let wid: String = r.get(0)?;
            Ok((
                IncomingWebhook {
                    path: format!("{}/hooks/{}/{}", API_PREFIX, wid, secret),
                    id: wid,
                    name: r.get(1)?,
                    conv_id: r.get(2)?,
                    conv_name: r.get(3)?,
                    bot_user_id: r.get(5)?,
                    enabled: r.get::<_, i64>(6)? != 0,
                    created_ms: r.get(7)?,
                    last_used_ms: r.get(8)?,
                },
                secret,
            ))
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("webhook not found"))
}

fn outgoing_row(conn: &Connection, id: &str) -> AppResult<OutgoingWebhook> {
    let mut w = conn
        .query_row("SELECT id,name,url,secret,events,conv_id,enabled,created_ms FROM webhooks_out WHERE id=?1", [id], |r| {
            Ok(OutgoingWebhook {
                id: r.get(0)?,
                name: r.get(1)?,
                url: r.get(2)?,
                secret: r.get(3)?,
                events: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
                conv_id: r.get(5)?,
                enabled: r.get::<_, i64>(6)? != 0,
                created_ms: r.get(7)?,
                last_status: None,
                last_delivery_ms: None,
                pending: 0,
                failed: 0,
            })
        })
        .optional()?
        .ok_or_else(|| AppError::not_found("webhook not found"))?;
    w.pending = conn.query_row("SELECT COUNT(*) FROM deliveries WHERE kind='hook' AND ref_id=?1 AND state='pending'", [id], |r| r.get(0))?;
    w.failed = conn.query_row("SELECT COUNT(*) FROM deliveries WHERE kind='hook' AND ref_id=?1 AND state='dead'", [id], |r| r.get(0))?;
    let last: Option<(String, Option<i64>, Option<String>, Option<i64>, i64)> = conn
        .query_row(
            "SELECT state, last_status, last_error, done_ms, created_ms FROM deliveries WHERE kind='hook' AND ref_id=?1 ORDER BY id DESC LIMIT 1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    if let Some((state, status, err, done, created)) = last {
        w.last_delivery_ms = Some(done.unwrap_or(created));
        w.last_status = Some(match (state.as_str(), status, err) {
            ("done", Some(s), _) => format!("delivered (HTTP {s})"),
            ("pending", _, Some(e)) => format!("retrying: {e}"),
            ("pending", _, None) => "queued".into(),
            (_, _, Some(e)) => format!("failed: {e}"),
            _ => state,
        });
    }
    Ok(w)
}

pub async fn list_webhooks(State(st): State<St>, Admin(_a): Admin) -> J<WebhooksOverview> {
    Ok(Json(
        st.db
            .call(|c| {
                let ids: Vec<String> = {
                    let mut s = c.prepare("SELECT id FROM webhooks_in ORDER BY created_ms")?;
                    let v = s.query_map([], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
                    v
                };
                let incoming = ids.iter().map(|i| incoming_row(c, i).map(|x| x.0)).collect::<AppResult<Vec<_>>>()?;
                let oids: Vec<String> = {
                    let mut s = c.prepare("SELECT id FROM webhooks_out ORDER BY created_ms")?;
                    let v = s.query_map([], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
                    v
                };
                let outgoing = oids.iter().map(|i| outgoing_row(c, i)).collect::<AppResult<Vec<_>>>()?;
                Ok(WebhooksOverview { incoming, outgoing })
            })
            .await?,
    ))
}

pub async fn create_incoming(State(st): State<St>, Admin(a): Admin, Json(req): Json<CreateIncomingWebhook>) -> J<IncomingWebhook> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let name = req.name.trim().to_string();
                if name.is_empty() || name.chars().count() > 80 {
                    return Err(AppError::validation("name must be 1-80 characters"));
                }
                let tx = c.transaction()?;
                let mut em = Emitter::new();
                let id = new_id();
                let username = format!("hook-{}", &id[id.len() - 8..]);
                let bot = bots::create_bot_user(&tx, &hub, &username, &name, &mut em)?;
                bots::add_to_conversation(&tx, &hub, &req.conv_id, &bot.id, &mut em)?;
                tx.execute(
                    "INSERT INTO webhooks_in(id,name,conv_id,secret,bot_user_id,created_by,created_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![id, name, req.conv_id, random_token("", 24), bot.id, a.user_id, now_ms()],
                )?;
                let out = incoming_row(&tx, &id)?.0;
                tx.commit()?;
                em.flush(&hub);
                Ok(out)
            })
            .await?,
    ))
}

#[derive(serde::Deserialize, Default)]
pub struct UpdateIncoming {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub regenerate_secret: Option<bool>,
}

pub async fn update_incoming(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>, Json(req): Json<UpdateIncoming>) -> J<IncomingWebhook> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let (cur, _) = incoming_row(c, &id)?;
                let tx = c.transaction()?;
                if let Some(n) = &req.name {
                    let n = n.trim();
                    if n.is_empty() || n.chars().count() > 80 {
                        return Err(AppError::validation("name must be 1-80 characters"));
                    }
                    tx.execute("UPDATE webhooks_in SET name=?2 WHERE id=?1", params![id, n])?;
                    tx.execute("UPDATE users SET display_name=?2 WHERE id=?1", params![cur.bot_user_id, n])?;
                }
                if let Some(e) = req.enabled {
                    tx.execute("UPDATE webhooks_in SET enabled=?2 WHERE id=?1", params![id, e as i64])?;
                }
                if req.regenerate_secret == Some(true) {
                    tx.execute("UPDATE webhooks_in SET secret=?2 WHERE id=?1", params![id, random_token("", 24)])?;
                }
                let mut em = Emitter::new();
                crate::users::announce_user(&tx, &hub, &cur.bot_user_id, &mut em)?;
                let out = incoming_row(&tx, &id)?.0;
                tx.commit()?;
                em.flush(&hub);
                Ok(out)
            })
            .await?,
    ))
}

pub async fn delete_incoming(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>) -> J<serde_json::Value> {
    let hub = st.hub.clone();
    st.db
        .call(move |c| {
            let (w, _) = incoming_row(c, &id)?;
            let tx = c.transaction()?;
            tx.execute("DELETE FROM webhooks_in WHERE id=?1", [&id])?;
            tx.execute("DELETE FROM members WHERE user_id=?1", [&w.bot_user_id])?;
            tx.execute("UPDATE users SET deleted=1, disabled=1 WHERE id=?1", [&w.bot_user_id])?;
            let mut em = Emitter::new();
            crate::routes::chat::upsert_for_members(&tx, &mut em, &w.conv_id)?;
            crate::users::announce_user(&tx, &hub, &w.bot_user_id, &mut em)?;
            tx.commit()?;
            em.flush(&hub);
            Ok(())
        })
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

fn check_events(events: &[String]) -> AppResult<()> {
    for e in events {
        if !crate::hooks::EVENTS.contains(&e.as_str()) {
            return Err(AppError::validation(format!("unknown event {e}; use one of {}", crate::hooks::EVENTS.join(", "))));
        }
    }
    Ok(())
}

pub async fn create_outgoing(State(st): State<St>, Admin(a): Admin, Json(req): Json<CreateOutgoingWebhook>) -> J<OutgoingWebhook> {
    crate::hooks::valid_url(&req.url)?;
    check_events(&req.events)?;
    let name = req.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(AppError::validation("name must be 1-80 characters"));
    }
    Ok(Json(
        st.db
            .call(move |c| {
                if let Some(cv) = &req.conv_id {
                    conv_row(c, cv)?;
                }
                let id = new_id();
                let secret = req.secret.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| random_token("whsec_", 24));
                c.execute(
                    "INSERT INTO webhooks_out(id,name,url,secret,events,conv_id,created_by,created_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![id, name, req.url.trim(), secret, serde_json::to_string(&req.events)?, req.conv_id, a.user_id, now_ms()],
                )?;
                outgoing_row(c, &id)
            })
            .await?,
    ))
}

pub async fn update_outgoing(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>, Json(req): Json<UpdateOutgoingWebhook>) -> J<OutgoingWebhook> {
    if let Some(u) = &req.url {
        crate::hooks::valid_url(u)?;
    }
    if let Some(e) = &req.events {
        check_events(e)?;
    }
    Ok(Json(
        st.db
            .call(move |c| {
                outgoing_row(c, &id)?;
                if let Some(n) = &req.name {
                    c.execute("UPDATE webhooks_out SET name=?2 WHERE id=?1", params![id, n.trim()])?;
                }
                if let Some(u) = &req.url {
                    c.execute("UPDATE webhooks_out SET url=?2 WHERE id=?1", params![id, u.trim()])?;
                }
                if let Some(e) = &req.events {
                    c.execute("UPDATE webhooks_out SET events=?2 WHERE id=?1", params![id, serde_json::to_string(e)?])?;
                }
                if let Some(cv) = &req.conv_id {
                    if cv.is_empty() {
                        c.execute("UPDATE webhooks_out SET conv_id=NULL WHERE id=?1", [&id])?;
                    } else {
                        conv_row(c, cv)?;
                        c.execute("UPDATE webhooks_out SET conv_id=?2 WHERE id=?1", params![id, cv])?;
                    }
                }
                if let Some(en) = req.enabled {
                    c.execute("UPDATE webhooks_out SET enabled=?2 WHERE id=?1", params![id, en as i64])?;
                }
                if req.regenerate_secret == Some(true) {
                    c.execute("UPDATE webhooks_out SET secret=?2 WHERE id=?1", params![id, random_token("whsec_", 24)])?;
                }
                outgoing_row(c, &id)
            })
            .await?,
    ))
}

pub async fn delete_outgoing(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>) -> J<serde_json::Value> {
    st.db
        .call(move |c| {
            outgoing_row(c, &id)?;
            c.execute("DELETE FROM webhooks_out WHERE id=?1", [&id])?;
            c.execute("DELETE FROM deliveries WHERE kind='hook' AND ref_id=?1", [&id])?;
            Ok(())
        })
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

/// Queues a `ping` delivery so an admin can check the receiver and its signature check.
pub async fn test_outgoing(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>) -> J<OutgoingWebhook> {
    let out = st
        .db
        .call(move |c| {
            let w = outgoing_row(c, &id)?;
            let body = serde_json::json!({"event": "ping", "ts": now_ms(), "webhook_id": w.id}).to_string();
            crate::hooks::enqueue(c, "hook", &w.id, &w.url, Some(&w.secret), "ping", &body)?;
            outgoing_row(c, &id)
        })
        .await?;
    crate::hooks::wake();
    Ok(Json(out))
}

// ------------------------------------------------------------------ AI settings

fn owner_only(a: &crate::auth::Auth) -> AppResult<()> {
    if a.role != Role::Owner {
        return Err(AppError::forbidden("only the owner can change server-wide settings"));
    }
    Ok(())
}

pub async fn get_ai(State(st): State<St>, Admin(_a): Admin) -> J<AiOverview> {
    Ok(Json(st.db.call(|c| Ok(crate::ai::load(c)?.view())).await?))
}

pub async fn add_ai_provider(State(st): State<St>, Admin(a): Admin, Json(req): Json<AiProviderInput>) -> J<AiProvider> {
    owner_only(&a)?;
    Ok(Json(
        st.db
            .call(move |c| {
                let (mut store, p) = crate::ai::add_provider(crate::ai::load(c)?, req)?;
                // The first provider becomes the default for both features.
                if store.providers.len() == 1 {
                    store.search_provider = Some(p.id.clone());
                    store.recap_provider = Some(p.id.clone());
                }
                crate::ai::save(c, &store)?;
                Ok(p.view())
            })
            .await?,
    ))
}

pub async fn update_ai_provider(State(st): State<St>, Admin(a): Admin, Path(id): Path<String>, Json(req): Json<AiProviderUpdate>) -> J<AiProvider> {
    owner_only(&a)?;
    Ok(Json(
        st.db
            .call(move |c| {
                let (store, p) = crate::ai::update_provider(crate::ai::load(c)?, &id, req)?;
                crate::ai::save(c, &store)?;
                Ok(p.view())
            })
            .await?,
    ))
}

pub async fn delete_ai_provider(State(st): State<St>, Admin(a): Admin, Path(id): Path<String>) -> J<AiOverview> {
    owner_only(&a)?;
    Ok(Json(
        st.db
            .call(move |c| {
                let store = crate::ai::delete_provider(crate::ai::load(c)?, &id)?;
                crate::ai::save(c, &store)?;
                Ok(store.view())
            })
            .await?,
    ))
}

pub async fn put_ai_defaults(State(st): State<St>, Admin(a): Admin, Json(req): Json<AiDefaultsUpdate>) -> J<AiOverview> {
    owner_only(&a)?;
    Ok(Json(
        st.db
            .call(move |c| {
                let store = crate::ai::set_defaults(crate::ai::load(c)?, req)?;
                crate::ai::save(c, &store)?;
                Ok(store.view())
            })
            .await?,
    ))
}

/// Sends a tiny request to exactly this provider (enabled or not) so the admin sees whether it works.
pub async fn test_ai_provider(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>) -> J<AiTestResponse> {
    let store = st.db.call(|c| crate::ai::load(c)).await?;
    let cfg = store.providers.iter().find(|p| p.id == id).cloned().ok_or_else(|| AppError::not_found("no such AI provider"))?;
    if cfg.api_key.is_empty() {
        return Ok(Json(AiTestResponse { ok: false, message: "enter an API key first".into() }));
    }
    Ok(Json(match crate::ai::complete(&st, &cfg, "Reply with the single word: ok", "ping", 16).await {
        Ok(t) => AiTestResponse { ok: true, message: format!("The provider answered: {}", t.chars().take(80).collect::<String>()) },
        Err(e) => AiTestResponse { ok: false, message: e.message },
    }))
}

// ------------------------------------------------------------------ scheduled (viewer)

#[derive(serde::Deserialize)]
pub struct ScheduledQ {
    conv_id: Option<String>,
    user_id: Option<String>,
}

/// Pending (and recent) scheduled messages of any user, for the chat viewer.
pub async fn admin_scheduled(State(st): State<St>, Admin(_a): Admin, axum::extract::Query(q): axum::extract::Query<ScheduledQ>) -> J<Vec<ScheduledMessage>> {
    Ok(Json(
        st.db
            .call(move |c| {
                let cutoff = now_ms() - 7 * 24 * 3600 * 1000;
                let mut stmt = c.prepare(
                    "SELECT id FROM scheduled WHERE (?1 IS NULL OR conv_id=?1) AND (?2 IS NULL OR user_id=?2) AND (state='pending' OR COALESCE(done_ms, created_ms)>?3) ORDER BY due_ms LIMIT 500",
                )?;
                let ids = stmt.query_map(params![q.conv_id, q.user_id, cutoff], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
                ids.iter().map(|i| crate::scheduled::get(c, i)).collect()
            })
            .await?,
    ))
}
