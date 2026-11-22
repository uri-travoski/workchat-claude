//! WorkChat core: networking, storage and sync in Rust, exposed to the Svelte UI as Tauri commands and events.

pub mod api;
pub mod audio;
pub mod engine;
pub mod error;
pub mod device;
pub mod features;
pub mod lock;
pub mod markup;
pub mod media;
pub mod mediainfo;
pub mod notify;
pub mod openfile;
pub mod portable;
pub mod player;
pub mod store;
pub mod sync;
pub mod update;
pub mod view;
pub mod voice;
#[cfg(target_os = "android")]
pub mod android;
#[cfg(desktop)]
mod desktop;

use engine::{AppInfo, Engine, Page, ServerCheck};
use error::{CoreError, Result};
use proto::*;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use features::{Staged, StagedMeta};
use tauri::AppHandle;
use view::{ChatRow, MessageView, Snapshot};

type Eng<'a> = State<'a, Arc<Engine>>;

#[tauri::command]
fn app_info(e: Eng) -> AppInfo {
    e.app_info()
}

#[tauri::command]
async fn check_server(e: Eng<'_>, server: String) -> Result<ServerCheck> {
    e.check_server(&server).await
}

#[tauri::command]
async fn login_password(e: Eng<'_>, server: String, username: String, password: String) -> Result<Me> {
    e.inner().clone().login_password(&server, &username, &password).await
}

#[tauri::command]
async fn login_link(e: Eng<'_>, link: String, server: Option<String>) -> Result<Me> {
    e.inner().clone().login_link(&link, server.as_deref()).await
}

#[tauri::command]
async fn sign_out(e: Eng<'_>) -> Result<()> {
    e.inner().clone().sign_out().await;
    Ok(())
}

#[tauri::command]
fn get_snapshot(e: Eng) -> Result<Snapshot> {
    e.snapshot()
}

#[tauri::command]
async fn open_conversation(e: Eng<'_>, conv_id: String, limit: Option<i64>) -> Result<Page> {
    e.open_conversation(&conv_id, limit.unwrap_or(50).clamp(10, 200)).await
}

#[tauri::command]
async fn load_older(e: Eng<'_>, conv_id: String, before_seq: i64, limit: Option<i64>) -> Result<Page> {
    e.load_older(&conv_id, before_seq, limit.unwrap_or(50).clamp(10, 200)).await
}

#[tauri::command]
fn send_message(e: Eng, conv_id: String, body: String, thread_root: Option<String>, reply_to: Option<String>, also_send_to_channel: Option<bool>) -> Result<MessageView> {
    e.send_message(&conv_id, &body, thread_root, reply_to, also_send_to_channel.unwrap_or(false))
}

#[tauri::command]
fn retry_message(e: Eng, local_id: String) -> Result<()> {
    e.retry_message(&local_id)
}

#[tauri::command]
fn discard_message(e: Eng, local_id: String) -> Result<()> {
    e.discard_message(&local_id)
}

#[tauri::command]
async fn edit_message(e: Eng<'_>, id: String, body: String) -> Result<MessageView> {
    e.edit_message(&id, &body).await
}

#[tauri::command]
async fn delete_message(e: Eng<'_>, id: String) -> Result<MessageView> {
    e.delete_message(&id).await
}

#[tauri::command]
async fn react(e: Eng<'_>, id: String, emoji: String, add: bool) -> Result<MessageView> {
    e.react(&id, &emoji, add).await
}

#[tauri::command]
async fn mark_read(e: Eng<'_>, conv_id: String) -> Result<()> {
    e.mark_read(&conv_id).await
}

#[tauri::command]
async fn set_favourite(e: Eng<'_>, conv_id: String, favourite: bool) -> Result<()> {
    e.set_favourite(&conv_id, favourite).await
}

#[tauri::command]
async fn order_conversations(e: Eng<'_>, conv_ids: Vec<String>) -> Result<()> {
    e.order_conversations(conv_ids).await
}

#[tauri::command]
async fn create_dm(e: Eng<'_>, user_id: String) -> Result<String> {
    e.create_dm(&user_id).await
}

#[tauri::command]
async fn list_files(e: Eng<'_>, kind: Option<String>, q: Option<String>, offset: Option<i64>, limit: Option<i64>) -> Result<Vec<FileHit>> {
    e.list_files(kind, q, offset, limit.unwrap_or(100).clamp(1, 200)).await
}

#[tauri::command]
async fn list_saved_messages(e: Eng<'_>) -> Result<Vec<MessageView>> {
    e.list_saved_messages().await
}

#[tauri::command]
async fn get_thread(e: Eng<'_>, conv_id: String, root: String) -> Result<ThreadView> {
    e.get_thread(&conv_id, &root).await
}

#[tauri::command]
fn chat_row(e: Eng, conv_id: String) -> Result<Option<ChatRow>> {
    e.chat_row(&conv_id)
}

#[tauri::command]
fn typing(e: Eng, conv_id: String) {
    e.typing(&conv_id);
}

#[tauri::command]
fn reconnect(e: Eng) {
    e.reconnect_now();
}

#[tauri::command]
fn open_url(app: tauri::AppHandle, url: String) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;
    let u = url::Url::parse(&url).map_err(|_| CoreError::local("That link is not valid."))?;
    if !matches!(u.scheme(), "http" | "https" | "mailto") {
        return Err(CoreError::local("Only web links can be opened."));
    }
    app.opener().open_url(u.as_str(), None::<&str>).map_err(|e| CoreError::local(e.to_string()))
}


// ---------------------------------------------------------------- attachments

fn stage_many(e: &Engine, paths: Vec<std::path::PathBuf>) -> Result<Vec<Staged>> {
    let mut out = vec![];
    let mut first_err = None;
    for p in paths {
        match e.stage_path(&p, None) {
            Ok(s) => out.push(s),
            Err(err) => first_err = first_err.or(Some(err)),
        }
    }
    // One bad file must not lose the good ones; the error is only reported when nothing could be added.
    match (out.is_empty(), first_err) {
        (true, Some(err)) => Err(err),
        _ => Ok(out),
    }
}

