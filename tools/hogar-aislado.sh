#!/bin/sh
# Builds a throwaway HOME for Altrune so screenshots and checks never read or change the
# person's real bottles, settings or Wine. The "Wine" here is a fake shell script: it answers
# `--version`, records every other call in $HOME/wine-falso.log and never runs Windows code.
#
# Usage: hogar-aislado.sh <empty dir> [vacio|wine|botellas]
#   vacio     no Wine and no bottles: the very first start
#   wine      the fake Wine, no bottles
#   botellas  the fake Wine plus three bottles: «Juegos de prueba» (two programs),
#             «Botella vacía» (no programs) and «A medio crear» (no system.reg: incomplete)
# Run the app with:  HOME=<dir> target/release/altrune --screenshot out.png --vista inicio
# (from app/; screenshot, --reposo and --medir modes take no single-instance lock).
set -eu
H="$1"
MODE="${2:-botellas}"
if [ ! -d "$H" ] || [ -n "$(/bin/ls -A "$H")" ]; then
  echo "hogar-aislado: $H must be an existing, empty folder" >&2
  exit 2
fi
case "$H" in /private/tmp/*|/tmp/*|/var/folders/*) ;; *) echo "hogar-aislado: use a temporary folder" >&2; exit 2;; esac
# The same data folder path the app uses (src/backend/paths.rs).
DATA="$H/Library/Application Support/Botella Libre"
BOTTLES="$DATA/Bottles"
mkdir -p "$BOTTLES" "$H/Applications"

WINE_JSON=null
if [ "$MODE" != vacio ]; then
  mkdir -p "$H/wine-falso/bin"
  cat > "$H/wine-falso/bin/wine" <<'W'
#!/bin/sh
# Fake Wine for isolated checks: never runs Windows code.
if [ "${1:-}" = "--version" ]; then echo "wine-11.0 (prueba)"; exit 0; fi
echo "wine $*" >> "$HOME/wine-falso.log"
exit 0
W
  cat > "$H/wine-falso/bin/wineserver" <<'W'
#!/bin/sh
# Fake wineserver: no bottle is ever running.
echo "wineserver $*" >> "$HOME/wine-falso.log"
[ "${1:-}" = "-k0" ] && exit 1
exit 0
W
  chmod +x "$H/wine-falso/bin/wine" "$H/wine-falso/bin/wineserver"
  WINE_JSON="\"$H/wine-falso/bin/wine\""
fi

BOTTLES_JSON="{}"
DEFAULT_JSON=null
if [ "$MODE" = botellas ]; then
  G="$BOTTLES/Juegos de prueba"
  mkdir -p "$G/drive_c/Program Files/Bloc de prueba" "$G/drive_c/Program Files/Juego de prueba"
  printf 'WINE REGISTRY Version 2\n' > "$G/system.reg"
  printf 'WINE REGISTRY Version 2\n' > "$G/user.reg"
  printf 'MZ' > "$G/drive_c/Program Files/Bloc de prueba/bloc.exe"
  printf 'MZ' > "$G/drive_c/Program Files/Juego de prueba/juego.exe"
  E="$BOTTLES/Botella vacía"
  mkdir -p "$E/drive_c"
  printf 'WINE REGISTRY Version 2\n' > "$E/system.reg"
  mkdir -p "$BOTTLES/A medio crear/drive_c"
  BOTTLES_JSON="{\"$G\": {\"msync\": null, \"windows\": \"win10\", \"programs\": [
    {\"name\": \"Bloc de prueba\", \"path\": \"$G/drive_c/Program Files/Bloc de prueba/bloc.exe\"},
    {\"name\": \"Juego de prueba\", \"path\": \"$G/drive_c/Program Files/Juego de prueba/juego.exe\"}]},
    \"$E\": {\"msync\": null, \"windows\": \"win10\"}}"
  DEFAULT_JSON="\"$G\""
fi

cat > "$DATA/config.json" <<J
{
  "wine_path": $WINE_JSON,
  "bottles": $BOTTLES_JSON,
  "default_bottle": $DEFAULT_JSON,
  "mac_launchers": false,
  "games_offered": true
}
J
echo "hogar aislado listo: $H ($MODE)"
