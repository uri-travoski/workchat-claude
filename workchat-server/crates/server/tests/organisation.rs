//! Search filters, threads, mentions and groups, pins, bookmarks, preferences, drafts, forwarding and permalinks,
//! archive and channel deletion, end to end through the real server.
mod common;
use common::*;

fn enc(s: &str) -> String {
    url_enc(s)
}

fn url_enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

async fn search(c: &Client, query: &str) -> SearchResponse {
    c.get(&format!("/search?{query}")).await.unwrap()
}

const Y2100: i64 = 4_102_444_800_000;
const Y2000: i64 = 946_684_800_000;

#[tokio::test]
async fn search_filters_and_strict_isolation() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let general = s.channel("general", &[&alice, &bob]).await;
    let secret = s.channel("secret", &[&alice, &carol]).await;

    let m1 = alice.send(&general, "budget review http://example.com/q4").await;
    bob.send(&general, "budget numbers attached").await;
    alice.send(&secret, "budget layoffs are secret").await;
    bob.react(&m1.id, "👍", true).await.unwrap();
    alice.send_file(&general, "budget file", "report.txt", b"quarterly report".to_vec()).await;

    // Plain search for Bob: never the secret channel.
    let r = search(&bob, "q=budget").await;
    assert_eq!(r.hits.len(), 3, "bob sees only general: {:?}", r.hits.iter().map(|h| &h.message.body).collect::<Vec<_>>());
    assert!(r.hits.iter().all(|h| h.message.conv_id == general));
    // Carol sees only the secret one.
    assert_eq!(search(&carol, "q=budget").await.hits.len(), 1);
    // Filters.
    assert_eq!(search(&alice, "q=budget&from=bob").await.hits.len(), 1);
    assert_eq!(search(&alice, &format!("q=budget&in={secret}")).await.hits.len(), 1);
    assert_eq!(search(&alice, "q=budget&has=link").await.hits.len(), 1);
    assert_eq!(search(&alice, "q=budget&has=file").await.hits.len(), 1);
    assert_eq!(search(&alice, "q=budget&has=reaction").await.hits.len(), 1);
    assert_eq!(search(&alice, &format!("q=budget&before={Y2100}")).await.hits.len(), 4);
    assert_eq!(search(&alice, &format!("q=budget&before={Y2000}")).await.hits.len(), 0);
    assert_eq!(search(&alice, &format!("q=budget&after={Y2000}")).await.hits.len(), 4);
    assert_eq!(search(&alice, &format!("q=budget&after={Y2100}")).await.hits.len(), 0);
    // Isolation when naming a conversation you are not in.
    assert!(search(&bob, &format!("q=layoffs&in={secret}")).await.hits.is_empty());
    assert!(search(&bob, "q=layoffs&from=alice").await.hits.is_empty());

    // Files and people and own channels.
    let r = search(&alice, "q=report").await;
    assert_eq!(r.files.len(), 1);
    assert_eq!(r.files[0].attachment.name, "report.txt");
    assert!(search(&carol, "q=report").await.files.is_empty(), "files of other channels stay hidden");
    assert!(search(&alice, "q=car").await.people.iter().any(|u| u.username == "carol"));
    assert!(search(&bob, "q=sec").await.channels.is_empty(), "bob is not in #secret");
    assert_eq!(search(&alice, "q=sec").await.channels.len(), 1);
    // Files tab list with type filter.
    let files: Vec<FileHit> = alice.get(&format!("/files?conv={general}&type=document")).await.unwrap();
    assert_eq!(files.len(), 1);
    assert!(alice.get::<Vec<FileHit>>(&format!("/files?conv={general}&type=image")).await.unwrap().is_empty());
    assert!(bob.get::<Vec<FileHit>>(&format!("/files?conv={secret}")).await.unwrap().is_empty());
}

