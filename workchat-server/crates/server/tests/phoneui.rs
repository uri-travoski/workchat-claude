//! 1.1.0 additions the apps need: profile title and phone, the directory search, the counts behind the filter pills,
//! the threads and files lists, read receipts, UnifiedPush registration, bot pictures and admin backdating.
mod common;
use common::fakes::*;
use common::*;
use std::time::Duration;

fn create_req(username: &str, display: &str, title: Option<&str>, phone: Option<&str>) -> CreateUserRequest {
    CreateUserRequest {
        username: username.into(),
        display_name: display.into(),
        password: Some(PW.into()),
        role: None,
        email: None,
        title: title.map(str::to_string),
        phone: phone.map(str::to_string),
        create_login_link: None,
    }
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(600)).await;
}

#[tokio::test]
async fn profile_title_phone_and_directory_search() {
    let s = TestServer::start().await;
    let created: CreateUserResponse = s.admin.post("/admin/users", &create_req("katherine", "Katherine Wells", Some("Head of Product Operations"), Some("+1 (555) 382-9014"))).await.unwrap();
    assert_eq!(created.user.user.title.as_deref(), Some("Head of Product Operations"));
    let kat = s.login("katherine").await;
    let bob = s.user("bob").await;

    // Other people see title and phone in the directory, and in /users/{id}.
    let users = bob.users_list().await.unwrap();
    let k = users.iter().find(|u| u.username == "katherine").unwrap();
    assert_eq!((k.title.as_deref(), k.phone.as_deref()), (Some("Head of Product Operations"), Some("+1 (555) 382-9014")));
    let one: User = bob.get(&format!("/users/{}", kat.user_id)).await.unwrap();
    assert_eq!(one.title.as_deref(), Some("Head of Product Operations"));
    assert!(users.iter().find(|u| u.username == "bob").unwrap().title.is_none());

    // The person edits their own profile; others get a user_upsert and Me carries the same.
    let mut ws = bob.ws(bob.cursor().await).await;
    let me: Me = kat.patch("/me", &UpdateMeRequest { title: Some("VP Operations".into()), phone: Some("+1 555 0100".into()), ..Default::default() }).await.unwrap();
    assert_eq!(me.user.title.as_deref(), Some("VP Operations"));
    ws.wait_event("user_upsert with title", |e| matches!(e, EventData::UserUpsert { user } if user.username == "katherine" && user.title.as_deref() == Some("VP Operations"))).await;

    // Search by name, username and title; `active` leaves out disabled people.
    let by_title: Vec<User> = bob.get("/users?q=vp%20oper").await.unwrap();
    assert_eq!(by_title.len(), 1);
    assert_eq!(by_title[0].username, "katherine");
    let by_name: Vec<User> = bob.get("/users?q=WELLS").await.unwrap();
    assert_eq!(by_name.len(), 1);
    let none: Vec<User> = bob.get("/users?q=zzz").await.unwrap();
    assert!(none.is_empty());
    s.admin.set_disabled(&kat.user_id, true).await.unwrap();
    let active: Vec<User> = bob.get("/users?active=true").await.unwrap();
    assert!(active.iter().all(|u| u.username != "katherine"));
    s.admin.set_disabled(&kat.user_id, false).await.unwrap();

    // An empty string clears; too long is refused; admins can edit it too.
    let kat = s.login("katherine").await;
    let me: Me = kat.patch("/me", &UpdateMeRequest { title: Some(String::new()), ..Default::default() }).await.unwrap();
    assert!(me.user.title.is_none());
    assert_eq!(me.user.phone.as_deref(), Some("+1 555 0100"));
    let r: Res<Me> = kat.patch("/me", &UpdateMeRequest { title: Some("x".repeat(81)), ..Default::default() }).await;
    assert_eq!(r.unwrap_err().code, Some(ErrorCode::Validation));
    let a = s.admin.update_user(&kat.user_id, &UpdateUserRequest { title: Some("Chief of Staff".into()), ..Default::default() }).await.unwrap();
    assert_eq!(a.user.title.as_deref(), Some("Chief of Staff"));
}

