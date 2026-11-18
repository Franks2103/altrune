#!/bin/sh
# Writes the release notes of one Altrune version on standard output (Markdown, in Spanish).
#
# Usage: sh tools/notas-version.sh VERSION DMG firmado|sin-firmar
#
#   VERSION     the full version, for example 0.1.0-alpha (the text comes from
#               docs/releases/VERSION.md)
#   DMG         the finished disk image; its SHA-256 goes at the end of the notes
#   firmado     the image was signed with Developer ID and notarised
#   sin-firmar  it was not: the notes then start with a clear warning that this is a test build
#
# It only reads files and prints. It signs nothing, uploads nothing and never opens the image.
# For "firmado" it checks that the image really carries a notarisation ticket (xcrun stapler
# validate), so the notes cannot claim something that is not true. ALTRUNE_NOTAS_NO_VERIFICAR=1
# skips that check; the tests use it with a fake image and nothing else should.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
fail() { echo "notas-version: $*" >&2; exit 1; }

[ "$#" -eq 3 ] || fail "usage: sh tools/notas-version.sh VERSION DMG firmado|sin-firmar"
VERSION=$1
DMG=$2
MODO=$3

# The version becomes a file name and a tag, so only accept the plain shape.
case "$VERSION" in
  "" | *[!0-9A-Za-z.-]*) fail "the version '$VERSION' is not like 0.1.0 or 0.1.0-alpha" ;;
esac
printf '%s\n' "$VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z]+(\.[0-9A-Za-z]+)*)?$' \
  || fail "the version '$VERSION' is not like 0.1.0 or 0.1.0-alpha"
case "$MODO" in
  firmado | sin-firmar) ;;
  *) fail "the third argument must be firmado or sin-firmar, not '$MODO'" ;;
esac

DOC="$ROOT/docs/releases/$VERSION.md"
[ -f "$DOC" ] || fail "no notes for this version: docs/releases/$VERSION.md"
[ -f "$DMG" ] && [ ! -L "$DMG" ] || fail "no disk image at $DMG"

NOMBRE=$(basename "$DMG")
[ "$NOMBRE" = "Altrune-$VERSION.dmg" ] || fail "the image is called $NOMBRE, expected Altrune-$VERSION.dmg"

if command -v shasum >/dev/null 2>&1; then
  SUMA=$(shasum -a 256 "$DMG" | awk '{ print $1 }')
elif command -v sha256sum >/dev/null 2>&1; then
  SUMA=$(sha256sum "$DMG" | awk '{ print $1 }')
else
  fail "neither shasum nor sha256sum is available"
fi
printf '%s\n' "$SUMA" | grep -Eq '^[0-9a-f]{64}$' || fail "could not compute the SHA-256 of $DMG"

if [ "$MODO" = firmado ] && [ "${ALTRUNE_NOTAS_NO_VERIFICAR:-0}" != 1 ]; then
  xcrun stapler validate "$DMG" >/dev/null 2>&1 \
    || fail "$NOMBRE has no notarisation ticket: it cannot be described as firmado"
fi

if [ "$MODO" = sin-firmar ]; then
  cat <<'AVISO'
> ## Versión de prueba sin firmar: solo para probar
>
> Esta descarga **no está firmada con un certificado Developer ID de Apple y no está notarizada**.
> No la ha revisado Apple. macOS la bloqueará la primera vez que intentes abrirla y dirá que no
> puede comprobar que esté libre de software malicioso.
>
> Úsala solo si quieres probar Altrune y aceptas ese riesgo. Si decides probarla, así es como Apple
> documenta abrir una app no notarizada:
>
> 1. Arrastra Altrune a Aplicaciones e intenta abrirla una vez. macOS la bloqueará.
> 2. Abre Ajustes del Sistema, entra en Privacidad y seguridad y baja hasta la sección Seguridad.
> 3. Pulsa el botón «Abrir igualmente» («Open Anyway» en inglés; el nombre exacto puede variar según
>    la versión de macOS) junto al aviso de Altrune y confirma con tu contraseña o Touch ID. El
>    botón aparece durante un tiempo limitado tras el primer intento.
>
> Si no estás seguro, no la abras: espera a una versión firmada y notarizada.

AVISO
else
  cat <<'FIRMA'
> Firmada con un certificado Developer ID y notarizada por Apple.

FIRMA
fi

cat "$DOC"

cat <<FIN

## Suma SHA-256 de esta descarga

\`\`\`
$SUMA  $NOMBRE
\`\`\`

Compárala con la del archivo \`$NOMBRE.sha256\` siguiendo los pasos de «Cómo comprobar la suma
SHA-256», más arriba.
FIN
