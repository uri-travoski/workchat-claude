//! SV-20: encrypted full backups, fresh-server restore, schedule and retention, per-user backups,
//! selective restore (merge / replace) and admin deletion with a mandatory backup first.
mod common;
use common::*;
use std::path::Path;
use std::time::Duration;

const PASS: &str = "correct horse battery staple";

fn local(p: &Path) -> StorageConfig {
    StorageConfig::Local { path: Some(p.to_string_lossy().to_string()) }
}

/// A server whose attachments live in a directory that outlives the server's own data dir.
async fn server_with_shared_storage() -> (TestServer, std::path::PathBuf, std::path::PathBuf) {
    let s = TestServer::start().await;
    let att = s.dir.path().join("shared-attachments");
    let bk = s.dir.path().join("backup-target");
    std::fs::create_dir_all(&att).unwrap();
    std::fs::create_dir_all(&bk).unwrap();
    s.admin.set_storage(&local(&att)).await.unwrap();
    s.admin
        .put_backup_settings(&BackupSettingsUpdate { enabled: Some(false), target: Some(local(&bk)), passphrase: Some(PASS.into()), retention: Some(3), ..Default::default() })
        .await
        .unwrap();
    (s, att, bk)
}

async fn fresh_server(dir: &Path) -> (workchat_server::state::St, String) {
    let st = workchat_server::build_state(Config::for_test(dir.join("server2"))).await.unwrap();
    let url = workchat_server::serve_for_test(st.clone()).await.unwrap();
    (st, url)
}