#[tokio::test]
async fn counts_for_the_filter_pills_and_the_threads_and_files_lists() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let ch = s.channel("design", &[&alice, &bob, &carol]).await;
    let ch2 = s.channel("ops", &[&alice, &bob]).await;
    let dm = alice.open_dm(&bob.user_id).await.unwrap();
    let dm2 = alice.open_dm(&carol.user_id).await.unwrap();

    // Nothing yet.
    let n: Counts = alice.get("/me/counts").await.unwrap();
    assert_eq!((n.conversations, n.dms, n.channels, n.threads, n.files, n.unread_messages), (4, 2, 2, 0, 0, 0));

    // Bob writes in the DM and the channel, replies to Alice's thread, shares an image, a voice note and two documents.
    let root = alice.send(&ch, "Q3 design review notes?").await;
    bob.send(&dm, "hi").await;
    bob.send(&dm, "are you there?").await;
    bob.send(&ch, "morning all").await;
    let r1 = bob.reply(&ch, &root.id, "Here they are").await;
    let img = bob.upload_as("photo.jpg", "image/jpeg", None, vec![1u8; 300]).await.unwrap();
    let voice = bob.upload_as("voice-1.ogg", "audio/ogg", None, vec![2u8; 300]).await.unwrap();
    let _: Value = bob.put(&format!("/files/{}/meta", voice.id), &AttachmentMeta { duration_ms: Some(42_000), peaks: vec![10, 90, 200], ..Default::default() }).await.unwrap();
    let pdf = bob.upload_as("Q3.pdf", "application/pdf", None, vec![3u8; 300]).await.unwrap();
    let zip = bob.upload_as("assets.zip", "application/zip", None, vec![4u8; 300]).await.unwrap();
    let m_img = bob.send_with_files(&ch, "", &[img.id.clone()]).await.unwrap();
    bob.send_with_files(&ch2, "", &[voice.id.clone()]).await.unwrap();
    bob.send_with_files(&dm, "", &[pdf.id.clone()]).await.unwrap();
    bob.send_with_files(&ch, "", &[zip.id.clone()]).await.unwrap();
    carol.send(&dm2, "dm from carol").await;
    // A deleted message's file is not counted.
    let gone = bob.upload_as("gone.png", "image/png", None, vec![5u8; 300]).await.unwrap();
    let m_gone = bob.send_with_files(&ch, "", &[gone.id]).await.unwrap();
    bob.delete_message(&m_gone.id).await.unwrap();

    let n: Counts = alice.get("/me/counts").await.unwrap();
    assert_eq!((n.conversations, n.dms, n.channels), (4, 2, 2));
    assert_eq!((n.unread_conversations, n.unread_dms, n.unread_channels), (4, 2, 2));
    // dm: hi, are you there, pdf = 3; dm2: 1; ch: morning, reply, image, zip = 4 (thread reply included); ch2: voice = 1.
    assert_eq!(n.unread_messages, 3 + 1 + 4 + 1, "{n:?}");
    assert_eq!(n.threads, 1);
    assert_eq!(n.threads_unread, 1);
    assert_eq!((n.files, n.files_image, n.files_video, n.files_audio, n.files_document), (4, 1, 0, 1, 2));
    // Carol is not in ops: she sees fewer files.
    let nc: Counts = carol.get("/me/counts").await.unwrap();
    assert_eq!((nc.files, nc.files_audio), (2, 0));

    // Threads list.
    let t: Vec<ThreadSummary> = alice.get("/threads").await.unwrap();
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].root.id, root.id);
    assert_eq!((t[0].reply_count, t[0].unread), (1, 1));
    assert_eq!(t[0].last_reply.as_ref().map(|m| m.id.as_str()), Some(r1.id.as_str()));
    assert_eq!(t[0].last_reply.as_ref().unwrap().body, "Here they are");
    let r2 = alice.reply(&ch, &root.id, "thanks").await;
    let t: Vec<ThreadSummary> = bob.get("/threads").await.unwrap();
    assert_eq!(t[0].last_reply.as_ref().unwrap().id, r2.id);
    assert_eq!(t[0].unread, 1);
    bob.reply(&ch, &root.id, "one more thing").await;
    // A second thread in another conversation, with filters.
    let root2 = bob.send(&ch2, "deploy plan").await;
    alice.reply(&ch2, &root2.id, "looks good").await;
    let all: Vec<ThreadSummary> = alice.get("/threads").await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].root.id, root2.id, "newest activity first");
    let only: Vec<ThreadSummary> = alice.get(&format!("/threads?conv={ch2}")).await.unwrap();
    assert_eq!(only.len(), 1);
    let unread: Vec<ThreadSummary> = alice.get("/threads?unread=true").await.unwrap();
    assert_eq!(unread.len(), 1, "the thread where Alice wrote last has nothing unread for her");
    assert_eq!(unread[0].root.id, root.id, "only the first thread has unread replies");
    let page1: Vec<ThreadSummary> = alice.get("/threads?limit=1").await.unwrap();
    assert_eq!(page1.len(), 1);
    let act = page1[0].last_reply_ts.max(page1[0].root.ts);
    let page2: Vec<ThreadSummary> = alice.get(&format!("/threads?limit=1&before={act}")).await.unwrap();
    assert_eq!(page2.len(), 1);
    assert_ne!(page2[0].root.id, page1[0].root.id);
    // Reading the thread clears the count.
    let _: ThreadSummary = alice.post_empty(&format!("/threads/{}/read", root.id)).await.unwrap();
    let n: Counts = alice.get("/me/counts").await.unwrap();
    assert_eq!(n.threads_unread, 0);

    // Files list: kinds (also under `kind=`), paging, details of voice notes, newest first.
    let files: Vec<FileHit> = alice.get("/files").await.unwrap();
    assert_eq!(files.len(), 4);
    assert!(files.windows(2).all(|w| w[0].ts >= w[1].ts));
    let audio: Vec<FileHit> = alice.get("/files?kind=audio").await.unwrap();
    assert_eq!(audio.len(), 1);
    assert_eq!(audio[0].conv_id, ch2);
    let meta = audio[0].attachment.meta.clone().expect("duration and peaks come with the file list");
    assert_eq!((meta.duration_ms, meta.peaks), (Some(42_000), vec![10, 90, 200]));
    assert_eq!(alice.get::<Vec<FileHit>>("/files?type=image").await.unwrap().len(), 1);
    assert_eq!(alice.get::<Vec<FileHit>>("/files?kind=document").await.unwrap().len(), 2);
    assert_eq!(alice.get::<Vec<FileHit>>("/files?kind=video").await.unwrap().len(), 0);
    assert_eq!(alice.get::<Vec<FileHit>>(&format!("/files?conv={dm}")).await.unwrap().len(), 1);
    assert_eq!(alice.get::<Vec<FileHit>>("/files?q=Q3").await.unwrap()[0].attachment.name, "Q3.pdf");
    let p1: Vec<FileHit> = alice.get("/files?limit=3").await.unwrap();
    let p2: Vec<FileHit> = alice.get("/files?limit=3&offset=3").await.unwrap();
    assert_eq!((p1.len(), p2.len()), (3, 1));
    assert!(p1.iter().all(|f| f.attachment.id != p2[0].attachment.id));
    assert_eq!(files.iter().find(|f| f.message_id == m_img.id).unwrap().sender_id, bob.user_id);

    // Reading everything clears the unread numbers.
    for c in [&ch, &ch2, &dm, &dm2] {
        let last = alice.conversation(c).await.unwrap().conversation.last_seq;
        alice.post::<ConversationSummary, _>(&format!("/conversations/{c}/read"), &MarkReadRequest { seq: last }).await.unwrap();
    }
    let n: Counts = alice.get("/me/counts").await.unwrap();
    assert_eq!((n.unread_conversations, n.unread_messages), (0, 0));
    // Marking one chat unread shows up as an unread conversation without unread messages.
    let _: ConversationSummary = alice.post_empty(&format!("/conversations/{dm}/unread")).await.unwrap();
    let n: Counts = alice.get("/me/counts").await.unwrap();
    assert_eq!((n.unread_conversations, n.unread_dms, n.unread_messages), (1, 1, 0));
}

