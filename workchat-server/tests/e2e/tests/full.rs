//! End-to-end run against the real server image in docker, MinIO (S3) and a fake Google Drive, driven over raw HTTP.
//!
//! Run with `tests/e2e/run.sh` (builds the image and sets WORKCHAT_E2E=1). Without WORKCHAT_E2E the test is skipped so
//! `cargo test --workspace` needs no docker.
//!
//! Covers: setup with S3 storage chosen in the wizard, login, DM, channel, the edit rule, delete tombstone,
//! upload/download through MinIO, sync after being offline, search filters and isolation, threads, membership,
//! saved items and labels, storage move + verify (S3 -> Google Drive -> S3), a content-free ntfy wake-up, an encrypted
//! full backup to MinIO restored onto a second EMPTY server container (chats and attachments come back), bulk delete
//! with restore, date-range delete with selective restore, the disabled-account kick and clear history with the
//! automatic backup. Server data lives in MinIO / the fake Drive throughout.
mod fake_gdrive;

use proto::*;
use serde_json::{json, Value};
use std::time::Duration;
use workchat_e2e::{enabled, sh, wait_http, Cleanup};
use workchat_testkit::{eventually, now_ms, uid, Client};

const SERVER_PORT: u16 = 18300;
const SERVER2_PORT: u16 = 18301;
const MINIO_PORT: u16 = 18310;
const IMAGE: &str = "workchat-server:e2e";
const MINIO_IMAGE: &str = "bitnamilegacy/minio:latest";
const NET: &str = "workchat-e2e";
const PW: &str = "password-123";

fn minio_files() -> usize {
    let (_, out) = sh("docker", &["exec", "workchat-e2e-minio", "sh", "-c", "find /data/workchat -name xl.meta | wc -l"]);
    out.trim().parse().unwrap_or(0)
}

async fn member(admin: &Client, url: &str, name: &str) -> Client {
    admin.create_user(name, name, Some(PW)).await.unwrap();
    Client::login_device(url, name, PW, &format!("e2e-{name}")).await.unwrap()
}

async fn has_text(c: &Client, conv: &str, text: &str) -> bool {
    c.texts(conv).await.iter().any(|t| t == text)
}

