# Status

## Phase C1 (core and shell): done

Built and verified against a real server (container `workchat-server:c1` and the seeded server on :18600):

- Tauri 2 app scaffold, size-tuned release profile, deb build (`WorkChat_1.0.0_amd64.deb`, about 4.2 MiB; release binary 9.1 MiB).
- Vendored protocol crate, schema, generated TS types, `scripts/sync-proto.sh` (synced to server 1.1.0, commit 5df0f11).
- Rust core: REST, WebSocket sync by cursor, SQLite cache, outbox, `wcmedia://` media scheme with range support,
  LAN http rule, token storage.
- Login with server + user + password, or a pasted one-time login link; sign-out wipes local data.
- Chat list: pills All / DMs / Channels / Threads / Files (Saved from M1) with counts, sections PINNED (with Reorder, gone since D1) / DIRECT MESSAGES /
  CHANNELS, avatars, chips, ticks, pin and unread badges, FAB, search.
- Conversation: day separators, bubbles, rich text, reactions, "N replies", attachments shown as image and file cards,
  infinite scroll up, live updates, optimistic send with outbox and retry, edit and delete from the context menu
  (server rules such as "cannot edit after a reply" are surfaced as toasts), typing indicator, presence.
- Composer: rich-text toolbar with live preview, emoji picker, navy mic/send button, Enter rules (desktop Enter sends;
  phone Enter is a newline unless "Send with Return Key"), drafts per chat.
- Layout: phone shell (bottom nav Chats / Contacts / Settings, no Calls; replaced by a hamburger drawer in M1) and desktop shell (navy rail, top bar,
  list column, conversation, thread pane with "Reply to ..." composer). Light and dark theme, accent colours.
- Tests: `cargo test --workspace` 38 passing; `npm test` 13 passing; `npm run check` clean.

## Phase C2 (messaging features): done

Verified under Xvfb against the seeded server as katherine.wells (screenshots in `docs/screenshots/c2/`, 44 images) plus
unit tests. Admin-only flows were checked as `admin`.

- Attachments: picker (photos and videos / any file), phone Camera option, drag and drop and paste (desktop), preview sheet
  with caption and several files, streamed upload with progress, cancel and retry, `max_upload_bytes` respected. Cards:
  single photo or video (poster, duration, expand icon), albums up to 4 with "+N", full-screen viewer (zoom with wheel or
  pinch and double tap, pan, swipe and arrow keys, save, open, show in chat), inline video, coloured document tiles with PDF
  first-page preview and a "Preview document" link, audio player. Downloads go to the Downloads folder through the core.
- Voice messages: hold the mic (release sends, slide left cancels, slide up locks) or tap for hands-free; live timer and level
  waveform; playback bubble with waveform from `meta.peaks`, scrub, 1x / 1.5x / 2x, duration. Seeded m4a (AAC) and ogg (Opus)
  notes play, speed and seek work (checked by driving the player). Recording is Ogg Opus, `PUT meta` before sending.
- Threads: `GET /threads` list with unread and last reply, thread pane (right pane on desktop, full screen on phone) with the
  vertical connector, follow / unfollow, mark read, reply composer with toolbar and attachments. "Also send to channel" is a
  client-side approximation (see gaps).
- Pills use `GET /me/counts` and refresh on events; all five fit at 360 to 412 px. Files is a real view: Media grid /
  Documents / Audio, search, paging.
- Message actions: context menu (right-click, long press), quick reactions and full emoji picker, reply in thread, copy, edit,
  delete, forward (several chats, comment), pin / unpin with a pinned bar and list in the chat header, save, info (read by).
  DM ticks come from `last_read_seq` and follow the `read_receipts` pref. In-conversation search and global search (messages,
  people, files, channels) jump to the hit and flash it.
- Avatars: own picture (Settings, circle crop), channel and group picture (admins), initials fallback, local-only override
  from the chat's menu or a contact's profile ("Set local picture" / "Remove local picture").
- Contacts tab from `GET /users`, profile sheet with Message. The "Bot" chip is gone from the chat list (a subtle "bot" line is
  in the profile).
