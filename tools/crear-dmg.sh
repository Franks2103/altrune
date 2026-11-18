#!/bin/sh
# Builds the installer image dist/Altrune-VERSION.dmg (VERSION is the full one in Cargo.toml,
# for example 0.1.0-alpha) from an already built Altrune.app, checks it and writes its SHA-256.
#
# Usage: sh tools/crear-dmg.sh [--app PATH/Altrune.app] [--prueba]
#   --app     the app to put in the image (default dist/Altrune.app); signed or not, it is
#             copied as it is: this script never signs and never touches the app
#   --prueba  a test image: allows an app without Contents/Resources/Licencias/terceros.html.
#             Never for a release.
#
# Output: dist/Altrune-VERSION.dmg and dist/Altrune-VERSION.dmg.sha256 (shasum -a 256 output
# with the file's base name, so `shasum -a 256 -c` works next to the image).
#
# What the image is: a Finder window with the picture assets/dmg/fondo.tiff, the app on the
# left, a shortcut to /Applications on the right and nothing else (no ads, no extra installer).
# Window, icons and picture are one design: tools/dmg/ajustes.py and assets/dmg/fondo.svg.
#
# Format and filesystem, with the reason: UDZO (zlib, read-only) on HFS+. UDZO mounts on every
# macOS and is kernel compatible. ULFO (lzfse) or ULMO (lzma) would save a few hundred KB to
# 1 MB of about 5 MB, but ULMO is flagged "kernel compatible: false" by hdiutil and needs a
# newer macOS, which is not worth it. HFS+ is what every macOS reads; APFS would synthesize a
# second disk when attached and gain nothing here. The volume is sized to the app plus 24 MiB
# (dmgbuild's default adds 128 MiB, which would show as a 140 MiB volume for an 11 MB app).
#
# dmgbuild 2.0.0 comes from a venv in target/dmg-venv, installed from tools/dmg/requirements.txt
# with --require-hashes --only-binary=:all: (nothing is compiled, every file has its hash). It
# needs Python 3.11 or later: ALTRUNE_PYTHON or python3. dmgbuild writes the layout itself
# (.DS_Store); it does not drive Finder, needs no permission and no desktop session.
# Never used on purpose: hide_extensions / SetFile on the app (they write extended attributes
# on the bundle and break `codesign --verify --strict`).
#
# The image is not byte-for-byte reproducible (HFS+ volume dates and UUID, and the creation date
# inside the background's alias change per build); what it holds is. That is why the SHA-256
# is taken after the build, from the finished file.
#
# The checks attach the image read-only at a temporary mount point (-nobrowse, -noautoopen) and
# detach exactly the device they got. Nothing is copied to /Applications and Finder is not used.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/.." && pwd)
cd "$ROOT"
DIST="$ROOT/dist"
VENV="$ROOT/target/dmg-venv"
fail() { echo "crear-dmg: $*" >&2; exit 1; }
ok() { echo "  ok  $*"; }

APP="$DIST/Altrune.app"
PRUEBA=0
while [ $# -gt 0 ]; do
  case "$1" in
    --app) [ $# -ge 2 ] || fail "--app needs a path"; APP=$2; shift 2 ;;
    --prueba) PRUEBA=1; shift ;;
    *) fail "unknown argument $1 (usage: sh tools/crear-dmg.sh [--app PATH] [--prueba])" ;;
  esac
done

[ -f Cargo.toml ] && [ -f assets/branding/Altrune.icns ] && [ -f assets/dmg/fondo.tiff ] \
  && [ -f tools/dmg/ajustes.py ] && [ -f tools/dmg/requirements.txt ] \
  || fail "run it from the app's repository (Cargo.toml, the icon, assets/dmg or tools/dmg is missing)"

# The full version names the image; the app's plist carries the numeric core.
FULL=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)
CORE=${FULL%%-*}
case "$CORE" in [0-9]*.[0-9]*.[0-9]*) ;; *) fail "could not read a x.y.z version from Cargo.toml (got \"$FULL\")" ;; esac
case "$FULL" in *[!0-9A-Za-z.-]*) fail "the version \"$FULL\" has characters that do not belong in a file name" ;; esac

