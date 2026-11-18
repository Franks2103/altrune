#!/bin/sh
# Rebuilds the README media in docs/media/ from scratch: the screenshots and the two animated GIFs.
#
# Usage (from anywhere):  sh tools/demo/generar.sh
#
# What it does:
#   1. builds the release binary (cargo build --release --locked);
#   2. makes a throwaway HOME with tools/hogar-aislado.sh (fake Wine, three invented bottles), so the
#      app never reads or changes real data, and takes the screenshots at 1280x800 with
#      --screenshot, --vista and --tema;
#   3. cross-fades them with ffmpeg into recorrido.gif (tour of the app) and temas.gif (light and dark theme).
# The two SVG diagrams (como-funciona*.svg) are hand-made sources and are not touched.
# Needs ffmpeg (8.x) on the PATH, plus bc and taskpolicy (both ship with macOS). The "ajustes" view is not used: it shows the fake Wine's temporary path.
# Never run it with a real HOME: the script sets HOME itself for every call to the app.
set -eu
cd "$(dirname "$0")/../.."
OUT=docs/media
BIN=target/release/altrune
SIZE=1280x800

for tool in ffmpeg bc taskpolicy; do
  command -v "$tool" >/dev/null || { echo "generar: $tool is required" >&2; exit 2; }
done
cargo build --release --locked

H=$(mktemp -d)
W=$(mktemp -d)
trap 'rm -rf "$H" "$W"' EXIT INT TERM
sh tools/hogar-aislado.sh "$H" botellas

mkdir -p "$OUT"
# The app saves at 2x (2560x1600); ffmpeg brings each shot down to 1280x800 for the repository.
# The app takes its screenshot after 120 frames, which at full speed can land in the middle of a
# fade (a dialog half transparent). Running it at background priority (taskpolicy -b) makes it slower
# than the animations, so every shot is settled and always the same.
# The "acerca" view shows two lines that are left out of the picture: the data folder path (a detail
# not worth showing) and, below the card, the log path inside the temporary home.
# The first is wiped with a white box and the licences line under it moves up to close the gap; the
# second is covered by a box in the page colour (#F5F6F8).
shot() { # <view> <theme>
  HOME="$H" taskpolicy -b "$BIN" --screenshot "$W/$1-$2.png" --vista "$1" --tema "$2" --tamano "$SIZE"
  if [ "$1" = acerca ]; then
    ffmpeg -v error -y -i "$W/$1-$2.png" -filter_complex "[0:v]scale=1280:800:flags=lanczos,split[a][b];[b]crop=960:24:276:646[l];[a]drawbox=x=276:y=622:w=960:h=48:color=white:t=fill[c];[c][l]overlay=276:622,drawbox=x=240:y=708:w=1040:h=48:color=0xF5F6F8:t=fill" "$OUT/$1-$2.png"
  else
    ffmpeg -v error -y -i "$W/$1-$2.png" -vf "scale=1280:800:flags=lanczos" "$OUT/$1-$2.png"
  fi
}
shot inicio claro
shot inicio oscuro
shot botella claro
shot nueva claro
shot diagnostico claro
shot acerca claro

# fade <width> <fps> <hold> <fade> <out.gif> <image>...
# Each image stays <hold> seconds, then cross-fades into the next one for <fade> seconds. The first image
# is repeated at the end and the first <fade> seconds are cut off, so the loop is seamless.
fade() {
  width=$1; fps=$2; hold=$3; fd=$4; gif=$5; shift 5
  n=$#
  period=$(echo "$hold + $fd" | bc)    # time from one fade start to the next
  clip=$(echo "$period + $fd" | bc)    # length each input needs so the next fade can start on time
  inputs=""; filt=""; i=0
  for img in "$@" "$1"; do
    inputs="$inputs -loop 1 -framerate $fps -t $clip -i $img"
    filt="$filt[$i:v]scale=$width:-2:flags=lanczos,format=yuv420p,setsar=1[v$i];"
    i=$((i + 1))
  done
  prev=v0; k=1
  while [ "$k" -le "$n" ]; do
    off=$(echo "$k * $period" | bc)
    filt="$filt[$prev][v$k]xfade=transition=fade:duration=$fd:offset=$off[x$k];"
    prev=x$k; k=$((k + 1))
  done
  total=$(echo "$n * $period" | bc)
  # shellcheck disable=SC2086
  ffmpeg -v error -y $inputs -filter_complex "${filt}[$prev]trim=start=$fd:duration=$total,setpts=PTS-STARTPTS,fps=$fps,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle" -loop 0 "$gif"
}

M=$OUT
fade 960 12 2.5 0.5 "$M/recorrido.gif" \
  "$M/inicio-claro.png" "$M/botella-claro.png" "$M/nueva-claro.png" \
  "$M/diagnostico-claro.png" "$M/acerca-claro.png" "$M/inicio-oscuro.png"
fade 800 10 1.5 0.5 "$M/temas.gif" "$M/inicio-claro.png" "$M/inicio-oscuro.png"
echo "generar: done, files in $OUT"