/// Opens the system file picker (photos and videos, or any file) and stages what was chosen.
/// `WORKCHAT_TEST_PICK=/a:/b` replaces the dialog in automated UI tests.
#[tauri::command]
async fn pick_files(app: AppHandle, e: Eng<'_>, kind: String) -> Result<Vec<Staged>> {
    use tauri_plugin_dialog::{DialogExt, FilePath};
    if let Ok(list) = std::env::var("WORKCHAT_TEST_PICK") {
        let paths = list.split(':').filter(|p| !p.is_empty()).map(std::path::PathBuf::from).collect();
        return stage_many(&e, paths);
    }
    #[cfg(target_os = "android")]
    {
        // The system picker returns content:// URIs; the Kotlin side copies each pick into the app's cache and returns real paths.
        let app2 = app.clone();
        let k = kind.clone();
        let v: serde_json::Value = tauri::async_runtime::spawn_blocking(move || tauri_plugin_workchat::call(&app2, "pick_files", serde_json::json!({"kind": k}))).await.map_err(|e| CoreError::local(e.to_string()))?.map_err(CoreError::local)?;
        let mut out = vec![];
        for f in v["files"].as_array().cloned().unwrap_or_default() {
            let (Some(p), name) = (f["path"].as_str(), f["name"].as_str()) else { continue };
            let r = e.stage_path(std::path::Path::new(p), name);
            let _ = std::fs::remove_file(p);
            out.push(r?);
        }
        return Ok(out);
    }
    #[allow(unreachable_code)]
    let mut b = app.dialog().file().set_title(if kind == "media" { "Choose photos or videos" } else { "Choose files" });
    if kind == "media" {
        b = b.add_filter("Photos and videos", &["jpg", "jpeg", "png", "gif", "webp", "heic", "mp4", "mov", "m4v", "webm", "mkv", "3gp"]);
    }
    let picked = tauri::async_runtime::spawn_blocking(move || b.blocking_pick_files()).await.map_err(|e| CoreError::local(e.to_string()))?;
    let mut paths = vec![];
    for p in picked.unwrap_or_default() {
        match p {
            FilePath::Path(p) => paths.push(p),
            FilePath::Url(u) => match u.to_file_path() {
                Ok(p) => paths.push(p),
                Err(_) => return Err(CoreError::local("This location cannot be read directly yet. Pick the file from another folder.")),
            },
        }
    }
    stage_many(&e, paths)
}

#[tauri::command]
fn stage_paths(e: Eng, paths: Vec<String>) -> Result<Vec<Staged>> {
    stage_many(&e, paths.into_iter().map(std::path::PathBuf::from).collect())
}

#[tauri::command]
fn stage_begin(e: Eng, name: String) -> Result<String> {
    e.stage_begin(&name)
}

#[tauri::command]
fn stage_chunk(e: Eng, request: tauri::ipc::Request<'_>) -> Result<()> {
    let id = request.headers().get("x-stage-id").and_then(|v| v.to_str().ok()).ok_or_else(|| CoreError::local("Missing file id."))?.to_string();
    match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => e.stage_chunk(&id, bytes),
        _ => Err(CoreError::local("Expected raw bytes.")),
    }
}

#[tauri::command]
fn stage_end(e: Eng, id: String) -> Result<Staged> {
    e.stage_end(&id)
}

#[tauri::command]
fn unstage(e: Eng, id: String) {
    e.unstage(&id);
}

#[tauri::command]
fn set_staged_meta(e: Eng, id: String, meta: StagedMeta) {
    e.set_staged_meta(&id, meta);
}

#[tauri::command]
fn send_files(e: Eng, conv_id: String, ids: Vec<String>, body: Option<String>, thread_root: Option<String>, reply_to: Option<String>, also_send_to_channel: Option<bool>) -> Result<MessageView> {
    e.send_files(&conv_id, &ids, &body.unwrap_or_default(), thread_root, reply_to, also_send_to_channel.unwrap_or(false))
}

#[tauri::command]
async fn save_attachment(app: AppHandle, e: Eng<'_>, id: String, name: String) -> Result<String> {
    #[cfg(target_os = "android")]
    {
        // Into the public Downloads folder through MediaStore.
        let tmp = e.dir.join("open").join(&id);
        let p = e.export_attachment_to(&id, &name, &tmp).await?;
        let (app2, path, nm) = (app.clone(), p.display().to_string(), name.clone());
        let v: serde_json::Value = tauri::async_runtime::spawn_blocking(move || tauri_plugin_workchat::call(&app2, "save_download", serde_json::json!({"path": path, "name": nm}))).await.map_err(|e| CoreError::local(e.to_string()))?.map_err(CoreError::local)?;
        let _ = std::fs::remove_file(&p);
        return Ok(v["where"].as_str().unwrap_or("Downloads").to_string());
    }
    #[allow(unreachable_code)]
    let dir = app.path().download_dir().or_else(|_| app.path().app_data_dir().map(|d| d.join("Downloads"))).map_err(|e| CoreError::local(e.to_string()))?;
    let p = e.export_attachment(&id, &name, &dir).await?;
    Ok(p.display().to_string())
}

