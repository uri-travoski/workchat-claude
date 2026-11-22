# Admin guide

Open `https://your-server/admin` and sign in with an owner or admin account.

* **Users**: create (optionally with a password; a one-time login link is always produced), disable (the person is signed out
  of every device immediately and their client wipes local data), enable, reset password (signs out all devices; produces a link if
  no password is typed), login link, back up, **clear history**, delete.
* **One-time login link**: a normal web address on your server, `https://<your-server>/l/<token>`, shown with **Copy link**, **Share**
  (where the browser offers sharing) and a **QR code** that encodes the same address. Tapping the link (or pointing a phone camera at the QR
  code) opens a page that tries to open the WorkChat client app (`workchat://login?...`) and sign the person in; if the app is not installed the page offers the downloads.
  The page only checks the link, it does not use it up; the link works once and expires after 7 days. The client then asks them to choose a password.
  The person can also paste the link on the client's sign-in page.
* **Address used in login links** (Users page, owner only): the link is built from `WORKCHAT_PUBLIC_URL` when set, else from the address saved here,
  else from the address you used to open the admin panel (so it works on a bare LAN address such as `http://192.168.1.10:8080`).
* **Downloads**: the landing page offers files found in the server's downloads folder (`WORKCHAT_DOWNLOADS_DIR`, default `<data>/downloads`;
  `.apk`, `.AppImage`/`.deb`/`.rpm`, `.exe`/`.msi`) and lists them at `/downloads` under the heading "Client downloads". The card always ends with
  "Open latest release on GitHub", a link to `WORKCHAT_RELEASE_URL` (default `https://github.com/uri-travoski/workchat-claude/releases/latest`, never a
  specific tag), also when the folder is empty.
* **Updates card** (Overview): this server's version against the newest GitHub release, update commands, and a "Client downloads" list (heading without a
  version; "Latest release: vX.Y.Z" is a small label under it). "Open latest release on GitHub" is shown in every state: up to date, update available,
  no release yet, GitHub unreachable or rate limited, and when the admin panel cannot reach the server. `GET /admin/updates` returns it as `release_url`.
* **Phones**: the whole panel works at phone width (menu button, cards instead of tables, single-pane chat viewer with a back button).
* **Role** (owner only): each other user has a Member / Admin / Owner menu in the Users table (and on the phone cards); a confirmation says what changes.
  The change applies at once, even to devices that are signed in, and their apps update without a restart. Rules: admins see roles but cannot change them;
  nobody can change their own role; at least one owner always remains (make someone else an owner before an owner steps down); bots are always members;
  disabled users can still be changed. Restoring a deleted user from a backup brings back the role in the backup (an "owner" in a backup returns as admin when another owner exists).
* **Clear history** always writes a per-user backup first and refuses to proceed if the backup fails. It removes only that
  person's copy of every chat (other participants keep theirs), moves files nobody else can see into
  `backups/users/<user>/<backup>/att/` in your storage, and signs the person out everywhere.
* **Conversations**: channels and group DMs. Channels are private (members only): people are in one only because an owner, admin or manager added them. Admins can create them here (without joining), rename, archive and manage members.
* **Settings**: *Who can create channels* (everyone, the default, or admins only) and *Who can pin messages in channels* (every member, the default, or channel managers and admins only). Saved as soon as you change a menu; apps update live.
* **Storage**: choose backend (local, S3-compatible incl. R2 and B2, Google Drive), Test, Save (only before any file exists),
  **Move all files** to another backend (copy, verify size and checksum, switch, delete old copies; uploads pause meanwhile),
  Verify (quick or deep).
* **Sessions**: every signed-in device with last seen; sign any out.
* **Backups**: list of per-user backups.
* **Privacy options** per user (Allow / Disallow): with Allow, the user sees Privacy settings (hide last-active) and can sign out / sign out of all devices; with Disallow (the default) their apps show no sign-out controls anywhere and the server refuses self-service sign-out (`forbidden`, "sign-out is not allowed for this account"), so people cannot remove their own session by accident. Their sessions end only when you revoke them (Users, Sessions), disable the account, reset the password or clear history. Admins and owners can always sign out. Changing the setting reaches open apps at once.

## Chat viewer, bulk delete, backups

* **Chats** (viewer): pick a user, then a channel, group or DM from the dropdowns; the timeline looks like a chat app (inline pictures,
  reactions, thread reply counts). Search with a date range inside the conversation or across everything. An admin can post in conversations
  they are a member of.
* **Bulk delete**: select messages (or a date range), choose "for everyone" or "only this person's copy" (plus the other participant's copy if wanted).
  A backup is written first; if it fails nothing is deleted. Files that nobody can see any more move into the backup folder.
* **Backups**: back up now, set the schedule and target, download, and restore. Restore a single person (merge keeps what they have now, replace makes
  their chats match the backup), a message backup (put deleted messages and files back), or everything. On a brand new server the setup page offers
  "restore from backup" before creating an owner.
* The backup passphrase lives in `<data>/backup.key` (created on first start) or in `WORKCHAT_BACKUP_PASSPHRASE`. It is never stored inside a backup. Copy it
  somewhere safe: without it the backups cannot be read.
* **Push**: Settings > Push holds the ntfy server, FCM service account and APNs key (masked once saved).

## Automation pages

* **Bots**: create a bot (username, name, description, conversations to add it to); the token is shown once on creation and can be shown or regenerated later
  (the old one stops at once). Membership: add or remove the bot per conversation. The card shows its webhook (URL, secret set, last error) or the
  number of updates waiting for polling. See [BOTS.md](BOTS.md).
* **Webhooks**: incoming (a secret URL per conversation; "New URL" rotates it) and outgoing (URL, secret, event filter, per-conversation filter, status, "Send test").
* **AI**: any number of providers (Anthropic, OpenAI, OpenAI-compatible with a base URL): add, edit, delete, **Test**, enable. Keys are write-only (the panel
  shows only the last four characters). Choose a default provider for AI search and another for recaps, or leave them on automatic: the first enabled provider is
  used and the next one takes over when it fails. Only the owner changes providers. Off by default; the page repeats that message text goes to the provider.
* **Groups**: `@handle` groups (members, rename, delete). Typing `@handle` mentions everyone in the group.
* **Bookmarks**: pick a conversation and add, edit or remove its link bookmarks (admins can edit any conversation).
* **Chats**: the viewer marks bot messages with APP, draws forms as cards, and lists scheduled messages of the open conversation.
