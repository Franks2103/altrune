#!/bin/sh
# Sends a Developer ID signed Altrune.app or Altrune-<version>.dmg to Apple's notary service,
# waits for the verdict, staples the ticket and checks that Gatekeeper accepts the result.
#
# Usage: sh tools/notarizar.sh PATH.app|PATH.dmg
#
# Credentials (environment; nothing is read from anywhere else, nothing is stored):
#   ALTRUNE_NOTARIO_PERFIL     the name of a keychain profile made with
#                              `xcrun notarytool store-credentials`
#   or the three of an App Store Connect API key (recommended, and what the release workflow uses):
#   ALTRUNE_NOTARIO_CLAVE      path of the AuthKey_XXXX.p8 file
#   ALTRUNE_NOTARIO_CLAVE_ID   the key's ID
#   ALTRUNE_NOTARIO_EMISOR     the issuer ID (a UUID; use a Team key from App Store Connect)
#   ALTRUNE_LLAVERO            optional keychain path, only with the profile
#   ALTRUNE_NOTARIO_ESPERA     optional wait limit for the verdict (default 30m; digits and s, m or h)
#   ALTRUNE_NOTARIO_GRAPAR_ESPERA  optional seconds between stapling attempts (default 15, 5 attempts)
# Without credentials it prints what is missing, does nothing and exits 2.
#
# An .app is zipped with `ditto -c -k --keepParent` (the zip is only for the upload; the ticket is
# stapled to the app itself). A .dmg is sent as it is. On a verdict other than Accepted the log
# is saved in dist/notary-log-<id>.json (git ignores it) and the script fails. Then:
#   xcrun stapler staple, xcrun stapler validate, and spctl (-t exec for the app,
#   -t open --context context:primary-signature for the image).
#
# It never prints a credential: the profile name, key path, key ID and issuer are only passed as
# arguments to notarytool.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
DIST="$ROOT/dist"
fail() { echo "notarizar: $*" >&2; exit 1; }
ok() { echo "  ok  $*"; }

[ "$#" -eq 1 ] || { echo "usage: sh tools/notarizar.sh PATH.app|PATH.dmg" >&2; exit 2; }
TARGET="$1"

# --- Credentials: which way, and is it complete? --------------------------------------------------
PERFIL="${ALTRUNE_NOTARIO_PERFIL:-}"
CLAVE="${ALTRUNE_NOTARIO_CLAVE:-}"
CLAVE_ID="${ALTRUNE_NOTARIO_CLAVE_ID:-}"
EMISOR="${ALTRUNE_NOTARIO_EMISOR:-}"
if [ -n "$PERFIL" ]; then
  MODE="the keychain profile"
elif [ -n "$CLAVE" ] || [ -n "$CLAVE_ID" ] || [ -n "$EMISOR" ]; then
  MISSING=""
  [ -n "$CLAVE" ] || MISSING="$MISSING ALTRUNE_NOTARIO_CLAVE"
  [ -n "$CLAVE_ID" ] || MISSING="$MISSING ALTRUNE_NOTARIO_CLAVE_ID"
  [ -n "$EMISOR" ] || MISSING="$MISSING ALTRUNE_NOTARIO_EMISOR"
  if [ -n "$MISSING" ]; then
    echo "notarizar: the API key credentials are incomplete, missing:$MISSING" >&2
    exit 2
  fi
  MODE="the App Store Connect API key"
else
  {
    echo "notarizar: no notarisation credentials, nothing was done."
    echo "  Set ALTRUNE_NOTARIO_PERFIL (a profile made with xcrun notarytool store-credentials)"
    echo "  or ALTRUNE_NOTARIO_CLAVE + ALTRUNE_NOTARIO_CLAVE_ID + ALTRUNE_NOTARIO_EMISOR"
    echo "  (an App Store Connect API key). See docs/firma-y-notarizacion.md."
  } >&2
  exit 2
fi

# --- Options ------------------------------------------------------------------------------------
ESPERA="${ALTRUNE_NOTARIO_ESPERA:-30m}"
case "$ESPERA" in
  ''|*[!0-9smh]*) fail "ALTRUNE_NOTARIO_ESPERA must be a number with an optional s, m or h" ;;
esac

# --- The target ---------------------------------------------------------------------------------
case "$TARGET" in
  *.app) KIND=app; [ -d "$TARGET" ] && [ ! -L "$TARGET" ] || fail "$TARGET is not an app bundle folder" ;;
  *.dmg) KIND=dmg; [ -f "$TARGET" ] && [ ! -L "$TARGET" ] || fail "$TARGET is not a disk image file" ;;
  *) echo "usage: sh tools/notarizar.sh PATH.app|PATH.dmg" >&2; exit 2 ;;
esac
if [ "$KIND" = app ]; then
  [ "$(plutil -extract CFBundleIdentifier raw -o - "$TARGET/Contents/Info.plist" 2>/dev/null)" = "com.altrune.app" ] \
    || fail "$TARGET is not Altrune (bundle identifier is not com.altrune.app)"
fi

# Apple rejects what is not signed with a Developer ID certificate; say so before uploading.
INFO=$(codesign -dv --verbose=4 "$TARGET" 2>&1) || fail "$TARGET is not signed: run tools/firmar.sh first"
if printf '%s\n' "$INFO" | grep -q 'Signature=adhoc'; then
  fail "$TARGET has an ad hoc signature, which Apple rejects: sign it with tools/firmar.sh and a Developer ID identity first"
