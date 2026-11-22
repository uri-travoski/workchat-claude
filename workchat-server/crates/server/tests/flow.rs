//! Core flows over raw HTTP and WebSocket: sign-in, DMs, edits, deletes, reactions, idempotent sends, channels,
//! search isolation, files, cursor sync, account lifecycle (disable, password reset, lockout), clear-history,
//! date-range delete, storage move, presence, pagination, sessions, landing page, public URL.
mod common;
use common::*;
use std::time::Duration;

#[tokio::test]
async fn login_dm_and_realtime_delivery() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let mut ws = bob.ws(bob.cursor().await).await;

    let conv = alice.open_dm(&bob.user_id).await.unwrap();
    let sent = alice.send(&conv, "hello bob").await;
    assert_eq!(sent.body, "hello bob");
    assert_eq!(sent.seq, 1);
    // Live delivery on bob's socket.
    let ev = ws.wait_event("message_new", |e| matches!(e, EventData::MessageNew { message } if message.body == "hello bob")).await;
    assert_eq!(bob.texts(&conv).await, vec!["hello bob"]);
    // Unread count for bob, none for alice.
    let unread = |cs: Vec<ConversationSummary>| cs.into_iter().find(|c| c.conversation.id == conv).unwrap().unread;
    assert_eq!(unread(bob.conversations().await.unwrap()), 1);
    assert_eq!(unread(alice.conversations().await.unwrap()), 0);
    // Delivery state reaches the sender once the receiving device acknowledges the cursor.
    assert!(!alice.history(&conv).await[0].delivered);
    let a_cur = alice.cursor().await;
    ws.send(&ClientWsMsg::Ack { cursor: ev.cursor }).await;
    eventually!("alice sees delivered", alice.history(&conv).await[0].delivered);
    let evs = alice.events(a_cur - 1).await.unwrap().events;
    assert!(evs.iter().any(|e| matches!(&e.event, EventData::MessageDelivered { message_id, .. } if *message_id == sent.id)) || alice.history(&conv).await[0].delivered);
    // Read state syncs.
    bob.mark_read(&conv).await.unwrap();
    assert_eq!(unread(bob.conversations().await.unwrap()), 0);
    // Both people are the two members of a DM, whoever opens it.
    let again = bob.open_dm(&alice.user_id).await.unwrap();
    assert_eq!(again, conv);
    let c = bob.conversation(&conv).await.unwrap().conversation;
    assert_eq!(c.kind, ConversationKind::Dm);
    assert_eq!(c.members.len(), 2);
}

#[tokio::test]
async fn notes_to_self_exists() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let me = alice.self_conv().await;
    assert_eq!(alice.self_conv().await, me, "one notes-to-self conversation per person");
    alice.send(&me, "remember the milk").await;
    assert_eq!(alice.texts(&me).await, vec!["remember the milk"]);
    let mut ws = alice.ws(0).await;
    ws.send(&ClientWsMsg::Ack { cursor: alice.cursor().await }).await;
    // No delivery indicator for notes to self.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(alice.history(&me).await.iter().all(|m| !m.delivered));
}

#[tokio::test]
async fn edit_rule_dm_and_channel() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let conv = alice.open_dm(&bob.user_id).await.unwrap();

    let m = alice.send(&conv, "first").await;
    let edited = alice.edit(&m.id, "first (fixed)").await.unwrap();
    assert_eq!(edited.version, 2);
    assert!(edited.is_edited());
    assert_eq!(bob.texts(&conv).await, vec!["first (fixed)"]);
    let evs = bob.events(0).await.unwrap().events;
    assert!(evs.iter().any(|e| matches!(&e.event, EventData::MessageEdit { message } if message.id == m.id && message.version == 2)));

    // Bob replies; Alice's earlier message can no longer be edited (server decides).
    let reply = bob.send(&conv, "reply").await;
    let err = alice.edit(&m.id, "too late").await.unwrap_err();
    assert_eq!(err.code, Some(ErrorCode::EditWindowClosed));
    // Bob's reply is still editable: nobody replied after it.
    assert!(bob.edit(&reply.id, "reply!").await.is_ok());

    // Channels: editable forever, even after replies.
    let cid = s.channel("project", &[&alice, &bob]).await;
    let first = alice.send(&cid, "channel one").await;
    bob.send(&cid, "channel two").await;
    assert!(alice.edit(&first.id, "channel one (edited)").await.is_ok());
    // Only the author may edit.
    let err = bob.edit(&first.id, "hijack").await.unwrap_err();
    assert_eq!(err.code, Some(ErrorCode::Forbidden));
    // Empty edits are refused.
    assert_eq!(alice.edit(&first.id, "").await.unwrap_err().code, Some(ErrorCode::Validation));
}

#[tokio::test]
async fn delete_leaves_tombstone_for_everyone() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let conv = alice.open_dm(&bob.user_id).await.unwrap();
    let m = alice.send(&conv, "oops secret").await;
    assert_eq!(bob.search("secret").await.unwrap().hits.len(), 1);
    alice.delete_message(&m.id).await.unwrap();
    let t = &bob.history(&conv).await[0];
    assert!(t.deleted);
    assert!(t.body.is_empty());
    let evs = bob.events(0).await.unwrap().events;
    assert!(evs.iter().any(|e| matches!(&e.event, EventData::MessageDelete { message } if message.id == m.id)));
    // Bob cannot delete Alice's message.
    let m2 = alice.send(&conv, "again").await;
    assert_eq!(bob.delete_message(&m2.id).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    // Search no longer finds deleted text.
    assert!(bob.search("secret").await.unwrap().hits.is_empty());
}

