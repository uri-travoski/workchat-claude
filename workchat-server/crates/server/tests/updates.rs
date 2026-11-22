//! The admin "Updates" card: the server asks a fake GitHub-shaped release server (the browser never does), caches the
//! answer, and reports "up to date", "update available", pre-release only, none published, rate limit and offline.
mod common;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use common::*;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Fake {
    /// Version answered (tag `v<ver>`); empty = nothing published (404).
    version: String,
    prerelease: bool,
    draft: bool,
    /// Answer 403 with rate-limit headers.
    limit: bool,
    user_agents: Vec<String>,
    accepts: Vec<String>,
    hits: u32,
}

type Shared = Arc<Mutex<Fake>>;

async fn latest(State(s): State<Shared>, h: HeaderMap) -> impl IntoResponse {
    let mut f = s.lock().unwrap();
    f.hits += 1;
    f.user_agents.push(h.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("").to_string());
    f.accepts.push(h.get(header::ACCEPT).and_then(|v| v.to_str().ok()).unwrap_or("").to_string());
    if f.limit {
        return (StatusCode::FORBIDDEN, [("x-ratelimit-remaining", "0")], "{\"message\":\"API rate limit exceeded\"}".to_string()).into_response();
    }
    if f.version.is_empty() {
        return (StatusCode::NOT_FOUND, "{\"message\":\"Not Found\"}").into_response();
    }
    let host = h.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let v = f.version.clone();
    let base = format!("http://{host}/dl");
    let body = serde_json::json!({
        "tag_name": format!("v{v}"), "name": format!("WorkChat Server {v}"), "draft": f.draft, "prerelease": f.prerelease,
        "html_url": format!("http://{host}/release"), "published_at": "2026-11-20T00:00:00Z",
        "body": "## What's new\n* Update checks\n* Fixes",
        "assets": [
            {"name": format!("workchat-server-{v}-linux-x86_64.tar.gz"), "size": 123, "browser_download_url": format!("{base}/workchat-server-{v}-linux-x86_64.tar.gz")},
            {"name": format!("workchat-server-{v}-android.apk"), "size": 456, "browser_download_url": format!("{base}/workchat-server-{v}-android.apk")}
        ]
    });
    ([(header::CONTENT_TYPE, "application/json")], body.to_string()).into_response()
}

async fn fake(f: Fake) -> (String, Shared) {
    let shared: Shared = Arc::new(Mutex::new(f));
    let app = Router::new().route("/releases/latest", get(latest)).with_state(shared.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (format!("http://{addr}/releases/latest"), shared)
}

async fn server_for(url: &str) -> TestServer {
    let url = url.to_string();
    TestServer::start_with(move |c| c.update_url = url).await
}

async fn card(s: &TestServer, refresh: bool) -> AdminUpdates {
    s.admin.get(&format!("/admin/updates{}", if refresh { "?refresh=true" } else { "" })).await.unwrap()
}

#[tokio::test]
async fn newer_release_is_offered_with_notes_and_assets() {
    let (url, f) = fake(Fake { version: "99.0.0".into(), ..Default::default() }).await;
    let s = server_for(&url).await;
    let u = card(&s, false).await;
    assert_eq!(u.current, VERSION);
    assert!(u.update_available && u.error.is_none(), "{u:?}");
    assert_eq!(u.source, url);
    assert_eq!(u.release_url, "https://example.invalid/releases", "the configured release_url is always sent");
    let r = u.latest.unwrap();
    assert_eq!(r.version, "99.0.0");
    assert_eq!(r.tag, "v99.0.0");
    assert!(r.notes.starts_with("What's new"), "{}", r.notes);
    assert!(r.notes.contains("Update checks") && !r.notes.contains("##"));
    assert_eq!(r.assets.len(), 2);
    assert!(r.assets[0].name.ends_with("linux-x86_64.tar.gz") && r.assets[0].size == 123);
    // The request identifies the server by version only.
    let f = f.lock().unwrap();
    assert_eq!(f.user_agents, vec![format!("WorkChat-Server/{VERSION}")]);
    assert_eq!(f.accepts, vec!["application/vnd.github+json".to_string()]);
}

#[tokio::test]
async fn same_version_is_up_to_date_and_the_answer_is_cached() {
    let (url, f) = fake(Fake { version: VERSION.into(), ..Default::default() }).await;
    let s = server_for(&url).await;
    let u = card(&s, false).await;
    assert!(!u.update_available && u.error.is_none());
    assert_eq!(u.latest.as_ref().unwrap().version, VERSION, "the newest release is shown even when it is not newer");
    assert!(u.checked_ms > 0);
    // Cached for an hour; "Check again" is honoured only after a minute.
    f.lock().unwrap().version = "99.0.0".into();
    assert_eq!(card(&s, false).await.latest.unwrap().version, VERSION);
    assert_eq!(card(&s, true).await.latest.unwrap().version, VERSION);
    assert_eq!(f.lock().unwrap().hits, 1);
}

#[tokio::test]
async fn prerelease_draft_none_rate_limit_and_offline_are_reported() {
    // Only a pre-release: nothing eligible, no error.
    let (url, _f) = fake(Fake { version: "99.0.0".into(), prerelease: true, ..Default::default() }).await;
    let u = card(&server_for(&url).await, false).await;
    assert!(u.latest.is_none() && !u.update_available && u.error.is_none(), "{u:?}");
    // Only a draft.
    let (url, _f) = fake(Fake { version: "99.0.0".into(), draft: true, ..Default::default() }).await;
    let u = card(&server_for(&url).await, false).await;
    assert!(u.latest.is_none() && !u.update_available);
    // Nothing published.
    let (url, _f) = fake(Fake::default()).await;
    let u = card(&server_for(&url).await, false).await;
    assert_eq!(u.error.as_deref(), Some("No release has been published yet."));
    assert!(!u.update_available);
    // Rate limited.
    let (url, _f) = fake(Fake { version: "99.0.0".into(), limit: true, ..Default::default() }).await;
    let u = card(&server_for(&url).await, false).await;
    assert!(u.error.unwrap().contains("limiting update checks"));
    // Nothing listens on port 1: offline.
    let u = card(&server_for("http://127.0.0.1:1/releases/latest").await, false).await;
    assert!(u.error.unwrap().contains("could not reach"), "offline wording");
    assert!(!u.update_available && u.latest.is_none());
    assert_eq!(u.release_url, "https://example.invalid/releases", "the link is there when offline too");
    // Default config points at the latest-release page, not a tag.
    let d = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/config.rs")).unwrap();
    assert!(d.contains("\"https://github.com/uri-travoski/workchat-claude/releases/latest\".into()"));
}

#[tokio::test]
async fn only_admins_see_the_updates_card() {
    let (url, _f) = fake(Fake { version: "99.0.0".into(), ..Default::default() }).await;
    let s = server_for(&url).await;
    let alice = s.user("alice").await;
    assert_eq!(alice.get::<AdminUpdates>("/admin/updates").await.unwrap_err().status, 403);
    assert_eq!(Client::anon(&s.url).get::<AdminUpdates>("/admin/updates").await.unwrap_err().status, 401);
}
