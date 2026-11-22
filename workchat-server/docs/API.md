# API (v1)

Types are defined once in `crates/proto` (serde + schemars). `cargo run -p workchat-proto --bin gen-schema` writes
`schema/api.schema.json`; the admin panel's `src/api-types.ts` is generated from it (`npm run gen-types`).
Base path `/api/v1`. `GET /meta` returns `api_version`, `min_client_version`, `server_version`, `setup_required`,
`max_upload_bytes`. Clients send `X-Client-Version` and are refused with `client_too_old` (HTTP 426) below the minimum.

Errors: `{"error": {"code": "<stable code>", "message": "<plain English>"}}`. Codes: unauthorized, invalid_credentials,
token_expired, session_revoked, account_disabled, password_reset, history_cleared (these last four end the session and wipe
the device), locked_out, rate_limited, forbidden, not_found, validation, conflict, edit_window_closed, file_too_large,
cursor_expired (410), client_too_old, setup_required, setup_done, backup_failed, storage_error, not_implemented, internal.

Auth: `POST /login`, `POST /login/link`, `POST /refresh` (stable refresh token), `POST /logout`; bearer access tokens.

Sync: `GET /sync/bootstrap` (snapshot + cursor), `GET /events?after=<cursor>`, WebSocket `/ws` (`hello{cursor}` then
`events`; also typing, presence, ack, ping). Each user has an ordered event log; the cursor is its id.

Chat: `/conversations` (+`/dm`, `/self`, `/{id}`, `/{id}/messages`, `/{id}/read`, `/{id}/unread`), `/messages/{id}` (PATCH edit,
DELETE tombstone), `/messages/{id}/reactions/{emoji}`, `/files` (upload), `/files/{id}` (Range, signed URLs via `/files/{id}/url`),
`/search`, `/users`, `/me`, `/me/password`, `/sessions`.
Sends are idempotent on `(sender, client_id)`: a retry returns the stored message (200 instead of 201).
Edit rule (`proto::rules::edit_allowed`): channels and notes-to-self always; DMs and group DMs only while nobody else has posted later.

Admin (`/admin/...`, owner or admin only): overview, users (+reset-password, login-link, revoke-sessions, clear-history, backup,
delete-range), backups, sessions, conversations (+members), files, storage (+test, migrate, verify), jobs.

## Membership, saved items, labels, activity

All of these are per user and private unless noted. Changes reach your other devices as events `saved_update`, `saved_remove`, `labels_update` and `activity_read` (activity itself is derived from messages, so new items arrive with the message events).

* **Membership (admins and channel managers).** `POST /conversations/{id}/members` with `{"user_ids": [...]}` adds people to a channel or group; `DELETE /conversations/{id}/members/{uid}` removes one.
  Plain members get `403 forbidden`; non-members get `404 not_found` (the conversation stays invisible). A "X added Y" / "X removed Y" system message is posted. `PATCH /conversations/{id}` (admins and channel managers) takes `name`, `topic`, `archived`, `visibility`.
* **Save for later.** `GET /saved` lists saved items (newest first); `PUT /saved/{message_id}` saves (optional `{"label_ids": [...]}`), `DELETE /saved/{message_id}` removes it,
  `PUT /saved/{message_id}/labels` with `{"label_ids": [...]}` sets its labels. Only messages in conversations you belong to can be saved (`404` otherwise).
* **Labels.** `GET /labels`, `POST /labels` (`{"name","color"}` as `#rrggbb`), `PATCH /labels/{id}` (rename, recolour, reorder), `DELETE /labels/{id}`. Each call returns the full label list.
* **Activity feed.** `GET /activity?limit=n` returns mentions of you, reactions to your messages and replies in threads you follow (newest first, `unread` per item, `read_ms` for the feed);
  `POST /activity/read` marks it read (optionally up to a timestamp). Items for conversations you left are not returned.
* Backups include each user's saved items and labels; a per-user restore merges them back.

## Sign-out policy

