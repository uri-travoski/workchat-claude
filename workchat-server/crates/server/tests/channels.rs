//! Phase S2: channel creation by members, private channels, pinning rules, "Delete for me", "Also send to
//! channel", the single-conversation endpoint and the sessions list.
mod common;
use common::*;

fn ids(v: &[&Client]) -> Vec<String> {
    v.iter().map(|c| c.user_id.clone()).collect()
}

async fn counts(c: &Client) -> Counts {
    c.get("/me/counts").await.unwrap()
}

fn role_of(conv: &Conversation, user: &str) -> String {
    conv.members.iter().find(|m| m.user_id == user).map(|m| m.role.clone()).unwrap_or_default()
}

// ------------------------------------------------------------------ 1. who can create channels, managers

#[tokio::test]
async fn org_settings_default_and_events() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let mut ws = alice.ws(alice.cursor().await).await;
    let st: OrgSettings = alice.get("/org-settings").await.unwrap();
    assert_eq!((st.channel_creation, st.channel_pins), (ChannelCreation::Everyone, ChannelPins::Everyone));
    assert_eq!(alice.bootstrap().await.unwrap().org_settings, st);
    // Members cannot change it; admins can, partial updates keep the other field.
    let e = alice.put::<OrgSettings, _>("/admin/org-settings", &UpdateOrgSettings { channel_creation: Some(ChannelCreation::Admins), channel_pins: None }).await.unwrap_err();
    assert_eq!(e.code, Some(ErrorCode::Forbidden));
    let n = s.admin.set_org_settings(&UpdateOrgSettings { channel_creation: Some(ChannelCreation::Admins), channel_pins: None }).await.unwrap();
    assert_eq!((n.channel_creation, n.channel_pins), (ChannelCreation::Admins, ChannelPins::Everyone));
    let n = s.admin.set_org_settings(&UpdateOrgSettings { channel_creation: None, channel_pins: Some(ChannelPins::Managers) }).await.unwrap();
    assert_eq!((n.channel_creation, n.channel_pins), (ChannelCreation::Admins, ChannelPins::Managers));
    ws.wait_event("org settings event", |e| matches!(e, EventData::OrgSettingsUpdate { settings } if settings.channel_pins == ChannelPins::Managers)).await;
    assert_eq!(alice.bootstrap().await.unwrap().org_settings, n);
    // An invalid value is refused.
    let r = s.admin.raw(reqwest::Method::PUT, "/admin/org-settings", Some(json!({"channel_creation": "nobody"}))).await;
    assert!(r.status >= 400, "unknown values are refused");
}