/// Opens the file with the system's default app (a copy with its real name, in the cache). Never fails silently:
/// when no app opens it the answer says so and the UI offers Open with / Save as / Show in folder.
#[tauri::command]
async fn open_attachment(app: AppHandle, e: Eng<'_>, id: String, name: String) -> Result<openfile::OpenOutcome> {
    let dir = e.dir.join("open").join(&id);
    let p = e.export_attachment_to(&id, &name, &dir).await?;
    let path = p.display().to_string();
    #[cfg(target_os = "android")]
    {
        let path2 = path.clone();
        tauri::async_runtime::spawn_blocking(move || tauri_plugin_workchat::call::<_, serde_json::Value>(&app, "view_file", serde_json::json!({"path": path2}))).await.map_err(|e| CoreError::local(e.to_string()))?.map_err(CoreError::local)?;
        return Ok(openfile::OpenOutcome { opened: true, path, reason: String::new() });
    }
    #[allow(unreachable_code)]
    {
        let p2 = p.clone();
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let res = {
            use tauri_plugin_opener::OpenerExt;
            app.opener().open_path(p2.display().to_string(), None::<&str>).map_err(|e| e.to_string())
        };
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let res = {
            let _ = &app;
            tauri::async_runtime::spawn_blocking(move || openfile::open_default(&p2)).await.map_err(|e| CoreError::local(e.to_string()))?
        };
        Ok(match res {
            Ok(()) => openfile::OpenOutcome { opened: true, path, reason: String::new() },
            Err(reason) => openfile::OpenOutcome { opened: false, path, reason },
        })
    }
}

/// Fallback "Save as...": asks where, copies the cached file there. Returns the saved path, or None when cancelled.
#[tauri::command]
async fn save_attachment_as(app: AppHandle, e: Eng<'_>, id: String, name: String) -> Result<Option<String>> {
    use tauri_plugin_dialog::{DialogExt, FilePath};
    let dir = e.dir.join("open").join(&id);
    let p = e.export_attachment_to(&id, &name, &dir).await?;
    let start = app.path().download_dir().ok();
    let mut b = app.dialog().file().set_title("Save as").set_file_name(&name);
    if let Some(d) = start {
        b = b.set_directory(d);
    }
    let picked = tauri::async_runtime::spawn_blocking(move || b.blocking_save_file()).await.map_err(|e| CoreError::local(e.to_string()))?;
    let Some(FilePath::Path(dest)) = picked else { return Ok(None) };
    std::fs::copy(&p, &dest).map_err(|e| CoreError::local(format!("Could not save the file: {e}")))?;
    Ok(Some(dest.display().to_string()))
}

/// Fallback "Show in folder": the file manager with the cached copy selected (or its folder).
#[tauri::command]
async fn show_attachment_in_folder(app: AppHandle, e: Eng<'_>, id: String, name: String) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;
    let dir = e.dir.join("open").join(&id);
    let p = e.export_attachment_to(&id, &name, &dir).await?;
    if app.opener().reveal_item_in_dir(&p).is_ok() {
        return Ok(());
    }
    let parent = p.parent().map(|d| d.to_path_buf()).unwrap_or_else(|| dir.clone());
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        return openfile::open_default(&parent).map_err(CoreError::local);
    }
    #[allow(unreachable_code)]
    app.opener().open_path(parent.display().to_string(), None::<&str>).map_err(|e| CoreError::local(e.to_string()))
}

/// Fallback "Open with...": the system chooser on Windows; elsewhere pick a program, then run it on the file.
#[tauri::command]
async fn open_attachment_with(app: AppHandle, e: Eng<'_>, id: String, name: String) -> Result<bool> {
    use tauri_plugin_dialog::{DialogExt, FilePath};
    let dir = e.dir.join("open").join(&id);
    let p = e.export_attachment_to(&id, &name, &dir).await?;
    #[cfg(windows)]
    {
        let _ = &app;
        openfile::windows_open_as(&p).map_err(CoreError::local)?;
        return Ok(true);
    }
    #[allow(unreachable_code)]
    {
        let mut b = app.dialog().file().set_title("Open with...");
        if cfg!(target_os = "macos") {
            b = b.set_directory("/Applications");
        } else if cfg!(target_os = "linux") {
            b = b.set_directory("/usr/bin");
        }
        let picked = tauri::async_runtime::spawn_blocking(move || b.blocking_pick_file()).await.map_err(|e| CoreError::local(e.to_string()))?;
        let Some(FilePath::Path(prog)) = picked else { return Ok(false) };
        tauri::async_runtime::spawn_blocking(move || openfile::open_with(&prog, &p)).await.map_err(|e| CoreError::local(e.to_string()))?.map_err(CoreError::local)?;
        Ok(true)
    }
}

// ---------------------------------------------------------------- voice and playback

#[tauri::command]
async fn voice_start(app: AppHandle, e: Eng<'_>) -> Result<()> {
    // cpal on Android records silence (or fails) until the runtime permission is granted: ask first.
    #[cfg(target_os = "android")]
    {
        let app2 = app.clone();
        let r = tauri::async_runtime::spawn_blocking(move || tauri_plugin_workchat::call::<_, serde_json::Value>(&app2, "request_permission", serde_json::json!({"name": "microphone"}))).await.map_err(|e| CoreError::local(e.to_string()))?;
        if !r.map(|v| v["state"] == "granted").unwrap_or(false) {
            return Err(CoreError::local("WorkChat needs the microphone permission to record voice messages. Allow it in the phone's app settings."));
        }
    }
    let _ = &app;
    e.voice_start()
}
#[tauri::command]
fn voice_finish(e: Eng) -> Result<Staged> {
    e.voice_finish()
}
#[tauri::command]
fn voice_cancel(e: Eng) {
    e.voice_cancel();
}

#[tauri::command]
async fn audio_play(e: Eng<'_>, id: String, speed: Option<f32>, from_ms: Option<u32>) -> Result<()> {
    e.audio_play(&id, speed.unwrap_or(1.0), from_ms.unwrap_or(0)).await
}
#[tauri::command]
fn audio_ctl(e: Eng, action: String, value: Option<f32>) {
    use player::Cmd;
    let c = match action.as_str() {
        "pause" => Cmd::Pause,
        "resume" => Cmd::Resume,
        "seek" => Cmd::Seek(value.unwrap_or(0.0) as u32),
        "speed" => Cmd::Speed(value.unwrap_or(1.0)),
        _ => Cmd::Stop,
    };
    e.player().send(c);
}

// ---------------------------------------------------------------- threads, counts, search, people

