//! A small raw HTTP + WebSocket client for testing workchat-server. It is not a chat client: it keeps no local
//! state, caches nothing and only wraps the documented API with `proto` types, so a test reads like the API doc.
//!
//! * [`Client`] is one signed-in person (or an admin, or an anonymous caller): every method is one API call.
//! * [`Ws`] is the live event stream (`/api/v1/ws`).
//! * [`eventually!`] polls a condition (with `.await` allowed inside) until it holds.

use futures_util::{SinkExt, StreamExt};
use proto::*;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use reqwest::Method as HttpMethod;

/// Polls `$cond` (an expression that may use `.await`) every 50 ms for up to 10 s; panics with `$what` on timeout.
#[macro_export]
macro_rules! eventually {
    ($what:expr, $cond:expr) => {{
        let mut ok = false;
        for _ in 0..200 {
            if $cond {
                ok = true;
                break;
            }
            ::tokio::time::sleep(::std::time::Duration::from_millis(50)).await;
        }
        assert!(ok, "timed out waiting for: {}", $what);
    }};
}

/// An error answer from the server (or a transport problem, then `status` is 0).
#[derive(Clone, Debug)]
pub struct ApiErr {
    pub status: u16,
    pub code: Option<ErrorCode>,
    pub message: String,
}

impl std::fmt::Display for ApiErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {} {:?}: {}", self.status, self.code, self.message)
    }
}

impl std::error::Error for ApiErr {}

pub type Res<T> = Result<T, ApiErr>;

/// A complete HTTP answer.
pub struct Resp {
    pub status: u16,
    pub headers: reqwest::header::HeaderMap,
    pub body: bytes::Bytes,
}

impl Resp {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    pub fn header(&self, name: &str) -> String {
        self.headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or("").to_string()
    }
    pub fn into_result<T: DeserializeOwned>(self) -> Res<T> {
        if (200..300).contains(&self.status) {
            serde_json::from_slice(&self.body).map_err(|e| ApiErr { status: self.status, code: None, message: format!("bad JSON ({e}): {}", self.text()) })
        } else {
            Err(self.error())
        }
    }
    pub fn error(&self) -> ApiErr {
        match serde_json::from_slice::<ErrorBody>(&self.body) {
            Ok(b) => ApiErr { status: self.status, code: Some(b.error.code), message: b.error.message },
            Err(_) => ApiErr { status: self.status, code: None, message: self.text() },
        }
    }
}

#[derive(Clone, Default)]
struct Tokens {
    access: String,
    refresh: String,
}

/// One caller of the API. Cheap to clone (clones share the tokens).
#[derive(Clone)]
pub struct Client {
    pub base: String,
    pub http: reqwest::Client,
    tokens: Arc<Mutex<Tokens>>,
    /// Filled by the login helpers.
    pub user_id: String,
    pub username: String,
    pub session_id: String,
    /// Sent as `X-Client-Version` when not empty.
    pub client_version: String,
}

fn http() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(120)).build().unwrap()
}

impl Client {
    /// Not signed in (for the public endpoints and for checking refusals).
    pub fn anon(base: &str) -> Client {
        Client {
            base: base.trim_end_matches('/').to_string(),
            http: http(),
            tokens: Arc::new(Mutex::new(Tokens::default())),
            user_id: String::new(),
            username: String::new(),
            session_id: String::new(),
            client_version: String::new(),
        }
    }

    fn adopt(base: &str, l: LoginResponse) -> Client {
        let mut c = Client::anon(base);
        *c.tokens.lock().unwrap() = Tokens { access: l.tokens.access_token, refresh: l.tokens.refresh_token };
        c.user_id = l.me.user.id.clone();
        c.username = l.me.user.username.clone();
        c.session_id = l.session_id;
        c
    }

    pub async fn setup(base: &str, req: &SetupRequest) -> Res<Client> {
        let l: LoginResponse = Client::anon(base).post("/setup", req).await?;
        Ok(Client::adopt(base, l))
    }