async fn search(c: &Client, query: &str) -> SearchResponse {
    c.get(&format!("/search?{query}")).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_scenario() {
    if !enabled() {
        eprintln!("e2e skipped: run tests/e2e/run.sh (needs docker)");
        return;
    }
    let _guard = Cleanup { containers: vec!["workchat-e2e-minio".into(), "workchat-e2e-server".into(), "workchat-e2e-server2".into()], network: Some(NET.into()) };
    let _ = sh("docker", &["rm", "-f", "-v", "workchat-e2e-minio", "workchat-e2e-server", "workchat-e2e-server2"]);
    let _ = sh("docker", &["network", "rm", NET]);
    let (ok, out) = sh("docker", &["network", "create", NET]);
    assert!(ok, "network: {out}");

    // --- infrastructure -----------------------------------------------------------------------------
    let (ok, out) = sh(
        "docker",
        &[
            "run", "-d", "--name", "workchat-e2e-minio", "--network", NET, "--user", "0", "-p", &format!("127.0.0.1:{MINIO_PORT}:9000"),
            "-e", "MINIO_ROOT_USER=e2eaccess", "-e", "MINIO_ROOT_PASSWORD=e2esecret-1234",
            "--entrypoint", "sh", MINIO_IMAGE, "-c", "mkdir -p /data/workchat && exec /opt/bitnami/minio/bin/minio server /data",
        ],
    );
    assert!(ok, "minio failed to start: {out}");
    let (gdrive, gport) = fake_gdrive::start().await;
    let public = format!("http://127.0.0.1:{SERVER_PORT}");
    let (ok, out) = sh(
        "docker",
        &[
            "run", "-d", "--name", "workchat-e2e-server", "--network", NET, "--add-host", "host.docker.internal:host-gateway",
            "-p", &format!("127.0.0.1:{SERVER_PORT}:8080"), "-e", &format!("WORKCHAT_PUBLIC_URL={public}"),
            "-e", "WORKCHAT_LOCKOUT_THRESHOLD=50", IMAGE,
        ],
    );
    assert!(ok, "server failed to start (build the image first): {out}");
    wait_http(&format!("{public}/api/v1/health")).await;
    wait_http(&format!("http://127.0.0.1:{MINIO_PORT}/minio/health/live")).await;

    let s3 = StorageConfig::S3 {
        provider: S3Provider::Minio,
        endpoint: Some("http://workchat-e2e-minio:9000".to_string()),
        region: "us-east-1".into(),
        bucket: "workchat".into(),
        access_key_id: "e2eaccess".into(),
        secret_access_key: "e2esecret-1234".into(),
        prefix: None,
        path_style: true,
        allow_http: true,
    };
    let gd = StorageConfig::Gdrive {
        client_id: "cid".into(),
        client_secret: "csecret".into(),
        refresh_token: "fake-refresh-token".into(),
        folder_name: Some("WorkChat e2e".into()),
        api_base: Some(format!("http://host.docker.internal:{gport}")),
        token_url: Some(format!("http://host.docker.internal:{gport}/token")),
    };

    // --- first-run setup: owner + S3 storage chosen in the wizard ---------------------------------------
    let admin = Client::setup(
        &public,
        &SetupRequest { username: "admin".into(), display_name: "Admin".into(), password: "test-admin-pw".into(), email: None, storage: Some(s3.clone()) },
    )
    .await
    .expect("setup with MinIO storage");
    let t = admin.test_storage(&s3).await.unwrap();
    assert!(t.ok, "{}", t.message);
    assert_eq!(admin.storage().await.unwrap().config.kind(), "s3");
    // A second setup is refused.
    assert!(Client::setup(&public, &SetupRequest { username: "x".into(), display_name: "x".into(), password: PW.into(), email: None, storage: None }).await.is_err());

    // --- login / users --------------------------------------------------------------------------------------
    let alice = member(&admin, &public, "alice").await;
    let bob = member(&admin, &public, "bob").await;
    let carol = member(&admin, &public, "carol").await;
    let (a_id, b_id, c_id) = (alice.user_id.clone(), bob.user_id.clone(), carol.user_id.clone());
    // Wrong password is rejected with a stable code.
    assert_eq!(Client::login(&public, "alice", "nope-nope").await.err().unwrap().code, Some(ErrorCode::InvalidCredentials));

    // --- DM + channel + edit rule + tombstone -------------------------------------------------------------------
    let ab = alice.open_dm(&b_id).await.unwrap();
    let first = alice.send(&ab, "hello bob").await;
    assert_eq!(bob.texts(&ab).await, vec!["hello bob"]);
    alice.edit(&first.id, "hello bob!").await.unwrap();
    assert_eq!(bob.texts(&ab).await, vec!["hello bob!"]);
    bob.send(&ab, "hi alice").await;
    assert_eq!(alice.texts(&ab).await.len(), 2);
    assert_eq!(alice.edit(&first.id, "too late").await.unwrap_err().code, Some(ErrorCode::EditWindowClosed), "DM edit closes after the other side replies");

    let ch = admin.create_channel("project", &[a_id.clone(), b_id.clone()]).await.unwrap().conversation.id;
    assert!(alice.has_conversation(&ch).await && bob.has_conversation(&ch).await);
    assert!(!carol.has_conversation(&ch).await, "channels are private");
    let kick = alice.send(&ch, "kickoff at 10").await;
    let ack = bob.send(&ch, "ack").await;
    alice.edit(&kick.id, "kickoff at 11").await.expect("channel messages stay editable");
    assert!(has_text(&bob, &ch, "kickoff at 11").await);
    bob.delete_message(&ack.id).await.unwrap();
    assert!(alice.history(&ch).await.iter().any(|m| m.deleted && m.id == ack.id), "tombstone");

    // --- upload / download through MinIO ----------------------------------------------------------------------------
    let data: Vec<u8> = (0..700_000u32).map(|i| (i % 253) as u8).collect();
    let report = alice.send_file(&ab, "the report", "report.pdf", data.clone()).await;
    let att = bob.history(&ab).await.into_iter().find(|m| !m.attachments.is_empty()).expect("bob has the file").attachments[0].clone();
    assert_eq!(att.id, report.attachments[0].id);
    assert_eq!(att.size, data.len() as i64);
    assert_eq!(bob.download(&att.id).await.unwrap(), data);
    assert_eq!(carol.download(&att.id).await.unwrap_err().code, Some(ErrorCode::NotFound), "non-members cannot fetch files");
    assert!(minio_files() >= 1, "the file is physically in MinIO");
    // A second, private file only alice sees (notes to self) for the clear-history step later.
    let me_conv = alice.self_conv().await;
    alice.send_file(&me_conv, "", "private.bin", vec![9u8; 4096]).await;

    // --- sync after offline: the event log replays everything bob missed ---------------------------------------------------
    let bob_cursor = bob.cursor().await;
    for i in 1..=5 {
        alice.send(&ch, &format!("offline-{i}")).await;
    }
    let missed = bob.events(bob_cursor).await.unwrap();
    let n = missed.events.iter().filter(|e| matches!(&e.event, EventData::MessageNew { message } if message.body.starts_with("offline-"))).count();
    assert_eq!(n, 5, "bob catches up by cursor");
    // And the live socket delivers the backlog too.
    let mut ws = bob.ws(bob_cursor).await;
    ws.wait_event("backlog over the socket", |e| matches!(e, EventData::MessageNew { message } if message.body == "offline-5")).await;
    drop(ws);

    // --- search isolation --------------------------------------------------------------------------------------------------
    alice.send(&ab, "the zebra plan").await;
    assert_eq!(alice.search("zebra").await.unwrap().hits.len(), 1);
    assert_eq!(bob.search("zebra").await.unwrap().hits.len(), 1);
    assert_eq!(carol.search("zebra").await.unwrap().hits.len(), 0, "carol shares no chat with alice and bob");

    // --- storage move + verify: S3 -> Google Drive (fake) -> S3 ---------------------------------------------------------------
    let before = admin.storage().await.unwrap();
    assert!(before.attachments >= 2);
    let t = admin.test_storage(&gd).await.unwrap();
    assert!(t.ok, "gdrive test: {}", t.message);
    let job = admin.migrate_storage(&gd, true).await.unwrap();
    let job = admin.wait_job(&job.id).await.unwrap();
    assert_eq!(job.state, "done", "{job:?}");
    assert_eq!(admin.storage().await.unwrap().config.kind(), "gdrive");
    assert!(gdrive.count_files() as i64 >= before.attachments, "files landed in the fake Drive: {:?}", gdrive.names());
    assert_eq!(minio_files(), 0, "source copies were removed after verification");
    let v = admin.verify_storage(true).await.unwrap();
    assert_eq!(admin.wait_job(&v.id).await.unwrap().state, "done");
    // Downloads still work, now served from Drive.
    assert_eq!(bob.download(&att.id).await.unwrap(), data);
    bob.send(&ab, "after the move").await;
    let job = admin.migrate_storage(&s3, true).await.unwrap();
    let job = admin.wait_job(&job.id).await.unwrap();
    assert_eq!(job.state, "done", "{job:?}");
    assert_eq!(gdrive.count_files(), 0, "Drive emptied after moving back");
    assert!(minio_files() >= 2);
    assert_eq!(bob.download(&att.id).await.unwrap(), data);

    // --- sign-out: members without "Privacy options: Allow" cannot, with Allow they can ---------------------------------------
    let scratch = Client::login_device(&public, "carol", PW, "e2e-carol-2").await.unwrap();
    assert!(scratch.logout().await.is_err(), "logout must be refused without Allow");
    admin.update_user(&c_id, &UpdateUserRequest { privacy_controls_enabled: Some(true), ..Default::default() }).await.unwrap();
    scratch.logout().await.expect("sign-out with Allow");
    admin.update_user(&c_id, &UpdateUserRequest { privacy_controls_enabled: Some(false), ..Default::default() }).await.unwrap();

    phase2(&admin, &s3, (&a_id, &alice), (&b_id, &bob), &carol, &ch, &ab, &data).await;

    // --- disabled account: kicked immediately ------------------------------------------------------------------------------------
    let mut ws = carol.ws(carol.cursor().await).await;
    admin.set_disabled(&c_id, true).await.unwrap();
    let (code, _) = ws.wait_kicked().await;
    assert_eq!(code, ErrorCode::AccountDisabled);
    assert_eq!(Client::login(&public, "carol", PW).await.err().unwrap().code, Some(ErrorCode::AccountDisabled));
    assert!(carol.me().await.is_err(), "the old token is dead");
    admin.set_disabled(&c_id, false).await.unwrap();

    // --- clear history: automatic backup first, own copy hidden, others untouched, files moved ------------------------------------------
    let n_backups = admin.backups().await.unwrap().len();
    let mut ws = alice.ws(alice.cursor().await).await;
    let r = admin.clear_history(&a_id).await.unwrap();
    assert!(r.messages_hidden > 0);
    assert!(r.attachments_moved >= 1, "the private file moved into the backup folder");
    let backups = admin.backups().await.unwrap();
    assert_eq!(backups.len(), n_backups + 1, "a backup was written first");
    assert!(backups.iter().any(|b| b.key == r.backup_key));
    ws.wait_kicked().await;
    assert!(alice.me().await.is_err(), "alice was signed out");
    // Bob keeps his copy of the DM and the channel.
    assert!(has_text(&bob, &ab, "hello bob!").await);
    assert!(has_text(&bob, &ch, "kickoff at 11").await);
    // Alice signs in again: her history is empty, but the DM shows only new messages.
    let alice = Client::login(&public, "alice", PW).await.unwrap();
    for c in alice.conversations().await.unwrap() {
        assert!(alice.texts(&c.conversation.id).await.is_empty(), "nothing old in {}", c.conversation.id);
    }
    assert_eq!(alice.search("zebra").await.unwrap().hits.len(), 0);
    bob.send(&ab, "fresh after clear").await;
    assert_eq!(alice.texts(&ab).await, vec!["fresh after clear"]);
}

/// A minimal fake ntfy server: records every request line and body, answers 200.
async fn fake_ntfy() -> (std::sync::Arc<std::sync::Mutex<Vec<String>>>, u16) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let log: std::sync::Arc<std::sync::Mutex<Vec<String>>> = Default::default();
    let l = tokio::net::TcpListener::bind("0.0.0.0:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let log2 = log.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = l.accept().await else { return };
            let log = log2.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let mut got = 0;
                // Read until the headers and the 4-byte "wake" body have arrived (or the peer pauses).
                while let Ok(Ok(n)) = tokio::time::timeout(Duration::from_millis(300), sock.read(&mut buf[got..])).await {
                    if n == 0 {
                        break;
                    }
                    got += n;
                    if String::from_utf8_lossy(&buf[..got]).contains("\r\n\r\nwake") {
                        break;
                    }
                }
                log.lock().unwrap().push(String::from_utf8_lossy(&buf[..got]).to_string());
                let _ = sock.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}").await;
            });
        }
    });
    (log, port)
}