#[tauri::command]
async fn get_counts(e: Eng<'_>) -> Result<Counts> {
    e.get_counts().await
}
#[tauri::command]
async fn list_threads(e: Eng<'_>, unread: Option<bool>, before: Option<i64>, limit: Option<i64>) -> Result<Vec<ThreadSummary>> {
    e.list_threads(unread.unwrap_or(false), before, limit.unwrap_or(50).clamp(1, 200)).await
}
#[tauri::command]
async fn follow_thread(e: Eng<'_>, root: String, on: bool) -> Result<()> {
    e.follow_thread(&root, on).await
}
#[tauri::command]
async fn read_thread(e: Eng<'_>, root: String) -> Result<()> {
    e.read_thread(&root).await
}
#[tauri::command]
async fn search(e: Eng<'_>, q: String, conv_id: Option<String>, limit: Option<i64>) -> Result<SearchResponse> {
    e.search(&q, conv_id, limit.unwrap_or(30).clamp(1, 100)).await
}
#[tauri::command]
async fn list_users(e: Eng<'_>, q: Option<String>) -> Result<Vec<User>> {
    e.list_users(q).await
}
#[tauri::command]
async fn update_profile(e: Eng<'_>, display_name: Option<String>, title: Option<String>, phone: Option<String>, read_receipts: Option<bool>, hide_last_active: Option<bool>) -> Result<Me> {
    e.update_me(UpdateMeRequest { display_name, title, phone, read_receipts, hide_last_active, ..Default::default() }).await
}

// ---------------------------------------------------------------- message actions

#[tauri::command]
async fn forward_message(e: Eng<'_>, id: String, conv_ids: Vec<String>, comment: Option<String>) -> Result<()> {
    e.forward_message(&id, conv_ids, comment).await
}
#[tauri::command]
async fn pin_message(e: Eng<'_>, conv_id: String, id: String, on: bool) -> Result<()> {
    e.pin_message(&conv_id, &id, on).await
}
#[tauri::command]
async fn get_pins(e: Eng<'_>, conv_id: String) -> Result<Vec<MessageView>> {
    e.get_pins(&conv_id).await
}
#[tauri::command]
async fn save_message(e: Eng<'_>, id: String, on: bool) -> Result<()> {
    e.save_message(&id, on).await
}
#[tauri::command]
async fn fetch_message(e: Eng<'_>, id: String) -> Result<MessageView> {
    e.fetch_message(&id).await
}
#[tauri::command]
async fn messages_around(e: Eng<'_>, conv_id: String, seq: i64) -> Result<Vec<MessageView>> {
    e.messages_around(&conv_id, seq).await
}

// ---------------------------------------------------------------- avatars and new chats

#[tauri::command]
async fn set_avatar(e: Eng<'_>, kind: String, target: Option<String>, staged_id: String) -> Result<()> {
    e.set_avatar(&kind, target, &staged_id).await
}
#[tauri::command]
async fn remove_avatar(e: Eng<'_>, kind: String, target: Option<String>) -> Result<()> {
    e.remove_avatar(&kind, target).await
}
#[tauri::command]
fn set_local_avatar(e: Eng, kind: String, target: String, staged_id: String) -> Result<()> {
    e.set_local_avatar(&kind, &target, &staged_id)
}
#[tauri::command]
fn remove_local_avatar(e: Eng, kind: String, target: String) -> Result<()> {
    e.remove_local_avatar(&kind, &target)
}
#[tauri::command]
async fn create_conversation(e: Eng<'_>, kind: String, name: String, topic: Option<String>, members: Vec<String>, avatar_staged: Option<String>) -> Result<String> {
    e.create_conversation(&kind, &name, topic, members, avatar_staged).await
}
#[tauri::command]
fn get_members(e: Eng, conv_id: String) -> Result<Vec<Member>> {
    e.guard(&conv_id)?;
    Ok(e.store.get_conv(&conv_id)?.map(|c| c.conversation.members).unwrap_or_default())
}
#[tauri::command]
async fn remove_member(e: Eng<'_>, conv_id: String, user_id: String) -> Result<()> {
    e.remove_member(&conv_id, &user_id).await
}
#[tauri::command]
async fn leave_conversation(e: Eng<'_>, conv_id: String) -> Result<()> {
    e.leave_conversation(&conv_id).await
}
#[tauri::command]
async fn hide_message(e: Eng<'_>, id: String) -> Result<()> {
    e.hide_message(&id).await
}
#[tauri::command]
async fn get_org_settings(e: Eng<'_>) -> Result<OrgSettings> {
    e.org_settings().await
}
#[tauri::command]
async fn add_members(e: Eng<'_>, conv_id: String, user_ids: Vec<String>) -> Result<()> {
    e.add_members(&conv_id, user_ids).await
}
#[tauri::command]
async fn update_conversation(e: Eng<'_>, conv_id: String, name: Option<String>, topic: Option<String>, archived: Option<bool>) -> Result<()> {
    e.update_conversation(&conv_id, UpdateConversation { name, topic, archived }).await
}


// ---------------------------------------------------------------- lock (6-digit PIN)

