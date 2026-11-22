//! 1.3 features on the server side: big streamed uploads, profile photos, deleting a channel with everything in it,
//! voice/video details, sidebar order of conversations and sections, PDF previews.
mod common;
use common::*;

#[tokio::test(flavor = "multi_thread")]
async fn large_uploads_stream_to_storage_and_the_limit_is_enforced_up_front() {
    let s = TestServer::start_with(|c| c.max_upload_bytes = 64 * 1024 * 1024).await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let ch = s.channel("files", &[&alice, &bob]).await;
    // 40 MB: far over the 2 MB JSON body limit, so the upload route must stream.
    let big: Vec<u8> = (0..40 * 1024 * 1024u32).map(|i| (i % 253) as u8).collect();
    let m = alice.send_file(&ch, "big one", "big.bin", big.clone()).await;
    assert_eq!(m.attachments[0].size, big.len() as i64);
    let back = bob.download(&m.attachments[0].id).await.unwrap();
    assert!(back == big, "the stored bytes are the uploaded bytes");
    // Over the configured maximum: refused up front from the declared Content-Length, before a byte of the body is read
    // (checked on a raw socket: the request announces 100 MB and sends none), and nothing half-stored is listed.
    let before = s.admin.storage().await.unwrap().attachments;
    let addr = s.url.trim_start_matches("http://").to_string();
    let mut sock = tokio::net::TcpStream::connect(&addr).await.unwrap();
    let req = format!(
        "POST /api/v1/files?name=huge.bin HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
        alice.access_token(),
        100 * 1024 * 1024
    );
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    sock.write_all(req.as_bytes()).await.unwrap();
    let mut buf = vec![0u8; 4096];
    let n = tokio::time::timeout(std::time::Duration::from_secs(5), sock.read(&mut buf)).await.expect("answered without the body").unwrap();
    let head = String::from_utf8_lossy(&buf[..n]).to_string();
    assert!(head.starts_with("HTTP/1.1 413"), "{head}");
    assert!(head.contains("file_too_large"), "{head}");
    assert_eq!(s.admin.storage().await.unwrap().attachments, before);
    // The queue (the server) still works afterwards.
    let m = alice.send_file(&ch, "after refusal", "small.txt", b"hello".to_vec()).await;
    assert_eq!(m.attachments[0].name, "small.txt");
    assert_eq!(bob.download(&m.attachments[0].id).await.unwrap(), b"hello");
}

#[tokio::test]
async fn a_message_can_carry_several_files_and_an_unused_upload_is_not_deliverable_by_others() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let ch = s.channel("files", &[&alice, &bob]).await;
    let a = alice.upload("a.txt", b"first".to_vec()).await.unwrap();
    let b = alice.upload("b.txt", b"second".to_vec()).await.unwrap();
    let m = alice.send_with_files(&ch, "two files", &[a.id.clone(), b.id.clone()]).await.unwrap();
    assert_eq!(m.attachments.len(), 2);
    assert_eq!(bob.history(&ch).await.iter().filter(|m| m.body == "two files").count(), 1);
    // Bob cannot attach alice's already used upload to his own message.
    let r = bob.send_with_files(&ch, "stolen", &[a.id.clone()]).await;
    assert!(r.is_err(), "an upload can only be sent by its uploader");
    // A file nobody attached cannot be downloaded by another user either.
    let lone = alice.upload("lone.txt", b"mine".to_vec()).await.unwrap();
    assert!(bob.download(&lone.id).await.is_err());
    assert_eq!(alice.download(&lone.id).await.unwrap(), b"mine");
}

