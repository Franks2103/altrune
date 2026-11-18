//! The loader script that gives each Windows program its Direct3D layer, for a Mac that already
//! has Apple's D3DMetal beside the Metal engine. Altrune never downloads or installs D3DMetal:
//! it only keeps the script of an engine that has the layer up to date.

use super::*;
use std::path::Path;

/// Why Altrune does not install Apple's software (D3DMetal, the Game Porting Toolkit and the
/// engine made from it). Every screen that would offer it says this instead.
pub const APPLE_SOFTWARE_NOT_INSTALLED: &str = "No se instala desde Altrune: es software de Apple y su licencia no permite que Altrune lo descargue.";

/// Wine starts every Windows program by running the file `wine` that sits beside its own
/// `ntdll.so`; neither `bin/wine` nor `$WINELOADER` is used for that. The Direct3D layer is read
/// from the environment when a program starts, and the environment is inherited, so this is the
/// only place where one program can get a different layer from the one that started it.
///
/// `/bin/sh` is a system program: macOS removes `DYLD_*` variables from it, so the Mac
/// libraries are named a second time in a variable of ours and put back.
///
/// A program gets D3DMetal when its name is in [`D3D12_PROGRAMS`](crate::backend::D3D12_PROGRAMS),
/// or when it ships Microsoft's Direct3D 12 runtime beside it (the Agility SDK, `D3D12\D3D12Core.dll`), which only Direct3D 12
/// programs do. Resident Evil Requiem is one: on DXMT it gets Wine's own Direct3D 12 over Vulkan
/// and stops with "Unsupported graphics card (0x80004002)" (2026-11-15, Apple M5). The second
/// rule needs the program's full Windows path, which is how Steam and launchers start games.
pub(super) const LOADER_SCRIPT: &str = r#"#!/bin/sh
# Botella Libre: gives each Windows program its Direct3D layer, then starts Wine's own loader.
real="${0%/*}/wine-motor"
[ -n "$BOTELLA_BIBLIOTECAS" ] && export DYLD_FALLBACK_LIBRARY_PATH="$BOTELLA_BIBLIOTECAS"
usa_d3d12() {
    programa=$(printf '%s' "${1##*[\\/]}" | /usr/bin/tr 'A-Z' 'a-z')
    case ":$BOTELLA_D3DMETAL_PROGRAMAS:" in
        *":${programa%.exe}:"*) return 0 ;;
    esac
    case "$1" in
        [A-Za-z]:\\*)
            unidad=$(printf '%s' "${1%%:*}" | /usr/bin/tr 'A-Z' 'a-z')
            ruta=$(printf '%s' "${1#?:}" | /usr/bin/tr '\\' '/')
            [ -f "${WINEPREFIX:-$HOME/.wine}/dosdevices/$unidad:${ruta%/*}/D3D12/D3D12Core.dll" ] && return 0
            ;;
    esac
    return 1
}
if [ -n "$BOTELLA_CAPA" ]; then
    capa="$BOTELLA_CAPA"
    unset CX_APPLEGPTK_LIBD3DSHARED_PATH
    if [ -n "$BOTELLA_D3DMETAL" ] && usa_d3d12 "$1"; then
        capa="$BOTELLA_D3DMETAL/wine"
        export CX_APPLEGPTK_LIBD3DSHARED_PATH="$BOTELLA_D3DMETAL/external/libd3dshared.dylib"
    fi
    export WINEDLLPATH_PREPEND="$capa"
fi
exec "$real" "$@"
"#;

/// Writes [`LOADER_SCRIPT`] in place of the engine's loader in one step (a new file renamed over
/// the old one), so a program starting at that moment reads either script whole.
fn write_loader(loader: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let new = loader.with_file_name("wine.nuevo");
    std::fs::write(&new, LOADER_SCRIPT)?;
    std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(&new, loader)
}

/// Brings the loader script of an engine that has Direct3D 12 installed up to this version of the
/// app (an older one may know fewer Direct3D 12 programs). True when it was rewritten.
pub fn refresh_loader(wine: &Path) -> bool {
    let Some(loader) = engine_loader(wine) else {
        return false;
    };
    if !loader.with_file_name(REAL_LOADER).is_file()
        || std::fs::read_to_string(&loader).is_ok_and(|s| s == LOADER_SCRIPT)
    {
        return false;
    }
    write_loader(&loader).is_ok()
}