* Self-service sign-out is gated by the admin's Privacy options (`privacy_controls_enabled` in `GET /me`). For a member without it, `POST /logout`, `DELETE /sessions/{own id}`,
  `POST /sessions/revoke-others` and the new `POST /sessions/revoke-all` answer `403 forbidden` ("sign-out is not allowed for this account") and change nothing.
  Admins and owners are exempt, admins can still end any session (`DELETE /sessions/{id}`, `POST /admin/users/{id}/revoke-sessions`), and token refresh and the forced
  revocations (`account_disabled`, `password_reset`, `history_cleared`) are unaffected. `GET /sessions` stays readable.
* `POST /sessions/revoke-all` signs the caller out of every device including the calling one and returns `{"revoked": n}`.

## Roles

* `PUT /admin/users/{id}/role` with `{"role":"member"|"admin"|"owner"}` returns the updated `AdminUser`. Owner only (`forbidden` for admins and members);
  `forbidden` when changing your own role; `validation` for deleted accounts, for a bot given any role but member, and when it would leave no owner.
  Permissions are read on every request, so the change is effective for open sessions immediately. The user's apps receive `me_update` (their own `Me`
  with the new role) and everyone receives `user_upsert`; no extra event type. `PATCH /admin/users/{id}` with `role` follows the same rules.

## Login link landing page, public URL, AI providers

* **Login link landing page** (no auth, not under `/api/v1`): `GET /l/<token>` shows the open-in-app page without consuming the token (200 valid, 410 used or
  expired, 404 unknown). `Cache-Control: no-store`, `Referrer-Policy: no-referrer`, a CSP that allows no outside content. `GET /downloads` lists installers and
  `GET /downloads/<file>` serves one from `WORKCHAT_DOWNLOADS_DIR`.
* `LoginLink.link` is now `<public-url>/l/<token>`. `POST /login/link` accepts that URL, the `workchat://login?server=&token=` form, or the bare token.
* `GET/PUT /admin/public-url` (`{"url": "https://chat.example.com" | null}`; PUT is owner-only; the response has `effective` and `source` = `env` | `setting` | `request`).
* AI providers (owner changes, admins read): `GET /admin/ai`, `POST /admin/ai/providers`, `PATCH`/`DELETE /admin/ai/providers/{id}`, `POST /admin/ai/providers/{id}/test`,
  `PUT /admin/ai/defaults`. Keys are never returned. See [BOTS.md](BOTS.md).
* `DELETE /admin/users/{id}` cascades correctly for users who signed in (sessions, devices, memberships, visibility, drafts, preferences, push registrations,
  bot tokens, login links) and keeps other people's copies of chats. Server errors are logged as `METHOD route -> status` plus the cause, never with tokens.

## Search, threads, pins, groups, preferences, push, admin viewer, backups

All paths are under `/api/v1`; types are in `crates/proto` and `schema/api.schema.json`.

* Search: `GET /search?q=&from=&in=&has=link|file|reaction&before=<ms>&after=<ms>&type=image|video|audio|document` returns
  `{hits, people, files, channels}`; only conversations the caller belongs to are ever searched. `GET /files?conv=&kind=&q=&before=` lists files.
* Threads: `GET /conversations/{id}/threads/{root}` (root + replies), `GET /threads` (threads the user follows or posted in, with unread),
  `PUT|DELETE /threads/{root}/follow`, `POST /threads/{root}/read`. A reply is a message with `thread_root` set; it is not part of the top-level timeline.
* Pins and bookmarks: `GET /conversations/{id}/pins`, `PUT|DELETE /conversations/{id}/pins/{message_id}` (members, see "Pinning" below), bookmarks: managers and admins,
  `GET|POST /conversations/{id}/bookmarks`, `PATCH|DELETE /bookmarks/{id}`.
* Groups: `GET /groups`, admin `POST /admin/groups`, `PATCH|DELETE /admin/groups/{id}`. `@handle` in a message mentions every member.
* Preferences: `GET /me/prefs`, `PUT /conversations/{id}/prefs` (`favourite`, `section_id` (empty string clears), `mute_until_ms`, `notify` =
  `all|mentions|nothing`), `GET|POST /sections`, `PATCH|DELETE /sections/{id}`. All synced to the user's other devices through events.
