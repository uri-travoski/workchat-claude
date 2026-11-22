# Changelog

## 1.5.0

Version bump only (labels were already served).

## 1.4.0

Version bump only.

## 1.3.0

Version bump only.

## 1.2.0

Channels are private only; public channel browse/join removed.

* Removed: `Conversation.visibility`, `visibility` in `CreateConversation` / `UpdateConversation` (an old caller that still sends it is not rejected, the field is ignored), `GET /channels/browse`,
  `POST /conversations/{id}/join` (both `404` now), the event `channel_directory_update`, the types `ChannelVisibility`, `ChannelBrowseItem`, `ChannelBrowsePage`, and the admin panel's public/private
  choice, "Make public / Make private" buttons and "Public channel" label. `POST /conversations/{id}/leave` stays for channels and group DMs. A channel is joined only by being added by an owner, admin or manager.
* Migration: nothing is dropped. Existing databases keep all channels; the old `conversations.visibility` column (1.1.0 databases) is left in place but unused and every row is set to `private`;
  new databases do not have it. Backups made by 1.1.0 (full snapshots and per-user data with `visibility`) still restore; the field is ignored.
* New: `GET /saved/messages` returns each saved message in full (attachments) with its conversation, for a "Saved" screen.
* The server never fetched link previews, so there was nothing to remove there; the apps just show the plain link.
* Admin Updates card and `/downloads`: the heading is just "Client downloads" (the release version is a small "Latest release: vX.Y.Z" label), and a link "Open latest release on GitHub"
  (`WORKCHAT_RELEASE_URL`, default `.../releases/latest`) is always shown, including the error, no-release and server-offline states. `AdminUpdates` gains `release_url`. Tests: `updates.rs`, `tests/ui/admin-updates.mjs`.
* Tests: `channels.rs` (404 for browse/join, legacy `visibility` ignored, leave), `backups.rs` (1.1.0 snapshot restore), `db.rs` (migration), `social.rs` (`/saved/messages`).

## 1.1.0 (unreleased)

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

## 1.0.0

First release as a standalone server, split from the earlier project; see docs/UPGRADING.md.

* One Rust server (`workchat-server`) with its admin panel, shipped as the Docker image `ghcr.io/uri-travoski/workchat-claude-server`.
* Settings are read from `WORKCHAT_*` environment variables, the database is `workchat.db`, outgoing webhooks are signed with `X-WorkChat-*` headers.
* Own Cargo workspace (`workchat-server`, `workchat-proto`), own CI and release workflow, standalone `docker-compose.yml` and `.env.example`.