#[tokio::test]
async fn reactions_sync() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let conv = alice.open_dm(&bob.user_id).await.unwrap();
    let m = alice.send(&conv, "react to me").await;
    let mut ws = alice.ws(alice.cursor().await).await;
    bob.react(&m.id, "👍", true).await.unwrap();
    ws.wait_event("reaction added", |e| matches!(e, EventData::ReactionUpdate { reactions, .. } if reactions.iter().any(|r| r.emoji == "👍" && r.user_ids == vec![bob.user_id.clone()]))).await;
    assert_eq!(alice.history(&conv).await[0].reactions.len(), 1);
    // Reacting twice does not stack.
    bob.react(&m.id, "👍", true).await.unwrap();
    assert_eq!(alice.history(&conv).await[0].reactions[0].user_ids.len(), 1);
    bob.react(&m.id, "👍", false).await.unwrap();
    ws.wait_event("reaction removed", |e| matches!(e, EventData::ReactionUpdate { reactions, .. } if reactions.is_empty())).await;
    assert!(alice.history(&conv).await[0].reactions.is_empty());
}

#[tokio::test]
async fn sends_are_idempotent() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let me = alice.self_conv().await;
    let req = SendMessage { client_id: "same-id".into(), body: "once".into(), thread_root: None, reply_to: None, mentions: vec![], attachment_ids: vec![], also_send_to_channel: false };
    let m1 = alice.send_req(&me, &req).await.unwrap();
    let m2 = alice.send_req(&me, &req).await.unwrap();
    assert_eq!(m1.id, m2.id);
    assert_eq!(m1.seq, m2.seq);
    assert_eq!(alice.history(&me).await.len(), 1);
}

#[tokio::test]
async fn private_channels_and_admin_only_creation() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    // With channel creation restricted to admins, members cannot create channels (group DMs stay open, see channels.rs).
    s.admin.set_org_settings(&UpdateOrgSettings { channel_creation: Some(ChannelCreation::Admins), channel_pins: None }).await.unwrap();
    let err = alice.create_channel("nope", &[]).await.unwrap_err();
    assert_eq!(err.code, Some(ErrorCode::Forbidden));
    // No public channels exist: only channel and group_dm kinds, both members-only.
    let cid = s.channel("secret-project", &[&alice]).await;
    assert!(alice.has_conversation(&cid).await);
    // Bob never learns about it, and direct access is hidden (404, not 403).
    assert!(!bob.has_conversation(&cid).await);
    assert_eq!(bob.conversation(&cid).await.unwrap_err().code, Some(ErrorCode::NotFound));
    assert_eq!(bob.history_raw(&cid, "").await.unwrap_err().code, Some(ErrorCode::NotFound));
    assert_eq!(bob.try_send(&cid, "let me in").await.unwrap_err().code, Some(ErrorCode::NotFound));
    // Adding Bob makes it appear automatically (M-01).
    s.admin.add_members(&cid, &[bob.user_id.clone()]).await.unwrap();
    assert!(bob.has_conversation(&cid).await);
    let evs = bob.events(0).await.unwrap().events;
    assert!(evs.iter().any(|e| matches!(&e.event, EventData::ConversationUpsert { conversation } if conversation.conversation.id == cid)));
    // Group DMs need at least two members (an admin creating one without joining it).
    assert!(s.admin.create_group("g", &[alice.user_id.clone()]).await.is_err());
}

#[tokio::test]
async fn search_is_isolated_between_users() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let ab = alice.open_dm(&bob.user_id).await.unwrap();
    alice.send(&ab, "the zebra plan is confidential").await;
    let ac = alice.open_dm(&carol.user_id).await.unwrap();
    alice.send(&ac, "carol gets zebra stripes").await;
    assert_eq!(alice.search("zebra").await.unwrap().hits.len(), 2);
    let rb = bob.search("zebra").await.unwrap();
    assert_eq!(rb.hits.len(), 1);
    assert!(rb.hits[0].snippet.contains("[[zebra]]"));
    let rc = carol.search("zebra").await.unwrap();
    assert_eq!(rc.hits.len(), 1);
    assert_eq!(rc.hits[0].message.conv_id, ac);
    // Bob cannot see Alice-Carol content even with a filter on that conversation.
    let r: SearchResponse = bob.get(&format!("/search?q=stripes&in={ac}")).await.unwrap();
    assert!(r.hits.is_empty());
}

