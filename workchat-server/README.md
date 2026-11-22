# workchat-server

A self-hosted team chat server: one small Rust program (Docker) with a built-in admin web panel. It speaks a documented HTTP and WebSocket API
for chat clients, a Telegram-style bot API and webhooks for automation. Your data stays on your server and in the storage you choose
(S3, Cloudflare R2, Backblaze B2 or Google Drive). The display name defaults to "WorkChat".

This folder is the server and its admin panel. It lives in the WorkChat monorepo (https://github.com/uri-travoski/workchat-claude) next to the apps in `../Clients`.
Moving from the earlier SuperChat server install? See [docs/UPGRADING.md](docs/UPGRADING.md).

## Features

* **Accounts** are created by an admin only (name and password, plus a one-time login link that can be tapped, scanned as a QR code or pasted). No public sign-up.
  Roles: owner, admin, member. Disabling a user signs them out of every device at once.
* **Conversations**: public and private channels and group chats (anyone can create them unless an admin restricts channel creation; the creator manages them; public channels can be browsed and joined), direct messages and notes to yourself.
  Threads, reactions, edit and delete, mentions and `@handle` groups, pins, bookmarks, favourites, sections, mute and notify levels, drafts, save for later with labels, an Activity feed.
* **Cloud chats**: all messages and files live on the server; a new device pulls down the whole history. Per-user ordered event log with a cursor, WebSocket live updates, idempotent sends.
* **Files** up to 100 MB (configurable) in S3, Cloudflare R2, Backblaze B2, any S3-compatible store, Google Drive or local disk; **move all files** to another storage and verify them.
* **Search** across the chats a person belongs to, with filters (from, in, has link/file/reaction, before, after, type), people, files and channels.
* **Admin panel** (`/admin`): users, conversations, storage, sessions, backups, a chat viewer (pick a user, browse their chats, search by date range, delete single or bulk messages),
  bots, webhooks, AI providers, groups, bookmarks, an Updates card. Works on a phone.
* **Clear history, delete user and date-range delete** always write a per-user backup first and are refused if the backup fails; each person's copy of a chat is stored separately.
* **Encrypted backups** (age, passphrase) to local disk, S3, R2, B2 or Google Drive; scheduled or one click; selective restore (one user, several or all; merge or replace) and restore onto a new server.
* **Bots and automation**: Telegram-style bot API (`sendMessage`, `sendDocument`, `getUpdates`, `setWebhook`, ...), a native bearer-token API, incoming and outgoing (signed, retried) webhooks,
  interactive forms, a bot Home tab, scheduled messages, optional AI search and recaps (off by default).
* **Push**: content-free wake-ups through your own ntfy server, plus FCM and APNs support configured in the admin panel.
* **Works behind tunnels** (Cloudflare Tunnel) and reverse proxies, with a domain name or a bare IP address; with a tunnel no inbound port has to be opened on the host.
* No end-to-end encryption: TLS in transit (your proxy or tunnel), and admins can read messages (needed for the admin chat viewer and search). The original requirements are in [docs/SPEC.md](docs/SPEC.md).

## Quick start (Docker)

One container, one compose file, nothing built locally (the image `ghcr.io/uri-travoski/workchat-claude-server` is public). A reverse proxy, Cloudflare Tunnel and ntfy are not included:
run those separately ([how](docs/DEPLOY.md#reverse-proxy-tunnel-and-ntfy-run-separately)).

```sh
mkdir workchat && cd workchat
curl -fLO https://github.com/uri-travoski/workchat-claude/releases/latest/download/docker-compose.yml
curl -fL -o .env https://github.com/uri-travoski/workchat-claude/releases/latest/download/workchat-server.env.example
nano .env                    # at least WORKCHAT_PUBLIC_URL, the address your users will type
docker compose up -d
```

Then open `http://<host>:8080/admin`, create the owner account and pick the attachment storage ([how](docs/DEPLOY.md#attachment-storage-s3-r2-b2)).

**Update**: download the new `docker-compose.yml` (it pins that release's image) and run `docker compose pull && docker compose up -d`; or set
`WORKCHAT_IMAGE=ghcr.io/uri-travoski/workchat-claude-server:latest` in `.env` once. The admin panel's Overview says when a new release exists.

## Configuration

Every setting is a `WORKCHAT_*` environment variable with exactly that name; all are optional. In the compose file only some are passed through from `.env`
(see `deploy/.env.example`); add the others under `environment:`.

| Variable | Default | Meaning |
|---|---|---|
| `WORKCHAT_BIND` | `0.0.0.0:8080` | Address the server listens on (the image sets the same value). |
| `WORKCHAT_DATA_DIR` | `./data` (`/data` in the image) | Folder with the database `workchat.db`, `backup.key`, local files and temporary files. |
| `WORKCHAT_ADMIN_DIR` | `/app/admin`, else `admin-web/dist` when it has an `index.html` (`/srv/admin` in the image) | Folder with the built admin panel, served at `/admin`. |
| `WORKCHAT_PUBLIC_URL` | none | Public address with scheme, used in one-time login links. If unset, the address saved in the admin panel, else the request's Host header. |
| `WORKCHAT_DOWNLOADS_DIR` | `<data dir>/downloads` | Folder with client installers offered on the login-link page and at `/downloads`. |
| `WORKCHAT_RELEASE_URL` | `https://github.com/uri-travoski/workchat-claude/releases/latest` | Where the login-link page sends people when the downloads folder is empty. |
| `WORKCHAT_UPDATE_URL` | `https://api.github.com/repos/uri-travoski/workchat-claude/releases/latest` | Where the admin Updates card looks for the newest release. |
| `WORKCHAT_NAME` | `WorkChat` | Server name shown to clients. |
| `WORKCHAT_MAX_UPLOAD_MB` | `100` | Largest single attachment in MB. |
| `WORKCHAT_NTFY_URL` | none | Base URL of your own ntfy server (the admin panel's setting wins). |
| `WORKCHAT_BOOTSTRAP_ADMIN` | none | `user:password`; creates the owner account on an empty database. |
| `WORKCHAT_BOOTSTRAP_EMAIL` | none | Email address stored on that owner account. |
| `WORKCHAT_BACKUP_PASSPHRASE` | none (a random one is written to `<data dir>/backup.key`) | Passphrase for encrypted backups. |
| `WORKCHAT_LOCKOUT_THRESHOLD` | `5` | Failed logins before a temporary lockout (per username and per IP). |
| `WORKCHAT_LOCKOUT_SECS` | `300` | Length of that lockout in seconds. |
| `WORKCHAT_ACCESS_TTL_SECS` | `900` | Access token lifetime in seconds. |
| `WORKCHAT_RATE_PER_SEC` | `40` | Sustained request rate per client. |
| `WORKCHAT_RATE_BURST` | `200` | Request burst size per client. |
| `WORKCHAT_EVENT_RETENTION_DAYS` | `90` | Per-user event log pruning; clients offline longer re-bootstrap. |
| `WORKCHAT_BACKUP_WORK_FACTOR` | age default (about 1 s) | scrypt work factor (log2 N) for backup encryption. Lower only in tests. |
| `WORKCHAT_WORKER_TICK_MS` | `1000` | How often the scheduled-message and webhook workers look for work. |
| `WORKCHAT_WEBHOOK_RETRY_BASE_MS` | `10000` | First retry delay for webhook deliveries (later retries multiply by 3, capped at one hour). |
| `WORKCHAT_WEBHOOK_MAX_ATTEMPTS` | `8` | Delivery attempts per webhook event. |
| `WORKCHAT_TEST_BIND` | `127.0.0.1:0` | Test hook: address used by `serve_for_test`. Not read by the server binary. |

Not read by the server, used by `deploy/docker-compose.yml` only: `WORKCHAT_IMAGE`, `WORKCHAT_PORT`, `WORKCHAT_BIND_ADDR`, `WORKCHAT_DATA_VOLUME`, `WORKCHAT_LOG_LEVEL` (passed on as `RUST_LOG`)
and `WORKCHAT_NETWORK`. The log filter itself is the standard `RUST_LOG` (default `info,tower_http=warn`).

## Build from source

Needs Rust (stable), Node 22 and, for the image, Docker.

```sh
(cd admin-web && npm ci && npm run build)              # admin panel -> admin-web/dist
cargo build --release                                  # target/release/workchat-server
WORKCHAT_ADMIN_DIR=admin-web/dist WORKCHAT_DATA_DIR=./data target/release/workchat-server
cargo test --workspace                                 # unit tests + the integration suite (in-process server, no docker)
cargo run -p workchat-proto --bin gen-schema           # regenerate schema/api.schema.json from the Rust types
(cd admin-web && npm run gen-types)                    # regenerate admin-web/src/api-types.ts from that schema
docker build -f docker/Dockerfile -t workchat-server .  # server image (context is this folder)
docker compose -f docker/docker-compose.build.yml up -d --build   # throw-away dev instance on 127.0.0.1:8080
```

The API types live once in `crates/proto`; the JSON schema and the admin panel's TypeScript types are generated from them.

## Layout

```
Cargo.toml, Cargo.lock    own workspace (crates/server, crates/proto, crates/testkit, tests/e2e)
crates/server/            the server: library `workchat_server` and the `workchat-server` binary; its tests are in crates/server/tests
crates/proto/             API types, rules, release helpers, the `gen-schema` binary
crates/testkit/           test-only HTTP and WebSocket client (raw API calls, admin calls, `Ws`), shared by the tests below
tests/e2e/                docker end-to-end tests (server image + MinIO + fake Google Drive) and run.sh
tests/ui/                 Playwright tests of the admin panel and run.sh
tools/seed/               sample-data generator and seeder (people, channels, chats, files) for demos and app testing
admin-web/                admin panel (TypeScript + Vite), built into the image
schema/api.schema.json    generated API JSON schema
assets/icon/              icon master (SVG)
docker/                   Dockerfile (build context = this folder) and a developer compose file
deploy/                   docker-compose.yml and .env.example: the two files attached to a release
docs/                     API, ADMIN, BOTS, DEPLOY, DECISIONS, SPEC, UPGRADING
.github/workflows/        ci.yml, release.yml (tag v*: GitHub release + ghcr.io image)
```

## Tests

Everything below runs from this folder and touches nothing outside it.

| What | Command | Needs |
|---|---|---|
| Unit tests and the integration suite (about 100 tests: login and sessions, lockout, login links, roles, disable and kick, delete user, sign-out, channels, DMs, threads, reactions, pins, saved items, search, files and thumbnails, storage move and verify, backups and restore, admin chat viewer, bots, webhooks, forms, scheduled messages, AI, push, the Updates card) | `cargo test --workspace` | Rust; the admin panel need not be built |
| End to end: the real image, MinIO (S3), a fake Google Drive, backup restored onto a second empty server | `tests/e2e/run.sh` | Docker and network access to pull the MinIO image. Uses host ports 18300, 18301, 18302, 18310 (never 8080) and removes what it starts |
| Admin panel in a browser (setup, users, roles, dialogs, chat viewer, bots and webhooks, phone layout, Updates card) | `tests/ui/run.sh` (one test: `tests/ui/run.sh admin-roles`) | Node 20+, a Chromium in `~/.cache/ms-playwright`, Java for the QR check (skipped if the decoder cannot be fetched). Ports 18311 to 18315, 18321, 18322 |

The integration tests start the server in-process on an ephemeral port (`Config::for_test`, `build_state`, `serve_for_test`) and drive it over raw
HTTP and WebSocket with `crates/testkit`; they use local fakes for webhook receivers, AI providers, ntfy/FCM/APNs and the GitHub release API.
The e2e tests skip themselves unless `WORKCHAT_E2E` is set, so `cargo test --workspace` never needs docker. CI runs `cargo test --workspace` on every push;
the e2e and ui jobs run when you start the workflow by hand.

## Documentation

[docs/DEPLOY.md](docs/DEPLOY.md) (install, update, proxy, storage) · [docs/ADMIN.md](docs/ADMIN.md) (admin guide) · [docs/BOTS.md](docs/BOTS.md) (bots, webhooks, forms, AI) ·
[docs/API.md](docs/API.md) (HTTP API) · [docs/DECISIONS.md](docs/DECISIONS.md) · [docs/SPEC.md](docs/SPEC.md) · [docs/UPGRADING.md](docs/UPGRADING.md)

## Licence

MIT, see [LICENSE](LICENSE). Third-party material in the binary and image: [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt).
