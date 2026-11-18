#!/bin/sh
# Builds the controller libraries of Altrune's Metal engine and installs them in the Wine/Mandos
# folder inside Altrune's data folder (its path is in src/backend/paths.rs):
#   - SDL3 with libusb, which reads wired "for Xbox" pads (GIP) that macOS has no driver for;
#   - sdl2-compat, the SDL2 face of it that Wine's winebus loads.
# Both for x86_64, which is what Wine runs as. Needs the Xcode command line tools, cmake and
# ninja (brew install cmake ninja). Nothing outside the app's folder is touched.
#
# These exact versions were built and verified on 2026-11-15 (Apple M5, macOS 27.0.1) with an
# 8BitDo Ultimate Wired Controller for Xbox (USB 2dc8:2006).
set -eu
SDL_TAG=release-3.4.20
COMPAT_TAG=release-2.32.74
USB=libusb-1.0.30
USB_SHA256=fea36f34f9156400209595e300840767ab1a385ede1dc7ee893015aea9c6dbaf
DEST="$HOME/Library/Application Support/Botella Libre/Wine/Mandos"
HERE=$(cd "$(dirname "$0")" && pwd)
B=$(mktemp -d "${TMPDIR:-/tmp}/botella-mando.XXXXXX")
P="$B/prefix"
paso() {
    nombre=$1; shift
    printf '%s... ' "$nombre"
    if "$@" > "$B/$nombre.log" 2>&1; then echo ok; else echo FALLO; tail -30 "$B/$nombre.log"; exit 1; fi
}
mkdir -p "$B/src" "$B/usb" "$P"
paso sdl-fuentes git clone --depth 1 --branch "$SDL_TAG" https://github.com/libsdl-org/SDL.git "$B/src/sdl"
paso compat-fuentes git clone --depth 1 --branch "$COMPAT_TAG" https://github.com/libsdl-org/sdl2-compat.git "$B/src/compat"
paso usb-fuentes curl -fsSL -o "$B/src/$USB.tar.bz2" "https://github.com/libusb/libusb/releases/download/v${USB#libusb-}/$USB.tar.bz2"
[ "$(shasum -a 256 "$B/src/$USB.tar.bz2" | cut -d' ' -f1)" = "$USB_SHA256" ] || { echo "libusb: suma SHA-256 distinta"; exit 1; }
tar -xjf "$B/src/$USB.tar.bz2" -C "$B/src"
export PKG_CONFIG_LIBDIR="$P/lib/pkgconfig"
cd "$B/usb"
paso usb-configure "$B/src/$USB/configure" --host=x86_64-apple-darwin --prefix="$P" --disable-shared --enable-static "CC=clang -arch x86_64" "CFLAGS=-O2 -mmacosx-version-min=11.0"
paso usb-make make -j8
paso usb-install make install
COMUN="-G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_OSX_ARCHITECTURES=x86_64 -DCMAKE_OSX_DEPLOYMENT_TARGET=11.0"
# shellcheck disable=SC2086
paso sdl-configure cmake -S "$B/src/sdl" -B "$B/sdl" $COMUN "-DCMAKE_PREFIX_PATH=$P" "-DCMAKE_INSTALL_PREFIX=$P" -DSDL_SHARED=ON -DSDL_STATIC=OFF -DSDL_TESTS=OFF -DSDL_EXAMPLES=OFF -DSDL_HIDAPI_LIBUSB=ON -DSDL_HIDAPI_LIBUSB_SHARED=OFF "-DLibUSB_LINK_LIBRARIES=-Wl,-framework,Security"
paso sdl-build cmake --build "$B/sdl"
paso sdl-install cmake --install "$B/sdl"
# shellcheck disable=SC2086
paso compat-configure cmake -S "$B/src/compat" -B "$B/compat" $COMUN "-DCMAKE_PREFIX_PATH=$P" "-DCMAKE_INSTALL_PREFIX=$P" -DSDL2COMPAT_TESTS=OFF -DSDL2COMPAT_X11=OFF
paso compat-build cmake --build "$B/compat"
paso compat-install cmake --install "$B/compat"
paso padcheck clang -arch x86_64 -O2 "$HERE/padcheck.c" "-I$P/include" "-L$P/lib" -lSDL3 "-Wl,-rpath,$P/lib" -o "$B/padcheck"
# sdl2-compat looks for "libSDL3.dylib" beside itself.
mkdir -p "$DEST"
cp "$P/lib/libSDL2-2.0.0.dylib" "$DEST/libSDL2-2.0.0.dylib"
cp "$P/lib/libSDL3.0.dylib" "$DEST/libSDL3.dylib"
echo "Instalado en $DEST"
echo "Mando: mueve las palancas y pulsa botones durante 10 segundos"
"$B/padcheck" 10
