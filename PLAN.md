> **Note:** this is a planning document written while the project was being built (2026-11-21). It records the original
> requirements and decisions, and parts of it are out of date. The README files describe the current state.

# WorkChat — plan (owner requests 2026-11-21)

Project folder: `Projects/WorkChat/`
- `workchat-server/` — the server (own git repo, Rust/axum, see its README). Already has DMs, channels, threads, files,
  avatars, pins, bots (Telegram-style API + webhooks), push (ntfy / FCM / APNs), admin panel.
- `Clients/` — NEW: the WorkChat apps (own git repo).
- `images/app-icon.svg` — the app icon (use for every platform).
- `images/screenshots/theme/` — THE look: copy layout, colours, type, spacing closely.
- `images/screenshots/desktop/` — desktop LAYOUT ideas (added 2026-11-21): navy left rail (logo, Chats, Contacts,
  Settings, avatar), centred global search, chat list column, conversation pane, right Thread pane with its own composer,
  centred PIN modal, ⋯ menu "local contact avatar override" popover, two-column Settings (Profile, Database & Cache,
  Notifications/ntfy). No Calls icon, no Bots & Automation screen (bots live in the server admin panel).
- `images/screenshots/usability/` — inspiration for features only (rich-text toolbar, "3 replies" thread link, PIN keypad,
  pinned section). Do NOT copy its look.

## Owner requirements
1. Apps for Windows (x86_64 + arm64, installer + portable), Linux (deb, rpm, AppImage, flatpak), Android (arm64, x86_64
   separate APKs), iOS — built by GitHub Actions as separate assets per version. Lightweight.
2. Chat, voice messages, attachments. NO calls, NO video calls (no Calls tab anywhere).
3. DMs and channels. Bots are set up on the server and appear as normal chat partners/members (no separate Bots section/filter).
4. Message threads.
5. Files: everything WhatsApp can send — photos, videos, audio, voice notes, documents of any type (pdf, docx, xlsx, pptx,
   txt, zip, apk, …), camera capture on phones. Server limit 100 MB.
6. UI: study `images/screenshots/theme` closely and match it.
7. Rich-text formatting toolbar in the composer (bold, italic, strike, bullet list, numbered list, inline code, code block,
   quote, link).
8. App icon: `images/app-icon.svg`.
9. Chats can be locked with a 6-digit PIN. Menu/action labels are exactly "Lock" / "Unlock" (never "Unlock Chat").
10. Notifications: Google (FCM), Apple (APNs), and self-hosted ntfy (on the server's Docker network).
11. Choose channel avatar and chat (DM/conversation) avatar, plus own profile avatar.
12. Telegram-style Bot API & webhooks (server side exists; apps simply show bot messages normally).
13. Mobile: filter pills at the top: All, DMs, Channels, Threads, Files (with counts like the screenshot).
14. Seed data: 10 users, 10 channels, chats with 30 messages each incl. images, videos, docx, xlsx, pdf, voice notes, threads.
15. Must work on the local network: plain `http://<LAN-IP>:<port>` server addresses (private IPs / .local / localhost)
    must work on every platform (Android cleartext config, iOS ATS local networking, desktop), https everywhere else.

## Owner decisions
- Stack: **Tauri 2** (Rust core + Svelte 5/TypeScript UI, Vite). One codebase for desktop + Android + iOS.
- No Firebase / Apple Developer account yet: ship ntfy push now; FCM and APNs code is built in and switches on when keys
  are added later (GitHub secrets / admin panel). iOS ships unsigned (sideload IPA + simulator build).
- Seed data: reusable seed script + run it on a local Docker test server. Not on the VPS.
- GitHub repo: not created yet — ask the owner before creating repos, uploading secrets or pushing.

## Release (owner 2026-11-21)
- Publish to GitHub as **WorkChat v1.1** (version 1.1.0, tag v1.1.0) in public repo `uri-travoski/workchat-claude`; give the
  owner links to every app asset.
- Settings shows the app version and a "Check for updates" button; apps check GitHub releases
  (api.github.com/repos/uri-travoski/workchat-claude/releases/latest) and offer the right asset for the platform
  (Android: download + install APK; Windows/Linux: download the matching package/installer and open it; iOS: open release page).
- Apps must work from the LAN against the local server on this PC: stop superchat-superchat-1 (keep volume), run the
  seeded workchat server on http://192.168.8.101:8080.

## Architecture (decided)
- **Rust core does all networking and storage** (inside the Tauri app, `src-tauri`): reqwest (rustls) + tokio-tungstenite
  WebSocket, uses a vendored copy of the server's `proto` crate (synced by a script from `../workchat-server/crates/proto`;
  the Clients repo must still build on its own, so the copy is committed). Tokens, sync-by-cursor, SQLite cache
  (rusqlite bundled), outbox, uploads/downloads streamed to disk, media served to the webview through a custom URI scheme
  (no auth headers in the webview, no CORS needed). LAN http allowed for private/loopback/.local hosts only.