#[tokio::test]
async fn other_peoples_profile_photos_are_published_and_can_be_fetched() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let find = |us: Vec<User>| us.into_iter().find(|u| u.id == alice.user_id).unwrap();
    assert!(find(bob.users_list().await.unwrap()).avatar.is_none(), "no photo yet: initials");
    let png = b"\x89PNG\r\n\x1a\nnot a real image but good enough for the cache".to_vec();
    let att = alice.upload_as("me.png", "image/png", Some("avatar"), png.clone()).await.unwrap();
    let me: Me = alice.patch("/me", &UpdateMeRequest { avatar: Some(att.id.clone()), ..Default::default() }).await.unwrap();
    assert_eq!(me.user.avatar.as_deref(), Some(att.id.as_str()));
    let mut ws = bob.ws(bob.cursor().await).await;
    let again: Me = alice.patch("/me", &UpdateMeRequest { avatar: Some(att.id.clone()), ..Default::default() }).await.unwrap();
    assert_eq!(again.user.avatar, me.user.avatar);
    assert_eq!(find(bob.users_list().await.unwrap()).avatar.as_deref(), Some(att.id.as_str()));
    assert_eq!(bob.download(&att.id).await.unwrap(), png, "everybody may fetch a profile photo");
    // Removing the photo goes back to initials, and others are told.
    alice.patch::<Me, _>("/me", &UpdateMeRequest { avatar: Some(String::new()), ..Default::default() }).await.unwrap();
    ws.wait_event("user_upsert without avatar", |e| matches!(e, EventData::UserUpsert { user } if user.id == alice.user_id && user.avatar.is_none())).await;
    assert!(find(bob.users_list().await.unwrap()).avatar.is_none());
    // A normal file cannot be set as a profile photo by someone else's id.
    let secret = alice.upload("secret.txt", b"x".to_vec()).await.unwrap();
    assert!(bob.patch::<Me, _>("/me", &UpdateMeRequest { avatar: Some(secret.id), ..Default::default() }).await.is_err());
}

#[tokio::test]
async fn admin_deletes_a_channel_with_messages_files_and_saved_items() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let ch = s.channel("doomed", &[&alice, &bob]).await;
    let m = alice.send_file(&ch, "with a file", "note.txt", b"remember".to_vec()).await;
    let _: Value = bob.put_empty(&format!("/saved/{}", m.id)).await.unwrap();
    let _ = bob.put_empty::<Value>(&format!("/conversations/{ch}/pins/{}", m.id)).await;
    // A member who is not an admin is refused; the owner/admin may delete.
    assert_eq!(bob.delete::<Value>(&format!("/admin/conversations/{ch}")).await.unwrap_err().code, Some(ErrorCode::Forbidden));
    s.admin.set_role(&alice.user_id, Role::Admin).await.unwrap();
    let c = bob.cursor().await;
    alice.delete::<Value>(&format!("/admin/conversations/{ch}")).await.unwrap();
    assert!(!bob.has_conversation(&ch).await && !alice.has_conversation(&ch).await);
    assert!(bob.events(c).await.unwrap().events.iter().any(|e| matches!(&e.event, EventData::ConversationRemove { conv_id } if *conv_id == ch)));
    assert_eq!(bob.history_raw(&ch, "").await.unwrap_err().code, Some(ErrorCode::NotFound));
    // Nothing is left on the server for it: no messages, no attachment rows, no saved items.
    let ch2 = ch.clone();
    let left: (i64, i64, i64) = s
        .st
        .db
        .call(move |c| {
            let msgs = c.query_row("SELECT COUNT(*) FROM messages WHERE conv_id=?1", [&ch2], |r| r.get(0))?;
            let saved = c.query_row("SELECT COUNT(*) FROM saved_items WHERE conv_id=?1", [&ch2], |r| r.get(0))?;
            let atts = c.query_row("SELECT COUNT(*) FROM attachments WHERE removed=0 AND purpose='file'", [], |r| r.get(0))?;
            Ok((msgs, saved, atts))
        })
        .await
        .unwrap();
    assert_eq!(left.0, 0);
    assert_eq!(left.1, 0);
    assert_eq!(left.2, 0, "the uploaded file was released");
}

