#!/bin/sh
# Makes and removes the temporary keychain that signs the release on a GitHub Actions macOS
# runner. FOR CI ONLY: it refuses to run unless CI=true and RUNNER_TEMP is set, because it changes
# the keychain search list and imports a private key, which must never happen on a person's Mac.
#
# Usage (in the release workflow, environment "release"):
#   KEYCHAIN=$(sh tools/llavero-ci.sh crear)     # prints the keychain path and nothing else on stdout
#   ... sh tools/firmar.sh ... with ALTRUNE_LLAVERO="$KEYCHAIN"
#   sh tools/llavero-ci.sh borrar                # always, even after a failure (if: always())
#
# Environment for crear:
#   MACOS_CERTIFICADO_P12_BASE64   the Developer ID Application certificate and its private key
#                                  as a .p12 file, base64 encoded (one GitHub secret)
#   MACOS_CERTIFICADO_CLAVE        the password of that .p12 (one GitHub secret)
#
# crear: a keychain in $RUNNER_TEMP with a random password that exists only in this process,
# the .p12 imported (the decoded file is deleted at once), key access granted to Apple's tools
# without a prompt (set-key-partition-list), the keychain put first in the search list (the old
# list is saved to be restored). borrar: restores the saved list and deletes the keychain.
# Nothing here prints the password, the certificate or its password; messages go to stderr.
set -eu

fail() { echo "llavero-ci: $*" >&2; exit 1; }
say() { echo "llavero-ci: $*" >&2; }

[ "$#" -eq 1 ] || { echo "usage: sh tools/llavero-ci.sh crear | borrar   (CI only)" >&2; exit 2; }
ACTION="$1"
case "$ACTION" in crear|borrar) ;; *) echo "usage: sh tools/llavero-ci.sh crear | borrar   (CI only)" >&2; exit 2 ;; esac

if [ "${CI:-}" != "true" ] || [ -z "${RUNNER_TEMP:-}" ]; then
  echo "llavero-ci: refusing to run outside CI (needs CI=true and RUNNER_TEMP). This script changes the keychain search list; it is for GitHub runners only." >&2
  exit 2
fi
[ -d "$RUNNER_TEMP" ] || fail "RUNNER_TEMP is not a folder"

KC="$RUNNER_TEMP/altrune-firma.keychain-db"
SAVED="$RUNNER_TEMP/altrune-llavero-lista.txt"

case "$ACTION" in
  crear)
    if [ -z "${MACOS_CERTIFICADO_P12_BASE64:-}" ]; then
      fail "MACOS_CERTIFICADO_P12_BASE64 is empty"
    fi
    if [ -z "${MACOS_CERTIFICADO_CLAVE:-}" ]; then
      fail "MACOS_CERTIFICADO_CLAVE is empty"
    fi
    [ ! -e "$KC" ] || fail "the temporary keychain already exists (run borrar first)"

    PW=$(openssl rand -hex 24)
    P12="$RUNNER_TEMP/altrune-certificado.p12"
    trap 'rm -f "$P12"' EXIT

    # Remember the search list so borrar can put it back.
    security list-keychains -d user > "$SAVED"

    security create-keychain -p "$PW" "$KC"
    security set-keychain-settings -lut 21600 "$KC"
    security unlock-keychain -p "$PW" "$KC"

    base64 -D > "$P12" <<EOF_P12
$MACOS_CERTIFICADO_P12_BASE64
EOF_P12
    security import "$P12" -k "$KC" -P "$MACOS_CERTIFICADO_CLAVE" -T /usr/bin/codesign -T /usr/bin/security >/dev/null
    rm -f "$P12"
    # Let codesign use the key without a dialog nobody can answer on a runner.
    security set-key-partition-list -S apple-tool:,apple: -s -k "$PW" "$KC" >/dev/null

    # The new keychain goes first, the old ones stay behind it.
    set -- "$KC"
    while IFS= read -r line; do
      old=$(printf '%s' "$line" | sed -e 's/^[[:space:]]*"//' -e 's/"[[:space:]]*$//')
      if [ -n "$old" ]; then set -- "$@" "$old"; fi
    done < "$SAVED"
    security list-keychains -d user -s "$@"

    say "temporary keychain ready"
    echo "$KC"
    ;;
  borrar)
    if [ -f "$SAVED" ]; then
      set --
      while IFS= read -r line; do
        old=$(printf '%s' "$line" | sed -e 's/^[[:space:]]*"//' -e 's/"[[:space:]]*$//')
        if [ -n "$old" ]; then set -- "$@" "$old"; fi
      done < "$SAVED"
      if [ "$#" -gt 0 ]; then security list-keychains -d user -s "$@"; fi
      rm -f "$SAVED"
    else
      say "no saved search list; the keychain is only deleted"
    fi
    if [ -e "$KC" ]; then
      security delete-keychain "$KC"
      say "temporary keychain deleted"
    else
      say "no temporary keychain to delete"
    fi
    rm -f "$RUNNER_TEMP/altrune-certificado.p12"
    ;;
esac
