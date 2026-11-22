//! The C2 operations of the engine: attachments (staging, upload through the outbox), voice messages, audio playback,
//! threads, counts, search, forward, pins, saved messages, avatars, contacts and creating chats.

use crate::api::now_ms;
use crate::engine::Engine;
use crate::error::{CoreError, Result};
use crate::mediainfo;
use crate::store::{OutAtt, OutboxItem};
use crate::view::{CoreEvent, LocalAvatar, MessageView};
use proto::*;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A file the person picked, dropped, pasted, captured or recorded, copied into the staging folder and waiting to be sent.
#[derive(Debug, Clone, Serialize)]
pub struct Staged {
    pub id: String,
    pub name: String,
    pub content_type: String,
    pub size: u64,
    /// `image`, `video`, `audio`, `voice` or `file`.
    pub kind: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u32>,
    pub pages: Option<u32>,
    pub poster_b64: Option<String>,
    pub peaks: Vec<u8>,
    #[serde(skip)]
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StagedMeta {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u32>,
    pub poster_b64: Option<String>,
}

fn safe_name(n: &str) -> String {
    let base = n.rsplit(['/', '\\']).next().unwrap_or("file");
    let cleaned: String = base.chars().filter(|c| !c.is_control() && *c != ':').take(150).collect();
    let cleaned = cleaned.trim().trim_start_matches('.').to_string();
    if cleaned.is_empty() {
        "file".into()
    } else {
        cleaned
    }
}

/// `voice-20261121-143005.ogg` (UTC), the name pattern the server documents for voice notes.
pub fn voice_name(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // civil from days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("voice-{y:04}{m:02}{d:02}-{:02}{:02}{:02}.ogg", rem / 3600, (rem % 3600) / 60, rem % 60)
}

fn human_mb(n: u64) -> String {
    if n >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", n as f64 / 1_073_741_824.0)
    } else {
        format!("{} MB", n / (1024 * 1024))
    }
}

impl Engine {
    // ---------------------------------------------------------------- staging

    pub fn staging_dir(&self) -> PathBuf {
        self.dir.join("staging")
    }

    pub fn max_upload(&self) -> u64 {
        self.store.kv_get::<u64>("max_upload").ok().flatten().unwrap_or(100 * 1024 * 1024)
    }

    fn new_staged_dir(&self) -> Result<(String, PathBuf)> {
        let id = format!("st_{}", uuid::Uuid::new_v4().simple());
        let dir = self.staging_dir().join(&id);
        std::fs::create_dir_all(&dir)?;
        Ok((id, dir))
    }

    fn register(&self, s: Staged) -> Staged {
        self.staged.lock().unwrap_or_else(|e| e.into_inner()).insert(s.id.clone(), s.clone());
        s
    }

    fn describe(&self, id: String, path: PathBuf, name: String) -> Result<Staged> {
        let size = std::fs::metadata(&path)?.len();
        if size == 0 {
            let _ = std::fs::remove_dir_all(path.parent().unwrap_or(&path));
            return Err(CoreError::local(format!("{name} is empty.")));
        }
        let max = self.max_upload();
        if size > max {
            let _ = std::fs::remove_dir_all(path.parent().unwrap_or(&path));
            return Err(CoreError::local(format!("{name} is {} and the limit on this server is {}.", human_mb(size), human_mb(max))));
        }
        let ct = mediainfo::content_type_for(&name).to_string();
        let info = mediainfo::probe(&path, &ct);
        let kind = mediainfo::kind_of(&ct).to_string();
        Ok(self.register(Staged { id, name, content_type: ct, size, kind, width: info.width, height: info.height, duration_ms: info.duration_ms, pages: info.pages, poster_b64: None, peaks: vec![], path }))
    }

    /// Copies (or hard-links) a file from anywhere on the device into the staging folder.
    pub fn stage_path(&self, src: &Path, name: Option<&str>) -> Result<Staged> {
        let md = std::fs::metadata(src).map_err(|_| CoreError::local("That file cannot be read."))?;
        if md.is_dir() {
            return Err(CoreError::local("Folders cannot be sent. Add the files inside it, or zip it first."));
        }
        let name = safe_name(name.or_else(|| src.file_name().and_then(|n| n.to_str())).unwrap_or("file"));
        // Refuse early, before copying a huge file.
        if md.len() > self.max_upload() {
            return Err(CoreError::local(format!("{name} is {} and the limit on this server is {}.", human_mb(md.len()), human_mb(self.max_upload()))));
        }
        let (id, dir) = self.new_staged_dir()?;
        let dest = dir.join(&name);
        if std::fs::hard_link(src, &dest).is_err() {
            std::fs::copy(src, &dest).map_err(|e| CoreError::local(format!("Cannot read {name}: {e}")))?;
        }
        self.describe(id, dest, name)
    }

