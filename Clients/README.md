# WorkChat apps

Tauri 2 desktop and mobile client for the WorkChat server. A Rust core does all networking and storage; a Svelte 5 +
TypeScript UI (Vite) talks to it with typed commands and events. Bundle id `app.workchat`, version 1.1.0. Source and releases: https://github.com/uri-travoski/workchat-claude.

## Layout

| Path | What |
|---|---|
| `src-tauri/` | Tauri app, crate `workchat` (lib `workchat_lib`) |
| `crates/workchat-proto/` | Vendored copy of the server's protocol crate (committed, so this folder builds alone) |
| `schema/api.schema.json` | Vendored JSON schema; TS types are generated from it into `src/lib/api-types.ts` |
| `crates/tauri-plugin-workchat/` | Mobile plugin: Kotlin (Android) and Swift (iOS) side of notifications, installer, picker, system bars |
| `packaging/` | Per-platform packaging inputs (flatpak, Android `MainActivity.kt`, iOS `Info.ios.plist`, Windows, Linux) |
| `src/` | Svelte UI (`components/`, `lib/`) |
| `tests/` | TS tests plus `markup-cases.json`, shared by the Rust and TS markup parsers |
| `scripts/` | `sync-proto.sh`, `gen-types.mjs`, `test-ts.sh`, `build-debug.sh`, `dev-seed.mjs` |
| `tests/ui/` | Xvfb screenshot driver (`run.sh`) and the UI scripts that walk the app against a seeded server |
| `docs/screenshots/c1/`, `c2/`, `c3/` | Phase C1, C2 and C3 screenshots (phone about 405x880, desktop 1280x800, `c3/android/` from the emulator) |

## Dev setup

Needs Rust (stable), Node 20+, and on Linux the webkit2gtk 4.1 / GTK3 dev packages.

```sh
export PATH="$HOME/.cargo/bin:$PATH"
# only if the default linker misbehaves on your machine:
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=/usr/bin/cc CC=/usr/bin/cc CXX=/usr/bin/c++
npm install
npm run tauri dev                      # app with hot reload
npm run check                          # svelte-check
npm test                               # TS tests (esbuild bundle + node --test)
cargo test --workspace                 # Rust tests
npm run tauri build -- --bundles deb   # release build, size-tuned profile
./scripts/build-debug.sh               # debug binary with embedded UI: target/debug/workchat
```

UI screenshots: `./scripts/build-debug.sh`, then `tests/ui/run.sh tests/ui/phone.js 405x880 out/` (or `desktop.js 1280x800`).
The driver starts Xvfb and the debug binary, signs in as `katherine.wells` on the seeded server (edit `tests/ui/boot.js`
for another server), runs the script inside the page and saves a PNG at every `api.mark(name)`. Debug builds only; the
hooks (`WORKCHAT_TEST_SCRIPT`, `test_run`, `test_mark`) compile to no-ops in release. Test aids for audio and pickers:
`WORKCHAT_TEST_PICK=/a.jpg:/b.pdf` answers the file picker, `WORKCHAT_FAKE_MIC=/clip.wav` feeds that file in real time as
the microphone, `WORKCHAT_AUDIO_NULL=1` plays to a silent clock instead of a sound card.

`WORKCHAT_DATA_DIR=/some/dir` overrides the app data directory (session, cache, media) - handy for several accounts
and for UI tests under Xvfb (set `WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1` there).

### Keeping the protocol in sync

```sh
./scripts/sync-proto.sh     # re-copies proto + schema from ../workchat-server and regenerates the TS types
```

Run it whenever the server's API changes, then `cargo test --workspace` and `npm run check`.

### Seed data for local testing

`node scripts/dev-seed.mjs` fills a fresh dev server with users, channels, DMs, a thread, reactions and uploads
(see the header of the script for the URL and credentials it expects).

## Architecture

Core modules (`src-tauri/src/`):

- `api.rs` - URL rules (https always; http only for loopback, RFC1918, link-local, `*.local`, `10.0.2.2`), REST client
  with token refresh, login (password or one-time link), `session.json` (0600 on desktop).
- `engine.rs` (WebSocket part): session (`hello{token,cursor}`, events, ack, ping, typing) with REST
  catch-up fallback and backoff. Wipe codes (`session_revoked`, `account_disabled`, `password_reset`, `history_cleared`)
  clear local data.
- `store.rs` - SQLite cache: kv, users, conversations, messages, prefs, threads, outbox, media index.
- `sync.rs` - applies the per-user ordered event log to the cache (idempotent, cursor based) and returns UI events.
- `view.rs` - view models built in Rust (`ChatRow`, `MessageView`, `Snapshot`, `CoreEvent`).
- `engine.rs` - orchestrates login, bootstrap, WS loop, outbox loop and every command.
- `media.rs` - custom `wcmedia://` scheme: serves cached files, downloads on demand, supports HTTP range requests.
- `features.rs` - C2 commands: staging of picked, dropped, pasted and recorded files, attachment save/open, threads, counts,
  search, forward, pins, saved messages, avatars (own, chat, local overrides), creating chats.