fi
printf '%s\n' "$INFO" | grep -q '^Authority=Developer ID Application' \
  || fail "$TARGET is not signed with a Developer ID Application certificate"
if [ "$KIND" = app ]; then
  printf '%s\n' "$INFO" | grep -E 'flags=0x[0-9a-f]+\(.*runtime' >/dev/null \
    || fail "$TARGET was signed without the hardened runtime, which notarisation requires"
fi
ok "signed with Developer ID"

# One notarytool call with the credentials appended. They are only ever arguments here.
nt() {
  _sub="$1"
  shift
  if [ -n "$PERFIL" ]; then
    if [ -n "${ALTRUNE_LLAVERO:-}" ]; then
      xcrun notarytool "$_sub" "$@" --keychain-profile "$PERFIL" --keychain "$ALTRUNE_LLAVERO"
    else
      xcrun notarytool "$_sub" "$@" --keychain-profile "$PERFIL"
    fi
  else
    xcrun notarytool "$_sub" "$@" --key "$CLAVE" --key-id "$CLAVE_ID" --issuer "$EMISOR"
  fi
}
if [ -z "$PERFIL" ] && [ ! -f "$CLAVE" ]; then
  fail "ALTRUNE_NOTARIO_CLAVE does not name a file"
fi

WORK=$(mktemp -d "${TMPDIR:-/tmp}/altrune-notarizar.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

# --- Upload and wait ----------------------------------------------------------------------------
if [ "$KIND" = app ]; then
  UPLOAD="$WORK/Altrune.zip"
  echo "Zipping the app for the upload..."
  ditto -c -k --keepParent "$TARGET" "$UPLOAD"
else
  UPLOAD="$TARGET"
fi

echo "Submitting to the notary service with $MODE (waiting up to $ESPERA)..."
RESULT="$WORK/resultado.json"
RC=0
nt submit "$UPLOAD" --wait --timeout "$ESPERA" --output-format json > "$RESULT" || RC=$?

# Read the JSON from its first line that starts with a brace, in case text comes before it.
sed -n '/^{/,$p' "$RESULT" > "$WORK/json" || true
ID=$(plutil -extract id raw -o - "$WORK/json" 2>/dev/null || true)
STATUS=$(plutil -extract status raw -o - "$WORK/json" 2>/dev/null || true)
case "$ID" in
  ''|*[!0-9A-Fa-f-]*) ID="" ;;
esac
if [ -z "$ID" ]; then
  # The answer holds no credentials, and the upload may exist at Apple: show it, point to the history.
  echo "notarytool answered (exit code $RC):" >&2
  cat "$RESULT" >&2
  fail "notarytool gave no submission id: nothing was accepted. If the upload started, find it with: xcrun notarytool history"
fi
echo "  submission $ID: ${STATUS:-no status}"

case "$STATUS" in
  Accepted) ;;
  Invalid|Rejected)
    mkdir -p "$DIST"
    LOG="$DIST/notary-log-$ID.json"
    nt log "$ID" "$LOG" >/dev/null 2>&1 || true
    if [ -s "$LOG" ]; then
      fail "notarisation was not accepted ($STATUS); the log is in dist/notary-log-$ID.json"
    fi
    fail "notarisation was not accepted ($STATUS) and the log could not be downloaded; try: xcrun notarytool log $ID"
    ;;
  *)
    # Still processing after the wait limit (or an answer we do not know): this is not a rejection.
    fail "no verdict yet (${STATUS:-no status}) after waiting $ESPERA; Apple keeps processing submission $ID: check it with xcrun notarytool info $ID, then run this script again or staple later with xcrun stapler staple"
    ;;
esac
ok "Apple accepted the submission"

# --- Staple and check ---------------------------------------------------------------------------
# Right after Accepted the ticket can take a moment to reach Apple's servers ("Record not found"),
# so stapling is tried a few times before giving up.
GRAPAR_ESPERA="${ALTRUNE_NOTARIO_GRAPAR_ESPERA:-15}"
case "$GRAPAR_ESPERA" in
  ''|*[!0-9]*) fail "ALTRUNE_NOTARIO_GRAPAR_ESPERA must be a number of seconds" ;;
esac
TRY=1
until xcrun stapler staple "$TARGET"; do
  [ "$TRY" -lt 5 ] || fail "stapler staple failed 5 times; Apple did accept submission $ID, so the ticket can be stapled later with: xcrun stapler staple $TARGET"
  echo "  the ticket is not available yet, trying again in $GRAPAR_ESPERA s ($TRY of 5)..."
  TRY=$((TRY + 1))
  sleep "$GRAPAR_ESPERA"
done
xcrun stapler validate "$TARGET" || fail "stapler validate failed"
ok "ticket stapled and valid"

if [ "$KIND" = app ]; then
  SP=$(spctl -a -vv -t exec "$TARGET" 2>&1) || { printf '%s\n' "$SP" >&2; fail "Gatekeeper rejects the app"; }
else
  SP=$(spctl -a -vv -t open --context context:primary-signature "$TARGET" 2>&1) \
    || { printf '%s\n' "$SP" >&2; fail "Gatekeeper rejects the image"; }
fi
printf '%s\n' "$SP"
ok "Gatekeeper accepts it"
echo "Notarised: $TARGET"