    pub fn stage_begin(&self, name: &str) -> Result<String> {
        let (id, dir) = self.new_staged_dir()?;
        let name = safe_name(name);
        std::fs::File::create(dir.join(&name))?;
        let path = dir.join(&name);
        self.register(Staged { id: id.clone(), name, content_type: String::new(), size: 0, kind: "file".into(), width: None, height: None, duration_ms: None, pages: None, poster_b64: None, peaks: vec![], path });
        Ok(id)
    }

    pub fn stage_chunk(&self, id: &str, data: &[u8]) -> Result<()> {
        use std::io::Write;
        let path = self.staged.lock().unwrap_or_else(|e| e.into_inner()).get(id).map(|s| s.path.clone()).ok_or_else(|| CoreError::local("Unknown file."))?;
        let mut f = std::fs::OpenOptions::new().append(true).open(path)?;
        f.write_all(data)?;
        Ok(())
    }

    pub fn stage_end(&self, id: &str) -> Result<Staged> {
        let s = self.staged.lock().unwrap_or_else(|e| e.into_inner()).remove(id).ok_or_else(|| CoreError::local("Unknown file."))?;
        self.describe(s.id, s.path, s.name)
    }

    pub fn unstage(&self, id: &str) {
        if let Some(s) = self.staged.lock().unwrap_or_else(|e| e.into_inner()).remove(id) {
            let _ = std::fs::remove_dir_all(s.path.parent().unwrap_or(&s.path));
        }
    }

    pub fn set_staged_meta(&self, id: &str, m: StagedMeta) {
        if let Some(s) = self.staged.lock().unwrap_or_else(|e| e.into_inner()).get_mut(id) {
            s.width = m.width.or(s.width);
            s.height = m.height.or(s.height);
            s.duration_ms = m.duration_ms.or(s.duration_ms);
            if m.poster_b64.as_ref().map(|p| p.len() <= 54 * 1024).unwrap_or(true) {
                s.poster_b64 = m.poster_b64.or(s.poster_b64.take());
            }
        }
    }

    pub fn staged_path(&self, id: &str) -> Option<(String, PathBuf)> {
        self.staged.lock().unwrap_or_else(|e| e.into_inner()).get(id).map(|s| (s.content_type.clone(), s.path.clone()))
    }

    /// Removes staging folders nothing refers to any more (not registered, not in the outbox).
    pub fn cleanup_staging(&self) {
        let Ok(rd) = std::fs::read_dir(self.staging_dir()) else { return };
        let keep: Vec<String> = {
            let mut v: Vec<String> = self.staged.lock().unwrap_or_else(|e| e.into_inner()).keys().cloned().collect();
            for o in self.store.outbox_all().unwrap_or_default() {
                for a in o.attachments {
                    if let Some(id) = Path::new(&a.path).parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()) {
                        v.push(id.to_string());
                    }
                }
            }
            v
        };
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if !keep.contains(&n) && n != "voice" {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }

    pub fn wipe_private_files(&self) {
        let _ = std::fs::remove_dir_all(self.staging_dir());
        let _ = std::fs::remove_dir_all(self.dir.join("local-avatars"));
        let _ = std::fs::remove_dir_all(self.dir.join("open"));
    }

    /// Moves an uploaded file into the media cache under its attachment id.
    pub(crate) fn adopt_upload(&self, att_id: &str, from: &Path, ct: &str, size: i64) -> Option<String> {
        let dir = self.media_dir();
        std::fs::create_dir_all(&dir).ok()?;
        let dest = dir.join(att_id);
        if std::fs::rename(from, &dest).is_err() {
            std::fs::copy(from, &dest).ok()?;
            let _ = std::fs::remove_file(from);
        }
        let p = dest.display().to_string();
        self.store.media_put(att_id, ct, size, &p).ok()?;
        Some(p)
    }

