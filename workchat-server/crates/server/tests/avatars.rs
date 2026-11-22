//! 1.4.1: channel pictures everyone sees, and private pictures a person chose for other users, DMs, groups and channels.
mod common;
use common::*;

fn pic(tag: u8) -> Vec<u8> {
    let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
    b.extend(std::iter::repeat(tag).take(64));
    b
}

fn tag_of(bytes: Vec<u8>) -> u8 {
    *bytes.last().unwrap()
}

async fn set_conv_avatar(c: &Client, conv: &str, tag: u8) -> Res<ConversationSummary> {
    let a = c.upload_as("avatar.png", "image/png", Some("avatar"), pic(tag)).await?;
    c.put(&format!("/conversations/{conv}/avatar"), &SetConvAvatar { avatar: a.id }).await
}

async fn clear_conv_avatar(c: &Client, conv: &str) -> Res<ConversationSummary> {
    c.put(&format!("/conversations/{conv}/avatar"), &SetConvAvatar { avatar: String::new() }).await
}

async fn set_override(c: &Client, kind: &str, target: &str, tag: u8) -> Res<AvatarOverride> {
    let a = c.upload_as("private.png", "image/png", Some("avatar_private"), pic(tag)).await?;
    c.put(&format!("/me/avatar-overrides/{kind}/{target}"), &SetAvatarOverride { attachment: a.id }).await
}

async fn overrides(c: &Client) -> Vec<AvatarOverride> {
    c.get("/me/avatar-overrides").await.unwrap()
}

async fn conv_avatar(c: &Client, conv: &str) -> Option<String> {
    c.conversation(conv).await.unwrap().conversation.avatar
}

async fn user_avatar(c: &Client, user: &str) -> Option<String> {
    c.users_list().await.unwrap().into_iter().find(|u| u.id == user).unwrap().avatar
}

#[tokio::test]
async fn admins_set_a_channel_picture_everyone_sees_and_members_cannot() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let ch = s.channel("design", &[&alice, &bob]).await;
    assert!(conv_avatar(&bob, &ch).await.is_none(), "initials until a picture is set");
    // A plain member may not set it.
    assert!(set_conv_avatar(&bob, &ch, 1).await.is_err());
    // A server admin may (alice is made an admin).
    s.admin.set_role(&alice.user_id, Role::Admin).await.unwrap();
    let mut ws = bob.ws(bob.cursor().await).await;
    let cs = set_conv_avatar(&alice, &ch, 7).await.unwrap();
    let att = cs.conversation.avatar.clone().expect("avatar set");
    ws.wait_event("channel picture event", |e| matches!(e, EventData::ConversationUpsert { conversation } if conversation.conversation.id == ch && conversation.conversation.avatar.is_some())).await;
    assert_eq!(conv_avatar(&bob, &ch).await, Some(att.clone()));
    assert_eq!(tag_of(bob.download(&att).await.unwrap()), 7, "members can fetch the picture");
    // Removal falls back to initials for everyone.
    clear_conv_avatar(&alice, &ch).await.unwrap();
    assert!(conv_avatar(&bob, &ch).await.is_none());
    assert!(conv_avatar(&alice, &ch).await.is_none());
    // Channels you are not in are not reachable.
    let carol = s.user("carol").await;
    assert!(set_conv_avatar(&carol, &ch, 3).await.is_err());
}

