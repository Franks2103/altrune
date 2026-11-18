#!/bin/sh
# Builds the third-party licence bundle: licencias/terceros.html, made by cargo-about from
# about.toml and licencias/plantilla.hbs. tools/empaquetar.sh copies it into
# Altrune.app/Contents/Resources/Licencias.
#
# Usage:
#   sh tools/licencias.sh                  regenerate licencias/terceros.html (commit the result)
#   sh tools/licencias.sh --comprobar      regenerate into a temporary file and fail if it differs
#                                          from the committed one (use it in CI and before a release)
#   sh tools/licencias.sh --instalar DIR   install the pinned cargo-about into DIR (the only mode
#                                          that uses the network) and print the binary path
#
# cargo-about is read from $CARGO_ABOUT, or from PATH. Its version is pinned below on purpose:
# another version may write the page differently, and then --comprobar would fail for no reason.
# Generating never touches the network: it runs with --locked --offline, so the crates of
# Cargo.lock must already be in the cargo cache (run `cargo fetch --locked` first on a clean
# machine). Nothing is written outside licencias/ and the temporary folder.
set -eu

# The version of cargo-about that makes the page. Change it here only (tests/licencias.rs checks
# that this is the one place), then regenerate and commit licencias/terceros.html.
CARGO_ABOUT_VERSION="0.9.3"

# The folder the person ran it from: a relative CARGO_ABOUT or DIR means "relative to here", not to
# the repository root that the script moves into next.
CALLER=$(pwd)
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
OUT="licencias/terceros.html"
fail() { echo "licencias: $*" >&2; exit 1; }
ok() { echo "  ok  $*"; }

[ -f Cargo.toml ] && [ -f about.toml ] && [ -f licencias/plantilla.hbs ] \
  || fail "run it from the app's repository (Cargo.toml, about.toml or licencias/plantilla.hbs is missing)"

MODE=generar
case "${1:-}" in
  "") ;;
  --comprobar) MODE=comprobar ;;
  --instalar) MODE=instalar ;;
  *) fail "unknown option \"$1\" (use no option, --comprobar or --instalar DIR)" ;;
esac

if [ "$MODE" = instalar ]; then
  [ $# -eq 2 ] || fail "--instalar needs one folder: sh tools/licencias.sh --instalar DIR"
  DIR=$2
  case "$DIR" in
    "" | /) fail "refusing to install into \"$DIR\"" ;;
    /*) ;;
    *) DIR=$CALLER/$DIR ;;
  esac
  command -v cargo >/dev/null 2>&1 || fail "cargo is not installed"
  # "cli" is the feature that builds the cargo-about program (without it cargo installs nothing).
  # A plain version number means exactly that version for cargo install, not a caret range.
  cargo install --locked --root "$DIR" --version "$CARGO_ABOUT_VERSION" --features cli cargo-about >&2
  BIN="$DIR/bin/cargo-about"
  [ -x "$BIN" ] || fail "cargo install finished but $BIN does not exist"
  echo "$BIN"
  exit 0
fi

[ $# -le 1 ] || fail "too many arguments"

ABOUT=${CARGO_ABOUT:-}
if [ -z "$ABOUT" ]; then
  ABOUT=$(command -v cargo-about 2>/dev/null || true)
else
  case "$ABOUT" in
    /*) ;;
    */*) ABOUT=$CALLER/$ABOUT ;;
  esac
fi
[ -n "$ABOUT" ] && [ -x "$ABOUT" ] \
  || fail "cargo-about $CARGO_ABOUT_VERSION is not available: set CARGO_ABOUT to its path, put it on PATH, or install it with: sh tools/licencias.sh --instalar DIR"
FOUND=$("$ABOUT" --version 2>/dev/null | awk 'NR == 1 { print $2 }')
[ "$FOUND" = "$CARGO_ABOUT_VERSION" ] \
  || fail "$ABOUT is cargo-about \"$FOUND\" but this script pins $CARGO_ABOUT_VERSION (install it with: sh tools/licencias.sh --instalar DIR)"

T=$(mktemp -d "${TMPDIR:-/tmp}/altrune-licencias.XXXXXX")
trap 'rm -rf "$T"' EXIT INT TERM

# Writes the page to the file named by $1. --fail stops on a licence that could not be read, and
# any warning of cargo-about is also a failure: a licence file whose checksum no longer matches the
# clarification of about.toml only produces a warning, and the page would silently lose a licence.
# $T holds the log.
generar() {
  if ! "$ABOUT" --color never generate --locked --offline --fail \
    --config about.toml --output-file "$1" licencias/plantilla.hbs 2>"$T/aviso.log"; then
    cat "$T/aviso.log" >&2
    fail "cargo-about failed (if it says a crate is missing, run: cargo fetch --locked)"
  fi
  if [ -s "$T/aviso.log" ]; then
    cat "$T/aviso.log" >&2
    fail "cargo-about warned (see above): fix about.toml or the template instead of ignoring it"
  fi
  [ -s "$1" ] || fail "cargo-about wrote an empty file"
}

case "$MODE" in
  generar)
    generar "$T/terceros.html"
    mkdir -p licencias
    cp "$T/terceros.html" "$OUT"
    ok "wrote $OUT ($(wc -c < "$OUT" | tr -d ' ') bytes)"
    ;;
  comprobar)
    [ -f "$OUT" ] || fail "$OUT does not exist; run: sh tools/licencias.sh"
    generar "$T/terceros.html"
    if cmp -s "$T/terceros.html" "$OUT"; then
      ok "$OUT is up to date"
    else
      diff "$OUT" "$T/terceros.html" | head -n 40 >&2 || true
      fail "$OUT is out of date (Cargo.lock, about.toml or the template changed); run: sh tools/licencias.sh and commit the result"
    fi
    ;;
esac
