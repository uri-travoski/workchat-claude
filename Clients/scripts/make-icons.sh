#!/usr/bin/env bash
# Regenerates every icon from ../images/app-icon.svg and packaging/android-icons/*.svg.
# Needs: rsvg-convert, ImageMagick (convert), node (npx tauri icon).
set -euo pipefail
cd "$(dirname "$0")/.."
SRC=../images/app-icon.svg
[ -f "$SRC" ] || SRC=packaging/linux/app.workchat.WorkChat.svg   # standalone checkout fallback
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

# 1. Tauri set: desktop png/ico/icns, Windows Square logos, plain Android + iOS sets
rsvg-convert -w 1024 -h 1024 "$SRC" -o "$TMP/app-icon-1024.png"
npx tauri icon "$TMP/app-icon-1024.png" >/dev/null 2>&1

# 2. Linux / flatpak
mkdir -p packaging/linux/hicolor
cp "$SRC" packaging/linux/app.workchat.WorkChat.svg
for s in 64 128 256 512; do
  rsvg-convert -w $s -h $s "$SRC" -o packaging/linux/hicolor/$s.png
done
cp packaging/linux/hicolor/512.png packaging/linux/app.workchat.WorkChat.png

# 3. Android adaptive icon (navy background, white bubble, themed monochrome)
A=packaging/android-icons
declare -A D=([mdpi]=108 [hdpi]=162 [xhdpi]=216 [xxhdpi]=324 [xxxhdpi]=432)
for d in "${!D[@]}"; do
  sz=${D[$d]}; out=$A/res/mipmap-$d; mkdir -p "$out"
  rsvg-convert -w $sz -h $sz $A/foreground.svg -o "$out/ic_launcher_foreground.png"
  rsvg-convert -w $sz -h $sz $A/monochrome.svg -o "$out/ic_launcher_monochrome.png"
  rsvg-convert -w $sz -h $sz $A/background.svg -o "$out/ic_launcher_background.png"
done
mkdir -p $A/res/mipmap-anydpi-v26
for n in ic_launcher ic_launcher_round; do
cat > $A/res/mipmap-anydpi-v26/$n.xml <<XML
<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
  <background android:drawable="@mipmap/ic_launcher_background"/>
  <foreground android:drawable="@mipmap/ic_launcher_foreground"/>
  <monochrome android:drawable="@mipmap/ic_launcher_monochrome"/>
</adaptive-icon>
XML
done
# overlay onto the Tauri-generated Android set (C3's `tauri android init` copies src-tauri/icons/android)
AI=src-tauri/icons/android
for d in "${!D[@]}"; do
  cp $A/res/mipmap-$d/ic_launcher_foreground.png $A/res/mipmap-$d/ic_launcher_monochrome.png \
     $A/res/mipmap-$d/ic_launcher_background.png $AI/mipmap-$d/
done
cp $A/res/mipmap-anydpi-v26/*.xml $AI/mipmap-anydpi-v26/
rm -f $AI/values/ic_launcher_background.xml; rmdir $AI/values 2>/dev/null || true

# legacy (API < 26) launcher icons: rounded square + round, same navy/white look
declare -A L=([mdpi]=48 [hdpi]=72 [xhdpi]=96 [xxhdpi]=144 [xxxhdpi]=192)
for d in "${!L[@]}"; do
  sz=${L[$d]}
  rsvg-convert -w 432 -h 432 $A/background.svg -o "$TMP/bg.png"; rsvg-convert -w 432 -h 432 $A/foreground.svg -o "$TMP/fg.png"
  convert "$TMP/bg.png" "$TMP/fg.png" -composite -crop 288x288+72+72 +repage -resize ${sz}x${sz} "$TMP/legacy.png"
  r=$((sz*22/100))
  convert -size ${sz}x${sz} xc:none -fill white -draw "roundrectangle 0,0 $((sz-1)),$((sz-1)) $r,$r" "$TMP/m1.png"
  convert -size ${sz}x${sz} xc:none -fill white -draw "circle $((sz/2)),$((sz/2)) $((sz/2)),0" "$TMP/m2.png"
  convert "$TMP/legacy.png" "$TMP/m1.png" -alpha off -compose copy_opacity -composite $AI/mipmap-$d/ic_launcher.png
  convert "$TMP/legacy.png" "$TMP/m2.png" -alpha off -compose copy_opacity -composite $AI/mipmap-$d/ic_launcher_round.png
done

# 4. iOS: opaque icons (no alpha, Apple rejects transparency), rendered from ios-icon.svg
for f in src-tauri/icons/ios/*.png; do
  sz=$(identify -format %w "$f")
  rsvg-convert -w $sz -h $sz $A/ios-icon.svg -o "$f"
  convert "$f" -alpha off "$f"
done
echo "icons done"