    /// Queues a message with files. The outbox uploads them (with progress), saves their details and sends the message.
    pub fn send_files(&self, conv: &str, ids: &[String], body: &str, thread_root: Option<String>, reply_to: Option<String>, also_send_to_channel: bool) -> Result<MessageView> {
        self.guard(conv)?;
        if ids.is_empty() {
            return Err(CoreError::local("Choose a file first."));
        }
        if self.store.get_conv(conv)?.is_none() {
            return Err(CoreError::local("This conversation is not available."));
        }
        let mut atts = vec![];
        {
            let mut reg = self.staged.lock().unwrap_or_else(|e| e.into_inner());
            for id in ids {
                let s = reg.remove(id).ok_or_else(|| CoreError::local("A file is no longer available. Add it again."))?;
                let voice = s.kind == "audio" && !s.peaks.is_empty();
                let meta = AttachmentMeta {
                    duration_ms: s.duration_ms,
                    peaks: if voice { s.peaks.clone() } else { vec![] },
                    width: s.width,
                    height: s.height,
                    poster_b64: s.poster_b64.clone(),
                    pages: s.pages,
                };
                let has_meta = meta.duration_ms.is_some() || !meta.peaks.is_empty() || meta.width.is_some() || meta.poster_b64.is_some() || meta.pages.is_some();
                atts.push(OutAtt { path: s.path.display().to_string(), name: s.name, content_type: s.content_type, size: s.size as i64, meta: has_meta.then_some(meta), att_id: None, meta_set: false });
            }
        }
        let body = body.trim_end().to_string();
        let names = crate::markup::mention_names(&body);
        let mentions: Vec<String> = if names.is_empty() { vec![] } else { self.store.all_users()?.into_iter().filter(|u| names.iter().any(|n| n.eq_ignore_ascii_case(&u.username))).map(|u| u.id).collect() };
        let item = OutboxItem {
            client_id: uuid::Uuid::new_v4().simple().to_string(),
            conv_id: conv.to_string(),
            also_send_to_channel: also_send_to_channel && thread_root.is_some(),
            thread_root,
            reply_to,
            body,
            mentions,
            ts: now_ms(),
            state: "queued".into(),
            attempts: 0,
            error: None,
            attachments: atts,
        };
        self.store.outbox_add(&item)?;
        let view = self.outbox_view(&item, self.pending_seq(&item));
        self.emit(CoreEvent::Message { message: view.clone(), replaces: None });
        self.emit_row(conv);
        self.outbox_wake.notify_one();
        Ok(view)
    }

    // ---------------------------------------------------------------- local files behind wcmedia ids

    /// Files that live on this device only: staged files (`st_`), files of unsent messages (`up_`), local avatars (`la_`).
    pub fn local_path(&self, id: &str) -> Option<(String, PathBuf)> {
        if id.starts_with("st_") {
            return self.staged_path(id);
        }
        if let Some(rest) = id.strip_prefix("up_") {
            let (client, idx) = rest.rsplit_once('_')?;
            let o = self.store.outbox_get(client).ok().flatten()?;
            let a = o.attachments.get(idx.parse::<usize>().ok()?)?;
            return Some((a.content_type.clone(), PathBuf::from(&a.path)));
        }
        if let Some(rest) = id.strip_prefix("la_") {
            let mut it = rest.splitn(3, '_');
            let (kind, target) = (it.next()?, it.next()?);
            let p = self.local_avatar_path(kind, target);
            return p.exists().then(|| ("image/jpeg".to_string(), p));
        }
        None
    }

    fn local_avatar_path(&self, kind: &str, target: &str) -> PathBuf {
        self.dir.join("local-avatars").join(format!("{kind}-{target}.img"))
    }

    // ---------------------------------------------------------------- voice

    pub fn voice_start(&self) -> Result<()> {
        let mut g = self.voice.lock().unwrap_or_else(|e| e.into_inner());
        if g.is_some() {
            return Err(CoreError::local("Already recording."));
        }
        if let Some(p) = self.player.get() {
            p.send(crate::player::Cmd::Stop);
        }
        let (id, dir) = self.new_staged_dir()?;
        let name = voice_name(now_ms());
        let s = crate::voice::start(&dir, &name, self.emit.read().unwrap_or_else(|e| e.into_inner()).clone()).inspect_err(|_| {
            let _ = std::fs::remove_dir_all(&dir);
        })?;
        // Remember the folder so `finish` can build the Staged entry.
        self.staged.lock().unwrap_or_else(|e| e.into_inner()).insert(
            id.clone(),
            Staged { id, name: name.clone(), content_type: "audio/ogg".into(), size: 0, kind: "voice".into(), width: None, height: None, duration_ms: None, pages: None, poster_b64: None, peaks: vec![], path: dir.join(&name) },
        );
        *g = Some(s);
        Ok(())
    }

    pub fn voice_finish(&self) -> Result<Staged> {
        let s = self.voice.lock().unwrap_or_else(|e| e.into_inner()).take().ok_or_else(|| CoreError::local("Not recording."))?;
        let res = s.finish();
        let pending: Option<Staged> = {
            let mut reg = self.staged.lock().unwrap_or_else(|e| e.into_inner());
            let id = reg.iter().find(|(_, v)| v.kind == "voice" && v.size == 0).map(|(k, _)| k.clone());
            id.and_then(|i| reg.remove(&i))
        };
        let pending = pending.ok_or_else(|| CoreError::local("Recording lost."))?;
        let r = match res {
            Ok(r) => r,
            Err(e) => {
                let _ = std::fs::remove_dir_all(pending.path.parent().unwrap_or(&pending.path));
                return Err(e);
            }
        };
        let size = std::fs::metadata(&r.path)?.len();
        if r.duration_ms < 500 {
            let _ = std::fs::remove_dir_all(pending.path.parent().unwrap_or(&pending.path));
            return Err(CoreError::local("Too short. Hold the microphone a little longer."));
        }
        let mut st = pending;
        st.size = size;
        st.kind = "audio".into();
        st.duration_ms = Some(r.duration_ms);
        st.peaks = r.peaks;
        Ok(self.register(st))
    }

