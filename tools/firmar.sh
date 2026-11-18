#!/bin/sh
# Signs Altrune.app or Altrune-<version>.dmg with a Developer ID identity, then verifies the
# signature. It signs and checks; it never notarises (tools/notarizar.sh), never builds the app
# (tools/empaquetar.sh), never builds the image (tools/crear-dmg.sh) and never opens anything.
#
# Usage:
#   sh tools/firmar.sh app PATH.app        hardened runtime + secure timestamp + entitlements
#   sh tools/firmar.sh dmg PATH.dmg        secure timestamp
#   sh tools/firmar.sh --prueba app PATH.app   local check only, see below
#
# Environment:
#   ALTRUNE_IDENTIDAD   "Developer ID Application: Name (TEAMID)" or the identity's SHA-1 (40 hex
#                       digits). Required except with --prueba. The identity "-" (ad hoc) and any
#                       identity that is not a Developer ID Application one are refused.
#   ALTRUNE_LLAVERO     optional: path of the keychain that holds the identity (the release
#                       workflow passes the temporary one made by tools/llavero-ci.sh).
#
# --prueba signs with the ad hoc identity "-" but with the same hardened runtime and the same
# entitlements, so that this Mac can show that the hardened runtime does not stop Altrune from
# starting. It signs the path it is given, so pass a COPY of the app (it refuses dist/Altrune.app).
# An ad hoc signature is never for other Macs: Gatekeeper rejects it.
#
# Order of signing: inside out. Code nested in the bundle (helpers in Contents/MacOS, anything in
# Frameworks, PlugIns, XPCServices, Helpers or Library, any *.dylib, *.so, *.framework, *.xpc,
# *.appex or *.app) is signed first, each with the runtime and the timestamp but without the
# entitlements, and the bundle itself last, because its signature seals the nested ones. Today
# the bundle holds one executable and nothing else, so nothing is nested.
#
# It never prints a secret: the identity and the keychain path are used as arguments only.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
ENTITLEMENTS="$ROOT/macos/Altrune.entitlements"
fail() { echo "firmar: $*" >&2; exit 1; }
ok() { echo "  ok  $*"; }
usage() {
  echo "usage: sh tools/firmar.sh [--prueba] app PATH.app | dmg PATH.dmg" >&2
  echo "  env: ALTRUNE_IDENTIDAD (Developer ID Application identity or its SHA-1), ALTRUNE_LLAVERO (optional keychain)" >&2
  exit 2
}

PRUEBA=0
if [ "${1:-}" = "--prueba" ]; then PRUEBA=1; shift; fi
[ "$#" -eq 2 ] || usage
KIND="$1"
TARGET="$2"
case "$KIND" in app|dmg) ;; *) usage ;; esac

# --- The identity ---------------------------------------------------------------------------------
if [ "$PRUEBA" -eq 1 ]; then
  IDENT="-"
else
  IDENT="${ALTRUNE_IDENTIDAD:-}"
  if [ -z "$IDENT" ]; then
    echo "firmar: set ALTRUNE_IDENTIDAD to a Developer ID Application identity (or use --prueba for a local check)" >&2
    exit 2
  fi
  if [ "$IDENT" = "-" ]; then
    fail "the ad hoc identity \"-\" is refused: it is only for --prueba, never for distribution"
  fi
  case "$IDENT" in
    "Developer ID Application: "*) ;;
    *[!0-9A-Fa-f]*) fail "the identity must be a \"Developer ID Application: ...\" one or its SHA-1" ;;
    *)
      [ "${#IDENT}" -eq 40 ] || fail "the identity must be a \"Developer ID Application: ...\" one or its SHA-1 (40 hex digits)"
      # A SHA-1 says nothing about the kind of certificate: look it up, read-only.
      if [ -n "${ALTRUNE_LLAVERO:-}" ]; then
        FOUND=$(security find-identity -v -p codesigning "$ALTRUNE_LLAVERO" 2>/dev/null | grep -i "$IDENT" | grep "Developer ID Application" || true)
      else
        FOUND=$(security find-identity -v -p codesigning 2>/dev/null | grep -i "$IDENT" | grep "Developer ID Application" || true)
      fi
      [ -n "$FOUND" ] || fail "that SHA-1 is not a valid Developer ID Application identity in the keychain"
      ;;
  esac
fi
if [ -n "${ALTRUNE_LLAVERO:-}" ] && [ ! -f "$ALTRUNE_LLAVERO" ]; then fail "ALTRUNE_LLAVERO does not name a keychain file"; fi

# --- The target -----------------------------------------------------------------------------------
case "$KIND" in
  app)
    case "$TARGET" in *.app) ;; *) fail "the app path must end in .app" ;; esac
    [ -d "$TARGET" ] && [ ! -L "$TARGET" ] || fail "$TARGET is not an app bundle folder"
    [ -f "$TARGET/Contents/Info.plist" ] || fail "$TARGET has no Contents/Info.plist"
    [ "$(plutil -extract CFBundleIdentifier raw -o - "$TARGET/Contents/Info.plist")" = "com.altrune.app" ] \
      || fail "$TARGET is not Altrune (bundle identifier is not com.altrune.app)"
    [ -f "$ENTITLEMENTS" ] || fail "missing $ENTITLEMENTS"
    plutil -lint "$ENTITLEMENTS" >/dev/null || fail "$ENTITLEMENTS is not a valid plist"
    if [ "$PRUEBA" -eq 1 ]; then
      case "$(cd "$TARGET" && pwd -P)" in
        */dist/Altrune.app) fail "--prueba would replace the ad hoc signature of dist/Altrune.app: pass a copy (ditto dist/Altrune.app SOME/COPY/Altrune.app)" ;;
      esac
    fi
    ;;
  dmg)
    case "$TARGET" in *.dmg) ;; *) fail "the image path must end in .dmg" ;; esac
    [ -f "$TARGET" ] && [ ! -L "$TARGET" ] || fail "$TARGET is not a disk image file"
    ;;