#[tokio::test]
async fn upload_download_and_size_limit() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let conv = alice.open_dm(&bob.user_id).await.unwrap();
    let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    alice.send_file(&conv, "see attached", "notes.txt", data.clone()).await;
    let att = bob.history(&conv).await[0].attachments[0].clone();
    assert_eq!(att.name, "notes.txt");
    assert_eq!(att.size, 300_000);
    assert_eq!(bob.download(&att.id).await.unwrap(), data);
    // Non-members cannot fetch it.
    assert_eq!(carol.download(&att.id).await.unwrap_err().code, Some(ErrorCode::NotFound));
    // Over the limit: the server refuses with file_too_large (limit is 5 MB in tests) and stores nothing.
    let before = s.admin.storage().await.unwrap().attachments;
    let err = alice.upload("big.bin", vec![7u8; 6 * 1024 * 1024]).await.unwrap_err();
    assert_eq!(err.code, Some(ErrorCode::FileTooLarge));
    assert_eq!(s.admin.storage().await.unwrap().attachments, before);
    // Range requests work (video seeking).
    let r = bob.http.get(bob.url(&format!("/files/{}", att.id))).bearer_auth(bob.access_token()).header("Range", "bytes=10-19").send().await.unwrap();
    assert_eq!(r.status().as_u16(), 206);
    assert_eq!(r.bytes().await.unwrap().to_vec(), data[10..20].to_vec());
    // A signed URL downloads without a token.
    let signed: Value = bob.get(&format!("/files/{}/url", att.id)).await.unwrap();
    let url = signed["url"].as_str().unwrap().to_string();
    let url = if url.starts_with("http") { url } else { format!("{}{}", s.url, url) };
    let r = reqwest::Client::new().get(url).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 200);
    assert_eq!(r.bytes().await.unwrap().len(), data.len());
}

#[tokio::test]
async fn sync_after_offline_catches_up_by_cursor() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let conv = alice.open_dm(&bob.user_id).await.unwrap();
    alice.send(&conv, "m0").await;
    let cursor_before = bob.cursor().await;
    for i in 1..=5 {
        alice.send(&conv, &format!("m{i}")).await;
    }
    // Bob was offline: the HTTP event log returns exactly what he missed, in order.
    let r = bob.events(cursor_before).await.unwrap();
    let bodies: Vec<String> = r
        .events
        .iter()
        .filter_map(|e| match &e.event {
            EventData::MessageNew { message } => Some(message.body.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(bodies, vec!["m1", "m2", "m3", "m4", "m5"]);
    assert!(r.cursor > cursor_before);
    assert!(r.events.windows(2).all(|w| w[0].cursor < w[1].cursor));
    // The WebSocket replays the same backlog and then says it has caught up.
    let mut ws = bob.ws(cursor_before).await;
    let mut seen = vec![];
    loop {
        match ws.next(Duration::from_secs(5)).await.expect("backlog") {
            ServerWsMsg::Events { events, caught_up } => {
                for e in events {
                    if let EventData::MessageNew { message } = e.event {
                        seen.push(message.body);
                    }
                }
                if caught_up {
                    break;
                }
            }
            _ => {}
        }
    }
    assert_eq!(seen, vec!["m1", "m2", "m3", "m4", "m5"]);
    assert_eq!(bob.texts(&conv).await, vec!["m0", "m1", "m2", "m3", "m4", "m5"]);
}

#[tokio::test]
async fn disabled_account_is_kicked_and_cannot_return() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let conv = alice.open_dm(&bob.user_id).await.unwrap();
    alice.send(&conv, "before disable").await;
    let mut ws = bob.ws(bob.cursor().await).await;
    s.admin.set_disabled(&bob.user_id, true).await.unwrap();
    let (code, msg) = ws.wait_kicked().await;
    assert_eq!(code, ErrorCode::AccountDisabled);
    assert!(msg.contains("disabled"));
    // The old token is dead: the server answers account_disabled and the device must wipe (X-05).
    assert_eq!(bob.me().await.unwrap_err().code, Some(ErrorCode::AccountDisabled));
    assert!(ErrorCode::AccountDisabled.requires_wipe());
    // Cannot sign in again while disabled.
    let err = Client::login(&s.url, "bob", PW).await.err().unwrap();
    assert_eq!(err.code, Some(ErrorCode::AccountDisabled));
    // The disabled flag shows in the admin list.
    assert!(s.admin.users().await.unwrap().iter().any(|u| u.user.id == bob.user_id && u.user.disabled));
    // Re-enabled: signs in and the history comes back (cloud chats).
    s.admin.set_disabled(&bob.user_id, false).await.unwrap();
    let b2 = Client::login(&s.url, "bob", PW).await.unwrap();
    assert_eq!(b2.texts(&conv).await, vec!["before disable"]);
}

#[tokio::test]
async fn password_reset_forces_sign_out() {
    let s = TestServer::start().await;
    let bob = s.user("bob").await;
    let mut ws = bob.ws(bob.cursor().await).await;
    s.admin.reset_password(&bob.user_id, Some("brand-new-pass")).await.unwrap();
    let (code, _) = ws.wait_kicked().await;
    assert_eq!(code, ErrorCode::PasswordReset);
    let err = bob.me().await.unwrap_err();
    assert_eq!(err.code, Some(ErrorCode::PasswordReset));
    assert_eq!(err.status, 401);
    assert!(Client::login(&s.url, "bob", PW).await.is_err());
    assert!(Client::login(&s.url, "bob", "brand-new-pass").await.is_ok());
}

#[tokio::test]
async fn one_time_login_link() {
    let s = TestServer::start().await;
    let r = s.admin.create_user("newbie", "New Person", None).await.unwrap();
    let link = r.login_link.expect("link created when no password is given");
    assert!(link.link.starts_with(&format!("{}/l/", s.url)), "{}", link.link);
    assert!(link.link.ends_with(&link.token));
    assert!(!r.user.password_set);
    let c = Client::login_with_token(&s.url, &link.token).await.unwrap();
    let me = c.me().await.unwrap();
    assert_eq!(me.user.username, "newbie");
    assert!(!me.password_set);
    // The user then sets a password.
    c.change_password(None, "my-own-password").await.unwrap();
    assert!(c.me().await.unwrap().password_set);
    // One-time: the same link cannot be used again.
    let err = Client::login_with_token(&s.url, &link.token).await.err().unwrap();
    assert_eq!(err.code, Some(ErrorCode::InvalidCredentials));
    // A fresh link works once more.
    let l2 = s.admin.login_link(&r.user.user.id).await.unwrap();
    assert!(Client::login_with_token(&s.url, &l2.token).await.is_ok());
    // The new password works, the account has no other one.
    assert!(Client::login(&s.url, "newbie", "my-own-password").await.is_ok());
    assert!(Client::login(&s.url, "newbie", "").await.is_err());
}

#[tokio::test]
async fn failed_logins_lock_out_temporarily() {
    let s = TestServer::start_with(|c| {
        c.lockout_threshold = 3;
        c.lockout_secs = 60;
    })
    .await;
    s.admin.create_user("victim", "Victim", Some("correct-horse")).await.unwrap();
    for _ in 0..3 {
        let e = Client::login(&s.url, "victim", "wrong").await.err().unwrap();
        assert_eq!(e.code, Some(ErrorCode::InvalidCredentials));
    }
    let e = Client::login(&s.url, "victim", "correct-horse").await.err().unwrap();
    assert_eq!(e.code, Some(ErrorCode::LockedOut), "even the right password is refused during lockout");
    assert_eq!(e.status, 429);
}

#[tokio::test]
async fn clear_history_backs_up_hides_own_copy_only_and_moves_files() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let conv = alice.open_dm(&bob.user_id).await.unwrap();
    alice.send_file(&conv, "pic", "photo.png", vec![1u8; 4096]).await;
    alice.send(&conv, "and text").await;
    let att = bob.history(&conv).await[0].attachments[0].clone();
    let mut ws = alice.ws(alice.cursor().await).await;

    let r = s.admin.clear_history(&alice.user_id).await.unwrap();
    assert_eq!(r.messages_hidden, 2);
    assert_eq!(r.attachments_moved, 0, "bob still sees the file, so it must not move");
    assert!(r.backup_key.contains("backups/users/"));
    let (code, _) = ws.wait_kicked().await;
    assert_eq!(code, ErrorCode::HistoryCleared);
    assert_eq!(alice.me().await.unwrap_err().code, Some(ErrorCode::HistoryCleared));
    // Bob's copy is untouched, including the attachment.
    assert_eq!(bob.texts(&conv).await, vec!["pic", "and text"]);
    assert!(bob.download(&att.id).await.is_ok());
    // The backup exists in storage and is listed.
    let backups = s.admin.backups().await.unwrap();
    assert_eq!(backups.len(), 1);
    assert!(s.st.storage().head(&backups[0].key).await.unwrap().is_some());
    let doc: Value = serde_json::from_slice(&s.st.storage().get_all(&backups[0].key).await.unwrap()).unwrap();
    assert_eq!(doc["messages"].as_array().unwrap().len(), 2);
    assert_eq!(doc["user"]["username"], "alice");
    // Alice signs in again: an empty history, but her conversations are still there.
    let a2 = s.login("alice").await;
    assert!(a2.has_conversation(&conv).await);
    assert!(a2.texts(&conv).await.is_empty());
    assert!(!a2.history_raw(&conv, "limit=50").await.unwrap().has_more_before);
    // And she cannot fetch the file via the old message any more.
    assert_eq!(a2.download(&att.id).await.unwrap_err().code, Some(ErrorCode::NotFound));
    // New messages flow normally.
    bob.send(&conv, "welcome back").await;
    assert_eq!(a2.texts(&conv).await, vec!["welcome back"]);
}

