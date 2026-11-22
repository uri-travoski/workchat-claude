# Decisions (server)

Design decisions that shape workchat-server. Client-side decisions are not recorded here: a client lives in its own project.
The owner's original requirements are in [SPEC.md](SPEC.md).

## Architecture and stack

* Rust workspace, edition 2021. `proto` is the single source of API types (serde + schemars); `schema/api.schema.json` is generated and the admin
  panel's TypeScript types are generated from it, so a client and the server cannot drift.
* Server: axum 0.8 + tokio, rusqlite (bundled SQLite, WAL, FTS5). No ORM. One writer connection behind a mutex (`Db::call`), which is plenty for
  a team server and keeps ordering simple.
* **Per-user event log with a cursor** is the sync primitive. Every state change is written as event rows (one per recipient) inside the same
  transaction as the change, then pushed to the WebSocket hub after commit while the DB mutex is still held, so live order always equals cursor order.
  Clients store the last cursor, reconnect with it, and dedupe. Pruning (default 90 days, `WORKCHAT_EVENT_RETENTION_DAYS`) raises an `events_floor`;
  a client behind the floor gets `cursor_expired` (410) and re-bootstraps.
* Idempotent sends: unique `(sender, client_id)`. A retry returns the stored message (HTTP 200 instead of 201).
* Edit rule lives in `proto::rules::edit_allowed`: channels and notes-to-self always; DMs and group DMs only while no one else has posted later.
  The server is the authority.
* Per-user visibility: `message_hidden(user_id, message_id)`. "Each side of a chat stored separately" is implemented as a per-user hide table rather
  than duplicated rows, so deleting or clearing one person's copy never touches anyone else's and attachments are reference-counted by visibility.
* Pure Rust where a system library would otherwise be needed: PDF previews use `hayro` (no poppler in the image), backups use `age`.
  The image stays free of extra system packages.
* **Attachment details (`AttachmentMeta`)** are computed by the sender (duration, waveform, size, a small JPEG poster) and stored in an
  `attachments.meta` column through `PUT /files/{id}/meta` (uploader only, 64 KB cap). Attachments without it render as plain cards.
* **Pictures**: channel and group pictures are attachments with purpose `avatar` readable by every signed-in user; private pictures use purpose
  `avatar_private`, readable only by their uploader. Overrides live in `avatar_overrides(viewer_id, kind, target)`; the event goes to the viewer
  only; backup and restore carry them.

## Auth and sessions

* Opaque tokens, not JWT: access `at_...` (default 15 min) and a refresh token `rt_...`. Only SHA-256 hashes are stored in `sessions`.
  Revocation (disable, password reset, clear history, sign-out) is therefore immediate: the next request, and the open WebSocket, fail.
* **Refresh tokens are not rotated.** Rotation needs a grace window to survive a lost response on flaky mobile networks; it is a
  possible hardening later. Mitigation: refresh tokens are per device and revocable.
* Passwords: argon2id. Login lockout per username and per IP, token-bucket rate limiting, and a dummy hash so unknown users cost the same time.
* Disabled account: sessions revoked with reason `account_disabled`, sockets closed with 4403, the client receives `SignedOut{code}` and
  wipes its local data. Re-enabling restores the history (cloud chats).
* **Login link is a web URL** (`<public-url>/l/<token>`) so it is clickable anywhere and a phone camera opens a browser, not nothing. The token is
  hashed, single use, 7-day expiry. The landing page does not consume the token (a link preview or a refresh must not burn it); only a client's sign-in does.
  The landing page hands over to an installed client through the `workchat://login?server=<url>&token=<token>` scheme (Android: an intent link for
  package `app.workchat`).
* No public registration. First-run `setup` creates the owner and is refused afterwards (`setup_done`).
* **Who may sign out:** admins and owners always; members only when their Privacy options is Allow (`privacy_controls_enabled`). The default is Disallow,
  so people cannot remove their own session by accident. Only an admin (admin panel), or the server (disable, password reset, clear history) ends such
  sessions. Enforcement is on the server (`forbidden`, "sign-out is not allowed for this account") for `POST /logout`, revoking an own session,
  `POST /sessions/revoke-others` and `POST /sessions/revoke-all`; clients only hide the controls. Admin revocation, token refresh and the forced wipes
  (`account_disabled`, `session_revoked`, `password_reset`, `history_cleared`) are not gated.
