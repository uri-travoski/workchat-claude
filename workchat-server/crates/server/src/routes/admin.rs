//! Admin API (role owner or admin): users, conversations, sessions, storage, backups, files.

use crate::auth::*;
use crate::backup;
use crate::db::now_ms;
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::queries::*;
use crate::state::St;
use crate::storage::build_storage;
use crate::users::*;
use axum::extract::{Path, State};
use axum::http::request::Parts;
use axum::Json;
use proto::*;
use rusqlite::{params, Connection, OptionalExtension};

type J<T> = AppResult<Json<T>>;

pub(crate) async fn public_base(st: &St, parts: &Parts) -> String {
    public_base_src(st, parts).await.0
}

/// The base URL for links plus where it came from: "env", "setting" or "request".
pub(crate) async fn public_base_src(st: &St, parts: &Parts) -> (String, &'static str) {
    if let Some(u) = &st.cfg.public_url {
        return (u.clone(), "env");
    }
    if let Some(u) = st.db.call(|c| crate::db::get_setting(c, "public_url")).await.ok().flatten().filter(|u| !u.is_empty()) {
        return (u, "setting");
    }
    (request_base(parts), "request")
}

fn request_base(parts: &Parts) -> String {
    let h = |n: &str| parts.headers.get(n).and_then(|v| v.to_str().ok()).map(|s| s.split(',').next().unwrap_or("").trim().to_string());
    let host = h("x-forwarded-host").or_else(|| h("host")).unwrap_or_else(|| "localhost".into());
    let scheme = h("x-forwarded-proto").unwrap_or_else(|| "http".into());
    format!("{scheme}://{host}")
}

fn create_link(conn: &Connection, base: &str, user_id: &str) -> AppResult<LoginLink> {
    let token = make_link_token();
    let now = now_ms();
    let exp = now + 7 * 24 * 3600 * 1000;
    // Only one live link per user.
    conn.execute("DELETE FROM login_links WHERE user_id=?1 AND used_ms IS NULL", [user_id])?;
    conn.execute(
        "INSERT INTO login_links(token_hash,user_id,created_ms,expires_ms) VALUES(?1,?2,?3,?4)",
        params![sha256_hex(&token), user_id, now, exp],
    )?;
    Ok(LoginLink { link: rules::make_login_link(base, &token), token, expires_ms: exp })
}

fn target_user(conn: &Connection, actor: &Auth, id: &str) -> AppResult<UserRow> {
    let u = get_user(conn, id)?;
    if u.role == Role::Owner && actor.role != Role::Owner {
        return Err(AppError::forbidden("only the owner can manage the owner account"));
    }
    Ok(u)
}

pub async fn list_users(State(st): State<St>, Admin(_a): Admin) -> J<Vec<AdminUser>> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let mut stmt = c.prepare(&format!("SELECT {USER_COLS} FROM users ORDER BY deleted, display_name COLLATE NOCASE"))?;
                let rows = stmt.query_map([], user_row)?.collect::<Result<Vec<_>, _>>()?;
                rows.iter().map(|r| admin_user(c, &hub, r)).collect()
            })
            .await?,
    ))
}

pub async fn create_user_route(State(st): State<St>, Admin(a): Admin, parts: Parts, Json(req): Json<CreateUserRequest>) -> J<CreateUserResponse> {
    let role = req.role.unwrap_or(Role::Member);
    if role == Role::Owner {
        return Err(AppError::validation("there is exactly one owner"));
    }
    if role == Role::Admin && a.role != Role::Owner {
        return Err(AppError::forbidden("only the owner can create admins"));
    }
    let has_pw = req.password.as_ref().map(|p| !p.is_empty()).unwrap_or(false);
    let pw = match &req.password {
        Some(p) if !p.is_empty() => {
            validate_password(p)?;
            p.clone()
        }
        _ => random_token("", 24),
    };
    let want_link = req.create_login_link.unwrap_or(!has_pw);
    let hash = hash_pw_async(pw).await?;
    let base = public_base(&st, &parts).await;
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let tx = c.transaction()?;
                let mut em = Emitter::new();
                let row = create_user(
                    &tx,
                    &hub,
                    NewUser {
                        username: req.username.trim().to_string(),
                        display_name: req.display_name.clone(),
                        password_hash: hash,
                        password_set: has_pw,
                        role,
                        email: req.email.clone(),
                        title: req.title.clone(),
                        phone: req.phone.clone(),
                    },
                    &mut em,
                )?;
                let link = if want_link { Some(create_link(&tx, &base, &row.id)?) } else { None };
                tx.commit()?;
                em.flush(&hub);
                Ok(CreateUserResponse { user: admin_user(c, &hub, &row)?, login_link: link })
            })
            .await?,
    ))
}