fn all_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(all_files(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[tokio::test]
async fn full_backup_is_encrypted_and_restores_on_a_fresh_server_with_old_attachments() {
    let (s, att, bk) = server_with_shared_storage().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let ch = s.channel("general", &[&alice, &bob]).await;
    let root = alice.send_file(&ch, "plan attached", "plan.txt", b"the secret launch plan".to_vec()).await;
    let orig = bob.send(&ch, "thanks, looks good").await;
    alice.react(&orig.id, "🎉", true).await.unwrap();
    let _: ConvPref = alice.put(&format!("/conversations/{ch}/prefs"), &ConvPrefRequest { favourite: Some(true), ..Default::default() }).await.unwrap();
    bob.reply(&ch, &root.id, "a threaded note").await;
    assert_eq!(alice.thread(&ch, &root.id).await.unwrap().replies.len(), 1);

    // Backup now: encrypted at rest, with pointer objects for a fresh restore.
    let info = s.admin.backup_now().await.unwrap();
    assert!(info.encrypted && info.kind == "full" && info.key.ends_with(".db.age"), "{info:?}");
    let blob = std::fs::read(bk.join(&info.key)).unwrap();
    assert!(!blob.windows(15).any(|w| w == b"SQLite format 3"), "backup must not be a plain SQLite file");
    assert!(!String::from_utf8_lossy(&blob).contains("thanks, looks good"));
    assert!(bk.join("backups/full/latest.json").exists() && bk.join("backups/full/index.json").exists());
    let st = s.admin.backup_settings().await.unwrap();
    assert!(st.passphrase_set && st.last_run_ms.is_some());
    assert!(s.admin.backups().await.unwrap().iter().any(|b| b.id == info.id));
    // The backup never contains the passphrase.
    assert!(!all_files(&bk).iter().any(|p| std::fs::read(p).map(|b| String::from_utf8_lossy(&b).contains(PASS)).unwrap_or(false)));

    // A brand new, empty server.
    let (st2, url2) = fresh_server(s.dir.path()).await;
    let meta: ServerMeta = Client::anon(&url2).get("/meta").await.unwrap();
    assert!(meta.setup_required);
    // Wrong passphrase is refused and leaves the server empty.
    let bad = Client::fresh_restore(&url2, &FreshRestoreRequest { target: Some(local(&bk)), key: None, passphrase: "wrong".into() }).await;
    assert!(bad.is_err());
    let r = Client::fresh_restore(&url2, &FreshRestoreRequest { target: Some(local(&bk)), key: None, passphrase: PASS.into() }).await.unwrap();
    assert!(r.users >= 3 && r.conversations >= 2 && r.messages >= 3, "{r:?}");
    assert!(r.attachments >= 1 && r.attachments_resolvable == r.attachments, "{r:?}");
    assert!(st2.db.call(|c| Ok(c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get::<_, i64>(0))?)).await.unwrap() >= 3);
    // Restoring again (server no longer fresh) is refused.
    let again = Client::fresh_restore(&url2, &FreshRestoreRequest { target: Some(local(&bk)), key: None, passphrase: PASS.into() }).await;
    assert_eq!(again.unwrap_err().code, Some(ErrorCode::SetupDone));
    // And the original server refuses too.
    assert!(Client::fresh_restore(&s.url, &FreshRestoreRequest { target: Some(local(&bk)), key: None, passphrase: PASS.into() }).await.is_err());

    // Old chats, reactions, threads, prefs and attachments are all there for the same logins.
    let alice2 = Client::login(&url2, "alice", PW).await.unwrap();
    let texts = alice2.texts(&ch).await;
    assert!(texts.contains(&"thanks, looks good".to_string()), "{texts:?}");
    let m = alice2.history(&ch).await.into_iter().find(|m| m.body == "plan attached").unwrap();
    assert_eq!(alice2.download(&m.attachments[0].id).await.unwrap(), b"the secret launch plan");
    let prefs: Vec<ConvPref> = alice2.get("/me/prefs").await.unwrap();
    assert!(prefs.iter().any(|p| p.conv_id == ch && p.favourite));
    let thanks = alice2.history(&ch).await.into_iter().find(|m| m.body == "thanks, looks good").unwrap();
    assert_eq!(thanks.reactions.len(), 1);
    assert_eq!(alice2.thread(&ch, &m.id).await.unwrap().replies.len(), 1);
    // The attachment files were not copied: the new server reads the same shared directory.
    assert!(!all_files(&att).is_empty());
    // The admin account works too, and the new server can take new messages.
    let admin2 = Client::login(&url2, "admin", "test-admin-pw").await.unwrap();
    assert!(admin2.users().await.unwrap().len() >= 3);
    alice2.send(&ch, "back online on the new server").await;

    // A device that was signed in to the old server has to sign in again (sessions are not part of a restore);
    // an old event cursor of the old server is meaningless on the new one but the log is consistent after a fresh bootstrap.
    assert!(bob.me().await.is_ok(), "the old server is unaffected");
    let bob2 = Client::login(&url2, "bob", PW).await.unwrap();
    let b = bob2.bootstrap().await.unwrap();
    assert!(b.messages[&ch].iter().any(|m| m.body == "back online on the new server"));
    assert!(b.messages[&ch].iter().any(|m| m.body == "thanks, looks good"));
    assert!(bob2.events(b.cursor).await.is_ok());
}

#[tokio::test]
async fn schedule_runs_when_due_and_retention_prunes_old_backups() {
    let (s, _att, bk) = server_with_shared_storage().await;
    s.admin
        .put_backup_settings(&BackupSettingsUpdate { enabled: Some(true), interval_hours: Some(24), retention: Some(2), ..Default::default() })
        .await
        .unwrap();
    let _alice = s.user("alice").await;
    let now = now_ms();
    let day = 24 * 3_600_000;
    // Nothing yet: due immediately.
    assert!(workchat_server::fullbackup::scheduled_tick(&s.st, now).await.unwrap());
    // Not due again until 24 h have passed.
    assert!(!workchat_server::fullbackup::scheduled_tick(&s.st, now + 3_600_000).await.unwrap());
    // The backup records its real finish time as last_run, so the simulated clock needs a margin larger
    // than however long a (slow CI) backup takes: 10 minutes on top of each simulated day.
    let margin = 600_000;
    for i in 1..=3 {
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(workchat_server::fullbackup::scheduled_tick(&s.st, now + i * day + margin).await.unwrap(), "run {i}");
    }
    let fulls: Vec<_> = s.admin.backups().await.unwrap().into_iter().filter(|b| b.kind == "full").collect();
    assert_eq!(fulls.len(), 2, "retention keeps the two newest");
    let on_disk = all_files(&bk.join("backups/full")).into_iter().filter(|p| p.to_string_lossy().ends_with(".db.age")).count();
    assert_eq!(on_disk, 2, "pruned backups are deleted from the target");
    // Disabled schedule does nothing.
    s.admin.put_backup_settings(&BackupSettingsUpdate { enabled: Some(false), ..Default::default() }).await.unwrap();
    assert!(!workchat_server::fullbackup::scheduled_tick(&s.st, now + 10 * day).await.unwrap());
}

#[tokio::test]
async fn backup_target_must_work_and_passphrase_is_required() {
    let s = TestServer::start().await;
    // No passphrase yet: refused with a clear message.
    let e = s.admin.backup_now().await.unwrap_err();
    assert_eq!(e.code, Some(ErrorCode::Validation));
    s.admin.put_backup_settings(&BackupSettingsUpdate { passphrase: Some(PASS.into()), ..Default::default() }).await.unwrap();
    assert!(s.admin.test_backup_target().await.unwrap().ok);
    // Short passphrases are refused.
    assert!(s.admin.put_backup_settings(&BackupSettingsUpdate { passphrase: Some("abc".into()), ..Default::default() }).await.is_err());
    // A broken target makes the backup fail loudly and leaves no record.
    let bad = StorageConfig::Local { path: Some("/proc/definitely/not/writable".into()) };
    s.admin.put_backup_settings(&BackupSettingsUpdate { target: Some(bad), ..Default::default() }).await.unwrap();
    let e = s.admin.backup_now().await.unwrap_err();
    assert!(matches!(e.code, Some(ErrorCode::BackupFailed) | Some(ErrorCode::StorageError)), "{e:?}");
    assert!(s.admin.backups().await.unwrap().iter().all(|b| b.kind != "full"));
    // Backups are for admins only.
    let m = s.user("member").await;
    assert_eq!(m.backups().await.unwrap_err().code, Some(ErrorCode::Forbidden));
    assert!(m.backup_now().await.is_err());
}

#[tokio::test]
async fn per_user_backup_is_encrypted_and_selective_restore_merges_or_replaces() {
    let (s, att, _bk) = server_with_shared_storage().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let carol = s.user("carol").await;
    let (a_id, b_id, c_id) = (alice.user_id.clone(), bob.user_id.clone(), carol.user_id.clone());
    let dm = alice.open_dm(&b_id).await.unwrap();
    alice.send_file(&dm, "old one with file", "note.txt", b"file in a dm".to_vec()).await;
    bob.send(&dm, "old two").await;
    let cdm = carol.open_dm(&b_id).await.unwrap();
    carol.send(&cdm, "carol to bob").await;

    // Alice's history is cleared: she loses her copy, bob keeps his. A backup was made first.
    let user_backup = s.admin.backup_user(&a_id).await.unwrap();
    assert!(user_backup.encrypted && user_backup.key.ends_with("data.json.age"), "{user_backup:?}");
    s.admin.clear_history(&a_id).await.unwrap();
    // Clearing history signs the account out of its sessions; sign in again.
    assert_eq!(alice.me().await.unwrap_err().code, Some(ErrorCode::HistoryCleared));
    let alice = s.login("alice").await;
    assert!(alice.texts(&dm).await.is_empty());
    assert!(bob.texts(&dm).await.contains(&"old two".to_string()), "bob keeps his copy");
    let backup_id = s.admin.backups().await.unwrap().into_iter().filter(|b| b.kind == "user" && b.user_id.as_deref() == Some(a_id.as_str())).max_by_key(|b| b.created_ms).unwrap().id;

    // Merge restore for alice only: her messages return, the file is back in the live area.
    let r = s.admin.restore(&RestoreRequest { backup_id: backup_id.clone(), user_ids: vec![a_id.clone()], all: false, mode: None }).await.unwrap();
    assert!(r.messages_unhidden + r.messages_added >= 2, "{r:?}");
    assert_eq!(alice.texts(&dm).await.len(), 2);
    let m = alice.history(&dm).await.into_iter().find(|m| m.body == "old one with file").unwrap();
    assert_eq!(alice.download(&m.attachments[0].id).await.unwrap(), b"file in a dm");
    assert!(!all_files(&att).iter().any(|p| p.to_string_lossy().contains("/backups/") && p.to_string_lossy().contains("/att/")), "files moved back out of the backup folder");
    // Bob was untouched and carol never involved.
    assert_eq!(bob.texts(&dm).await.len(), 2);
    assert!(carol.texts(&cdm).await.contains(&"carol to bob".to_string()));
    // Choosing nobody is a mistake, a wrong user is refused, a bad mode is refused.
    assert!(s.admin.restore(&RestoreRequest { backup_id: backup_id.clone(), user_ids: vec![], all: false, mode: None }).await.is_err());
    assert!(s.admin.restore(&RestoreRequest { backup_id: backup_id.clone(), user_ids: vec![c_id.clone()], all: false, mode: None }).await.is_err());
    assert!(s.admin.restore(&RestoreRequest { backup_id: backup_id.clone(), user_ids: vec![a_id.clone()], all: false, mode: Some("bogus".into()) }).await.is_err());

    // Replace mode: what alice has now but the backup does not know is hidden for her only.
    alice.send(&dm, "written after the backup").await;
    // merge never removes it...
    s.admin.restore(&RestoreRequest { backup_id: backup_id.clone(), user_ids: vec![a_id.clone()], all: false, mode: Some("merge".into()) }).await.unwrap();
    assert!(alice.texts(&dm).await.contains(&"written after the backup".to_string()));
    // ...replace does.
    let r = s.admin.restore(&RestoreRequest { backup_id: backup_id.clone(), user_ids: vec![a_id.clone()], all: false, mode: Some("replace".into()) }).await.unwrap();
    assert!(r.messages_hidden >= 1, "{r:?}");
    let t = alice.texts(&dm).await;
    assert!(!t.contains(&"written after the backup".to_string()) && t.len() == 2, "{t:?}");
    assert!(bob.texts(&dm).await.contains(&"written after the backup".to_string()), "bob's copy is never touched");

    // Selective restore from a full backup: clear bob's history, restore only bob.
    s.admin.backup_now().await.unwrap();
    let full = s.admin.backups().await.unwrap().into_iter().find(|b| b.kind == "full").unwrap();
    s.admin.clear_history(&b_id).await.unwrap();
    let r = s.admin.restore(&RestoreRequest { backup_id: full.id.clone(), user_ids: vec![b_id.clone()], all: false, mode: None }).await.unwrap();
    assert!(r.users_restored == 1 && r.messages_unhidden + r.messages_added > 0, "{r:?}");
    let bob_again = s.login("bob").await;
    let in_dm = bob_again.texts(&dm).await;
    assert!(in_dm.contains(&"old two".to_string()), "{in_dm:?}");
    assert!(bob_again.texts(&cdm).await.contains(&"carol to bob".to_string()));
}

#[tokio::test]
async fn deleted_accounts_come_back_from_backup() {
    let (s, _att, _bk) = server_with_shared_storage().await;
    let alice = s.user("alice").await;
    let bob = s.user("bob").await;
    let dm = alice.open_dm(&bob.user_id).await.unwrap();
    alice.send(&dm, "before deletion").await;
    s.admin.backup_now().await.unwrap();
    let full = s.admin.backups().await.unwrap().into_iter().find(|b| b.kind == "full").unwrap();
    s.admin.delete_user(&alice.user_id).await.unwrap();
    assert!(alice.me().await.is_err());
    assert!(Client::login(&s.url, "alice", PW).await.is_err());
    let r = s.admin.restore(&RestoreRequest { backup_id: full.id, user_ids: vec![alice.user_id.clone()], all: false, mode: None }).await.unwrap();
    assert_eq!(r.users_restored, 1, "{r:?}");
    let alice2 = s.login("alice").await;
    assert!(alice2.texts(&dm).await.contains(&"before deletion".to_string()));
}

/// A full backup made by 1.1.0 (its `conversations` table still has the `visibility` column, with public channels in
/// it) restores on a 1.2.0 server: every channel is kept, the old column is ignored and all channels are members-only.
#[tokio::test]
async fn full_backup_from_1_1_0_with_visibility_still_restores() {
    let dir = tempfile::tempdir().unwrap();
    // Build a 1.1.0-shaped snapshot: today's schema plus the legacy column holding public values.
    let old = workchat_server::db::Db::open(&dir.path().join("old.db")).unwrap();
    let snap = dir.path().join("snapshot.db");
    let snap2 = snap.clone();
    old.call(move |c| {
        c.execute_batch(
            "ALTER TABLE conversations ADD COLUMN visibility TEXT NOT NULL DEFAULT 'private';
             INSERT INTO conversations(id,kind,name,created_ms,visibility) VALUES('pub1','channel','town-square',1,'public');
             INSERT INTO conversations(id,kind,name,created_ms,visibility) VALUES('priv1','channel','secret',2,'private');",
        )?;
        c.execute("VACUUM INTO ?1", [snap2.to_string_lossy().to_string()])?;
        Ok(())
    })
    .await
    .unwrap();
    drop(old);

    let (st, _url) = fresh_server(dir.path()).await;
    workchat_server::fullbackup::import_snapshot(&st, &snap).await.unwrap();
    let rows: Vec<(String, String)> = st
        .db
        .call(|c| {
            let mut s = c.prepare("SELECT id,name FROM conversations ORDER BY id")?;
            let v = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
        .await
        .unwrap();
    assert_eq!(rows, vec![("priv1".to_string(), "secret".to_string()), ("pub1".to_string(), "town-square".to_string())]);
    // Reading them back through the normal query path works (no visibility needed anywhere).
    let sum = st.db.call(|c| workchat_server::queries::load_conversation(c, "pub1")).await.unwrap();
    assert_eq!(sum.name.as_deref(), Some("town-square"));
}