#[tokio::test]
async fn members_create_channels_and_group_dms_and_become_managers() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;

    // Default: everyone can create. The creator is a member and the manager.
    let c = alice.new_channel("project", &ids(&[&bob])).await.unwrap();
    let id = c.conversation.id.clone();
    assert_eq!(c.conversation.kind, ConversationKind::Channel);
    assert_eq!(role_of(&c.conversation, &alice.user_id), "owner");
    assert_eq!(role_of(&c.conversation, &bob.user_id), "member");
    assert!(alice.has_conversation(&id).await && bob.has_conversation(&id).await && !carol.has_conversation(&id).await);

    // The manager renames, sets the topic, adds, removes and archives.
    let u: ConversationSummary = alice.patch(&format!("/conversations/{id}"), &UpdateConversation { name: Some("project-x".into()), topic: Some("ship it".into()), ..Default::default() }).await.unwrap();
    assert_eq!((u.conversation.name.as_deref(), u.conversation.topic.as_deref()), (Some("project-x"), Some("ship it")));
    let add = |c: &Client, who: &str| { let (c, p, w) = (c.clone(), format!("/conversations/{id}/members"), who.to_string()); async move { c.post::<Conversation, _>(&p, &MembersRequest { user_ids: vec![w] }).await } };
    assert_eq!(add(&alice, &carol.user_id).await.unwrap().members.len(), 3);
    assert_eq!(alice.delete::<Conversation>(&format!("/conversations/{id}/members/{}", carol.user_id)).await.unwrap().members.len(), 2);
    let avatar = alice.upload_as("a.png", "image/png", Some("avatar"), vec![1, 2, 3]).await.unwrap();
    let a: ConversationSummary = alice.put(&format!("/conversations/{id}/avatar"), &SetConvAvatar { avatar: avatar.id.clone() }).await.unwrap();
    assert_eq!(a.conversation.avatar.as_deref(), Some(avatar.id.as_str()));

    // A plain member (bob) may not; an outsider (carol) does not even learn it exists.
    let e = bob.patch::<ConversationSummary, _>(&format!("/conversations/{id}"), &UpdateConversation { name: Some("mine".into()), ..Default::default() }).await.unwrap_err();
    assert_eq!(e.code, Some(ErrorCode::Forbidden));
    assert_eq!(add(&bob, &carol.user_id).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    let e = carol.patch::<ConversationSummary, _>(&format!("/conversations/{id}"), &UpdateConversation { archived: Some(true), ..Default::default() }).await.unwrap_err();
    assert_eq!(e.code, Some(ErrorCode::NotFound));

    let arch: ConversationSummary = alice.patch(&format!("/conversations/{id}"), &UpdateConversation { archived: Some(true), ..Default::default() }).await.unwrap();
    assert!(arch.conversation.archived);
    let back: ConversationSummary = alice.patch(&format!("/conversations/{id}"), &UpdateConversation { archived: Some(false), ..Default::default() }).await.unwrap();
    assert!(!back.conversation.archived);
    // Admins still manage everything.
    s.admin.patch::<ConversationSummary, _>(&format!("/conversations/{id}"), &UpdateConversation { topic: Some("admin edit".into()), ..Default::default() }).await.unwrap();

    // Restricted to admins: members get forbidden, admins can still create, group DMs stay open.
    s.admin.set_org_settings(&UpdateOrgSettings { channel_creation: Some(ChannelCreation::Admins), channel_pins: None }).await.unwrap();
    assert_eq!(alice.new_channel("nope", &[]).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    let by_admin = s.admin.new_channel("official", &ids(&[&alice])).await.unwrap();
    assert_eq!(role_of(&by_admin.conversation, &s.admin.user_id), "owner", "an admin creating through the app joins too");
    let g = alice.new_group("lunch", &ids(&[&bob, &carol])).await.unwrap();
    assert_eq!(g.conversation.kind, ConversationKind::GroupDm);
    assert_eq!(g.conversation.members.len(), 3);
    assert_eq!(role_of(&g.conversation, &alice.user_id), "owner");
    assert!(bob.has_conversation(&g.conversation.id).await && carol.has_conversation(&g.conversation.id).await);
    // The group manager renames it; others cannot.
    alice.patch::<ConversationSummary, _>(&format!("/conversations/{}", g.conversation.id), &UpdateConversation { name: Some("lunch club".into()), ..Default::default() }).await.unwrap();
    assert!(bob.patch::<ConversationSummary, _>(&format!("/conversations/{}", g.conversation.id), &UpdateConversation { name: Some("x".into()), ..Default::default() }).await.is_err());
    // A group DM needs at least one other person; DMs and notes cannot be created through this endpoint.
    assert_eq!(alice.new_group("solo", &[]).await.unwrap_err().code, Some(ErrorCode::Validation));
    let r = alice.post::<ConversationSummary, _>("/conversations", &CreateConversation { kind: ConversationKind::Dm, name: "x".into(), topic: None, members: vec![], include_creator: None }).await;
    assert!(r.is_err());
    // Members cannot add bots; disabled people are refused.
    s.admin.set_disabled(&carol.user_id, true).await.unwrap();
    assert!(alice.new_group("gone", &ids(&[&bob, &carol])).await.is_err());
}

#[tokio::test]
async fn single_conversation_endpoint() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let c = alice.new_channel("solo-view", &ids(&[&bob])).await.unwrap();
    let id = c.conversation.id.clone();
    bob.mark_read(&id).await.unwrap();
    alice.send(&id, "one").await;
    alice.send(&id, "two").await;
    let from_list = bob.conversations().await.unwrap().into_iter().find(|x| x.conversation.id == id).unwrap();
    let single: ConversationSummary = bob.get(&format!("/conversations/{id}")).await.unwrap();
    assert_eq!(single, from_list);
    assert_eq!(single.unread, 2);
    assert_eq!(carol.conversation(&id).await.unwrap_err().code, Some(ErrorCode::NotFound));
}

