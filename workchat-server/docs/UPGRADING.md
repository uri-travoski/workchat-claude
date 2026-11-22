# Moving an existing SuperChat server install to workchat-server

workchat-server 1.0.0 is the server of the earlier SuperChat project (its last server release was 1.4.1, earlier ones were called Atomic Chat),
split out into its own project and renamed. The database layout and the HTTP API are unchanged, so **your users, chats, settings and attachments carry over**.
What changed are names: environment variables, the image, the compose project, the database file name, webhook headers and the admin panel's
browser storage key. There are **no compatibility shims**: workchat-server does not read the old variable names and does not look for the old database file.

This is the only document that uses the old names.

## What changes at a glance

| What | Old | New |
|---|---|---|
| Image | `ghcr.io/uri-travoski/superchat-server` | `ghcr.io/uri-travoski/workchat-claude-server` |
| Environment variables | `SUPERCHAT_*` (or the older `ATOMIC_*`) | `WORKCHAT_*` (one name each) |
| Compose project / service | `superchat` / `superchat` (container `superchat-superchat-1`) | `workchat` / `workchat-server` (container `workchat-workchat-server-1`) |
| Docker volume (default) | `superchat-data` (a 1.0-era install: `atomic-chat_atomic-data`) | `workchat-data`, or keep the old one with `WORKCHAT_DATA_VOLUME` |
| Database file in `/data` | `atomic.db` (+ `atomic.db-wal`, `atomic.db-shm`) | `workchat.db` (+ `-wal`, `-shm`) |
| Container user | `superchat` (uid 10001) | `workchat` (uid 10001, same number) |
| Default server name | `SuperChat` | `WorkChat` |
| Release asset with the settings | `superchat.env.example` | `workchat-server.env.example` |
| Outgoing webhook headers | `X-SuperChat-Event`, `-Delivery`, `-Timestamp`, `-Signature` (and the 1.0 `X-Atomic-*` copies) | `X-WorkChat-Event`, `X-WorkChat-Delivery`, `X-WorkChat-Timestamp`, `X-WorkChat-Signature` only |
| Webhook `User-Agent` | `superchat/<version>` | `workchat/<version>` |
| Admin panel session key (browser storage) | `superchat-admin-session` | `workchat-admin-session` |
| Backup `format` field (in newly written user and message backups) | `superchat-user-backup`, `superchat-messages-backup` | `workchat-user-backup`, `workchat-messages-backup` |
| Login link scheme the landing page opens | `superchat://` (Android package `app.superchat`) | `workchat://` (Android package `app.workchat`) |
| Update check (Updates card) | `github.com/uri-travoski/superchat` | `github.com/uri-travoski/workchat-claude` |
| Version number | 1.4.1 | 1.0.0 (a new version line) |

## Steps

Plan for a few minutes of downtime. Do it on the machine that runs the old stack. `OLD` is the folder with the old `docker-compose.yml`.

### 1. Find the data volume and stop the old server

```sh
cd OLD
docker compose ps                       # note the container name, usually superchat-superchat-1
docker inspect superchat-superchat-1 --format '{{range .Mounts}}{{.Name}} -> {{.Destination}}{{"\n"}}{{end}}'
```

The volume mounted at `/data` is the one to keep: normally `superchat-data`; a 1.0-era install has `atomic-chat_atomic-data`. Write it down as `<vol>`.

Make a copy first (this is your way back; it also copies the backup passphrase file `backup.key`):

```sh
docker compose stop
docker run --rm -v <vol>:/data -v "$PWD":/backup debian:trixie-slim tar czf /backup/data-before-workchat.tgz -C /data .
```

### 2. Rename the database inside the volume

The database is the only file in `/data` that carries the old name. Everything else (`backup.key`, `files/`, `downloads/`, `tmp/`) keeps its name.
The server runs as uid 10001 in both images and `mv` keeps ownership, so no `chown` is needed:

```sh
docker run --rm -v <vol>:/data debian:trixie-slim sh -c '
  cd /data && ls -ln &&
  for s in "" -wal -shm; do [ -e "atomic.db$s" ] && mv -v "atomic.db$s" "workchat.db$s"; done
  ls -ln'
```

Check that `workchat.db` is there and owned by `10001`. If, for any reason, a file is owned by someone else (for example after a restore from a tar made as root),
fix it with `docker run --rm -v <vol>:/data debian:trixie-slim chown -R 10001 /data`.
If you start workchat-server before renaming, it silently creates a new **empty** `workchat.db` and shows the setup wizard: stop it, delete that empty file, and do this step.

### 3. Set up the new compose folder

Use a new folder; keep `OLD` until you are happy.

```sh
mkdir workchat && cd workchat
curl -fLO https://github.com/uri-travoski/workchat-claude/releases/latest/download/docker-compose.yml
cp OLD/.env .env                                                  # adjust the path to your old folder
```

Rename the keys in `.env` (both old prefixes) and fix the values that mention old names:

```sh
sed -i -E 's/^(SUPERCHAT|ATOMIC)_/WORKCHAT_/' .env
sed -i -E '/^WORKCHAT_IMAGE=/d' .env                              # the old value points at the old image; the new compose file pins its own
sed -i -E 's/^WORKCHAT_NAME=SuperChat$/WORKCHAT_NAME=WorkChat/' .env   # only if you never chose a name of your own
```