pub async fn get_user_route(State(st): State<St>, Admin(a): Admin, Path(id): Path<String>) -> J<AdminUser> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| admin_user(c, &hub, &target_user(c, &a, &id)?)).await?))
}

pub async fn update_user(State(st): State<St>, Admin(a): Admin, Path(id): Path<String>, Json(req): Json<UpdateUserRequest>) -> J<AdminUser> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let u = target_user(c, &a, &id)?;
                if u.deleted {
                    return Err(AppError::validation("account was deleted"));
                }
                let tx = c.transaction()?;
                if let Some(n) = &req.display_name {
                    let n = n.trim();
                    if n.is_empty() || n.chars().count() > 80 {
                        return Err(AppError::validation("display name must be 1-80 characters"));
                    }
                    tx.execute("UPDATE users SET display_name=?2 WHERE id=?1", params![id, n])?;
                }
                if let Some(e) = &req.email {
                    tx.execute("UPDATE users SET email=?2 WHERE id=?1", params![id, e])?;
                }
                if req.title.is_some() {
                    tx.execute("UPDATE users SET title=?2 WHERE id=?1", params![id, profile_field(&req.title, 80, "job title")?])?;
                }
                if req.phone.is_some() {
                    tx.execute("UPDATE users SET phone=?2 WHERE id=?1", params![id, profile_field(&req.phone, 40, "phone number")?])?;
                }
                if let Some(r) = req.role {
                    apply_role_change(&tx, &a, &u, r)?;
                }
                if let Some(p) = req.privacy_controls_enabled {
                    tx.execute("UPDATE users SET privacy_controls_enabled=?2 WHERE id=?1", params![id, p as i64])?;
                    if !p {
                        tx.execute("UPDATE users SET hide_last_active=0 WHERE id=?1", [&id])?;
                    }
                }
                if let Some(d) = req.disabled {
                    if d && (id == a.user_id || u.role == Role::Owner) {
                        return Err(AppError::validation("this account cannot be disabled"));
                    }
                    tx.execute("UPDATE users SET disabled=?2 WHERE id=?1", params![id, d as i64])?;
                    if d {
                        // Immediate kick: all sessions end, live sockets are closed with account_disabled.
                        revoke_sessions(&tx, &hub, &id, "account_disabled", None)?;
                    }
                }
                let mut em = Emitter::new();
                announce_user(&tx, &hub, &id, &mut em)?;
                let row = get_user(&tx, &id)?;
                if !row.disabled {
                    em.emit(&tx, &id, EventData::MeUpdate { me: to_me(&row, &hub) })?;
                }
                tx.commit()?;
                em.flush(&hub);
                crate::routes::ws::broadcast_presence_hub(c, &hub, &id)?;
                admin_user(c, &hub, &get_user(c, &id)?)
            })
            .await?,
    ))
}

/// The pure rules for a role change (PRD section 3). `other_owners` counts owners besides the target.
pub(crate) fn check_role_change(actor_role: Role, actor_id: &str, target: &UserRow, new: Role, other_owners: i64) -> AppResult<()> {
    if actor_role != Role::Owner {
        return Err(AppError::forbidden("only the owner can change roles"));
    }
    if target.id == actor_id {
        return Err(AppError::forbidden("you cannot change your own role"));
    }
    if target.deleted {
        return Err(AppError::validation("account was deleted"));
    }
    if target.is_bot && new != Role::Member {
        return Err(AppError::validation("a bot always has the member role"));
    }
    if target.role == Role::Owner && new != Role::Owner && other_owners < 1 {
        return Err(AppError::validation("the last owner cannot be demoted; make someone else an owner first"));
    }
    Ok(())
}

