#!/bin/sh
# Installs the Altrune.app built by tools/empaquetar.sh into a folder of applications, so the app
# (and the Finder opener, which points at whichever Altrune last ran) has a fixed place.
#
# Usage: sh tools/instalar.sh [folder]      (default folder: /Applications)
#
# Safety:
#   - the source is dist/Altrune.app (or $ALTRUNE_APP); it must carry a valid signature with the
#     identifier com.altrune.app;
#   - an existing <folder>/Altrune.app is only replaced when it is Altrune too (same identifier);
#     anything else with that name is left alone and the script stops;
#   - it stops while Altrune runs from that place (checked before the copy and again just before
#     the swap): replacing a running app could break it;
#   - the new copy is made beside the old one, checked, and only then swapped in; the old copy is
#     removed last, after the installed one verifies. A failure or an interruption before the swap
#     leaves the installed app as it was, and a half-made copy is removed;
#   - it never opens the app and never reads or writes the person's data.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
SRC="${ALTRUNE_APP:-$ROOT/dist/Altrune.app}"
DEST_DIR="${1:-/Applications}"
fail() { echo "instalar: $*" >&2; exit 1; }
ok() { echo "  ok  $*"; }

ID=com.altrune.app
bundle_id() { plutil -extract CFBundleIdentifier raw -o - "$1/Contents/Info.plist" 2>/dev/null || true; }

# The source: a signed Altrune bundle.
[ -d "$SRC" ] && [ ! -L "$SRC" ] || fail "no bundle at $SRC (run sh tools/empaquetar.sh first)"
[ "$(bundle_id "$SRC")" = "$ID" ] || fail "$SRC is not Altrune ($ID)"
codesign --verify --strict "$SRC" 2>/dev/null || fail "the signature of $SRC does not verify"

# The destination folder: an existing, real folder.
[ -d "$DEST_DIR" ] && [ ! -L "$DEST_DIR" ] || fail "$DEST_DIR is not a folder"
DEST_GIVEN="$DEST_DIR"
DEST_DIR=$(cd "$DEST_DIR" && pwd -P)
DEST="$DEST_DIR/Altrune.app"
NEW="$DEST_DIR/.Altrune.app.nuevo"
OLD="$DEST_DIR/.Altrune.app.anterior"
case "$SRC" in "$DEST") fail "the source and the destination are the same bundle" ;; esac

# Whether Altrune runs from DEST. lsof finds every process that has the installed executable
# open, whatever spelling of the path started it (/var or /private/var, other case, relative).
# Without lsof, the process list is matched against both spellings of the folder, ignoring case
# (it is read first and matched here: a grep for the path would find itself).
lower() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }
runs_from_dest() {
  exe="$DEST/Contents/MacOS/Altrune"
  [ -e "$exe" ] || return 1
  if [ -x /usr/sbin/lsof ]; then
    if /usr/sbin/lsof -t -- "$exe" >/dev/null 2>&1; then return 0; fi
    return 1
  fi
  running=$(lower "$(ps -axo command=)")
  for folder in "$DEST_DIR" "$DEST_GIVEN"; do
    case "$running" in *"$(lower "$folder/Altrune.app/Contents/MacOS/")"*) return 0 ;; esac
  done
  return 1
}

# An interruption between the two moves of an earlier run left the last working Altrune aside:
# it goes back first, so it is kept until a new copy has verified.
if [ ! -e "$DEST" ] && [ ! -L "$DEST" ] && [ -d "$OLD" ] && [ ! -L "$OLD" ] \
   && [ "$(bundle_id "$OLD")" = "$ID" ]; then
  mv "$OLD" "$DEST"
  ok "restored the Altrune an interrupted run had moved aside"
fi

# Only Altrune may be replaced, and not while it runs from there.
if [ -e "$DEST" ] || [ -L "$DEST" ]; then
  [ ! -L "$DEST" ] || fail "$DEST is a link; remove it yourself"
  [ "$(bundle_id "$DEST")" = "$ID" ] || fail "$DEST exists and is not Altrune: nothing was changed"
fi
if runs_from_dest; then fail "Altrune is open from $DEST: quit it first (Altrune › Quit Altrune)"; fi

# Leftovers of an interrupted run are removed when they are ours: Altrune bundles, or half-made
# copies with our staging name that never got their Info.plist (it is copied last). A bundle of
# another app under these names stops the script.
for leftover in "$NEW" "$OLD"; do
  if [ -e "$leftover" ] || [ -L "$leftover" ]; then
    [ ! -L "$leftover" ] && [ -d "$leftover" ] || fail "$leftover is not a folder of ours: nothing was changed"
    if [ -f "$leftover/Contents/Info.plist" ]; then
      [ "$(bundle_id "$leftover")" = "$ID" ] || fail "$leftover is another app: nothing was changed"
    fi
    rm -rf "$leftover"
  fi
done

# Copy beside the installed app (same volume, so the swap is a rename), then check the copy. Until
# the swap, an interruption or a failure removes the half-made copy and nothing else.
trap 'rm -rf "$NEW"' EXIT
trap 'exit 130' INT TERM
ditto "$SRC" "$NEW" || fail "the copy failed (disk full or no permission); nothing was changed"
codesign --verify --strict "$NEW" 2>/dev/null || fail "the copy does not verify; nothing was changed"
cmp -s "$SRC/Contents/MacOS/Altrune" "$NEW/Contents/MacOS/Altrune" || fail "the copy differs from the source; nothing was changed"
ok "copy made and verified"

# Last look before touching the installed app: it may have been opened meanwhile.
if runs_from_dest; then fail "Altrune was opened from $DEST meanwhile: quit it and run this again"; fi

# Swap: old aside, new in place. From here the copy belongs to the installed app.
trap - EXIT
if [ -e "$DEST" ]; then mv "$DEST" "$OLD"; fi
if ! mv "$NEW" "$DEST"; then
  if [ -e "$OLD" ]; then mv "$OLD" "$DEST"; fi
  rm -rf "$NEW"
  fail "could not put the new copy in place; the installed app was restored"
fi
trap - INT TERM

# The old copy goes only once the installed one verifies; otherwise the old one comes back.
if ! codesign --verify --strict "$DEST" 2>/dev/null; then
  if [ -e "$OLD" ]; then
    mv "$DEST" "$NEW" && mv "$OLD" "$DEST" && rm -rf "$NEW"
    fail "the installed bundle did not verify; the earlier Altrune was put back"
  fi
  fail "the installed bundle does not verify"
fi
if [ -e "$OLD" ]; then rm -rf "$OLD"; fi
ok "installed at $DEST"

# Tell Launch Services now, so Finder, Spotlight and Launchpad list it without waiting.
# (ALTRUNE_NO_REGISTER=1 skips it: the tests install into throwaway folders.)
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
if [ "${ALTRUNE_NO_REGISTER:-}" != 1 ] && [ -x "$LSREGISTER" ]; then
  "$LSREGISTER" -f "$DEST" >/dev/null 2>&1 && ok "registered with Launch Services"
fi
echo "Done: open it from $DEST_DIR (the Finder opener will point there after its first start)"