    pub fn voice_cancel(&self) {
        if let Some(s) = self.voice.lock().unwrap_or_else(|e| e.into_inner()).take() {
            s.cancel();
        }
        let mut reg = self.staged.lock().unwrap_or_else(|e| e.into_inner());
        let ids: Vec<String> = reg.iter().filter(|(_, v)| v.kind == "voice" && v.size == 0).map(|(k, _)| k.clone()).collect();
        for i in ids {
            if let Some(p) = reg.remove(&i) {
                let _ = std::fs::remove_dir_all(p.path.parent().unwrap_or(&p.path));
            }
        }
    }

    // ---------------------------------------------------------------- playback

    pub fn player(&self) -> &crate::player::Player {
        self.player.get_or_init(|| crate::player::Player::new(self.emit.read().unwrap_or_else(|e| e.into_inner()).clone()))
    }

    pub async fn audio_play(&self, id: &str, speed: f32, from_ms: u32) -> Result<()> {
        let path = match self.local_path(id) {
            Some((_, p)) => p,
            None => PathBuf::from(crate::media::ensure_cached(self, id).await?.2),
        };
        self.player().send(crate::player::Cmd::Play { id: id.to_string(), path, speed, from_ms });
        Ok(())
    }

    // ---------------------------------------------------------------- saving and opening files

    /// Copies a file into `dir` (the Downloads folder) under its own name, never overwriting. Returns the new path.
    pub async fn export_attachment_to(&self, id: &str, name: &str, dir: &Path) -> Result<PathBuf> {
        let _ = std::fs::remove_dir_all(dir);
        self.export_attachment(id, name, dir).await
    }