/// Validates and applies a role change inside `tx`. The new role applies to the user's live sessions at once
/// (the role is read on every request); the caller emits the events.
fn apply_role_change(tx: &Connection, actor: &Auth, target: &UserRow, new: Role) -> AppResult<()> {
    let other_owners: i64 = tx.query_row("SELECT COUNT(*) FROM users WHERE role='owner' AND deleted=0 AND id!=?1", [&target.id], |r| r.get(0))?;
    check_role_change(actor.role, &actor.user_id, target, new, other_owners)?;
    if target.role != new {
        tx.execute("UPDATE users SET role=?2 WHERE id=?1", params![target.id, new.as_str()])?;
    }
    Ok(())
}

pub async fn set_role(State(st): State<St>, Admin(a): Admin, Path(id): Path<String>, Json(req): Json<SetRoleRequest>) -> J<AdminUser> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let u = get_user(c, &id)?;
                let tx = c.transaction()?;
                apply_role_change(&tx, &a, &u, req.role)?;
                if u.role != req.role {
                    let mut em = Emitter::new();
                    announce_user(&tx, &hub, &id, &mut em)?;
                    let row = get_user(&tx, &id)?;
                    if !row.disabled && !row.deleted {
                        em.emit(&tx, &id, EventData::MeUpdate { me: to_me(&row, &hub) })?;
                    }
                    tx.commit()?;
                    em.flush(&hub);
                } else {
                    tx.commit()?;
                }
                admin_user(c, &hub, &get_user(c, &id)?)
            })
            .await?,
    ))
}

pub async fn reset_password(State(st): State<St>, Admin(a): Admin, parts: Parts, Path(id): Path<String>, Json(req): Json<ResetPasswordRequest>) -> J<ResetPasswordResponse> {
    let given = match &req.new_password {
        Some(p) if !p.is_empty() => {
            validate_password(p)?;
            Some(p.clone())
        }
        _ => None,
    };
    let hash = hash_pw_async(given.clone().unwrap_or_else(|| random_token("", 24))).await?;
    let base = public_base(&st, &parts).await;
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let u = target_user(c, &a, &id)?;
                if u.deleted {
                    return Err(AppError::validation("account was deleted"));
                }
                c.execute(
                    "UPDATE users SET password_hash=?2, password_set=?3 WHERE id=?1",
                    params![id, hash, given.is_some() as i64],
                )?;
                // Forced sign-out: every device gets password_reset and wipes local data.
                revoke_sessions(c, &hub, &id, "password_reset", None)?;
                let link = if given.is_none() { Some(create_link(c, &base, &id)?) } else { None };
                Ok(ResetPasswordResponse { login_link: link })
            })
            .await?,
    ))
}

pub async fn new_login_link(State(st): State<St>, Admin(a): Admin, parts: Parts, Path(id): Path<String>) -> J<LoginLink> {
    let base = public_base(&st, &parts).await;
    Ok(Json(
        st.db
            .call(move |c| {
                let u = target_user(c, &a, &id)?;
                if u.deleted || u.disabled {
                    return Err(AppError::validation("account is disabled or deleted"));
                }
                create_link(c, &base, &id)
            })
            .await?,
    ))
}

pub async fn revoke_user_sessions(State(st): State<St>, Admin(a): Admin, Path(id): Path<String>) -> J<serde_json::Value> {
    let hub = st.hub.clone();
    let n = st
        .db
        .call(move |c| {
            target_user(c, &a, &id)?;
            revoke_sessions(c, &hub, &id, "session_revoked", None)
        })
        .await?;
    Ok(Json(serde_json::json!({"revoked": n})))
}

pub async fn clear_history_route(State(st): State<St>, Admin(a): Admin, Path(id): Path<String>) -> J<ClearHistoryResponse> {
    let id2 = id.clone();
    st.db.call(move |c| target_user(c, &a, &id2).map(|_| ())).await?;
    Ok(Json(backup::clear_history(&st, &id).await?))
}

