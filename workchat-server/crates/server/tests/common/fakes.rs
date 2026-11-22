//! Local fakes of the outside world: a webhook receiver and an AI provider (Anthropic / OpenAI shaped).
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::routing::any;
use axum::Router;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct Req {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: HeaderMap,
    pub body: String,
}

#[derive(Clone, Default)]
pub struct Receiver {
    reqs: Arc<Mutex<Vec<Req>>>,
    /// Number of initial requests to answer with HTTP 500.
    fail_first: Arc<Mutex<usize>>,
}

impl Receiver {
    /// Starts a receiver; the first `fail_first` requests are answered with HTTP 500.
    pub async fn start(fail_first: usize) -> (Receiver, String) {
        let r = Receiver { fail_first: Arc::new(Mutex::new(fail_first)), ..Default::default() };
        async fn handle(State(r): State<Receiver>, m: Method, u: Uri, h: HeaderMap, b: Bytes) -> (StatusCode, &'static str) {
            r.reqs.lock().unwrap().push(Req { method: m.to_string(), path: u.path().to_string(), query: u.query().unwrap_or("").to_string(), headers: h, body: String::from_utf8_lossy(&b).to_string() });
            let mut f = r.fail_first.lock().unwrap();
            if *f > 0 {
                *f -= 1;
                return (StatusCode::INTERNAL_SERVER_ERROR, "no");
            }
            (StatusCode::OK, "ok")
        }
        let app = Router::new().fallback(any(handle)).with_state(r.clone());
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        (r, url)
    }
    pub fn all(&self) -> Vec<Req> {
        self.reqs.lock().unwrap().clone()
    }
    pub fn json_bodies(&self) -> Vec<Value> {
        self.all().iter().filter_map(|r| serde_json::from_str(&r.body).ok()).collect()
    }
}

/// A fake Anthropic / OpenAI endpoint. The reply depends on the system prompt.
#[derive(Clone, Default)]
pub struct Provider {
    pub reqs: Arc<Mutex<Vec<(String, HeaderMap, Value)>>>,
}

impl Provider {
    pub async fn start() -> (Provider, String) {
        let p = Provider::default();
        async fn handle(State(p): State<Provider>, u: Uri, h: HeaderMap, b: Bytes) -> axum::Json<Value> {
            let v: Value = serde_json::from_slice(&b).unwrap_or(Value::Null);
            p.reqs.lock().unwrap().push((u.path().to_string(), h, v.clone()));
            let openai = u.path().ends_with("/chat/completions");
            let system = if openai { v["messages"][0]["content"].as_str().unwrap_or("").to_string() } else { v["system"].as_str().unwrap_or("").to_string() };
            let text = if system.contains("search filter") {
                r#"Here you go: {"q": "invoice", "from": "bob", "in": "finance", "has": null, "after": "2000-01-01", "before": null}"#.to_string()
            } else if system.contains("summarise") {
                "Recap: the team talked about invoices.".to_string()
            } else if system.contains("Answer the user") {
                "Bob sent the invoice.".to_string()
            } else {
                "ok".to_string()
            };
            if openai {
                axum::Json(json!({"choices": [{"message": {"role": "assistant", "content": text}}]}))
            } else {
                axum::Json(json!({"content": [{"type": "text", "text": text}], "role": "assistant"}))
            }
        }
        let app = Router::new().fallback(any(handle)).with_state(p.clone());
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        (p, url)
    }
}

/// A fake ntfy / FCM / APNs endpoint: records everything, answers the OAuth token exchange of FCM.
#[derive(Clone, Default)]
pub struct PushFake {
    pub reqs: Arc<Mutex<Vec<Req>>>,
}

impl PushFake {
    pub async fn start() -> (PushFake, String) {
        let f = PushFake::default();
        async fn handle(State(f): State<PushFake>, m: Method, u: Uri, h: HeaderMap, b: Bytes) -> ([(&'static str, &'static str); 1], String) {
            let req = Req { method: m.to_string(), path: u.path().to_string(), query: u.query().unwrap_or("").to_string(), headers: h, body: String::from_utf8_lossy(&b).to_string() };
            let reply = if req.path.ends_with("/oauth2/token") { r#"{"access_token":"fake-access","expires_in":3600}"#.to_string() } else { "{}".to_string() };
            f.reqs.lock().unwrap().push(req);
            ([("content-type", "application/json")], reply)
        }
        let app = Router::new().fallback(any(handle)).with_state(f.clone());
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        (f, url)
    }

    /// Requests that are real pushes (not the OAuth token exchange).
    pub fn pushes(&self) -> Vec<Req> {
        self.reqs.lock().unwrap().iter().filter(|r| !r.path.ends_with("/oauth2/token")).cloned().collect()
    }
    pub fn token_requests(&self) -> Vec<Req> {
        self.reqs.lock().unwrap().iter().filter(|r| r.path.ends_with("/oauth2/token")).cloned().collect()
    }
    /// Everything received, as one string (to search for leaked content).
    pub fn dump(&self) -> String {
        format!("{:?}", self.reqs.lock().unwrap())
    }
}
