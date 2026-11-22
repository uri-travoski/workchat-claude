//! Admin chat viewer API and deletion with mandatory backup: single / bulk / date-range deletes for
//! everyone or for one person, restore of deleted messages, and refusal when the backup fails.
mod common;
use common::*;
use std::time::Duration;

struct Fixture {
    s: TestServer,
    alice: Client,
    bob: Client,
    ch: String,
}

async fn setup() -> Fixture {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let ch = s.channel("general", &[&alice, &bob]).await;
    Fixture { s, alice, bob, ch }
}

fn delete_req(ids: &[&str], scope: &str) -> AdminDeleteMessagesRequest {
    AdminDeleteMessagesRequest { message_ids: ids.iter().map(|s| s.to_string()).collect(), conv_id: None, from_ms: None, to_ms: None, before_ms: None, scope: scope.into(), user_id: None }
}

#[tokio::test]
async fn viewer_lists_pages_and_searches_any_conversation() {
    let Fixture { s, alice, bob, ch } = setup().await;
    let dm = alice.open_dm(&bob.user_id).await.unwrap();
    for i in 0..30 {
        alice.send(&ch, &format!("message number {i:02} about apples")).await;
    }
    bob.send(&dm, "private pears").await;
    bob.send_file(&ch, "photo", "pic.png", vec![0x89, b'P', b'N', b'G', 9, 9]).await;

    // Conversations: everything shared, filtered by kind, or one person's view.
    let all = s.admin.viewer_conversations(None, None, None).await.unwrap();
    assert!(all.iter().any(|c| c.title == "general" && c.message_count == 31));
    let dms = s.admin.viewer_conversations(None, Some("dm"), None).await.unwrap();
    assert!(dms.iter().any(|c| c.conversation.id == dm), "{dms:?}");
    let alice_view = s.admin.viewer_conversations(Some(&alice.user_id), None, None).await.unwrap();
    let dm_row = alice_view.iter().find(|c| c.conversation.id == dm).unwrap();
    assert_eq!(dm_row.title, "Bob", "a DM is titled with the other person");
    assert_eq!(dm_row.last_message.as_ref().unwrap().body, "private pears");
    assert!(alice_view.iter().any(|c| c.title.starts_with("Notes to self")));
    assert_eq!(s.admin.viewer_conversations(None, None, Some("gener")).await.unwrap().len(), 1);

    // Paging: newest page, older page, a date window.
    let page = s.admin.viewer_messages(&ch, "limit=10").await.unwrap();
    assert_eq!(page.messages.len(), 10);
    assert!(page.has_more_before && !page.has_more_after);
    assert_eq!(page.messages.last().unwrap().body, "photo");
    assert_eq!(page.messages.last().unwrap().attachments.len(), 1, "photos come with their attachment metadata");
    let first_seq = page.messages[0].seq;
    let older = s.admin.viewer_messages(&ch, &format!("limit=10&before={first_seq}")).await.unwrap();
    assert_eq!(older.messages.len(), 10);
    assert!(older.messages.last().unwrap().seq < first_seq);
    let ts = older.messages[0].ts;
    let window = s.admin.viewer_messages(&ch, &format!("from_ms={ts}&to_ms={}&limit=200", older.messages[3].ts)).await.unwrap();
    assert!(window.messages.len() >= 4 && window.messages.iter().all(|m| m.ts >= ts && m.ts <= older.messages[3].ts));
    let around = s.admin.viewer_messages(&ch, &format!("around_id={}&limit=6", older.messages[5].id)).await.unwrap();
    assert!(around.messages.iter().any(|m| m.id == older.messages[5].id) && around.has_more_after && around.has_more_before);

    // Search across everything, within a date range, as a given user.
    let hits = s.admin.viewer_search("q=apples&limit=100").await.unwrap();
    assert_eq!(hits.len(), 30);
    assert!(hits[0].snippet.contains("[[apples]]"));
    let hits = s.admin.viewer_search("q=pears").await.unwrap();
    assert_eq!(hits.len(), 1, "admins can search private DMs of others");
    let now = now_ms();
    assert!(s.admin.viewer_search(&format!("q=apples&to_ms={}", now - 10_000_000)).await.unwrap().is_empty());
    assert_eq!(s.admin.viewer_search(&format!("q=apples&from_ms={}&to_ms={}", now - 10_000_000, now + 10_000_000)).await.unwrap().len(), 30);
    assert_eq!(s.admin.viewer_search(&format!("user_id={}&q=pears", bob.user_id)).await.unwrap().len(), 1);
    assert_eq!(s.admin.viewer_search("q=a&has=file").await.unwrap().len(), 0);
    assert_eq!(s.admin.viewer_search("has=file&limit=5").await.unwrap().len(), 1);
    // Normal users cannot use any of it.
    assert_eq!(alice.get::<Vec<ViewerConversation>>("/admin/viewer/conversations").await.unwrap_err().code, Some(ErrorCode::Forbidden));
    assert!(alice.viewer_messages(&ch, "limit=5").await.is_err());
    assert!(alice.viewer_search("q=apples").await.is_err());
}

