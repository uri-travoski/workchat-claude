#!/usr/bin/env bash
# Prints the CHANGELOG.md section for a version: scripts/changelog-section.sh 1.1.0
# Falls back to a one-line note when there is no such section.
set -euo pipefail
cd "$(dirname "$0")/.."
VER=${1:?version}
awk -v v="$VER" '
  /^## / { if (on) exit; if (index($0, "[" v "]") || index($0, " " v " ") || $0 ~ ("^## " v "$")) { on=1; next } }
  on { print }
' CHANGELOG.md > /tmp/wc-notes.$$ || true
if [ -s /tmp/wc-notes.$$ ]; then cat /tmp/wc-notes.$$; else echo "WorkChat $VER"; fi
rm -f /tmp/wc-notes.$$