#[tokio::test]
async fn clear_history_moves_orphaned_attachments_into_the_backup_folder() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let me = alice.self_conv().await;
    let m = alice.send_file(&me, "my file", "private.pdf", b"%PDF-1.4 secret".to_vec()).await;
    let att = m.attachments[0].clone();
    let key = format!("att/{}", att.id);
    assert!(s.st.storage().head(&key).await.unwrap().is_some());
    let r = s.admin.clear_history(&alice.user_id).await.unwrap();
    assert_eq!(r.attachments_moved, 1);
    // Moved, not erased.
    assert!(s.st.storage().head(&key).await.unwrap().is_none());
    let moved = format!("{}/att/{}", r.backup_key.trim_end_matches("/data.json"), att.id);
    assert!(s.st.storage().head(&moved).await.unwrap().is_some());
    // The storage verification pass still finds it (it follows the moved key).
    let j = s.admin.verify_storage(true).await.unwrap();
    let j = s.admin.wait_job(&j.id).await.unwrap();
    assert_eq!(j.state, "done", "{j:?}");
}

#[tokio::test]
async fn clear_history_is_refused_when_the_backup_fails() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let me = alice.self_conv().await;
    alice.send(&me, "keep me").await;
    // Make the storage backend unusable: point local storage at a path that is a file.
    let bad = s.dir.path().join("not-a-dir");
    std::fs::write(&bad, b"x").unwrap();
    let broken = workchat_server::storage::build_storage(&StorageConfig::Local { path: Some(bad.to_string_lossy().to_string()) }, s.dir.path());
    // Building may already fail; in that case swap in a read-only dir instead.
    match broken {
        Ok(b) => *s.st.storage.write().unwrap() = b,
        Err(_) => {
            let ro = s.dir.path().join("ro");
            std::fs::create_dir_all(&ro).unwrap();
            let b = workchat_server::storage::build_storage(&StorageConfig::Local { path: Some(ro.to_string_lossy().to_string()) }, s.dir.path()).unwrap();
            std::fs::remove_dir_all(&ro).unwrap();
            std::fs::write(&ro, b"x").unwrap();
            *s.st.storage.write().unwrap() = b;
        }
    }
    let err = s.admin.clear_history(&alice.user_id).await.unwrap_err();
    assert_eq!(err.code, Some(ErrorCode::BackupFailed));
    // Nothing changed: still signed in, message still there, no backup recorded.
    assert!(alice.me().await.is_ok());
    assert_eq!(alice.texts(&me).await, vec!["keep me"]);
    assert!(s.admin.backups().await.unwrap().is_empty());
}

