# Specification

The owner's requirements for workchat-server, tidied from the original brief. See [DECISIONS.md](DECISIONS.md) for how each was built and
[README.md](../README.md) for the feature overview.

## Goal

A lightweight, self-hosted chat server with ntfy, Apple and Google push notification support. It must work behind tunnels (Cloudflare) and
without one, with domain names and with direct IP addresses, and it must be possible to run it without opening a port on the VPS it is hosted on
(through a tunnel).

## Messaging and storage

* End-to-end encryption based on the user login was requested; chats are also readable on the server by the administrator. (As built, there is no
  end-to-end encryption: the administrator can read chats. See DECISIONS.md, "Transport security".)
* Unlimited retention of messages and attachment links.
* Cloud chats: all messages and files live on the server, as in Telegram, and a new or second device pulls down all old chats and attachments.
* Any file type up to 100 MB (photos, videos, voice notes, documents).
* Attachments are stored only in Google Drive, S3, Cloudflare R2 or Backblaze B2 (the choice of storage is made on initial setup).
* Files can be moved between storages, and the server still finds them as before.
* Only configuration, accounts and the text chat database live on the server's disk.
* A disabled user is logged out and their old chats are removed from the phone.

## Bots and automation

* Telegram-style bots and API tokens are created in the admin panel.
* Telegram-style endpoints (`/sendMessage`, `/sendDocument`, `/setWebhook`, `/getUpdates`) for apps, receipt loggers and AI agents.

## Backups

* Automatic and one-click encrypted backups to the selected storage (local, Google Drive, S3, Cloudflare R2 or Backblaze B2).
* Restore onto a new server and reconnect to the existing storage so old chats and media work immediately.

## Deployment

* Hosted with docker compose.
* ntfy can be on the same Docker network.

## Admin panel and users

* Create users (name and password) directly in the admin panel; a one-time link is sent to the user, who copies and pastes it on a client's login page.
* Disabling a user ends their sessions and signs them out of the clients straight away.
* One-click "Clear history": wipes the user's server-side chats and cloud attachments and forces a re-login.
* Admin chat viewer in the style of a desktop chat client (a web app served by the server): a user dropdown, photos, videos and files shown inline, using
  the full window. The admin can pick a user, channel or group, see all their chats and history, search between dates, and delete individual or bulk
  messages and attachments, filter messages between dates or before a date to delete them, and do everything a normal user can, plus more functions
  such as search, filter and delete.
* Each side of a chat is stored separately: clearing or deleting one user leaves the other person's copy.
* Date-range delete: delete messages between two dates, with an optional "also delete the other person's copy" (off by default).
* Per-user backup button: saves one user's account, contacts and chats.
* Selective restore: one user, several, or all, merging by default with a "replace" option.
* Automatic backup before Clear history or Delete: the action is refused if the backup fails.
* Attachments kept in the backup folder: moved there instead of erased on Clear history or Delete, and moved back on restore.