* **User deletion**: v7 UUIDs begin with a timestamp, which made an ordering assumption in the cascade wrong; the cascade uses explicit deletes
  (sessions, devices, memberships, visibility, drafts, preferences, push registrations, bot tokens, login links) and keeps other people's copies of chats.

## Transport security

* The server speaks plain HTTP on 8080 and expects a tunnel or reverse proxy in front for HTTPS (see DEPLOY.md). Clients may refuse plain `http://`.
* **No end-to-end encryption.** The server can read messages; this is a self-hosted team tool where the admin is the account owner.
  Encryption at rest of the attachment bucket is the storage provider's job.

## Storage

* One `Storage` trait (`put_file`, `put_bytes`, `get` with range, `head`, `delete`, `rename`). `ObjStore` (the `object_store` crate) covers local disk,
  S3, R2, B2 and MinIO; `GDrive` is a small hand-written Drive v3 REST client (OAuth refresh token, `drive.file` scope, one app folder).
* "Move all files": background job that copies each object, verifies size (and sha256 when deep), switches the active backend, then deletes sources.
  Uploads and backups pause while it runs (`migrating` flag; uploads get `conflict`, backups get `backup_failed`).
* Google Drive is tested only against an in-process fake, not the real service. Real-world quotas, rate limits and Google's app verification for the
  `drive.file` scope are untested.
* Local storage is allowed (dev, small installs) but the admin panel says to use a cloud backend for production.

## Clear history and backups

* Clear history, delete user and date-range delete write a **per-user backup first** (JSON document, written to the storage, read back and checked);
  if that fails the action is refused with `backup_failed`. Attachments that no visible message references any more are *moved* (not deleted) to
  `backups/users/<uid>/<backup>/att/`.
* Uploader exception for downloads: the uploader can fetch a file only while it is not attached to any message yet.
* **Backups use age (scrypt passphrase)**, one file per backup. The passphrase is kept in `<data>/backup.key` or `WORKCHAT_BACKUP_PASSPHRASE`, never
  inside the backup or in the database. A full backup is a `VACUUM INTO` snapshot, so it is consistent while the server runs.
  `backups/full/latest.json` plus `index.json` in the storage target let a fresh server find the newest backup.
* **Restore bumps event ids by 1,000,000** and emits `resync`, so every client re-bootstraps instead of replaying a stale log.
* **Restore semantics**: "merge" (default) adds what is missing and keeps newer data; "replace" makes the person's chats equal to the backup.
  Deleted accounts are recreated. Per-user deletes need `user_ids` on restore; deletes "for everyone" write a `messages` backup that restores without it.
* **Mandatory backup before any bulk delete**; failure refuses the delete (`backup_failed`).
* **Backup format ids** are `workchat-user-backup` and `workchat-messages-backup` (JSON documents).

## Search, push, bots and AI

* **Search is always members-only**, also when filters name a conversation the caller is not in.
* **Push is content-free.** The server only sends a wake (ntfy body `wake`, FCM/APNs data `type: wake`); the client syncs and builds its own
  notification. Wakes go only to offline, non-DND, non-muted members whose notify level allows it, and only when they have no live WebSocket.
  The ntfy URL is per device (`reg.ntfy_url`). ntfy was verified against a real ntfy container; FCM and APNs only against local fakes.
* **Forms** are attached to a message (fields `text`, `multiline`, `boolean`, `select`, `multi_select`, plus buttons); answers arrive as
  `form.submitted` updates/webhooks. **App Home** is a bot-published tab of blocks (`header, text, divider, button, form`).
* **Scheduled messages** are rows in the database; the server sends them (also after a restart: late ones go out on start-up).
* **Outgoing webhooks** are signed with HMAC-SHA256 over `"<timestamp>.<body>"` (headers `X-WorkChat-Timestamp`, `X-WorkChat-Signature`); failed
  deliveries are retried with growing delays and survive a restart.