#[tokio::test]
async fn bulk_delete_for_everyone_backs_up_first_and_can_be_restored() {
    let Fixture { s, alice, bob, ch } = setup().await;
    let with_file = alice.send_file(&ch, "with attachment", "evidence.txt", b"evidence bytes".to_vec()).await;
    let m1 = alice.send(&ch, "delete me").await;
    let m2 = bob.send(&ch, "delete me too").await;
    let keep = bob.send(&ch, "keep me").await;
    let att = with_file.attachments[0].clone();

    // A plain user cannot do it.
    assert!(alice.post::<AdminDeleteMessagesResponse, _>("/admin/messages/delete", &delete_req(&[&m1.id], "everyone")).await.is_err());
    let r = s.admin.delete_messages(&delete_req(&[&m1.id, &m2.id, &with_file.id], "everyone")).await.unwrap();
    assert_eq!(r.messages_deleted, 3);
    assert_eq!(r.attachments_moved, 1);
    let b = s.admin.backups().await.unwrap().into_iter().find(|b| b.id == r.backup_id).unwrap();
    assert_eq!(b.kind, "messages");
    // Both users now see tombstones; the file cannot be downloaded any more; unrelated messages stay.
    for c in [&alice, &bob] {
        assert_eq!(c.history(&ch).await.iter().filter(|m| m.deleted).count(), 3);
    }
    assert!(alice.history(&ch).await.iter().any(|m| m.id == keep.id && !m.deleted));
    assert!(bob.download(&att.id).await.is_err());
    assert!(alice.search("delete").await.unwrap().hits.is_empty());
    assert!(s.admin.viewer_search("q=delete").await.unwrap().is_empty(), "deleted messages are not searchable");

    // Restore the backup: texts and the attachment return.
    let rr = s.admin.restore(&RestoreRequest { backup_id: r.backup_id.clone(), user_ids: vec![], all: false, mode: None }).await.unwrap();
    assert_eq!(rr.messages_added, 3, "{rr:?}");
    assert_eq!(rr.attachments_moved_back, 1, "{rr:?}");
    assert!(alice.texts(&ch).await.contains(&"delete me".to_string()));
    assert!(bob.texts(&ch).await.contains(&"delete me too".to_string()));
    let again = bob.history(&ch).await.into_iter().find(|m| m.id == with_file.id).unwrap();
    assert_eq!(bob.download(&again.attachments[0].id).await.unwrap(), b"evidence bytes");
}

#[tokio::test]
async fn delete_for_one_person_only_and_by_date_range() {
    let Fixture { s, alice, bob, ch } = setup().await;
    alice.send(&ch, "ancient one").await;
    let old2 = bob.send(&ch, "ancient two").await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    let cut = now_ms();
    tokio::time::sleep(Duration::from_millis(30)).await;
    alice.send(&ch, "recent one").await;

    // Scope "user": only alice's copy of the selected message disappears.
    let mut req = delete_req(&[&old2.id], "user");
    req.user_id = Some(alice.user_id.clone());
    let r = s.admin.delete_messages(&req).await.unwrap();
    assert_eq!(r.messages_deleted, 1);
    assert!(!alice.texts(&ch).await.contains(&"ancient two".to_string()));
    assert!(bob.texts(&ch).await.contains(&"ancient two".to_string()));
    // Restoring the message backup brings it back to alice only.
    s.admin.restore(&RestoreRequest { backup_id: r.backup_id, user_ids: vec![alice.user_id.clone()], all: false, mode: None }).await.unwrap();
    assert!(alice.texts(&ch).await.contains(&"ancient two".to_string()));

    // "Delete before date" for everyone in a conversation: older than the cut, nothing newer.
    let mut req = delete_req(&[], "everyone");
    req.conv_id = Some(ch.clone());
    req.before_ms = Some(cut);
    let r = s.admin.delete_messages(&req).await.unwrap();
    assert_eq!(r.messages_deleted, 2, "only the two ancient messages");
    assert_eq!(bob.history(&ch).await.iter().filter(|m| m.deleted).count(), 2);
    assert!(bob.texts(&ch).await.contains(&"recent one".to_string()));
    // Range with nothing in it, and an inverted range, are mistakes.
    let mut none = delete_req(&[], "everyone");
    none.conv_id = Some(ch.clone());
    none.from_ms = Some(1);
    none.to_ms = Some(2);
    assert!(s.admin.delete_messages(&none).await.is_err());
    let mut inverted = none.clone();
    inverted.from_ms = Some(10);
    inverted.to_ms = Some(2);
    assert_eq!(s.admin.delete_messages(&inverted).await.unwrap_err().code, Some(ErrorCode::Validation));
}