* Drafts: `GET /drafts`, `PUT /conversations/{id}/draft` (optional `thread_root`).
* `GET /messages/{id}`, `POST /messages/{id}/forward {conv_ids, comment}` (forwarded copies carry `forwarded` attribution).
  Permalinks are `workchat://m?server=&c=&m=`.
* Push: `GET|PUT /me/push`, `DELETE /me/push/{id}` (device registration: `ntfy`, `unifiedpush`, `fcm` or `apns`, see "Phone and desktop apps" below); admin `GET|PUT /admin/push` (secrets are masked on read).
  Pushes are content-free wakes: ntfy body `wake`, FCM/APNs data `type: wake`. They go only to offline, non-DND, non-muted members who want the notification.
* Sessions: `DELETE /sessions/{id}`, `POST /sessions/revoke-others`.
* Admin viewer: `GET /admin/viewer/conversations?user_id=&kind=&q=`, `GET /admin/viewer/conversations/{id}/messages?as_user=&before=&after=&around_id=&around_ts=&from_ms=&to_ms=&thread_root=&top_level=&limit=`,
  `GET /admin/viewer/search`, `POST /admin/messages/delete {message_ids | conv_id + from_ms/to_ms/before_ms, scope: "everyone"|"user", user_id, ...}` (see `AdminDeleteMessagesRequest`)
  (always writes a backup first and refuses with `backup_failed` otherwise).
* Backups and restore: `POST /admin/backups/full`, `GET|PUT /admin/backups/settings`, `POST /admin/backups/test-target`,
  `GET /admin/backups/{id}/download`, `POST /admin/restore {backup_id, user_ids | all, mode: "merge"|"replace"}` (backup kinds `user`, `full`, `messages`),
  and on a server with no users `POST /restore` (from the storage target) and `POST /restore/upload` (an uploaded `.age` file) with the passphrase.
  After a restore, event ids jump by 1,000,000 and clients receive `resync`, so every device re-bootstraps.

## Phone and desktop apps (1.1.0)

What the WorkChat apps need on top of the chat API above. All paths are under `/api/v1`, auth is the normal bearer token. `GET /meta` reports
`server_version`; an app can treat `>= 1.1.0` as "has these endpoints".

### Profile and contacts

* `User` now has optional `title` (job title, 80 characters) and `phone` (40 characters). The owner or an admin sets them in `POST /admin/users`
  (`title`, `phone`) and `PATCH /admin/users/{id}`; people edit their own with `PATCH /me {"title": "...", "phone": "..."}` (empty string clears).
  Everyone receives the change as `user_upsert`. The `@handle` shown on a profile is `username`.
* `GET /users?q=<text>&active=true`: the contacts directory (bots included, `is_bot`); `q` matches display name, username and title, case-insensitive;
  `active=true` hides disabled people. `GET /users/{id}` returns one. `presence` (`online`, `away`, `dnd`, `offline`) and `last_active_ms` are live; the
  header "Online" of the app is the app's own socket state, "Online" for a contact is `presence == "online"`.
* Avatars: own picture `PATCH /me {"avatar": <attachment id>}` after `POST /files?purpose=avatar`; channel and group chat picture
  `PUT /conversations/{id}/avatar {"avatar": id}` (owners, admins); a private picture for another person or a DM
  `PUT /me/avatar-overrides/{user|conv}/{target} {"attachment": id}` (upload with `purpose=avatar_private`). Bots: an admin uploads with `purpose=avatar`
  and sets it with `PATCH /admin/bots/{id} {"avatar": id, "title": "CI/CD"}`.

### Filter pills: counts, threads, files

* `GET /me/counts` returns `Counts`: `conversations`, `dms` (DMs and group DMs), `channels`, `unread_conversations`, `unread_dms`, `unread_channels`,
  `unread_messages`, `mentions`, `threads` (threads you take part in), `threads_unread`, `files`, `files_image`, `files_video`, `files_audio`,
  `files_document`, `saved`. Archived chats and notes-to-self are not counted. Refresh it on start and when `thread_update`, `conversation_upsert`,
  `message_new` or `attachment_removed` arrive; conversation counts can also be derived from the bootstrap data.
