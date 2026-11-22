# Changelog

All notable changes to the WorkChat apps. The release workflow uses the section matching the tag as the
GitHub release notes, so add a `## [x.y.z]` heading before tagging.

## [1.1.0] - Unreleased

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