pub async fn delete_user_route(State(st): State<St>, Admin(a): Admin, Path(id): Path<String>) -> J<ClearHistoryResponse> {
    let (id2, a2) = (id.clone(), a.clone());
    let u = st.db.call(move |c| target_user(c, &a2, &id2)).await?;
    if id == a.user_id || u.role == Role::Owner {
        return Err(AppError::validation("this account cannot be deleted"));
    }
    if u.deleted {
        return Err(AppError::validation("account already deleted"));
    }
    Ok(Json(backup::delete_user(&st, &id).await?))
}

pub async fn backup_user_route(State(st): State<St>, Admin(a): Admin, Path(id): Path<String>) -> J<BackupInfo> {
    let id2 = id.clone();
    st.db.call(move |c| target_user(c, &a, &id2).map(|_| ())).await?;
    Ok(Json(backup::backup_user(&st, &id, "manual").await?.info))
}

pub async fn delete_range(State(st): State<St>, Admin(a): Admin, Path(id): Path<String>, Json(req): Json<DateRangeDeleteRequest>) -> J<ClearHistoryResponse> {
    // "Delete before date" = everything strictly older than `before_ms`.
    let (from_ms, to_ms) = match req.before_ms {
        Some(b) => (0, b - 1),
        None => (req.from_ms, req.to_ms),
    };
    if to_ms < from_ms {
        return Err(AppError::validation("the end date is before the start date"));
    }
    let id2 = id.clone();
    st.db.call(move |c| target_user(c, &a, &id2).map(|_| ())).await?;
    let (b, out) = backup::hide_with_backup(
        &st,
        backup::HideRequest {
            user_id: id,
            range: Some((from_ms, to_ms)),
            conversation_id: req.conversation_id.clone(),
            also_others: req.also_other_copies,
            message_ids: None,
        },
        "date_range_delete",
    )
    .await?;
    Ok(Json(ClearHistoryResponse {
        backup_id: b.info.id,
        backup_key: b.info.key,
        messages_hidden: out.messages_hidden,
        attachments_moved: out.attachments_moved,
        sessions_revoked: 0,
    }))
}

