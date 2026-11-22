//! The engine ties the pieces together: sign-in, the sync loop (bootstrap, WebSocket, REST catch-up), the outbox and
//! the operations the UI asks for. It owns no UI types: everything the UI should know is pushed through `emit`.

use crate::api::{self, now_ms, Api, CLIENT_VERSION};
use crate::error::{CoreError, Result};
use crate::markup;
use crate::store::{OutboxItem, Store};
use crate::sync::{apply_events, apply_sent};
use crate::features::Staged;
use crate::store::OutAtt;
use crate::view::{build_row, ChatRow, CoreEvent, MessageView, RowCtx, Snapshot, UploadInfo};
use futures_util::{SinkExt, StreamExt};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use proto::*;
use reqwest::Method;
use serde::Serialize;
use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;
use tokio::sync::{mpsc, Notify};
use tokio_tungstenite::tungstenite::Message as WsMessage;

pub type Emit = Arc<dyn Fn(CoreEvent) + Send + Sync>;
pub type Notifier = Arc<dyn Fn(crate::notify::Notice) + Send + Sync>;

#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub messages: Vec<MessageView>,
    pub has_more_before: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub version: String,
    pub signed_in: bool,
    pub server: Option<String>,
    pub username: Option<String>,
    pub connection: String,
    pub data_dir: String,
    pub platform: String,
    pub arch: String,
    pub portable: bool,
    pub appimage: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerCheck {
    pub url: String,
    pub name: String,
    pub server_version: String,
    pub max_upload_bytes: u64,
}

enum WsEnd {
    /// The connection ended; `online` says whether it had been established.
    Closed { online: bool },
    /// Drop the cursor and bootstrap again.
    Resync,
    /// The session is over (the person was signed out).
    Fatal,
}

pub struct Engine {
    pub dir: PathBuf,
    pub portable: bool,
    pub store: Arc<Store>,
    pub(crate) emit: RwLock<Emit>,
    pub(crate) notifier: RwLock<Option<Notifier>>,
    /// The locked area is open (PIN entered).
    pub(crate) lock_open: AtomicBool,
    pub(crate) last_active: AtomicI64,
    /// (window focused, open conversation) as reported by the UI.
    pub(crate) ui: Mutex<(bool, Option<String>)>,
    pub(crate) tz_offset_min: AtomicI32,
    pub(crate) update_cancel: AtomicBool,
    pub(crate) started: AtomicBool,
    /// Started by a service without the app window.
    pub(crate) headless: AtomicBool,
    pub(crate) rt: tokio::runtime::Handle,
    api: RwLock<Option<Arc<Api>>>,
    conn: Mutex<String>,
    gen: AtomicU64,
    pub(crate) outbox_wake: Notify,
    reconnect_wake: Notify,
    ws_out: Mutex<Option<mpsc::UnboundedSender<ClientWsMsg>>>,
    pub(crate) counts_wake: Notify,
    pub(crate) staged: Mutex<HashMap<String, Staged>>,
    pub(crate) progress: Mutex<HashMap<String, UploadInfo>>,
    pub(crate) cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub(crate) voice: Mutex<Option<crate::voice::VoiceSession>>,
    pub(crate) player: OnceLock<crate::player::Player>,
}

