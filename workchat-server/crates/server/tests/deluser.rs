//! Regression: deleting an account that has signed in (device sessions, push registrations, drafts, prefs ...) used to
//! answer 500; deleting must cascade and leave the other person's copy of the chats alone.
mod common;
use common::*;

#[tokio::test]
async fn deleting_a_user_who_has_signed_in_works_and_keeps_the_other_copy() {
    let s = TestServer::start().await;
    let bob = s.user("bob").await;
    // Carol signs in through a one-time link, on two devices, with a push registration, a draft, a preference.
    let r = s.admin.create_user("carol", "Carol", None).await.unwrap();
    let c_id = r.user.user.id.clone();
    let carol = Client::login_with_token(&s.url, &r.login_link.unwrap().token).await.unwrap();
    carol.change_password(None, "carol-password-1").await.unwrap();
    let mut ws = carol.ws(carol.cursor().await).await;
    let _carol2 = Client::login(&s.url, "carol", "carol-password-1").await.unwrap();
    let _: PushRegistration = carol
        .put("/me/push", &PushRegistration { id: String::new(), kind: "ntfy".into(), target: "topic-carol".into(), ntfy_url: None, device_name: "phone".into(), created_ms: 0 })
        .await
        .unwrap();
    let dm = carol.open_dm(&bob.user_id).await.unwrap();
    assert!(bob.has_conversation(&dm).await);
    let ch = s.channel("team", &[&bob, &carol]).await;
    carol.send(&dm, "hello from carol").await;
    carol.send(&ch, "channel hello").await;
    let _: DraftItem = carol.put(&format!("/conversations/{ch}/draft"), &DraftRequest { thread_root: None, body: "half written".into() }).await.unwrap();
    let _: ConvPref = carol.put(&format!("/conversations/{ch}/prefs"), &ConvPrefRequest { favourite: Some(true), ..Default::default() }).await.unwrap();
    carol.mark_read(&dm).await.unwrap();
    s.admin.create_bot("tester_bot", "Tester", "t", &[ch.clone()]).await.unwrap();

    s.admin.delete_user(&c_id).await.expect("deleting a signed-in user");
    ws.wait_kicked().await;
    assert!(Client::login(&s.url, "carol", "carol-password-1").await.is_err());
    // Bob keeps his copy of the DM and the channel history.
    assert!(bob.texts(&dm).await.contains(&"hello from carol".to_string()));
    assert!(bob.texts(&ch).await.contains(&"channel hello".to_string()));
    // Deleting again is a clean error, not a 500.
    let e = s.admin.delete_user(&c_id).await;
    assert!(e.is_err() && e.unwrap_err().code != Some(ErrorCode::Internal));
    // A second account can be deleted too (the placeholder user names must not collide).
    let dave = s.user("dave").await;
    let erin = s.user("erin").await;
    let mut wd = dave.ws(dave.cursor().await).await;
    s.admin.delete_user(&dave.user_id).await.expect("second deletion");
    s.admin.delete_user(&erin.user_id).await.expect("third deletion");
    wd.wait_kicked().await;
    // The server still works for everyone else.
    bob.send(&dm, "still here").await;
}
