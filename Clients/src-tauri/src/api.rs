//! Typed HTTP access to the server: address rules, sign-in state and transparent token refresh.

use crate::error::{CoreError, Result};
use proto::*;
use reqwest::{Method, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

pub const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Persisted sign-in state (`session.json` in the app data folder, mode 0600 on desktop).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Session {
    pub server: String,
    pub username: String,
    pub user_id: String,
    pub session_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at_ms: i64,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

// ---------------------------------------------------------------- server address rules

/// True for hosts that only exist on a local or private network: loopback, RFC 1918, link-local,
/// `*.local`, `*.lan`, `*.home.arpa`, unique-local IPv6 and single-label names (`chatbox`).
/// The Android emulator's host alias `10.0.2.2` is a private address and so is covered too.
pub fn is_private_host(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']').trim_end_matches('.').to_ascii_lowercase();
    if h.is_empty() {
        return false;
    }
    if h == "localhost" || h.ends_with(".localhost") || h.ends_with(".local") || h.ends_with(".lan") || h.ends_with(".home.arpa") {
        return true;
    }
    match h.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(a)) => a.is_loopback() || a.is_private() || a.is_link_local(),
        Ok(std::net::IpAddr::V6(a)) => a.is_loopback() || a.is_unicast_link_local() || (a.segments()[0] & 0xfe00) == 0xfc00,
        // A name without a dot cannot be a public DNS name.
        Err(_) => !h.contains('.'),
    }
}

/// Turns what the person typed into a base URL (`scheme://host[:port]`).
/// `https` always works; `http` only for private-network hosts (see [`is_private_host`]).
/// With no scheme typed, private hosts get `http://` and everything else `https://`.
pub fn normalize_server_url(input: &str) -> Result<String> {
    let s = input.trim().trim_end_matches('/');
    if s.is_empty() {
        return Err(CoreError::local("Enter the server address."));
    }
    let with_scheme = if s.contains("://") {
        s.to_string()
    } else {
        let host_part = s.split(['/', '?', '#']).next().unwrap_or(s);
        // Strip a port (and keep IPv6 brackets) to look at the host only.
        let host_only = if let Some(rest) = host_part.strip_prefix('[') {
            rest.split(']').next().unwrap_or("").to_string()
        } else {
            host_part.rsplit_once(':').map(|(h, _)| h).unwrap_or(host_part).to_string()
        };
        if is_private_host(&host_only) {
            format!("http://{s}")
        } else {
            format!("https://{s}")
        }
    };
    let u = url::Url::parse(&with_scheme).map_err(|_| CoreError::local("That server address is not valid."))?;
    let host = u.host_str().ok_or_else(|| CoreError::local("That server address is not valid."))?;
    match u.scheme() {
        "https" => {}
        "http" if is_private_host(host) => {}
        "http" => {
            return Err(CoreError::local(
                "This address uses http, which is only allowed on your local network (192.168.x.x, 10.x.x.x, localhost, name.local). Use https:// for other servers.",
            ))
        }
        _ => return Err(CoreError::local("The server address must start with https:// (or http:// on a local network).")),
    }
    let port = u.port().map(|p| format!(":{p}")).unwrap_or_default();
    Ok(format!("{}://{}{}", u.scheme(), host, port))
}