// ------------------------------------------------------------------ 2. private-only channels

#[tokio::test]
async fn channels_are_private_leave_works_and_public_endpoints_are_gone() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let mut wb = bob.ws(bob.cursor().await).await;

    let design = alice.new_channel("design", &ids(&[&bob])).await.unwrap().conversation;
    let ops = alice.new_channel("ops", &ids(&[&carol])).await.unwrap().conversation;
    let ops_id = ops.id.clone();

    // Nobody outside a channel can see it or its history, and no directory event ever reaches them.
    assert_eq!(bob.conversation(&ops.id).await.unwrap_err().code, Some(ErrorCode::NotFound));
    assert_eq!(bob.history_raw(&ops.id, "").await.unwrap_err().code, Some(ErrorCode::NotFound));
    let evs = bob.events(0).await.unwrap().events;
    assert!(!evs.iter().any(|e| serde_json::to_string(&e.event).unwrap().contains("channel_directory")));

    // Browse and join no longer exist.
    let r: Result<Value, _> = bob.get("/channels/browse").await;
    assert_eq!(r.unwrap_err().status, 404);
    let r: Result<Value, _> = bob.get("/channels/browse?q=ops").await;
    assert_eq!(r.unwrap_err().status, 404);
    for id in [&ops.id, &design.id] {
        assert_eq!(bob.post_empty::<Value>(&format!("/conversations/{id}/join")).await.unwrap_err().status, 404);
    }
    assert!(!bob.has_conversation(&ops.id).await);

    // Old callers still sending `visibility` are not broken: it is ignored, the channel is members-only anyway.
    let v: Value = alice.post("/conversations", &serde_json::json!({"kind": "channel", "name": "legacy-caller", "members": [], "visibility": "public"})).await.unwrap();
    assert!(v.get("visibility").is_none(), "no visibility field in responses any more");
    let legacy_id = v["id"].as_str().unwrap().to_string();
    assert_eq!(bob.conversation(&legacy_id).await.unwrap_err().code, Some(ErrorCode::NotFound));
    let p: Value = alice.patch(&format!("/conversations/{legacy_id}"), &serde_json::json!({"topic": "t", "visibility": "public"})).await.unwrap();
    assert_eq!(p["topic"], "t");
    assert_eq!(bob.conversation(&legacy_id).await.unwrap_err().code, Some(ErrorCode::NotFound));
    let r = alice.post::<Value, _>("/conversations", &serde_json::json!({"kind": "group_dm", "name": "g", "members": [bob.user_id], "visibility": "public"})).await;
    assert!(r.is_ok(), "an unknown/obsolete field rejects nothing");

    // Being added is the only way in; the others see the system message.
    let carol_added: Conversation = alice.post(&format!("/conversations/{}/members", design.id), &MembersRequest { user_ids: vec![carol.user_id.clone()] }).await.unwrap();
    assert_eq!(carol_added.members.len(), 3);

    // Leave: removed for the leaver (event), the others see "Bob left"; he cannot come back by himself.
    let r: Value = bob.post_empty(&format!("/conversations/{}/leave", design.id)).await.unwrap();
    assert_eq!(r["ok"], true);
    wb.wait_event("leaver removed", |e| matches!(e, EventData::ConversationRemove { conv_id } if conv_id == &design.id)).await;
    assert!(!bob.has_conversation(&design.id).await);
    assert_eq!(alice.texts(&design.id).await.last().map(|t| t.as_str()), Some("Bob left"));
    assert_eq!(bob.post_empty::<Value>(&format!("/conversations/{}/leave", design.id)).await.unwrap_err().code, Some(ErrorCode::NotFound));
    assert_eq!(bob.post_empty::<Value>(&format!("/conversations/{}/join", design.id)).await.unwrap_err().status, 404);
    // A group DM can be left too; DMs cannot.
    let g = alice.new_group("trio", &ids(&[&bob, &carol])).await.unwrap().conversation.id;
    bob.post_empty::<Value>(&format!("/conversations/{g}/leave")).await.unwrap();
    let dm = bob.open_dm(&carol.user_id).await.unwrap();
    assert_eq!(bob.post_empty::<Value>(&format!("/conversations/{dm}/leave")).await.unwrap_err().code, Some(ErrorCode::Validation));
    // Deleting a channel works as before.
    let _: Value = s.admin.delete(&format!("/admin/conversations/{ops_id}")).await.unwrap();
    assert!(!carol.has_conversation(&ops_id).await);
}