pub async fn list_backups(State(st): State<St>, Admin(_a): Admin) -> J<Vec<BackupInfo>> {
    Ok(Json(
        st.db
            .call(|c| {
                let mut s = c.prepare("SELECT id,kind,user_id,created_ms,reason,key,size,attachments FROM backups ORDER BY created_ms DESC")?;
                let v = s
                    .query_map([], |r| {
                        Ok(BackupInfo { id: r.get(0)?, kind: r.get(1)?, user_id: r.get(2)?, created_ms: r.get(3)?, reason: r.get(4)?, key: r.get(5)?, size: r.get(6)?, attachments: r.get(7)?, encrypted: false })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(v.into_iter().map(|mut b: BackupInfo| { b.encrypted = b.key.ends_with(".age"); b }).collect())
            })
            .await?,
    ))
}

pub async fn all_sessions(State(st): State<St>, Admin(a): Admin) -> J<Vec<SessionInfo>> {
    Ok(Json(
        st.db
            .call(move |c| {
                let mut s = c.prepare(
                    "SELECT s.id,s.user_id,u.username,s.device_name,s.client_version,s.ip,s.created_ms,s.last_seen_ms,s.revoked FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.revoked=0 ORDER BY s.last_seen_ms DESC LIMIT 500",
                )?;
                let v = s
                    .query_map([], |r| {
                        let id: String = r.get(0)?;
                        Ok(SessionInfo {
                            current: id == a.session_id,
                            id,
                            user_id: r.get(1)?,
                            username: r.get(2)?,
                            device_name: r.get(3)?,
                            client_version: r.get(4)?,
                            ip: r.get(5)?,
                            created_ms: r.get(6)?,
                            last_seen_ms: r.get(7)?,
                            revoked: r.get::<_, i64>(8)? != 0,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(v)
            })
            .await?,
    ))
}

// ---------------------------------------------------------------- conversations

pub async fn list_conversations(State(st): State<St>, Admin(_a): Admin) -> J<Vec<AdminConversation>> {
    Ok(Json(
        st.db
            .call(|c| {
                let ids: Vec<String> = {
                    let mut s = c.prepare("SELECT id FROM conversations WHERE kind IN ('channel','group_dm') ORDER BY COALESCE(last_ts, created_ms) DESC")?;
                    let v = s.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
                    v
                };
                ids.into_iter()
                    .map(|id| {
                        let conversation = load_conversation(c, &id)?;
                        let message_count: i64 = c.query_row("SELECT COUNT(*) FROM messages WHERE conv_id=?1", [&id], |r| r.get(0))?;
                        Ok(AdminConversation { conversation, message_count })
                    })
                    .collect()
            })
            .await?,
    ))
}

pub async fn add_members(State(st): State<St>, Admin(auth): Admin, Path(id): Path<String>, Json(req): Json<MembersRequest>) -> J<Conversation> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| crate::routes::social::add_members_tx(c, &hub, &auth, &id, &req.user_ids)).await?))
}

pub async fn remove_member(State(st): State<St>, Admin(auth): Admin, Path((id, uid)): Path<(String, String)>) -> J<Conversation> {
    let hub = st.hub.clone();
    Ok(Json(st.db.call(move |c| crate::routes::social::remove_member_tx(c, &hub, &auth, &id, &uid)).await?))
}

pub async fn delete_conversation(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>) -> J<serde_json::Value> {
    let hub = st.hub.clone();
    let keys: Vec<String> = st
        .db
        .call(move |c| {
            let cr = conv_row(c, &id)?;
            if !matches!(cr.kind, ConversationKind::Channel | ConversationKind::GroupDm) {
                return Err(AppError::validation("only channels and group DMs can be deleted"));
            }
            let tx = c.transaction()?;
            let members = member_ids(&tx, &id)?;
            let mut keys = vec![];
            let mids: Vec<String> = {
                let mut s = tx.prepare("SELECT id FROM messages WHERE conv_id=?1")?;
                let v = s.query_map([&id], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
                v
            };
            for m in &mids {
                keys.extend(crate::routes::chat::release_attachments(&tx, m)?);
                tx.execute("DELETE FROM reactions WHERE message_id=?1", [m])?;
                tx.execute("DELETE FROM message_edits WHERE message_id=?1", [m])?;
                tx.execute("DELETE FROM message_hidden WHERE message_id=?1", [m])?;
            }
            tx.execute("DELETE FROM messages WHERE conv_id=?1", [&id])?;
            tx.execute("DELETE FROM members WHERE conv_id=?1", [&id])?;
            tx.execute("DELETE FROM read_state WHERE conv_id=?1", [&id])?;
            // Everything else that points at the conversation goes with it (C-06): pins, bookmarks, followed threads,
            // per-user prefs, drafts, scheduled messages, saved items, webhooks.
            tx.execute("DELETE FROM saved_labels WHERE message_id IN (SELECT message_id FROM saved_items WHERE conv_id=?1)", [&id])?;
            for t in ["pins", "bookmarks", "thread_follows", "conv_prefs", "drafts", "scheduled", "saved_items", "webhooks_in", "webhooks_out"] {
                tx.execute(&format!("DELETE FROM {t} WHERE conv_id=?1"), [&id])?;
            }
            tx.execute("DELETE FROM avatar_overrides WHERE kind='conv' AND target=?1", [&id])?;
            tx.execute("DELETE FROM conversations WHERE id=?1", [&id])?;
            let mut em = Emitter::new();
            for m in members {
                em.emit(&tx, &m, EventData::ConversationRemove { conv_id: id.clone() })?;
            }
            tx.commit()?;
            em.flush(&hub);
            Ok(keys)
        })
        .await?;
    let storage = st.storage();
    for k in keys {
        let _ = storage.delete(&k).await;
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

/// F-07: admin removes a file; the message shows "File removed".
pub async fn remove_file(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>) -> J<serde_json::Value> {
    let hub = st.hub.clone();
    let id2 = id.clone();
    let key: String = st
        .db
        .call(move |c| {
            let key: String = c
                .query_row("SELECT storage_key FROM attachments WHERE id=?1 AND removed=0", [&id2], |r| r.get(0))
                .optional()?
                .ok_or_else(|| AppError::not_found("file not found"))?;
            let tx = c.transaction()?;
            tx.execute("UPDATE attachments SET removed=1 WHERE id=?1", [&id2])?;
            let mut users: Vec<String> = vec![];
            {
                let mut s = tx.prepare(
                    "SELECT DISTINCT mem.user_id FROM message_attachments ma JOIN messages m ON m.id=ma.message_id JOIN members mem ON mem.conv_id=m.conv_id WHERE ma.attachment_id=?1",
                )?;
                for r in s.query_map([&id2], |r| r.get::<_, String>(0))? {
                    users.push(r?);
                }
            }
            let mut em = Emitter::new();
            for u in users {
                em.emit(&tx, &u, EventData::AttachmentRemoved { attachment_id: id2.clone() })?;
            }
            tx.commit()?;
            em.flush(&hub);
            Ok(key)
        })
        .await?;
    let _ = st.storage().delete(&key).await;
    // F-03: a removed file leaves no preview behind either.
    let _ = st.storage().delete(&format!("thumb/{id}.png")).await;
    Ok(Json(serde_json::json!({"ok": true})))
}

// ---------------------------------------------------------------- storage

pub async fn get_storage(State(st): State<St>, Admin(_a): Admin) -> J<StorageInfo> {
    let (attachments, bytes): (i64, i64) = st
        .db
        .call(|c| Ok(c.query_row("SELECT COUNT(*), COALESCE(SUM(size),0) FROM attachments WHERE removed=0", [], |r| Ok((r.get(0)?, r.get(1)?)))?))
        .await?;
    Ok(Json(StorageInfo { config: st.storage_config().masked(), attachments, bytes }))
}

pub async fn test_storage(State(st): State<St>, Admin(_a): Admin, Json(cfg): Json<StorageConfig>) -> J<StorageTestResponse> {
    let cfg = cfg.unmask_from(&st.storage_config());
    let res = match build_storage(&cfg, &st.cfg.data_dir) {
        Ok(s) => s.check().await,
        Err(e) => Err(e),
    };
    Ok(Json(match res {
        Ok(m) => StorageTestResponse { ok: true, message: m },
        Err(e) => StorageTestResponse { ok: false, message: e.message },
    }))
}

/// Initial choice / change of backend; refused when files exist (use migrate then).
pub async fn put_storage(State(st): State<St>, Admin(a): Admin, Json(cfg): Json<StorageConfig>) -> J<StorageInfo> {
    if a.role != Role::Owner {
        return Err(AppError::forbidden("only the owner can change server-wide settings"));
    }
    let cfg = cfg.unmask_from(&st.storage_config());
    let n: i64 = st.db.call(|c| Ok(c.query_row("SELECT COUNT(*) FROM attachments", [], |r| r.get(0))?)).await?;
    if n > 0 && cfg != st.storage_config() {
        return Err(AppError::conflict("files already exist; use 'move all files to another storage' instead"));
    }
    let storage = build_storage(&cfg, &st.cfg.data_dir)?;
    storage.check().await?;
    crate::admin::set_storage(&st, cfg, storage).await?;
    get_storage(State(st), Admin(a)).await
}

pub async fn migrate_storage(State(st): State<St>, Admin(a): Admin, Json(req): Json<StorageMigrateRequest>) -> J<JobStatus> {
    if a.role != Role::Owner {
        return Err(AppError::forbidden("only the owner can change server-wide settings"));
    }
    let target = req.target.unmask_from(&st.storage_config());
    let id = crate::admin::start_migration(&st, target, req.delete_source).await?;
    job(State(st), Admin(a), Path(id)).await
}

pub async fn verify_storage(State(st): State<St>, Admin(a): Admin, Json(req): Json<StorageVerifyRequest>) -> J<JobStatus> {
    let id = crate::admin::start_verify(&st, req.deep).await?;
    job(State(st), Admin(a), Path(id)).await
}

pub async fn job(State(st): State<St>, Admin(_a): Admin, Path(id): Path<String>) -> J<JobStatus> {
    st.jobs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| AppError::not_found("job not found"))
}

pub async fn overview(State(st): State<St>, Admin(_a): Admin) -> J<serde_json::Value> {
    let online = st.hub.online_count();
    let v = st
        .db
        .call(|c| {
            let q = |sql: &str| -> AppResult<i64> { Ok(c.query_row(sql, [], |r| r.get(0))?) };
            Ok(serde_json::json!({
                "users": q("SELECT COUNT(*) FROM users WHERE deleted=0")?,
                "disabled_users": q("SELECT COUNT(*) FROM users WHERE deleted=0 AND disabled=1")?,
                "conversations": q("SELECT COUNT(*) FROM conversations WHERE kind IN ('channel','group_dm','dm')")?,
                "messages": q("SELECT COUNT(*) FROM messages")?,
                "attachments": q("SELECT COUNT(*) FROM attachments WHERE removed=0")?,
                "attachment_bytes": q("SELECT COALESCE(SUM(size),0) FROM attachments WHERE removed=0")?,
                "sessions": q("SELECT COUNT(*) FROM sessions WHERE revoked=0")?,
            }))
        })
        .await?;
    let mut v = v;
    v["online"] = online.into();
    v["server_version"] = VERSION.into();
    v["storage"] = st.storage_config().kind().into();
    Ok(Json(v))
}

// ---------------------------------------------------------------------- updates (GitHub releases)

#[derive(serde::Deserialize)]
pub struct UpdatesParams {
    #[serde(default)]
    refresh: bool,
}

/// How long a good answer is reused; a failed check is retried after five minutes; "Check again" is honoured after a minute.
const UPDATES_TTL_MS: i64 = 3_600_000;
const UPDATES_ERROR_TTL_MS: i64 = 300_000;
const UPDATES_REFRESH_GAP_MS: i64 = 60_000;

/// `GET /admin/updates`: this server's version against the newest GitHub release. The server asks GitHub (the browser never
/// does, so the admin pages need no relaxed Content-Security-Policy) and caches the answer for an hour. Only
/// `User-Agent: WorkChat-Server/<version>` is sent.
pub async fn get_updates(State(st): State<St>, Admin(_a): Admin, axum::extract::Query(p): axum::extract::Query<UpdatesParams>) -> J<AdminUpdates> {
    let mut cache = st.updates.lock().await;
    let now = now_ms();
    if let Some(c) = cache.as_ref() {
        let age = now - c.checked_ms;
        let ttl = if p.refresh { UPDATES_REFRESH_GAP_MS } else if c.error.is_some() { UPDATES_ERROR_TTL_MS } else { UPDATES_TTL_MS };
        if age < ttl && c.current == VERSION {
            return Ok(Json(c.clone()));
        }
    }
    let fresh = check_updates(&st).await;
    *cache = Some(fresh.clone());
    Ok(Json(fresh))
}

async fn check_updates(st: &St) -> AdminUpdates {
    let url = st.cfg.update_url.clone();
    let mut out = AdminUpdates { current: VERSION.into(), latest: None, update_available: false, checked_ms: now_ms(), error: None, source: url.clone(), release_url: st.cfg.release_url.clone() };
    let res = async {
        let resp = st
            .http
            .get(&url)
            .header("User-Agent", format!("WorkChat-Server/{VERSION}"))
            .header("Accept", "application/vnd.github+json")
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|_| "The server could not reach GitHub (is outbound internet blocked?).".to_string())?;
        let code = resp.status().as_u16();
        if code == 403 || code == 429 {
            return Err("GitHub is limiting update checks from this server's address. Try again later.".to_string());
        }
        if code == 404 {
            return Err("No release has been published yet.".to_string());
        }
        if !resp.status().is_success() {
            return Err(format!("GitHub answered HTTP {code}."));
        }
        let text = resp.text().await.map_err(|_| "The answer from GitHub could not be read.".to_string())?;
        proto::release::parse_releases(&text).map_err(|_| "The release information from GitHub could not be read.".to_string())
    }
    .await;
    match res {
        Ok(rel) => {
            out.update_available = rel.as_ref().map(|r| proto::release::is_newer(&r.version, VERSION)).unwrap_or(false);
            out.latest = rel;
        }
        Err(e) => out.error = Some(e),
    }
    out
}

// ---------------------------------------------------------------------- public URL (login links)

pub async fn get_public_url(State(st): State<St>, Admin(_a): Admin, parts: Parts) -> J<PublicUrlSettings> {
    let saved = st.db.call(|c| crate::db::get_setting(c, "public_url")).await?.filter(|u| !u.is_empty());
    let (effective, source) = public_base_src(&st, &parts).await;
    Ok(Json(PublicUrlSettings { url: saved, effective, source: source.into() }))
}

pub async fn put_public_url(State(st): State<St>, Admin(a): Admin, parts: Parts, Json(req): Json<PublicUrlUpdate>) -> J<PublicUrlSettings> {
    if a.role != Role::Owner {
        return Err(AppError::forbidden("only the owner can change the public address"));
    }
    let v = req.url.unwrap_or_default().trim().trim_end_matches('/').to_string();
    if !v.is_empty() {
        let ok = url::Url::parse(&v).map(|u| (u.scheme() == "https" || u.scheme() == "http") && u.has_host() && u.path() == "/" && u.query().is_none()).unwrap_or(false);
        if !ok {
            return Err(AppError::validation("enter an address like https://chat.example.com or http://192.168.1.10:8080"));
        }
    }
    st.db.call(move |c| crate::db::set_setting(c, "public_url", &v)).await?;
    get_public_url(State(st), Admin(a), parts).await
}

#[cfg(test)]
mod role_rule_tests {
    use super::*;

    fn row(id: &str, role: Role, bot: bool) -> UserRow {
        UserRow {
            id: id.into(), username: id.into(), display_name: id.into(), role, disabled: false, deleted: false, is_bot: bot,
            hide_last_active: false, privacy_controls_enabled: false, presence_pref: "auto".into(), last_active_ms: None,
            avatar: None, email: None, password_set: true, created_ms: 0, title: None, phone: None, read_receipts: true,
        }
    }

    #[test]
    fn rules() {
        let m = row("m", Role::Member, false);
        // only owners change roles
        assert_eq!(check_role_change(Role::Admin, "a", &m, Role::Admin, 1).unwrap_err().code, ErrorCode::Forbidden);
        assert_eq!(check_role_change(Role::Member, "a", &m, Role::Admin, 1).unwrap_err().code, ErrorCode::Forbidden);
        assert!(check_role_change(Role::Owner, "o", &m, Role::Owner, 0).is_ok());
        // never your own role
        let me = row("o", Role::Owner, false);
        assert_eq!(check_role_change(Role::Owner, "o", &me, Role::Admin, 1).unwrap_err().code, ErrorCode::Forbidden);
        // the last owner cannot be demoted, another owner can
        let o2 = row("o2", Role::Owner, false);
        assert_eq!(check_role_change(Role::Owner, "o", &o2, Role::Member, 0).unwrap_err().code, ErrorCode::Validation);
        assert!(check_role_change(Role::Owner, "o", &o2, Role::Admin, 1).is_ok());
        assert!(check_role_change(Role::Owner, "o", &o2, Role::Owner, 0).is_ok());
        // bots stay members
        let bot = row("b", Role::Member, true);
        assert_eq!(check_role_change(Role::Owner, "o", &bot, Role::Admin, 1).unwrap_err().code, ErrorCode::Validation);
        assert!(check_role_change(Role::Owner, "o", &bot, Role::Member, 1).is_ok());
        // deleted accounts
        let mut d = row("d", Role::Member, false);
        d.deleted = true;
        assert!(check_role_change(Role::Owner, "o", &d, Role::Admin, 1).is_err());
    }
}