#[tokio::test]
async fn threads_replies_follow_and_unread() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let ch = s.channel("dev", &[&alice, &bob, &carol]).await;
    let threads = |c: &Client| {
        let c = c.clone();
        async move { c.get::<Vec<ThreadSummary>>("/threads").await.unwrap() }
    };

    let root = alice.send(&ch, "who can review the PR?").await;
    // Bob replies in the thread; the reply stays out of the main timeline.
    bob.reply(&ch, &root.id, "I can").await;
    assert!(alice.thread(&ch, &root.id).await.unwrap().replies.iter().any(|m| m.body == "I can"));
    // The history lists the reply flagged with its root (apps keep it out of the channel timeline); the root itself is top-level.
    let hist = alice.history(&ch).await;
    assert!(hist.iter().filter(|m| m.thread_root.is_none()).all(|m| m.body != "I can"));
    assert_eq!(hist.iter().find(|m| m.body == "I can").unwrap().thread_root.as_deref(), Some(root.id.as_str()));
    assert_eq!(hist[0].id, root.id);
    // Alice (root author) follows automatically and has an unread thread; Carol is not following.
    assert!(threads(&alice).await.iter().any(|t| t.root.id == root.id && t.unread == 1));
    assert!(threads(&carol).await.is_empty());
    // Bob (replier) follows automatically.
    assert!(threads(&bob).await.iter().any(|t| t.root.id == root.id && t.following));

    // Carol follows manually, then gets the next reply.
    let t: ThreadSummary = carol.put_empty(&format!("/threads/{}/follow", root.id)).await.unwrap();
    assert!(t.following);
    alice.reply(&ch, &root.id, "thanks!").await;
    assert!(threads(&carol).await.iter().any(|t| t.root.id == root.id && t.reply_count == 2 && t.unread >= 1));
    let v = carol.thread(&ch, &root.id).await.unwrap();
    assert_eq!(v.replies.len(), 2);
    assert_eq!(v.summary.reply_count, 2);
    // Reading clears it; unfollowing removes it from the list.
    let _: ThreadSummary = carol.post_empty(&format!("/threads/{}/read", root.id)).await.unwrap();
    assert!(threads(&carol).await.iter().all(|t| t.unread == 0));
    let _: ThreadSummary = carol.delete(&format!("/threads/{}/follow", root.id)).await.unwrap();
    assert!(threads(&carol).await.is_empty());
    // Unfollowed threads are not re-followed by someone else's reply.
    bob.reply(&ch, &root.id, "np").await;
    assert_eq!(alice.thread(&ch, &root.id).await.unwrap().replies.len(), 3);
    assert!(threads(&carol).await.is_empty());
    assert_eq!(threads(&alice).await.len(), 1);
    // A reply to a reply resolves to the same root.
    let reply = alice.thread(&ch, &root.id).await.unwrap().replies[0].clone();
    let m = alice.reply(&ch, &reply.id, "nested").await;
    assert_eq!(m.thread_root.as_deref(), Some(root.id.as_str()));
    // Non-members cannot open or follow it.
    let dave = s.user("dave").await;
    assert!(dave.thread(&ch, &root.id).await.is_err());
    assert!(dave.put_empty::<ThreadSummary>(&format!("/threads/{}/follow", root.id)).await.is_err());
}

#[tokio::test]
async fn mentions_channel_here_and_user_groups() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let (a_id, b_id, c_id) = (alice.user_id.clone(), bob.user_id.clone(), carol.user_id.clone());
    let ch = s.channel("ops", &[&alice, &bob, &carol]).await;
    let mentions = |c: &Client| {
        let (c, ch) = (c.clone(), ch.clone());
        async move { c.conversation(&ch).await.unwrap().mentions }
    };
    // @channel mentions everyone.
    alice.send(&ch, "@channel standup now").await;
    assert_eq!(mentions(&bob).await, 1);
    assert_eq!(mentions(&carol).await, 1);
    assert_eq!(mentions(&alice).await, 0);
    let m = bob.history(&ch).await[0].clone();
    assert!(m.mentions.contains(&c_id) && m.mentions.contains(&b_id));
    assert!(!m.mentions.contains(&a_id), "sender is not mentioned");

    // A user group expands to its members; handle clashes are refused.
    let g = s.admin.create_user_group("oncall", "On call", &[b_id.clone()]).await.unwrap();
    assert_eq!(g.handle, "oncall");
    assert!(s.admin.create_user_group("alice", "x", &[]).await.is_err(), "clashes with a username");
    assert!(s.admin.create_user_group("channel", "x", &[]).await.is_err(), "clashes with @channel");
    let groups: Vec<UserGroup> = alice.get("/groups").await.unwrap();
    assert!(groups.iter().any(|g| g.handle == "oncall"));
    alice.send(&ch, "@oncall please look").await;
    assert_eq!(mentions(&bob).await, 2);
    assert_eq!(mentions(&carol).await, 1, "carol is not in the group");
    // @here only reaches people who are online right now: bob has a live connection, carol has none.
    let _bob_online = bob.ws(bob.cursor().await).await;
    alice.send(&ch, "@here quick question").await;
    assert_eq!(mentions(&bob).await, 3);
    assert_eq!(mentions(&carol).await, 1);
    // Reading clears the count.
    bob.mark_read(&ch).await.unwrap();
    assert_eq!(mentions(&bob).await, 0);
    // Group management: members can be changed, deleting removes it.
    let upd: UserGroup = s.admin.patch(&format!("/admin/groups/{}", g.id), &UserGroupRequest { handle: None, name: Some("Pager".into()), member_ids: Some(vec![c_id.clone()]) }).await.unwrap();
    assert_eq!((upd.name.as_str(), upd.member_ids.clone()), ("Pager", vec![c_id.clone()]));
    let _: Value = s.admin.delete(&format!("/admin/groups/{}", g.id)).await.unwrap();
    assert!(alice.get::<Vec<UserGroup>>("/groups").await.unwrap().iter().all(|g| g.handle != "oncall"));
    // Members may not manage groups.
    assert!(alice.create_user_group("mine", "Mine", &[]).await.is_err());
}

