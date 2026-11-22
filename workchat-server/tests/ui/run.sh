#!/usr/bin/env bash
# Admin-panel UI tests (Playwright + headless Chromium). Builds the server and the admin panel, then runs every
# tests/ui/*.mjs in turn; each starts its own server on its own port (18311-18315, 18321/18322), never 8080.
#
# Needs: node 20+, npm, and a Chromium in ~/.cache/ms-playwright (npx playwright-core install chromium puts one there).
# Usage:  tests/ui/run.sh            # all of them
#         tests/ui/run.sh admin-roles   # one
set -euo pipefail
cd "$(dirname "$0")/../.."
export PATH="$HOME/.cargo/bin:$PATH"
if ! ls "$HOME"/.cache/ms-playwright/chromium-* >/dev/null 2>&1; then
  echo "No Chromium in ~/.cache/ms-playwright: run  (cd tests/ui && npm install && npx playwright-core install chromium)" >&2
  exit 2
fi
cargo build -p workchat-server
(cd admin-web && npm ci --no-audit --no-fund && npm run build)
(cd tests/ui && [ -d node_modules/playwright-core ] || npm install --no-audit --no-fund)
export PLAYWRIGHT_DIR="$PWD/tests/ui"
names=("$@")
[ ${#names[@]} -gt 0 ] || names=(admin-smoke admin-roles admin-dialogs admin-viewer admin-automation admin-mobile admin-updates)
for n in "${names[@]}"; do
  echo "=== $n"
  node "tests/ui/$n.mjs"
done
echo "All UI tests passed."