// ------------------------------------------------------------------ 3. pinning

#[tokio::test]
async fn pinning_rules() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let pin = |c: &Client, conv: &str, mid: &str| {
        let (c, conv, mid) = (c.clone(), conv.to_string(), mid.to_string());
        async move { c.put_empty::<Vec<Pin>>(&format!("/conversations/{conv}/pins/{mid}")).await }
    };
    let unpin = |c: &Client, conv: &str, mid: &str| {
        let (c, conv, mid) = (c.clone(), conv.to_string(), mid.to_string());
        async move { c.delete::<Vec<Pin>>(&format!("/conversations/{conv}/pins/{mid}")).await }
    };

    // DMs: both people can pin and unpin.
    let dm = alice.open_dm(&bob.user_id).await.unwrap();
    let m = alice.send(&dm, "in a dm").await;
    assert_eq!(pin(&bob, &dm, &m.id).await.unwrap().len(), 1);
    assert_eq!(pin(&alice, &dm, &m.id).await.unwrap().len(), 1);
    assert!(unpin(&bob, &dm, &m.id).await.unwrap().is_empty());
    // Group DMs: any member; outsiders get not_found.
    let g = alice.new_group("trio", &ids(&[&bob, &carol])).await.unwrap().conversation.id;
    let m = alice.send(&g, "group message").await;
    assert_eq!(pin(&carol, &g, &m.id).await.unwrap().len(), 1);
    let dave = s.user("dave").await;
    assert_eq!(pin(&dave, &g, &m.id).await.unwrap_err().code, Some(ErrorCode::NotFound));
    assert_eq!(pin(&dave, &dm, &m.id).await.unwrap_err().code, Some(ErrorCode::NotFound));

    // Channels: members pin by default, with the PinsUpdate event to everyone.
    let ch = alice.new_channel("pinboard", &ids(&[&bob, &carol])).await.unwrap().conversation.id;
    let m = alice.send(&ch, "pin me").await;
    let mut wa = alice.ws(alice.cursor().await).await;
    assert_eq!(pin(&bob, &ch, &m.id).await.unwrap().len(), 1);
    wa.wait_event("pins update", |e| matches!(e, EventData::PinsUpdate { conv_id, pins } if conv_id == &ch && pins.len() == 1 && pins[0].pinned_by == bob.user_id)).await;
    assert!(unpin(&carol, &ch, &m.id).await.unwrap().is_empty());
    // Non-members still get not_found, also on public channels.
    assert_eq!(pin(&dave, &ch, &m.id).await.unwrap_err().code, Some(ErrorCode::NotFound));

    // Restricted to managers: plain members get forbidden, the channel manager and admins can; DMs stay open.
    s.admin.set_org_settings(&UpdateOrgSettings { channel_creation: None, channel_pins: Some(ChannelPins::Managers) }).await.unwrap();
    assert_eq!(pin(&bob, &ch, &m.id).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    assert_eq!(pin(&alice, &ch, &m.id).await.unwrap().len(), 1);
    assert_eq!(unpin(&bob, &ch, &m.id).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    assert!(unpin(&s.admin, &ch, &m.id).await.unwrap().is_empty());
    let dmm = bob.send(&dm, "still open").await;
    assert_eq!(pin(&alice, &dm, &dmm.id).await.unwrap().len(), 1);
    // A manager made by the manager list (conversation role) is not needed: the creator is the manager; others stay members.
    assert_eq!(pin(&carol, &g, &m.id).await.unwrap_err().code, Some(ErrorCode::NotFound), "message of another conversation");
}

// ------------------------------------------------------------------ 4. delete for me

#[tokio::test]
async fn delete_for_me_hides_everywhere_for_that_user_only() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let bob2 = s.login("bob").await; // second device
    let carol = s.user("carol").await;
    let ch = alice.new_channel("hide-me", &ids(&[&bob, &carol])).await.unwrap().conversation.id;
    let keep = alice.send(&ch, "keep this").await;
    let gone = alice.send_file(&ch, "zebra report", "zebra.pdf", b"%PDF-1.4 zebra".to_vec()).await;
    let own = bob.send(&ch, "my own words").await;
    let root = alice.send(&ch, "thread root").await;
    let r1 = bob.reply(&ch, &root.id, "first reply").await;
    let r2 = alice.reply(&ch, &root.id, "second reply").await;
    bob.mark_read(&ch).await.unwrap();
    // Unread bits for bob.
    let unread_msg = alice.send(&ch, "unread one").await;
    assert_eq!(bob.conversation(&ch).await.unwrap().unread, 1);
    let before = counts(&bob).await;
    assert_eq!(before.files, 1);
    assert!(bob.search("zebra").await.unwrap().hits.iter().any(|h| h.message.id == gone.id));
    assert_eq!(bob.get::<Vec<FileHit>>("/files").await.unwrap().len(), 1);

    let cur = bob2.cursor().await;
    let mut w2 = bob2.ws(cur).await;
    let cur_b = bob.cursor().await;

    // Any message can be hidden, other people's too.
    let r: HideMessageResponse = bob.post_empty(&format!("/messages/{}/hide", gone.id)).await.unwrap();
    assert_eq!((r.message_id.as_str(), r.conv_id.as_str()), (gone.id.as_str(), ch.as_str()));
    // The user's other device is told with message_hide, then the new counts.
    w2.wait_event("hide on the other device", |e| matches!(e, EventData::MessageHide { conv_id, message_ids } if conv_id == &ch && message_ids == &vec![gone.id.clone()])).await;
    // Idempotent: hiding again changes nothing and sends no second event.
    bob.post_empty::<HideMessageResponse>(&format!("/messages/{}/hide", gone.id)).await.unwrap();
    let hides = bob.events(cur_b).await.unwrap().events.iter().filter(|e| matches!(&e.event, EventData::MessageHide { .. })).count();
    assert_eq!(hides, 1);

    // History, search, files and counts exclude it for bob, on both devices.
    for c in [&bob, &bob2] {
        assert!(!c.history(&ch).await.iter().any(|m| m.id == gone.id));
        assert!(c.history(&ch).await.iter().any(|m| m.id == keep.id));
        assert!(c.search("zebra").await.unwrap().hits.is_empty());
        assert!(c.get::<Vec<FileHit>>("/files").await.unwrap().is_empty());
        assert_eq!(counts(c).await.files, 0);
    }
    assert!(bob.get::<Value>(&format!("/messages/{}", gone.id)).await.is_err());
    // Everyone else is unaffected.
    for c in [&alice, &carol] {
        assert!(c.history(&ch).await.iter().any(|m| m.id == gone.id));
        assert_eq!(c.search("zebra").await.unwrap().hits.len(), 1);
        assert_eq!(counts(c).await.files, 1);
    }
    // The bootstrap of a new device leaves it out as well.
    let boot = bob.bootstrap().await.unwrap();
    assert!(!boot.messages[&ch].iter().any(|m| m.id == gone.id));

    // Hiding an unread message lowers the unread count (and bob's devices hear read_update).
    bob.post_empty::<HideMessageResponse>(&format!("/messages/{}/hide", unread_msg.id)).await.unwrap();
    w2.wait_event("unread dropped", |e| matches!(e, EventData::ReadUpdate { conv_id, unread: 0, .. } if conv_id == &ch)).await;
    assert_eq!(bob.conversation(&ch).await.unwrap().unread, 0);
    assert_eq!(alice.conversation(&ch).await.unwrap().unread, 0);

    // Your own messages can be hidden for you too (the others still see them).
    bob.post_empty::<HideMessageResponse>(&format!("/messages/{}/hide", own.id)).await.unwrap();
    assert!(!bob.history(&ch).await.iter().any(|m| m.id == own.id));
    assert!(alice.history(&ch).await.iter().any(|m| m.id == own.id));

    // Threads: a hidden reply leaves the thread view and the counts; the thread stays.
    let t: ThreadView = bob.thread(&ch, &root.id).await.unwrap();
    assert_eq!(t.replies.len(), 2);
    bob.post_empty::<HideMessageResponse>(&format!("/messages/{}/hide", r2.id)).await.unwrap();
    w2.wait_event("thread updated", |e| matches!(e, EventData::ThreadUpdate { thread } if thread.root.id == root.id && thread.reply_count == 1)).await;
    let t = bob.thread(&ch, &root.id).await.unwrap();
    assert_eq!((t.replies.len(), t.summary.reply_count), (1, 1));
    assert_eq!(alice.thread(&ch, &root.id).await.unwrap().replies.len(), 2);
    let list: Vec<ThreadSummary> = bob.get("/threads").await.unwrap();
    assert_eq!(list.len(), 1);
    // A hidden thread root removes the thread from the list (thread_update following=false) and the count.
    assert_eq!(counts(&bob).await.threads, 1);
    bob.post_empty::<HideMessageResponse>(&format!("/messages/{}/hide", root.id)).await.unwrap();
    w2.wait_event("thread removed", |e| matches!(e, EventData::ThreadUpdate { thread } if thread.root.id == root.id && !thread.following)).await;
    assert!(bob.get::<Vec<ThreadSummary>>("/threads").await.unwrap().is_empty());
    assert_eq!(counts(&bob).await.threads, 0);
    assert_eq!(alice.get::<Vec<ThreadSummary>>("/threads").await.unwrap().len(), 1);
    let _ = r1;

    // Not a member: not_found. Unknown message: not_found.
    let dave = s.user("dave").await;
    assert_eq!(dave.post_empty::<Value>(&format!("/messages/{}/hide", keep.id)).await.unwrap_err().code, Some(ErrorCode::NotFound));
    assert_eq!(bob.post_empty::<Value>("/messages/nope/hide").await.unwrap_err().code, Some(ErrorCode::NotFound));
    // The mark survives a restart of the session (it is stored on the server).
    let fresh = s.login("bob").await;
    assert!(!fresh.history(&ch).await.iter().any(|m| m.id == gone.id));
}