#[tokio::test]
async fn voice_message_details_reach_the_recipient_and_only_the_uploader_may_set_them() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let ch = s.channel("voice", &[&alice, &bob]).await;
    let att = alice.upload_as("voice-message-1.wav", "audio/wav", None, vec![0u8; 2000]).await.unwrap();
    // Duration and a 64-value waveform, set by the uploader before the message is sent.
    let peaks: Vec<u8> = (0..64).map(|i| (i * 4) as u8).collect();
    let _: Value = alice.put(&format!("/files/{}/meta", att.id), &AttachmentMeta { duration_ms: Some(1000), peaks: peaks.clone(), ..Default::default() }).await.unwrap();
    alice.send_with_files(&ch, "", &[att.id.clone()]).await.unwrap();
    let m = bob.history(&ch).await.into_iter().find(|m| !m.attachments.is_empty()).unwrap();
    let meta = m.attachments[0].meta.clone().expect("metadata reached the recipient");
    assert_eq!(meta.duration_ms, Some(1000));
    assert_eq!(meta.peaks, peaks);
    // Only the uploader may set details.
    let r: Res<Value> = bob.put(&format!("/files/{}/meta", att.id), &AttachmentMeta { duration_ms: Some(5), ..Default::default() }).await;
    assert!(r.is_err());
    // Oversized or invalid details are refused.
    let big = AttachmentMeta { poster_b64: Some("x".repeat(60 * 1024)), ..Default::default() };
    assert!(alice.put::<Value, _>(&format!("/files/{}/meta", att.id), &big).await.is_err());
    let too_many_peaks = AttachmentMeta { peaks: vec![1; 65], ..Default::default() };
    assert!(alice.put::<Value, _>(&format!("/files/{}/meta", att.id), &too_many_peaks).await.is_err());
    // A video poster frame and size are kept as they are.
    let video = alice.upload_as("clip.mp4", "video/mp4", None, vec![0u8; 500]).await.unwrap();
    let poster = "A".repeat(300);
    let _: Value = alice.put(&format!("/files/{}/meta", video.id), &AttachmentMeta { duration_ms: Some(3000), width: Some(480), height: Some(270), poster_b64: Some(poster.clone()), ..Default::default() }).await.unwrap();
    alice.send_with_files(&ch, "clip", &[video.id.clone()]).await.unwrap();
    let m = bob.history(&ch).await.into_iter().find(|m| m.body == "clip").unwrap();
    let meta = m.attachments[0].meta.clone().unwrap();
    assert_eq!((meta.duration_ms, meta.width, meta.height, meta.poster_b64), (Some(3000), Some(480), Some(270), Some(poster)));
}

#[tokio::test]
async fn sidebar_order_of_conversations_and_sections_is_stored_per_user() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let c1 = s.channel("alpha", &[&alice, &bob]).await;
    let c2 = s.channel("beta", &[&alice, &bob]).await;
    let c3 = s.channel("gamma", &[&alice, &bob]).await;
    let positions = |c: Client| async move {
        let prefs: Vec<ConvPref> = c.get("/me/prefs").await.unwrap();
        let mut v: Vec<(i64, String)> = prefs.into_iter().filter_map(|p| p.position.map(|x| (x, p.conv_id))).collect();
        v.sort();
        v.into_iter().map(|(_, id)| id).collect::<Vec<_>>()
    };
    assert!(positions(alice.clone()).await.is_empty(), "no manual order by default");

    // Conversations: a hand-made order, per user, announced to the user's other devices.
    let mut ws = alice.ws(alice.cursor().await).await;
    let _: Vec<ConvPref> = alice.post("/conversations/order", &ConvOrderRequest { conv_ids: vec![c3.clone(), c1.clone(), c2.clone()] }).await.unwrap();
    assert_eq!(positions(alice.clone()).await, vec![c3.clone(), c1.clone(), c2.clone()]);
    ws.wait_event("pref update", |e| matches!(e, EventData::PrefUpdate { pref } if pref.conv_id == c3 && pref.position == Some(0))).await;
    assert!(positions(bob.clone()).await.is_empty(), "another user's order is not affected");
    // Unknown or foreign conversations are refused.
    assert!(alice.post::<Vec<ConvPref>, _>("/conversations/order", &ConvOrderRequest { conv_ids: vec!["nope".into()] }).await.is_err());
    // Reset: an empty list removes every manual order.
    let _: Vec<ConvPref> = alice.post("/conversations/order", &ConvOrderRequest { conv_ids: vec![] }).await.unwrap();
    assert!(positions(alice.clone()).await.is_empty());

    // Sections: default listing, then a custom order including the built-in sections.
    let created: Vec<Section> = alice.post("/sections", &SectionRequest { name: Some("Projects".into()), ..Default::default() }).await.unwrap();
    let a = created.iter().find(|x| x.name == "Projects").unwrap().id.clone();
    let created: Vec<Section> = alice.post("/sections", &SectionRequest { name: Some("Later".into()), ..Default::default() }).await.unwrap();
    let b = created.iter().find(|x| x.name == "Later").unwrap().id.clone();
    let key = |x: &Section| x.builtin_key().map(str::to_string).unwrap_or_else(|| x.id.clone());
    let order_of = |mut v: Vec<Section>| {
        v.sort_by_key(|x| x.position);
        v.iter().map(key).collect::<Vec<_>>()
    };
    let want_ids = vec!["builtin:dms".to_string(), b.clone(), "builtin:fav".into(), "builtin:channels".into(), a.clone()];
    let got: Vec<Section> = alice.post("/sections/order", &SectionOrderRequest { ids: want_ids }).await.unwrap();
    assert_eq!(order_of(got), vec!["dms".to_string(), b.clone(), "fav".into(), "channels".into(), a.clone()]);
    let listed: Vec<Section> = alice.get("/sections").await.unwrap();
    assert_eq!(order_of(listed), vec!["dms".to_string(), b.clone(), "fav".into(), "channels".into(), a.clone()]);
    // Bob has none of it.
    let bobs: Vec<Section> = bob.get("/sections").await.unwrap();
    assert!(bobs.iter().all(|x| x.builtin_key().is_some() || x.name != "Projects"));
    // Bad keys are refused; an empty list puts the built-in sections back.
    assert!(alice.post::<Vec<Section>, _>("/sections/order", &SectionOrderRequest { ids: vec!["builtin:nonsense".into()] }).await.is_err());
    let got: Vec<Section> = alice.post("/sections/order", &SectionOrderRequest { ids: vec![] }).await.unwrap();
    assert!(got.iter().all(|x| x.builtin_key().is_none()), "the order-only rows of the built-in sections are gone: {got:?}");
    assert_eq!(order_of(got), vec![b.clone(), a.clone()], "custom sections keep their relative order");
    // Renaming, collapsing and deleting a section.
    let upd: Vec<Section> = alice.patch(&format!("/sections/{a}"), &SectionRequest { name: Some("Renamed".into()), collapsed: Some(true), ..Default::default() }).await.unwrap();
    assert!(upd.iter().any(|x| x.id == a && x.name == "Renamed" && x.collapsed));
    let after: Vec<Section> = alice.delete(&format!("/sections/{a}")).await.unwrap();
    assert!(after.iter().all(|x| x.id != a));
}