#[allow(clippy::too_many_arguments)]
async fn phase2(
    admin: &Client,
    s3: &StorageConfig,
    (a_id, alice): (&String, &Client),
    (b_id, bob): (&String, &Client),
    carol: &Client,
    ch: &str,
    ab: &str,
    data: &[u8],
) {
    // --- search filters + isolation -------------------------------------------------------------------------
    alice.send(ch, "budget review http://example.com/q4").await;
    bob.send(ch, "budget numbers").await;
    assert_eq!(search(alice, "q=budget").await.hits.len(), 2);
    assert_eq!(search(alice, "q=budget&from=bob").await.hits.len(), 1);
    assert_eq!(search(alice, &format!("q=budget&has=link&in={ch}")).await.hits.len(), 1);
    assert_eq!(search(alice, "q=budget&before=946684800000").await.hits.len(), 0, "nothing before the year 2000");
    assert_eq!(search(carol, "q=budget").await.hits.len(), 0, "carol is not in #project");
    assert!(!search(alice, "q=report").await.files.is_empty(), "file names are searchable");
    assert!(search(carol, "q=report").await.files.is_empty());

    // --- threads --------------------------------------------------------------------------------------------------
    let root = alice.history(ch).await.into_iter().find(|m| m.body.starts_with("budget review")).unwrap();
    bob.reply(ch, &root.id, "reply in thread").await;
    let threads: Vec<ThreadSummary> = alice.get("/threads").await.unwrap();
    assert!(threads.iter().any(|t| t.root.id == root.id && t.unread >= 1));
    let _: ThreadSummary = alice.delete(&format!("/threads/{}/follow", root.id)).await.unwrap();
    let threads: Vec<ThreadSummary> = alice.get("/threads").await.unwrap();
    assert!(threads.iter().all(|t| t.root.id != root.id), "unfollow removes it from the Threads list");

    // --- membership add/remove by an admin; saved items and labels stay private ------------------------------------
    let c_id = carol.user_id.clone();
    assert!(!carol.has_conversation(ch).await);
    admin.add_members(ch, std::slice::from_ref(&c_id)).await.unwrap();
    assert!(carol.has_conversation(ch).await);
    assert_eq!(carol.post::<Value, _>(&format!("/conversations/{ch}/members"), &json!({"user_ids": [a_id]})).await.unwrap_err().status, 403, "a plain member cannot manage people");
    admin.remove_member(ch, &c_id).await.unwrap();
    assert!(!carol.has_conversation(ch).await);
    let keep = alice.history(ch).await.into_iter().find(|m| m.body.starts_with("budget numbers")).unwrap();
    let labels: Vec<Label> = alice.post("/labels", &LabelRequest { name: Some("Follow up".into()), color: Some("#e8590c".into()), position: None }).await.unwrap();
    let label = labels.into_iter().find(|l| l.name == "Follow up").unwrap();
    let _: SavedItem = alice.put(&format!("/saved/{}", keep.id), &SaveRequest { label_ids: None }).await.unwrap();
    let _: SavedItem = alice.put(&format!("/saved/{}/labels", keep.id), &SavedLabelsRequest { label_ids: vec![label.id.clone()] }).await.unwrap();
    let saved: Vec<SavedItem> = alice.get("/saved").await.unwrap();
    assert!(saved.iter().any(|v| v.message_id == keep.id && v.label_ids == vec![label.id.clone()]));
    assert!(bob.get::<Vec<SavedItem>>("/saved").await.unwrap().is_empty() && bob.get::<Vec<Label>>("/labels").await.unwrap().is_empty(), "saved items and labels are per user");
    assert!(carol.put::<SavedItem, _>(&format!("/saved/{}", keep.id), &SaveRequest { label_ids: None }).await.is_err(), "no saving messages from chats you are not in");
    let _: Value = alice.delete(&format!("/saved/{}", keep.id)).await.unwrap();
    assert!(alice.get::<Vec<SavedItem>>("/saved").await.unwrap().is_empty());

    // --- push: fake ntfy receives one content-free wake-up when bob is offline -------------------------------------
    let (ntfy_log, ntfy_port) = fake_ntfy().await;
    admin.put_push_settings(&PushSettings { ntfy_url: Some(format!("http://host.docker.internal:{ntfy_port}")), ..Default::default() }).await.unwrap();
    let _: PushRegistration = bob.put("/me/push", &PushRegistration { id: String::new(), kind: "ntfy".into(), target: "bob-e2e-topic".into(), ntfy_url: None, device_name: "e2e".into(), created_ms: 0 }).await.unwrap();
    alice.send(ab, "top secret wake test").await;
    eventually!("ntfy wake", !ntfy_log.lock().unwrap().is_empty());
    {
        let log = ntfy_log.lock().unwrap();
        assert!(log[0].starts_with("POST /bob-e2e-topic"), "{}", log[0]);
        assert!(log.iter().all(|r| !r.contains("secret") && !r.to_lowercase().contains("alice")), "push must be content free: {log:?}");
    }

    // --- encrypted full backup to MinIO, then restore onto a second EMPTY server -----------------------------------
    let mut backup_target = s3.clone();
    if let StorageConfig::S3 { prefix, .. } = &mut backup_target {
        *prefix = Some("e2e-backups".into());
    }
    admin
        .put_backup_settings(&BackupSettingsUpdate { enabled: Some(true), interval_hours: Some(24), retention: Some(3), target: Some(backup_target.clone()), passphrase: Some("e2e backup passphrase".into()), ..Default::default() })
        .await
        .unwrap();
    assert!(admin.test_backup_target().await.unwrap().ok);
    let full = admin.backup_now().await.unwrap();
    assert!(full.encrypted && full.size > 0);
    // Objects in MinIO are encrypted: the plain-text message must not appear in any backup object.
    let (_, grep) = sh("docker", &["exec", "workchat-e2e-minio", "sh", "-c", "grep -rl 'top secret wake test' /data/workchat/e2e-backups 2>/dev/null | wc -l"]);
    assert_eq!(grep.trim(), "0", "backup objects contain no plain text");
    let public2 = format!("http://127.0.0.1:{SERVER2_PORT}");
    let (ok, out) = sh(
        "docker",
        &[
            "run", "-d", "--name", "workchat-e2e-server2", "--network", NET, "-p", &format!("127.0.0.1:{SERVER2_PORT}:8080"),
            "-e", &format!("WORKCHAT_PUBLIC_URL={public2}"), "-e", "WORKCHAT_LOCKOUT_THRESHOLD=50", IMAGE,
        ],
    );
    assert!(ok, "second server failed to start: {out}");
    wait_http(&format!("{public2}/api/v1/health")).await;
    let r = Client::fresh_restore(&public2, &FreshRestoreRequest { target: Some(backup_target.clone()), key: None, passphrase: "e2e backup passphrase".into() }).await.expect("fresh restore");
    assert!(r.users >= 4 && r.messages >= 10, "{r:?}");
    assert!(r.attachments >= 2 && r.attachments_resolvable == r.attachments, "restored server reconnects to the existing attachments: {r:?}");
    let admin2 = Client::login(&public2, "admin", "test-admin-pw").await.expect("owner login on the restored server");
    let alice2 = Client::login(&public2, "alice", PW).await.unwrap();
    assert!(has_text(&alice2, ab, "hello bob!").await, "old chat restored");
    let old = alice2.history(ab).await.into_iter().find(|m| !m.attachments.is_empty()).unwrap();
    assert_eq!(alice2.download(&old.attachments[0].id).await.unwrap(), data, "old attachment downloads through the new server");
    assert!(has_text(&alice2, ch, "kickoff at 11").await);

    // --- on the restored server: bulk delete (backup first), restore; date-range delete; selective restore -------------
    let bob2 = Client::login(&public2, "bob", PW).await.unwrap();
    assert!(has_text(&bob2, ab, "hello bob!").await);
    let target = alice2.history(ab).await.into_iter().find(|m| m.body == "hi alice").unwrap();
    let del = admin2
        .delete_messages(&AdminDeleteMessagesRequest { message_ids: vec![target.id.clone()], conv_id: None, from_ms: None, to_ms: None, before_ms: None, scope: "everyone".into(), user_id: None })
        .await
        .unwrap();
    assert!(alice2.history(ab).await.iter().any(|m| m.id == target.id && m.deleted), "tombstone");
    admin2.restore(&RestoreRequest { backup_id: del.backup_id, user_ids: vec![], all: false, mode: None }).await.unwrap();
    assert!(has_text(&alice2, ab, "hi alice").await, "restored message");
    // Date-range delete (before a date in the future = everything), default keeps the other copy.
    let dr = admin2
        .delete_range(b_id, &DateRangeDeleteRequest { from_ms: 0, to_ms: 0, before_ms: Some(now_ms() + 60_000), conversation_id: Some(ab.to_string()), also_other_copies: false })
        .await
        .unwrap();
    assert!(dr.messages_hidden >= 3, "{dr:?}");
    assert!(alice2.texts(ab).await.len() >= 3, "alice keeps her copy");
    // Selective restore of just bob from the user backup written before the delete.
    let ub = admin2.backups().await.unwrap().into_iter().filter(|b| b.kind == "user" && b.user_id.as_deref() == Some(b_id.as_str())).max_by_key(|b| b.created_ms).unwrap();
    let rr = admin2.restore(&RestoreRequest { backup_id: ub.id, user_ids: vec![b_id.clone()], all: false, mode: Some("merge".into()) }).await.unwrap();
    assert!(rr.messages_unhidden >= 3, "{rr:?}");
    let bob3 = Client::login(&public2, "bob", PW).await.unwrap();
    assert!(has_text(&bob3, ab, "hello bob!").await, "bob's history is back");
    let _ = (bob2, uid());
}
