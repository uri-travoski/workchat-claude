//! 1.2: membership management by owners/admins, Save for later + labels, Activity feed.
mod common;
use common::*;

async fn saved(c: &Client) -> Vec<SavedItem> {
    c.get("/saved").await.unwrap()
}
async fn labels(c: &Client) -> Vec<Label> {
    c.get("/labels").await.unwrap()
}
async fn save(c: &Client, mid: &str, label_ids: Option<Vec<String>>) -> Res<SavedItem> {
    c.put(&format!("/saved/{mid}"), &SaveRequest { label_ids }).await
}
async fn set_labels(c: &Client, mid: &str, ids: &[String]) -> Res<SavedItem> {
    c.put(&format!("/saved/{mid}/labels"), &SavedLabelsRequest { label_ids: ids.to_vec() }).await
}
async fn new_label(c: &Client, name: &str, color: &str) -> Res<Vec<Label>> {
    c.post("/labels", &LabelRequest { name: Some(name.into()), color: Some(color.into()), position: None }).await
}
async fn members(c: &Client, conv: &str, ids: &[String]) -> Res<Conversation> {
    c.post(&format!("/conversations/{conv}/members"), &MembersRequest { user_ids: ids.to_vec() }).await
}
async fn unmember(c: &Client, conv: &str, id: &str) -> Res<Conversation> {
    c.delete(&format!("/conversations/{conv}/members/{id}")).await
}

