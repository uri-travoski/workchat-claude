//! A small in-process fake of the Google Drive v3 / OAuth subset the server's GDrive backend uses:
//! token refresh, list (q=...), create folder, multipart upload, media download with Range,
//! patch (rename) and delete. It is not Google; it only lets the backend be exercised for real over HTTP.
#![allow(dead_code)]
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const ACCESS_TOKEN: &str = "fake-access-token";

#[derive(Clone, Debug)]
pub struct FakeFile {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    pub is_folder: bool,
    pub data: Vec<u8>,
}

#[derive(Default)]
pub struct Drive {
    pub files: Mutex<Vec<FakeFile>>,
    pub token_requests: Mutex<u32>,
    next: Mutex<u32>,
}

impl Drive {
    pub fn count_files(&self) -> usize {
        self.files.lock().unwrap().iter().filter(|f| !f.is_folder).count()
    }
    pub fn names(&self) -> Vec<String> {
        self.files.lock().unwrap().iter().filter(|f| !f.is_folder).map(|f| f.name.clone()).collect()
    }
    fn new_id(&self) -> String {
        let mut n = self.next.lock().unwrap();
        *n += 1;
        format!("fake{:05}", *n)
    }
}

type S = Arc<Drive>;

fn authed(h: &HeaderMap) -> bool {
    h.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) == Some(&format!("Bearer {ACCESS_TOKEN}"))
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({"error": {"code": 401, "message": "bad token"}}))).into_response()
}

pub async fn start() -> (S, u16) {
    let drive: S = Arc::new(Drive::default());
    let app = Router::new()
        .route("/token", post(token))
        .route("/drive/v3/files", get(list).post(create_folder))
        .route("/upload/drive/v3/files", post(upload))
        .route("/drive/v3/files/{id}", get(download).patch(patch).delete(remove))
        .with_state(drive.clone());
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (drive, port)
}

async fn token(State(d): State<S>, Form(f): Form<HashMap<String, String>>) -> Response {
    *d.token_requests.lock().unwrap() += 1;
    if f.get("grant_type").map(String::as_str) != Some("refresh_token") || f.get("refresh_token").map(String::as_str) != Some("fake-refresh-token") {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid_grant"}))).into_response();
    }
    Json(json!({"access_token": ACCESS_TOKEN, "expires_in": 3600, "token_type": "Bearer"})).into_response()
}

/// Pulls `name='...'` out of a Drive query (handles `\'` escapes).
fn q_name(q: &str) -> Option<String> {
    let i = q.find("name='")? + 6;
    let mut out = String::new();
    let mut chars = q[i..].chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(chars.next()?),
            '\'' => return Some(out),
            c => out.push(c),
        }
    }
    None
}

fn q_parent(q: &str) -> Option<String> {
    let end = q.find("' in parents")?;
    let start = q[..end].rfind('\'')? + 1;
    Some(q[start..end].to_string())
}

async fn list(State(d): State<S>, h: HeaderMap, Query(p): Query<HashMap<String, String>>) -> Response {
    if !authed(&h) {
        return unauthorized();
    }
    let q = p.get("q").cloned().unwrap_or_default();
    let want_folder = q.contains("mimeType='application/vnd.google-apps.folder'");
    let name = q_name(&q);
    let parent = q_parent(&q);
    let files = d.files.lock().unwrap();
    let out: Vec<Value> = files
        .iter()
        .filter(|f| f.is_folder == want_folder)
        .filter(|f| name.as_ref().is_none_or(|n| &f.name == n))
        .filter(|f| parent.as_ref().is_none_or(|pa| f.parent.as_ref() == Some(pa)))
        .map(|f| json!({"id": f.id, "name": f.name, "size": f.data.len().to_string()}))
        .collect();
    Json(json!({"files": out})).into_response()
}

async fn create_folder(State(d): State<S>, h: HeaderMap, Json(v): Json<Value>) -> Response {
    if !authed(&h) {
        return unauthorized();
    }
    let id = d.new_id();
    d.files.lock().unwrap().push(FakeFile { id: id.clone(), name: v["name"].as_str().unwrap_or("").into(), parent: None, is_folder: true, data: vec![] });
    Json(json!({"id": id})).into_response()
}

fn find_sub(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

async fn upload(State(d): State<S>, h: HeaderMap, body: Bytes) -> Response {
    if !authed(&h) {
        return unauthorized();
    }
    let ct = h.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
    let Some(boundary) = ct.split("boundary=").nth(1).map(|b| b.trim_matches('"').to_string()) else {
        return (StatusCode::BAD_REQUEST, "no boundary").into_response();
    };
    let delim = format!("--{boundary}").into_bytes();
    // Part 1: JSON metadata. Part 2: media.
    let mut parts: Vec<&[u8]> = vec![];
    let mut pos = find_sub(&body, &delim, 0);
    while let Some(start) = pos {
        let after = start + delim.len();
        if body[after..].starts_with(b"--") {
            break;
        }
        let next = find_sub(&body, &delim, after);
        let end = next.unwrap_or(body.len());
        parts.push(&body[after..end]);
        pos = next;
    }
    if parts.len() != 2 {
        return (StatusCode::BAD_REQUEST, "expected two parts").into_response();
    }
    let body_of = |p: &[u8]| -> Vec<u8> {
        let i = find_sub(p, b"\r\n\r\n", 0).map(|i| i + 4).unwrap_or(0);
        let mut v = p[i..].to_vec();
        if v.ends_with(b"\r\n") {
            v.truncate(v.len() - 2);
        }
        v
    };
    let meta: Value = serde_json::from_slice(&body_of(parts[0])).unwrap_or(Value::Null);
    let data = body_of(parts[1]);
    let id = d.new_id();
    d.files.lock().unwrap().push(FakeFile {
        id: id.clone(),
        name: meta["name"].as_str().unwrap_or("").into(),
        parent: meta["parents"][0].as_str().map(String::from),
        is_folder: false,
        data,
    });
    Json(json!({"id": id})).into_response()
}

async fn download(State(d): State<S>, h: HeaderMap, Path(id): Path<String>) -> Response {
    if !authed(&h) {
        return unauthorized();
    }
    let files = d.files.lock().unwrap();
    let Some(f) = files.iter().find(|f| f.id == id) else { return StatusCode::NOT_FOUND.into_response() };
    if let Some(r) = h.get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("bytes=")) {
        let (s, e) = r.split_once('-').unwrap_or(("0", ""));
        let s: usize = s.parse().unwrap_or(0);
        let e: usize = e.parse().map(|e: usize| e + 1).unwrap_or(f.data.len()).min(f.data.len());
        if s >= e {
            return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
        }
        return (StatusCode::PARTIAL_CONTENT, [(header::CONTENT_RANGE, format!("bytes {}-{}/{}", s, e - 1, f.data.len()))], f.data[s..e].to_vec()).into_response();
    }
    f.data.clone().into_response()
}

async fn patch(State(d): State<S>, h: HeaderMap, Path(id): Path<String>, Json(v): Json<Value>) -> Response {
    if !authed(&h) {
        return unauthorized();
    }
    let mut files = d.files.lock().unwrap();
    match files.iter_mut().find(|f| f.id == id) {
        Some(f) => {
            if let Some(n) = v["name"].as_str() {
                f.name = n.into();
            }
            Json(json!({"id": id})).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn remove(State(d): State<S>, h: HeaderMap, Path(id): Path<String>) -> Response {
    if !authed(&h) {
        return unauthorized();
    }
    let mut files = d.files.lock().unwrap();
    let before = files.len();
    files.retain(|f| f.id != id);
    if files.len() < before { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND }.into_response()
}
