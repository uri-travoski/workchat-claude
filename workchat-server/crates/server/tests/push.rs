//! Wake-up pushes through fake ntfy / FCM / APNs servers. Nothing here talks to the real services. Pushes carry no
//! message content and respect online state, Do Not Disturb, mute and notify level.
mod common;
use common::fakes::*;
use common::*;
use std::time::Duration;

/// Time for a push that must NOT happen to show up if it were going to.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(600)).await;
}

fn ntfy_reg(topic: &str) -> PushRegistration {
    PushRegistration { id: String::new(), kind: "ntfy".into(), target: topic.into(), ntfy_url: None, device_name: "phone".into(), created_ms: 0 }
}

async fn register(c: &Client, reg: &PushRegistration) -> Res<PushRegistration> {
    c.put("/me/push", reg).await
}

async fn set_conv_pref(c: &Client, conv: &str, req: &ConvPrefRequest) {
    let _: ConvPref = c.put(&format!("/conversations/{conv}/prefs"), req).await.unwrap();
}

async fn set_presence(c: &Client, pref: &str) {
    let _: Me = c.patch("/me", &UpdateMeRequest { presence_pref: Some(pref.into()), ..Default::default() }).await.unwrap();
}

/// Bob's app is connected (a live WebSocket), then goes away.
async fn connect_then_go_offline(s: &TestServer, c: &Client) {
    let ws = c.ws(c.cursor().await).await;
    eventually!("server sees the user online", s.st.hub.is_online(&c.user_id));
    drop(ws);
    eventually!("server sees the user offline", !s.st.hub.is_online(&c.user_id));
}

#[tokio::test]
async fn ntfy_wake_only_when_offline_and_content_free() {
    let (fake, url) = PushFake::start().await;
    let s = TestServer::start().await;
    s.admin.put_push_settings(&PushSettings { ntfy_url: Some(url.clone()), ..Default::default() }).await.unwrap();
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let dm = alice.open_dm(&bob.user_id).await.unwrap();
    let ch = s.channel("team", &[&alice, &bob]).await;
    let reg = register(&bob, &ntfy_reg("bob-topic")).await.unwrap();
    assert!(!reg.id.is_empty());
    assert_eq!(bob.get::<Vec<PushRegistration>>("/me/push").await.unwrap().len(), 1);

    // Bob has a live connection: no push.
    let ws = bob.ws(bob.cursor().await).await;
    eventually!("online", s.st.hub.is_online(&bob.user_id));
    alice.send(&dm, "secret plan A").await;
    settle().await;
    assert!(fake.pushes().is_empty(), "no push while the WebSocket is live");
    drop(ws);
    eventually!("offline", !s.st.hub.is_online(&bob.user_id));

    // Bob is offline: one content-free wake-up.
    alice.send(&dm, "secret plan B").await;
    eventually!("ntfy wake", fake.pushes().len() == 1);
    let p = fake.pushes().remove(0);
    assert_eq!((p.method.as_str(), p.path.as_str()), ("POST", "/bob-topic"));
    assert_eq!(p.body, "wake");
    let all = fake.dump();
    for leak in ["secret", "plan", "Alice", "alice", alice.user_id.as_str(), dm.as_str()] {
        assert!(!all.contains(leak), "push leaked {leak:?}: {all}");
    }

    // Do Not Disturb silences it.
    set_presence(&bob, "dnd").await;
    alice.send(&dm, "while dnd").await;
    settle().await;
    assert_eq!(fake.pushes().len(), 1);
    set_presence(&bob, "auto").await;

    // Muted conversation: silent.
    set_conv_pref(&bob, &dm, &ConvPrefRequest { mute_until_ms: Some(-1), ..Default::default() }).await;
    alice.send(&dm, "while muted").await;
    settle().await;
    assert_eq!(fake.pushes().len(), 1);
    set_conv_pref(&bob, &dm, &ConvPrefRequest { mute_until_ms: Some(0), ..Default::default() }).await;

    // Notify level "mentions": plain messages are silent, mentions in channels are not.
    set_conv_pref(&bob, &ch, &ConvPrefRequest { notify: Some("mentions".into()), ..Default::default() }).await;
    alice.send(&ch, "plain chatter").await;
    settle().await;
    assert_eq!(fake.pushes().len(), 1);
    alice.send_req(&ch, &SendMessage { client_id: uid(), body: "@bob ping".into(), thread_root: None, reply_to: None, mentions: vec![bob.user_id.clone()], attachment_ids: vec![], also_send_to_channel: false }).await.unwrap();
    eventually!("mention wake", fake.pushes().len() == 2);
    set_conv_pref(&bob, &ch, &ConvPrefRequest { notify: Some("nothing".into()), ..Default::default() }).await;
    alice.send_req(&ch, &SendMessage { client_id: uid(), body: "@bob ping again".into(), thread_root: None, reply_to: None, mentions: vec![bob.user_id.clone()], attachment_ids: vec![], also_send_to_channel: false }).await.unwrap();
    settle().await;
    assert_eq!(fake.pushes().len(), 2);

    // Unregistering stops pushes.
    let _: Value = bob.delete(&format!("/me/push/{}", reg.id)).await.unwrap();
    alice.send(&dm, "after unregister").await;
    settle().await;
    assert_eq!(fake.pushes().len(), 2);
}

