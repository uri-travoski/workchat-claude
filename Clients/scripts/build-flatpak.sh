#!/usr/bin/env bash
# Builds WorkChat-<version>-linux-<x86_64|aarch64>.flatpak from a prebuilt release binary of the SAME CPU
# (the arch is the machine's; override with FLATPAK_ARCH=x86_64|aarch64 only when the binary was built for it).
# usage: scripts/build-flatpak.sh [path-to-workchat-binary] [output-dir]
# Needs flatpak, flatpak-builder and the org.gnome.Platform//48 + Sdk runtimes (flathub).
set -euo pipefail
cd "$(dirname "$0")/.."
BIN=${1:-target/release/workchat}
OUT=${2:-target/flatpak}
case "${FLATPAK_ARCH:-$(uname -m)}" in
  x86_64|amd64) ARCH=x86_64 ;;
  aarch64|arm64) ARCH=aarch64 ;;
  *) echo "unsupported CPU: ${FLATPAK_ARCH:-$(uname -m)}"; exit 1 ;;
esac
VER=$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
[ -x "$BIN" ] || { echo "binary not found: $BIN (run: npm run tauri build -- --bundles deb)"; exit 1; }
cp "$BIN" packaging/flatpak/workchat
trap 'rm -f packaging/flatpak/workchat' EXIT
mkdir -p "$OUT"
flatpak-builder --user --force-clean --arch="$ARCH" --install-deps-from=flathub \
  --repo="$OUT/repo" "$OUT/build" packaging/flatpak/app.workchat.WorkChat.yml
flatpak build-bundle --arch="$ARCH" "$OUT/repo" "$OUT/WorkChat-$VER-linux-$ARCH.flatpak" app.workchat.WorkChat
ls -l "$OUT/WorkChat-$VER-linux-$ARCH.flatpak"
