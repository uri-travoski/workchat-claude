# Changelog

Release notes for the whole WorkChat repository. The release workflow publishes the section matching the tag
(`## [1.2.0]` for `v1.2.0`) as the GitHub release notes. Component details: [workchat-server/CHANGELOG.md](workchat-server/CHANGELOG.md),
[Clients/CHANGELOG.md](Clients/CHANGELOG.md).

## [1.5.10] - 2026-11-25

Smaller W in the app icon.

* The W inside the WorkChat icon is 15% smaller, on desktop, phone and the server's pages.

## [1.5.9] - 2026-11-25

Desktop chat layout, closer to the reference.

* The active chat row has a light blue tint and a navy edge.
* Avatars have 12px rounded corners.
* The rail shows online or offline under the profile.
* The paperclip sits inside the message box, and there is space above the date separator.

## [1.5.8] - 2026-11-25

Desktop layout, closer to the reference design.

* The top bar is gone. Your profile is at the bottom of the left rail, above Settings; the WorkChat icon is at the top.
* Core chats (All, DMs, Channels, Threads) are separated from Files, Saved and Contacts by a divider.
* The chat list has a title, a compose button, one search bar (Ctrl+K) and pills: All, Unread, Channels, Bots. The floating + button is gone on desktop.
* Ctrl+N starts a new chat.
* Timestamps are smaller and grey; empty chats show no timestamp.
* Channels no longer show a second # badge; pictures show without a border.
* The empty state offers New Chat, New Channel and Notes to Self.

## [1.5.7] - 2026-11-25

New app icon.

* The new WorkChat icon is used on desktop, phone (launcher, with a larger symbol on a navy background), in the app, and on the server's web pages and browser tab.

## [1.5.6] - 2026-11-25

Desktop: paste a copied file.

* Copy a file in the file manager and paste it into a chat: the file is attached, not its path.
* Copy a picture in an image viewer or editor and paste it: it is attached (as before).
* A deleted last message no longer shows "Message deleted" in the chat list.
* The send preview shows transparent pictures on a light checkerboard, not black.

## [1.5.5] - 2026-11-25

Select several messages; deleted messages are no longer shown.

* Select messages: long-press (phone) or right-click (desktop) a message and choose Select, or Ctrl/Cmd-click on desktop. Tap more messages to add them. The bar at the bottom copies the text, deletes them for you, or deletes your own messages for everyone.
* Deleted messages are no longer shown as "Message deleted".

## [1.5.4] - 2026-11-25

Pull to refresh shows a spinner.

* Pulling down from the top of the chat list or a conversation shows a spinning circle until the app is connected to the server and the list has reloaded.
* If the server is not reached within 30 seconds the spinner stops and an error is shown.

## [1.5.3] - 2026-11-25

Sharing from other apps, launcher icon and pull to refresh. The server is unchanged; the version is kept in step across the repository.

* Android: "Share" from Google Photos and other apps now opens "Share to…" with the picture attached. Pick a chat, check the preview, and press Send.
* Android: the app icon is larger and has no white background.
* Pull down from the top of the chat list or a conversation to reconnect to the server and reload. Shows "Still offline" when the server cannot be reached.

## [1.5.2] - 2026-11-24

Picture fixes. The server is unchanged; the version is kept in step across the repository.

* Desktop: a copied image can be pasted into a chat (some systems only offer copied images as clipboard items).
* Transparent pictures no longer sit on a black box: thumbnails and the full-screen viewer show a light checkerboard behind them.

## [1.5.1] - 2026-11-23

Avatar and channel display tweaks. The server is unchanged; the version is kept in step across the repository.

