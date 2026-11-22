//! Self-service sign-out is gated by the admin's "Privacy options" (Allow) setting, on the server.
mod common;
use common::*;

async fn set_allow(s: &TestServer, id: &str, on: bool) {
    s.admin.update_user(id, &UpdateUserRequest { privacy_controls_enabled: Some(on), ..Default::default() }).await.unwrap();
}

async fn second_device(s: &TestServer, name: &str) -> Client {
    Client::login_device(&s.url, name, PW, &format!("{name}-2")).await.unwrap()
}

#[tokio::test]
async fn disallow_refuses_every_self_service_sign_out_and_keeps_sessions() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let laptop = second_device(&s, "alice").await;
    assert!(!alice.me().await.unwrap().privacy_controls_enabled);
    assert_eq!(alice.logout().await.unwrap_err().code, Some(ErrorCode::Forbidden));
    assert_eq!(alice.post_empty::<Value>("/sessions/revoke-all").await.unwrap_err().code, Some(ErrorCode::Forbidden));
    assert_eq!(alice.post_empty::<Value>("/sessions/revoke-others").await.unwrap_err().code, Some(ErrorCode::Forbidden));
    let list = alice.sessions().await.unwrap();
    assert_eq!(list.len(), 2);
    let other = list.iter().find(|x| !x.current).unwrap().id.clone();
    let own = list.iter().find(|x| x.current).unwrap().id.clone();
    let e = alice.delete::<Value>(&format!("/sessions/{other}")).await.unwrap_err();
    assert_eq!(e.code, Some(ErrorCode::Forbidden));
    assert!(e.message.contains("sign-out is not allowed for this account"), "{e}");
    assert_eq!(alice.delete::<Value>(&format!("/sessions/{own}")).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    // Nothing was wiped or ended.
    assert_eq!(alice.sessions().await.unwrap().len(), 2);
    assert!(alice.me().await.is_ok() && laptop.me().await.is_ok());
}

#[tokio::test]
async fn allow_enables_sign_out_and_logout_all_ends_every_device() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let laptop = second_device(&s, "alice").await;
    s.allow_signout(&alice.user_id).await;
    let mut ws = laptop.ws(laptop.cursor().await).await;
    alice.post_empty::<Value>("/sessions/revoke-all").await.unwrap();
    let (code, _) = ws.wait_kicked().await;
    assert_eq!(code, ErrorCode::SessionRevoked);
    assert_eq!(alice.me().await.unwrap_err().code, Some(ErrorCode::SessionRevoked));
    assert_eq!(laptop.me().await.unwrap_err().code, Some(ErrorCode::SessionRevoked));
    // Both sessions are gone on the server.
    let list = s.admin.all_sessions().await.unwrap();
    assert!(list.iter().all(|x| x.user_id != alice.user_id || x.revoked), "{list:?}");

    // Plain sign-out of one device.
    let again = second_device(&s, "alice").await;
    let keep = second_device(&s, "alice").await;
    again.logout().await.unwrap();
    assert_eq!(again.me().await.unwrap_err().code, Some(ErrorCode::SessionRevoked));
    assert!(keep.me().await.is_ok());
}

#[tokio::test]
async fn flipping_the_flag_is_followed_live_via_me_update() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let mut ws = alice.ws(alice.cursor().await).await;
    set_allow(&s, &alice.user_id, true).await;
    ws.wait_event("allowed", |e| matches!(e, EventData::MeUpdate { me } if me.privacy_controls_enabled)).await;
    assert!(alice.me().await.unwrap().privacy_controls_enabled);
    set_allow(&s, &alice.user_id, false).await;
    ws.wait_event("disallowed again", |e| matches!(e, EventData::MeUpdate { me } if !me.privacy_controls_enabled)).await;
    assert_eq!(alice.logout().await.unwrap_err().code, Some(ErrorCode::Forbidden));
    assert!(alice.me().await.is_ok());
}

#[tokio::test]
async fn admins_are_never_locked_out_and_admin_panel_ends_any_session() {
    let s = TestServer::start().await;
    let bob = s.user("bob").await;
    s.admin.set_role(&bob.user_id, Role::Admin).await.unwrap();
    assert!(!bob.me().await.unwrap().privacy_controls_enabled);
    let phone = second_device(&s, "bob").await;
    let mut ws = phone.ws(phone.cursor().await).await;
    bob.post_empty::<Value>("/sessions/revoke-others").await.unwrap();
    ws.wait_kicked().await;
    assert_eq!(phone.me().await.unwrap_err().code, Some(ErrorCode::SessionRevoked));
    bob.logout().await.unwrap();
    assert_eq!(bob.me().await.unwrap_err().code, Some(ErrorCode::SessionRevoked));

    // An admin may end a member's sessions from the panel even without Allow.
    let carol = s.user("carol").await;
    let mut ws = carol.ws(carol.cursor().await).await;
    s.admin.revoke_user_sessions(&carol.user_id).await.unwrap();
    let (code, _) = ws.wait_kicked().await;
    assert_eq!(code, ErrorCode::SessionRevoked);
}

#[tokio::test]
async fn forced_wipes_still_work_without_allow() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let mut ws = alice.ws(alice.cursor().await).await;
    s.admin.set_disabled(&alice.user_id, true).await.unwrap();
    assert_eq!(ws.wait_kicked().await.0, ErrorCode::AccountDisabled);
    let mut ws = bob.ws(bob.cursor().await).await;
    s.admin.reset_password(&bob.user_id, Some("brand-new-pass")).await.unwrap();
    assert_eq!(ws.wait_kicked().await.0, ErrorCode::PasswordReset);
    let mut ws = carol.ws(carol.cursor().await).await;
    s.admin.clear_history(&carol.user_id).await.unwrap();
    assert_eq!(ws.wait_kicked().await.0, ErrorCode::HistoryCleared);
}