#[tokio::test]
async fn personal_pictures_are_private_win_over_the_real_one_and_fall_back_when_removed() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let ch = s.channel("general", &[&alice, &bob, &carol]).await;
    // Bob has a profile picture of his own (tag 2).
    let own = bob.upload_as("bob.png", "image/png", Some("avatar"), pic(2)).await.unwrap();
    let _: Me = bob.patch("/me", &UpdateMeRequest { avatar: Some(own.id.clone()), ..Default::default() }).await.unwrap();
    assert_eq!(user_avatar(&alice, &bob.user_id).await, Some(own.id.clone()));
    assert_eq!(user_avatar(&carol, &bob.user_id).await, Some(own.id.clone()));

    // Alice sets a private picture for bob: only her override list has it.
    let o = set_override(&alice, "user", &bob.user_id, 9).await.unwrap();
    assert_eq!((o.kind.as_str(), o.target.as_str()), ("user", bob.user_id.as_str()));
    assert_eq!(tag_of(alice.download(&o.attachment).await.unwrap()), 9, "she can read her private picture");
    assert!(overrides(&alice).await.iter().any(|x| x.target == bob.user_id && x.attachment == o.attachment));
    assert!(overrides(&carol).await.is_empty() && overrides(&bob).await.is_empty());
    assert_eq!(user_avatar(&carol, &bob.user_id).await, Some(own.id.clone()), "carol still sees bob's own photo");
    assert_eq!(user_avatar(&bob, &bob.user_id).await, Some(own.id.clone()), "bob sees his own photo");
    // The file itself is readable only by the person who chose it (not by bob or carol, even with its id).
    assert!(bob.download(&o.attachment).await.is_err() && carol.download(&o.attachment).await.is_err());

    // A second device of alice gets it through the same list (and the event log).
    let alice2 = s.login("alice").await;
    assert!(overrides(&alice2).await.iter().any(|x| x.target == bob.user_id));
    let evs = alice2.events(0).await.unwrap().events;
    assert!(evs.iter().any(|e| matches!(&e.event, EventData::AvatarOverrideUpdate { kind, target, attachment: Some(_) } if kind == "user" && *target == bob.user_id)));
    // ... and no one else's event log carries it.
    assert!(!carol.events(0).await.unwrap().events.iter().any(|e| matches!(&e.event, EventData::AvatarOverrideUpdate { .. })));

    // A private picture for a channel, with the channel's own picture set by an admin underneath.
    s.admin.set_role(&carol.user_id, Role::Admin).await.unwrap();
    let chan = set_conv_avatar(&carol, &ch, 4).await.unwrap().conversation.avatar.unwrap();
    assert_eq!(conv_avatar(&alice, &ch).await, Some(chan.clone()));
    let oc = set_override(&alice, "conv", &ch, 5).await.unwrap();
    assert_eq!(conv_avatar(&bob, &ch).await, Some(chan.clone()), "bob sees the channel's picture, not alice's");
    assert_eq!(conv_avatar(&alice, &ch).await, Some(chan.clone()), "the override is a separate list, the shared picture is unchanged");
    assert!(overrides(&alice2).await.iter().any(|x| x.kind == "conv" && x.attachment == oc.attachment));

    // Removal: the real picture (or initials) shows again, on every device of alice.
    alice.delete::<Value>(&format!("/me/avatar-overrides/user/{}", bob.user_id)).await.unwrap();
    assert!(!overrides(&alice2).await.iter().any(|x| x.kind == "user"));
    let evs = alice2.events(0).await.unwrap().events;
    assert!(evs.iter().any(|e| matches!(&e.event, EventData::AvatarOverrideUpdate { kind, attachment: None, .. } if kind == "user")));
    alice.delete::<Value>(&format!("/me/avatar-overrides/conv/{ch}")).await.unwrap();
    assert!(overrides(&alice).await.is_empty());
    // Bob removes his own photo: everyone sees initials.
    let _: Me = bob.patch("/me", &UpdateMeRequest { avatar: Some(String::new()), ..Default::default() }).await.unwrap();
    assert!(user_avatar(&carol, &bob.user_id).await.is_none());
    // Not for yourself, not for strangers' conversations.
    assert!(set_override(&alice, "user", &alice.user_id, 1).await.is_err());
    let other = s.channel("secret", &[&bob]).await;
    assert!(set_override(&alice, "conv", &other, 1).await.is_err());
}

#[tokio::test]
async fn personal_pictures_are_in_the_per_user_backup_and_come_back_on_restore() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let first = set_override(&alice, "user", &bob.user_id, 6).await.unwrap();
    let info = s.admin.backup_user(&alice.user_id).await.unwrap();
    // Changed afterwards: replace-restore makes the view match the backup again.
    let second = set_override(&alice, "user", &bob.user_id, 7).await.unwrap();
    assert_ne!(first.attachment, second.attachment);
    s.admin.restore(&RestoreRequest { backup_id: info.id.clone(), user_ids: vec![alice.user_id.clone()], all: false, mode: Some("replace".into()) }).await.unwrap();
    let cur = overrides(&alice).await.into_iter().find(|x| x.target == bob.user_id).unwrap();
    assert_eq!(tag_of(alice.download(&cur.attachment).await.unwrap()), 6, "the backed-up picture is back");
    // Removed afterwards: a merge restore brings it back.
    alice.delete::<Value>(&format!("/me/avatar-overrides/user/{}", bob.user_id)).await.unwrap();
    assert!(overrides(&alice).await.is_empty());
    s.admin.restore(&RestoreRequest { backup_id: info.id, user_ids: vec![alice.user_id.clone()], all: false, mode: None }).await.unwrap();
    let cur = overrides(&alice).await.into_iter().find(|x| x.target == bob.user_id).unwrap();
    assert_eq!(tag_of(alice.download(&cur.attachment).await.unwrap()), 6, "restored by merge");
}