// ------------------------------------------------------------------ 5. also send to channel

#[tokio::test]
async fn thread_reply_also_sent_to_channel_is_one_message() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let ch = alice.new_channel("broadcast", &ids(&[&bob])).await.unwrap().conversation.id;
    let root = alice.send(&ch, "root").await;
    let mut wb = bob.ws(bob.cursor().await).await;
    let req = SendMessage { client_id: "bc-1".into(), body: "this matters to everyone".into(), thread_root: Some(root.id.clone()), reply_to: None, mentions: vec![], attachment_ids: vec![], also_send_to_channel: true };
    let m = bob.send_req(&ch, &req).await.unwrap();
    assert!(m.also_send_to_channel);
    assert_eq!(m.thread_root.as_deref(), Some(root.id.as_str()));
    let ev = wb.wait_event("new message", |e| matches!(e, EventData::MessageNew { message } if message.id == m.id)).await;
    assert!(matches!(ev.event, EventData::MessageNew { message } if message.also_send_to_channel));
    // One row: in the conversation history (the main timeline, filtered by clients on `thread_root` unless the flag is set)
    // and in the thread, with the same id; not duplicated.
    let h = alice.history(&ch).await;
    assert_eq!(h.iter().filter(|x| x.body == "this matters to everyone").count(), 1);
    let hm = h.iter().find(|x| x.id == m.id).unwrap();
    assert!(hm.also_send_to_channel);
    assert_eq!(hm.thread_root.as_deref(), Some(root.id.as_str()));
    let t = alice.thread(&ch, &root.id).await.unwrap();
    assert_eq!(t.replies.len(), 1);
    assert_eq!(t.replies[0].id, m.id);
    assert!(t.replies[0].also_send_to_channel);
    assert_eq!(t.summary.reply_count, 1);
    // Idempotent retry returns the same message, without a copy.
    let again = bob.send_req(&ch, &req).await.unwrap();
    assert_eq!(again.id, m.id);
    assert_eq!(alice.history(&ch).await.len(), 2, "root and the one broadcast reply");
    // A normal reply does not carry it; the flag needs a thread; search and the JSON shape are unchanged.
    let plain = alice.reply(&ch, &root.id, "just a reply").await;
    assert!(!plain.also_send_to_channel);
    let v = serde_json::to_value(&plain).unwrap();
    assert!(v.get("also_send_to_channel").is_none(), "false is left out");
    let bad = SendMessage { client_id: "bc-2".into(), body: "x".into(), thread_root: None, reply_to: None, mentions: vec![], attachment_ids: vec![], also_send_to_channel: true };
    assert_eq!(alice.send_req(&ch, &bad).await.unwrap_err().code, Some(ErrorCode::Validation));
    assert_eq!(alice.search("matters").await.unwrap().hits.len(), 1);
    // Deleting it deletes the single message everywhere.
    bob.delete_message(&m.id).await.unwrap();
    assert!(alice.history(&ch).await.iter().find(|x| x.id == m.id).unwrap().deleted);
}