* `GET /threads?conv=&unread=true&before=<ms>&limit=` returns `ThreadSummary[]` (the threads the caller follows, which includes every thread they wrote in or
  started that got a reply, or were mentioned in; newest activity first, at most 200): `root` (a full message), `reply_count`, `last_reply_ts`,
  `last_reply` (the newest reply, for the preview line), `participants`, `unread`, `following`. `before` pages by last activity (`max(last_reply_ts, root.ts)`).
  `POST /threads/{root}/read` clears `unread`; `thread_update` events keep it current (`following: false` removes it).
* `GET /files?kind=image|video|audio|document&conv=&q=&limit=&offset=` returns `FileHit[]` (`type=` is the same parameter): files across all
  conversations of the caller, newest first, limit 1 to 200 (default 50). Each hit has `attachment` (with `meta`: duration, waveform peaks, width, height, poster, pages),
  `message_id`, `conv_id`, `sender_id`, `ts`. Removed files are listed with `removed: true`; deleted messages are not. Page with `offset`.
* Voice notes are ordinary audio attachments: any `audio/*` (the apps record Ogg Opus or MP4 AAC) named `voice-<date>-<time>.<ext>` with
  `PUT /files/{id}/meta` details (`duration_ms`, up to 64 `peaks` 0..255) set by the sender before the message is sent. Videos carry `width`, `height`,
  `duration_ms` and a small JPEG `poster_b64`; PDFs and decks carry `pages`. `GET /files/{id}/thumb` renders the first page of a PDF.

### Read receipts

* `Me.read_receipts` (default on) and `PATCH /me {"read_receipts": false}`. In `dm` and `group_dm` conversations each `Member` carries `last_read_seq` when that member
  has receipts on; your message is "read" when the other member's `last_read_seq >= message.seq` (a read position includes the person's own messages, so it
  is never below their latest message). Others' progress arrives as event `read_receipt {conv_id, user_id, last_read_seq}`, only when it moves and only if the reader has
  receipts on. Channels never carry positions. If you switch receipts off, hide other people's ticks in your own app too (the server does not hide theirs from you).
* Delivery: `message_delivered` events (`delivered` on a message) are the single grey tick; unchanged.

### Message time, for imports

`PUT /admin/messages/{id}/ts {"ts": <ms>}` (owner or admin) sets the time a message shows; it may not be in the future and must stay between the times of the
neighbouring messages. Members get `message_edit` with the new `ts`. Used by `tools/seed`.

## Channels, pinning, hide, broadcast replies (1.1.0, phase S2)

All paths are under `/api/v1`, bearer token. Types are in `schema/api.schema.json` (`OrgSettings`, `UpdateOrgSettings`, `ChannelBrowsePage`, `ChannelBrowseItem`,
`CreateConversation`, `UpdateConversation`, `HideMessageResponse`, `Counts`). A conversation you are not in never reveals itself: you get `not_found`, not `forbidden` (except public channels, which you can browse and join).

### Organisation settings

* `GET /org-settings` (any user) returns `OrgSettings` `{"channel_creation": "everyone"|"admins", "channel_pins": "everyone"|"managers"}` (defaults `everyone`, `everyone`).
  The same object is `Bootstrap.org_settings`. `PUT /admin/org-settings` (owner or admin) takes any of the two fields and returns the full object.
* Event `org_settings_update {settings}` goes to everyone when it changes. Use it to show or hide "New channel" and pin buttons.

### Creating channels and group chats

* `POST /conversations` with `CreateConversation` `{"kind": "channel"|"group_dm", "name", "topic"?, "members": [user ids], "visibility"?: "public"|"private", "include_creator"?: bool}`
  returns `ConversationSummary` (201 body as before). The creator is added automatically as a member with conversation role **`owner`** (this is the channel manager role that already existed:
  `Member.role`). `members` lists the other people. `visibility` defaults to `public` for channels; a group DM is always `private` (`validation` if you ask for public).
  Channels: allowed for everyone unless `channel_creation` is `admins`, then `403 forbidden` for plain members. Group DMs: always allowed for everyone and need at least two members in total
  (creator plus one, else `validation`). Non-admins cannot add bots. `include_creator` is for admins only (the admin panel sends `false`); for everyone else it is ignored and the creator joins.