#[tokio::test]
async fn pins_and_bookmarks_with_permissions() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let ch = s.channel("docs", &[&alice, &bob]).await;
    let m = alice.send(&ch, "important").await;
    // With pinning restricted to managers, plain members cannot pin in channels they do not own.
    s.admin.set_org_settings(&UpdateOrgSettings { channel_creation: None, channel_pins: Some(ChannelPins::Managers) }).await.unwrap();
    let e = bob.put_empty::<Vec<Pin>>(&format!("/conversations/{ch}/pins/{}", m.id)).await.unwrap_err();
    assert_eq!(e.code, Some(ErrorCode::Forbidden));
    // An admin may (without being a member of the channel).
    let mut ws = bob.ws(bob.cursor().await).await;
    let pins: Vec<Pin> = s.admin.put_empty(&format!("/conversations/{ch}/pins/{}", m.id)).await.unwrap();
    assert_eq!(pins.len(), 1);
    ws.wait_event("pins update", |e| matches!(e, EventData::PinsUpdate { pins, .. } if pins.len() == 1)).await;
    let listed: Vec<Pin> = bob.get(&format!("/conversations/{ch}/pins")).await.unwrap();
    assert_eq!(listed[0].message_id, m.id);
    assert_eq!(alice.get::<Vec<Pin>>(&format!("/conversations/{ch}/pins")).await.unwrap().len(), 1);

    // Bookmarks: link and validation.
    let attempt = bob.post::<Vec<Bookmark>, _>(&format!("/conversations/{ch}/bookmarks"), &BookmarkRequest { title: "x".into(), url: Some("https://x.test".into()), attachment_id: None, position: None }).await;
    assert_eq!(attempt.unwrap_err().code, Some(ErrorCode::Forbidden));
    let v: Vec<Bookmark> = s.admin.post(&format!("/conversations/{ch}/bookmarks"), &BookmarkRequest { title: "Wiki".into(), url: Some("https://example.com/wiki".into()), attachment_id: None, position: None }).await.unwrap();
    assert_eq!(v.len(), 1);
    let listed: Vec<Bookmark> = bob.get(&format!("/conversations/{ch}/bookmarks")).await.unwrap();
    assert_eq!(listed.len(), 1);
    let bad = s.admin.post::<Vec<Bookmark>, _>(&format!("/conversations/{ch}/bookmarks"), &BookmarkRequest { title: "bad".into(), url: None, attachment_id: None, position: None }).await;
    assert!(bad.is_err(), "a bookmark needs a link or a file");
    // A member cannot delete it; an admin can.
    assert!(alice.delete::<Vec<Bookmark>>(&format!("/bookmarks/{}", listed[0].id)).await.is_err());
    assert!(s.admin.delete::<Vec<Bookmark>>(&format!("/bookmarks/{}", listed[0].id)).await.unwrap().is_empty());
    // Unpin.
    let _: Vec<Pin> = s.admin.delete(&format!("/conversations/{ch}/pins/{}", m.id)).await.unwrap();
    assert!(bob.get::<Vec<Pin>>(&format!("/conversations/{ch}/pins")).await.unwrap().is_empty());
}

