#!/bin/sh
# Builds dist/Altrune.app from the release binary and checks it, without installing anything.
#
# Usage: sh tools/empaquetar.sh      (from anywhere; it works from the repository root)
#
#   1. cargo build --release --locked, in its own target folder (target/empaquetar) and with
#      the build machine's paths remapped away, then a guard that fails if the binary still
#      holds /Users/, the home folder or the user name
#   2. dist/Altrune.app/Contents/MacOS/Altrune          the release binary, renamed so the app
#      menu reads "About/Hide/Quit Altrune" (winit names those items after the process)
#      dist/Altrune.app/Contents/Resources/Altrune.icns  from assets/branding
#      dist/Altrune.app/Contents/Resources/Licencias/    LICENSE, NOTICE, THIRD_PARTY_NOTICES.md
#      and licencias/terceros.html (the licence texts of the crates; a warning if it is missing)
#      dist/Altrune.app/Contents/Info.plist              macos/Info.plist with the numeric core
#      of the Cargo.toml version (0.1.0 for 0.1.0-alpha) and the minimum macOS from the binary
#   3. ad hoc signature of the finished bundle (it seals the resources and the icon)
#   4. checks: plist, signature, icon, executable, and the icon Finder will draw
#
# Only dist/Altrune.app is ever replaced. Nothing goes to /Applications or ~/Applications, the
# app is never opened (no window, no --open) and the one run of the binary uses a throwaway HOME.
# The ad hoc signature is for this Mac only: giving the app to other Macs needs a Developer ID
# signature and notarisation (docs/distribucion.md).
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
DIST="$ROOT/dist"
APP="$DIST/Altrune.app"
fail() { echo "empaquetar: $*" >&2; exit 1; }
ok() { echo "  ok  $*"; }

[ -f Cargo.toml ] && [ -f macos/Info.plist ] && [ -f assets/branding/Altrune.icns ] \
  || fail "run it from the app's repository (Cargo.toml, macos/Info.plist or the icon is missing)"

# The full version is the first `version = "..."` of Cargo.toml, which is the package's: x.y.z
# or x.y.z-label (0.1.0-alpha). The plist takes the numeric core only (CFBundleVersion must be
# numbers and dots); the DMG, the tag and the release use the full version.
FULL=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)
VERSION=${FULL%%-*}
case "$VERSION" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) fail "could not read a x.y.z version from Cargo.toml (got \"$FULL\")" ;;
esac
case "$VERSION" in *[!0-9.]*|*..*|.*|*.) fail "the numeric core \"$VERSION\" must be digits and dots" ;; esac
case "$FULL" in
  "$VERSION") ;;
  "$VERSION"-*[!0-9A-Za-z.-]*|"$VERSION"-) fail "the label of the version \"$FULL\" may only hold letters, digits, dots and hyphens" ;;
  "$VERSION"-*) ;;
  *) fail "unexpected version \"$FULL\"" ;;
esac

# The binary must not carry the paths of the machine that built it. Cargo's trim-paths is still
# unstable on stable toolchains, so rustc is told to remap them: the home folder, the cargo
# folder (dependency sources) and this repository. The flags travel in CARGO_ENCODED_RUSTFLAGS
# (separator 0x1f) because the paths may hold spaces. A dedicated target folder keeps these
# builds apart from the ones for tests and development, which would otherwise rebuild everything
# each time the flags change. Order matters: the last matching rule wins, so the most specific
# comes last.
# CARGO_ENCODED_RUSTFLAGS wins over every other source of rustc flags, so flags the caller set
# would be dropped without a word: refuse instead.
for v in RUSTFLAGS CARGO_ENCODED_RUSTFLAGS CARGO_BUILD_RUSTFLAGS; do
  eval "val=\${$v:-}"
  [ -z "$val" ] || fail "$v is set: this script sets its own rustc flags (path remapping) and would discard yours; unset it"
