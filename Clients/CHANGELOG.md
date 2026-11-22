# Changelog

All notable changes to the WorkChat apps. The release workflow uses the section matching the tag as the
GitHub release notes, so add a `## [x.y.z]` heading before tagging.

## [1.2.0] - 2026-11-22

Desktop redesign, global search, all channels private, and Linux arm64 builds on every release.

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