    pub async fn export_attachment(&self, id: &str, name: &str, dir: &Path) -> Result<PathBuf> {
        let src = match self.local_path(id) {
            Some((_, p)) => p,
            None => PathBuf::from(crate::media::ensure_cached(self, id).await?.2),
        };
        std::fs::create_dir_all(dir)?;
        let name = safe_name(name);
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
            _ => (name.clone(), String::new()),
        };
        let mut dest = dir.join(&name);
        let mut n = 1;
        while dest.exists() {
            dest = dir.join(format!("{stem} ({n}){ext}"));
            n += 1;
        }
        std::fs::copy(&src, &dest)?;
        Ok(dest)
    }

    // ---------------------------------------------------------------- counts, threads, search, users

    pub async fn counts_loop(self: Arc<Self>, api: Arc<crate::api::Api>, gen: u64) {
        while self.current(gen) {
            self.counts_wake.notified().await;
            // Many events arrive in bursts: wait a moment and fetch once.
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            if !self.current(gen) {
                return;
            }
            if let Ok(c) = api.get::<Counts>("/me/counts", &[]).await {
                let _ = self.store.kv_set("counts", &c);
                self.emit(CoreEvent::Counts { counts: c });
            }
        }
    }

    pub async fn get_counts(&self) -> Result<Counts> {
        let c: Counts = self.api()?.get("/me/counts", &[]).await?;
        let _ = self.store.kv_set("counts", &c);
        Ok(self.adjust_counts(c))
    }

    pub async fn list_threads(&self, unread: bool, before: Option<i64>, limit: i64) -> Result<Vec<ThreadSummary>> {
        let mut q = vec![("limit", limit.to_string())];
        if unread {
            q.push(("unread", "true".into()));
        }
        if let Some(b) = before {
            q.push(("before", b.to_string()));
        }
        let hidden = self.hidden_ids();
        let list: Vec<ThreadSummary> = self.api()?.get("/threads", &q).await?;
        Ok(list.into_iter().filter(|t| !hidden.contains(&t.conv_id)).collect())
    }

    pub async fn follow_thread(&self, root: &str, on: bool) -> Result<()> {
        let api = self.api()?;
        let _ = api.send(if on { Method::PUT } else { Method::DELETE }, &format!("/threads/{root}/follow"), &[], None, &[]).await?;
        Ok(())
    }

    pub async fn read_thread(&self, root: &str) -> Result<()> {
        let api = self.api()?;
        let _ = api.send(Method::POST, &format!("/threads/{root}/read"), &[], None, &[]).await?;
        if let Some(mut t) = self.store.all_threads()?.into_iter().find(|t| t.root.id == root) {
            t.unread = 0;
            self.store.upsert_thread(&t)?;
            self.emit(CoreEvent::Thread { thread: t });
        }
        self.counts_wake.notify_one();
        Ok(())
    }

    pub async fn search(&self, q: &str, conv: Option<String>, limit: i64) -> Result<SearchResponse> {
        let mut query = vec![("q", q.to_string()), ("limit", limit.to_string())];
        if let Some(c) = conv {
            query.push(("in", c));
        }
        let mut r: SearchResponse = self.api()?.get("/search", &query).await?;
        let hidden = self.hidden_ids();
        if !hidden.is_empty() {
            r.hits.retain(|h| !hidden.contains(&h.message.conv_id));
            r.files.retain(|f| !hidden.contains(&f.conv_id));
            r.channels.retain(|c| !hidden.contains(&c.conversation.id));
        }
        Ok(r)
    }

    pub async fn list_users(&self, q: Option<String>) -> Result<Vec<User>> {
        let mut query = vec![("active", "true".to_string())];
        if let Some(q) = q.filter(|q| !q.trim().is_empty()) {
            query.push(("q", q));
        }
        let users: Vec<User> = self.api()?.get("/users", &query).await?;
        for u in &users {
            let _ = self.store.upsert_user(u);
        }
        Ok(users)
    }

    // ---------------------------------------------------------------- message actions

    pub async fn forward_message(&self, id: &str, conv_ids: Vec<String>, comment: Option<String>) -> Result<()> {
        if conv_ids.is_empty() {
            return Err(CoreError::local("Choose at least one chat."));
        }
        let req = ForwardRequest { conv_ids, comment: comment.filter(|c| !c.trim().is_empty()) };
        let _ = self.api()?.send(Method::POST, &format!("/messages/{id}/forward"), &[], Some(serde_json::to_value(&req)?), &[]).await?;
        Ok(())
    }

    pub fn pin_ids(&self) -> BTreeMap<String, Vec<String>> {
        let map: BTreeMap<String, Vec<Pin>> = self.store.kv_get("pins").ok().flatten().unwrap_or_default();
        let hidden = crate::sync::hidden_messages(&self.store);
        map.into_iter().map(|(k, v)| (k, v.into_iter().map(|p| p.message_id).filter(|id| !hidden.contains(id)).collect())).collect()
    }

    pub async fn pin_message(&self, conv: &str, id: &str, on: bool) -> Result<()> {
        let api = self.api()?;
        let pins: Vec<Pin> = api.json(if on { Method::PUT } else { Method::DELETE }, &format!("/conversations/{conv}/pins/{id}"), &[], None).await?;
        let mut map: BTreeMap<String, Vec<Pin>> = self.store.kv_get("pins")?.unwrap_or_default();
        map.insert(conv.to_string(), pins.clone());
        self.store.kv_set("pins", &map)?;
        let hidden = crate::sync::hidden_messages(&self.store);
        self.emit(CoreEvent::Pins { conv_id: conv.to_string(), ids: pins.iter().map(|p| p.message_id.clone()).filter(|id| !hidden.contains(id)).collect() });
        Ok(())
    }

    /// The pinned messages of a conversation, newest pin first.
    pub async fn get_pins(&self, conv: &str) -> Result<Vec<MessageView>> {
        self.guard(conv)?;
        let api = self.api()?;
        let mut pins: Vec<Pin> = api.get(&format!("/conversations/{conv}/pins"), &[]).await.unwrap_or_else(|_| {
            let map: BTreeMap<String, Vec<Pin>> = self.store.kv_get("pins").ok().flatten().unwrap_or_default();
            map.get(conv).cloned().unwrap_or_default()
        });
        pins.sort_by_key(|p| std::cmp::Reverse(p.ts));
        let mut out = vec![];
        let hidden = crate::sync::hidden_messages(&self.store);
        for p in pins.into_iter().filter(|p| !hidden.contains(&p.message_id)) {
            let m = match self.store.get_message(&p.message_id)? {
                Some(m) => Some(m),
                None => api.get::<Message>(&format!("/messages/{}", p.message_id), &[]).await.ok(),
            };
            if let Some(m) = m.filter(|m| !m.deleted) {
                out.push(MessageView::server(m));
            }
        }
        Ok(out)
    }

    pub async fn save_message(&self, id: &str, on: bool) -> Result<()> {
        let api = self.api()?;
        let body = if on { Some(serde_json::json!({})) } else { None };
        let _ = api.send(if on { Method::PUT } else { Method::DELETE }, &format!("/saved/{id}"), &[], body, &[]).await?;
        let mut list: Vec<SavedItem> = self.store.kv_get("saved")?.unwrap_or_default();
        list.retain(|s| s.message_id != id);
        if on {
            let conv = self.store.get_message(id)?.map(|m| m.conv_id).unwrap_or_default();
            list.push(SavedItem { message_id: id.to_string(), conv_id: conv, saved_ms: now_ms(), label_ids: vec![] });
        }
        self.store.kv_set("saved", &list)?;
        self.emit(CoreEvent::Saved { ids: list.iter().map(|s| s.message_id.clone()).collect() });
        self.counts_wake.notify_one();
        Ok(())
    }

    /// Loads one message by id (for "open in context" from search and files).
    pub async fn fetch_message(&self, id: &str) -> Result<MessageView> {
        if let Some(m) = self.store.get_message(id)? {
            self.guard(&m.conv_id)?;
            return Ok(MessageView::server(m));
        }
        let m: Message = self.api()?.get(&format!("/messages/{id}"), &[]).await?;
        self.guard(&m.conv_id)?;
        Ok(MessageView::server(m))
    }

    /// Messages around `seq` of a conversation, to show a search hit in its context.
    pub async fn messages_around(&self, conv: &str, around_seq: i64) -> Result<Vec<MessageView>> {
        self.guard(conv)?;
        let api = self.api()?;
        let before: HistoryResponse = api.get(&format!("/conversations/{conv}/messages"), &[("limit", "30".into()), ("before", (around_seq + 8).to_string())]).await?;
        for m in &before.messages {
            self.store.upsert_message(m)?;
        }
        Ok(before.messages.into_iter().filter(|m| m.thread_root.is_none() || m.also_send_to_channel).map(MessageView::server).collect())
    }

    // ---------------------------------------------------------------- avatars

    /// Uploads the staged picture (already cropped by the UI) and sets it as `me` or as a channel / group picture.
    pub async fn set_avatar(&self, kind: &str, target: Option<String>, staged_id: &str) -> Result<()> {
        let api = self.api()?;
        let (_, path) = self.staged_path(staged_id).ok_or_else(|| CoreError::local("The picture is no longer available."))?;
        let att = api.upload_file(&path, "avatar.jpg", "image/jpeg", Some("avatar"), Default::default(), Arc::new(|_| {})).await?;
        self.unstage(staged_id);
        match kind {
            "me" => {
                let me: Me = api.json(Method::PATCH, "/me", &[], Some(serde_json::json!({"avatar": att.id}))).await?;
                self.store.kv_set("me", &me)?;
                self.store.upsert_user(&me.user)?;
                self.emit(CoreEvent::Me { me: me.clone() });
                self.emit(CoreEvent::User { user: me.user });
            }
            "conv" => {
                let conv = target.ok_or_else(|| CoreError::local("No chat chosen."))?;
                // The server answers with the updated chat (there is no GET for a single chat).
                let cs: ConversationSummary = api.json(Method::PUT, &format!("/conversations/{conv}/avatar"), &[], Some(serde_json::to_value(&SetConvAvatar { avatar: att.id })?)).await?;
                self.store.upsert_conv(&cs)?;
                self.emit_row(&conv);
            }
            _ => return Err(CoreError::local("Unknown picture target.")),
        }
        Ok(())
    }

    pub async fn remove_avatar(&self, kind: &str, target: Option<String>) -> Result<()> {
        let api = self.api()?;
        match kind {
            "me" => {
                let me: Me = api.json(Method::PATCH, "/me", &[], Some(serde_json::json!({"avatar": ""}))).await?;
                self.store.kv_set("me", &me)?;
                self.store.upsert_user(&me.user)?;
                self.emit(CoreEvent::Me { me: me.clone() });
                self.emit(CoreEvent::User { user: me.user });
            }
            "conv" => {
                let conv = target.ok_or_else(|| CoreError::local("No chat chosen."))?;
                let cs: ConversationSummary = api.json(Method::PUT, &format!("/conversations/{conv}/avatar"), &[], Some(serde_json::to_value(&SetConvAvatar { avatar: String::new() })?)).await?;
                {
                    self.store.upsert_conv(&cs)?;
                }
                self.emit_row(&conv);
            }
            _ => return Err(CoreError::local("Unknown picture target.")),
        }
        Ok(())
    }

    pub async fn update_me(&self, req: UpdateMeRequest) -> Result<Me> {
        let me: Me = self.api()?.json(Method::PATCH, "/me", &[], Some(serde_json::to_value(&req)?)).await?;
        self.store.kv_set("me", &me)?;
        self.store.upsert_user(&me.user)?;
        self.emit(CoreEvent::Me { me: me.clone() });
        self.emit(CoreEvent::User { user: me.user.clone() });
        Ok(me)
    }

    fn emit_all_rows(&self) {
        if let Ok(convs) = self.store.all_convs() {
            for cs in convs {
                self.emit_row(&cs.conversation.id);
            }
        }
    }

    /// A picture kept on this device only, for a person (`user`) or a chat (`conv`). Wins over every other picture.
    pub fn set_local_avatar(&self, kind: &str, target: &str, staged_id: &str) -> Result<()> {
        if !matches!(kind, "user" | "conv") || target.is_empty() || !target.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(CoreError::local("Unknown picture target."));
        }
        let (_, src) = self.staged_path(staged_id).ok_or_else(|| CoreError::local("The picture is no longer available."))?;
        let dest = self.local_avatar_path(kind, target);
        std::fs::create_dir_all(dest.parent().unwrap_or(&self.dir))?;
        std::fs::copy(&src, &dest)?;
        self.unstage(staged_id);
        let mut list: Vec<LocalAvatar> = self.store.kv_get("local_avatars")?.unwrap_or_default();
        list.retain(|a| !(a.kind == kind && a.target == target));
        list.push(LocalAvatar { kind: kind.into(), target: target.into(), version: now_ms() });
        self.store.kv_set("local_avatars", &list)?;
        self.emit(CoreEvent::LocalAvatars { items: list });
        self.emit_all_rows();
        Ok(())
    }

    pub fn remove_local_avatar(&self, kind: &str, target: &str) -> Result<()> {
        let _ = std::fs::remove_file(self.local_avatar_path(kind, target));
        let mut list: Vec<LocalAvatar> = self.store.kv_get("local_avatars")?.unwrap_or_default();
        list.retain(|a| !(a.kind == kind && a.target == target));
        self.store.kv_set("local_avatars", &list)?;
        self.emit(CoreEvent::LocalAvatars { items: list });
        self.emit_all_rows();
        Ok(())
    }

    // ---------------------------------------------------------------- creating chats

    pub async fn create_conversation(&self, kind: &str, name: &str, topic: Option<String>, members: Vec<String>, visibility: Option<String>, avatar_staged: Option<String>) -> Result<String> {
        let api = self.api()?;
        let kind = ConversationKind::parse(kind).ok_or_else(|| CoreError::local("Unknown chat type."))?;
        // The server adds the creator (as the chat's manager); `members` lists the other people.
        let me = self.me_id();
        let members: Vec<String> = members.into_iter().filter(|m| *m != me).collect();
        let visibility = if kind == ConversationKind::Channel { visibility.as_deref().and_then(ChannelVisibility::parse) } else { None };
        let req = CreateConversation { kind, name: name.trim().to_string(), topic: topic.filter(|t| !t.trim().is_empty()), members, visibility, include_creator: None };
        let cs: ConversationSummary = api.json(Method::POST, "/conversations", &[], Some(serde_json::to_value(&req)?)).await?;
        self.store.upsert_conv(&cs)?;
        self.emit_row(&cs.conversation.id);
        if let Some(sid) = avatar_staged {
            // A failed picture must not undo the chat that now exists.
            let _ = self.set_avatar("conv", Some(cs.conversation.id.clone()), &sid).await;
        }
        Ok(cs.conversation.id)
    }

    pub async fn add_members(&self, conv: &str, user_ids: Vec<String>) -> Result<()> {
        let _ = self.api()?.send(Method::POST, &format!("/conversations/{conv}/members"), &[], Some(serde_json::json!({"user_ids": user_ids})), &[]).await?;
        Ok(())
    }

    /// The organisation settings (cached from the bootstrap and the live event; fetched when missing).
    pub async fn org_settings(&self) -> Result<OrgSettings> {
        let s: OrgSettings = self.api()?.get("/org-settings", &[]).await?;
        self.store.kv_set("org_settings", &s)?;
        self.emit(CoreEvent::OrgSettings { settings: s.clone() });
        Ok(s)
    }

    pub async fn remove_member(&self, conv: &str, user_id: &str) -> Result<()> {
        let _ = self.api()?.send(Method::DELETE, &format!("/conversations/{conv}/members/{user_id}"), &[], None, &[]).await?;
        // The server sends the updated summary; read it now so the list is right at once.
        self.refresh_conversation(conv).await
    }

    /// Re-reads one conversation (`GET /conversations/{id}`) into the cache.
    pub async fn refresh_conversation(&self, conv: &str) -> Result<()> {
        let cs: ConversationSummary = self.api()?.get(&format!("/conversations/{conv}"), &[]).await?;
        self.store.upsert_conv(&cs)?;
        self.emit_row(conv);
        Ok(())
    }

    /// `GET /channels/browse`: public channels the person has not joined.
    pub async fn browse_channels(&self, q: &str, limit: u32, offset: u32) -> Result<ChannelBrowsePage> {
        let mut query = vec![("limit", limit.clamp(1, 200).to_string()), ("offset", offset.to_string())];
        if !q.trim().is_empty() {
            query.push(("q", q.trim().to_string()));
        }
        self.api()?.get("/channels/browse", &query).await
    }

    pub async fn join_channel(&self, conv: &str) -> Result<String> {
        let cs: ConversationSummary = self.api()?.json(Method::POST, &format!("/conversations/{conv}/join"), &[], None).await?;
        self.store.upsert_conv(&cs)?;
        self.emit_row(&cs.conversation.id);
        self.counts_wake.notify_one();
        Ok(cs.conversation.id)
    }

    pub async fn leave_conversation(&self, conv: &str) -> Result<()> {
        let _: serde_json::Value = self.api()?.json(Method::POST, &format!("/conversations/{conv}/leave"), &[], None).await?;
        self.store.remove_conv(conv)?;
        self.emit(CoreEvent::ChatRemove { id: conv.to_string() });
        self.counts_wake.notify_one();
        Ok(())
    }

    /// "Delete for me": the message disappears on all of this person's devices; other people keep it.
    pub async fn hide_message(&self, id: &str) -> Result<()> {
        if id.starts_with("local:") {
            return self.discard_message(id);
        }
        let r: HideMessageResponse = self.api()?.json(Method::POST, &format!("/messages/{id}/hide"), &[], None).await?;
        let ids = vec![r.message_id.clone()];
        self.store.delete_messages(&ids)?;
        crate::sync::remember_hidden(&self.store, &ids);
        self.emit(CoreEvent::MessageRemove { conv_id: r.conv_id.clone(), id: r.message_id });
        self.emit_row(&r.conv_id);
        self.counts_wake.notify_one();
        Ok(())
    }

    pub async fn update_conversation(&self, conv: &str, req: UpdateConversation) -> Result<()> {
        let cs: ConversationSummary = self.api()?.json(Method::PATCH, &format!("/conversations/{conv}"), &[], Some(serde_json::to_value(&req)?)).await?;
        self.store.upsert_conv(&cs)?;
        self.emit_row(conv);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_names_follow_the_documented_pattern() {
        // 2026-11-21 14:30:05 UTC
        assert_eq!(voice_name(1_795_271_405_000), "voice-20261121-143005.ogg");
        assert_eq!(voice_name(0), "voice-19700101-000000.ogg");
    }

    #[test]
    fn names_are_made_safe() {
        assert_eq!(safe_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_name("C:\\x\\report.pdf"), "report.pdf");
        assert_eq!(safe_name("   "), "file");
        assert_eq!(safe_name(".hidden"), "hidden");
    }

    // ---- what the server sends for the new C2 endpoints (shapes taken from docs/API.md and a live server)

    #[test]
    fn counts_parse_and_default_missing_fields() {
        let c: Counts = serde_json::from_str(
            r#"{"conversations":16,"dms":6,"channels":10,"unread_conversations":4,"unread_dms":2,"unread_channels":2,"unread_messages":18,"mentions":1,"threads":15,"threads_unread":3,"files":106,"files_image":46,"files_video":0,"files_audio":17,"files_document":43,"saved":2}"#,
        )
        .unwrap();
        assert_eq!((c.dms, c.channels, c.threads, c.files), (6, 10, 15, 106));
        assert_eq!(c.files_image + c.files_video + c.files_audio + c.files_document, 106);
    }

    #[test]
    fn thread_list_item_parses_with_last_reply() {
        let t: ThreadSummary = serde_json::from_str(
            r#"{"conv_id":"c1","root":{"id":"m1","conv_id":"c1","seq":4,"sender_id":"u2","body":"Plan?","kind":"text","ts":10,"version":1},"reply_count":3,"last_reply_ts":99,"unread":2,"following":true,"participants":["u1","u2"],
               "last_reply":{"id":"m9","conv_id":"c1","seq":9,"sender_id":"u1","body":"Yes","kind":"text","ts":99,"version":1,"thread_root":"m1"}}"#,
        )
        .unwrap();
        assert_eq!((t.reply_count, t.unread, t.following), (3, 2, true));
        assert_eq!(t.last_reply.unwrap().thread_root.as_deref(), Some("m1"));
        assert_eq!(t.participants.len(), 2);
    }

    #[test]
    fn file_listing_parses_media_meta() {
        let hits: Vec<FileHit> = serde_json::from_str(
            r#"[{"attachment":{"id":"a1","name":"walk.mp4","content_type":"video/mp4","size":2690456,"meta":{"width":1280,"height":720,"duration_ms":8000,"poster_b64":"/9j/4AAQ"}},"message_id":"m1","conv_id":"c1","sender_id":"u2","ts":5},
               {"attachment":{"id":"a2","name":"voice-1.ogg","content_type":"audio/ogg","size":9000,"meta":{"duration_ms":5000,"peaks":[1,200,30]}},"message_id":"m2","conv_id":"c1","sender_id":"u2","ts":6}]"#,
        )
        .unwrap();
        assert_eq!(hits.len(), 2);
        let m = hits[0].attachment.meta.as_ref().unwrap();
        assert_eq!((m.width, m.height, m.duration_ms), (Some(1280), Some(720), Some(8000)));
        assert_eq!(hits[1].attachment.meta.as_ref().unwrap().peaks, vec![1, 200, 30]);
    }
}