#[tokio::test]
async fn delete_range_by_user_supports_before_date_and_other_copies() {
    let Fixture { s, alice, bob, ch } = setup().await;
    alice.send(&ch, "old a").await;
    bob.send(&ch, "old b").await;
    tokio::time::sleep(Duration::from_millis(30)).await;
    let cut = now_ms();
    tokio::time::sleep(Duration::from_millis(30)).await;
    alice.send(&ch, "new a").await;
    // Default: only alice's own copy, only before the date.
    let r = s.admin.delete_range(&alice.user_id, &DateRangeDeleteRequest { from_ms: 0, to_ms: 0, before_ms: Some(cut), conversation_id: Some(ch.clone()), also_other_copies: false }).await.unwrap();
    assert_eq!(r.messages_hidden, 2, "{r:?}");
    assert!(s.admin.backups().await.unwrap().iter().any(|b| b.user_id.as_deref() == Some(alice.user_id.as_str())), "backup first");
    assert_eq!(alice.texts(&ch).await, vec!["new a"]);
    assert_eq!(bob.texts(&ch).await.len(), 3, "the other person's copy stays");
    // Opt in: also delete the other person's copy of what alice wrote.
    let r = s.admin.delete_range(&alice.user_id, &DateRangeDeleteRequest { from_ms: 0, to_ms: i64::MAX / 2, before_ms: None, conversation_id: Some(ch.clone()), also_other_copies: true }).await.unwrap();
    assert!(r.messages_hidden >= 1);
    assert!(!bob.texts(&ch).await.contains(&"new a".to_string()));
}

#[tokio::test]
async fn deleting_is_refused_when_the_backup_cannot_be_written() {
    let Fixture { s, alice, bob, ch } = setup().await;
    let m = alice.send(&ch, "must survive").await;
    let ro = s.dir.path().join("ro");
    std::fs::create_dir_all(&ro).unwrap();
    let b = workchat_server::storage::build_storage(&StorageConfig::Local { path: Some(ro.to_string_lossy().to_string()) }, s.dir.path()).unwrap();
    std::fs::remove_dir_all(&ro).unwrap();
    std::fs::write(&ro, b"x").unwrap();
    *s.st.storage.write().unwrap() = b;
    let err = s.admin.delete_messages(&delete_req(&[&m.id], "everyone")).await.unwrap_err();
    assert_eq!(err.code, Some(ErrorCode::BackupFailed));
    assert!(bob.texts(&ch).await.contains(&"must survive".to_string()));
    assert!(!bob.history(&ch).await.iter().any(|m| m.deleted));
    assert!(s.admin.backups().await.unwrap().is_empty(), "no half-written backup record");
}

#[tokio::test]
async fn admin_can_chat_like_a_normal_user_in_conversations_they_belong_to() {
    let Fixture { s, alice, ch, .. } = setup().await;
    // The admin joins the channel and uses the ordinary user API (send, react, edit own, thread).
    let admin_id = s.admin.user_id_of("admin").await;
    s.admin.add_members(&ch, &[admin_id]).await.unwrap();
    let root = s.admin.send(&ch, "hello from the admin viewer").await;
    assert!(alice.texts(&ch).await.contains(&"hello from the admin viewer".to_string()));
    s.admin.react(&root.id, "👍", true).await.unwrap();
    let edited = s.admin.edit(&root.id, "edited by admin").await.unwrap();
    assert_eq!(edited.version, 2);
    let reply = s.admin.reply(&ch, &root.id, "in a thread").await;
    assert_eq!(reply.thread_root.as_deref(), Some(root.id.as_str()));
    assert!(alice.texts(&ch).await.contains(&"edited by admin".to_string()));
    assert_eq!(alice.thread(&ch, &root.id).await.unwrap().replies.len(), 1);
}