fn tiny_pdf() -> Vec<u8> {
    let content = "0.1 0.4 0.8 rg 40 600 300 100 re f BT /F1 24 Tf 50 500 Td (Hello preview) Tj ET";
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 800] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}\nendstream", content.len(), content),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offs = vec![];
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend(format!("{} 0 obj\n{}\nendobj\n", i + 1, o).as_bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", objs.len() + 1, xref).as_bytes());
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn pdf_gets_a_server_drawn_first_page_preview_and_broken_pdfs_fall_back_to_a_card() {
    let s = TestServer::start().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let ch = s.channel("docs", &[&alice, &bob]).await;
    let good = alice.send_file(&ch, "good", "report.pdf", tiny_pdf()).await.attachments[0].clone();
    let bad = alice.send_file(&ch, "bad", "broken.pdf", b"%PDF-1.4 this is not a real document".to_vec()).await.attachments[0].clone();
    // A preview that every member can fetch.
    let r = bob.raw(HttpMethod::GET, &format!("/files/{}/thumb", good.id), None).await;
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(&r.body[1..4], b"PNG");
    assert!(r.body.len() > 2000, "a drawn page, not a blank");
    // The document that cannot be drawn: no preview, and the file itself is still downloadable.
    for _ in 0..2 {
        assert!(bob.raw(HttpMethod::GET, &format!("/files/{}/thumb", bad.id), None).await.status >= 400);
    }
    assert!(bob.download(&bad.id).await.is_ok());
    // Someone outside the channel cannot get the preview.
    assert!(carol.raw(HttpMethod::GET, &format!("/files/{}/thumb", good.id), None).await.status >= 400);
    // A removed file has no preview; an unknown one neither.
    s.admin.remove_file(&good.id).await.unwrap();
    assert!(alice.raw(HttpMethod::GET, &format!("/files/{}/thumb", good.id), None).await.status >= 400);
    assert!(alice.raw(HttpMethod::GET, &format!("/files/{}x/thumb", good.id), None).await.status >= 400);
}
