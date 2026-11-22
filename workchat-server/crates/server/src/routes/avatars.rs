//! Pictures: channel and group pictures everyone sees, and private pictures a person chose for other users or
//! conversations (shown only to them).

use crate::auth::Auth;
use crate::db::now_ms;
use crate::emit::Emitter;
use crate::error::{AppError, AppResult};
use crate::queries::*;
use crate::routes::chat::upsert_for_members;
use crate::state::St;
use axum::extract::{Path, State};
use axum::Json;
use proto::*;
use rusqlite::{params, Connection, OptionalExtension};

type J<T> = AppResult<Json<T>>;

/// The caller's private pictures (also part of the bootstrap snapshot).
pub fn load_overrides(conn: &Connection, viewer: &str) -> AppResult<Vec<AvatarOverride>> {
    let mut st = conn.prepare("SELECT kind,target,attachment_id FROM avatar_overrides WHERE viewer_id=?1 ORDER BY updated_ms")?;
    let v = st.query_map([viewer], |r| Ok(AvatarOverride { kind: r.get(0)?, target: r.get(1)?, attachment: r.get(2)? }))?.collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

/// An uploaded, not removed image of the caller with the wanted purpose.
fn own_image(conn: &Connection, id: &str, uploader: &str, purpose: &str) -> AppResult<()> {
    let ok: i64 = conn.query_row(
        "SELECT COUNT(*) FROM attachments WHERE id=?1 AND uploader_id=?2 AND purpose=?3 AND removed=0",
        params![id, uploader, purpose],
        |r| r.get(0),
    )?;
    if ok == 0 {
        return Err(AppError::validation("unknown picture upload"));
    }
    Ok(())
}

pub async fn list_overrides(State(st): State<St>, auth: Auth) -> J<Vec<AvatarOverride>> {
    Ok(Json(st.db.call(move |c| load_overrides(c, &auth.user_id)).await?))
}

pub async fn set_override(State(st): State<St>, auth: Auth, Path((kind, target)): Path<(String, String)>, Json(req): Json<SetAvatarOverride>) -> J<AvatarOverride> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                check_target(c, &auth.user_id, &kind, &target)?;
                own_image(c, &req.attachment, &auth.user_id, "avatar_private")?;
                let tx = c.transaction()?;
                tx.execute(
                    "INSERT INTO avatar_overrides(viewer_id,kind,target,attachment_id,updated_ms) VALUES(?1,?2,?3,?4,?5)
                     ON CONFLICT(viewer_id,kind,target) DO UPDATE SET attachment_id=excluded.attachment_id, updated_ms=excluded.updated_ms",
                    params![auth.user_id, kind, target, req.attachment, now_ms()],
                )?;
                let mut em = Emitter::new();
                // Only the viewer's own devices ever hear about it.
                em.emit(&tx, &auth.user_id, EventData::AvatarOverrideUpdate { kind: kind.clone(), target: target.clone(), attachment: Some(req.attachment.clone()) })?;
                tx.commit()?;
                em.flush(&hub);
                Ok(AvatarOverride { kind, target, attachment: req.attachment })
            })
            .await?,
    ))
}

pub async fn delete_override(State(st): State<St>, auth: Auth, Path((kind, target)): Path<(String, String)>) -> J<serde_json::Value> {
    let hub = st.hub.clone();
    st.db
        .call(move |c| {
            if !matches!(kind.as_str(), "user" | "conv") {
                return Err(AppError::validation("kind must be user or conv"));
            }
            let tx = c.transaction()?;
            let n = tx.execute("DELETE FROM avatar_overrides WHERE viewer_id=?1 AND kind=?2 AND target=?3", params![auth.user_id, kind, target])?;
            if n > 0 {
                let mut em = Emitter::new();
                em.emit(&tx, &auth.user_id, EventData::AvatarOverrideUpdate { kind, target, attachment: None })?;
                tx.commit()?;
                em.flush(&hub);
            }
            Ok(())
        })
        .await?;
    Ok(Json(serde_json::json!({"ok": true})))
}

/// A private picture can be set for another user or for a conversation the caller is in.
fn check_target(conn: &Connection, viewer: &str, kind: &str, target: &str) -> AppResult<()> {
    match kind {
        "user" => {
            if target == viewer {
                return Err(AppError::validation("use your profile photo for yourself"));
            }
            let exists: Option<i64> = conn.query_row("SELECT 1 FROM users WHERE id=?1 AND deleted=0", [target], |r| r.get(0)).optional()?;
            if exists.is_none() {
                return Err(AppError::not_found("user not found"));
            }
        }
        "conv" => {
            member_conv(conn, target, viewer)?;
        }
        _ => return Err(AppError::validation("kind must be user or conv")),
    }
    Ok(())
}

/// Owners and admins set (or clear, with an empty string) the picture of a channel or group DM.
pub async fn set_conv_avatar(State(st): State<St>, auth: Auth, Path(id): Path<String>, Json(req): Json<SetConvAvatar>) -> J<ConversationSummary> {
    let hub = st.hub.clone();
    Ok(Json(
        st.db
            .call(move |c| {
                let cr = conv_row(c, &id)?;
                let role: Option<String> = c.query_row("SELECT role FROM members WHERE conv_id=?1 AND user_id=?2", params![id, auth.user_id], |r| r.get(0)).optional()?;
                let is_owner = role.as_deref() == Some("owner");
                if !auth.is_admin() && !is_owner {
                    return Err(if role.is_some() { AppError::forbidden("only owners and admins can change the picture") } else { AppError::not_found("conversation not found") });
                }
                if !matches!(cr.kind, ConversationKind::Channel | ConversationKind::GroupDm) {
                    return Err(AppError::validation("only channels and group DMs have a picture"));
                }
                let tx = c.transaction()?;
                if req.avatar.is_empty() {
                    tx.execute("UPDATE conversations SET avatar_attachment=NULL WHERE id=?1", [&id])?;
                } else {
                    own_image(&tx, &req.avatar, &auth.user_id, "avatar")?;
                    tx.execute("UPDATE conversations SET avatar_attachment=?2 WHERE id=?1", params![id, req.avatar])?;
                }
                let mut em = Emitter::new();
                upsert_for_members(&tx, &mut em, &id)?;
                tx.commit()?;
                em.flush(&hub);
                conv_summary(c, &id, &auth.user_id)
            })
            .await?,
    ))
}
