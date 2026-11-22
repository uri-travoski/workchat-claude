#!/usr/bin/env bash
# Creates the permanent WorkChat Android release keystore OUTSIDE the repo.
#   keys/workchat/workchat-release.jks   the keystore (chmod 600)
#   keys/workchat/signing.env            ANDROID_KEYSTORE_PATH / _PASSWORD / ANDROID_KEY_ALIAS / _PASSWORD (chmod 600)
# Refuses to overwrite an existing keystore: replacing the key breaks updates for installed apps.
# Passwords are generated randomly and never printed. Back the folder up; it cannot be recreated.
set -euo pipefail
DIR="${WORKCHAT_KEYS_DIR:-$HOME/keys/workchat}"
JKS="$DIR/workchat-release.jks"
ENVF="$DIR/signing.env"
ALIAS=workchat
if [ -e "$JKS" ]; then
  echo "Keystore already exists: $JKS (not touching it)"; 
  keytool -list -v -keystore "$JKS" -storepass "$(. "$ENVF"; echo "$ANDROID_KEYSTORE_PASSWORD")" -alias "$ALIAS" 2>/dev/null | grep -E "SHA256:" | head -1 || true
  exit 0
fi
umask 077
mkdir -p "$DIR"; chmod 700 "$DIR"
STOREPASS=$(openssl rand -hex 24)
KEYPASS=$STOREPASS   # PKCS12 uses one password for store and key
keytool -genkeypair -v -storetype PKCS12 -keystore "$JKS" -alias "$ALIAS" \
  -keyalg RSA -keysize 4096 -validity 36500 \
  -storepass "$STOREPASS" -keypass "$KEYPASS" \
  -dname "CN=WorkChat, OU=WorkChat, O=WorkChat, C=AU" >/dev/null 2>&1
cat > "$ENVF" <<ENV
export ANDROID_KEYSTORE_PATH="$JKS"
export ANDROID_KEYSTORE_PASSWORD="$STOREPASS"
export ANDROID_KEY_ALIAS="$ALIAS"
export ANDROID_KEY_PASSWORD="$KEYPASS"
ENV
chmod 600 "$JKS" "$ENVF"
echo "Created $JKS and $ENVF"
echo "Certificate fingerprint:"
keytool -list -v -keystore "$JKS" -storepass "$STOREPASS" -alias "$ALIAS" 2>/dev/null | grep -E "SHA256:"
echo
echo "GitHub secrets (set them yourself, e.g. with gh secret set):"
echo "  ANDROID_KEYSTORE_B64     = base64 -w0 $JKS"
echo "  ANDROID_KEYSTORE_PASSWORD, ANDROID_KEY_ALIAS, ANDROID_KEY_PASSWORD = values in $ENVF"