#[tokio::test]
async fn delete_range_removes_only_selected_copy_by_default() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let conv = alice.open_dm(&bob.user_id).await.unwrap();
    let one = alice.send(&conv, "one").await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    let cut = now_ms();
    tokio::time::sleep(Duration::from_millis(30)).await;
    alice.send(&conv, "two").await;
    let r = s
        .admin
        .delete_range(&alice.user_id, &DateRangeDeleteRequest { from_ms: one.ts - 10, to_ms: cut, before_ms: None, conversation_id: None, also_other_copies: false })
        .await
        .unwrap();
    assert_eq!(r.messages_hidden, 1);
    assert_eq!(alice.texts(&conv).await, vec!["two"]);
    assert_eq!(bob.texts(&conv).await, vec!["one", "two"], "other person's copy stays");
    // Alice's device is told which message vanished from her view.
    let evs = alice.events(0).await.unwrap().events;
    assert!(evs.iter().any(|e| matches!(&e.event, EventData::MessageHide { message_ids, .. } if message_ids.contains(&one.id))));
    // With the option set, the other copy goes too.
    let r = s
        .admin
        .delete_range(&alice.user_id, &DateRangeDeleteRequest { from_ms: 0, to_ms: now_ms() + 10_000, before_ms: None, conversation_id: Some(conv.clone()), also_other_copies: true })
        .await
        .unwrap();
    assert_eq!(r.messages_hidden, 1);
    assert_eq!(bob.texts(&conv).await, vec!["one"]);
}

#[tokio::test]
async fn storage_move_between_backends_and_verify() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let me = alice.self_conv().await;
    let mut originals = vec![];
    for i in 0..3usize {
        let data = vec![i as u8 + 1; 10_000 + i * 777];
        alice.send_file(&me, "", &format!("f{i}.bin"), data.clone()).await;
        originals.push(data);
    }
    let info = s.admin.storage().await.unwrap();
    assert_eq!(info.attachments, 3);
    let target_dir = s.dir.path().join("other-storage");
    let target = StorageConfig::Local { path: Some(target_dir.to_string_lossy().to_string()) };
    // Test button
    assert!(s.admin.test_storage(&target).await.unwrap().ok);
    // Changing the backend directly is refused while files exist; migration is the way.
    assert_eq!(s.admin.set_storage(&target).await.unwrap_err().code, Some(ErrorCode::Conflict));
    let job = s.admin.migrate_storage(&target, true).await.unwrap();
    let job = s.admin.wait_job(&job.id).await.unwrap();
    assert_eq!(job.state, "done", "{job:?}");
    assert_eq!(job.done, 3);
    // Files physically moved.
    assert!(target_dir.join("att").exists());
    // The server still finds every attachment exactly as before.
    let hist = alice.history(&me).await;
    for (i, m) in hist.iter().filter(|m| !m.attachments.is_empty()).enumerate() {
        assert_eq!(alice.download(&m.attachments[0].id).await.unwrap(), originals[i]);
    }
    let v = s.admin.verify_storage(true).await.unwrap();
    let v = s.admin.wait_job(&v.id).await.unwrap();
    assert_eq!((v.state.as_str(), v.done, v.failed), ("done", 3, 0));
    // Break one file in the new storage: verification reports it.
    let victim = std::fs::read_dir(target_dir.join("att")).unwrap().next().unwrap().unwrap().path();
    std::fs::remove_file(victim).unwrap();
    let v = s.admin.verify_storage(false).await.unwrap();
    let v = s.admin.wait_job(&v.id).await.unwrap();
    assert_eq!(v.state, "failed");
    assert_eq!(v.failed, 1);
}

#[tokio::test]
async fn privacy_flag_controls_last_active_visibility() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    // The flag is off by default: the user may not hide anything.
    let req = UpdateMeRequest { hide_last_active: Some(true), ..Default::default() };
    let err = alice.patch::<Me, _>("/me", &req).await.unwrap_err();
    assert_eq!(err.code, Some(ErrorCode::Forbidden));
    assert!(!alice.me().await.unwrap().privacy_controls_enabled);
    // An admin selects Alice.
    s.allow_signout(&alice.user_id).await;
    assert!(alice.me().await.unwrap().privacy_controls_enabled);
    let me: Me = alice.patch("/me", &req).await.unwrap();
    assert!(me.hide_last_active);
    // Bob no longer receives Alice's last-active time.
    let users = bob.users_list().await.unwrap();
    assert!(users.iter().find(|u| u.id == alice.user_id).unwrap().last_active_ms.is_none());
    assert!(users.iter().find(|u| u.id == bob.user_id).unwrap().last_active_ms.is_some(), "users without the flag are unaffected");
    // Admin always sees the real value.
    let au = s.admin.users().await.unwrap().into_iter().find(|u| u.user.id == alice.user_id).unwrap();
    assert!(au.last_active_real_ms.is_some());
}

