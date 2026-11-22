#!/usr/bin/env bash
# Signs the "-unsigned" APKs from a CI release with the local release key, ready to replace them on the release.
# usage: scripts/android-sign-local.sh <version> [dir-with-unsigned-apks]
#   e.g. gh release download v1.1.0 -p '*android*unsigned.apk' -D apks && scripts/android-sign-local.sh 1.1.0 apks
# Writes WorkChat-<ver>-android-<abi>.apk next to the inputs, then prints the gh commands to swap the assets.
set -euo pipefail
VER=${1:?version}; DIR=${2:-.}
. "${WORKCHAT_SIGNING_ENV:-$HOME/keys/workchat/signing.env}"
SDK=${ANDROID_HOME:-/opt/android-sdk}
BT=$(ls -d "$SDK"/build-tools/* | sort -V | tail -1)
for abi in arm64 x86_64; do
  in="$DIR/WorkChat-$VER-android-$abi-unsigned.apk"; out="$DIR/WorkChat-$VER-android-$abi.apk"
  [ -f "$in" ] || { echo "missing $in"; exit 1; }
  "$BT/zipalign" -f -p 4 "$in" "$DIR/.aligned-$abi.apk"
  "$BT/apksigner" sign --ks "$ANDROID_KEYSTORE_PATH" --ks-key-alias "$ANDROID_KEY_ALIAS" \
    --ks-pass env:ANDROID_KEYSTORE_PASSWORD --key-pass env:ANDROID_KEY_PASSWORD --out "$out" "$DIR/.aligned-$abi.apk"
  rm -f "$DIR/.aligned-$abi.apk" "$out.idsig"
  "$BT/apksigner" verify --print-certs "$out" | head -3
done
echo
echo "Replace the release assets:"
for abi in arm64 x86_64; do
  echo "  gh release delete-asset v$VER WorkChat-$VER-android-$abi-unsigned.apk -y"
  echo "  gh release upload v$VER $DIR/WorkChat-$VER-android-$abi.apk --clobber"
done
echo "  (then regenerate SHA256SUMS: gh release download v$VER -p 'WorkChat-*' -D all && (cd all && sha256sum WorkChat-* > SHA256SUMS) && gh release upload v$VER all/SHA256SUMS --clobber)"