# The app: a bundle named Altrune.app, the right identity and version, a valid signature.
[ -d "$APP" ] || fail "no app at $APP (run sh tools/empaquetar.sh first)"
APP=$(cd "$APP" && pwd -P)
[ "$(basename "$APP")" = Altrune.app ] || fail "the app must be called Altrune.app (got $(basename "$APP"))"
[ -f "$APP/Contents/Info.plist" ] || fail "$APP has no Contents/Info.plist"
plist() { plutil -extract "$1" raw -o - "$APP/Contents/Info.plist"; }
[ "$(plist CFBundleIdentifier)" = com.altrune.app ] || fail "CFBundleIdentifier is not com.altrune.app"
[ "$(plist CFBundleShortVersionString)" = "$CORE" ] || fail "the app is version $(plist CFBundleShortVersionString) but Cargo.toml says $FULL (core $CORE): build it again with sh tools/empaquetar.sh"
EXE="$APP/Contents/MacOS/$(plist CFBundleExecutable)"
[ -x "$EXE" ] || fail "the app's executable is missing or not executable"
codesign --verify --deep --strict "$APP" 2>/dev/null || fail "the app's signature does not verify (codesign --verify --deep --strict)"
if [ ! -f "$APP/Contents/Resources/Licencias/terceros.html" ]; then
  [ "$PRUEBA" = 1 ] || fail "the app has no Contents/Resources/Licencias/terceros.html: run sh tools/licencias.sh and sh tools/empaquetar.sh (or use --prueba for a test image)"
  echo "crear-dmg: warning: test image without the licence texts of the crates (--prueba); never publish it" >&2
fi
ok "app: com.altrune.app $CORE, signature verifies"

# The volume is called Altrune and the background's alias names it: a second mounted volume with
# that name would be mounted as "Altrune 1" and break the alias while building.
[ ! -e /Volumes/Altrune ] || fail "a volume named Altrune is already mounted; eject it yourself and run again"

[ ! -L "$DIST" ] || fail "$DIST is a link; remove it yourself"
mkdir -p "$DIST"
OUT="$DIST/Altrune-$FULL.dmg"
[ ! -L "$OUT" ] || fail "$OUT is a link; remove it yourself"

# Python 3.11 or later, and dmgbuild with the pinned hashes in a venv under target/.
PY=${ALTRUNE_PYTHON:-python3}
command -v "$PY" >/dev/null 2>&1 || fail "no $PY (set ALTRUNE_PYTHON to a Python 3.11 or later)"
"$PY" -I -c 'import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)' || fail "$PY is older than Python 3.11 (dmgbuild 2.0.0 needs it); set ALTRUNE_PYTHON"
WANT=$(shasum -a 256 tools/dmg/requirements.txt | cut -d' ' -f1)
if [ ! -x "$VENV/bin/python" ] || [ "$(cat "$VENV/.requisitos" 2>/dev/null || true)" != "$WANT" ]; then
  echo "Installing dmgbuild into $VENV (hashes pinned, wheels only)..."
  "$PY" -I -m venv --clear "$VENV"
  "$VENV/bin/python" -I -m pip install --quiet --disable-pip-version-check --require-hashes --only-binary=:all: -r tools/dmg/requirements.txt
  echo "$WANT" > "$VENV/.requisitos"
fi
"$VENV/bin/python" -I -m pip freeze 2>/dev/null | grep -qx 'dmgbuild==2.0.0' || fail "the venv at $VENV does not hold dmgbuild 2.0.0"
ok "dmgbuild 2.0.0 from the hashed requirements"

