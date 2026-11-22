#!/usr/bin/env bash
# End-to-end run: builds the server image from docker/Dockerfile, then runs tests/e2e/tests/full.rs and bots.rs against it
# (server in docker, MinIO in docker, fake Google Drive and webhook receivers in-process, driven over raw HTTP).
# Host ports used: 18300 (server), 18301 (restored server), 18302 (bots server) and 18310 (MinIO). Never 8080.
# Everything it starts is removed again, pass or fail.
set -euo pipefail
cd "$(dirname "$0")/../.."
# Use the rustup toolchain if present (a distro rustc may be too old for the dependencies).
export PATH="$HOME/.cargo/bin:$PATH"
docker build -f docker/Dockerfile -t workchat-server:e2e .
WORKCHAT_E2E=1 cargo test -p workchat-e2e --test full --test bots -- --nocapture --test-threads=1