* Channel names no longer show a "#" in front (chat list, and the Leave confirmation).
* Pictures for chats, groups and people have a dark navy (#1B2B47) ring. Pictures for channels keep their square corners and get the same ring.
* The small corner badges on channel and bot avatars are 10% bigger. The bot badge is a new robot glyph.

## [1.5.0] - 2026-11-22

Message labels, picture and text options, connection dot.

Labels (private to each person, managed in the apps):

* Label a message from its menu (right click on desktop, press and hold on the phone). Each label is a tick box; ticking applies it at once. Labelling a message that is not saved saves it.
* Create, rename, recolour and delete labels in Settings, Labels. Creating a label from a message also applies it to that message.
* Filter Saved messages by label: desktop has label chips above the Saved list; the phone's top-right menu has "Filter by label".

Phone:

* Emoji picker in chats opens as a full-width panel with larger emoji.
* Settings, Appearance: Message Text Weight (Regular, Medium, Semibold, Bold).
* The "Online" label under All Chats is gone. The avatar dot is green when connected to the server and red when offline.

## [1.4.0] - 2026-11-22

Picture fixes, channel and bot badges, smoother image zoom.

Phone and desktop:

* Channel names no longer start with "#". Channel avatars show a small hash badge in the bottom corner; bot accounts (the server tells the app which users are bots) show a small robot badge.
* Setting a channel, chat or profile picture works on Android again (the "expected raw bytes" error). Pictures can be PNG with transparency, WebP, SVG, GIF, JPEG, HEIC and more; they are saved as PNG.
* Image viewer: pinch and wheel zoom follow your fingers or cursor smoothly, and a zoomed picture can be dragged around (one finger or the mouse).

Phone:

* Touch and hold a chat, then Lock now locks it (and Pin / Mark as read work again).

Desktop:

* The All Chats and Threads icons are 10% bigger and thicker so they match the other sidebar icons.
* Windows: opening a document from a message is more robust (falls back to the shell's own open).

Server: version bump only (1.4.0); the compose file is image-only and the image is built by GitHub Actions on every release.

## [1.3.0] - 2026-11-22

Phone menu and icons, desktop fixes, Android share target.

Phone:

* The menu (top left) now has All Chats, DMs, Channels, Threads and Saved above Files, People and Settings. The filter pills are gone; the app always opens on All Chats.
* New icons for All Chats, DMs and Threads. The floating new-chat button shows a plus.
* "Share" from other Android apps now lists WorkChat (files, photos, videos and text); pick the chat to send to.
* The PIN keypad's delete key is a backspace icon.
* "Check for updates" stays readable after a tap.
* The app icon is inverted: navy bubble on white.

Desktop:

* The new-chat button shows a plus. The image viewer's close button is the last icon at the top right.
* The window sets its own taskbar icon. Same new icons as the phone.

Server: version bump only (1.3.0), no changes.

## [1.2.0] - 2026-11-22

Desktop redesign, global search, all channels private, and Linux arm64 builds on every release.

### Server

* **Public channels are gone.** Every channel is private: members are added by admins and channel managers (the creator becomes the first manager).
  Browsing and joining public channels no longer exists, so the browse and join endpoints are removed.
* The `visibility` field is removed from the channel API (create and update requests, responses, events), the JSON schema and the admin panel.
* Existing public channels become private; their members keep access. API docs and schema updated accordingly.

### Apps

Desktop:

* Left rail reorganised: All Chats, DMs, Channels, Threads, Files, Saved, Contacts; Settings at the bottom. The app tile and the avatar are removed from the rail.
* Draggable pane dividers; the widths are remembered.
* The filter pills are removed on desktop (the rail replaces them) and kept on phones.
* Two independent search boxes: the top bar is a global search of messages, people and files with a results panel and jump-to-message highlight; the box above the chat list filters chat names only.
* New Saved view.
* Pinned chats are reordered by dragging (the Reorder button is removed). Unpinned chats sort by recent activity, with an A-Z / Z-A option.
* The tray shows only the plain icon (no unread badge).
* Attachments open in the system default app, with an Open with / Save as / Show in folder dialog as fallback.
* Fix: closing the image viewer or any other overlay no longer closes the app (Esc handling is centralised).
* Open-padlock icon on locked chats while they are unlocked.
* Formatting toolbar fixes.
* No link previews any more: links stay plain and clickable.

Phone:

* The bottom navigation bar is gone. A hamburger icon at the top left of the chat list (with the unread total as a badge) opens a slide-in drawer with your profile, Files, People and Settings, and the app version. Android Back closes the drawer first.
* The top-right menu now holds only Lock / Unlock (Unlock asks for the PIN, Lock locks at once, the first Lock sets up a PIN) and Sort chats (Recent or Alphabetically, remembered on the device; pinned chats are never sorted). New message, Reconnect, Settings and Sign out moved away (new chat stays on the pencil button; Sign out is inside Settings).
* The Files pill is replaced by Saved: your saved messages with sender, chat and time, tap to jump to the message, Unsave from the row button or a long press. Files is in the drawer.
* Pinned chats are rearranged by pressing and holding a row and dragging it; "Reorder" and the three dots next to the section title are gone.
* Message menu: Save and Unsave.

Everywhere:

* **Linux arm64 builds (deb, rpm, AppImage, flatpak) on every release** (`WorkChat-<ver>-linux-arm64.deb`, `-linux-aarch64.rpm`, `-linux-aarch64.AppImage`, `-linux-aarch64.flatpak`).
* The update checker detects the CPU and picks the arm64 assets on arm64 Linux.
* Channels no longer have a public or private choice; the browse and join screens are gone.

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