- `mediainfo.rs` - content types, image size, MP4 duration and size, PDF page count (details the server shows on cards).
- `audio.rs` / `voice.rs` / `player.rs` - voice messages. Recording (cpal, any OS) is resampled to 16 kHz mono and written
  as Ogg Opus by libopus; playback decodes Ogg Opus (libopus) and AAC/m4a, MP3, Vorbis, FLAC, WAV (symphonia) and plays
  through cpal, with 1x / 1.5x / 2x by WSOLA time stretching (pitch stays). Recording and playing happen in Rust, not
  in the webview, because WebKitGTK and iOS WKWebView cannot play Ogg Opus and `getUserMedia` is unreliable in them.
- `markup.rs` - rich-text parser; `src/lib/markup.ts` is its twin and both pass `tests/markup-cases.json`.

Commands (`invoke`) - core: `app_info, check_server, login_password, login_link, sign_out, get_snapshot,
open_conversation, load_older, send_message, retry_message, discard_message, edit_message, delete_message, react,
mark_read, set_favourite, order_conversations, create_dm, list_files, get_thread, chat_row, typing, reconnect, open_url`.
C2: `pick_files, stage_paths, stage_begin/chunk/end, unstage, set_staged_meta, send_files, save_attachment,
open_attachment, voice_start/finish/cancel, audio_play, audio_ctl, get_counts, list_threads, follow_thread, read_thread,
search, list_users, update_profile, forward_message, pin_message, get_pins, save_message, fetch_message, messages_around,
set_avatar, remove_avatar, set_local_avatar, remove_local_avatar, create_conversation, add_members, update_conversation`.

Events (channel `wc`, `CoreEvent`): `conn, chat, chat_remove, message (with replaces), message_remove, user, thread,
thread_remove, typing, presence, me, reset, signed_out, counts, voice (live level), audio (player state), saved, pins,
upload (progress), local_avatars`.

C3 modules:

- `lock.rs` - per-chat Lock: PBKDF2-HMAC-SHA256 (ring, 210k iterations, random salt) PIN hash that never leaves the device,
  throttling after 5 wrong tries, auto-relock (leaving the app, idle timeout), Change PIN, Forgot PIN (signs out and wipes).
  Locked chats are filtered out in `view.rs`/`store.rs` (lists, counts, search, files, notification text) while the area is closed.
- `notify.rs` - pure `decide()` (which events notify, mentions, preview off, locked chats show "New message", DND, muted,
  focused chat) plus the OS notification on desktop; `desktop.rs` - tray icon with unread badge, close-to-tray;
  `android.rs` - JNI entry points so a push, a boot or the keep-alive service can run the sync engine without the UI
  (wake, fetch, local notification); `device.rs` - this device's push registration and sessions.
- `update.rs` - update check against `https://api.github.com/repos/uri-travoski/workchat-claude/releases/latest`
  (`WORKCHAT_UPDATE_API` or the About page's developer field overrides it), semver compare, per-platform asset choice
  by the names in `docs/RELEASE.md`, streamed download with SHA-256 check, install (Android installer, Windows setup,
  AppImage swap, deb/rpm/flatpak instructions), a silent check about once a day with a badge on About.
- `portable.rs` - Windows portable mode (data next to the exe when a `portable` marker file exists).
- Plugin `tauri-plugin-workchat`: UnifiedPush (connector 3.x) with a wake fetch and local notification; "keep connection
  alive" foreground-service fallback; FCM only when `google-services.json` exists; POST_NOTIFICATIONS; iOS APNs token
  registration in Swift (compile-only here).

C3 commands: `lock_status, lock_set_pin, lock_unlock, lock_relock, lock_change_pin, lock_set_timeout, lock_tick, lock_touch,
lock_background, lock_chat, lock_forgot`, `get_notify_settings, set_notify_settings, notify_status, notify_request_permission,
notify_test, push_info, push_register, push_endpoint, push_unregister`, `update_state, check_update, start_update,
set_update_api`, `list_sessions` (and revoke), `get_org_settings, browse_channels, join_channel, leave_conversation,
remove_member, get_members, hide_message, set_system_bars`. Events add `org_settings, channel_directory` and the lock and
update state events.

Sending is optimistic: the message is stored in the outbox with local id `local:<client_id>`, shown immediately,
retried with the same `client_id` (idempotent on the server) and replaced when the server echo arrives. Messages with
files go through the same outbox: each file is streamed to `POST /files` with progress (cancel stops the stream), its
details (`duration_ms`, `peaks`, size, poster, pages) are saved with `PUT /files/{id}/meta`, then the message is sent with
`attachment_ids`. A restart resumes where it stopped (`att_id` and `meta_set` are stored per file). The uploaded file is
adopted into the media cache, so the sender never downloads their own file again.

Media ids in `wcmedia://localhost/<id>`: server attachment ids, plus local ones that never reach the server: `st_*`
(staged, e.g. in the preview sheet), `up_<client>_<n>` (files of an unsent message) and `la_*` (local avatar overrides).

UI: phone (< 760 px) is one screen at a time with the bottom bar Chats / Contacts / Settings. Desktop is a navy left
rail, a top bar (brand, search, profile), the chat list column, the conversation and an optional thread pane.
