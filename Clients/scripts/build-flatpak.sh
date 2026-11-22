#!/usr/bin/env bash
# Builds WorkChat-<version>-linux-x86_64.flatpak from a prebuilt release binary.
# usage: scripts/build-flatpak.sh [path-to-workchat-binary] [output-dir]
# Needs flatpak, flatpak-builder and the org.gnome.Platform//48 + Sdk runtimes (flathub).
set -euo pipefail
cd "$(dirname "$0")/.."
BIN=${1:-target/release/workchat}
OUT=${2:-target/flatpak}
VER=$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
[ -x "$BIN" ] || { echo "binary not found: $BIN (run: npm run tauri build -- --bundles deb)"; exit 1; }
cp "$BIN" packaging/flatpak/workchat
trap 'rm -f packaging/flatpak/workchat' EXIT
mkdir -p "$OUT"
flatpak-builder --user --force-clean --install-deps-from=flathub \
  --repo="$OUT/repo" "$OUT/build" packaging/flatpak/app.workchat.WorkChat.yml
flatpak build-bundle "$OUT/repo" "$OUT/WorkChat-$VER-linux-x86_64.flatpak" app.workchat.WorkChat
ls -l "$OUT/WorkChat-$VER-linux-x86_64.flatpak"
