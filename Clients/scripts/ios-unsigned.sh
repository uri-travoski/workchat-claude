#!/usr/bin/env bash
# macOS only. Builds an UNSIGNED iOS .ipa (device, arm64) and a simulator .app zip, no Apple account needed.
# usage: scripts/ios-unsigned.sh <version> <out-dir>
# Output: <out>/WorkChat-<ver>-ios-unsigned.ipa and <out>/WorkChat-<ver>-ios-simulator.zip
#
# How: `tauri ios build` normally archives and signs. We put a small `xcodebuild` shim first on PATH that adds
# CODE_SIGNING_ALLOWED=NO (and friends) to archive/build calls and skips the signed export step, then zip the
# resulting .app from the .xcarchive into Payload/ ourselves. Best effort: not verified on a Mac in phase C4a.
set -euo pipefail
VER=${1:?version}; OUT=${2:?out dir}
cd "$(dirname "$0")/.."
ROOT=$PWD
mkdir -p "$OUT"; OUT=$(cd "$OUT" && pwd)

[ -d src-tauri/gen/apple ] || npx tauri ios init --ci
# Info.plist additions (usage strings, ATS local networking, URL scheme, background modes)
[ -f src-tauri/Info.ios.plist ] || cp packaging/ios/Info.ios.plist src-tauri/Info.ios.plist

REAL_XCODEBUILD=$(command -v xcodebuild)
SHIM=$(mktemp -d)
cat > "$SHIM/xcodebuild" <<SH
#!/usr/bin/env bash
for a in "\$@"; do
  if [ "\$a" = "-exportArchive" ]; then echo "[shim] skipping signed export"; exit 0; fi
done
exec "$REAL_XCODEBUILD" "\$@" CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" DEVELOPMENT_TEAM=""
SH
chmod +x "$SHIM/xcodebuild"

# 1. device build -> .xcarchive (the CLI may exit non-zero after the archive step because there is no ipa to export)
PATH="$SHIM:$PATH" npx tauri ios build --ci --target aarch64 || echo "tauri ios build exited non-zero (expected without signing); looking for the archive"

APP=$(find src-tauri/gen/apple -path '*.xcarchive/Products/Applications/*.app' -maxdepth 8 -type d | head -1 || true)
[ -n "$APP" ] || { echo "no .app found in an xcarchive"; exit 1; }
rm -rf "$OUT/Payload"; mkdir -p "$OUT/Payload"
cp -R "$APP" "$OUT/Payload/"
( cd "$OUT" && zip -qry "WorkChat-$VER-ios-unsigned.ipa" Payload && rm -rf Payload )
echo "built $OUT/WorkChat-$VER-ios-unsigned.ipa"

# 2. simulator build (the options file written by the previous run lets the Xcode script phase find its settings)
PROJ=$(ls -d src-tauri/gen/apple/*.xcodeproj | head -1)
SCHEME=$(basename "$PROJ" .xcodeproj)_iOS
PATH="$SHIM:$PATH" xcodebuild -project "$PROJ" -scheme "$SCHEME" -configuration release \
  -sdk iphonesimulator -arch arm64 -derivedDataPath "$ROOT/target/ios-sim" build
SIMAPP=$(find "$ROOT/target/ios-sim/Build/Products" -name '*.app' -maxdepth 3 -type d | head -1)
[ -n "$SIMAPP" ] || { echo "no simulator .app found"; exit 1; }
( cd "$(dirname "$SIMAPP")" && zip -qry "$OUT/WorkChat-$VER-ios-simulator.zip" "$(basename "$SIMAPP")" )
echo "built $OUT/WorkChat-$VER-ios-simulator.zip"