#[tokio::test(flavor = "multi_thread")]
async fn admins_add_and_remove_members_and_members_cannot() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let dave = s.user("dave").await;
    let ch = s.channel("ops", &[&alice, &bob]).await;
    s.admin.set_role(&bob.user_id, Role::Admin).await.unwrap();
    // A plain member is refused; an outsider cannot even tell the channel exists.
    assert_eq!(members(&alice, &ch, &[carol.user_id.clone()]).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    assert_eq!(members(&carol, &ch, &[carol.user_id.clone()]).await.unwrap_err().code, Some(ErrorCode::NotFound));
    assert_eq!(unmember(&alice, &ch, &bob.user_id).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    // An admin who is a member adds two people at once.
    let mut wc = carol.ws(carol.cursor().await).await;
    members(&bob, &ch, &[carol.user_id.clone(), dave.user_id.clone()]).await.unwrap();
    wc.wait_event("carol gets the channel", |e| matches!(e, EventData::ConversationUpsert { conversation } if conversation.conversation.id == ch)).await;
    assert!(carol.has_conversation(&ch).await && dave.has_conversation(&ch).await);
    assert_eq!(alice.conversation(&ch).await.unwrap().conversation.members.len(), 4);
    // The channel shows a system message and it is not unread noise.
    let sys = alice.history(&ch).await.into_iter().find(|m| m.kind == MessageKind::System).unwrap();
    assert_eq!(sys.body, "Bob added Carol and Dave");
    assert_eq!(alice.conversation(&ch).await.unwrap().unread, 0);
    // Adding the same person again changes nothing (no second message).
    let before = alice.history(&ch).await.len();
    members(&bob, &ch, &[carol.user_id.clone()]).await.unwrap();
    assert_eq!(alice.history(&ch).await.len(), before);
    // Remove one: they lose the channel, the others see who removed them.
    unmember(&bob, &ch, &dave.user_id).await.unwrap();
    assert!(!dave.has_conversation(&ch).await);
    assert!(alice.history(&ch).await.iter().any(|m| m.kind == MessageKind::System && m.body == "Bob removed Dave"));
    assert_eq!(dave.history_raw(&ch, "").await.unwrap_err().code, Some(ErrorCode::NotFound));
    // DMs and notes cannot be managed.
    let dm = bob.open_dm(&alice.user_id).await.unwrap();
    assert_eq!(members(&bob, &dm, &[carol.user_id.clone()]).await.unwrap_err().code, Some(ErrorCode::Validation));
}

#[tokio::test(flavor = "multi_thread")]
async fn saved_items_and_labels_are_private_synced_and_survive_deletion() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let ch = s.channel("design", &[&alice, &bob]).await;
    let m1 = alice.send(&ch, "first").await;
    let m2 = alice.send(&ch, "second").await;
    // Alice saves two messages and labels one with two labels.
    let urgent = new_label(&alice, "Urgent", "#d93025").await.unwrap();
    assert_eq!(urgent.len(), 1);
    let todo = new_label(&alice, "todo", "#0467dd").await.unwrap();
    let (l_urgent, l_todo) = (urgent[0].id.clone(), todo.iter().find(|l| l.name == "todo").unwrap().id.clone());
    save(&alice, &m1.id, None).await.unwrap();
    let it = save(&alice, &m2.id, Some(vec![l_urgent.clone(), l_todo.clone()])).await.unwrap();
    assert_eq!(it.label_ids.len(), 2);
    assert!(saved(&alice).await.iter().any(|i| i.message_id == m1.id));
    // A second device of Alice sees it; Bob never sees anything.
    let alice2 = s.login("alice").await;
    assert_eq!(saved(&alice2).await.len(), 2);
    assert_eq!(labels(&alice2).await.len(), 2);
    assert!(saved(&bob).await.is_empty() && labels(&bob).await.is_empty());
    // Bob cannot label or unsave Alice's item; his own saves are his own.
    assert_eq!(set_labels(&bob, &m2.id, &[l_urgent.clone()]).await.unwrap_err().code, Some(ErrorCode::NotFound));
    save(&bob, &m1.id, None).await.unwrap();
    assert_eq!(saved(&bob).await.len(), 1);
    assert!(labels(&bob).await.is_empty());
    assert_eq!(save(&bob, &m1.id, Some(vec![l_urgent.clone()])).await.unwrap_err().code, Some(ErrorCode::NotFound), "someone else's label id is unknown to Bob");
    // Rename, recolour, remove a label from an item through a label delete.
    let _: Vec<Label> = alice.patch(&format!("/labels/{l_todo}"), &LabelRequest { name: Some("To do".into()), color: Some("#7b61ff".into()), position: None }).await.unwrap();
    assert!(labels(&alice).await.iter().any(|l| l.id == l_todo && l.name == "To do" && l.color == "#7b61ff"));
    assert_eq!(new_label(&alice, "urgent", "#000000").await.unwrap_err().code, Some(ErrorCode::Conflict), "names are unique per user, ignoring case");
    assert_eq!(new_label(&alice, "x", "red").await.unwrap_err().code, Some(ErrorCode::Validation));
    set_labels(&alice, &m1.id, &[l_todo.clone()]).await.unwrap();
    let _: Vec<Label> = alice.delete(&format!("/labels/{l_todo}")).await.unwrap();
    assert!(saved(&alice2).await.iter().all(|i| !i.label_ids.contains(&l_todo)));
    assert_eq!(labels(&alice2).await.len(), 1);
    // Deleting the message leaves a tombstone: the saved item stays and the message reads "deleted".
    alice.delete_message(&m2.id).await.unwrap();
    assert!(saved(&alice).await.iter().any(|i| i.message_id == m2.id));
    let tomb: Message = alice.get(&format!("/messages/{}", m2.id)).await.unwrap();
    assert!(tomb.deleted);
    // Unsave.
    alice.delete::<Value>(&format!("/saved/{}", m1.id)).await.unwrap();
    assert_eq!(saved(&alice2).await.len(), 1);
    // Losing access to the conversation hides the item (and it does not leak into the list).
    save(&bob, &m1.id, None).await.ok();
    s.admin.remove_member(&ch, &bob.user_id).await.unwrap();
    assert!(saved(&bob).await.is_empty());
    assert!(save(&bob, &m1.id, None).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn saved_items_and_labels_are_in_the_per_user_backup_and_come_back() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let ch = s.channel("notes", &[&alice]).await;
    let m = alice.send(&ch, "remember this").await;
    let l = new_label(&alice, "Keep", "#0dbd8b").await.unwrap();
    save(&alice, &m.id, Some(vec![l[0].id.clone()])).await.unwrap();
    let bk = s.admin.backup_user(&alice.user_id).await.unwrap();
    // Replace mode wipes the live state first; the backup restores it.
    alice.delete::<Value>(&format!("/saved/{}", m.id)).await.unwrap();
    let _: Vec<Label> = alice.delete(&format!("/labels/{}", l[0].id)).await.unwrap();
    assert!(saved(&alice).await.is_empty() && labels(&alice).await.is_empty());
    s.admin.restore(&RestoreRequest { backup_id: bk.id.clone(), user_ids: vec![alice.user_id.clone()], all: false, mode: Some("replace".into()) }).await.unwrap();
    let items = saved(&alice).await;
    let ls = labels(&alice).await;
    assert!(items.len() == 1 && ls.len() == 1);
    assert_eq!(items[0].label_ids, vec![ls[0].id.clone()]);
    // A deleted account restores with its saved items.
    s.admin.delete_user(&alice.user_id).await.unwrap();
    let bk2 = s.admin.backups().await.unwrap().into_iter().filter(|b| b.user_id.as_deref() == Some(alice.user_id.as_str())).max_by_key(|b| b.created_ms).expect("deletion backup");
    s.admin.restore(&RestoreRequest { backup_id: bk2.id, user_ids: vec![alice.user_id.clone()], all: false, mode: None }).await.unwrap();
    s.admin.reset_password(&alice.user_id, Some(PW)).await.ok();
    let back = s.login("alice").await;
    assert!(saved(&back).await.len() == 1 && labels(&back).await.len() == 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn activity_feed_lists_mentions_reactions_and_thread_replies_with_read_state() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let ch = s.channel("team", &[&alice, &bob, &carol]).await;
    let feed = |c: &Client| {
        let c = c.clone();
        async move { c.get::<ActivityFeed>("/activity?limit=150").await.unwrap() }
    };
    let mine = alice.send(&ch, "my announcement").await;
    // Bob mentions Alice, reacts to her message and replies in a thread under it; Carol is not involved.
    // Structured mentions come from the sender's app (the server expands only @channel, @here and group handles).
    bob.send_req(&ch, &SendMessage { client_id: uid(), body: "hello @alice please look".into(), thread_root: None, reply_to: None, mentions: vec![alice.user_id.clone()], attachment_ids: vec![], also_send_to_channel: false }).await.unwrap();
    bob.react(&mine.id, "\u{1F44D}", true).await.unwrap();
    bob.reply(&ch, &mine.id, "a reply under your post").await;

    let f = feed(&alice).await;
    assert_eq!(f.items.len(), 3, "{:?}", f.items);
    let kinds: Vec<String> = f.items.iter().map(|i| i.kind.clone()).collect();
    for k in ["mention", "reaction", "thread_reply"] {
        assert!(kinds.contains(&k.to_string()), "{k} missing in {kinds:?}");
    }
    assert_eq!(f.unread, 3);
    assert!(feed(&carol).await.items.is_empty());
    assert!(feed(&bob).await.items.is_empty(), "your own actions are never activity");
    // Mark all read: the marker syncs, new items after it are unread again.
    let mut ws = alice.ws(alice.cursor().await).await;
    let after: ActivityFeed = alice.post_empty("/activity/read").await.unwrap();
    assert_eq!(after.unread, 0);
    ws.wait_event("activity read", |e| matches!(e, EventData::ActivityRead { .. })).await;
    assert_eq!(feed(&alice).await.unread, 0);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    bob.react(&mine.id, "\u{2764}\u{FE0F}", true).await.unwrap();
    assert_eq!(feed(&alice).await.unread, 1);
}

/// `GET /saved/messages`: the saved item with its full message (attachments included) and its conversation.
#[tokio::test(flavor = "multi_thread")]
async fn saved_messages_endpoint_returns_message_attachments_and_conversation() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let ch = s.channel("design", &[&alice, &bob]).await;
    let plain = alice.send(&ch, "plain").await;
    let withfile = alice.send_file(&ch, "with file", "note.txt", b"hello".to_vec()).await;
    save(&bob, &plain.id, None).await.unwrap();
    save(&bob, &withfile.id, None).await.unwrap();
    let list: Vec<SavedMessage> = bob.get("/saved/messages").await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].item.message_id, withfile.id, "newest saved first");
    assert_eq!(list[0].message.body, "with file");
    assert_eq!(list[0].message.attachments.len(), 1);
    assert_eq!(list[0].message.attachments[0].name, "note.txt");
    assert_eq!(list[0].conversation.conversation.id, ch);
    assert_eq!(list[0].conversation.conversation.name.as_deref(), Some("design"));
    assert!(alice.get::<Vec<SavedMessage>>("/saved/messages").await.unwrap().is_empty());
}