#[tauri::command]
fn lock_status(e: Eng) -> lock::LockStatus {
    e.lock_status()
}
#[tauri::command]
fn lock_set_pin(e: Eng, pin: String, confirm: String) -> Result<()> {
    e.lock_set_pin(&pin, &confirm)
}
#[tauri::command]
fn lock_unlock(e: Eng, pin: String) -> Result<lock::UnlockResult> {
    e.lock_unlock(&pin)
}
#[tauri::command]
fn lock_relock(e: Eng) {
    e.lock_relock();
}
#[tauri::command]
fn lock_change_pin(e: Eng, old: String, new: String, confirm: String) -> Result<lock::UnlockResult> {
    e.lock_change_pin(&old, &new, &confirm)
}
#[tauri::command]
fn lock_set_timeout(e: Eng, secs: i64, on_background: bool) -> Result<()> {
    e.lock_set_timeout(secs, on_background)
}
#[tauri::command]
fn lock_tick(e: Eng) -> bool {
    e.lock_tick()
}
#[tauri::command]
fn lock_touch(e: Eng) {
    e.touch();
}
#[tauri::command]
fn lock_background(e: Eng) {
    e.lock_background();
}
#[tauri::command]
async fn lock_chat(e: Eng<'_>, conv_id: String, on: bool) -> Result<()> {
    e.lock_chat(&conv_id, on).await
}
/// Forgot PIN: signing out wipes the PIN, the locked-chat list and every cached message.
#[tauri::command]
async fn lock_forgot(e: Eng<'_>) -> Result<()> {
    e.inner().clone().sign_out().await;
    Ok(())
}
/// Ids of locked chats (only while the area is open, otherwise empty): the "Locked chats" list.
#[tauri::command]
fn locked_ids(e: Eng) -> Vec<String> {
    if e.lock_status().open {
        let mut v: Vec<String> = lock::locked_set(&e.store).into_iter().collect();
        v.sort();
        v
    } else {
        vec![]
    }
}

// ---------------------------------------------------------------- notifications and push

#[derive(serde::Serialize)]
struct NotifyStatus {
    platform: String,
    /// `granted`, `denied` or `prompt`.
    permission: String,
    /// `os` (desktop), `unifiedpush`, `keepalive`, `fcm`, `apns` or `none`.
    method: String,
    distributors: Vec<String>,
    distributor: String,
    fcm: bool,
    keep_alive_running: bool,
    tray: bool,
    push: device::PushInfo,
    install_ok: bool,
}

#[tauri::command]
fn get_notify_settings(e: Eng) -> notify::NotifySettings {
    e.notify_settings()
}

#[tauri::command]
fn set_notify_settings(app: AppHandle, e: Eng, settings: notify::NotifySettings) -> Result<()> {
    e.set_notify_settings(&settings)?;
    #[cfg(desktop)]
    desktop::CLOSE_TO_TRAY.store(settings.close_to_tray && desktop::tray_ok(), std::sync::atomic::Ordering::SeqCst);
    #[cfg(target_os = "android")]
    {
        let dir = e.dir.display().to_string();
        let on = settings.keep_alive;
        let _ = tauri_plugin_workchat::call::<_, serde_json::Value>(&app, "set_keep_alive", serde_json::json!({"on": on, "dataDir": dir}));
    }
    let _ = &app;
    Ok(())
}

/// The window state for notification rules (focused, open chat) and the time zone, in minutes east of UTC.
#[tauri::command]
fn ui_state(e: Eng, focused: bool, conv: Option<String>, tz_offset_min: Option<i32>) {
    e.set_ui_state(focused, conv, tz_offset_min);
}