#[tokio::test]
async fn presence_and_typing_events() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let conv = alice.open_dm(&bob.user_id).await.unwrap();
    let mut wb = bob.ws(bob.cursor().await).await;
    let mut wa = alice.ws(alice.cursor().await).await;
    wa.send(&ClientWsMsg::Typing { conv_id: conv.clone() }).await;
    let m = wb.wait_msg("typing", |m| matches!(m, ServerWsMsg::Typing { .. })).await;
    assert_eq!(m, ServerWsMsg::Typing { conv_id: conv.clone(), user_id: alice.user_id.clone() });
    // Typing in someone else's conversation is ignored silently.
    let carol = s.user("carol").await;
    let mut wc = carol.ws(carol.cursor().await).await;
    wc.send(&ClientWsMsg::Typing { conv_id: conv.clone() }).await;
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    while let Some(m) = wb.next(deadline.saturating_duration_since(std::time::Instant::now())).await {
        assert!(!matches!(m, ServerWsMsg::Typing { .. }), "no typing from a non-member");
    }
    // Presence: set DND and see it on the other side.
    let _: Me = alice.patch("/me", &UpdateMeRequest { presence_pref: Some("dnd".into()), ..Default::default() }).await.unwrap();
    wb.wait_msg("dnd", |m| matches!(m, ServerWsMsg::Presence { user_id, presence: Presence::Dnd, .. } if *user_id == alice.user_id)).await;
    let _: Me = alice.patch("/me", &UpdateMeRequest { presence_pref: Some("auto".into()), ..Default::default() }).await.unwrap();
    wb.wait_msg("online", |m| matches!(m, ServerWsMsg::Presence { user_id, presence: Presence::Online, .. } if *user_id == alice.user_id)).await;
    // Pings are answered.
    wb.send(&ClientWsMsg::Ping).await;
    wb.wait_msg("pong", |m| matches!(m, ServerWsMsg::Pong)).await;
}

#[tokio::test]
async fn history_pagination_and_jump_to_date() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let me = alice.self_conv().await;
    for i in 0..130 {
        alice.send(&me, &format!("note {i}")).await;
    }
    // A fresh device only gets the recent window, then loads older pages on scroll.
    let phone = s.login("alice").await;
    let b = phone.bootstrap().await.unwrap();
    let initial = b.messages[&me].len();
    assert!(initial < 130, "bootstrap returns only recent messages (got {initial})");
    assert!(b.has_more[&me]);
    assert_eq!(b.messages[&me].last().unwrap().body, "note 129");
    let mut all = b.messages[&me].clone();
    while all.first().map(|m| m.seq).unwrap_or(1) > 1 {
        let before = all.first().unwrap().seq;
        let h = phone.history_raw(&me, &format!("before={before}&limit=50")).await.unwrap();
        assert!(!h.messages.is_empty());
        let mut page = h.messages;
        page.extend(all);
        all = page;
    }
    assert_eq!(all.len(), 130);
    assert_eq!(all[0].body, "note 0");
    assert!(all.windows(2).all(|w| w[1].seq == w[0].seq + 1), "gapless sequence numbers");
    // Jump to a date in the middle of the history.
    let mid_ts = all[60].ts;
    let h = phone.history_raw(&me, &format!("around_ts={mid_ts}&limit=60")).await.unwrap();
    assert!(h.messages.iter().any(|m| m.ts >= mid_ts));
    assert!(h.messages.len() <= 60 && !h.messages.is_empty());
}

#[tokio::test]
async fn cursor_expiry_triggers_rebootstrap() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let conv = alice.open_dm(&bob.user_id).await.unwrap();
    alice.send(&conv, "one").await;
    let bobs_cursor = bob.cursor().await;
    alice.send(&conv, "two").await;
    // The server prunes its event log past bob's cursor.
    s.st.db.call(|c| { c.execute("UPDATE events SET ts=0", [])?; Ok(()) }).await.unwrap();
    let floor = workchat_server::prune_events(&s.st).await.unwrap();
    assert!(floor > bobs_cursor);
    assert_eq!(bob.events(bobs_cursor).await.unwrap_err().code, Some(ErrorCode::CursorExpired));
    // The WebSocket says so too; a fresh bootstrap has everything.
    let mut ws = Ws::connect_raw(&s.url).await;
    ws.send(&ClientWsMsg::Hello { token: bob.access_token(), cursor: bobs_cursor, client_version: String::new() }).await;
    ws.wait_msg("cursor_expired", |m| matches!(m, ServerWsMsg::Error { code: ErrorCode::CursorExpired, .. })).await;
    let b = bob.bootstrap().await.unwrap();
    assert_eq!(b.messages[&conv].iter().map(|m| m.body.as_str()).collect::<Vec<_>>(), vec!["one", "two"]);
    assert!(bob.events(b.cursor).await.is_ok());
}