#[tokio::test]
async fn read_receipts_follow_the_readers_choice() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let ch = s.channel("team", &[&alice, &bob]).await;
    let dm = alice.open_dm(&bob.user_id).await.unwrap();
    let m1 = alice.send(&dm, "one").await;
    let m2 = alice.send(&dm, "two").await;
    alice.send(&ch, "in channel").await;

    // Nobody has read yet: the DM members carry last_read_seq 0 for the other person; channels carry none.
    let c = alice.conversation(&dm).await.unwrap().conversation;
    assert_eq!(c.members.iter().find(|m| m.user_id == bob.user_id).unwrap().last_read_seq, Some(0));
    let cc = alice.conversation(&ch).await.unwrap().conversation;
    assert!(cc.members.iter().all(|m| m.last_read_seq.is_none()));

    // Bob reads: Alice's open app gets a read_receipt, and the conversation shows it.
    let mut ws = alice.ws(alice.cursor().await).await;
    bob.post::<ConversationSummary, _>(&format!("/conversations/{dm}/read"), &MarkReadRequest { seq: m1.seq }).await.unwrap();
    let e = ws.wait_event("read receipt", |e| matches!(e, EventData::ReadReceipt { .. })).await;
    match e.event {
        EventData::ReadReceipt { conv_id, user_id, last_read_seq } => assert_eq!((conv_id, user_id, last_read_seq), (dm.clone(), bob.user_id.clone(), m1.seq)),
        _ => unreachable!(),
    }
    let c = alice.conversation(&dm).await.unwrap().conversation;
    assert_eq!(c.members.iter().find(|m| m.user_id == bob.user_id).unwrap().last_read_seq, Some(m1.seq));
    // Reading again without progress sends nothing new.
    bob.post::<ConversationSummary, _>(&format!("/conversations/{dm}/read"), &MarkReadRequest { seq: m1.seq }).await.unwrap();
    // The reader does not get a receipt about themselves; channels never send them.
    bob.post::<ConversationSummary, _>(&format!("/conversations/{ch}/read"), &MarkReadRequest { seq: 1 }).await.unwrap();

    // Bob switches receipts off: no event, and his position is hidden.
    let me: Me = bob.patch("/me", &UpdateMeRequest { read_receipts: Some(false), ..Default::default() }).await.unwrap();
    assert!(!me.read_receipts);
    assert!(alice.me().await.unwrap().read_receipts, "default is on");
    bob.post::<ConversationSummary, _>(&format!("/conversations/{dm}/read"), &MarkReadRequest { seq: m2.seq }).await.unwrap();
    let c = alice.conversation(&dm).await.unwrap().conversation;
    assert_eq!(c.members.iter().find(|m| m.user_id == bob.user_id).unwrap().last_read_seq, None);
    // His own unread state is still tracked normally.
    assert_eq!(bob.conversation(&dm).await.unwrap().unread, 0);
    // Alice still shares hers (her own messages count as read).
    assert_eq!(bob.conversation(&dm).await.unwrap().conversation.members.iter().find(|m| m.user_id == alice.user_id).unwrap().last_read_seq, Some(m2.seq));
    // The event log of Alice holds exactly one receipt.
    let evs = alice.events(0).await.unwrap().events;
    assert_eq!(evs.iter().filter(|e| matches!(e.event, EventData::ReadReceipt { .. })).count(), 1);
}

