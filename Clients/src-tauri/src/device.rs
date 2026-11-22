//! Device-level features of the settings screens: storage, push registration, updates, the contact card.

use crate::api::{device_name, now_ms, CLIENT_VERSION};
use crate::engine::Engine;
use crate::error::{CoreError, Result};
use crate::update::{self, Platform, UpdateInfo};
use crate::view::CoreEvent;
use proto::*;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

fn dir_size(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten().map(|e| match e.metadata() {
        Ok(m) if m.is_dir() => dir_size(&e.path()),
        Ok(m) => m.len(),
        Err(_) => 0,
    }).sum()
}

fn clear_dir(p: &Path) {
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
            } else {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageInfo {
    pub database: u64,
    pub media: u64,
    pub other: u64,
    pub total: u64,
    pub files: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PushInfo {
    pub kind: String,
    pub target_host: String,
    pub registered_ms: i64,
    pub error: String,
}

impl Engine {
    pub fn storage_info(&self) -> StorageInfo {
        let db: u64 = ["cache.db", "cache.db-wal", "cache.db-shm"].iter().map(|n| std::fs::metadata(self.dir.join(n)).map(|m| m.len()).unwrap_or(0)).sum();
        let media = dir_size(&self.media_dir());
        let other = dir_size(&self.dir.join("open")) + dir_size(&self.dir.join("updates")) + dir_size(&self.dir.join("staging"));
        let files = std::fs::read_dir(self.media_dir()).map(|r| r.flatten().count() as u64).unwrap_or(0);
        StorageInfo { database: db, media, other, total: db + media + other, files }
    }

    /// Downloaded media and temporary copies go; messages (the database) and your session stay.
    pub fn clear_cache(&self) -> Result<StorageInfo> {
        // Files of unsent messages live in the media folder after upload: keep those.
        let keep: Vec<PathBuf> = self.store.outbox_all().unwrap_or_default().into_iter().flat_map(|o| o.attachments.into_iter().map(|a| PathBuf::from(a.path))).collect();
        if let Ok(rd) = std::fs::read_dir(self.media_dir()) {
            for e in rd.flatten() {
                if !keep.contains(&e.path()) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        self.store.media_clear()?;
        clear_dir(&self.dir.join("open"));
        clear_dir(&self.dir.join("updates"));
        Ok(self.storage_info())
    }

    pub fn compact_database(&self) -> Result<StorageInfo> {
        self.store.compact()?;
        Ok(self.storage_info())
    }

    pub fn media_cached(&self, ids: Vec<String>) -> Vec<String> {
        ids.into_iter().filter(|id| self.store.media_has(id)).collect()
    }

    // ---------------------------------------------------------------- push

    pub fn push_info(&self) -> PushInfo {
        self.store.kv_get("push").ok().flatten().unwrap_or_default()
    }

    /// `PUT /me/push`: tells the server where to send content-free wake-ups for this device.
    pub async fn register_push(&self, kind: &str, target: &str) -> Result<PushInfo> {
        if !matches!(kind, "unifiedpush" | "fcm" | "apns") || target.is_empty() {
            return Err(CoreError::local("Unsupported push registration."));
        }
        let reg = PushRegistration { id: String::new(), kind: kind.into(), target: target.into(), ntfy_url: None, device_name: device_name(), created_ms: 0 };
        let host = url::Url::parse(target).ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_default();
        let mut info = PushInfo { kind: kind.into(), target_host: host, registered_ms: now_ms(), error: String::new() };
        let r: Result<serde_json::Value> = self.api()?.json(Method::PUT, "/me/push", &[], Some(serde_json::to_value(&reg)?)).await;
        match r {
            Ok(_) => {
                let _ = self.store.kv_set("push_target", &(kind.to_string(), target.to_string()));
                let _ = self.store.kv_set("push", &info);
                Ok(info)
            }
            Err(e) => {
                info.error = e.to_string();
                let _ = self.store.kv_set("push", &info);
                Err(e)
            }
        }
    }

    /// Registers again with the saved target (after every sign-in the old registration is gone).
    pub async fn reregister_push(&self) {
        if let Ok(Some((kind, target))) = self.store.kv_get::<(String, String)>("push_target") {
            let _ = self.register_push(&kind, &target).await;
        }
    }

    pub async fn unregister_push(&self) -> Result<()> {
        let api = self.api()?;
        let list: Vec<PushRegistration> = api.get("/me/push", &[]).await.unwrap_or_default();
        let mine: Option<(String, String)> = self.store.kv_get("push_target").ok().flatten();
        for r in list.iter().filter(|r| mine.as_ref().map(|(k, t)| &r.kind == k && &r.target == t).unwrap_or(false)) {
            let _ = api.send(Method::DELETE, &format!("/me/push/{}", r.id), &[], None, &[]).await;
        }
        let _ = self.store.kv_set("push", &PushInfo::default());
        let _ = self.store.kv_set("push_target", &("".to_string(), "".to_string()));
        Ok(())
    }

    // ---------------------------------------------------------------- updates

    pub fn update_api(&self) -> String {
        update::api_url(self.store.kv_get::<String>("update_api").ok().flatten())
    }

    pub fn platform(&self) -> Platform {
        Platform::detect(self.portable)
    }

    pub async fn check_update(&self) -> Result<UpdateInfo> {
        let r = update::check(&self.update_api(), CLIENT_VERSION, &self.platform()).await;
        let _ = self.store.kv_set("update_checked_ms", &now_ms());
        match &r {
            Ok(i) => {
                let _ = self.store.kv_set("update_found", &if i.newer { Some(i.latest.clone()) } else { None });
            }
            Err(_) => {}
        }
        r
    }

    /// Latest version found by a check (the badge on Settings, About); `None` when up to date or never checked.
    pub fn update_found(&self) -> Option<String> {
        self.store.kv_get::<Option<String>>("update_found").ok().flatten().flatten().filter(|v| update::is_newer(v, CLIENT_VERSION))
    }

    /// True when the daily silent check is due.
    pub fn update_check_due(&self) -> bool {
        let last: i64 = self.store.kv_get("update_checked_ms").ok().flatten().unwrap_or(0);
        now_ms() - last > 24 * 3600 * 1000
    }

    pub fn set_update_api(&self, url: &str) -> Result<()> {
        self.store.kv_set("update_api", &url.trim().to_string())
    }

    pub fn updates_dir(&self) -> PathBuf {
        self.dir.join("updates")
    }

    /// Downloads the asset into the updates folder and returns its path; progress arrives as `Update` events.
    pub async fn download_update(&self, asset: &update::Asset, checksum_url: Option<String>) -> Result<PathBuf> {
        self.update_cancel.store(false, Ordering::SeqCst);
        let name = Path::new(&asset.name).file_name().and_then(|n| n.to_str()).ok_or_else(|| CoreError::local("Bad file name."))?.to_string();
        let dest = self.updates_dir().join(&name);
        let expected = match checksum_url {
            Some(u) => update::fetch_checksum(&u, &asset.name).await,
            None => None,
        };
        self.emit(CoreEvent::Update { state: "downloading".into(), got: 0, total: asset.size, message: String::new() });
        let emit = |got: u64, total: u64| self.emit(CoreEvent::Update { state: "downloading".into(), got, total: if total > 0 { total } else { asset.size }, message: String::new() });
        update::download(&asset.url, &dest, expected.as_deref(), &self.update_cancel, emit).await?;
        self.emit(CoreEvent::Update { state: "ready".into(), got: asset.size, total: asset.size, message: if expected.is_some() { "Checksum verified.".into() } else { String::new() } });
        Ok(dest)
    }
}

// ---------------------------------------------------------------- devices and sessions

impl Engine {
    /// `GET /sessions`: the account's sessions (this device first). Revoked ones are left out.
    pub async fn list_sessions(&self) -> Result<serde_json::Value> {
        let mut list: Vec<SessionInfo> = self.api()?.get("/sessions", &[]).await?;
        list.retain(|s| !s.revoked);
        list.sort_by(|a, b| b.current.cmp(&a.current).then(b.last_seen_ms.cmp(&a.last_seen_ms)));
        Ok(serde_json::to_value(list)?)
    }

    pub async fn revoke_session(&self, id: &str) -> Result<()> {
        let _ = self.api()?.send(Method::DELETE, &format!("/sessions/{id}"), &[], None, &[]).await?;
        Ok(())
    }

    pub async fn revoke_other_sessions(&self) -> Result<()> {
        let _ = self.api()?.send(Method::POST, "/sessions/revoke-others", &[], None, &[]).await?;
        Ok(())
    }
}

impl Engine {
    /// Unread messages in chats that notify (not muted, not hidden by the lock): the number on the tray icon.
    pub fn badge_count(&self) -> u32 {
        let hidden = self.hidden_ids();
        let prefs: std::collections::HashMap<String, ConvPref> = self.store.all_prefs().unwrap_or_default().into_iter().map(|p| (p.conv_id.clone(), p)).collect();
        let now = now_ms();
        self.store
            .all_convs()
            .unwrap_or_default()
            .iter()
            .filter(|cs| !hidden.contains(&cs.conversation.id) && !cs.conversation.archived && !prefs.get(&cs.conversation.id).map(|p| p.is_muted(now)).unwrap_or(false))
            .map(|cs| if cs.unread > 0 { cs.unread } else if cs.marked_unread { 1 } else { 0 })
            .sum::<i64>()
            .clamp(0, 999) as u32
    }
}