fn tls_config() -> Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    Arc::new(
        rustls::ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
            .expect("tls versions")
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

fn enc(s: &str) -> String {
    utf8_percent_encode(s, NON_ALPHANUMERIC).to_string()
}

impl Engine {
    pub fn new(dir: PathBuf, emit: Emit, rt: tokio::runtime::Handle) -> Result<Arc<Engine>> {
        Self::with_portable(dir, emit, rt, false)
    }

    pub fn with_portable(dir: PathBuf, emit: Emit, rt: tokio::runtime::Handle, portable: bool) -> Result<Arc<Engine>> {
        std::fs::create_dir_all(&dir)?;
        let store = Arc::new(Store::open(&dir.join("cache.db"))?);
        Ok(Arc::new(Engine {
            dir,
            portable,
            tz_offset_min: AtomicI32::new(store.kv_get::<i32>("tz_offset").ok().flatten().unwrap_or(0)),
            store,
            emit: RwLock::new(emit),
            notifier: RwLock::new(None),
            lock_open: AtomicBool::new(false),
            last_active: AtomicI64::new(now_ms()),
            ui: Mutex::new((false, None)),
            update_cancel: AtomicBool::new(false),
            started: AtomicBool::new(false),
            headless: AtomicBool::new(true),
            rt,
            api: RwLock::new(None),
            conn: Mutex::new("offline".into()),
            gen: AtomicU64::new(0),
            outbox_wake: Notify::new(),
            reconnect_wake: Notify::new(),
            ws_out: Mutex::new(None),
            counts_wake: Notify::new(),
            staged: Mutex::new(HashMap::new()),
            progress: Mutex::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
            voice: Mutex::new(None),
            player: OnceLock::new(),
        }))
    }

    /// Sends an event to the UI. Events about locked chats are dropped while the locked area is closed.
    pub fn emit(&self, e: CoreEvent) {
        if let Some(e) = self.filter_event(e) {
            let f = self.emit.read().unwrap_or_else(|e| e.into_inner()).clone();
            f(e);
        }
    }

    /// Replaces where events go (the app window attaches to an engine that was started without one).
    pub fn set_emit(&self, e: Emit) {
        *self.emit.write().unwrap_or_else(|x| x.into_inner()) = e;
    }

    pub fn set_notifier(&self, n: Option<Notifier>) {
        *self.notifier.write().unwrap_or_else(|x| x.into_inner()) = n;
    }

    pub fn api(&self) -> Result<Arc<Api>> {
        self.api.read().unwrap_or_else(|e| e.into_inner()).clone().ok_or(CoreError::NotSignedIn)
    }

    pub fn me_id(&self) -> String {
        self.store.kv_get::<String>("user_id").ok().flatten().unwrap_or_default()
    }

    fn set_conn(&self, state: &str, detail: Option<String>) {
        *self.conn.lock().unwrap_or_else(|e| e.into_inner()) = state.to_string();
        self.emit(CoreEvent::Conn { state: state.to_string(), detail });
    }

    pub fn connection(&self) -> String {
        self.conn.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn media_dir(&self) -> PathBuf {
        self.dir.join("media")
    }

    // ---------------------------------------------------------------- start, sign-in, sign-out

    /// Called once at app start: resumes a saved session.
    pub fn start(self: &Arc<Self>) {
        self.started.store(true, Ordering::SeqCst);
        if let Some(s) = api::load_session(&self.dir) {
            match Api::new(&s.server, &self.dir) {
                Ok(api) => {
                    api.set_session(Some(s));
                    self.attach(Arc::new(api));
                }
                Err(e) => tracing::warn!("cannot resume session: {e}"),
            }
        }
    }

    pub fn app_info(&self) -> AppInfo {
        let s = self.api().ok().and_then(|a| a.session());
        AppInfo {
            version: CLIENT_VERSION.to_string(),
            signed_in: s.is_some(),
            server: s.as_ref().map(|s| s.server.clone()),
            username: s.map(|s| s.username),
            connection: self.connection(),
            data_dir: self.dir.display().to_string(),
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            portable: self.portable,
            appimage: std::env::var_os("APPIMAGE").is_some(),
        }
    }

    /// Installs `api` as the current account and starts the background loops.
    fn attach(self: &Arc<Self>, api: Arc<Api>) {
        let gen = self.gen.fetch_add(1, Ordering::SeqCst) + 1;
        let weak = Arc::downgrade(self);
        api.set_fatal_handler(Arc::new(move |code, message| {
            if let Some(e) = weak.upgrade() {
                e.kicked(code, message);
            }
        }));
        *self.api.write().unwrap_or_else(|e| e.into_inner()) = Some(api.clone());
        self.set_conn("connecting", None);
        let me = self.clone();
        let a2 = api.clone();
        self.rt.spawn(async move { me.run_loop(a2, gen).await });
        let me = self.clone();
        let a3 = api.clone();
        self.rt.spawn(async move { me.outbox_loop(api, gen).await });
        let me = self.clone();
        self.rt.spawn(async move { me.counts_loop(a3, gen).await });
        self.cleanup_staging();
    }

    /// Checks that `input` is a WorkChat server and returns its normalised address.
    pub async fn check_server(&self, input: &str) -> Result<ServerCheck> {
        let url = api::normalize_server_url(input)?;
        let api = Api::new(&url, &self.dir)?;
        let m = api.meta().await?;
        Ok(ServerCheck { url, name: m.name, server_version: m.server_version, max_upload_bytes: m.max_upload_bytes })
    }

    pub async fn login_password(self: &Arc<Self>, server: &str, username: &str, password: &str) -> Result<Me> {
        if username.trim().is_empty() || password.is_empty() {
            return Err(CoreError::local("Enter your username and password."));
        }
        let url = api::normalize_server_url(server)?;
        let api = Arc::new(Api::new(&url, &self.dir)?);
        let lr = api.login(username, password).await?;
        self.finish_login(api, lr)
    }

    /// `input` is a pasted login link (`https://host/l/<token>`, `workchat://login?...`) or a bare token.
    /// A bare token needs `server_hint`.
    pub async fn login_link(self: &Arc<Self>, input: &str, server_hint: Option<&str>) -> Result<Me> {
        let parsed = proto::rules::parse_login_link(input).ok_or_else(|| CoreError::local("That does not look like a WorkChat login link."))?;
        let server = match (parsed.server, server_hint.filter(|s| !s.trim().is_empty())) {
            (Some(s), _) => s,
            (None, Some(h)) => h.to_string(),
            (None, None) => return Err(CoreError::local("This login code has no server address. Enter the server address too.")),
        };
        let url = api::normalize_server_url(&server)?;
        let api = Arc::new(Api::new(&url, &self.dir)?);
        let lr = api.login_with_link(&parsed.token).await?;
        self.finish_login(api, lr)
    }

    fn finish_login(self: &Arc<Self>, api: Arc<Api>, lr: LoginResponse) -> Result<Me> {
        // A different account must never see the previous one's cache or unsent messages.
        let previous: Option<String> = self.store.kv_get("user_id")?;
        let same = previous.as_deref() == Some(lr.me.user.id.as_str()) && self.store.kv_get::<String>("server")?.as_deref() == Some(api.base.as_str());
        if !same {
            self.store.wipe()?;
            let _ = std::fs::remove_dir_all(self.media_dir());
            self.wipe_private_files();
        }
        self.store.kv_set("user_id", &lr.me.user.id)?;
        self.store.kv_set("server", &api.base)?;
        self.store.kv_set("me", &lr.me)?;
        self.attach(api);
        Ok(lr.me)
    }

    pub async fn sign_out(self: &Arc<Self>) {
        if let Ok(api) = self.api() {
            // Self-service sign-out can be disabled by an admin; local sign-out happens anyway.
            let _ = tokio::time::timeout(Duration::from_secs(5), api.logout()).await;
        }
        self.wipe_local("signed_out", "You have been signed out.");
    }

    /// Account disabled, session revoked, password reset, history cleared: forget everything on this device.
    fn kicked(&self, code: ErrorCode, message: String) {
        self.wipe_local(&code.as_str(), &message);
    }

    pub fn wipe_local(&self, code: &str, message: &str) {
        self.gen.fetch_add(1, Ordering::SeqCst);
        self.lock_open.store(false, Ordering::SeqCst);
        *self.api.write().unwrap_or_else(|e| e.into_inner()) = None;
        *self.ws_out.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let _ = api::save_session(&self.dir, None);
        let _ = self.store.wipe();
        let _ = std::fs::remove_dir_all(self.media_dir());
        self.wipe_private_files();
        self.set_conn("offline", None);
        self.emit(CoreEvent::SignedOut { code: code.to_string(), message: message.to_string() });
    }

    pub fn headless(&self) -> bool {
        self.headless.load(Ordering::SeqCst)
    }

    /// The app window took over the engine.
    pub fn attach_window(&self) {
        self.headless.store(false, Ordering::SeqCst);
    }

    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    /// Stops the background loops (the session stays).
    pub fn stop(&self) {
        self.gen.fetch_add(1, Ordering::SeqCst);
        self.started.store(false, Ordering::SeqCst);
        *self.ws_out.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// A wake-up push arrived and the app is not running its loops: fetch what is new, once, without a WebSocket.
    /// Notifications go out through the notifier while events are applied. Returns the number of events applied.
    /// The API client of the saved session, loading it from disk when the engine has not signed in yet this run
    /// (a push receiver or the keep-alive service starts the process without the app window).
    pub fn load_api(&self) -> Result<Arc<Api>> {
        match self.api() {
            Ok(a) => Ok(a),
            Err(_) => {
                let s = api::load_session(&self.dir).ok_or(CoreError::NotSignedIn)?;
                let a = Arc::new(Api::new(&s.server, &self.dir)?);
                a.set_session(Some(s));
                *self.api.write().unwrap_or_else(|e| e.into_inner()) = Some(a.clone());
                Ok(a)
            }
        }
    }

    pub async fn wake_once(self: &Arc<Self>) -> Result<usize> {
        let api = self.load_api()?;
        if self.store.cursor().is_none() {
            // Nothing to compare with yet: just make the cache current, without a burst of notifications.
            self.bootstrap(&api).await?;
            return Ok(0);
        }
        let before = self.store.cursor().unwrap_or(0);
        self.rest_catch_up(&api).await?;
        Ok((self.store.cursor().unwrap_or(0) - before).max(0) as usize)
    }

    pub(crate) fn current(&self, gen: u64) -> bool {
        self.gen.load(Ordering::SeqCst) == gen
    }

    pub fn reconnect_now(&self) {
        self.reconnect_wake.notify_one();
        self.outbox_wake.notify_one();
    }

    // ---------------------------------------------------------------- sync loop

    async fn run_loop(self: Arc<Self>, api: Arc<Api>, gen: u64) {
        let mut backoff = 1u64;
        while self.current(gen) {
            if self.store.cursor().is_none() {
                self.set_conn("connecting", None);
                if let Err(e) = self.bootstrap(&api).await {
                    if !self.current(gen) {
                        return;
                    }
                    self.set_conn("offline", Some(e.to_string()));
                    self.sleep_or_wake(backoff).await;
                    backoff = (backoff * 2).min(30);
                    continue;
                }
            }
            match self.ws_session(&api, gen).await {
                WsEnd::Fatal => return,
                WsEnd::Resync => {
                    let _ = self.store.clear_cursor();
                    continue;
                }
                WsEnd::Closed { online } => {
                    if !self.current(gen) {
                        return;
                    }
                    if online {
                        backoff = 1;
                    }
                }
            }
            self.set_conn("offline", None);
            // The WebSocket may be blocked (proxy); the REST log still keeps the app current.
            if let Err(e) = self.rest_catch_up(&api).await {
                if matches!(e, CoreError::Api { code: ErrorCode::CursorExpired, .. }) {
                    let _ = self.store.clear_cursor();
                    continue;
                }
            }
            self.sleep_or_wake(backoff).await;
            backoff = (backoff * 2).min(30);
        }
    }

    async fn sleep_or_wake(&self, secs: u64) {
        let jitter = rand::random::<u64>() % 500;
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(secs * 1000 + jitter)) => {}
            _ = self.reconnect_wake.notified() => {}
        }
    }

    async fn bootstrap(&self, api: &Api) -> Result<()> {
        let b: Bootstrap = api.get("/sync/bootstrap", &[]).await?;
        self.store.replace_bootstrap(&b)?;
        self.store.kv_set("server", &api.base)?;
        if let Ok(m) = api.meta().await {
            let _ = self.store.kv_set("max_upload", &m.max_upload_bytes);
        }
        self.emit(CoreEvent::Reset);
        self.outbox_wake.notify_one();
        self.counts_wake.notify_one();
        Ok(())
    }

    async fn rest_catch_up(&self, api: &Api) -> Result<()> {
        let mut cursor = self.store.cursor().unwrap_or(0);
        loop {
            let r: EventsResponse = api.get("/events", &[("after", cursor.to_string()), ("limit", "500".into())]).await?;
            let more = r.has_more;
            let had = !r.events.is_empty();
            self.handle_events(api, &r.events).await?;
            cursor = r.cursor;
            if !more || !had {
                return Ok(());
            }
        }
    }

    /// Applies events, pushes UI updates, and refreshes summaries the cache could not compute itself.
    async fn handle_events(&self, api: &Api, events: &[Event]) -> Result<bool> {
        let applied = apply_events(&self.store, events)?;
        if applied.counts_dirty {
            self.counts_wake.notify_one();
        }
        for e in applied.events {
            self.emit(e);
        }
        for m in &applied.new_messages {
            self.notify_new(m);
        }
        for id in &applied.refresh {
            if let Ok(cs) = api.get::<ConversationSummary>(&format!("/conversations/{id}"), &[]).await {
                let _ = self.store.upsert_conv(&cs);
                self.emit_row(&cs.conversation.id);
            }
        }
        Ok(applied.resync)
    }

    pub(crate) fn emit_row(&self, conv_id: &str) {
        if let Ok(Some(cs)) = self.store.get_conv(conv_id) {
            if let Ok(ctx) = RowCtx::load(&self.store) {
                if let Ok(row) = build_row(&self.store, &ctx, &cs) {
                    self.emit(CoreEvent::Chat { row });
                }
            }
        }
    }

    async fn ws_session(&self, api: &Arc<Api>, gen: u64) -> WsEnd {
        use tokio_tungstenite::tungstenite::Error as WsError;
        if api.ensure_fresh().await.is_err() && !self.current(gen) {
            return WsEnd::Fatal;
        }
        let url = api.ws_url();
        let connector = tokio_tungstenite::Connector::Rustls(tls_config());
        let connect = tokio::time::timeout(Duration::from_secs(12), tokio_tungstenite::connect_async_tls_with_config(url, None, false, Some(connector)));
        let (mut ws, _) = match connect.await {
            Ok(Ok(c)) => c,
            Ok(Err(WsError::Http(r))) if r.status().as_u16() == 401 => {
                let _ = api.refresh().await;
                return WsEnd::Closed { online: false };
            }
            _ => return WsEnd::Closed { online: false },
        };
        let (tx, mut rx) = mpsc::unbounded_channel::<ClientWsMsg>();
        *self.ws_out.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        let token = match api.bearer() {
            Ok(t) => t,
            Err(_) => return WsEnd::Fatal,
        };
        let hello = ClientWsMsg::Hello { token, cursor: self.store.cursor().unwrap_or(0), client_version: CLIENT_VERSION.into() };
        if ws.send(WsMessage::text(serde_json::to_string(&hello).unwrap_or_default())).await.is_err() {
            return WsEnd::Closed { online: false };
        }
        let mut online = false;
        let mut ping = tokio::time::interval(Duration::from_secs(25));
        ping.tick().await;
        let mut last_rx = tokio::time::Instant::now();
        let end = loop {
            if !self.current(gen) {
                break WsEnd::Fatal;
            }
            tokio::select! {
                msg = ws.next() => {
                    last_rx = tokio::time::Instant::now();
                    let msg = match msg { Some(Ok(m)) => m, _ => break WsEnd::Closed { online } };
                    match msg {
                        WsMessage::Text(t) => {
                            let Ok(sm) = serde_json::from_str::<ServerWsMsg>(t.as_str()) else { continue };
                            match sm {
                                ServerWsMsg::HelloOk { .. } => {
                                    online = true;
                                    self.set_conn("online", None);
                                    self.outbox_wake.notify_one();
                                }
                                ServerWsMsg::Events { events, .. } => {
                                    let last = events.last().map(|e| e.cursor);
                                    match self.handle_events(api, &events).await {
                                        Ok(true) => break WsEnd::Resync,
                                        Ok(false) => {}
                                        Err(e) => tracing::warn!("applying events: {e}"),
                                    }
                                    if let Some(c) = last {
                                        let _ = ws.send(WsMessage::text(serde_json::to_string(&ClientWsMsg::Ack { cursor: c }).unwrap_or_default())).await;
                                    }
                                }
                                ServerWsMsg::Typing { conv_id, user_id } => self.emit(CoreEvent::Typing { conv_id, user_id }),
                                ServerWsMsg::Presence { user_id, presence, last_active_ms } => self.on_presence(user_id, presence, last_active_ms),
                                ServerWsMsg::Pong => {}
                                ServerWsMsg::Error { code, message } => {
                                    if code.requires_wipe() {
                                        self.kicked(code, message);
                                        break WsEnd::Fatal;
                                    }
                                    match code {
                                        ErrorCode::CursorExpired => break WsEnd::Resync,
                                        ErrorCode::TokenExpired | ErrorCode::Unauthorized => {
                                            let _ = api.refresh().await;
                                            break WsEnd::Closed { online };
                                        }
                                        _ => break WsEnd::Closed { online },
                                    }
                                }
                            }
                        }
                        WsMessage::Close(frame) => {
                            if let Some(f) = frame {
                                let code: u16 = f.code.into();
                                match code {
                                    proto::close::UNAUTHORIZED => {
                                        if api.refresh().await.is_err() && self.api().is_err() {
                                            break WsEnd::Fatal;
                                        }
                                    }
                                    proto::close::SESSION_ENDED | proto::close::FORBIDDEN => {
                                        // The server ended the session without a coded error: probe once to get the reason.
                                        if let Err(CoreError::Api { code, message }) = api.get::<Me>("/me", &[]).await {
                                            if code.requires_wipe() {
                                                // the fatal handler has already wiped
                                                let _ = message;
                                                break WsEnd::Fatal;
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            break WsEnd::Closed { online };
                        }
                        WsMessage::Ping(p) => {
                            let _ = ws.send(WsMessage::Pong(p)).await;
                        }
                        _ => {}
                    }
                }
                out = rx.recv() => {
                    match out {
                        Some(m) => {
                            if ws.send(WsMessage::text(serde_json::to_string(&m).unwrap_or_default())).await.is_err() {
                                break WsEnd::Closed { online };
                            }
                        }
                        None => break WsEnd::Closed { online },
                    }
                }
                _ = ping.tick() => {
                    if last_rx.elapsed() > Duration::from_secs(75) {
                        break WsEnd::Closed { online };
                    }
                    if ws.send(WsMessage::text(serde_json::to_string(&ClientWsMsg::Ping).unwrap_or_default())).await.is_err() {
                        break WsEnd::Closed { online };
                    }
                }
                _ = self.reconnect_wake.notified() => {}
            }
        };
        *self.ws_out.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let _ = ws.close(None).await;
        end
    }

    fn on_presence(&self, user_id: String, presence: Presence, last_active_ms: Option<i64>) {
        if let Ok(Some(mut u)) = self.store.get_user(&user_id) {
            u.presence = Some(presence);
            if last_active_ms.is_some() {
                u.last_active_ms = last_active_ms;
            }
            let _ = self.store.upsert_user(&u);
            if let Ok(convs) = self.store.all_convs() {
                for cs in convs.iter().filter(|c| c.conversation.kind == ConversationKind::Dm && c.conversation.members.iter().any(|m| m.user_id == user_id)) {
                    self.emit_row(&cs.conversation.id);
                }
            }
        }
        self.emit(CoreEvent::Presence { user_id, presence, last_active_ms });
    }

    pub fn typing(&self, conv_id: &str) {
        if let Some(tx) = self.ws_out.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let _ = tx.send(ClientWsMsg::Typing { conv_id: conv_id.to_string() });
        }
    }

    // ---------------------------------------------------------------- outbox

    async fn outbox_loop(self: Arc<Self>, api: Arc<Api>, gen: u64) {
        let mut backoff = 2u64;
        while self.current(gen) {
            let items = self.store.outbox_queued().unwrap_or_default();
            let mut transient = false;
            for it in items {
                if !self.current(gen) {
                    return;
                }
                match self.send_outbox_item(&api, &it).await {
                    Ok(()) => {}
                    Err(e) if e.is_transient() => {
                        let _ = self.store.outbox_mark(&it.client_id, "queued", Some(&e.to_string()), true);
                        transient = true;
                        break;
                    }
                    Err(CoreError::NotSignedIn) => return,
                    Err(e) => {
                        if self.store.outbox_get(&it.client_id).ok().flatten().is_none() {
                            continue; // discarded or cancelled by the person
                        }
                        let _ = self.store.outbox_mark(&it.client_id, "failed", Some(&e.to_string()), true);
                        self.progress.lock().unwrap_or_else(|e| e.into_inner()).remove(&it.client_id);
                        if let Ok(Some(o)) = self.store.outbox_get(&it.client_id) {
                            let seq = self.pending_seq(&o);
                            self.emit(CoreEvent::Message { message: self.outbox_view(&o, seq), replaces: None });
                        }
                        self.emit_row(&it.conv_id);
                    }
                }
            }
            let wait = if transient { backoff } else { 60 };
            backoff = if transient { (backoff * 2).min(30) } else { 2 };
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(wait)) => {}
                _ = self.outbox_wake.notified() => { backoff = 2; }
            }
        }
    }

    pub(crate) fn outbox_view(&self, o: &OutboxItem, seq: i64) -> MessageView {
        let info = self.progress.lock().unwrap_or_else(|e| e.into_inner()).get(&o.client_id).cloned();
        let total: u64 = o.attachments.iter().map(|a| a.size.max(0) as u64).sum();
        let info = match (info, o.attachments.is_empty()) {
            (_, true) => None,
            (Some(i), _) if o.state != "failed" => Some(i),
            _ => Some(UploadInfo { index: 0, count: o.attachments.len(), sent: 0, total, state: if o.state == "failed" { "failed".into() } else { "waiting".into() } }),
        };
        MessageView::from_outbox(o, &self.me_id(), seq).with_upload(info)
    }

    pub(crate) fn set_progress(&self, o: &OutboxItem, index: usize, sent: u64, total: u64, state: &str) {
        let info = UploadInfo { index, count: o.attachments.len(), sent, total, state: state.to_string() };
        self.progress.lock().unwrap_or_else(|e| e.into_inner()).insert(o.client_id.clone(), info.clone());
        self.emit(CoreEvent::Upload { local_id: o.local_id(), info });
    }

    async fn send_outbox_item(&self, api: &Api, it: &OutboxItem) -> Result<()> {
        let mut atts: Vec<OutAtt> = it.attachments.clone();
        if !atts.is_empty() {
            let cancel = self.cancels.lock().unwrap_or_else(|e| e.into_inner()).entry(it.client_id.clone()).or_default().clone();
            let total: u64 = atts.iter().map(|a| a.size.max(0) as u64).sum();
            let mut done: u64 = atts.iter().filter(|a| a.att_id.is_some()).map(|a| a.size.max(0) as u64).sum();
            for i in 0..atts.len() {
                if cancel.load(Ordering::SeqCst) {
                    return Err(CoreError::local("Upload cancelled."));
                }
                if atts[i].att_id.is_none() {
                    let p = std::path::PathBuf::from(&atts[i].path);
                    if !p.exists() {
                        return Err(CoreError::local(format!("{} is no longer on this device, so it cannot be sent.", atts[i].name)));
                    }
                    self.set_progress(it, i, done, total, "uploading");
                    let last = Arc::new(AtomicU64::new(0));
                    let (me_emit, item, base) = (self.emit.read().unwrap_or_else(|e| e.into_inner()).clone(), it.clone(), done);
                    let progress: Arc<dyn Fn(u64) + Send + Sync> = Arc::new(move |sent| {
                        let now = now_ms() as u64;
                        let prev = last.load(Ordering::Relaxed);
                        if now.saturating_sub(prev) >= 120 {
                            last.store(now, Ordering::Relaxed);
                            me_emit(CoreEvent::Upload { local_id: item.local_id(), info: UploadInfo { index: i, count: item.attachments.len(), sent: base + sent, total, state: "uploading".into() } });
                        }
                    });
                    let att = api.upload_file(&p, &atts[i].name, &atts[i].content_type, None, cancel.clone(), progress).await?;
                    atts[i].att_id = Some(att.id.clone());
                    // Keep our copy as the cache entry of the uploaded file: it opens instantly and needs no download.
                    if let Some(np) = self.adopt_upload(&att.id, &p, &atts[i].content_type, atts[i].size) {
                        atts[i].path = np;
                    }
                    self.store.outbox_set_attachments(&it.client_id, &atts)?;
                }
                if let (Some(meta), Some(id), false) = (atts[i].meta.clone(), atts[i].att_id.clone(), atts[i].meta_set) {
                    let _: serde_json::Value = api.json(Method::PUT, &format!("/files/{id}/meta"), &[], Some(serde_json::to_value(&meta)?)).await?;
                    atts[i].meta_set = true;
                    self.store.outbox_set_attachments(&it.client_id, &atts)?;
                }
                done += atts[i].size.max(0) as u64;
            }
            self.set_progress(it, atts.len().saturating_sub(1), total, total, "sending");
        }
        let req = SendMessage {
            client_id: it.client_id.clone(),
            body: it.body.clone(),
            thread_root: it.thread_root.clone(),
            also_send_to_channel: it.also_send_to_channel && it.thread_root.is_some(),
            reply_to: it.reply_to.clone(),
            mentions: it.mentions.clone(),
            attachment_ids: atts.iter().filter_map(|a| a.att_id.clone()).collect(),
        };
        let m: Message = api.json(Method::POST, &format!("/conversations/{}/messages", it.conv_id), &[], Some(serde_json::to_value(&req)?)).await?;
        for e in apply_sent(&self.store, &m)? {
            self.emit(e);
        }
        self.progress.lock().unwrap_or_else(|e| e.into_inner()).remove(&it.client_id);
        self.cancels.lock().unwrap_or_else(|e| e.into_inner()).remove(&it.client_id);
        self.cleanup_staging();
        Ok(())
    }

    pub(crate) fn pending_seq(&self, o: &OutboxItem) -> i64 {
        self.store.get_conv(&o.conv_id).ok().flatten().map(|c| c.conversation.last_seq).unwrap_or(0) + 1
    }

    // ---------------------------------------------------------------- queries for the UI

    pub fn snapshot(&self) -> Result<Snapshot> {
        let api = self.api()?;
        let me: Me = self.store.kv_get("me")?.ok_or_else(|| CoreError::local("Not synced yet."))?;
        let ctx = RowCtx::load(&self.store)?;
        let hidden = self.hidden_ids();
        let mut rows = vec![];
        for cs in self.store.all_convs()?.into_iter().filter(|cs| !hidden.contains(&cs.conversation.id)) {
            rows.push(build_row(&self.store, &ctx, &cs)?);
        }
        let saved: Vec<SavedItem> = self.store.kv_get("saved")?.unwrap_or_default();
        let saved_labels = crate::view::saved_labels(&saved);
        let saved_ids = self.visible_saved(saved.into_iter().map(|s| s.message_id).collect());
        Ok(Snapshot {
            me,
            server: api.base.clone(),
            users: self.store.all_users()?,
            rows,
            threads: self.store.all_threads()?.into_iter().filter(|t| !hidden.contains(&t.conv_id)).collect(),
            connection: self.connection(),
            counts: self.store.kv_get::<Counts>("counts")?.map(|c| self.adjust_counts(c)),
            org_settings: self.store.kv_get::<OrgSettings>("org_settings")?.unwrap_or_default(),
            labels: self.store.kv_get("labels")?.unwrap_or_default(),
            saved_labels: saved_labels.into_iter().filter(|(id, _)| saved_ids.contains(id)).collect(),
            saved: saved_ids,
            local_avatars: self.store.kv_get("local_avatars")?.unwrap_or_default(),
            max_upload_bytes: self.max_upload(),
            pins: self.pin_ids().into_iter().filter(|(k, _)| !hidden.contains(k)).collect(),
        })
    }

    fn page(&self, conv: &str, before: Option<i64>, limit: i64, with_pending: bool) -> Result<Page> {
        let msgs = self.store.messages_page(conv, before, limit)?;
        let mut out: Vec<MessageView> = msgs.into_iter().map(MessageView::server).collect();
        if with_pending {
            let base = self.store.get_conv(conv)?.map(|c| c.conversation.last_seq).unwrap_or(0);
            for (i, o) in self.store.outbox_for_conv(conv)?.into_iter().filter(|o| o.thread_root.is_none() || o.also_send_to_channel).enumerate() {
                out.push(self.outbox_view(&o, base + 1 + i as i64));
            }
        }
        // Older messages may also sit in the cache without the server flag saying so (they arrived as live events).
        let first = out.iter().find(|m| m.pending.is_none()).map(|m| m.message.seq);
        let older_cached = first.and_then(|f| self.store.min_seq(conv).map(|min| min < f)).unwrap_or(false);
        Ok(Page { messages: out, has_more_before: self.store.has_more_before(conv) || older_cached })
    }

    async fn fetch_history(&self, conv: &str, before: Option<i64>, limit: i64) -> Result<()> {
        let api = self.api()?;
        let mut q = vec![("limit", limit.to_string())];
        if let Some(b) = before {
            q.push(("before", b.to_string()));
        }
        let h: HistoryResponse = api.get(&format!("/conversations/{conv}/messages"), &q).await?;
        for m in &h.messages {
            self.store.upsert_message(m)?;
        }
        self.store.set_has_more_before(conv, h.has_more_before)?;
        Ok(())
    }

    pub async fn open_conversation(&self, conv: &str, limit: i64) -> Result<Page> {
        self.guard(conv)?;
        let have = self.store.messages_page(conv, None, limit)?;
        if (have.len() as i64) < limit && self.store.has_more_before(conv) {
            let before = have.first().map(|m| m.seq);
            // Offline is fine: show what the cache has.
            let _ = self.fetch_history(conv, before, limit).await;
        }
        self.page(conv, None, limit, true)
    }

    pub async fn load_older(&self, conv: &str, before_seq: i64, limit: i64) -> Result<Page> {
        self.guard(conv)?;
        let have = self.store.messages_page(conv, Some(before_seq), limit)?;
        if (have.len() as i64) < limit && self.store.has_more_before(conv) {
            let before = have.first().map(|m| m.seq).unwrap_or(before_seq);
            self.fetch_history(conv, Some(before), limit).await?;
        }
        self.page(conv, Some(before_seq), limit, false)
    }

    // ---------------------------------------------------------------- actions

    pub fn send_message(&self, conv: &str, body: &str, thread_root: Option<String>, reply_to: Option<String>, also_send_to_channel: bool) -> Result<MessageView> {
        self.guard(conv)?;
        let body = body.trim_end().to_string();
        if body.trim().is_empty() {
            return Err(CoreError::local("Write a message first."));
        }
        if self.store.get_conv(conv)?.is_none() {
            return Err(CoreError::local("This conversation is not available."));
        }
        // @username in the text become structured mentions.
        let names = markup::mention_names(&body);
        let mentions: Vec<String> = if names.is_empty() {
            vec![]
        } else {
            self.store.all_users()?.into_iter().filter(|u| names.iter().any(|n| n.eq_ignore_ascii_case(&u.username))).map(|u| u.id).collect()
        };
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
            attachments: vec![],
        };
        self.store.outbox_add(&item)?;
        let view = self.outbox_view(&item, self.pending_seq(&item));
        self.emit(CoreEvent::Message { message: view.clone(), replaces: None });
        self.emit_row(conv);
        self.outbox_wake.notify_one();
        Ok(view)
    }

    pub fn retry_message(&self, local_id: &str) -> Result<()> {
        let cid = local_id.strip_prefix("local:").unwrap_or(local_id);
        if let Some(o) = self.store.outbox_get(cid)? {
            self.cancels.lock().unwrap_or_else(|e| e.into_inner()).remove(cid);
            self.store.outbox_mark(cid, "queued", None, false)?;
            if let Some(o) = self.store.outbox_get(cid)? {
                let v = self.outbox_view(&o, self.pending_seq(&o));
                self.emit(CoreEvent::Message { message: v, replaces: None });
            }
            let _ = o;
            self.outbox_wake.notify_one();
        }
        Ok(())
    }

    pub fn discard_message(&self, local_id: &str) -> Result<()> {
        let cid = local_id.strip_prefix("local:").unwrap_or(local_id);
        if let Some(c) = self.cancels.lock().unwrap_or_else(|e| e.into_inner()).get(cid) {
            c.store(true, Ordering::SeqCst);
        }
        if let Some(o) = self.store.outbox_get(cid)? {
            self.store.outbox_delete(cid)?;
            self.progress.lock().unwrap_or_else(|e| e.into_inner()).remove(cid);
            self.cancels.lock().unwrap_or_else(|e| e.into_inner()).remove(cid);
            self.cleanup_staging();
            self.emit(CoreEvent::MessageRemove { conv_id: o.conv_id.clone(), id: o.local_id() });
            self.emit_row(&o.conv_id);
        }
        Ok(())
    }

    fn store_changed(&self, m: Message) -> MessageView {
        let _ = self.store.upsert_message(&m);
        let conv = m.conv_id.clone();
        let v = MessageView::server(m);
        self.emit(CoreEvent::Message { message: v.clone(), replaces: None });
        self.emit_row(&conv);
        v
    }

    pub async fn edit_message(&self, id: &str, body: &str) -> Result<MessageView> {
        let api = self.api()?;
        let body = body.trim_end().to_string();
        if body.trim().is_empty() {
            return Err(CoreError::local("A message cannot be empty. Delete it instead."));
        }
        let names = markup::mention_names(&body);
        let mentions: Vec<String> = self.store.all_users()?.into_iter().filter(|u| names.iter().any(|n| n.eq_ignore_ascii_case(&u.username))).map(|u| u.id).collect();
        let m: Message = api.json(Method::PATCH, &format!("/messages/{id}"), &[], Some(serde_json::to_value(&EditMessage { body, mentions })?)).await?;
        Ok(self.store_changed(m))
    }

    pub async fn delete_message(&self, id: &str) -> Result<MessageView> {
        if id.starts_with("local:") {
            self.discard_message(id)?;
            return Err(CoreError::local("Removed the unsent message."));
        }
        let api = self.api()?;
        let m: Message = api.json(Method::DELETE, &format!("/messages/{id}"), &[], None).await?;
        Ok(self.store_changed(m))
    }

    pub async fn react(&self, id: &str, emoji: &str, add: bool) -> Result<MessageView> {
        let api = self.api()?;
        let m: Message = api.json(if add { Method::PUT } else { Method::DELETE }, &format!("/messages/{id}/reactions/{}", enc(emoji)), &[], None).await?;
        Ok(self.store_changed(m))
    }

    pub async fn mark_read(&self, conv: &str) -> Result<()> {
        let Some(cs) = self.store.get_conv(conv)? else { return Ok(()) };
        if cs.unread == 0 && !cs.marked_unread {
            return Ok(());
        }
        // Optimistic: the badge disappears at once; the server's answer is authoritative.
        let mut local = cs.clone();
        local.unread = 0;
        local.mentions = 0;
        local.marked_unread = false;
        local.last_read_seq = local.conversation.last_seq;
        self.store.upsert_conv(&local)?;
        self.emit_row(conv);
        let api = self.api()?;
        let r: ConversationSummary = api.json(Method::POST, &format!("/conversations/{conv}/read"), &[], Some(serde_json::to_value(&MarkReadRequest { seq: cs.conversation.last_seq })?)).await?;
        self.store.upsert_conv(&r)?;
        self.emit_row(conv);
        Ok(())
    }

    pub async fn set_favourite(&self, conv: &str, favourite: bool) -> Result<()> {
        let api = self.api()?;
        let p: ConvPref = api.json(Method::PUT, &format!("/conversations/{conv}/prefs"), &[], Some(serde_json::to_value(&ConvPrefRequest { favourite: Some(favourite), ..Default::default() })?)).await?;
        self.store.upsert_pref(&p)?;
        self.emit_row(conv);
        Ok(())
    }

    /// Saves a manual order (the pinned section's "Reorder").
    pub async fn order_conversations(&self, ids: Vec<String>) -> Result<()> {
        let api = self.api()?;
        let prefs: Vec<ConvPref> = api.json(Method::POST, "/conversations/order", &[], Some(serde_json::to_value(&ConvOrderRequest { conv_ids: ids })?)).await?;
        for p in prefs {
            self.store.upsert_pref(&p)?;
            self.emit_row(&p.conv_id);
        }
        Ok(())
    }

    pub async fn create_dm(&self, user_id: &str) -> Result<String> {
        let api = self.api()?;
        let cs: ConversationSummary = api.json(Method::POST, "/conversations/dm", &[], Some(serde_json::to_value(&CreateDm { user_id: user_id.to_string() })?)).await?;
        self.store.upsert_conv(&cs)?;
        self.emit_row(&cs.conversation.id);
        Ok(cs.conversation.id)
    }

    pub async fn list_files(&self, kind: Option<String>, q: Option<String>, offset: Option<i64>, limit: i64) -> Result<Vec<FileHit>> {
        let api = self.api()?;
        let mut query = vec![("limit", limit.to_string())];
        if let Some(k) = kind.filter(|k| !k.is_empty()) {
            query.push(("type", k));
        }
        if let Some(q) = q.filter(|q| !q.trim().is_empty()) {
            query.push(("q", q));
        }
        if let Some(o) = offset {
            query.push(("offset", o.to_string()));
        }
        let hits: Vec<FileHit> = api.get("/files", &query).await?;
        let hidden = self.hidden_ids();
        Ok(hits.into_iter().filter(|h| !hidden.contains(&h.conv_id)).collect())
    }

    pub async fn get_thread(&self, conv: &str, root: &str) -> Result<ThreadView> {
        self.guard(conv)?;
        let api = self.api()?;
        let t: ThreadView = api.get(&format!("/conversations/{conv}/threads/{root}"), &[]).await?;
        for m in &t.replies {
            let _ = self.store.upsert_message(m);
        }
        Ok(t)
    }

    pub fn chat_row(&self, conv: &str) -> Result<Option<ChatRow>> {
        match self.store.get_conv(conv)? {
            Some(cs) => Ok(Some(build_row(&self.store, &RowCtx::load(&self.store)?, &cs)?)),
            None => Ok(None),
        }
    }
}