#[tokio::test]
async fn unifiedpush_endpoint_registration_and_content_free_wake() {
    let (fake, url) = PushFake::start().await;
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let dm = alice.open_dm(&bob.user_id).await.unwrap();

    // Not a URL, or not http(s): refused.
    for bad in ["upAbC123", "ftp://ntfy.example.com/up1", ""] {
        let r: Res<PushRegistration> = bob.put("/me/push", &PushRegistration { id: String::new(), kind: "unifiedpush".into(), target: bad.into(), ntfy_url: None, device_name: "p".into(), created_ms: 0 }).await;
        assert_eq!(r.unwrap_err().code, Some(ErrorCode::Validation), "{bad}");
    }
    // The full endpoint an ntfy distributor gave the app (path prefix and ?up=1 included).
    let endpoint = format!("{url}/ntfy/upAbC123?up=1");
    let reg: PushRegistration = bob.put("/me/push", &PushRegistration { id: String::new(), kind: "unifiedpush".into(), target: endpoint.clone(), ntfy_url: None, device_name: "Pixel".into(), created_ms: 0 }).await.unwrap();
    assert_eq!(reg.kind, "unifiedpush");
    assert_eq!(bob.get::<Vec<PushRegistration>>("/me/push").await.unwrap()[0].target, endpoint);

    alice.send(&dm, "top secret text").await;
    eventually!("unifiedpush wake", fake.pushes().len() == 1);
    let p = fake.pushes().remove(0);
    assert_eq!((p.method.as_str(), p.path.as_str(), p.query.as_str()), ("POST", "/ntfy/upAbC123", "up=1"));
    assert_eq!(p.body, "wake");
    let all = fake.dump();
    for leak in ["secret", "Alice", "alice", alice.user_id.as_str(), dm.as_str()] {
        assert!(!all.contains(leak), "push leaked {leak:?}: {all}");
    }

    // The same device may also use the split form (server URL + topic) with kind "ntfy": the ntfy app's endpoint
    // `https://host/<topic>?up=1` is `ntfy_url=https://host`, `target=<topic>`.
    let reg2: PushRegistration = bob
        .put("/me/push", &PushRegistration { id: String::new(), kind: "ntfy".into(), target: "upXyz789".into(), ntfy_url: Some(url.clone()), device_name: "Pixel".into(), created_ms: 0 })
        .await
        .unwrap();
    assert_ne!(reg.id, reg2.id);
    assert_eq!(bob.get::<Vec<PushRegistration>>("/me/push").await.unwrap().len(), 2);
    alice.send(&dm, "second").await;
    eventually!("both wakes", fake.pushes().len() == 3);
    let paths: Vec<String> = fake.pushes().iter().map(|p| p.path.clone()).collect();
    assert!(paths.contains(&"/upXyz789".to_string()), "{paths:?}");
}

