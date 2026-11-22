# Status

## 1.2.0 (unreleased): channels are private only

* Public channels are gone: no `visibility`, no browse, no join, no `channel_directory_update`. Old databases and 1.1.0 backups keep every channel (all private). `GET /saved/messages` added for a Saved view.
  No link previews exist on the server (nothing to remove). Version 1.2.0 in Cargo.toml and deploy/docker-compose.yml. Details in CHANGELOG.md.

## 1.1.0 (unreleased): phase S1 for the WorkChat apps

* Added what the apps need: profile title and phone, contacts search, `GET /me/counts`, richer `GET /threads` and `GET /files`, read receipts, the `unifiedpush` push kind,
  bot pictures, admin backdating, the new icon. Details in CHANGELOG.md and docs/API.md. Version bumped to 1.1.0 in Cargo.toml.
* `tools/seed` (see its README) fills a server with sample people, chats and files; run against the local test container `workchat-test` (port 18600).
* Tests: `cargo test --workspace` green (see CHANGELOG for the new `phoneui` tests). Not done: nothing committed or pushed; the e2e and UI suites were not re-run.

## 1.1.0 (unreleased): phase S2 for the WorkChat apps

* Channels created by members (org setting), channel managers, public/private channels with browse/join/leave, pinning rules, "Delete for me", "Also send to channel", `Counts` in the schema,
  documented single-conversation and own-sessions endpoints, admin panel Settings tab. See CHANGELOG.md and docs/API.md. Two migrations (additive): `conversations.visibility`, `messages.also_channel`.
* Tests: see CHANGELOG; `cargo test --workspace` green. The e2e and Playwright UI suites were not re-run. Nothing committed or pushed.

## Current state (Phase A done: standalone split)

* workchat-server 1.0.0 is the server and admin panel of the earlier chat project, copied into its own standalone folder and renamed (see docs/UPGRADING.md for the mapping).
  The code, database layout and HTTP/WebSocket API are otherwise unchanged. The folder builds on its own: own Cargo workspace and lock file, no path outside it.
* Builds and checks done in this folder: `cargo build --release`, `cargo test` (unit tests of `crates/server` and `crates/proto`), `gen-schema`, the admin panel build
  (`npm ci && npm run build`), the Docker image build and a smoke run of the image (landing redirect, `/admin`, health check).
* Not git-initialised yet; no GitHub repository, no release, no image has been published. The CI and release workflows are written but have never run.
* Not done on purpose: no client apps, no client-core, no CLI. Nothing has been tested against the old client apps.

## Phase B done: tests

* A server test suite that needs no client library: 101 integration tests in `crates/server/tests` (adminview, automation, avatars, backups, deluser, features, flow,
  organisation, pins, push, roles, signout, social, updates) plus 27 unit tests, all passing with `cargo test --workspace`. They drive the in-process server over raw HTTP
  and WebSocket through the test-only crate `crates/testkit`.
* Docker end-to-end tests in `tests/e2e` (image, MinIO, fake Google Drive, bots, webhooks, forms, scheduled messages, AI, backup restored onto a second server):
  `tests/e2e/run.sh`, both scenarios passed. The Dockerfile now also copies `tests/e2e/` because it is a workspace member.
* Seven Playwright UI tests of the admin panel in `tests/ui` (`tests/ui/run.sh`), all passing.
* CI: the rust job runs `cargo test --workspace`; optional manual `e2e` and `ui` jobs (workflow_dispatch). Never run on GitHub yet.
* Not ported on purpose (client-side only): local PIN/app lock, local cache encryption, client preferences, spell check, sounds, the client's own update checker, outbox
  progress/cancel/retry, video-poster generation, the screenshot demo seeder. No server bug was found while porting.
* Still to do: a clean-room check from a copy of the folder at another path, and the first git commit.
