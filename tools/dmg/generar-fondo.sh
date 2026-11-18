#!/bin/sh
# Rebuilds the installer background from its editable source, the same bytes every time:
#
#   assets/dmg/fondo.svg  ->  assets/dmg/fondo.png      660x416 points at 72 dpi (1x)
#                             assets/dmg/fondo@2x.png   1320x832 pixels at 144 dpi (2x)
#                             assets/dmg/fondo.tiff     both in one file (tiffutil -cathidpicheck),
#                                                       the HiDPI picture Finder reads
#
# Usage: sh tools/dmg/generar-fondo.sh
#
# Needs rsvg-convert (librsvg) and the macOS tools sips and tiffutil. The result is committed, so
# a release build does not need rsvg-convert; this is only for changing the design. The renders
# are tagged sRGB (the profile is only embedded, no pixel is converted). The script renders twice
# and stops if the two runs differ.
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
fail() { echo "generar-fondo: $*" >&2; exit 1; }
command -v rsvg-convert >/dev/null 2>&1 || fail "rsvg-convert is needed (librsvg)"
SRC="$ROOT/assets/dmg/fondo.svg"
[ -f "$SRC" ] || fail "no $SRC"
PROFILE="/System/Library/ColorSync/Profiles/sRGB Profile.icc"
[ -f "$PROFILE" ] || fail "no sRGB profile at $PROFILE"

T=$(mktemp -d "${TMPDIR:-/tmp}/altrune-fondo.XXXXXX")
case "$T" in /private/tmp/*|/tmp/*|/var/folders/*|/private/var/folders/*) ;; *) fail "unexpected temp folder $T" ;; esac
trap 'case "$T" in */altrune-fondo.*) rm -rf "$T" ;; esac' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

render() { # render DIR
  mkdir -p "$1"
  rsvg-convert -w 660 -h 416 "$SRC" -o "$1/fondo.png"
  rsvg-convert -w 1320 -h 832 "$SRC" -o "$1/fondo@2x.png"
  sips -s dpiWidth 72 -s dpiHeight 72 --embedProfile "$PROFILE" "$1/fondo.png" >/dev/null
  sips -s dpiWidth 144 -s dpiHeight 144 --embedProfile "$PROFILE" "$1/fondo@2x.png" >/dev/null
  tiffutil -cathidpicheck "$1/fondo.png" "$1/fondo@2x.png" -out "$1/fondo.tiff" 2>/dev/null
}
render "$T/uno"
render "$T/dos"
for f in fondo.png fondo@2x.png fondo.tiff; do
  cmp -s "$T/uno/$f" "$T/dos/$f" || fail "two renders of $f differ: the build is not repeatable"
done

mkdir -p "$ROOT/assets/dmg"
for f in fondo.png fondo@2x.png fondo.tiff; do
  cp "$T/uno/$f" "$ROOT/assets/dmg/$f"
  echo "  $(shasum -a 256 "$ROOT/assets/dmg/$f" | cut -c1-16)  assets/dmg/$f"
done
echo "Done: two renders were identical"