#[tokio::test]
async fn favourites_sections_mute_and_prefs_sync_between_devices() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let c1 = s.channel("alpha", &[&alice, &bob]).await;
    let c2 = s.channel("beta", &[&alice, &bob]).await;
    let c3 = s.channel("gamma", &[&alice, &bob]).await;
    let pref = |c: Client, conv: String| async move { c.get::<Vec<ConvPref>>("/me/prefs").await.unwrap().into_iter().find(|p| p.conv_id == conv).unwrap_or_else(|| ConvPref::default_for(&conv)) };
    let put = |conv: String, req: ConvPrefRequest| {
        let alice = alice.clone();
        async move { alice.put::<ConvPref, _>(&format!("/conversations/{conv}/prefs"), &req).await }
    };
    put(c2.clone(), ConvPrefRequest { favourite: Some(true), ..Default::default() }).await.unwrap();
    put(c3.clone(), ConvPrefRequest { mute_until_ms: Some(-1), ..Default::default() }).await.unwrap();
    let secs: Vec<Section> = alice.post("/sections", &SectionRequest { name: Some("Projects".into()), ..Default::default() }).await.unwrap();
    let sec = secs.iter().find(|x| x.builtin_key().is_none()).unwrap().id.clone();
    put(c1.clone(), ConvPrefRequest { section_id: Some(sec.clone()), ..Default::default() }).await.unwrap();

    // A second device of the same account sees all of it after login (bootstrap) ...
    let phone = s.login("alice").await;
    let b = phone.bootstrap().await.unwrap();
    let bp = |id: &String| b.prefs.iter().find(|p| p.conv_id == *id).cloned().unwrap_or_else(|| ConvPref::default_for(id));
    assert!(bp(&c2).favourite && bp(&c3).is_muted(now_ms()) && bp(&c1).section_id.as_deref() == Some(sec.as_str()));
    assert_eq!(b.sections.iter().filter(|x| x.builtin_key().is_none()).count(), 1);
    // ... and live updates.
    let mut ws = phone.ws(b.cursor).await;
    put(c1.clone(), ConvPrefRequest { favourite: Some(true), ..Default::default() }).await.unwrap();
    ws.wait_event("live fav", |e| matches!(e, EventData::PrefUpdate { pref } if pref.conv_id == c1 && pref.favourite)).await;
    put(c3.clone(), ConvPrefRequest { mute_until_ms: Some(0), ..Default::default() }).await.unwrap();
    ws.wait_event("unmuted live", |e| matches!(e, EventData::PrefUpdate { pref } if pref.conv_id == c3 && !pref.is_muted(now_ms()))).await;
    assert!(!pref(phone.clone(), c3.clone()).await.is_muted(now_ms()));
    // Notification level.
    let p = put(c3.clone(), ConvPrefRequest { notify: Some("mentions".into()), ..Default::default() }).await.unwrap();
    assert_eq!(p.notify, "mentions");
    assert!(put(c3.clone(), ConvPrefRequest { notify: Some("bogus".into()), ..Default::default() }).await.is_err());
    // Bob's preferences are his own.
    assert!(!pref(bob.clone(), c2.clone()).await.favourite);
    // Deleting a section un-files its conversations.
    let _: Vec<Section> = alice.delete(&format!("/sections/{sec}")).await.unwrap();
    ws.wait_event("sections update", |e| matches!(e, EventData::SectionsUpdate { sections } if sections.iter().all(|x| x.id != sec))).await;
    assert!(pref(phone.clone(), c1.clone()).await.section_id.is_none());
}

#[tokio::test]
async fn drafts_sync_between_devices() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let dm = alice.open_dm(&bob.user_id).await.unwrap();
    let phone = s.login("alice").await;
    let mut ws = phone.ws(phone.cursor().await).await;
    let put = |thread: Option<&str>, body: &str| {
        let alice = alice.clone();
        let (dm, thread, body) = (dm.clone(), thread.map(str::to_string), body.to_string());
        async move { alice.put::<DraftItem, _>(&format!("/conversations/{dm}/draft"), &DraftRequest { thread_root: thread, body }).await.unwrap() }
    };
    put(None, "half written thought").await;
    ws.wait_event("draft event", |e| matches!(e, EventData::DraftUpdate { draft } if draft.body == "half written thought" && draft.thread_root.is_none())).await;
    put(Some("t1"), "thread draft").await;
    let drafts: Vec<DraftItem> = phone.get("/drafts").await.unwrap();
    assert_eq!(drafts.len(), 2);
    assert!(drafts.iter().any(|d| d.thread_root.as_deref() == Some("t1") && d.body == "thread draft"));
    // Bob never sees them.
    assert!(bob.get::<Vec<DraftItem>>("/drafts").await.unwrap().is_empty());
    // Clearing it (or sending) clears everywhere.
    put(None, "").await;
    ws.wait_event("draft cleared", |e| matches!(e, EventData::DraftUpdate { draft } if draft.body.is_empty() && draft.thread_root.is_none())).await;
    assert_eq!(phone.get::<Vec<DraftItem>>("/drafts").await.unwrap().len(), 1);
}