#[tokio::test]
async fn unifiedpush_endpoint_that_is_gone_is_dropped() {
    use axum::http::StatusCode;
    use axum::routing::any;
    let app = axum::Router::new().fallback(any(|| async { StatusCode::NOT_FOUND }));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let dm = alice.open_dm(&bob.user_id).await.unwrap();
    bob.put::<PushRegistration, _>("/me/push", &PushRegistration { id: String::new(), kind: "unifiedpush".into(), target: format!("{url}/up1?up=1"), ntfy_url: None, device_name: "p".into(), created_ms: 0 }).await.unwrap();
    alice.send(&dm, "hello").await;
    eventually!("registration dropped after 404", bob.get::<Vec<PushRegistration>>("/me/push").await.unwrap().is_empty());
}

#[tokio::test]
async fn bots_get_a_picture_and_title_and_admins_can_backdate_messages() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let ch = s.channel("deploys", &[&alice]).await;
    let bot = s.admin.create_bot("deploybot", "Deploy Bot", "Posts deployments", &[ch.clone()]).await.unwrap();
    let pic = s.admin.upload_as("bot.png", "image/png", Some("avatar"), vec![9u8; 200]).await.unwrap();
    let b = s.admin.update_bot(&bot.user.id, &UpdateBotRequest { avatar: Some(pic.id.clone()), title: Some("CI/CD".into()), ..Default::default() }).await.unwrap();
    assert_eq!(b.user.avatar.as_deref(), Some(pic.id.as_str()));
    assert_eq!(b.user.title.as_deref(), Some("CI/CD"));
    let seen = alice.users_list().await.unwrap().into_iter().find(|u| u.is_bot).unwrap();
    assert_eq!((seen.avatar, seen.title.as_deref()), (Some(pic.id.clone()), Some("CI/CD")));
    assert!(s.admin.update_bot(&bot.user.id, &UpdateBotRequest { avatar: Some("nope".into()), ..Default::default() }).await.is_err());
    let b = s.admin.update_bot(&bot.user.id, &UpdateBotRequest { avatar: Some(String::new()), ..Default::default() }).await.unwrap();
    assert!(b.user.avatar.is_none());

    // Backdating: three messages, the middle one moved two days back is refused (older than its predecessor);
    // moving the first back is fine, the conversation's last time follows, and order stays consistent.
    let m1 = alice.send(&ch, "first").await;
    let m2 = alice.send(&ch, "second").await;
    let day = 86_400_000;
    let mut ws = alice.ws(alice.cursor().await).await;
    let r: Res<Message> = s.admin.put(&format!("/admin/messages/{}/ts", m2.id), &AdminSetMessageTs { ts: m1.ts - 2 * day }).await;
    assert_eq!(r.unwrap_err().code, Some(ErrorCode::Validation));
    let r: Res<Message> = s.admin.put(&format!("/admin/messages/{}/ts", m1.id), &AdminSetMessageTs { ts: m2.ts + 10 * day }).await;
    assert_eq!(r.unwrap_err().code, Some(ErrorCode::Validation), "not in the future");
    let moved: Message = s.admin.put(&format!("/admin/messages/{}/ts", m1.id), &AdminSetMessageTs { ts: m1.ts - 3 * day }).await.unwrap();
    assert_eq!(moved.ts, m1.ts - 3 * day);
    assert!(!moved.is_edited());
    ws.wait_event("message_edit with the new time", |e| matches!(e, EventData::MessageEdit { message } if message.id == m1.id && message.ts == m1.ts - 3 * day)).await;
    let m2b: Message = s.admin.put(&format!("/admin/messages/{}/ts", m2.id), &AdminSetMessageTs { ts: m2.ts - 2 * day }).await.unwrap();
    assert_eq!(m2b.ts, m2.ts - 2 * day);
    let c = alice.conversation(&ch).await.unwrap().conversation;
    assert_eq!(c.last_message_ts, Some(m2.ts - 2 * day));
    let hist = alice.history(&ch).await;
    let times: Vec<i64> = hist.iter().filter(|m| m.kind != MessageKind::System).map(|m| m.ts).collect();
    assert!(times.windows(2).all(|w| w[0] <= w[1]), "{times:?}");
    // Members cannot use it.
    let r: Res<Message> = alice.put(&format!("/admin/messages/{}/ts", m1.id), &AdminSetMessageTs { ts: m1.ts }).await;
    assert_eq!(r.unwrap_err().code, Some(ErrorCode::Forbidden));
    settle().await;
}

#[tokio::test]
async fn the_server_serves_the_workchat_icon() {
    let s = TestServer::start().await;
    let anon = Client::anon(&s.url);
    for (path, ct, magic) in [("/favicon.svg", "image/svg+xml", &b"<svg"[..]), ("/favicon.png", "image/png", &b"\x89PNG"[..]), ("/favicon.ico", "image/x-icon", &b"\x00\x00\x01\x00"[..]), ("/apple-touch-icon.png", "image/png", &b"\x89PNG"[..])] {
        let r = anon.raw_url(HttpMethod::GET, &format!("{}{}", s.url, path), None, &[]).await;
        assert_eq!(r.status, 200, "{path}");
        assert_eq!(r.header("content-type"), ct, "{path}");
        assert!(r.body.starts_with(magic), "{path}");
    }
    // The landing page of a login link links to it and still loads nothing from other sites.
    let link = s.admin.login_link(&s.admin.user_id).await.unwrap();
    let r = anon.raw_url(HttpMethod::GET, &link.link, None, &[]).await;
    let html = r.text();
    assert!(html.contains(r#"href="/favicon.svg""#));
    assert!(r.header("content-security-policy").contains("img-src 'self' data:"));
}