* Events: every member gets `conversation_upsert`; people outside a public channel get `channel_directory_update` (below).
* **Channel manager** = conversation role `owner` (admins have the same power everywhere). A manager can `PATCH /conversations/{id}` (`name`, `topic`, `archived`, `visibility`; response is the caller's
  `ConversationSummary`), `PUT /conversations/{id}/avatar`, `POST /conversations/{id}/members {"user_ids"}`, `DELETE /conversations/{id}/members/{uid}` and always pin. Plain members get `403`,
  non-members `404`. A manager cannot remove another manager (admins can). Works for group DMs too (not `visibility`).
* `Conversation.visibility` is `"public"` or `"private"` on every conversation (always `private` except channels). Channels that existed before 1.1.0 stay `private`.
* `GET /conversations/{id}` returns one `ConversationSummary` exactly as the list does (members only, `404` otherwise).

### Public and private channels

* `GET /channels/browse?q=<text>&limit=<1..200, default 50>&offset=<n>` returns `ChannelBrowsePage` `{"channels": [{"id","name","topic"?,"avatar"?,"member_count","created_ms"}], "total", "has_more"}`:
  public, not archived channels the caller is **not** in, ordered by name; `q` matches name and topic case-insensitively. Private channels never appear.
* `POST /conversations/{id}/join` joins a public channel and returns its `ConversationSummary` (200; repeating it is a no-op). Private (or unknown) ids answer `404 not_found`, an archived channel `403 forbidden`.
  A system message "<name> joined" is posted; the joiner (all devices) and the members get `conversation_upsert`, everyone gets `message_new` for the system message. Fetch history with the normal messages endpoint.
* `POST /conversations/{id}/leave` leaves a channel or group DM (managers too; `validation` for DMs and notes to self, `404` when not a member). Returns `{"ok": true}`. The leaver's devices get
  `conversation_remove {conv_id}`, the others `conversation_upsert` and the system message "<name> left". `DELETE /conversations/{id}/members/{your id}` does the same.
* Event `channel_directory_update {conv_id, channel: ChannelBrowseItem | null}` goes to people who are not in the channel when a public channel is created, renamed, gets a topic or picture, is
  archived/unarchived, changes visibility or is deleted; `channel: null` means "remove it from the browse list". It is not sent for every join or leave (`member_count` is as of your last
  `GET /channels/browse`), and never for private channels.

### Pinning

`PUT|DELETE /conversations/{id}/pins/{message_id}` (list: `GET /conversations/{id}/pins`; event `pins_update`). Any member can pin and unpin in DMs, group DMs and notes to self. In channels any member can by
default; with `channel_pins = "managers"` only the channel's managers and admins can (plain members get `403 forbidden`). Non-members get `404`. At most 100 pins per conversation.

### Delete for me