done
CARGO_DIR=${CARGO_HOME:-$HOME/.cargo}
SEP=$(printf '\037')
CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$HOME=/home${SEP}--remap-path-prefix=$CARGO_DIR=/cargo${SEP}--remap-path-prefix=$ROOT=."
CARGO_TARGET_DIR="$ROOT/target/empaquetar"
export CARGO_ENCODED_RUSTFLAGS CARGO_TARGET_DIR

echo "Building the release binary (locked, build paths remapped)..."
cargo build --release --locked
BIN="$CARGO_TARGET_DIR/release/altrune"
[ -x "$BIN" ] || fail "no release binary at $BIN"

# Guard: look at the strings of the finished binary, not at the flags.
guard_paths() {
  hits=$(strings -a "$1" | grep -c -e '/Users/' -e "$HOME" -e "/$(id -un)/" || true)
  [ "$hits" = 0 ] || fail "$1 still holds $hits strings with /Users/, the home folder or the user name"
}
guard_paths "$BIN"
ok "no /Users/, home folder or user name in the release binary"

# The minimum macOS is a fact of the binary, not a guess.
MINOS=$(vtool -show-build "$BIN" | awk '$1 == "minos" { print $2; exit }')
case "$MINOS" in
  [0-9]*.[0-9]*|[0-9]*) ;;
  *) fail "could not read the minimum macOS from $BIN (got \"$MINOS\")" ;;
esac

# Replace only dist/Altrune.app. The path is built from fixed names; refuse anything odd first.
[ ! -L "$DIST" ] || fail "$DIST is a link; remove it yourself"
mkdir -p "$DIST"
[ ! -L "$APP" ] || fail "$APP is a link; remove it yourself"
case "$APP" in
  */dist/Altrune.app) ;;
  *) fail "unexpected bundle path $APP" ;;
esac
if [ -e "$APP" ]; then rm -rf "$APP"; fi

mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/Altrune"
chmod 755 "$APP/Contents/MacOS/Altrune"
cp assets/branding/Altrune.icns "$APP/Contents/Resources/Altrune.icns"
mkdir -p "$APP/Contents/Resources/Licencias"
cp LICENSE NOTICE THIRD_PARTY_NOTICES.md "$APP/Contents/Resources/Licencias/"
if [ -f licencias/terceros.html ]; then
  cp licencias/terceros.html "$APP/Contents/Resources/Licencias/terceros.html"
else
  echo "empaquetar: warning: licencias/terceros.html is missing; run sh tools/licencias.sh, the app would ship without the licence texts of its crates" >&2
fi
sed -e "s/@VERSION@/$VERSION/g" -e "s/@MINOS@/$MINOS/g" macos/Info.plist > "$APP/Contents/Info.plist"
xattr -cr "$APP" 2>/dev/null || true

# Sign last, once every file is in place: the signature seals the icon and the plist.
codesign --force --sign - --timestamp=none "$APP"

echo "Checking $APP"
plutil -lint "$APP/Contents/Info.plist" >/dev/null || fail "Info.plist is not a valid plist"
ok "Info.plist is valid"
plist() { plutil -extract "$1" raw -o - "$APP/Contents/Info.plist"; }
[ "$(plist CFBundleIdentifier)" = com.altrune.app ] || fail "wrong CFBundleIdentifier"
[ "$(plist CFBundleShortVersionString)" = "$VERSION" ] || fail "wrong version in the plist"
[ "$(plist LSMinimumSystemVersion)" = "$MINOS" ] || fail "wrong LSMinimumSystemVersion"
case "$(grep -c '@[A-Z]*@' "$APP/Contents/Info.plist")" in 0) ;; *) fail "unfilled marker left in Info.plist" ;; esac
ok "plist: com.altrune.app, version $VERSION (release $FULL), macOS $MINOS or later"
[ "$(plist CFBundleVersion)" = "$VERSION" ] || fail "wrong CFBundleVersion in the plist"
for f in LICENSE NOTICE THIRD_PARTY_NOTICES.md; do
  cmp -s "$f" "$APP/Contents/Resources/Licencias/$f" || fail "Licencias/$f is missing or differs"