- **UI**: Svelte 5 + TS, talks to the core with Tauri commands + events. Generated TS types from the server's
  `schema/api.schema.json` (vendored copy). Inter font bundled. One responsive UI:
  - phone (< 760 px): exactly the theme screenshots — header ("Chats" + "● Online", search, ⋮), filter pills
    All / DMs / Channels / Threads / Files with counts, sections PINNED / DIRECT MESSAGES / CHANNELS, navy FAB (pencil),
    bottom nav **Chats / Contacts / Settings** (no Calls).
  - desktop/tablet: left sidebar (workspace header, filters, sections) + conversation + thread side pane.
- Theme tokens from screenshots: navy `#21375B` / `#19273F`, page `#F4F6FA`, cards white, radius 12–16, link/time blue
  `#1E5BB8`, muted grey text, navy unread badges, outgoing bubbles navy with white text, incoming white; dark mode;
  accent choices (Navy Classic, Blue, Slate, Deep navy), bubble radius and text size sliders as in "User Preferences".
- Voice messages: record + waveform playback on all platforms (implementation chosen per platform; must be verified on
  Linux desktop and Android emulator).
- Lock: per-chat lock with a 6-digit PIN (PIN hash stored locally, never sent), locked chats hidden from lists until
  unlocked with the keypad screen (6 dots), auto re-lock when the app goes to background / after a timeout.
- Notifications: desktop = OS notifications while running (tray), Android = UnifiedPush via the ntfy app/self-hosted ntfy
  (server push kind "ntfy") + FCM flavour when `google-services.json` is provided at build time, iOS = APNs registration
  (works once signed with an Apple team).
- Identity: product **WorkChat**, bundle id `app.workchat`, deep link `workchat://`, version 1.0.0.

## Phases (Sonnet builds, Opus plans/verifies)
- **S1 server** (workchat-server): any missing endpoints the apps need (Threads list for the user, Files list across
  chats, whatever else the UI needs), UnifiedPush-compatible ntfy registration check, seed tool + generated sample files,
  local seeded test server, tests green.
- **C1 core + shell**: Tauri scaffold, Rust core (auth/login incl. one-time login link, sync, WS, cache, outbox), theme,
  chat list (phone + desktop), conversation view, composer + rich-text toolbar, LAN http.
- **C2 messaging features**: attachments (all types, previews, viewer, video player, document cards), voice messages,
  threads, filters, reactions/edit/delete/pin/forward, avatars, contacts, search.
- **C3 settings + lock + notifications + mobile**: settings screens per screenshots, Lock/Unlock PIN, notifications
  (desktop, Android UnifiedPush/FCM, iOS APNs), deep links, Android build + emulator test.
- **C4 packaging + CI**: icons from app-icon.svg, Windows x64/arm64 setup + portable, Linux deb/rpm/AppImage/flatpak,
  Android arm64/x86_64 signed APKs, iOS unsigned IPA, release workflow; local Linux + Android builds verified, sizes reported.
- **Verify (Opus)**: run against seeded server, screenshots phone + desktop vs theme screenshots, emulator test, LAN test.