    /// Signs in with a password; `device` is the name shown in the sessions list.
    pub async fn login_device(base: &str, username: &str, password: &str, device: &str) -> Res<Client> {
        let req = LoginRequest { username: username.into(), password: password.into(), device_name: device.into(), client_version: "1.0.0".into() };
        let l: LoginResponse = Client::anon(base).post("/login", &req).await?;
        Ok(Client::adopt(base, l))
    }

    pub async fn login(base: &str, username: &str, password: &str) -> Res<Client> {
        Client::login_device(base, username, password, "test-device").await
    }

    /// Redeems a one-time login link token (`LinkLoginRequest`).
    pub async fn login_with_token(base: &str, token: &str) -> Res<Client> {
        let l: LoginResponse = Client::anon(base).post("/login/link", &json!({"token": token, "device_name": "test-device", "client_version": "1.0.0"})).await?;
        Ok(Client::adopt(base, l))
    }

    pub fn access_token(&self) -> String {
        self.tokens.lock().unwrap().access.clone()
    }
    pub fn refresh_token(&self) -> String {
        self.tokens.lock().unwrap().refresh.clone()
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}{}", self.base, API_PREFIX, path)
    }

    // ------------------------------------------------------------ raw access

    /// Sends the request and returns the answer whatever its status.
    pub async fn raw(&self, method: Method, path: &str, body: Option<Value>) -> Resp {
        self.raw_url(method, &self.url(path), body, &[]).await
    }

    pub async fn raw_url(&self, method: Method, url: &str, body: Option<Value>, headers: &[(&str, &str)]) -> Resp {
        let mut rb = self.http.request(method, url);
        let t = self.access_token();
        if !t.is_empty() {
            rb = rb.bearer_auth(t);
        }
        if !self.client_version.is_empty() {
            rb = rb.header("X-Client-Version", &self.client_version);
        }
        for (k, v) in headers {
            rb = rb.header(*k, *v);
        }
        if let Some(b) = body {
            rb = rb.json(&b);
        }
        Self::finish(rb).await
    }

    async fn finish(rb: reqwest::RequestBuilder) -> Resp {
        match rb.send().await {
            Ok(r) => {
                let status = r.status().as_u16();
                let headers = r.headers().clone();
                let body = r.bytes().await.unwrap_or_default();
                Resp { status, headers, body }
            }
            Err(e) => Resp { status: 0, headers: Default::default(), body: format!("transport error: {e}").into() },
        }
    }

    pub async fn call<T: DeserializeOwned>(&self, method: Method, path: &str, body: Option<Value>) -> Res<T> {
        self.raw(method, path, body).await.into_result()
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Res<T> {
        self.call(Method::GET, path, None).await
    }
    pub async fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Res<T> {
        self.call(Method::POST, path, Some(serde_json::to_value(body).unwrap())).await
    }
    pub async fn put<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Res<T> {
        self.call(Method::PUT, path, Some(serde_json::to_value(body).unwrap())).await
    }
    pub async fn put_empty<T: DeserializeOwned>(&self, path: &str) -> Res<T> {
        self.call(Method::PUT, path, Some(json!({}))).await
    }
    pub async fn patch<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Res<T> {
        self.call(Method::PATCH, path, Some(serde_json::to_value(body).unwrap())).await
    }
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Res<T> {
        self.call(Method::DELETE, path, None).await
    }
    /// POST with an empty JSON object body.
    pub async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Res<T> {
        self.call(Method::POST, path, Some(json!({}))).await
    }

    /// The error code of a failed call (panics when the call succeeded).
    pub async fn code_of(&self, method: Method, path: &str, body: Option<Value>) -> ErrorCode {
        let r = self.raw(method, path, body).await;
        assert!(!(200..300).contains(&r.status), "expected an error, got HTTP {}: {}", r.status, r.text());
        r.error().code.unwrap_or_else(|| panic!("error without a code: HTTP {} {}", r.status, r.text()))
    }

    // ------------------------------------------------------------ account

    pub async fn me(&self) -> Res<Me> {
        self.get("/me").await
    }

    /// Exchanges the refresh token for a new access token (the refresh token stays).
    pub async fn refresh(&self) -> Res<()> {
        let r: TokenPair = Client::anon(&self.base).post("/refresh", &RefreshRequest { refresh_token: self.refresh_token() }).await?;
        self.tokens.lock().unwrap().access = r.access_token;
        Ok(())
    }

    pub async fn change_password(&self, current: Option<&str>, new: &str) -> Res<Value> {
        self.post("/me/password", &json!({"current_password": current, "new_password": new})).await
    }

    pub async fn sessions(&self) -> Res<Vec<SessionInfo>> {
        self.get("/sessions").await
    }

    pub async fn logout(&self) -> Res<Value> {
        self.post_empty("/logout").await
    }

    // ------------------------------------------------------------ chat

    pub async fn bootstrap(&self) -> Res<Bootstrap> {
        self.get("/sync/bootstrap").await
    }

    /// Events after `cursor` (the same log the WebSocket streams).
    pub async fn events(&self, cursor: i64) -> Res<EventsResponse> {
        self.get(&format!("/events?after={cursor}")).await
    }

    /// The newest cursor right now.
    pub async fn cursor(&self) -> i64 {
        self.bootstrap().await.unwrap().cursor
    }

    pub async fn conversations(&self) -> Res<Vec<ConversationSummary>> {
        self.get("/conversations").await
    }

    pub async fn conversation(&self, conv: &str) -> Res<ConversationSummary> {
        self.get(&format!("/conversations/{conv}")).await
    }

    pub async fn has_conversation(&self, conv: &str) -> bool {
        self.conversations().await.unwrap().iter().any(|c| c.conversation.id == conv)
    }

    pub async fn open_dm(&self, user_id: &str) -> Res<String> {
        let c: ConversationSummary = self.post("/conversations/dm", &json!({"user_id": user_id})).await?;
        Ok(c.conversation.id)
    }

    pub async fn self_conv(&self) -> String {
        let c: ConversationSummary = self.post_empty("/conversations/self").await.unwrap();
        c.conversation.id
    }

    pub async fn history_raw(&self, conv: &str, query: &str) -> Res<HistoryResponse> {
        self.get(&format!("/conversations/{conv}/messages?{query}")).await
    }

    /// The latest page of top-level history, oldest first.
    pub async fn history(&self, conv: &str) -> Vec<Message> {
        self.history_raw(conv, "limit=200").await.unwrap().messages
    }

    /// Message bodies of `history`.
    pub async fn texts(&self, conv: &str) -> Vec<String> {
        self.history(conv).await.into_iter().map(|m| m.body).collect()
    }

    pub async fn send_req(&self, conv: &str, req: &SendMessage) -> Res<Message> {
        self.post(&format!("/conversations/{conv}/messages"), req).await
    }

    /// Sends a text message (fresh idempotency key) and returns the stored message.
    pub async fn send(&self, conv: &str, body: &str) -> Message {
        self.try_send(conv, body).await.unwrap()
    }

    pub async fn try_send(&self, conv: &str, body: &str) -> Res<Message> {
        self.send_req(conv, &SendMessage { client_id: uid(), body: body.into(), thread_root: None, reply_to: None, mentions: vec![], attachment_ids: vec![], also_send_to_channel: false }).await
    }

    pub async fn reply(&self, conv: &str, root: &str, body: &str) -> Message {
        self.send_req(conv, &SendMessage { client_id: uid(), body: body.into(), thread_root: Some(root.into()), reply_to: None, mentions: vec![], attachment_ids: vec![], also_send_to_channel: false }).await.unwrap()
    }

    pub async fn send_with_files(&self, conv: &str, body: &str, attachment_ids: &[String]) -> Res<Message> {
        self.send_req(conv, &SendMessage { client_id: uid(), body: body.into(), thread_root: None, reply_to: None, mentions: vec![], attachment_ids: attachment_ids.to_vec(), also_send_to_channel: false }).await
    }

    pub async fn edit(&self, mid: &str, body: &str) -> Res<Message> {
        self.patch(&format!("/messages/{mid}"), &json!({"body": body})).await
    }

    pub async fn delete_message(&self, mid: &str) -> Res<Value> {
        self.delete(&format!("/messages/{mid}")).await
    }

    pub async fn react(&self, mid: &str, emoji: &str, on: bool) -> Res<Value> {
        let enc: String = url::form_urlencoded::byte_serialize(emoji.as_bytes()).collect();
        let path = format!("/messages/{mid}/reactions/{enc}");
        if on {
            self.put_empty(&path).await
        } else {
            self.delete(&path).await
        }
    }

    /// Marks everything in the conversation as read (up to its newest message).
    pub async fn thread(&self, conv: &str, root: &str) -> Res<ThreadView> {
        self.get(&format!("/conversations/{conv}/threads/{root}")).await
    }

    pub async fn mark_read(&self, conv: &str) -> Res<Value> {
        let seq = self.conversation(conv).await?.conversation.last_seq;
        self.post(&format!("/conversations/{conv}/read"), &MarkReadRequest { seq }).await
    }

    pub async fn search(&self, q: &str) -> Res<SearchResponse> {
        let enc: String = url::form_urlencoded::byte_serialize(q.as_bytes()).collect();
        self.get(&format!("/search?q={enc}")).await
    }

    pub async fn users_list(&self) -> Res<Vec<User>> {
        self.get("/users").await
    }

    // ------------------------------------------------------------ files

    pub async fn upload_as(&self, name: &str, content_type: &str, purpose: Option<&str>, data: Vec<u8>) -> Res<Attachment> {
        let mut u = url::Url::parse(&self.url("/files")).unwrap();
        u.query_pairs_mut().append_pair("name", name);
        if let Some(p) = purpose {
            u.query_pairs_mut().append_pair("purpose", p);
        }
        let rb = self.http.post(u).bearer_auth(self.access_token()).header("content-type", content_type).body(data);
        let r = Self::finish(rb).await;
        r.into_result::<UploadResponse>().map(|u| u.attachment)
    }

    pub async fn upload(&self, name: &str, data: Vec<u8>) -> Res<Attachment> {
        self.upload_as(name, "application/octet-stream", None, data).await
    }

    /// Uploads one file and sends it with `body` as a message.
    pub async fn send_file(&self, conv: &str, body: &str, name: &str, data: Vec<u8>) -> Message {
        let a = self.upload(name, data).await.unwrap();
        self.send_with_files(conv, body, &[a.id]).await.unwrap()
    }

    pub async fn download(&self, attachment_id: &str) -> Res<Vec<u8>> {
        let r = self.raw(Method::GET, &format!("/files/{attachment_id}"), None).await;
        if r.status == 200 {
            Ok(r.body.to_vec())
        } else {
            Err(r.error())
        }
    }

    // ------------------------------------------------------------ websocket

    pub async fn ws(&self, cursor: i64) -> Ws {
        Ws::connect(&self.base, &self.access_token(), cursor).await
    }
}

