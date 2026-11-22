//! Bots, webhooks, forms, scheduled messages and AI against the real server image.
//!
//! The server runs in docker with local storage; the receivers (bot webhook, outgoing webhook, fake AI provider) run in
//! this process and are reached through `host.docker.internal`. Run with `tests/e2e/run.sh` (needs docker; skipped
//! without WORKCHAT_E2E).
//!
//! Covers: Telegram-style sendMessage / sendDocument / getUpdates / setWebhook delivery (secret header), incoming
//! webhook, outgoing webhook with a verified HMAC signature, a form submit round trip, a scheduled message that survives
//! a server restart, and AI search with a fake provider.
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::routing::any;
use axum::Router;
use proto::*;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use workchat_e2e::{enabled, sh, wait_http, Cleanup};
use workchat_testkit::{eventually, now_ms, webhook_signature, Client};

const PORT: u16 = 18302;
const IMAGE: &str = "workchat-server:e2e";
const NAME: &str = "workchat-e2e-bots";

#[derive(Clone, Default)]
struct Fake {
    reqs: Arc<Mutex<Vec<(String, HeaderMap, String)>>>,
}

impl Fake {
    async fn start() -> (Fake, u16) {
        let f = Fake::default();
        async fn handle(State(f): State<Fake>, u: Uri, h: HeaderMap, b: Bytes) -> axum::Json<Value> {
            let body = String::from_utf8_lossy(&b).to_string();
            f.reqs.lock().unwrap().push((u.path().to_string(), h, body.clone()));
            // Doubles as the fake AI provider (Anthropic messages API).
            let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let system = v["system"].as_str().unwrap_or("");
            let text = if system.contains("search filter") { r#"{"q": "invoice", "from": null, "in": null, "has": null, "after": null, "before": null}"# } else { "ok" };
            axum::Json(json!({"content": [{"type": "text", "text": text}], "role": "assistant"}))
        }
        let app = Router::new().fallback(any(handle)).with_state(f.clone());
        let l = tokio::net::TcpListener::bind("0.0.0.0:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        (f, port)
    }
    fn on(&self, path: &str) -> Vec<(HeaderMap, String)> {
        self.reqs.lock().unwrap().iter().filter(|r| r.0 == path).map(|r| (r.1.clone(), r.2.clone())).collect()
    }
}

async fn has_text(c: &Client, conv: &str, text: &str) -> bool {
    c.texts(conv).await.iter().any(|t| t == text)
}

async fn tg(http: &reqwest::Client, url: &str, token: &str, method: &str, body: Value) -> Value {
    let r = http.post(format!("{url}/bot{token}/{method}")).json(&body).send().await.unwrap();
    let v: Value = r.json().await.unwrap();
    assert_eq!(v["ok"], true, "{method}: {v}");
    v["result"].clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bots_webhooks_forms_schedule_ai() {
    if !enabled() {
        eprintln!("e2e (bots) skipped: run tests/e2e/run.sh (needs docker)");
        return;
    }
    let _g = Cleanup { containers: vec![NAME.into()], network: None };
    let _ = sh("docker", &["rm", "-f", "-v", NAME]);
    let public = format!("http://127.0.0.1:{PORT}");
    let (ok, out) = sh(
        "docker",
        &[
            "run", "-d", "--name", NAME, "--add-host", "host.docker.internal:host-gateway", "-p", &format!("127.0.0.1:{PORT}:8080"),
            "-e", &format!("WORKCHAT_PUBLIC_URL={public}"), "-e", "WORKCHAT_LOCKOUT_THRESHOLD=50", "-e", "WORKCHAT_WORKER_TICK_MS=300", IMAGE,
        ],
    );
    assert!(ok, "server failed to start (build the image first): {out}");
    wait_http(&format!("{public}/api/v1/health")).await;
    let (fake, fport) = Fake::start().await;
    let host = format!("http://host.docker.internal:{fport}");
    let http = reqwest::Client::new();

    let admin = Client::setup(&public, &SetupRequest { username: "admin".into(), display_name: "Admin".into(), password: "test-admin-pw".into(), email: None, storage: None }).await.unwrap();
    let a = admin.create_user("alice", "alice", Some("password-123")).await.unwrap().user.user.id;
    let alice = Client::login_device(&public, "alice", "password-123", "e2e-alice").await.unwrap();
    let chan = admin.create_channel("finance", &[a.clone()]).await.unwrap().conversation.id;
    assert!(alice.has_conversation(&chan).await);
    let bot = admin.create_bot("receipts_bot", "Receipt Logger", "Logs receipts", &[chan.clone()]).await.unwrap();
    let tok = bot.token.clone();

    // --- Telegram-style: getMe, sendMessage, sendDocument ------------------------------------------------------
    assert_eq!(tg(&http, &public, &tok, "getMe", json!({})).await["username"], "receipts_bot");
    tg(&http, &public, &tok, "sendMessage", json!({"chat_id": chan, "text": "Receipt logged: $12.50"})).await;
    let m = alice.history(&chan).await.into_iter().find(|m| m.body.starts_with("Receipt logged")).expect("alice sees the bot message");
    assert_eq!(m.sender_id, bot.user.id);
    let form = reqwest::multipart::Form::new()
        .text("chat_id", chan.clone())
        .text("caption", "the receipt")
        .part("document", reqwest::multipart::Part::bytes(b"%PDF-1.4 fake receipt".to_vec()).file_name("receipt.pdf").mime_str("application/pdf").unwrap());
    let r: Value = http.post(format!("{public}/bot{tok}/sendDocument")).multipart(form).send().await.unwrap().json().await.unwrap();
    assert_eq!(r["ok"], true, "{r}");
    assert!(alice.history(&chan).await.iter().any(|m| m.attachments.iter().any(|x| x.name == "receipt.pdf")));

    // --- getUpdates: a user message reaches the bot, offset acknowledges it -------------------------------------------
    alice.send(&chan, "please log lunch").await;
    let ups = tg(&http, &public, &tok, "getUpdates", json!({})).await;
    let ups = ups.as_array().unwrap();
    assert!(ups.iter().any(|u| u["message"]["text"] == "please log lunch"));
    let last = ups.iter().map(|u| u["update_id"].as_i64().unwrap()).max().unwrap();
    let after = tg(&http, &public, &tok, "getUpdates", json!({"offset": last + 1})).await;
    assert!(after.as_array().unwrap().is_empty(), "acknowledged updates are gone");

    // --- setWebhook delivery to a local receiver, secret header --------------------------------------------------------
    tg(&http, &public, &tok, "setWebhook", json!({"url": format!("{host}/tg-hook"), "secret_token": "s3cr3t"})).await;
    alice.send(&chan, "webhook me").await;
    eventually!("telegram webhook delivered", fake.on("/tg-hook").iter().any(|r| r.1.contains("webhook me")));
    let (h, body) = fake.on("/tg-hook").into_iter().find(|r| r.1.contains("webhook me")).unwrap();
    assert_eq!(h.get("x-telegram-bot-api-secret-token").unwrap(), "s3cr3t");
    assert!(serde_json::from_str::<Value>(&body).unwrap()["update_id"].as_i64().unwrap() > 0);
    tg(&http, &public, &tok, "deleteWebhook", json!({})).await;

    // --- incoming webhook ------------------------------------------------------------------------------------------------
    let inc = admin.create_incoming_webhook("CI builds", &chan).await.unwrap();
    let r = http.post(format!("{public}{}", inc.path)).json(&json!({"text": "build 42 passed"})).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 201);
    assert!(has_text(&alice, &chan, "build 42 passed").await);

    // --- outgoing webhook with a verified signature -----------------------------------------------------------------------
    let out = admin
        .create_outgoing_webhook(&CreateOutgoingWebhook { name: "Audit".into(), url: format!("{host}/audit"), secret: Some("topsecret".into()), events: vec!["message.created".into()], conv_id: Some(chan.clone()) })
        .await
        .unwrap();
    assert_eq!(out.secret, "topsecret");
    alice.send(&chan, "audit me").await;
    eventually!("outgoing delivered", fake.on("/audit").iter().any(|r| r.1.contains("audit me")));
    let (h, body) = fake.on("/audit").into_iter().find(|r| r.1.contains("audit me")).unwrap();
    let ts = h.get("x-workchat-timestamp").unwrap().to_str().unwrap().to_string();
    assert_eq!(h.get("x-workchat-signature").unwrap().to_str().unwrap(), webhook_signature("topsecret", &ts, body.as_bytes()));
    assert_eq!(h.get("x-workchat-event").unwrap(), "message.created");
    admin.update_outgoing_webhook(&out.id, &UpdateOutgoingWebhook { enabled: Some(false), ..Default::default() }).await.unwrap();

    // --- form submit round trip ---------------------------------------------------------------------------------------------
    let sent = tg(
        &http, &public, &tok, "sendMessage",
        json!({"chat_id": chan, "text": "Approve?", "form": {"id": "exp", "title": "Expense", "fields": [{"id": "ref", "type": "text", "label": "Reference", "required": true}], "buttons": [{"id": "ok", "label": "Approve", "style": "primary"}]}}),
    )
    .await;
    let mid = sent["message_id"].as_str().unwrap().to_string();
    assert!(alice.history(&chan).await.iter().any(|m| m.id == mid && m.form.is_some()), "the form arrives with the message");
    let submit = FormSubmit { button_id: None, values: serde_json::from_value(json!({"ref": "INV-7"})).unwrap() };
    let _: Message = alice.post(&format!("/messages/{mid}/form-response"), &submit).await.unwrap();
    let ups = tg(&http, &public, &tok, "getUpdates", json!({})).await;
    assert!(ups.as_array().unwrap().iter().any(|u| u["callback_query"]["values"]["ref"] == "INV-7"), "the bot received the form response");

    // --- scheduled message survives a server restart --------------------------------------------------------------------------
    let req = ScheduleRequest { body: "scheduled across restart".into(), due_ms: now_ms() + 6_000, thread_root: None, attachment_ids: vec![] };
    let _: ScheduledMessage = alice.post(&format!("/conversations/{chan}/scheduled"), &req).await.unwrap();
    assert!(!has_text(&alice, &chan, "scheduled across restart").await);
    let (ok, out) = sh("docker", &["restart", "-t", "1", NAME]);
    assert!(ok, "{out}");
    wait_http(&format!("{public}/api/v1/health")).await;
    for _ in 0..200 {
        if has_text(&alice, &chan, "scheduled across restart").await {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(has_text(&alice, &chan, "scheduled across restart").await, "scheduled message sent after the restart");

    // --- AI search with a fake provider ----------------------------------------------------------------------------------------
    let meta: ServerMeta = Client::anon(&public).get("/meta").await.unwrap();
    assert!(!meta.ai_search_enabled, "AI is off by default");
    let ask = AiSearchRequest { query: "invoice".into(), answer: false };
    assert!(alice.post::<AiSearchResponse, _>("/ai/search", &ask).await.is_err());
    admin
        .add_ai_provider(&AiProviderInput { name: "Fake".into(), kind: "anthropic".into(), api_key: "sk-fake-0000".into(), base_url: Some(host.clone()), ..Default::default() })
        .await
        .unwrap();
    let meta: ServerMeta = Client::anon(&public).get("/meta").await.unwrap();
    assert!(meta.ai_search_enabled);
    alice.send(&chan, "invoice 99 is paid").await;
    let ask = AiSearchRequest { query: "find the invoice".into(), answer: false };
    let r: AiSearchResponse = alice.post("/ai/search", &ask).await.unwrap();
    assert_eq!(r.interpreted.q, "invoice");
    assert!(r.results.hits.iter().any(|h| h.message.body == "invoice 99 is paid"));
    let calls = fake.on("/v1/messages");
    assert!(!calls.is_empty());
    assert_eq!(calls[0].0.get("x-api-key").unwrap(), "sk-fake-0000");
}
