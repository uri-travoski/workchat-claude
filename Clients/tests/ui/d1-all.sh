#!/usr/bin/env bash
# Runs every D1 UI script at 1280x800 and 1000x700 and saves the screenshots under docs/screenshots/d1/<size>/.
# The failing xdg-open stub (exit 3) gives the "Can't open this file" dialog; pass STUB_DIR to use another PATH prefix.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
stub=${STUB_FAIL:-}
for size in 1280x800 1000x700; do
  out="$here/docs/screenshots/d1/$size"
  for s in d1-desktop d1-more d1-lock d1-viewer; do
    data=${DATA:-/tmp/workchat-ui-d1}; [ "$s" = d1-lock ] && { data=/tmp/workchat-ui-d1-lock; rm -rf "$data"; }
    "$here/tests/ui/run.sh" "$here/tests/ui/$s.js" "$size" "$out" "$data" 2>&1 | grep -E "^(error|app exited|no window)" && echo "FAILED $s $size"
  done
  PATH="$stub:$PATH" "$here/tests/ui/run.sh" "$here/tests/ui/d1-files.js" "$size" "$out" "${DATA:-/tmp/workchat-ui-d1}" 2>&1 | grep -E "^(error[^-]|app exited)" 
done
