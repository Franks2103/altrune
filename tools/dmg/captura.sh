#!/bin/sh
# Opens a disk image the way a person does (open: DiskImageMounter mounts it and Finder shows its
# window), captures that one Finder window as a PNG and detaches exactly the device of that image.
#
# Usage: sh tools/dmg/captura.sh IMAGE.dmg OUT.png
#
# Only reads: the window list comes from a tiny Swift helper (CGWindowListCopyWindowInfo, built
# into a temporary folder) and the picture from screencapture -l. No AppleScript, no change to
# Finder, Dock or any preference, no process is stopped. It needs a desktop session and the
# permission to capture the screen. It refuses to run when a volume named Altrune is already
# mounted or when the image is already attached, so it never detaches something that is not its.
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
fail() { echo "captura: $*" >&2; exit 1; }
[ $# -eq 2 ] || fail "usage: sh tools/dmg/captura.sh IMAGE.dmg OUT.png"
DMG=$(cd "$(dirname "$1")" && pwd -P)/$(basename "$1")
OUT=$2
[ -f "$DMG" ] || fail "no image at $DMG"
case "$OUT" in *.png) ;; *) fail "the output must be a .png file" ;; esac
[ ! -e /Volumes/Altrune ] || fail "a volume named Altrune is already mounted; eject it yourself first"

# Devices of one image: the whole-disk entries (/dev/diskN) whose image-path is the given file.
device_of() {
  hdiutil info -plist | python3 -I -c '
import plistlib, sys
want = sys.argv[1]
for image in plistlib.loads(sys.stdin.buffer.read()).get("images", []):
    if image.get("image-path") == want:
        for e in image.get("system-entities", []):
            dev = e.get("dev-entry", "")
            if dev.startswith("/dev/disk") and "s" not in dev[len("/dev/disk"):]:
                print(dev)
                raise SystemExit
' "$1"
}
[ -z "$(device_of "$DMG")" ] || fail "$DMG is already attached; detach it yourself first"

T=$(mktemp -d "${TMPDIR:-/tmp}/altrune-captura.XXXXXX")
case "$T" in /private/tmp/*|/tmp/*|/var/folders/*|/private/var/folders/*) ;; *) fail "unexpected temp folder $T" ;; esac
DEV=""
cleanup() {
  if [ -n "$DEV" ]; then
    hdiutil detach "$DEV" >/dev/null 2>&1 || hdiutil detach "$DEV" -force >/dev/null 2>&1 || echo "captura: could not detach $DEV, do it yourself" >&2
  fi
  case "$T" in */altrune-captura.*) rm -rf "$T" ;; esac
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

swiftc -O -o "$T/ventana" "$HERE/ventana.swift" 2>"$T/swiftc.txt" || { cat "$T/swiftc.txt" >&2; fail "could not build the window helper"; }

open "$DMG"
# Wait for the mount (DEV) and for Finder to show its window.
i=0
WIN=""
while [ $i -lt 60 ]; do
  [ -n "$DEV" ] || DEV=$(device_of "$DMG")
  if [ -n "$DEV" ] && WIN=$("$T/ventana" Finder Altrune); then break; fi
  WIN=""
  i=$((i + 1))
  sleep 0.5
done
[ -n "$DEV" ] || fail "the image did not mount in 30 seconds"
[ -n "$WIN" ] || fail "Finder did not show a window titled Altrune in 30 seconds"
# Let Finder finish drawing the icons and the background.
sleep 2
WIN=$("$T/ventana" Finder Altrune) || fail "the window went away"
read -r WID WX WY WW WH <<WINDOW
$WIN
WINDOW
echo "captura: window $WID at $WX,$WY size ${WW}x$WH points"
screencapture -l"$WID" -o -x "$OUT"
[ -s "$OUT" ] || fail "screencapture wrote nothing"
echo "captura: wrote $OUT"