#[tokio::test]
async fn old_clients_are_told_to_update() {
    let s = TestServer::start().await;
    let anon = Client::anon(&s.url);
    let r = anon.raw_url(HttpMethod::GET, &anon.url("/me"), None, &[("X-Client-Version", "0.1.0"), ("Authorization", "Bearer x")]).await;
    assert_eq!(r.status, 403);
    assert_eq!(r.error().code, Some(ErrorCode::ClientTooOld));
    let meta: ServerMeta = anon.get("/meta").await.unwrap();
    assert_eq!(meta.api_version, API_VERSION);
    assert_eq!(meta.min_client_version, MIN_CLIENT_VERSION);
    assert_eq!(meta.name, "WorkChat (test)");
    assert!(!meta.setup_required);
    assert_eq!(meta.max_upload_bytes, 5 * 1024 * 1024);
    // A current version passes the gate (the bad token is then the only problem).
    let r = anon.raw_url(HttpMethod::GET, &anon.url("/me"), None, &[("X-Client-Version", "1.0.0"), ("Authorization", "Bearer x")]).await;
    assert_eq!(r.status, 401);
    // The health endpoints answer without credentials.
    assert_eq!(anon.raw_url(HttpMethod::GET, &format!("{}/healthz", s.url), None, &[]).await.status, 200);
    assert_eq!(anon.raw(HttpMethod::GET, "/health", None).await.status, 200);
}

#[tokio::test]
async fn registration_does_not_exist_and_setup_is_one_time() {
    let s = TestServer::start().await;
    let anon = Client::anon(&s.url);
    for path in ["/register", "/signup"] {
        let r = anon.raw(HttpMethod::POST, path, Some(json!({"username":"x","password":"y"}))).await;
        assert!(r.status == 404 || r.status == 405, "{path} must not exist ({})", r.status);
    }
    // Creating users needs an admin: anonymous and members are refused.
    let r = anon.raw(HttpMethod::POST, "/admin/users", Some(json!({"username":"x","display_name":"X"}))).await;
    assert_eq!(r.status, 401);
    let m = s.user("member").await;
    assert_eq!(m.create_user("y", "Y", Some("password-123")).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    let again = Client::setup(&s.url, &SetupRequest { username: "evil".into(), display_name: "Evil".into(), password: "password-123".into(), email: None, storage: None }).await;
    assert_eq!(again.err().and_then(|e| e.code), Some(ErrorCode::SetupDone));
}

#[tokio::test]
async fn setup_is_required_on_an_empty_server_and_validates_input() {
    let dir = tempfile::tempdir().unwrap();
    let st = workchat_server::build_state(Config::for_test(dir.path().join("server"))).await.unwrap();
    let url = workchat_server::serve_for_test(st).await.unwrap();
    let anon = Client::anon(&url);
    let meta: ServerMeta = anon.get("/meta").await.unwrap();
    assert!(meta.setup_required);
    // Everything else says so until the owner exists.
    assert!(Client::login(&url, "admin", "x").await.is_err());
    // A short password is refused and does not consume the wizard.
    let bad = Client::setup(&url, &SetupRequest { username: "admin".into(), display_name: "A".into(), password: "short".into(), email: None, storage: None }).await;
    assert_eq!(bad.err().unwrap().code, Some(ErrorCode::Validation));
    let c = Client::setup(&url, &SetupRequest { username: "admin".into(), display_name: "Admin".into(), password: "long-enough-pw".into(), email: None, storage: None }).await.unwrap();
    assert_eq!(c.me().await.unwrap().user.role, Role::Owner);
    assert!(!anon.get::<ServerMeta>("/meta").await.unwrap().setup_required);
}

#[tokio::test]
async fn sessions_list_and_revoke() {
    let s = TestServer::start().await;
    let phone = s.user("alice").await;
    // Without the admin's permission a member cannot sign other devices out (see signout.rs).
    s.allow_signout(&phone.user_id).await;
    let laptop = Client::login_device(&s.url, "alice", PW, "test-alice-laptop").await.unwrap();
    let mut ws = laptop.ws(laptop.cursor().await).await;
    let list = phone.sessions().await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list.iter().filter(|s| s.current).count(), 1);
    assert!(list.iter().any(|s| s.device_name == "test-alice-laptop"));
    phone.post_empty::<Value>("/sessions/revoke-others").await.unwrap();
    let (code, _) = ws.wait_kicked().await;
    assert_eq!(code, ErrorCode::SessionRevoked);
    assert_eq!(laptop.me().await.unwrap_err().code, Some(ErrorCode::SessionRevoked));
    assert!(phone.me().await.is_ok());
    // Single-session revoke by id.
    let tablet = Client::login_device(&s.url, "alice", PW, "tablet").await.unwrap();
    phone.delete::<Value>(&format!("/sessions/{}", tablet.session_id)).await.unwrap();
    assert_eq!(tablet.me().await.unwrap_err().code, Some(ErrorCode::SessionRevoked));
}

#[tokio::test]
async fn token_refresh_works_after_the_access_token_expired() {
    let s = TestServer::start_with(|c| c.access_ttl_secs = 1).await;
    s.admin.create_user("carol", "Carol", Some(PW)).await.unwrap();
    let c = Client::login(&s.url, "carol", PW).await.unwrap();
    assert!(c.me().await.is_ok());
    tokio::time::sleep(Duration::from_millis(2200)).await;
    assert_eq!(c.me().await.unwrap_err().code, Some(ErrorCode::TokenExpired));
    let before = c.access_token();
    c.refresh().await.unwrap();
    assert_ne!(c.access_token(), before, "a fresh access token was obtained");
    assert_eq!(c.me().await.unwrap().user.username, "carol");
    // A bogus refresh token is refused.
    let r = Client::anon(&s.url).post::<TokenPair, _>("/refresh", &RefreshRequest { refresh_token: "nope".into() }).await;
    assert!(r.is_err());
}