#[tauri::command]
async fn notify_status(app: AppHandle, e: Eng<'_>) -> Result<NotifyStatus> {
    let settings = e.notify_settings();
    let mut st = NotifyStatus {
        platform: std::env::consts::OS.into(),
        permission: "granted".into(),
        method: if settings.enabled { "os".into() } else { "none".into() },
        distributors: vec![],
        distributor: String::new(),
        fcm: false,
        keep_alive_running: false,
        tray: false,
        push: e.push_info(),
        install_ok: true,
    };
    #[cfg(desktop)]
    {
        st.tray = desktop::tray_ok();
        #[cfg(not(target_os = "linux"))]
        {
            use tauri_plugin_notification::NotificationExt;
            st.permission = match app.notification().permission_state() {
                Ok(tauri_plugin_notification::PermissionState::Granted) => "granted",
                Ok(tauri_plugin_notification::PermissionState::Denied) => "denied",
                _ => "prompt",
            }
            .into();
        }
    }
    #[cfg(mobile)]
    {
        let app2 = app.clone();
        let dir = e.dir.display().to_string();
        let v: serde_json::Value = tauri::async_runtime::spawn_blocking(move || tauri_plugin_workchat::call(&app2, "status", serde_json::json!({"dataDir": dir})).unwrap_or_default()).await.unwrap_or_default();
        st.permission = v["notifications"].as_str().unwrap_or("prompt").to_string();
        st.distributors = v["distributors"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        st.distributor = v["distributor"].as_str().unwrap_or("").to_string();
        st.fcm = v["fcm"].as_bool().unwrap_or(false);
        st.keep_alive_running = v["keepAlive"].as_bool().unwrap_or(false);
        st.install_ok = v["canInstall"].as_bool().unwrap_or(true);
        st.method = if !settings.enabled {
            "none"
        } else if st.push.kind == "unifiedpush" && !st.push.error.is_empty() {
            "none"
        } else if st.push.kind == "unifiedpush" {
            "unifiedpush"
        } else if st.push.kind == "fcm" {
            "fcm"
        } else if st.push.kind == "apns" {
            "apns"
        } else if st.keep_alive_running {
            "keepalive"
        } else {
            "none"
        }
        .into();
    }
    let _ = &app;
    Ok(st)
}

/// Asks the system for permission to show notifications (Android 13+, iOS, Windows). Returns the new state.
#[tauri::command]
async fn notify_request_permission(app: AppHandle) -> Result<String> {
    #[cfg(mobile)]
    {
        let r = tauri::async_runtime::spawn_blocking(move || tauri_plugin_workchat::call::<_, serde_json::Value>(&app, "request_permission", serde_json::json!({"name": "notifications"}))).await.map_err(|e| CoreError::local(e.to_string()))?;
        return Ok(r.map(|v| v["state"].as_str().unwrap_or("prompt").to_string()).unwrap_or_else(|_| "denied".into()));
    }
    #[cfg(all(desktop, not(target_os = "linux")))]
    {
        use tauri_plugin_notification::NotificationExt;
        let s = app.notification().request_permission().map_err(|e| CoreError::local(e.to_string()))?;
        return Ok(if s == tauri_plugin_notification::PermissionState::Granted { "granted" } else { "denied" }.into());
    }
    #[allow(unreachable_code)]
    {
        let _ = app;
        Ok("granted".into())
    }
}

#[tauri::command]
fn notify_test(app: AppHandle, e: Eng) -> Result<()> {
    let s = e.notify_settings();
    let n = notify::Notice { conv_id: String::new(), title: "WorkChat".into(), body: "Test notification: this is how new messages will look.".into(), sound: s.sound.clone(), vibrate: s.vibration, locked: false, message_id: String::new() };
    #[cfg(desktop)]
    desktop::show_notice(&app, n);
    #[cfg(target_os = "android")]
    android::post_notice(&n);
    #[cfg(target_os = "ios")]
    {
        use tauri_plugin_notification::NotificationExt;
        let _ = app.notification().builder().title(&n.title).body(&n.body).show();
    }
    let _ = (&app, &e);
    Ok(())
}

#[tauri::command]
fn push_info(e: Eng) -> device::PushInfo {
    e.push_info()
}

/// Registers this device for push: Android asks the UnifiedPush distributor for an endpoint (it arrives later through
/// `push_endpoint`), iOS asks for an APNs token. Returns what the platform answered right away.
#[tauri::command]
async fn push_register(app: AppHandle, e: Eng<'_>, distributor: Option<String>) -> Result<serde_json::Value> {
    let _ = (&app, &e, &distributor);
    #[cfg(mobile)]
    {
        let app2 = app.clone();
        let cmd = if cfg!(target_os = "ios") { "ios_register" } else { "push_register" };
        let v: serde_json::Value = tauri::async_runtime::spawn_blocking(move || tauri_plugin_workchat::call(&app2, cmd, serde_json::json!({"distributor": distributor}))).await.map_err(|e| CoreError::local(e.to_string()))?.map_err(CoreError::local)?;
        // An endpoint or token that is already known is registered with the server at once.
        if let (Some(kind), Some(target)) = (v["kind"].as_str(), v["target"].as_str()) {
            if !target.is_empty() {
                e.register_push(kind, target).await?;
            }
        }
        return Ok(v);
    }
    #[allow(unreachable_code)]
    Err(CoreError::local("Push registration is for phones. This computer shows notifications itself."))
}

/// A UnifiedPush endpoint or an APNs / FCM token the platform produced: tell the server.
#[tauri::command]
async fn push_endpoint(e: Eng<'_>, kind: String, target: String) -> Result<device::PushInfo> {
    e.register_push(&kind, &target).await
}

#[tauri::command]
async fn push_unregister(app: AppHandle, e: Eng<'_>) -> Result<()> {
    let _ = &app;
    e.unregister_push().await?;
    #[cfg(mobile)]
    {
        let _ = tauri_plugin_workchat::call::<_, serde_json::Value>(&app, "push_unregister", serde_json::json!({}));
    }
    Ok(())
}

// ---------------------------------------------------------------- storage

#[tauri::command]
fn storage_info(e: Eng) -> device::StorageInfo {
    e.storage_info()
}
#[tauri::command]
fn clear_cache(e: Eng) -> Result<device::StorageInfo> {
    e.clear_cache()
}
#[tauri::command]
fn compact_db(e: Eng) -> Result<device::StorageInfo> {
    e.compact_database()
}
#[tauri::command]
fn media_cached(e: Eng, ids: Vec<String>) -> Vec<String> {
    e.media_cached(ids)
}

// ---------------------------------------------------------------- updates

#[derive(serde::Serialize)]
struct UpdateState {
    current: String,
    found: Option<String>,
    due: bool,
    api: String,
    custom_api: bool,
}

#[tauri::command]
fn update_state(e: Eng) -> UpdateState {
    let api = e.update_api();
    UpdateState { current: api::CLIENT_VERSION.into(), found: e.update_found(), due: e.update_check_due(), custom_api: api != update::DEFAULT_API, api }
}

#[tauri::command]
async fn check_update(e: Eng<'_>) -> Result<update::UpdateInfo> {
    e.check_update().await
}

#[tauri::command]
fn set_update_api(e: Eng, url: String) -> Result<()> {
    e.set_update_api(&url)
}

#[tauri::command]
fn cancel_update(e: Eng) {
    e.update_cancel.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Downloads the platform's asset and starts the installation the way that platform does it.
#[tauri::command]
async fn start_update(app: AppHandle, e: Eng<'_>, info: update::UpdateInfo) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;
    let Some(asset) = info.asset.clone() else {
        return open_url(app, info.page);
    };
    let path = match e.download_update(&asset, info.checksum_url.clone()).await {
        Ok(p) => p,
        Err(err) => {
            e.emit(view::CoreEvent::Update { state: "error".into(), got: 0, total: 0, message: err.to_string() });
            return Err(err);
        }
    };
    let finish = |state: &str, msg: &str| e.emit(view::CoreEvent::Update { state: state.into(), got: asset.size, total: asset.size, message: msg.into() });
    match info.action.as_str() {
        "install" => {
            #[cfg(target_os = "android")]
            {
                let app2 = app.clone();
                let p = path.display().to_string();
                let v: serde_json::Value = tauri::async_runtime::spawn_blocking(move || tauri_plugin_workchat::call(&app2, "install_apk", serde_json::json!({"path": p}))).await.map_err(|e| CoreError::local(e.to_string()))?.map_err(CoreError::local)?;
                if v["state"] == "needs_permission" {
                    finish("needs_permission", "Allow WorkChat to install apps, then tap Install again.");
                } else {
                    finish("installing", "The system installer is open.");
                }
            }
            #[cfg(not(target_os = "android"))]
            finish("ready", "Downloaded.");
        }
        "installer" => {
            std::process::Command::new(&path).spawn().map_err(|e| CoreError::local(format!("Could not start the installer: {e}")))?;
            finish("installing", "The installer is starting. WorkChat will close.");
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            app.exit(0);
        }
        "portable" => {
            let me = std::env::current_exe().map_err(|e| CoreError::local(e.to_string()))?;
            let old = me.with_extension("exe.old");
            let _ = std::fs::remove_file(&old);
            std::fs::rename(&me, &old).map_err(|e| CoreError::local(format!("Could not replace the program: {e}")))?;
            if let Err(err) = std::fs::copy(&path, &me) {
                let _ = std::fs::rename(&old, &me);
                return Err(CoreError::local(format!("Could not write the new program: {err}")));
            }
            let _ = std::fs::remove_file(&path);
            std::process::Command::new(&me).spawn().map_err(|e| CoreError::local(e.to_string()))?;
            finish("installing", "Restarting with the new version.");
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            app.exit(0);
        }
        "appimage" => {
            let target = std::env::var("APPIMAGE").map_err(|_| CoreError::local("Not running as an AppImage."))?;
            let target = std::path::PathBuf::from(target);
            // A temporary file next to the AppImage, then an atomic rename over it (the running copy keeps its old inode).
            let tmp = target.with_extension("update-tmp");
            std::fs::copy(&path, &tmp).map_err(|e| CoreError::local(format!("Could not write next to the AppImage: {e}")))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
            }
            std::fs::rename(&tmp, &target).map_err(|e| CoreError::local(e.to_string()))?;
            let _ = std::fs::remove_file(&path);
            std::process::Command::new(&target).spawn().map_err(|e| CoreError::local(e.to_string()))?;
            finish("installing", "Restarting with the new version.");
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            app.exit(0);
        }
        _ => {
            // deb / rpm / flatpak: the system's own installer opens the downloaded file.
            match app.opener().open_path(path.display().to_string(), None::<&str>) {
                Ok(()) => finish("installing", "Opened with your software installer."),
                Err(_) => {
                    let _ = app.opener().open_url(asset.url.as_str(), None::<&str>);
                    finish("ready", "Downloaded to the updates folder; opened the download in your browser.");
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- phone: permissions

/// Asks Android for a runtime permission (`microphone`, `camera`, `notifications`). `true` when granted.
#[tauri::command]
async fn request_permission(app: AppHandle, name: String) -> Result<bool> {
    let _ = (&app, &name);
    #[cfg(target_os = "android")]
    {
        let r = tauri::async_runtime::spawn_blocking(move || tauri_plugin_workchat::call::<_, serde_json::Value>(&app, "request_permission", serde_json::json!({"name": name}))).await.map_err(|e| CoreError::local(e.to_string()))?;
        return Ok(r.map(|v| v["state"] == "granted").unwrap_or(false));
    }
    #[allow(unreachable_code)]
    Ok(true)
}

/// Status and navigation bar colours follow the app theme (Android). `color` is the page background.
#[tauri::command]
async fn set_system_bars(app: AppHandle, color: String, dark: bool) -> Result<()> {
    let _ = (&app, &color, dark);
    #[cfg(target_os = "android")]
    {
        let _ = tauri::async_runtime::spawn_blocking(move || tauri_plugin_workchat::call::<_, serde_json::Value>(&app, "set_system_bars", serde_json::json!({"color": color, "dark": dark}))).await;
    }
    Ok(())
}

/// Sessions of this account (Devices & Sessions).
#[tauri::command]
async fn list_sessions(e: Eng<'_>) -> Result<serde_json::Value> {
    e.list_sessions().await
}
#[tauri::command]
async fn revoke_session(e: Eng<'_>, id: String) -> Result<()> {
    e.revoke_session(&id).await
}
#[tauri::command]
async fn revoke_other_sessions(e: Eng<'_>) -> Result<()> {
    e.revoke_other_sessions().await
}

// ---------------------------------------------------------------- automated UI tests (debug builds only)

/// Debug builds only: the script file named by `WORKCHAT_TEST_SCRIPT`, run by the UI to drive screenshots.
#[tauri::command]
fn test_script() -> Option<String> {
    if cfg!(debug_assertions) {
        std::env::var("WORKCHAT_TEST_SCRIPT").ok().and_then(|p| std::fs::read_to_string(p).ok())
    } else {
        None
    }
}

/// Debug builds only: evaluates the test script in the page (the page's CSP forbids `eval`, the webview's own eval is not subject to it).
#[tauri::command]
fn test_run(win: tauri::WebviewWindow, script: String) {
    if cfg!(debug_assertions) {
        let js = format!("(async()=>{{const api=window.__wcApi;try{{{script}\n}}catch(e){{await api.core.testMark('error: '+(e&&e.message||e));return}}await api.core.testMark('done')}})()");
        let _ = win.eval(&js);
    }
}

/// Debug builds only: writes `<data dir>/mark` and waits until the test driver has taken its screenshot.
#[tauri::command]
async fn test_mark(e: Eng<'_>, name: String) -> Result<()> {
    if !cfg!(debug_assertions) {
        return Ok(());
    }
    let p = e.dir.join("mark");
    std::fs::write(&p, name)?;
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if !p.exists() {
            break;
        }
    }
    Ok(())
}

fn media_response(r: media::MediaResp) -> tauri::http::Response<Vec<u8>> {
    let mut b = tauri::http::Response::builder()
        .status(r.status)
        .header("Content-Type", r.content_type)
        .header("Accept-Ranges", "bytes")
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "private, max-age=3600")
        .header("Content-Length", r.body.len().to_string());
    if let Some(cr) = r.content_range {
        b = b.header("Content-Range", cr);
    }
    b.body(r.body).unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn,workchat_lib=info".into())).try_init();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_workchat::init());
    #[cfg(not(target_os = "android"))]
    let builder = builder.plugin(tauri_plugin_notification::init());
    #[cfg(desktop)]
    let builder = builder.on_window_event(|window, event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            if desktop::CLOSE_TO_TRAY.load(std::sync::atomic::Ordering::SeqCst) && desktop::tray_ok() {
                api.prevent_close();
                let _ = window.hide();
            }
        }
    });
    builder
        .setup(|app| {
            // The taskbar / dock / alt-tab icon: set it on the window explicitly (the bundled default is only the 32 px one).
            #[cfg(desktop)]
            if let Some(w) = app.get_webview_window("main") {
                if let Ok(img) = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png")) {
                    let _ = w.set_icon(img);
                }
            }
            // Windows portable: data lives next to the exe. WORKCHAT_DATA_DIR wins (tests, several accounts).
            #[cfg(windows)]
            let portable_dir = std::env::current_exe().ok().and_then(|p| portable::portable_dir(&p));
            #[cfg(not(windows))]
            let portable_dir: Option<std::path::PathBuf> = None;
            let dir = match (std::env::var("WORKCHAT_DATA_DIR"), &portable_dir) {
                (Ok(d), _) => std::path::PathBuf::from(d),
                (_, Some(p)) => p.clone(),
                _ => app.path().app_data_dir()?,
            };
            let handle = app.handle().clone();
            #[cfg(desktop)]
            let badge_engine: Arc<std::sync::OnceLock<Arc<Engine>>> = Arc::new(std::sync::OnceLock::new());
            #[cfg(desktop)]
            let be = badge_engine.clone();
            let emit: engine::Emit = Arc::new(move |ev| {
                #[cfg(desktop)]
                if matches!(ev, view::CoreEvent::Chat { .. } | view::CoreEvent::Reset | view::CoreEvent::Counts { .. } | view::CoreEvent::ChatRemove { .. }) {
                    if let Some(e) = be.get() {
                        desktop::set_badge(&handle, e.badge_count());
                    }
                }
                let _ = handle.emit("wc", &ev);
            });
            let rt = tauri::async_runtime::handle().inner().clone();
            #[cfg(target_os = "android")]
            let existing = android::global_engine();
            #[cfg(not(target_os = "android"))]
            let existing: Option<Arc<Engine>> = None;
            let engine = match existing {
                Some(e) => {
                    e.set_emit(emit);
                    e
                }
                None => Engine::with_portable(dir, emit, rt, portable_dir.is_some()).map_err(|e| e.to_string())?,
            };
            engine.attach_window();
            #[cfg(target_os = "android")]
            {
                android::set_global_engine(engine.clone());
                engine.set_notifier(Some(Arc::new(|n| android::post_notice(&n))));
            }
            #[cfg(desktop)]
            {
                let _ = badge_engine.set(engine.clone());
                desktop::setup_tray(app);
                desktop::CLOSE_TO_TRAY.store(engine.notify_settings().close_to_tray && desktop::tray_ok(), std::sync::atomic::Ordering::SeqCst);
                let h = app.handle().clone();
                engine.set_notifier(Some(Arc::new(move |n| desktop::show_notice(&h, n))));
            }
            #[cfg(target_os = "ios")]
            {
                let h = app.handle().clone();
                engine.set_notifier(Some(Arc::new(move |n| {
                    use tauri_plugin_notification::NotificationExt;
                    let _ = h.notification().builder().title(&n.title).body(&n.body).show();
                })));
            }
            app.manage(engine.clone());
            // `start` spawns tasks, so it must run inside the runtime.
            if !engine.is_started() {
                tauri::async_runtime::block_on(async { engine.start() });
            }
            Ok(())
        })
        .register_asynchronous_uri_scheme_protocol("wcmedia", |ctx, request, responder| {
            let engine = ctx.app_handle().state::<Arc<Engine>>().inner().clone();
            let path = request.uri().path().trim_start_matches('/').to_string();
            let thumb = request.uri().query().map(|q| q.split('&').any(|p| p == "thumb=1")).unwrap_or(false);
            let range = request.headers().get("range").and_then(|v| v.to_str().ok()).map(str::to_string);
            tauri::async_runtime::spawn(async move {
                let r = media::serve(&engine, &path, thumb, range.as_deref()).await;
                responder.respond(media_response(r));
            });
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            check_server,
            login_password,
            login_link,
            sign_out,
            get_snapshot,
            open_conversation,
            load_older,
            send_message,
            retry_message,
            discard_message,
            edit_message,
            delete_message,
            react,
            mark_read,
            set_favourite,
            order_conversations,
            create_dm,
            list_files,
            get_thread,
            chat_row,
            typing,
            reconnect,
            open_url,
            pick_files,
            stage_paths,
            stage_begin,
            stage_chunk,
            stage_end,
            unstage,
            set_staged_meta,
            send_files,
            save_attachment,
            open_attachment,
            list_saved_messages,
            save_attachment_as,
            show_attachment_in_folder,
            open_attachment_with,
            voice_start,
            voice_finish,
            voice_cancel,
            audio_play,
            audio_ctl,
            get_counts,
            list_threads,
            follow_thread,
            read_thread,
            search,
            list_users,
            update_profile,
            forward_message,
            pin_message,
            get_pins,
            save_message,
            fetch_message,
            messages_around,
            set_avatar,
            remove_avatar,
            set_local_avatar,
            remove_local_avatar,
            create_conversation,
            add_members,
            remove_member,
            get_members,
            leave_conversation,
            hide_message,
            get_org_settings,
            update_conversation,
            lock_status, lock_set_pin, lock_unlock, lock_relock, lock_change_pin, lock_set_timeout, lock_tick, lock_touch, lock_background, lock_chat, lock_forgot, locked_ids,
            get_notify_settings, set_notify_settings, ui_state, notify_status, notify_request_permission, notify_test,
            push_info, push_register, push_endpoint, push_unregister,
            storage_info, clear_cache, compact_db, media_cached,
            update_state, check_update, set_update_api, cancel_update, start_update,
            request_permission, set_system_bars, list_sessions, revoke_session, revoke_other_sessions,
            test_script,
            test_mark,
            test_run,
        ])
        .run(tauri::generate_context!())
        .expect("error while running WorkChat");
}