pub fn uid() -> String {
    // Unique enough for idempotency keys in tests: time + counter.
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    format!("t{t:x}-{}", N.fetch_add(1, Ordering::Relaxed))
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

// ---------------------------------------------------------------- admin calls

impl Client {
    pub async fn create_user(&self, username: &str, display: &str, password: Option<&str>) -> Res<CreateUserResponse> {
        self.post(
            "/admin/users",
            &CreateUserRequest { username: username.into(), display_name: display.into(), password: password.map(str::to_string), role: None, email: None, title: None, phone: None, create_login_link: None },
        )
        .await
    }

    pub async fn users(&self) -> Res<Vec<AdminUser>> {
        self.get("/admin/users").await
    }

    pub async fn user_id_of(&self, username: &str) -> String {
        self.users().await.unwrap().into_iter().find(|u| u.user.username == username).unwrap().user.id
    }

    pub async fn set_role(&self, id: &str, role: Role) -> Res<AdminUser> {
        self.put(&format!("/admin/users/{id}/role"), &SetRoleRequest { role }).await
    }

    pub async fn set_disabled(&self, id: &str, disabled: bool) -> Res<AdminUser> {
        self.patch(&format!("/admin/users/{id}"), &json!({"disabled": disabled})).await
    }

    pub async fn update_user(&self, id: &str, req: &UpdateUserRequest) -> Res<AdminUser> {
        self.patch(&format!("/admin/users/{id}"), req).await
    }

    pub async fn reset_password(&self, id: &str, new_password: Option<&str>) -> Res<ResetPasswordResponse> {
        self.post(&format!("/admin/users/{id}/reset-password"), &ResetPasswordRequest { new_password: new_password.map(str::to_string) }).await
    }

    pub async fn login_link(&self, id: &str) -> Res<LoginLink> {
        self.post_empty(&format!("/admin/users/{id}/login-link")).await
    }

    pub async fn clear_history(&self, id: &str) -> Res<ClearHistoryResponse> {
        self.post_empty(&format!("/admin/users/{id}/clear-history")).await
    }

    pub async fn delete_user(&self, id: &str) -> Res<ClearHistoryResponse> {
        self.delete(&format!("/admin/users/{id}")).await
    }

    pub async fn backup_user(&self, id: &str) -> Res<BackupInfo> {
        self.post_empty(&format!("/admin/users/{id}/backup")).await
    }

    pub async fn delete_range(&self, id: &str, req: &DateRangeDeleteRequest) -> Res<ClearHistoryResponse> {
        self.post(&format!("/admin/users/{id}/delete-range"), req).await
    }

    pub async fn backups(&self) -> Res<Vec<BackupInfo>> {
        self.get("/admin/backups").await
    }

    /// A private channel with the given members; an admin creating it does not join (`include_creator: false`).
    pub async fn create_channel(&self, name: &str, members: &[String]) -> Res<ConversationSummary> {
        self.post(
            "/conversations",
            &CreateConversation { kind: ConversationKind::Channel, name: name.into(), topic: None, members: members.to_vec(), include_creator: Some(false) },
        )
        .await
    }

    /// A channel the way an app creates it: the creator joins as manager, always private.
    pub async fn new_channel(&self, name: &str, members: &[String]) -> Res<ConversationSummary> {
        self.post(
            "/conversations",
            &CreateConversation { kind: ConversationKind::Channel, name: name.into(), topic: None, members: members.to_vec(), include_creator: None },
        )
        .await
    }

    /// A group DM the way an app creates it (the creator joins automatically).
    pub async fn new_group(&self, name: &str, members: &[String]) -> Res<ConversationSummary> {
        self.post(
            "/conversations",
            &CreateConversation { kind: ConversationKind::GroupDm, name: name.into(), topic: None, members: members.to_vec(), include_creator: None },
        )
        .await
    }

    pub async fn create_group(&self, name: &str, members: &[String]) -> Res<ConversationSummary> {
        self.post("/conversations", &CreateConversation { kind: ConversationKind::GroupDm, name: name.into(), topic: None, members: members.to_vec(), include_creator: Some(false) }).await
    }

    pub async fn set_org_settings(&self, req: &UpdateOrgSettings) -> Res<OrgSettings> {
        self.put("/admin/org-settings", req).await
    }

    pub async fn remove_member(&self, conv: &str, user: &str) -> Res<Conversation> {
        self.delete(&format!("/admin/conversations/{conv}/members/{user}")).await
    }

    pub async fn add_members(&self, conv: &str, users: &[String]) -> Res<Conversation> {
        self.post(&format!("/admin/conversations/{conv}/members"), &MembersRequest { user_ids: users.to_vec() }).await
    }

    pub async fn admin_conversations(&self) -> Res<Vec<AdminConversation>> {
        self.get("/admin/conversations").await
    }

    pub async fn storage(&self) -> Res<StorageInfo> {
        self.get("/admin/storage").await
    }

    pub async fn test_storage(&self, cfg: &StorageConfig) -> Res<StorageTestResponse> {
        self.post("/admin/storage/test", cfg).await
    }

    pub async fn set_storage(&self, cfg: &StorageConfig) -> Res<StorageInfo> {
        self.put("/admin/storage", cfg).await
    }

    pub async fn migrate_storage(&self, target: &StorageConfig, delete_source: bool) -> Res<JobStatus> {
        self.post("/admin/storage/migrate", &StorageMigrateRequest { target: target.clone(), delete_source }).await
    }

    pub async fn verify_storage(&self, deep: bool) -> Res<JobStatus> {
        self.post("/admin/storage/verify", &StorageVerifyRequest { deep }).await
    }

    /// Polls a background job until it is no longer running.
    pub async fn wait_job(&self, id: &str) -> Res<JobStatus> {
        for _ in 0..600 {
            let j: JobStatus = self.get(&format!("/admin/jobs/{id}")).await?;
            if j.state != "running" {
                return Ok(j);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        Err(ApiErr { status: 0, code: None, message: "job timed out".into() })
    }

    pub async fn remove_file(&self, id: &str) -> Res<Value> {
        self.delete(&format!("/admin/files/{id}")).await
    }

    pub async fn all_sessions(&self) -> Res<Vec<SessionInfo>> {
        self.get("/admin/sessions").await
    }

    pub async fn revoke_user_sessions(&self, user_id: &str) -> Res<Value> {
        self.post_empty(&format!("/admin/users/{user_id}/revoke-sessions")).await
    }

    pub async fn backup_settings(&self) -> Res<BackupSettings> {
        self.get("/admin/backups/settings").await
    }

    pub async fn put_backup_settings(&self, req: &BackupSettingsUpdate) -> Res<BackupSettings> {
        self.put("/admin/backups/settings", req).await
    }

    pub async fn test_backup_target(&self) -> Res<StorageTestResponse> {
        self.post_empty("/admin/backups/test-target").await
    }

    pub async fn backup_now(&self) -> Res<BackupInfo> {
        self.post_empty("/admin/backups/full").await
    }

    pub async fn restore(&self, req: &RestoreRequest) -> Res<RestoreResponse> {
        self.post("/admin/restore", req).await
    }

    /// Fresh-server restore (no authentication while the server has no users).
    pub async fn fresh_restore(base: &str, req: &FreshRestoreRequest) -> Res<FreshRestoreResponse> {
        Client::anon(base).post("/restore", req).await
    }

    pub async fn viewer_conversations(&self, user_id: Option<&str>, kind: Option<&str>, q: Option<&str>) -> Res<Vec<ViewerConversation>> {
        let mut p = vec![];
        if let Some(u) = user_id {
            p.push(format!("user_id={u}"));
        }
        if let Some(k) = kind {
            p.push(format!("kind={k}"));
        }
        if let Some(q) = q {
            p.push(format!("q={}", url::form_urlencoded::byte_serialize(q.as_bytes()).collect::<String>()));
        }
        self.get(&format!("/admin/viewer/conversations?{}", p.join("&"))).await
    }

    pub async fn viewer_messages(&self, conv: &str, query: &str) -> Res<ViewerHistory> {
        self.get(&format!("/admin/viewer/conversations/{conv}/messages?{query}")).await
    }

    pub async fn viewer_search(&self, query: &str) -> Res<Vec<SearchHit>> {
        self.get(&format!("/admin/viewer/search?{query}")).await
    }

    pub async fn delete_messages(&self, req: &AdminDeleteMessagesRequest) -> Res<AdminDeleteMessagesResponse> {
        self.post("/admin/messages/delete", req).await
    }

    pub async fn create_user_group(&self, handle: &str, name: &str, members: &[String]) -> Res<UserGroup> {
        self.post("/admin/groups", &UserGroupRequest { handle: Some(handle.into()), name: Some(name.into()), member_ids: Some(members.to_vec()) }).await
    }

    pub async fn push_settings(&self) -> Res<PushSettings> {
        self.get("/admin/push").await
    }

    pub async fn put_push_settings(&self, s: &PushSettings) -> Res<PushSettings> {
        self.put("/admin/push", s).await
    }

    pub async fn bots(&self) -> Res<Vec<AdminBot>> {
        self.get("/admin/bots").await
    }

    pub async fn create_bot(&self, username: &str, display: &str, description: &str, conv_ids: &[String]) -> Res<AdminBot> {
        self.post("/admin/bots", &CreateBotRequest { username: username.into(), display_name: display.into(), description: description.into(), conv_ids: conv_ids.to_vec() }).await
    }

    pub async fn update_bot(&self, id: &str, req: &UpdateBotRequest) -> Res<AdminBot> {
        self.patch(&format!("/admin/bots/{id}"), req).await
    }

    pub async fn regenerate_bot_token(&self, id: &str) -> Res<AdminBot> {
        self.post_empty(&format!("/admin/bots/{id}/token")).await
    }

    pub async fn delete_bot(&self, id: &str) -> Res<Value> {
        self.delete(&format!("/admin/bots/{id}")).await
    }

    pub async fn add_bot_to(&self, id: &str, conv: &str) -> Res<AdminBot> {
        self.put_empty(&format!("/admin/bots/{id}/conversations/{conv}")).await
    }

    pub async fn remove_bot_from(&self, id: &str, conv: &str) -> Res<AdminBot> {
        self.delete(&format!("/admin/bots/{id}/conversations/{conv}")).await
    }

    pub async fn webhooks(&self) -> Res<WebhooksOverview> {
        self.get("/admin/webhooks").await
    }

    pub async fn create_incoming_webhook(&self, name: &str, conv_id: &str) -> Res<IncomingWebhook> {
        self.post("/admin/webhooks/incoming", &CreateIncomingWebhook { name: name.into(), conv_id: conv_id.into() }).await
    }

    pub async fn create_outgoing_webhook(&self, req: &CreateOutgoingWebhook) -> Res<OutgoingWebhook> {
        self.post("/admin/webhooks/outgoing", req).await
    }

    pub async fn update_outgoing_webhook(&self, id: &str, req: &UpdateOutgoingWebhook) -> Res<OutgoingWebhook> {
        self.patch(&format!("/admin/webhooks/outgoing/{id}"), req).await
    }

    pub async fn test_outgoing_webhook(&self, id: &str) -> Res<OutgoingWebhook> {
        self.post_empty(&format!("/admin/webhooks/outgoing/{id}/test")).await
    }

    pub async fn get_public_url(&self) -> Res<PublicUrlSettings> {
        self.get("/admin/public-url").await
    }

    pub async fn set_public_url(&self, url: Option<&str>) -> Res<PublicUrlSettings> {
        self.put("/admin/public-url", &PublicUrlUpdate { url: url.map(str::to_string) }).await
    }

    pub async fn ai_overview(&self) -> Res<AiOverview> {
        self.get("/admin/ai").await
    }

    pub async fn add_ai_provider(&self, i: &AiProviderInput) -> Res<AiProvider> {
        self.post("/admin/ai/providers", i).await
    }

    pub async fn update_ai_provider(&self, id: &str, u: &AiProviderUpdate) -> Res<AiProvider> {
        self.patch(&format!("/admin/ai/providers/{id}"), u).await
    }

    pub async fn delete_ai_provider(&self, id: &str) -> Res<AiOverview> {
        self.delete(&format!("/admin/ai/providers/{id}")).await
    }

    pub async fn test_ai_provider(&self, id: &str) -> Res<AiTestResponse> {
        self.post_empty(&format!("/admin/ai/providers/{id}/test")).await
    }

    pub async fn put_ai_defaults(&self, u: &AiDefaultsUpdate) -> Res<AiOverview> {
        self.put("/admin/ai/defaults", u).await
    }
}

// ---------------------------------------------------------------- websocket

type WsStream = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// The live event stream. `connect` performs the `hello` handshake; everything the server pushes is collected
/// by [`Ws::next`] and the `wait_*` helpers.
pub struct Ws {
    stream: WsStream,
    pub hello_cursor: i64,
    /// Set when the server closed the socket: the close code (for example 4410 session ended).
    pub closed: Option<u16>,
}

impl Ws {
    /// Connects and sends `hello`; panics if the server does not answer `hello_ok`.
    pub async fn connect(base: &str, token: &str, cursor: i64) -> Ws {
        let mut ws = Ws::connect_raw(base).await;
        ws.send(&ClientWsMsg::Hello { token: token.into(), cursor, client_version: "1.0.0".into() }).await;
        match ws.next(Duration::from_secs(5)).await {
            Some(ServerWsMsg::HelloOk { cursor, .. }) => ws.hello_cursor = cursor,
            other => panic!("expected hello_ok, got {other:?}"),
        }
        ws
    }

    /// Opens the socket without sending anything (to test the handshake rules).
    pub async fn connect_raw(base: &str) -> Ws {
        let url = format!("{}{}/ws", base.replacen("http://", "ws://", 1), API_PREFIX);
        let (stream, _) = tokio_tungstenite::connect_async(url).await.expect("websocket connect");
        Ws { stream, hello_cursor: 0, closed: None }
    }

    pub async fn send(&mut self, m: &ClientWsMsg) {
        self.stream.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::to_string(m).unwrap().into())).await.unwrap();
    }

    /// The next server message, or None on timeout or close (then `closed` is set if a code was given).
    pub async fn next(&mut self, within: Duration) -> Option<ServerWsMsg> {
        use tokio_tungstenite::tungstenite::Message;
        let deadline = tokio::time::Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(left, self.stream.next()).await {
                Err(_) => return None,
                Ok(None) => return None,
                Ok(Some(Err(_))) => return None,
                Ok(Some(Ok(Message::Text(t)))) => return serde_json::from_str(&t).ok(),
                Ok(Some(Ok(Message::Close(f)))) => {
                    self.closed = f.map(|f| u16::from(f.code));
                    return None;
                }
                Ok(Some(Ok(_))) => continue,
            }
        }
    }

    /// Waits for an event matching `pred` (any `Events` batch), up to 10 s. Panics on timeout.
    pub async fn wait_event(&mut self, what: &str, mut pred: impl FnMut(&EventData) -> bool) -> Event {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match self.next(left).await {
                Some(ServerWsMsg::Events { events, .. }) => {
                    if let Some(e) = events.into_iter().find(|e| pred(&e.event)) {
                        return e;
                    }
                }
                Some(_) => {}
                None => panic!("timed out waiting for event: {what} (closed={:?})", self.closed),
            }
        }
    }

    /// Waits for a non-event message (typing, presence, error, pong) matching `pred`.
    pub async fn wait_msg(&mut self, what: &str, mut pred: impl FnMut(&ServerWsMsg) -> bool) -> ServerWsMsg {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match self.next(left).await {
                Some(m) if pred(&m) => return m,
                Some(_) => {}
                None => panic!("timed out waiting for message: {what} (closed={:?})", self.closed),
            }
        }
    }

    /// Waits until the server closes the socket (messages in between are dropped) and returns the close code.
    pub async fn wait_closed(&mut self) -> Option<u16> {
        loop {
            if self.next(Duration::from_secs(10)).await.is_none() {
                return self.closed;
            }
        }
    }

    /// Waits for the server to end this session (account disabled, password reset, session revoked, ...):
    /// returns the error code and message it sent, and checks that the socket was then closed with `SESSION_ENDED`.
    pub async fn wait_kicked(&mut self) -> (ErrorCode, String) {
        let m = self.wait_msg("an error message", |m| matches!(m, ServerWsMsg::Error { .. })).await;
        let ServerWsMsg::Error { code, message } = m else { unreachable!() };
        assert_eq!(self.wait_closed().await, Some(close::SESSION_ENDED), "closed with the session-ended code");
        (code, message)
    }
}

// ---------------------------------------------------------------- webhook signatures

/// The documented `X-WorkChat-Signature` value: `sha256=` + hex(HMAC-SHA256(secret, "<timestamp>.<body>")).
pub fn webhook_signature(secret: &str, timestamp: &str, body: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}