T=$(mktemp -d "${TMPDIR:-/tmp}/altrune-dmg.XXXXXX")
case "$T" in /private/tmp/*|/tmp/*|/var/folders/*|/private/var/folders/*) ;; *) fail "unexpected temp folder $T" ;; esac
DEV=""
cleanup() {
  if [ -n "$DEV" ]; then
    hdiutil detach "$DEV" >/dev/null 2>&1 || hdiutil detach "$DEV" -force >/dev/null 2>&1 \
      || echo "crear-dmg: could not detach $DEV; detach it yourself" >&2
  fi
  case "$T" in */altrune-dmg.*) rm -rf "$T" ;; esac
}
# A signal must end the script (the EXIT trap then cleans up once), not resume it after cleanup.
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# Build into the temp folder, then move the finished file into dist/.
APP_BYTES=$(( $(du -sk "$APP" | cut -f1) * 1024 ))
SIZE=$(( APP_BYTES + 24 * 1024 * 1024 ))
echo "Building the image (UDZO on HFS+, volume of $((SIZE / 1048576)) MiB)..."
"$VENV/bin/python" -I -m dmgbuild -s "$HERE/dmg/ajustes.py" \
  -D app="$APP" -D icono="$ROOT/assets/branding/Altrune.icns" -D fondo="$ROOT/assets/dmg/fondo.tiff" \
  -D formato=UDZO -D tamano="$SIZE" Altrune "$T/Altrune.dmg" >"$T/dmgbuild.txt" 2>&1 \
  || { cat "$T/dmgbuild.txt" >&2; fail "dmgbuild failed"; }
[ -f "$T/Altrune.dmg" ] || fail "dmgbuild made no image"

echo "Checking the image..."
hdiutil verify "$T/Altrune.dmg" >"$T/verify.txt" 2>&1 || { cat "$T/verify.txt" >&2; fail "hdiutil verify failed"; }
grep -q 'is VALID' "$T/verify.txt" || fail "hdiutil verify did not say the checksum is VALID"
ok "hdiutil verify: checksum is VALID"
hdiutil imageinfo -plist "$T/Altrune.dmg" > "$T/info.plist" 2>/dev/null
FORMAT=$(plutil -extract Format raw -o - "$T/info.plist")
[ "$FORMAT" = UDZO ] || fail "the image format is $FORMAT, not UDZO"
case "$(plutil -extract Properties.Encrypted raw -o - "$T/info.plist" 2>/dev/null || echo false)" in false) ;; *) fail "the image is encrypted" ;; esac
ok "format $FORMAT (hdiutil imageinfo), not encrypted"

MNT="$T/montaje"
mkdir "$MNT"
hdiutil attach -readonly -nobrowse -noautoopen -plist -mountpoint "$MNT" "$T/Altrune.dmg" > "$T/attach.plist" 2>"$T/attach.txt" \
  || { cat "$T/attach.txt" >&2; fail "could not attach the image read-only"; }
DEV=$(plutil -extract system-entities.0.dev-entry raw -o - "$T/attach.plist")
case "$DEV" in
  /dev/disk*[!0-9]*|/dev/disk) hdiutil detach "$MNT" >/dev/null 2>&1 || true; DEV=""; fail "unexpected device in the attach result" ;;
  /dev/disk[0-9]*) ;;
  *) hdiutil detach "$MNT" >/dev/null 2>&1 || true; DEV=""; fail "unexpected device in the attach result" ;;
esac
ok "attached read-only at a temporary mount point as $DEV"

# Everything in the root of the volume, hidden files included: exactly the app, the shortcut and
# the three layout files dmgbuild writes. Anything else is junk (.fseventsd, .Trashes, ._*...).
# C collation: the order must not depend on the caller's locale.
LISTED=$(/bin/ls -A "$MNT" | LC_ALL=C sort | tr '\n' ' ')
[ "$LISTED" = ".DS_Store .VolumeIcon.icns .background.tiff Altrune.app Applications " ] \
  || fail "unexpected content in the image: $LISTED"
[ -L "$MNT/Applications" ] && [ "$(readlink "$MNT/Applications")" = /Applications ] || fail "Applications is not a link to /Applications"
[ -d "$MNT/Altrune.app" ] && [ ! -L "$MNT/Altrune.app" ] || fail "Altrune.app is not a folder"
ok "contents: Altrune.app, Applications -> /Applications and the layout files; no junk"