Keep the old data volume: put its name into `.env` (this replaces an existing line, or adds one):

```sh
sed -i '/^WORKCHAT_DATA_VOLUME=/d' .env && echo 'WORKCHAT_DATA_VOLUME=<vol>' >> .env
```

(With the default volume name `workchat-data` you would get a new, empty volume instead; to move the data into it, copy the volume's content with
`docker run --rm -v <vol>:/from -v workchat-data:/to debian:trixie-slim cp -a /from/. /to/`.)

Variables that exist only in compose (`WORKCHAT_PORT`, `WORKCHAT_BIND_ADDR`, `WORKCHAT_LOG_LEVEL`, `WORKCHAT_NETWORK`) keep working after the rename. The full variable list is in
the [README](../README.md#configuration). If you also set variables by hand under `environment:` in an edited old compose file, rename them the same way.

### 4. Start it and check

```sh
docker compose up -d
docker compose ps                          # workchat-server should become "healthy"
docker compose logs --tail 20
curl -fsS http://127.0.0.1:8080/api/v1/meta
```

Open `/admin` and sign in with your existing owner account (the setup wizard must **not** appear; if it does, the volume or the database rename is wrong, see step 2).
Then remove the old stack: `cd OLD && docker compose down` (this does not delete the volume; do not run `down -v`). The old container also used host port 8080, so it
has to be stopped before the new one can start on the same port; if you changed `WORKCHAT_PORT`, both can run side by side only on different ports, but never on the same volume.

### 5. Things that point at the old names

* **Reverse proxy or tunnel on a shared Docker network**: it used `http://superchat:8080`; change it to `http://workchat-server:8080` (and the network name if you set one).
  A proxy that targets `http://<host>:8080` needs no change.
* **Bots and webhook receivers**: outgoing webhooks now carry only `X-WorkChat-Event`, `X-WorkChat-Delivery`, `X-WorkChat-Timestamp` and `X-WorkChat-Signature`
  (`sha256=<hex>`, HMAC-SHA256 over `"<timestamp>.<body>"` with the webhook secret; same algorithm and secrets as before). Receivers that check `X-SuperChat-*` or
  `X-Atomic-*` headers must read the `X-WorkChat-*` ones, otherwise signature checks fail. Telegram-style bot webhooks (`X-Telegram-Bot-Api-Secret-Token`), bot tokens,
  incoming webhook URLs and the bot API paths are unchanged.
* **Admins sign in again**: the admin panel keeps its session in the browser under a new key, so each admin signs in once more. Users' sessions on their devices are stored in the
  database and are not affected.
* **Login links**: links already issued (`https://<server>/l/<token>`) stay valid. The landing page now offers to open the app through `workchat://`, so it does not open an old SuperChat app (see below).
* **Cloned configs**: anything of yours that sets `SUPERCHAT_*`/`ATOMIC_*` variables (scripts, other compose files, CI) has to use the `WORKCHAT_*` names.
* **Google Drive storage**: an already configured Drive keeps the folder name stored in its settings (a new Drive setup defaults to `WorkChat`; the old default was `SuperChat`). The server finds its folder by that name, so if the stored Drive settings have an empty folder name, open Storage in the admin panel before anything is uploaded and check that the folder name reads `SuperChat`, otherwise a new, empty `WorkChat` folder would be used. Do not rename the folder of a working install.

### Old backups and the backup passphrase

* **Old backup files stay restorable.** The `format` field written into user and message backups is informational: restore reads the documents' contents and does
  not check that field, and full-server backups (`backups/full/*.db.age`) are encrypted database snapshots without any name inside. Backup file names and the folder layout
  in your storage target (`backups/users/...`, `backups/full/...`) did not change, so the new server finds and lists the existing ones.
* The passphrase is unchanged: it is still the file `backup.key` in the data volume (kept by step 2), or `WORKCHAT_BACKUP_PASSPHRASE` if you set it (renamed by the `sed` above).
* This path was checked by reading the code, not by restoring a real old backup on a workchat-server. **Take a fresh backup right after the upgrade** (admin panel, Backups, back up now)
  so that you have one written by workchat-server itself, and keep the old ones until you have done so. Use a restore on a spare machine if you need certainty.

### Going back

Stop the new stack and start the old one with the volume as it was: rename `workchat.db*` back to `atomic.db*` with the same one-off command (swap the two names), or restore `data-before-workchat.tgz`
into an empty volume. The old server image keeps working with the old names.

## Existing SuperChat client apps

Facts only:

* workchat-server keeps the HTTP and WebSocket API of the earlier server unchanged (`/api/v1`, `api_version` 1, `min_client_version` 1.0.0), so the API paths and message formats are the same.
* The existing SuperChat client apps (desktop, Android, the command-line client) have **not** been tested against workchat-server, and they are not supported with it.
  Nothing in this project was built or tested for them.
* What is known to differ: the login-link landing page opens `workchat://`, which those apps do not register; they still accept a pasted `https://<server>/l/<token>` link on their login page by the same code path as before.
  Those apps check the old project's releases for updates and show the old project's links. Their download offers on the landing page come from the server's downloads folder, which is empty unless you put files there.
* New WorkChat clients are built separately; this repository contains only the server and its admin panel.
