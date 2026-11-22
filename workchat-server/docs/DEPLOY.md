# Deploying workchat-server

The server is one container (Rust binary plus the admin panel). Everything that must survive lives in one volume, `/data`
(SQLite database `workchat.db`, settings, local files if you use local storage, backups). Attachments normally live in your chosen
storage (S3, R2, B2, Google Drive), not on the server's disk.

Moving from the earlier server of this project? Read [UPGRADING.md](UPGRADING.md) first.

## Quick start (install)

The server is **one container, one compose file**: `docker-compose.yml` has a single service, `workchat-server`, which pulls the published image
`ghcr.io/uri-travoski/workchat-claude-server` (a **public** package: no login, nothing is built on your machine). There is no reverse proxy,
tunnel, ntfy or object store in it; those run separately (see below). You need Docker with compose and two files from the latest GitHub Release:

```
mkdir workchat && cd workchat
curl -fLO https://github.com/uri-travoski/workchat-claude/releases/latest/download/docker-compose.yml
curl -fL -o .env https://github.com/uri-travoski/workchat-claude/releases/latest/download/workchat-server.env.example
nano .env                     # set WORKCHAT_PUBLIC_URL at least; every variable is explained in the file
docker compose up -d          # http://<this machine>:8080/admin  -> setup wizard
```

(From a git checkout the same two files are `deploy/docker-compose.yml` and `deploy/.env.example`. The compose file works with `docker compose` and `docker-compose`.)

Nothing in `.env` is required: with no `.env` at all the server starts on port 8080 with the defaults below. The first visit to `/admin` shows the setup
wizard: owner account plus attachment storage (with a Test button). Alternatively set `WORKCHAT_BOOTSTRAP_ADMIN=user:password` in `.env` to create the owner on first start.

## Updating the server

Every release publishes a `docker-compose.yml` whose default image is **that release's version** (pinned on purpose, so nothing moves under you). To update:

```
curl -fLO https://github.com/uri-travoski/workchat-claude/releases/latest/download/docker-compose.yml     # the new pinned version
docker compose pull && docker compose up -d
```

or, without touching the file, set the image in `.env` and run the same two compose commands:

```
WORKCHAT_IMAGE=ghcr.io/uri-travoski/workchat-claude-server:1.4.0     # a specific release (tags are the release numbers without the v)
WORKCHAT_IMAGE=ghcr.io/uri-travoski/workchat-claude-server:latest    # always the newest release: docker compose pull is then the whole update
```

Run them in the folder with `docker-compose.yml`. The data volume is kept; the container restarts on the new image in a few seconds and the
database is migrated on start. The admin panel's Overview has an **Updates** card that says whether a newer release exists, shows its release notes, the
commands and links to that release's downloads. Take a backup first if you want a way back. To go back one version, pin the older tag (a newer database
is not downgraded, so restore a backup if a release changed the schema; the release notes say when).

## Reverse proxy, tunnel and ntfy run separately

workchat-server only serves plain HTTP on container port 8080 (published on the host as `WORKCHAT_PORT`, default 8080). HTTPS, a Cloudflare Tunnel or any other front door is
yours to run, in its own container or on the host. What the server needs from it:

* **Point it at the server**: `http://<host>:8080` (the host's address, or `http://127.0.0.1:8080` if cloudflared runs on the host), or `http://workchat-server:8080` when the
  proxy shares a Docker network with the server (see the `networks:` example at the bottom of `docker-compose.yml`: create the network once, `docker network create proxy`,
  uncomment the lines, and the published port can be dropped). With a proxy on the same machine set `WORKCHAT_BIND_ADDR=127.0.0.1` so the plain-HTTP port is not exposed.
* **Pass everything through**, in particular the **WebSocket** at `/api/v1/ws` (live messages). Cloudflare Tunnel does this by default; for nginx, Caddy or Traefik make sure the
  `Upgrade` header is forwarded (Caddy's `reverse_proxy` and Traefik do it automatically; nginx needs `proxy_set_header Upgrade $http_upgrade; proxy_set_header Connection "upgrade";`).
  `/l/` (login links) and `/downloads` must reach the server too. Allow request bodies of at least `WORKCHAT_MAX_UPLOAD_MB`.
* **Set `WORKCHAT_PUBLIC_URL`** to the public https address (`https://chat.example.com`); it goes into one-time login links.
* **ntfy** (optional push): run your own ntfy server separately and set `WORKCHAT_NTFY_URL` to its base URL, for example `https://ntfy.example.com`,
  `http://host.docker.internal:2586` (ntfy published on the same host: the compose file maps that name to the host) or `http://ntfy:80` (shared Docker network).
  The server only sends a wake-up to people who have no open connection, and the wake never carries message text.
* A bare IP with a self-signed certificate works too, if the client supports trusting it.

| Situation | What to do |
|---|---|
| A domain, ports 80/443 open | your own reverse proxy (Caddy, nginx, Traefik) in front of `:8080`; `WORKCHAT_PUBLIC_URL=https://chat.example.com` |
| No open ports | your own cloudflared; hostname -> `http://<host>:8080` or `http://workchat-server:8080`; `WORKCHAT_PUBLIC_URL=https://chat.example.com` |
| LAN testing only | nothing extra: `http://<lan-ip>:8080`. Clients may refuse plain http; use only on a trusted network. |

## Attachment storage (S3, R2, B2)

No object store is shipped: the server talks to yours. Choose it in the admin panel on first setup (or later under **Storage**), fill the form and press **Test**:

* **Cloudflare R2**: create a bucket and an R2 API token with object read and write. Endpoint `https://<account-id>.r2.cloudflarestorage.com`, region `auto`, the token's access key ID and secret.
* **Backblaze B2**: create a bucket and an application key with access to it. Endpoint `https://s3.<region>.backblazeb2.com` (the region is in the bucket's endpoint on the B2 page, for example `us-west-004`), region the same string.
* **Amazon S3**: leave the endpoint empty, set the region, bucket and an IAM user's keys. Any other S3-compatible service works with its endpoint (tick path-style if it needs it).
* **Google Drive** (OAuth client, refresh token with the `drive.file` scope) and **local disk** (inside the `/data` volume; for testing and small installs) are the other choices.

## Configuration (environment)

The compose file passes the variables below from `.env`. The server reads each of its settings as `WORKCHAT_<NAME>` (one name, no alternatives). The complete list,
with defaults, is in the README; this is the part you normally touch.

| Variable | Default | Meaning |
|---|---|---|
| `WORKCHAT_PUBLIC_URL` | none | Address clients use; goes into one-time login links (`<url>/l/<token>`). When unset the owner can save one in the admin panel, else it is taken from the request's Host header. |
| `WORKCHAT_NAME` | WorkChat | Server name shown in clients. |
| `WORKCHAT_BOOTSTRAP_ADMIN` | none | `user:password` of the owner to create when no users exist. |
| `WORKCHAT_MAX_UPLOAD_MB` | 100 | Largest single attachment. |
| `WORKCHAT_NTFY_URL` | none | Base URL of your own ntfy server. |
| `WORKCHAT_BACKUP_PASSPHRASE` | file `<data>/backup.key` | Passphrase for encrypted backups (age, scrypt). If unset one is generated into `backup.key`. Keep a copy elsewhere; it is never inside a backup. |
| `WORKCHAT_LOG_LEVEL` | `info,tower_http=warn` | Compose passes it as `RUST_LOG`. |
| `WORKCHAT_IMAGE` | `ghcr.io/uri-travoski/workchat-claude-server:<release>` | Image to run (compose only). |
| `WORKCHAT_PORT`, `WORKCHAT_BIND_ADDR` | `8080`, `0.0.0.0` | Host port and address that publish the server (compose only). |
| `WORKCHAT_DATA_VOLUME` | `workchat-data` | Name of the Docker volume mounted at `/data` (compose only). |
| `WORKCHAT_NETWORK` | `proxy` | Name of the external Docker network, only when you enable the `networks:` block (compose only). |

Further settings the server understands (not in `.env.example`; add them under `environment:` in the compose file if you need them):

| Variable | Default | Meaning |
|---|---|---|
| `WORKCHAT_DOWNLOADS_DIR` | `<data>/downloads` | Folder with client installers (`.apk`, `.AppImage`, `.deb`, `.exe`, ...) offered by the login link page and `/downloads`. |
| `WORKCHAT_RELEASE_URL` | this project's GitHub releases | Where the login link page sends people when the downloads folder is empty. |
| `WORKCHAT_UPDATE_URL` | GitHub API `releases/latest` of this project | Where the admin Updates card looks for a newer release (point it at a mirror or a fake in tests). |
| `WORKCHAT_ACCESS_TTL_SECS` | 900 | Access token lifetime. |
| `WORKCHAT_LOCKOUT_THRESHOLD` / `WORKCHAT_LOCKOUT_SECS` | 5 / 300 | Failed logins before a temporary lockout (per username and per IP). |
| `WORKCHAT_RATE_PER_SEC` / `WORKCHAT_RATE_BURST` | 40 / 200 | Per-client request rate limit. |
| `WORKCHAT_EVENT_RETENTION_DAYS` | 90 | Event log pruning. Clients that were offline longer re-bootstrap. |
| `WORKCHAT_BOOTSTRAP_EMAIL` | none | Email address stored on the owner created by `WORKCHAT_BOOTSTRAP_ADMIN`. |
| `WORKCHAT_BIND`, `WORKCHAT_DATA_DIR`, `WORKCHAT_ADMIN_DIR` | `0.0.0.0:8080`, `/data`, `/srv/admin` | Set by the image; normally untouched. |

Test-only knobs (do not set in production): `WORKCHAT_BACKUP_WORK_FACTOR`, `WORKCHAT_WORKER_TICK_MS`, `WORKCHAT_WEBHOOK_RETRY_BASE_MS`,
`WORKCHAT_WEBHOOK_MAX_ATTEMPTS`, `WORKCHAT_TEST_BIND`.

## Backups of the server itself

Back up the data volume (`workchat-data`, or whatever `WORKCHAT_DATA_VOLUME` names) (stop the container or copy the SQLite file with `sqlite3 .backup`). Per-user backups made by the
admin panel are stored in your attachment storage under `backups/`, not on the server disk, and survive a server rebuild.

### Restoring onto a new server

1. Start a fresh container with an empty data volume and open `/admin/`: the setup page offers "Restore from backup".
2. Enter the same attachment storage settings (the backups and attachments live there) and the backup passphrase, or upload the `.age` file.
3. Users, chats, settings and sessions come back; attachments are reconnected in place. Every client re-syncs automatically (event ids jump and clients re-bootstrap).

Push (optional): ntfy needs only a URL (see "Reverse proxy, tunnel and ntfy run separately"). FCM needs a Google service account JSON and APNs a `.p8` key, both entered in the admin panel; wakes never contain message text.

## Building the image (developers only)

Normal installs never build: they pull the published image. The Dockerfile lives in `docker/` (not in `deploy/`, which holds only the two files of a release). The build context is the repository root:

```
docker build -f docker/Dockerfile -t workchat-server:1.4.0 .
docker tag workchat-server:1.4.0 ghcr.io/uri-travoski/workchat-claude-server:1.4.0     # what release.yml publishes; lets deploy/docker-compose.yml run it offline
docker compose -f docker/docker-compose.build.yml up -d --build                  # or: build and run a throw-away dev instance (port 8080, volume workchat-dev-data)
```
The image is multi-stage (node for the admin panel, rust for the server, debian-slim runtime with ca-certificates and tini,
non-root user `workchat` with uid 10001, HEALTHCHECK).

## Update checks

The server asks GitHub for the newest release for the admin panel's Updates card (`GET /api/v1/admin/updates`, admins only, cached for an hour, "Check again" allowed once a minute).
Only the release list is read (`https://api.github.com/repos/uri-travoski/workchat-claude/releases/latest`) and the only thing sent is `User-Agent: WorkChat-Server/<version>`:
no account, device or server information. Drafts and pre-releases are ignored. The *server* asks GitHub, not the browser, so the admin pages keep their strict
Content-Security-Policy. If the server has no outbound internet the card says so and still shows the commands. Change the address with `WORKCHAT_UPDATE_URL`.