// ------------------------------------------------------------------ 6. schema, 7. sessions

#[test]
fn schema_file_is_current_and_has_the_new_types() {
    let generated = serde_json::to_value(proto::api_schema()).unwrap();
    let defs = generated["definitions"].as_object().unwrap();
    for gone in ["ChannelBrowsePage", "ChannelBrowseItem", "ChannelVisibility"] {
        assert!(!defs.contains_key(gone), "{gone} must be gone from the schema");
    }
    for t in ["Counts", "OrgSettings", "UpdateOrgSettings", "HideMessageResponse", "SessionInfo", "ConversationSummary"] {
        assert!(defs.contains_key(t), "{t} is missing from the generated schema");
    }
    let file = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schema/api.schema.json")).unwrap();
    let on_disk: Value = serde_json::from_str(&file).unwrap();
    assert_eq!(on_disk["definitions"], generated["definitions"], "run: cargo run -p workchat-proto --bin gen-schema");
}

#[tokio::test]
async fn sessions_list_and_revoke() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    s.allow_signout(&alice.user_id).await;
    let phone = Client::login_device(&s.url, "alice", PW, "Pixel 8").await.unwrap();
    let tablet = Client::login_device(&s.url, "alice", PW, "Tablet").await.unwrap();
    let list = phone.sessions().await.unwrap();
    assert_eq!(list.len(), 3);
    let me = list.iter().find(|x| x.current).unwrap();
    assert_eq!((me.id.as_str(), me.device_name.as_str()), (phone.session_id.as_str(), "Pixel 8"));
    assert!(me.last_seen_ms > 0 && me.created_ms > 0 && !me.revoked);
    assert_eq!(list.iter().filter(|x| x.current).count(), 1);
    assert!(list.iter().all(|x| x.user_id == alice.user_id));
    // Revoke one other device.
    let _: Value = phone.delete(&format!("/sessions/{}", tablet.session_id)).await.unwrap();
    assert_eq!(tablet.me().await.unwrap_err().code, Some(ErrorCode::SessionRevoked));
    assert_eq!(phone.sessions().await.unwrap().len(), 2);
    // Revoke all others keeps this one.
    let r: Value = phone.post_empty("/sessions/revoke-others").await.unwrap();
    assert_eq!(r["revoked"], 1);
    assert_eq!(phone.sessions().await.unwrap().len(), 1);
    assert!(phone.me().await.is_ok());
    // Another person's session id is a plain 404.
    let bob = s.user("bob").await;
    assert_eq!(bob.delete::<Value>(&format!("/sessions/{}", phone.session_id)).await.unwrap_err().code, Some(ErrorCode::NotFound));
}