#[tokio::test]
async fn admin_removes_file_message_shows_file_removed() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let conv = alice.open_dm(&bob.user_id).await.unwrap();
    let m = alice.send_file(&conv, "f", "x.bin", b"abc".to_vec()).await;
    let att = m.attachments[0].clone();
    let c = bob.cursor().await;
    s.admin.remove_file(&att.id).await.unwrap();
    assert!(bob.history(&conv).await[0].attachments[0].removed);
    let evs = bob.events(c).await.unwrap().events;
    assert!(evs.iter().any(|e| matches!(&e.event, EventData::AttachmentRemoved { attachment_id } if *attachment_id == att.id)));
    assert!(bob.download(&att.id).await.is_err());
}

#[tokio::test]
async fn uploads_and_backups_pause_while_storage_is_moving() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    s.st.migrating.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(alice.upload("x.txt", b"hello".to_vec()).await.is_err(), "uploads are refused while files are moving");
    // Clear history needs a backup first, and backups are refused while files are moving.
    assert_eq!(s.admin.clear_history(&alice.user_id).await.unwrap_err().code, Some(ErrorCode::BackupFailed));
    s.st.migrating.store(false, std::sync::atomic::Ordering::SeqCst);
    assert!(alice.upload("x.txt", b"hello".to_vec()).await.is_ok());
    assert!(s.admin.backup_user(&alice.user_id).await.is_ok());
}

#[tokio::test]
async fn landing_page_checks_the_link_without_using_it_up() {
    let s = TestServer::start().await;
    let r = s.admin.create_user("lander", "Land Er", None).await.unwrap();
    let link = r.login_link.unwrap();
    let anon = Client::anon(&s.url);
    // Valid: the page offers the app, hands out the right deep links and is never cached or referred.
    for _ in 0..2 {
        let res = anon.raw_url(HttpMethod::GET, &link.link, None, &[]).await;
        assert_eq!(res.status, 200);
        assert_eq!(res.header("cache-control"), "no-store, max-age=0");
        assert_eq!(res.header("referrer-policy"), "no-referrer");
        assert!(res.header("content-security-policy").contains("default-src 'none'"));
        let html = res.text();
        assert!(html.contains("Open in WorkChat") && html.contains("intent://login?server="));
        assert!(html.contains("scheme=workchat;package=app.workchat"));
        assert!(html.contains("workchat://login?server="));
        assert!(html.contains("Copy login link"));
        assert!(!html.contains("src=\"http") && !html.contains("href=\"https://fonts"), "no third-party assets");
    }
    // GET did not consume it: signing in still works, once.
    Client::login_with_token(&s.url, &link.token).await.unwrap();
    let res = anon.raw_url(HttpMethod::GET, &link.link, None, &[]).await;
    assert_eq!(res.status, 410);
    assert!(res.text().contains("already been used"));
    // Unknown and malformed tokens.
    let res = anon.raw_url(HttpMethod::GET, &format!("{}/l/{}", s.url, "z".repeat(43)), None, &[]).await;
    assert_eq!(res.status, 404);
    assert!(res.text().contains("not valid"));
    assert_eq!(anon.raw_url(HttpMethod::GET, &format!("{}/l/short", s.url), None, &[]).await.status, 404);
    // The Host header is escaped, never echoed raw.
    let res = anon.raw_url(HttpMethod::GET, &link.link.replace("/l/", "/l/x"), None, &[("Host", "<script>alert(1)</script>")]).await;
    assert!(!res.text().contains("<script>alert"));
}

#[tokio::test]
async fn expired_link_is_reported_by_the_landing_page() {
    let s = TestServer::start().await;
    let r = s.admin.create_user("late", "Late", None).await.unwrap();
    let link = r.login_link.unwrap();
    s.expire_login_links().await;
    let res = Client::anon(&s.url).raw_url(HttpMethod::GET, &link.link, None, &[]).await;
    assert_eq!(res.status, 410);
    assert!(res.text().contains("expired"));
    // An expired link cannot sign in either.
    assert_eq!(Client::login_with_token(&s.url, &link.token).await.err().unwrap().code, Some(ErrorCode::InvalidCredentials));
}

#[tokio::test]
async fn public_url_setting_changes_new_links_and_env_wins() {
    let s = TestServer::start().await;
    let cur = s.admin.get_public_url().await.unwrap();
    assert_eq!(cur.source, "request");
    assert!(s.admin.set_public_url(Some("not a url")).await.is_err());
    let set = s.admin.set_public_url(Some("https://chat.example.org/")).await.unwrap();
    assert_eq!((set.source.as_str(), set.effective.as_str()), ("setting", "https://chat.example.org"));
    let r = s.admin.create_user("pub", "Pub", None).await.unwrap();
    assert!(r.login_link.unwrap().link.starts_with("https://chat.example.org/l/"));
    let cleared = s.admin.set_public_url(None).await.unwrap();
    assert_eq!(cleared.source, "request");
    // WORKCHAT_PUBLIC_URL (the config value) wins over the saved setting.
    let e = TestServer::start_with(|c| c.public_url = Some("https://env.example.org".into())).await;
    let set = e.admin.set_public_url(Some("https://chat.example.org")).await.unwrap();
    assert_eq!(set.source, "env");
    assert!(e.admin.create_user("p2", "P2", None).await.unwrap().login_link.unwrap().link.starts_with("https://env.example.org/l/"));
}
