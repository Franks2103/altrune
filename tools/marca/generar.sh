#!/bin/bash
# Rebuilds every file in assets/branding/ from the official Altrune logo.
#
#   tools/marca/generar.sh [path-to-original-logo]
#
# The original is only read, never modified. Running this twice gives
# byte-identical files. Needs macOS (Swift, iconutil, sips, file, shasum).
set -euo pipefail

AQUI="$(cd "$(dirname "$0")" && pwd)"
RAIZ="$(cd "$AQUI/../.." && pwd)"
OUT="$RAIZ/assets/branding"
# The master is the copy kept in assets/branding; another path can be given to re-import it.
ORIGINAL="${1:-${ALTRUNE_LOGO:-$OUT/Logotipo de Altrune.png}}"
HASH_ESPERADO="6c04dbef66b138e39d69538fbf14dee795d2586ce95886c08bebfa80a15cde3e"

[ -f "$ORIGINAL" ] || { echo "Original logo not found: $ORIGINAL" >&2; exit 1; }
hash_original="$(shasum -a 256 "$ORIGINAL" | cut -d' ' -f1)"
if [ "$hash_original" != "$HASH_ESPERADO" ]; then
  echo "warning: the logo is not the expected one (sha256 $hash_original)" >&2
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$OUT"

# 1. byte-identical copy of the original (nothing to copy when the master itself is the source)
ORIGINAL_ABS="$(cd "$(dirname "$ORIGINAL")" && pwd)/$(basename "$ORIGINAL")"
if [ "$ORIGINAL_ABS" != "$OUT/Logotipo de Altrune.png" ]; then
  cp "$ORIGINAL" "$OUT/Logotipo de Altrune.png"
fi
[ "$(shasum -a 256 "$OUT/Logotipo de Altrune.png" | cut -d' ' -f1)" = "$hash_original" ] \
  || { echo "copy differs from the original" >&2; exit 1; }
echo "original sha256: $hash_original"

# 2. compile once (optimised) and run the three steps
swiftc -O -swift-version 5 "$AQUI/marca.swift" -o "$TMP/marca"
"$TMP/marca" extraer "$ORIGINAL" "$OUT"
"$TMP/marca" paleta "$ORIGINAL"

# 3. the app icon: master PNG, temporary iconset, .icns
mkdir "$TMP/Altrune.iconset"
"$TMP/marca" icono "$OUT" "$TMP/Altrune.iconset"
/usr/bin/iconutil -c icns "$TMP/Altrune.iconset" -o "$OUT/Altrune.icns"

# 4. verify the .icns: ten images of the right sizes
mkdir "$TMP/volver"
/usr/bin/iconutil -c iconset "$OUT/Altrune.icns" -o "$TMP/volver/Altrune.iconset"
n=0
for par in 16x16:16 16x16@2x:32 32x32:32 32x32@2x:64 128x128:128 128x128@2x:256 \
           256x256:256 256x256@2x:512 512x512:512 512x512@2x:1024; do
  f="$TMP/volver/Altrune.iconset/icon_${par%%:*}.png"
  px="${par##*:}"
  w="$(/usr/bin/sips -g pixelWidth "$f" | awk '/pixelWidth/ {print $2}')"
  h="$(/usr/bin/sips -g pixelHeight "$f" | awk '/pixelHeight/ {print $2}')"
  [ "$w" = "$px" ] && [ "$h" = "$px" ] || { echo "wrong size for $f: ${w}x${h}" >&2; exit 1; }
  n=$((n + 1))
done
echo "icns: $n images with the right sizes; $(/usr/bin/file -b "$OUT/Altrune.icns")"