- New chat button: new message, new group, new channel (name, topic, picture, members), browse channels.
- Fixed: the stray "⋮" in `docs/screenshots/c1/phone-conversation-seeded.png` was the per-message button showing on a phone
  under a hover pointer; phone has no such button now (long press), desktop uses a hover toolbar inside the bubble.
- Tests: `cargo test --workspace` 62 passing (47 core, 15 proto), `npm test` 18 passing, `npm run check` clean.
  New: upload with progress / cancel / server refusal against a mock server, outbox attachments, Opus encode / decode,
  peaks, resampling, WSOLA stretch, seeded-style m4a / ogg / mp3 / wav decode, fake-mic recording, MP4 / PDF probes,
  counts / thread / file listing parsing, file type tiles. The markup tests are unchanged.

### Server gaps found in C2 (workarounds in the app)
- Creating channels and group DMs is admin-only (`403 forbidden`, `admin_only`); the app shows the form with a warning and
  surfaces the server's answer. The server does not add the creator as a member, so the app adds them.
- Pinning is owners and admins only; the menu shows Pin only where the server allows it.
- No "delete for me", no "also send to channel" option on thread replies (approximated by a second message in the channel
  that quotes the thread root), no public / private switch when creating a channel, no way to browse and join channels you
  are not in (Browse lists your own).
- `GET /me/counts` is not in the generated schema (type written by hand); there is no `GET /conversations/{id}` (the avatar
  `PUT` returns the updated chat, which the app uses).

### Known limits after C2
- Video poster frames come from the webview; under WebKitGTK in Xvfb here it produced none (probably no H.264 decoder), so videos sent from there show
  a plain play card; other platforms make the poster. Other people's posters (seeded) show.
- No real sound card or microphone exists in this build container: real cpal capture and playback were not run here. Tested
  instead: the encoder and decoder as unit tests, recording from a file-fed fake microphone, playback against a silent clock.
- Android content URIs from the system picker and the iOS / Android share sheet for downloads are not wired (C3).

## Phase C3 (settings, lock, notifications, Android, iOS): done

Desktop was verified under Xvfb against the seeded server (screenshots in `docs/screenshots/c3/`: s2-desktop, settings-desktop,
settings-phone, lock-desktop, update-desktop), Android on the emulator (AVD workchat-phone, Android 15 x86_64, screenshots in
`docs/screenshots/c3/android/`). The server gaps listed under C2 are closed by server 1.1 (see below).

- Settings (phone list and desktop two-column): profile with Edit Profile and Share Card (QR, vCard), Account (Saved, Starred and
  Pinned, Recent Files), Notifications and Sounds, Privacy and Security, Data and Storage, Appearance and User Preferences, Server,
  Devices and Sessions (revoke buttons only when `Me.privacy_controls_enabled`), Help and Support, About, Log Out, footer
  "WorkChat for <platform> - Version 1.1.0". Appearance (theme, accent, radius, density) and the preferences really apply.
- Lock and Unlock: menu says "Lock" and "Unlock"; salted PBKDF2 PIN; locked chats hidden everywhere (lists, counts, search, Files,
  notification text shows "WorkChat / New message"); full-screen keypad on phone, modal on desktop; wrong-PIN throttling;
  Locked chats sheet with Unlock per chat and Lock now. Emulator: set PIN, lock a chat, it disappears and the pill counts drop,
  a message to it notifies without name or text, wrong PIN message, unlock.
- Notifications: desktop OS notifications, tray with badge, close-to-tray (code and unit tests; the Xvfb container has no
  notification daemon or tray host). Android: UnifiedPush plugin, wake-then-fetch, "Keep connection alive" foreground
  service (verified on the emulator: a message from another user while the app is in the background posts "David Kim / text",
  tapping it opens the chat), FCM compiled only if `google-services.json` exists, POST_NOTIFICATIONS requested.
- Updates: GitHub latest-release API (`uri-travoski/workchat-claude`; today it answers "no release published yet"), semver,
  notes, platform asset choice, checksum, install step. Verified against a local fake release server on desktop and on the
  emulator (found 9.9.9, notes shown, download with progress, then the Android "Install unknown apps" permission screen).