done
ok "Contents/Resources/Licencias holds LICENSE, NOTICE and THIRD_PARTY_NOTICES.md"
guard_paths "$APP/Contents/MacOS/Altrune"

VERIFY=$(codesign --verify --strict --verbose=2 "$APP" 2>&1) || { echo "$VERIFY" >&2; fail "codesign --verify --strict failed"; }
echo "$VERIFY" | sed "s|$DIST/|      |"
SIG=$(codesign -dv "$APP" 2>&1)
echo "$SIG" | grep -q '^Identifier=com.altrune.app$' || fail "signature identifier is not com.altrune.app"
echo "$SIG" | grep -q 'Signature=adhoc' || fail "the signature is not ad hoc"
echo "$SIG" | grep -q 'Sealed Resources' || fail "the signature does not seal the resources"
ok "signature: ad hoc, identifier com.altrune.app, resources sealed"

ICON_NAME=$(plist CFBundleIconFile)
[ -f "$APP/Contents/Resources/$ICON_NAME.icns" ] || fail "icon $ICON_NAME.icns is not in Resources"
cmp -s "$APP/Contents/Resources/$ICON_NAME.icns" assets/branding/Altrune.icns || fail "icon differs from assets/branding"
EXE_NAME=$(plist CFBundleExecutable)
[ -x "$APP/Contents/MacOS/$EXE_NAME" ] || fail "executable $EXE_NAME is missing or not executable"
ok "icon $ICON_NAME.icns and executable $EXE_NAME are where the plist says"

# One run of the binary, only with a throwaway HOME: no window, no Wine, nothing real is read.
T=$(mktemp -d "${TMPDIR:-/tmp}/altrune-paquete.XXXXXX")
case "$T" in /private/tmp/*|/tmp/*|/var/folders/*|/private/var/folders/*) ;; *) fail "unexpected temp folder $T" ;; esac
sh "$ROOT/tools/hogar-aislado.sh" "$T" vacio >/dev/null
mkdir -p "$T/medida"
HOME="$T" "$APP/Contents/MacOS/$EXE_NAME" --medir "$T/medida" --sin-wine >/dev/null 2>"$T/medir.txt" \
  || { cat "$T/medir.txt" >&2; fail "the bundled binary failed its --medir run"; }
ok "the bundled binary ran --medir --sin-wine in an isolated HOME"

# What Finder and the Dock will draw: ask the system for the bundle's icon and keep it as a PNG.
# The system may tint the picture with the Mac's icon style (Dark, Tinted), so the colours of
# the PNG can differ from the icon file; the shape is what shows that the icon loaded.
cat > "$T/icono.swift" <<'S'
import AppKit
let args = CommandLine.arguments
let icon = NSWorkspace.shared.icon(forFile: args[1])
guard let tiff = icon.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff),
      let png = rep.representation(using: .png, properties: [:]) else { exit(1) }
try png.write(to: URL(fileURLWithPath: args[2]))
S
# This step needs Swift and a desktop session. A person running it by hand gets a hard failure;
# on a CI runner (no desktop session, and the PNG is only for a person to look at) it is a warning.
ICON_ERR=""
if ! swiftc -O -o "$T/icono" "$T/icono.swift" 2>"$T/swiftc.txt"; then
  cat "$T/swiftc.txt" >&2
  ICON_ERR="could not build the icon check"
elif ! "$T/icono" "$APP" "$DIST/icono-comprobado.png"; then
  ICON_ERR="the system gave no icon for the bundle"
fi
if [ -z "$ICON_ERR" ]; then
  ok "Finder's icon for the bundle saved at dist/icono-comprobado.png (look at it)"
elif [ -n "${CI:-}" ]; then
  echo "empaquetar: warning: $ICON_ERR; icon picture skipped on CI" >&2
else
  fail "$ICON_ERR"
fi

case "$T" in */altrune-paquete.*) rm -rf "$T" ;; esac
echo "Done: $APP (release $FULL)"
