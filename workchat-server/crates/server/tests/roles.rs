//! Role management: `PUT /admin/users/{id}/role` (owner only), its rules, the immediate effect on live
//! sessions, the event that updates the user's apps, and per-user backup/restore consistency.
mod common;
use common::*;

async fn can_admin(c: &Client) -> bool {
    c.users().await.is_ok()
}

#[tokio::test]
async fn owner_changes_roles_and_sessions_follow_immediately() {
    let s = TestServer::start().await;
    let bob = s.user("bob").await;
    let mut ws = bob.ws(bob.cursor().await).await;
    assert_eq!(bob.me().await.unwrap().user.role, Role::Member);
    assert!(!can_admin(&bob).await);
    s.admin.set_org_settings(&UpdateOrgSettings { channel_creation: Some(ChannelCreation::Admins), channel_pins: None }).await.unwrap();
    assert_eq!(bob.create_channel("mine", &[]).await.unwrap_err().code, Some(ErrorCode::Forbidden));

    // Member to admin: the already-open session gets the power at once, no new login.
    let u = s.admin.set_role(&bob.user_id, Role::Admin).await.unwrap();
    assert_eq!(u.user.role, Role::Admin);
    assert!(can_admin(&bob).await, "the same token is an admin right away");
    bob.create_channel("mine", &[]).await.expect("admins create channels");
    // The open app learns about it from an event: /me refreshes without a restart.
    ws.wait_event("me update admin", |e| matches!(e, EventData::MeUpdate { me } if me.user.role == Role::Admin)).await;

    // Admin to member: powers vanish at once, the app follows.
    s.admin.set_role(&bob.user_id, Role::Member).await.unwrap();
    assert!(!can_admin(&bob).await);
    assert_eq!(bob.create_channel("again", &[]).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    ws.wait_event("me update member", |e| matches!(e, EventData::MeUpdate { me } if me.user.role == Role::Member)).await;
    // The role is also what every other user sees.
    let alice = s.user("alice").await;
    let mut wa = alice.ws(alice.cursor().await).await;
    s.admin.set_role(&bob.user_id, Role::Admin).await.unwrap();
    wa.wait_event("others see the role", |e| matches!(e, EventData::UserUpsert { user } if user.id == bob.user_id && user.role == Role::Admin)).await;
    assert_eq!(alice.users_list().await.unwrap().iter().find(|u| u.id == bob.user_id).unwrap().role, Role::Admin);
    // Setting the same role again is a harmless no-op that returns the user.
    assert_eq!(s.admin.set_role(&bob.user_id, Role::Admin).await.unwrap().user.role, Role::Admin);
}

#[tokio::test]
async fn only_an_owner_changes_roles_and_never_their_own() {
    let s = TestServer::start().await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    s.admin.set_role(&bob.user_id, Role::Admin).await.unwrap();
    // An admin may use the admin panel but not change roles (not even to demote or promote a member).
    assert_eq!(bob.set_role(&carol.user_id, Role::Admin).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    assert_eq!(bob.set_role(&carol.user_id, Role::Member).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    assert_eq!(bob.set_role(&bob.user_id, Role::Owner).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    // A member is refused as well.
    assert_eq!(carol.set_role(&carol.user_id, Role::Admin).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    // The owner cannot change their own role.
    let me = s.admin.user_id_of("admin").await;
    assert_eq!(s.admin.set_role(&me, Role::Member).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    assert_eq!(s.admin.set_role(&me, Role::Admin).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    // Unknown user and invalid role are clean errors.
    assert_eq!(s.admin.set_role("nope", Role::Admin).await.unwrap_err().code, Some(ErrorCode::NotFound));
    let bad = s.admin.raw(HttpMethod::PUT, &format!("/admin/users/{}/role", carol.user_id), Some(json!({"role": "king"}))).await;
    assert!(bad.status >= 400 && bad.status < 500);
    // Nothing above changed anything.
    let users = s.admin.users().await.unwrap();
    let role = |n: &str| users.iter().find(|u| u.user.username == n).unwrap().user.role;
    assert_eq!((role("admin"), role("bob"), role("carol")), (Role::Owner, Role::Admin, Role::Member));
}

#[tokio::test]
async fn owners_can_be_added_and_one_owner_always_remains() {
    let s = TestServer::start().await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let admin_id = s.admin.user_id_of("admin").await;
    // Promote bob to owner: he then manages roles himself.
    assert_eq!(s.admin.set_role(&bob.user_id, Role::Owner).await.unwrap().user.role, Role::Owner);
    assert_eq!(bob.set_role(&carol.user_id, Role::Admin).await.unwrap().user.role, Role::Admin);
    // He can transfer ownership by demoting the first owner (he is not the last one) ...
    assert_eq!(bob.set_role(&admin_id, Role::Admin).await.unwrap().user.role, Role::Admin);
    // ... and the demoted former owner has lost the right to change roles at once.
    assert_eq!(s.admin.set_role(&carol.user_id, Role::Member).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    // The only remaining owner cannot demote themselves.
    assert_eq!(bob.set_role(&bob.user_id, Role::Member).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    let owners = bob.users().await.unwrap().into_iter().filter(|u| u.user.role == Role::Owner).count();
    assert_eq!(owners, 1);
}

#[tokio::test]
async fn bots_stay_members_and_disabled_users_can_be_changed() {
    let s = TestServer::start().await;
    let bot = s.admin.create_bot("tester_bot", "Tester", "t", &[]).await.unwrap();
    assert_eq!(s.admin.set_role(&bot.user.id, Role::Admin).await.unwrap_err().code, Some(ErrorCode::Validation));
    assert_eq!(s.admin.set_role(&bot.user.id, Role::Owner).await.unwrap_err().code, Some(ErrorCode::Validation));
    assert_eq!(s.admin.set_role(&bot.user.id, Role::Member).await.unwrap().user.role, Role::Member);

    let dan = s.user("dan").await;
    s.admin.set_disabled(&dan.user_id, true).await.unwrap();
    assert_eq!(s.admin.set_role(&dan.user_id, Role::Admin).await.unwrap().user.role, Role::Admin);
    assert!(s.admin.users().await.unwrap().iter().any(|u| u.user.id == dan.user_id && u.user.disabled && u.user.role == Role::Admin));
    // After re-enabling, he signs in with the new role.
    s.admin.set_disabled(&dan.user_id, false).await.unwrap();
    let dan2 = s.login("dan").await;
    assert_eq!(dan2.me().await.unwrap().user.role, Role::Admin);
    // Deleted accounts cannot be changed.
    let erin = s.user("erin").await;
    s.admin.delete_user(&erin.user_id).await.unwrap();
    assert_eq!(s.admin.set_role(&erin.user_id, Role::Admin).await.unwrap_err().code, Some(ErrorCode::Validation));
}

#[tokio::test]
async fn per_user_backup_and_restore_keep_roles_consistent() {
    let s = TestServer::start().await;
    let bob = s.user("bob").await;
    let bob_id = bob.user_id.clone();
    s.admin.set_role(&bob_id, Role::Admin).await.unwrap();
    let b1 = s.admin.backup_user(&bob_id).await.unwrap();
    // Restoring over a live account never touches its current role (the role is not part of "replace").
    s.admin.set_role(&bob_id, Role::Member).await.unwrap();
    s.admin.restore(&RestoreRequest { backup_id: b1.id.clone(), user_ids: vec![bob_id.clone()], all: false, mode: Some("replace".into()) }).await.unwrap();
    let role = |us: &[AdminUser]| us.iter().find(|u| u.user.id == bob_id).unwrap().user.role;
    assert_eq!(role(&s.admin.users().await.unwrap()), Role::Member);
    // A deleted account comes back with the role it had in the backup ...
    s.admin.set_role(&bob_id, Role::Admin).await.unwrap();
    s.admin.delete_user(&bob_id).await.unwrap();
    let bk = s.admin.backups().await.unwrap().into_iter().find(|b| b.user_id.as_deref() == Some(bob_id.as_str())).expect("deletion backup");
    s.admin.restore(&RestoreRequest { backup_id: bk.id, user_ids: vec![bob_id.clone()], all: false, mode: None }).await.unwrap();
    assert_eq!(role(&s.admin.users().await.unwrap()), Role::Admin);
    // ... and a backup that says "owner" never creates a second owner by accident: it comes back as admin.
    s.admin.set_role(&bob_id, Role::Owner).await.unwrap();
    let b2 = s.admin.backup_user(&bob_id).await.unwrap();
    s.admin.set_role(&bob_id, Role::Member).await.unwrap();
    s.admin.delete_user(&bob_id).await.unwrap();
    s.admin.restore(&RestoreRequest { backup_id: b2.id, user_ids: vec![bob_id.clone()], all: false, mode: None }).await.unwrap();
    assert_eq!(role(&s.admin.users().await.unwrap()), Role::Admin);
}
