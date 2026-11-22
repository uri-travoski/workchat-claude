#!/usr/bin/env bash
# Re-copies the vendored protocol (crates/workchat-proto, schema/api.schema.json) from the server repo and
# regenerates the TypeScript types. The Clients repo builds on its own without ../workchat-server; this script
# is only run when the server's API changes.
#
#   scripts/sync-proto.sh [path/to/workchat-server]     (default: ../workchat-server)
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
server="${1:-$here/../workchat-server}"
[ -d "$server/crates/proto" ] || { echo "no server checkout at $server" >&2; exit 1; }

rm -rf "$here/crates/workchat-proto"
mkdir -p "$here/crates"
cp -r "$server/crates/proto" "$here/crates/workchat-proto"
rm -rf "$here/crates/workchat-proto/target"
cp "$server/schema/api.schema.json" "$here/schema/api.schema.json"
(cd "$here" && npm run --silent gen-types)
echo "synced proto from $server ($(git -C "$server" rev-parse --short HEAD 2>/dev/null || echo unknown))"
