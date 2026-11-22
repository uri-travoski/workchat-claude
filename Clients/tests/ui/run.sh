#!/usr/bin/env bash
# Drives the debug build under Xvfb with a UI test script and saves a screenshot at every api.mark(name).
# usage: tests/ui/run.sh <script.js> <WxH> <out-dir> [data-dir]
# Env: WORKCHAT_TEST_PICK, WORKCHAT_FAKE_MIC are passed through to the app.
set -euo pipefail
script=$(realpath "$1"); full=$(mktemp --suffix=.js); cat "$(dirname "$script")/${BOOT:-boot}.js" "$script" > "$full"; script=$full; size=${2:-1280x800}; out=$(realpath -m "$3"); data=${4:-/tmp/workchat-ui-data}
here=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
W=${size%x*}; H=${size#*x}
mkdir -p "$out" "$data"; rm -f "$data/mark"
disp=:97
Xvfb $disp -screen 0 1400x1000x24 >/dev/null 2>&1 &
xvfb=$!
cleanup() { rm -f "$full"; kill "$app" 2>/dev/null || true; kill $xvfb 2>/dev/null || true; }
trap cleanup EXIT
sleep 1
export DISPLAY=$disp WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 WORKCHAT_DATA_DIR="$data" WORKCHAT_TEST_SCRIPT="$script" WORKCHAT_AUDIO_NULL=${WORKCHAT_AUDIO_NULL:-1}
"$here/target/debug/workchat" >"$out/app.log" 2>&1 &
app=$!
for i in $(seq 1 50); do
  id=$(xdotool search --onlyvisible --name WorkChat 2>/dev/null | head -1 || true); [ -n "$id" ] && break; sleep 0.3
done
[ -n "${id:-}" ] || { echo "no window"; cat "$out/app.log"; exit 1; }
xdotool windowmove "$id" 0 0 windowsize "$id" "$W" "$H"
sleep 1
for i in $(seq 1 1200); do
  if [ -f "$data/mark" ]; then
    name=$(cat "$data/mark")
    case "$name" in
      done) break;;
      error*) echo "$name"; break;;
      # xdo:<args> runs real X input (xdotool) against the window, e.g. xdo:key_Escape or xdo:mousemove_300_300_click_1
      xdo:*) xdotool $(echo "${name#xdo:}" | tr '_' ' ') >/dev/null 2>&1 || true; echo "input ${name#xdo:}"; rm -f "$data/mark"; continue;;
    esac
    import -window root -crop "${W}x${H}+0+0" +repage "$out/$name.png"
    echo "shot $name"
    rm -f "$data/mark"
  fi
  kill -0 "$app" 2>/dev/null || { echo "app exited"; break; }
  sleep 0.15
done
tail -5 "$out/app.log" || true