`POST /messages/{id}/hide` (no body) hides one message (any message in a conversation you are in, your own or others', thread replies and thread roots too) for **you only**; other people keep it.
It is stored on the server, so every device of yours drops it. Returns `{"message_id", "conv_id"}`; repeating it is a no-op; non-members get `404`.
Events to **your** sessions only: `message_hide {conv_id, message_ids: [id]}` (the same event admin clear-history uses: remove the messages from cache and lists), then `read_update` (new `unread`/`mentions`)
and, for threads, `thread_update` (a hidden reply lowers `reply_count`; a hidden thread root arrives with `following: false`, remove the thread).
Hidden messages are left out of `GET /conversations/{id}/messages` (also bootstrap), `GET /messages/{id}`, `GET /conversations/{id}/threads/{root}`, `GET /threads` (thread roots too), `GET /search`, `GET /files`,
`GET /saved`, `GET /activity`, push wakes and every number in `GET /me/counts` and in `unread`. Not filtered: the pin list (a pin may point at a message you hid; skip those) and other people's reply counts.
There is no un-hide endpoint.

### Also send to channel (thread broadcast)

`SendMessage.also_send_to_channel: true` together with `thread_root` makes the reply show in the conversation's main timeline too (`validation` without a `thread_root`). It is **one message**, not a copy:
the same `id`, `seq`, edits, reactions and deletion; `Message.thread_root` stays set and `Message.also_send_to_channel` is `true` (the field is absent when false). The messages endpoint always returned thread replies;
apps put a message in the main timeline when `thread_root == null || also_send_to_channel` and in the thread view whenever `thread_root` is set. Render a broadcast reply as a reply with a reference to the thread
root (`thread_root` is its id; `GET /messages/{id}` gives the root's text if it is not cached). Scheduled messages and bots cannot set the flag.

### Counts, sessions

* `GET /me/counts` returns `Counts` (it is in the schema now): see "Filter pills" above.
* Own devices: `GET /sessions` returns `SessionInfo[]` (`id`, `device_name`, `client_version`, `ip`, `created_ms`, `last_seen_ms`, `current`), `DELETE /sessions/{id}` signs out one device,
  `POST /sessions/revoke-others` `{"revoked": n}` all others, `POST /sessions/revoke-all` everything incl. this one. For plain members these three need the admin's **Privacy options = Allow**
  (see Sign-out policy); otherwise `403 forbidden` (show no sign-out controls; `Me.privacy_controls_enabled` tells).

## Push registration (all apps)

`PUT /me/push` registers the calling device (one registration per session and kind; `GET /me/push` lists, `DELETE /me/push/{id}` removes). Pushes are
content-free wake-ups: ntfy and UnifiedPush get the body `wake`, FCM data `type=wake`, APNs a silent `content-available` push. They are sent only to members who have no live WebSocket,
are not in Do Not Disturb, have not muted the chat and want the notification level. After a wake the app syncs by cursor and posts its own local notification.

| Platform | Body of `PUT /me/push` |
|---|---|
| Android with the ntfy app or any UnifiedPush distributor (preferred) | `{"kind":"unifiedpush","target":"<endpoint URL the distributor gave the app>","device_name":"Pixel 8"}` e.g. `target` = `https://ntfy.example.com/upAbC123?up=1`. The server POSTs `wake` to that exact URL (so path prefixes, query strings and any distributor work, LAN addresses too). A 404 or 410 from it removes the registration. |
| ntfy, split form | `{"kind":"ntfy","target":"upAbC123","ntfy_url":"https://ntfy.example.com"}`: the endpoint `https://host/<topic>?up=1` cut into server URL and topic. `ntfy_url` may be omitted when the server has `WORKCHAT_NTFY_URL` / the admin setting (`GET /meta` shows it as `ntfy_url`). |
| Android with Google services | `{"kind":"fcm","target":"<FCM registration token>"}`; works once the owner has uploaded a Firebase service account in the admin panel (`GET /meta` `fcm_enabled`). |
| iOS | `{"kind":"apns","target":"<hex device token>"}`; works once the owner has entered the APNs key in the admin panel. Bundle id topic is `app.workchat`. |

Re-register after every login (registrations die with their session) and whenever the endpoint or token changes. `unifiedpush` targets must be `http(s)` URLs.

## Plain http on a local network

Nothing in the server needs https: sessions are bearer tokens (no cookies, no Secure flag), the WebSocket works as `ws://`, no redirects to https, no HSTS,
and one-time login links use `WORKCHAT_PUBLIC_URL` (or the saved address, else the address the browser used, honouring `X-Forwarded-Proto`), so
`http://192.168.1.20:8080` produces `http://192.168.1.20:8080/l/<token>` and `workchat://login?server=http://192.168.1.20:8080&token=...`. The admin panel and the
landing page also work without a secure context (copy and idempotency keys have fallbacks). What a plain-http install cannot do is anything the browser or phone
reserves for https (installing the panel as an app, browser notifications); the WorkChat apps allow cleartext only for private, loopback and `.local` hosts.