#[tokio::test]
async fn per_device_ntfy_server_and_revoked_session_gets_nothing() {
    let (fake_default, url_default) = PushFake::start().await;
    let (fake_own, url_own) = PushFake::start().await;
    let s = TestServer::start().await;
    s.admin.put_push_settings(&PushSettings { ntfy_url: Some(url_default), ..Default::default() }).await.unwrap();
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let dm = alice.open_dm(&bob.user_id).await.unwrap();
    let mut reg = ntfy_reg("own-topic");
    reg.ntfy_url = Some(url_own);
    register(&bob, &reg).await.unwrap();
    alice.send(&dm, "hello").await;
    eventually!("own ntfy server used", fake_own.pushes().len() == 1);
    assert!(fake_default.pushes().is_empty());
    // Revoking bob's sessions removes his registrations from consideration.
    s.admin.revoke_user_sessions(&bob.user_id).await.unwrap();
    alice.send(&dm, "hello again").await;
    settle().await;
    assert_eq!(fake_own.pushes().len(), 1);
}

#[tokio::test]
async fn fcm_http_v1_and_apns_are_content_free() {
    let (fake, url) = PushFake::start().await;
    let s = TestServer::start().await;
    let sa = json!({
        "client_email": "svc@proj.iam.gserviceaccount.com",
        "private_key": include_str!("fixtures/test_rsa_pkcs8.pem"),
        "project_id": "proj-1"
    });
    s.admin
        .put_push_settings(&PushSettings {
            fcm_service_account: Some(sa.to_string()),
            fcm_api_base: Some(url.clone()),
            apns_key_pem: Some(include_str!("fixtures/test_ec_pkcs8.pem").to_string()),
            apns_key_id: Some("KEY123".into()),
            apns_team_id: Some("TEAM123".into()),
            apns_topic: Some("app.workchat".into()),
            apns_api_base: Some(url.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    // Reading the settings back never reveals secrets.
    let shown = s.admin.push_settings().await.unwrap();
    assert_eq!(shown.fcm_service_account.as_deref(), Some(PushSettings::MASK));
    assert_eq!(shown.apns_key_pem.as_deref(), Some(PushSettings::MASK));

    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let dm = alice.open_dm(&bob.user_id).await.unwrap();
    for (kind, token) in [("fcm", "fcm-device-token"), ("apns", "apns-device-token")] {
        register(&bob, &PushRegistration { id: String::new(), kind: kind.into(), target: token.into(), ntfy_url: None, device_name: kind.into(), created_ms: 0 }).await.unwrap();
    }
    alice.send(&dm, "top secret text").await;
    eventually!("both pushes", fake.pushes().len() == 2);
    let pushes = fake.pushes();
    let fcm = pushes.iter().find(|p| p.path.contains("messages:send")).expect("fcm request");
    assert_eq!(fcm.path, "/v1/projects/proj-1/messages:send");
    assert_eq!(fcm.headers.get("authorization").unwrap(), "Bearer fake-access");
    let j: Value = serde_json::from_str(&fcm.body).unwrap();
    assert_eq!(j["message"]["token"], "fcm-device-token");
    assert_eq!(j["message"]["data"]["type"], "wake");
    assert!(j["message"].get("notification").is_none(), "no notification payload: the app builds its own");
    // The OAuth exchange used a signed RS256 JWT bearer assertion.
    let tok = fake.token_requests().remove(0);
    assert!(tok.body.contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer"));
    let assertion = tok.body.split("assertion=").nth(1).unwrap();
    assert_eq!(assertion.split('.').count(), 3);

    let apns = pushes.iter().find(|p| p.path.starts_with("/3/device/")).expect("apns request");
    assert_eq!(apns.path, "/3/device/apns-device-token");
    let hv = |n: &str| apns.headers.get(n).map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
    assert!(hv("authorization").starts_with("bearer ") && hv("authorization").matches('.').count() == 2);
    assert_eq!(hv("apns-push-type"), "background");
    assert_eq!(hv("apns-topic"), "app.workchat");
    let j: Value = serde_json::from_str(&apns.body).unwrap();
    assert_eq!(j["aps"]["content-available"], 1);
    assert!(j["aps"].get("alert").is_none());
    let all = fake.dump();
    assert!(!all.contains("secret") && !all.contains("Alice"), "{all}");
}
