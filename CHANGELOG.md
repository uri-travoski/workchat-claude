# Changelog

Release notes for the whole WorkChat repository. The release workflow publishes the section matching the tag
(`## [1.1.0]` for `v1.1.0`) as the GitHub release notes. Component details: [workchat-server/CHANGELOG.md](workchat-server/CHANGELOG.md),
[Clients/CHANGELOG.md](Clients/CHANGELOG.md).

## [1.1.0] - 2026-11-22

First public release of WorkChat (server and apps), published from https://github.com/uri-travoski/workchat-claude as "WorkChat v1.1".

### Server

What the new WorkChat apps need ([docs/API.md](docs/API.md#phone-and-desktop-apps-110)).

* Profile: `title` and `phone` on users (admin create and edit, own edit, contacts search `GET /users?q=&active=`).
* `GET /me/counts` (numbers for the All / DMs / Channels / Threads / Files pills), `GET /threads` gains `conv`, `unread`, `before`, `limit` and each thread a `last_reply`,
  `GET /files` gains `kind=` (alias of `type=`), `offset` and returns attachment details (voice duration and waveform, video poster, `pages`).
* Read receipts: `read_receipts` setting, `Member.last_read_seq` in DMs and group DMs and the `read_receipt` event.
* Push: new registration kind `unifiedpush` (full endpoint URL, any distributor); the ntfy split form (`ntfy_url` + topic) keeps working. Documented for every platform.
* Bots can get a picture and a title (`PATCH /admin/bots/{id}`); `PUT /admin/messages/{id}/ts` backdates a message (imports, seeding).
* Admin panel: works on plain http on a LAN (no `crypto.randomUUID`, clipboard fallback). The WorkChat icon replaces the old artwork everywhere: favicon (`/favicon.svg`, `.png`, `.ico`,
  `/apple-touch-icon.png` and in `/admin`), logo, landing page (now in the navy theme).
* `tools/seed`: sample-data generator and seeder (10 users, 10 channels, a group chat, 8 DMs, 2 bots, 30 messages each with photos, videos, voice notes and documents).
* Tests: `crates/server/tests/phoneui.rs`.

Phase S2 (what the app developers still missed, [docs/API.md](docs/API.md#channels-pinning-hide-broadcast-replies-110-phase-s2)):

* **Channels by everyone**: org setting `channel_creation` = `everyone` (default) | `admins` (`GET /org-settings`, `PUT /admin/org-settings`, in the bootstrap, event `org_settings_update`, admin panel *Settings* tab).
  The creator of a channel or group DM joins automatically as conversation role `owner` (the existing channel-manager role). Managers rename, set topic and picture, add and remove members, archive
  (`PATCH /conversations/{id}` is no longer admin-only). Group DMs can be created by anyone.
* **Public and private channels**: `Conversation.visibility` (`public` default for new channels, old channels stay `private`; migration adds the column), `GET /channels/browse?q=&limit=&offset=`,
  `POST /conversations/{id}/join`, `POST /conversations/{id}/leave`, event `channel_directory_update`; the admin panel shows and toggles visibility.
* **Pinning** is open to every member in DMs and group DMs and (default) in channels; setting `channel_pins` = `managers` restricts channels to managers and admins.
* **Delete for me**: `POST /messages/{id}/hide`, stored per user, event `message_hide` to the user's sessions; excluded from history, search, files, threads, counts and unread.
* **Also send to channel**: `SendMessage.also_send_to_channel` with `thread_root`; the reply is one message that also belongs to the main timeline (`Message.also_send_to_channel`). Migration adds `messages.also_channel`.
* Schema: `Counts` and the new types are in `schema/api.schema.json`; `GET /conversations/{id}` is documented; the own-sessions endpoints (`GET /sessions`, revoke one, revoke others, revoke all) are documented for the apps.
* Admin-created conversations (admin panel, seed tool) pass `include_creator: false` so the admin does not join them.
* Tests: `crates/server/tests/channels.rs`, a migration test in `db.rs`.

### Apps

First public release, published from https://github.com/uri-travoski/workchat-claude as "WorkChat v1.1".


- First release of the WorkChat apps (Tauri 2: Rust core, Svelte 5 UI).
- Settings: profile card (Edit Profile, Share Card with QR and vCard), Saved, Starred and Pinned, Recent Files,
  Notifications and Sounds, Privacy and Security, Data and Storage, Appearance and User Preferences (applied live),
  server, Devices and Sessions, Help and Support, About with an update check.
- Update check against the GitHub releases of uri-travoski/workchat-claude: semver compare, release notes, a per-platform
  action (Android installer, Windows setup, AppImage, package instructions) and a silent daily check with a badge.
- Lock and Unlock with a salted 6-digit PIN (never sent), throttling, auto-relock, Change PIN, Forgot PIN. Locked chats are
  hidden from lists, counts, search, notifications and Files; the keypad is full screen on phones and a modal on desktop.
- Notifications: OS notifications, tray badge and close-to-tray on desktop; UnifiedPush with a wake fetch, a keep-alive
  foreground service fallback, optional FCM and POST_NOTIFICATIONS on Android; APNs plugin for iOS.
- Server 1.1 features: org settings, public and private channels with browse and join, manager rights, pins for members,
  "Delete for me", "Also send to channel", `GET /conversations/{id}`.
- Android: back button, edge-to-edge insets, status bar colour, deep links `workchat://` and `https://<server>/l/<token>`.
- Windows portable mode.
- Direct messages, channels, message threads, reactions, edit and delete.
- Attachments of any type, voice messages, rich-text composer toolbar.
- Per-chat Lock with a 6-digit PIN.
- Notifications through self-hosted ntfy (FCM and APNs switch on when keys are added).
- Works with plain http servers on the local network as well as https.
- Builds: Windows x64 and arm64 (setup and portable), Linux (deb, rpm, AppImage, flatpak),
  Android (arm64 and x86_64 APKs), iOS (unsigned IPA and simulator build).