esac

# --- Signing --------------------------------------------------------------------------------------
# sign_one PATH ENTITLEMENTS(yes|no): one codesign call; the options are built in $@.
sign_one() {
  _path="$1"
  _ent="$2"
  set -- --force --sign "$IDENT"
  if [ "$KIND" = app ]; then set -- "$@" --options runtime; fi
  if [ "$PRUEBA" -eq 1 ]; then set -- "$@" --timestamp=none; else set -- "$@" --timestamp; fi
  if [ "$_ent" = yes ]; then set -- "$@" --entitlements "$ENTITLEMENTS"; fi
  if [ -n "${ALTRUNE_LLAVERO:-}" ]; then set -- "$@" --keychain "$ALTRUNE_LLAVERO"; fi
  codesign "$@" "$_path"
}

if [ "$KIND" = app ]; then
  MAIN=$(plutil -extract CFBundleExecutable raw -o - "$TARGET/Contents/Info.plist")
  LIST=$(mktemp "${TMPDIR:-/tmp}/altrune-firmar.XXXXXX")
  trap 'rm -f "$LIST"' EXIT
  # Deepest first (-depth), so a nested bundle is signed after what it contains.
  find "$TARGET/Contents" -depth \( -name '*.dylib' -o -name '*.so' -o -name '*.framework' \
    -o -name '*.xpc' -o -name '*.appex' -o -name '*.app' \) > "$LIST"
  if [ -d "$TARGET/Contents/MacOS" ]; then
    find "$TARGET/Contents/MacOS" -type f ! -name "$MAIN" >> "$LIST"
  fi
  NESTED=0
  while IFS= read -r item; do
    [ -n "$item" ] || continue
    echo "Signing nested code: ${item#"$TARGET"/}"
    sign_one "$item" no
    NESTED=$((NESTED + 1))
  done < "$LIST"
  echo "Signing $TARGET (nested items signed first: $NESTED)"
  sign_one "$TARGET" yes
else
  echo "Signing $TARGET"
  sign_one "$TARGET" no
fi

# --- Verifying ------------------------------------------------------------------------------------
echo "Verifying..."
if [ "$KIND" = app ]; then
  codesign --verify --deep --strict --verbose=2 "$TARGET" || fail "the signature does not verify"
  ok "codesign --verify --deep --strict"
else
  codesign --verify --strict --verbose=2 "$TARGET" || fail "the signature does not verify"
  ok "codesign --verify --strict"
fi

INFO=$(codesign -dv --verbose=4 "$TARGET" 2>&1) || fail "codesign -dv failed"
if [ "$KIND" = app ]; then
  printf '%s\n' "$INFO" | grep -E 'flags=0x[0-9a-f]+\(.*runtime' >/dev/null \
    || fail "the hardened runtime flag is missing from the code directory"
  ok "hardened runtime flag present"
fi
if [ "$PRUEBA" -eq 1 ]; then
  printf '%s\n' "$INFO" | grep -q 'Signature=adhoc' || fail "--prueba should have produced an ad hoc signature"
  ok "ad hoc signature (local check only, Gatekeeper will reject it)"
else
  printf '%s\n' "$INFO" | grep -q '^TeamIdentifier=' || fail "no TeamIdentifier in the signature"
  if printf '%s\n' "$INFO" | grep -q '^TeamIdentifier=not set'; then fail "TeamIdentifier is not set"; fi
  ok "TeamIdentifier present"
  printf '%s\n' "$INFO" | grep -q '^Authority=Developer ID Application' || fail "the signing authority is not a Developer ID Application certificate"
  ok "Authority is Developer ID Application"
  printf '%s\n' "$INFO" | grep -q '^Timestamp=' || fail "the signature has no secure timestamp"
  ok "secure timestamp present"
fi

if [ "$KIND" = app ]; then
  echo "Entitlements in the signature:"
  ENT=$(codesign -d --entitlements :- "$TARGET" 2>/dev/null) || fail "could not read the entitlements"
  printf '%s\n' "$ENT"
  printf '%s\n' "$ENT" | plutil -extract 'com\.apple\.security\.automation\.apple-events' raw -o - - 2>/dev/null | grep -qx true \
    || fail "the Apple events entitlement is not in the signature"
  # Exactly the keys of the file: the same number, and the Apple events one among them.
  WANT=$(grep -o '<key>' "$ROOT/macos/Altrune.entitlements" | wc -l | tr -d ' ')
  HAVE=$(printf '%s\n' "$ENT" | grep -o '<key>' | wc -l | tr -d ' ')
  [ "$WANT" = "$HAVE" ] || fail "the signature carries $HAVE entitlement keys and macos/Altrune.entitlements has $WANT"
  ok "entitlements are those of macos/Altrune.entitlements ($HAVE key)"
fi
echo "Signed: $TARGET"