pub fn device_name() -> String {
    let os = match std::env::consts::OS {
        "linux" => "Linux",
        "windows" => "Windows",
        "macos" => "macOS",
        "android" => "Android",
        "ios" => "iOS",
        o => o,
    };
    let host = std::env::var("HOSTNAME").ok().or_else(|| std::fs::read_to_string("/etc/hostname").ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    match host {
        Some(h) if os == "Linux" || os == "Windows" || os == "macOS" => format!("WorkChat on {os} ({h})"),
        _ => format!("WorkChat on {os}"),
    }
}

// ---------------------------------------------------------------- session file

pub fn session_path(dir: &Path) -> PathBuf {
    dir.join("session.json")
}

pub fn load_session(dir: &Path) -> Option<Session> {
    let text = std::fs::read_to_string(session_path(dir)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_session(dir: &Path, s: Option<&Session>) -> Result<()> {
    let p = session_path(dir);
    match s {
        None => {
            let _ = std::fs::remove_file(&p);
            Ok(())
        }
        Some(s) => {
            let tmp = p.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_string(s)?)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
            }
            std::fs::rename(&tmp, &p)?;
            Ok(())
        }
    }
}

// ---------------------------------------------------------------- client

pub type FatalHandler = Arc<dyn Fn(ErrorCode, String) + Send + Sync>;

pub struct Api {
    pub base: String,
    http: reqwest::Client,
    up_http: reqwest::Client,
    session: RwLock<Option<Session>>,
    dir: PathBuf,
    refresh_lock: tokio::sync::Mutex<()>,
    fatal: Mutex<Option<FatalHandler>>,
}

impl Api {
    pub fn new(base: &str, dir: &Path) -> Result<Api> {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(60))
            .user_agent(format!("workchat/{CLIENT_VERSION}"))
            .build()
            .map_err(|e| CoreError::local(format!("http client: {e}")))?;
        // Uploads and downloads of big files must not hit the 60 s total timeout; a stalled connection still ends.
        let up_http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .read_timeout(std::time::Duration::from_secs(45))
            .user_agent(format!("workchat/{CLIENT_VERSION}"))
            .build()
            .map_err(|e| CoreError::local(format!("http client: {e}")))?;
        Ok(Api { base: base.trim_end_matches('/').to_string(), http, up_http, session: RwLock::new(None), dir: dir.to_path_buf(), refresh_lock: tokio::sync::Mutex::new(()), fatal: Mutex::new(None) })
    }

    pub fn set_fatal_handler(&self, h: FatalHandler) {
        *self.fatal.lock().unwrap_or_else(|e| e.into_inner()) = Some(h);
    }
    pub fn session(&self) -> Option<Session> {
        self.session.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub fn set_session(&self, s: Option<Session>) {
        *self.session.write().unwrap_or_else(|e| e.into_inner()) = s;
    }
    fn persist(&self) -> Result<()> {
        save_session(&self.dir, self.session().as_ref())
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}{}", self.base, API_PREFIX, path)
    }
    pub fn ws_url(&self) -> String {
        let b = if let Some(r) = self.base.strip_prefix("https://") { format!("wss://{r}") } else { self.base.replacen("http://", "ws://", 1) };
        format!("{b}{API_PREFIX}/ws")
    }

    fn net(e: reqwest::Error) -> CoreError {
        if e.is_connect() {
            return CoreError::Network("Cannot reach the server. Check the address and your connection.".into());
        }
        if e.is_timeout() {
            return CoreError::Network("The server did not answer in time.".into());
        }
        let mut msg = e.to_string();
        let mut src = std::error::Error::source(&e);
        while let Some(s) = src {
            msg = format!("{msg}: {s}");
            src = s.source();
        }
        CoreError::Network(msg)
    }

    async fn parse_error(&self, resp: Response) -> CoreError {
        let status = resp.status();
        match resp.json::<ErrorBody>().await {
            Ok(b) => {
                let e = b.error;
                if e.code.requires_wipe() {
                    if let Some(h) = self.fatal.lock().unwrap_or_else(|e| e.into_inner()).clone() {
                        h(e.code, e.message.clone());
                    }
                }
                CoreError::Api { code: e.code, message: e.message }
            }
            Err(_) => CoreError::Api {
                code: if status.is_server_error() { ErrorCode::Internal } else { ErrorCode::Validation },
                message: format!("Unexpected response from the server (HTTP {}).", status.as_u16()),
            },
        }
    }

    /// Refreshes the access token when it is missing or about to expire.
    pub async fn ensure_fresh(&self) -> Result<()> {
        if self.session().map(|s| s.expires_at_ms - now_ms() < 30_000).unwrap_or(false) {
            self.refresh().await?;
        }
        Ok(())
    }

    pub async fn refresh(&self) -> Result<()> {
        let before = self.session().ok_or(CoreError::NotSignedIn)?;
        let _g = self.refresh_lock.lock().await;
        // Another task may have refreshed while this one waited for the lock.
        if let Some(cur) = self.session() {
            if cur.access_token != before.access_token {
                return Ok(());
            }
        }
        let resp = self
            .http
            .post(self.url("/refresh"))
            .header("X-Client-Version", CLIENT_VERSION)
            .json(&RefreshRequest { refresh_token: before.refresh_token.clone() })
            .send()
            .await
            .map_err(Self::net)?;
        if !resp.status().is_success() {
            return Err(self.parse_error(resp).await);
        }
        let t: TokenPair = resp.json().await.map_err(Self::net)?;
        let mut s = before;
        s.access_token = t.access_token;
        s.refresh_token = t.refresh_token;
        s.expires_at_ms = now_ms() + t.expires_in * 1000;
        self.set_session(Some(s));
        self.persist()
    }

    pub fn bearer(&self) -> Result<String> {
        self.session().map(|s| s.access_token).ok_or(CoreError::NotSignedIn)
    }

    /// Sends an authenticated request, refreshing the token once if it has expired.
    pub async fn send(&self, method: Method, path: &str, query: &[(&str, String)], body: Option<serde_json::Value>, extra: &[(&str, String)]) -> Result<Response> {
        self.ensure_fresh().await?;
        for attempt in 0..2 {
            let mut rb = self
                .http
                .request(method.clone(), self.url(path))
                .bearer_auth(self.bearer()?)
                .header("X-Client-Version", CLIENT_VERSION);
            if !query.is_empty() {
                rb = rb.query(query);
            }
            for (k, v) in extra {
                rb = rb.header(*k, v);
            }
            if let Some(b) = &body {
                rb = rb.json(b);
            }
            let resp = rb.send().await.map_err(Self::net)?;
            if resp.status().is_success() {
                return Ok(resp);
            }
            let err = self.parse_error(resp).await;
            if attempt == 0 && matches!(err, CoreError::Api { code: ErrorCode::TokenExpired, .. }) {
                self.refresh().await?;
                continue;
            }
            return Err(err);
        }
        Err(CoreError::NotSignedIn)
    }

    /// Streams a local file to `POST /files` with progress (bytes sent so far) and cooperative cancel.
    /// `purpose` is `None` for chat files, or `avatar` / `avatar_private`.
    pub async fn upload_file(&self, path: &Path, name: &str, content_type: &str, purpose: Option<&str>, cancel: Arc<std::sync::atomic::AtomicBool>, progress: Arc<dyn Fn(u64) + Send + Sync>) -> Result<Attachment> {
        use std::sync::atomic::Ordering;
        let size = tokio::fs::metadata(path).await?.len();
        for attempt in 0..2 {
            self.ensure_fresh().await?;
            if cancel.load(Ordering::SeqCst) {
                return Err(CoreError::local("Upload cancelled."));
            }
            let file = tokio::fs::File::open(path).await?;
            let (c2, p2) = (cancel.clone(), progress.clone());
            let stream = futures_util::stream::unfold((file, 0u64), move |(mut f, sent)| {
                let (c2, p2) = (c2.clone(), p2.clone());
                async move {
                    use tokio::io::AsyncReadExt;
                    if c2.load(Ordering::SeqCst) {
                        return Some((Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled")), (f, sent)));
                    }
                    let mut buf = vec![0u8; 64 * 1024];
                    match f.read(&mut buf).await {
                        Ok(0) => None,
                        Ok(n) => {
                            buf.truncate(n);
                            let sent = sent + n as u64;
                            p2(sent);
                            Some((Ok(bytes::Bytes::from(buf)), (f, sent)))
                        }
                        Err(e) => Some((Err(e), (f, sent))),
                    }
                }
            });
            let mut q = vec![("name", name.to_string())];
            if let Some(p) = purpose {
                q.push(("purpose", p.to_string()));
            }
            let resp = self
                .up_http
                .post(self.url("/files"))
                .query(&q)
                .bearer_auth(self.bearer()?)
                .header("X-Client-Version", CLIENT_VERSION)
                .header("Content-Type", content_type)
                .header("Content-Length", size.to_string())
                .body(reqwest::Body::wrap_stream(stream))
                .send()
                .await;
            let resp = match resp {
                Ok(r) => r,
                Err(e) => {
                    if cancel.load(Ordering::SeqCst) {
                        return Err(CoreError::local("Upload cancelled."));
                    }
                    return Err(Self::net(e));
                }
            };
            if resp.status().is_success() {
                let r: UploadResponse = resp.json().await.map_err(Self::net)?;
                return Ok(r.attachment);
            }
            let err = self.parse_error(resp).await;
            if attempt == 0 && matches!(err, CoreError::Api { code: ErrorCode::TokenExpired, .. }) {
                self.refresh().await?;
                continue;
            }
            return Err(err);
        }
        Err(CoreError::NotSignedIn)
    }

    pub async fn json<T: DeserializeOwned>(&self, method: Method, path: &str, query: &[(&str, String)], body: Option<serde_json::Value>) -> Result<T> {
        let resp = self.send(method, path, query, body, &[]).await?;
        resp.json::<T>().await.map_err(Self::net)
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        self.json(Method::GET, path, query, None).await
    }

    // ---- unauthenticated and sign-in calls

    pub async fn meta(&self) -> Result<ServerMeta> {
        let resp = self.http.get(self.url("/meta")).header("X-Client-Version", CLIENT_VERSION).send().await.map_err(Self::net)?;
        if !resp.status().is_success() {
            let st = resp.status().as_u16();
            return Err(match resp.json::<ErrorBody>().await {
                Ok(b) => CoreError::Api { code: b.error.code, message: b.error.message },
                Err(_) => CoreError::Network(format!("This does not look like a WorkChat server (HTTP {st}).")),
            });
        }
        resp.json::<ServerMeta>().await.map_err(|_| CoreError::Network("This does not look like a WorkChat server.".into()))
    }

    async fn login_call<B: Serialize>(&self, path: &str, body: &B, username_hint: &str) -> Result<LoginResponse> {
        let resp = self.http.post(self.url(path)).header("X-Client-Version", CLIENT_VERSION).json(body).send().await.map_err(Self::net)?;
        if !resp.status().is_success() {
            return Err(self.parse_error(resp).await);
        }
        let lr: LoginResponse = resp.json().await.map_err(Self::net)?;
        let username = if username_hint.is_empty() { lr.me.user.username.clone() } else { username_hint.to_string() };
        self.set_session(Some(Session {
            server: self.base.clone(),
            username,
            user_id: lr.me.user.id.clone(),
            session_id: lr.session_id.clone(),
            access_token: lr.tokens.access_token.clone(),
            refresh_token: lr.tokens.refresh_token.clone(),
            expires_at_ms: now_ms() + lr.tokens.expires_in * 1000,
        }));
        self.persist()?;
        Ok(lr)
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<LoginResponse> {
        let req = LoginRequest { username: username.trim().to_string(), password: password.to_string(), device_name: device_name(), client_version: CLIENT_VERSION.to_string() };
        self.login_call("/login", &req, username.trim()).await
    }

    pub async fn login_with_link(&self, token: &str) -> Result<LoginResponse> {
        let req = LinkLoginRequest { token: token.to_string(), device_name: device_name(), client_version: CLIENT_VERSION.to_string() };
        self.login_call("/login/link", &req, "").await
    }

    pub async fn logout(&self) {
        let _ = self.send(Method::POST, "/logout", &[], None, &[]).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_hosts() {
        for h in ["localhost", "127.0.0.1", "127.5.5.5", "10.0.2.2", "10.1.2.3", "172.16.0.1", "172.31.255.1", "192.168.1.20", "169.254.10.10", "chat.local", "nas.lan", "chatbox", "[::1]", "::1", "fe80::1", "fd12:3456::1", "Server.LOCAL."] {
            assert!(is_private_host(h), "{h} should be private");
        }
        for h in ["chat.example.com", "8.8.8.8", "172.32.0.1", "172.15.0.1", "11.0.0.1", "100.64.0.1", "2001:db8::1", "example.com", "localhost.example.com"] {
            assert!(!is_private_host(h), "{h} should be public");
        }
    }

    #[test]
    fn normalize_accepts_lan_http_and_public_https() {
        assert_eq!(normalize_server_url("192.168.1.20:8080").unwrap(), "http://192.168.1.20:8080");
        assert_eq!(normalize_server_url("http://10.0.2.2:18601/").unwrap(), "http://10.0.2.2:18601");
        assert_eq!(normalize_server_url("http://chat.local").unwrap(), "http://chat.local");
        assert_eq!(normalize_server_url("localhost:8080").unwrap(), "http://localhost:8080");
        assert_eq!(normalize_server_url("[::1]:8080").unwrap(), "http://[::1]:8080");
        assert_eq!(normalize_server_url("chat.example.com").unwrap(), "https://chat.example.com");
        assert_eq!(normalize_server_url("https://chat.example.com/admin?x=1").unwrap(), "https://chat.example.com");
        assert_eq!(normalize_server_url(" https://192.168.1.2:8443 ").unwrap(), "https://192.168.1.2:8443");
    }

    #[test]
    fn normalize_refuses_public_http_and_junk() {
        assert!(normalize_server_url("http://chat.example.com").is_err());
        assert!(normalize_server_url("http://8.8.8.8:8080").is_err());
        assert!(normalize_server_url("ftp://chat.local").is_err());
        assert!(normalize_server_url("").is_err());
        assert!(normalize_server_url("   ").is_err());
        let msg = normalize_server_url("http://example.org").unwrap_err().to_string();
        assert!(msg.contains("local network"));
    }

    #[test]
    fn ws_url_follows_scheme() {
        let a = Api::new("http://192.168.1.2:8080", Path::new("/tmp")).unwrap();
        assert_eq!(a.ws_url(), "ws://192.168.1.2:8080/api/v1/ws");
        let b = Api::new("https://x.example.com", Path::new("/tmp")).unwrap();
        assert_eq!(b.ws_url(), "wss://x.example.com/api/v1/ws");
    }

    #[test]
    fn session_file_roundtrip_and_mode() {
        let d = tempfile::tempdir().unwrap();
        let s = Session { server: "http://a".into(), username: "u".into(), user_id: "1".into(), session_id: "s".into(), access_token: "a".into(), refresh_token: "r".into(), expires_at_ms: 5 };
        save_session(d.path(), Some(&s)).unwrap();
        assert_eq!(load_session(d.path()), Some(s));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(session_path(d.path())).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        save_session(d.path(), None).unwrap();
        assert!(load_session(d.path()).is_none());
    }

    // ---- uploads against a tiny in-process HTTP server

    /// Accepts one request, reads headers and (up to `read_body` of) the body, then answers with `status` and `body`.
    async fn mock_server(status: &'static str, body: String, stall_after: Option<usize>) -> (String, tokio::task::JoinHandle<(String, usize)>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        let h = tokio::spawn(async move {
            let (mut s, _) = l.accept().await.unwrap();
            let mut head = Vec::new();
            let mut b = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                s.read_exact(&mut b).await.unwrap();
                head.push(b[0]);
            }
            let head = String::from_utf8_lossy(&head).to_string();
            let len: usize = head.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap())).unwrap_or(0);
            let mut got = 0usize;
            let mut buf = vec![0u8; 16 * 1024];
            while got < len {
                match tokio::time::timeout(std::time::Duration::from_secs(5), s.read(&mut buf)).await {
                    Ok(Ok(0)) | Err(_) | Ok(Err(_)) => break,
                    Ok(Ok(n)) => got += n,
                }
                if stall_after.map(|k| got >= k).unwrap_or(false) {
                    // keep the connection open and silent: the client must give up by itself (cancel)
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    break;
                }
            }
            let resp = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            let _ = s.write_all(resp.as_bytes()).await;
            (head, got)
        });
        (url, h)
    }

    fn signed_in(url: &str, dir: &Path) -> Api {
        let a = Api::new(url, dir).unwrap();
        a.set_session(Some(Session { server: url.into(), username: "u".into(), user_id: "1".into(), session_id: "s".into(), access_token: "tok".into(), refresh_token: "r".into(), expires_at_ms: now_ms() + 3_600_000 }));
        a
    }

    fn big_file(dir: &Path, n: usize) -> PathBuf {
        let p = dir.join("big.bin");
        std::fs::write(&p, vec![7u8; n]).unwrap();
        p
    }

    #[tokio::test]
    async fn upload_streams_the_file_and_reports_progress() {
        let d = tempfile::tempdir().unwrap();
        let path = big_file(d.path(), 300_000);
        let (url, srv) = mock_server("200 OK", r#"{"attachment":{"id":"att1","name":"big.bin","content_type":"application/octet-stream","size":300000}}"#.into(), None).await;
        let api = signed_in(&url, d.path());
        let seen = Arc::new(Mutex::new(Vec::<u64>::new()));
        let s2 = seen.clone();
        let att = api
            .upload_file(&path, "big.bin", "application/octet-stream", None, Arc::new(std::sync::atomic::AtomicBool::new(false)), Arc::new(move |n| s2.lock().unwrap().push(n)))
            .await
            .unwrap();
        assert_eq!(att.id, "att1");
        let (head, got) = srv.await.unwrap();
        assert_eq!(got, 300_000, "the whole file reaches the server");
        assert!(head.starts_with("POST /api/v1/files?name=big.bin"), "{head}");
        assert!(head.to_lowercase().contains("authorization: bearer tok"));
        let seen = seen.lock().unwrap();
        assert!(seen.len() > 3, "progress is reported in several steps");
        assert!(seen.windows(2).all(|w| w[0] < w[1]), "progress only grows");
        assert_eq!(*seen.last().unwrap(), 300_000);
    }

    #[tokio::test]
    async fn upload_can_be_cancelled_midway() {
        let d = tempfile::tempdir().unwrap();
        let path = big_file(d.path(), 2_000_000);
        let (url, srv) = mock_server("200 OK", "{}".into(), Some(60_000)).await;
        let api = signed_in(&url, d.path());
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let c2 = cancel.clone();
        let err = api
            .upload_file(&path, "big.bin", "application/octet-stream", None, cancel, Arc::new(move |n| if n > 100_000 { c2.store(true, std::sync::atomic::Ordering::SeqCst) }))
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("cancel"), "{err}");
        let (_, got) = srv.await.unwrap();
        assert!(got < 2_000_000, "the rest of the file is not sent after cancelling (got {got})");
    }

    #[tokio::test]
    async fn upload_surfaces_the_servers_refusal() {
        let d = tempfile::tempdir().unwrap();
        let path = big_file(d.path(), 1000);
        let (url, _srv) = mock_server("413 Payload Too Large", r#"{"error":{"code":"file_too_large","message":"That file is larger than the 100 MB limit."}}"#.into(), None).await;
        let api = signed_in(&url, d.path());
        let err = api.upload_file(&path, "big.bin", "application/octet-stream", None, Arc::new(std::sync::atomic::AtomicBool::new(false)), Arc::new(|_| {})).await.unwrap_err();
        assert!(err.to_string().contains("100 MB limit"), "{err}");
    }
}