#[tokio::test]
async fn forwarding_quote_reply_and_permalinks() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let src = s.channel("source", &[&alice, &bob]).await;
    let dst = s.channel("dest", &[&alice, &carol]).await;
    let orig = alice.send_file(&src, "look at this", "pic.png", vec![0x89, b'P', b'N', b'G', 1, 2, 3]).await;
    // Carol is not in #source: she cannot forward or read it.
    let fwd_req = |c: &Client, comment: Option<&str>| {
        let (c, id, dst) = (c.clone(), orig.id.clone(), dst.clone());
        let comment = comment.map(str::to_string);
        async move { c.post::<Vec<Message>, _>(&format!("/messages/{id}/forward"), &ForwardRequest { conv_ids: vec![dst], comment }).await }
    };
    assert!(fwd_req(&carol, None).await.is_err());
    let fw = fwd_req(&alice, Some("FYI")).await.unwrap();
    assert_eq!(fw.len(), 2, "forwarded copy plus the comment");
    let fwd = fw.iter().find(|m| m.forwarded.is_some()).unwrap();
    assert_eq!(fwd.forwarded.as_ref().unwrap().sender_id, alice.user_id);
    assert_eq!(fwd.forwarded.as_ref().unwrap().message_id, orig.id);
    let got = carol.history(&dst).await.into_iter().find(|m| m.body == "look at this").unwrap();
    assert!(got.forwarded.is_some());
    assert_eq!(got.attachments.len(), 1, "forwarded attachment is re-used");
    // Carol can download it even though she is not in the source channel.
    assert_eq!(carol.download(&got.attachments[0].id).await.unwrap().len(), 7);
    assert!(carol.texts(&dst).await.iter().any(|t| t == "FYI"));
    // Deleting the original does not delete the forwarded copy (F-04).
    alice.delete_message(&orig.id).await.unwrap();
    assert!(carol.download(&got.attachments[0].id).await.is_ok());

    // Permalinks resolve for members only.
    let m = alice.send(&dst, "see here").await;
    let link = proto::rules::make_permalink(&s.url, &m.conv_id, &m.id);
    assert!(link.starts_with("workchat://m?"), "{link}");
    let parsed = proto::rules::parse_permalink(&link).unwrap();
    let opened: Message = carol.get(&format!("/messages/{}", parsed.message_id)).await.unwrap();
    assert_eq!(opened.id, m.id);
    let h = carol.history_raw(&opened.conv_id, &format!("around_id={}&limit=40", opened.id)).await.unwrap();
    assert!(h.messages.iter().any(|x| x.id == m.id));
    let err = bob.get::<Message>(&format!("/messages/{}", parsed.message_id)).await.unwrap_err();
    assert!(matches!(err.code, Some(ErrorCode::Forbidden) | Some(ErrorCode::NotFound)), "{err:?}");

    // Quote reply: reply_to points at the original, within the same conversation.
    let q = carol.send_req(&dst, &SendMessage { client_id: "q1".into(), body: "quoting".into(), thread_root: None, reply_to: Some(m.id.clone()), mentions: vec![], attachment_ids: vec![], also_send_to_channel: false }).await.unwrap();
    assert_eq!(q.reply_to.as_deref(), Some(m.id.as_str()));
    assert_eq!(alice.history(&dst).await.iter().find(|x| x.body == "quoting").unwrap().reply_to.as_deref(), Some(m.id.as_str()));
}

#[tokio::test]
async fn archive_is_read_only_and_reversible_and_admin_deletes_channels() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let ch = s.channel("old-project", &[&alice]).await;
    alice.send(&ch, "before archive").await;
    let path = format!("/conversations/{ch}");
    let mut ws = alice.ws(alice.cursor().await).await;
    s.admin.patch::<Value, _>(&path, &json!({"archived": true})).await.unwrap();
    ws.wait_event("archived syncs", |e| matches!(e, EventData::ConversationUpsert { conversation } if conversation.conversation.id == ch && conversation.conversation.archived)).await;
    assert!(alice.conversation(&ch).await.unwrap().conversation.archived);
    assert!(alice.try_send(&ch, "should fail").await.is_err(), "an archived conversation is read-only");
    assert_eq!(alice.texts(&ch).await, vec!["before archive"]);
    s.admin.patch::<Value, _>(&path, &json!({"archived": false})).await.unwrap();
    assert!(!alice.conversation(&ch).await.unwrap().conversation.archived);
    assert!(alice.try_send(&ch, "works again").await.is_ok());
    // Members may not archive.
    assert!(alice.patch::<Value, _>(&path, &json!({"archived": true})).await.is_err());
    // Admin deletes the channel: it disappears for members.
    let _: Value = s.admin.delete(&format!("/admin/conversations/{ch}")).await.unwrap();
    assert!(!alice.has_conversation(&ch).await);
}