- Android shell: Back button (sheet, thread, chat, settings page, tab, then background), edge-to-edge with the status bar
  taking the theme colour (light and dark checked), adjustResize, deep links. Also checked: login, chat list pills, photo
  album, inline video, voice record, send and play, attachment from the system picker, download to Downloads.
- iOS: Swift plugin (APNs token capture, status, permission), `Info.ios.plist` in `src-tauri/`; compile-only, not run here.
- Windows: portable mode in Rust (`portable.rs`), not run here.
- Server 1.1 (S2) integrated: org settings (New channel and pin visibility), create with visibility and automatic owner,
  manager rights, browse and join and leave, pins for members, Delete for me, Also send to channel with a "Replied to a thread"
  reference, `GET /conversations/{id}`, shared `Counts` type.
- Tests: `cargo test --workspace` 85 passing (70 core, 15 proto, including lock, notify, update, portable and store tests);
  `npm test` 20 passing; `npm run check` clean.

### Known limits after C3
- iOS silent-push wake (fetch before showing the notification) is not implemented; APNs shows what the server sends.
- No real UnifiedPush distributor was installed on the emulator (ntfy), so the endpoint registration path is covered by code
  and unit tests only; the keep-alive fallback is the verified path.
- FCM, APNs, Windows and iOS were not run (no keys, no Windows or Mac here).
- Android sandbox: the installer step of an update was verified only up to the system permission screen (the fake APK is not
  installable).

## Phase D1 (desktop changes, release 1.2.0): done

Verified under Xvfb against the seeded server (1280x800 and 1000x700, `docs/screenshots/d1/`; scripts `tests/ui/d1-*.js`, runner
`tests/ui/d1-all.sh`) plus unit tests (see README "UI" section for the behaviour):

- Resizable panes, new rail, no pills on desktop, two independent searches, Saved view, pinned drag and sorting, plain tray icon,
  default-app opening with the fallback dialog, open padlock on unlocked locked chats, centralised Escape handling, formatting
  toolbar fixes, public channels removed (server 1.2.0 synced), Linux arm64 packages and update asset choice, version 1.2.0.
- Tests: `cargo test --workspace` and `npm test` (see the release report for counts).

## Phase M1 (phone changes, part of 1.2.0): done

Phone layout only (desktop unchanged). Verified under Xvfb at 405x880 against the seeded server with real X input
(`tests/ui/m1-phone.js`, screenshots `docs/screenshots/m1/`) plus unit tests (`tests/phone.test.ts`):

- Bottom bar removed; hamburger drawer (profile, Files, People, Settings, version) with Back-button handling.
- Top-right menu: Lock / Unlock and Sort chats (Recent default / Alphabetically); New message, Reconnect, Settings, Sign out removed.
- Pills: Files replaced by Saved (list, tap to jump, Unsave button and long-press menu, count follows); Save / Unsave toggle in the message menu.
- Chat list headings: no "Reorder" text, no sort control on phone; pinned chats rearranged by press-and-hold then drag.
- Dead code removed: `BottomNav.svelte`, phone header menu items, grip and hint on phone.

### Known limits after D1
- The Xvfb box has no real default applications, so default-app opening was verified with a stub `xdg-open` on PATH that records its
  argument (all six document types) and a failing stub (exit 3) for the dialog; real handlers, "Open with", "Save as" dialogs and the
  Windows / macOS paths were not run here.
- The viewer closing the whole app could not be reproduced on Linux/Xvfb; the fix is defensive (one Escape stack).
- The arm64 release jobs have only been linted until the first tag run.

## Not done yet (left for later phases)

C4
- Packaging beyond the local deb (rpm, AppImage, flatpak, Windows, Android, iOS), icons for every platform from the final
  icon, GitHub Actions release workflow (these were added in the packaging step; see `docs/RELEASE.md`).
- Custom window title bar was not attempted; native window decorations are kept.

## Known quirks
- The "Bots & Automation" screen is intentionally not built (bots are managed in the server admin panel).
- UI testing under Xvfb is slow with software rendering; keep `WEBKIT_DISABLE_*` env vars set.