# The same files inside the app, no more and no less (a stray .DS_Store or ._ file would show).
(cd "$APP" && find . | LC_ALL=C sort) > "$T/lista-origen.txt"
(cd "$MNT/Altrune.app" && find . | LC_ALL=C sort) > "$T/lista-imagen.txt"
cmp -s "$T/lista-origen.txt" "$T/lista-imagen.txt" || { diff "$T/lista-origen.txt" "$T/lista-imagen.txt" >&2 || true; fail "the app in the image holds different files than the source app"; }
ok "the app holds the same $(wc -l < "$T/lista-imagen.txt" | tr -d ' ') files as the source"

codesign --verify --deep --strict "$MNT/Altrune.app" 2>"$T/codesign.txt" || { cat "$T/codesign.txt" >&2; fail "the app inside the image fails codesign --verify --deep --strict"; }
ok "codesign --verify --deep --strict passes on the app inside the image"

IMG_EXE="$MNT/Altrune.app/Contents/MacOS/$(basename "$EXE")"
[ -x "$IMG_EXE" ] && [ "$(stat -f %Lp "$IMG_EXE")" = "$(stat -f %Lp "$EXE")" ] || fail "the executable lost its mode in the image"
[ "$(shasum -a 256 "$IMG_EXE" | cut -d' ' -f1)" = "$(shasum -a 256 "$EXE" | cut -d' ' -f1)" ] || fail "the binary in the image differs from the source app's"
ok "the executable keeps mode $(stat -f %Lp "$IMG_EXE") and its SHA-256 equals the source app's"

# No personal data: the path of the machine, the home folder or the user name, in the binary and
# in the layout file (the background's alias must hold only the volume name and /.background.tiff).
hits=$(strings -a "$IMG_EXE" | grep -c -e '/Users/' -e "$HOME" -e "/$(id -un)/" || true)
[ "$hits" = 0 ] || fail "the binary holds $hits strings with /Users/, the home folder or the user name"
# .DS_Store keeps names and paths as UTF-16BE, which strings(1) cannot see: look for each
# pattern as ASCII and as UTF-16BE.
"$VENV/bin/python" -I -c '
import sys
data = open(sys.argv[1], "rb").read()
bad = []
for pattern in sys.argv[2:]:
    if pattern in ("", "/"):
        continue
    for enc in ("ascii", "utf-16-be"):
        if pattern.encode(enc) in data:
            bad.append(pattern + " (" + enc + ")")
if bad:
    print("found: " + ", ".join(bad), file=sys.stderr)
    sys.exit(1)
' "$MNT/.DS_Store" /Users/ "$HOME" "/$(id -un)/" "$T" altrune-dmg. || fail ".DS_Store holds /Users/, the home folder, the user name or a temporary path"
ok "no /Users/, home folder or user name in the binary or in .DS_Store (ASCII and UTF-16)"

cmp -s "$MNT/.VolumeIcon.icns" assets/branding/Altrune.icns || fail "the volume icon is not assets/branding/Altrune.icns"
cmp -s "$MNT/.background.tiff" assets/dmg/fondo.tiff || fail "the background is not assets/dmg/fondo.tiff"
ok "volume icon and background are the committed files"

hdiutil detach "$DEV" >/dev/null 2>&1 || { sleep 2; hdiutil detach "$DEV" >/dev/null 2>&1; } || fail "could not detach $DEV"
DEV=""
! hdiutil info | grep -q "$T/Altrune.dmg" || fail "the image is still attached after detach"
ok "detached"

mv -f "$T/Altrune.dmg" "$OUT"
( cd "$DIST" && shasum -a 256 "Altrune-$FULL.dmg" > "Altrune-$FULL.dmg.sha256" )
( cd "$DIST" && shasum -a 256 -c "Altrune-$FULL.dmg.sha256" >/dev/null ) || fail "the checksum file does not verify"
echo "Done: $OUT ($(stat -f %z "$OUT") bytes, $FORMAT)"
echo "      $(cat "$DIST/Altrune-$FULL.dmg.sha256")"