* **AI providers** are a list in the database with per-feature defaults (search, recap) and failover to the next enabled provider. Keys are
  never returned (only the last four characters). AI is off by default and the notice says message text goes to the provider.
* AI search runs on demand (Enter), not on every keystroke (cost).

## Admin panel

* Vanilla TypeScript + Vite (no framework), builds with a plain `npm run build`, served by the server from `WORKCHAT_ADMIN_DIR` at `/admin`.
  The Docker image builds it in a node stage and copies `dist`.
* All text is set through `textContent`, never `innerHTML`.
* **QR codes** for login links are generated in the admin panel by a small built-in encoder (byte mode, level M, versions 1 to 10), no library.
* **Mobile admin**: tables become cards under 720 px, the menu collapses, the viewer is single-pane.
* **The server asks GitHub, the browser does not** (Updates card): `GET /api/v1/admin/updates` (admins only) fetches the release with the server's HTTP
  client, caches the answer for an hour (failures for five minutes; "Check again" is honoured after a minute) and returns `AdminUpdates`. The admin
  pages keep their strict policy, one request per hour serves all admins, `WORKCHAT_UPDATE_URL` works for a mirror, and a test can use a fake server.
  A server without outbound internet shows "could not reach GitHub" and still shows the commands.
* Release semver comparison (`proto::release`): leading `v` ignored, numeric parts compared as numbers (`1.3.10` > `1.3.9`), a pre-release is older than
  its release, build metadata ignored. Drafts and pre-releases are never offered.

## Deployment

* Image: multi-stage (node, rust, debian trixie-slim with ca-certificates + tini), non-root user `workchat` (uid 10001), HEALTHCHECK via
  `workchat-server healthcheck` (no curl needed).
* **`deploy/` holds exactly the two release assets**: `docker-compose.yml` and `.env.example` (published as `workchat-server.env.example`).
  Everything developer-only lives elsewhere: the Dockerfile in `docker/Dockerfile` and a self-contained `docker/docker-compose.build.yml`
  (own project `workchat-dev`, `build:` from the checkout, loopback port, its own volume).
* **No Caddy, cloudflared, ntfy, MinIO or RustFS services.** The owner runs the tunnel and ntfy in separate containers and wants S3/R2/B2 handled inside
  the server (the storage backends are the server's own code, chosen in the admin panel). DEPLOY.md has one short section on what a front door must do.
* **Default image is pinned to the release** (`WORKCHAT_IMAGE` overrides, `:latest` documented). `docker compose pull` alone does not move to a newer
  release unless `WORKCHAT_IMAGE=...:latest`. `release.yml` fails when the compose default does not match the tag being released, so a release can
  never ship a compose file that pins another version.
* **`WORKCHAT_*` names only**, one name per setting, read directly by the server. Compose-only names (`WORKCHAT_IMAGE`, `_PORT`, `_BIND_ADDR`,
  `_DATA_VOLUME`, `_LOG_LEVEL`, `_NETWORK`) are not read by the server: the log level is passed as `RUST_LOG`, which the server's tracing filter reads.
* **No required variables.** Nothing uses `:?`; with no `.env` the stack starts with defaults (port 8080, public URL from the Host header or the admin
  panel). The compose healthcheck repeats the image's so the file shows it. `extra_hosts: host.docker.internal:host-gateway` stays so an ntfy on the
  host is reachable. Syntax is the common subset (no `depends_on` conditions, profiles or `include`).
* **The data volume** is `name: ${WORKCHAT_DATA_VOLUME:-workchat-data}` (explicit name, so the project folder name does not change it).
* **The release job order**: `release-files` runs first and creates the GitHub Release (body from `docs/RELEASE-NOTES-<tag>.md` when it exists, plus the
  compose and env files); the image job runs independently.
* **Clean break from the earlier server.** workchat-server reads no old setting names, looks for no old database file and has no compatibility shims;
  UPGRADING.md is the single place that explains the move.
