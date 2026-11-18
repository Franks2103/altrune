//! Where the free Wines and their layers live on disk, and what each engine needs in its
//! environment. This is read-only knowledge about the disk (no download, unpacking, signing or
//! installing happens here), so the backend, which builds every Wine command, can ask it
//! directly: the engine of a Wine is recognised by its files, the Metal engine's DXMT, video and
//! Direct3D 12 layers sit beside it, and `runtime_env` says which variables that engine needs.
//! `wine_setup` re-exports all of it and adds what installs and changes these folders.

use super::*;
use std::path::{Path, PathBuf};

/// Where Altrune keeps the Wine it installs itself.
pub fn managed_dir() -> PathBuf {
    data_dir().join("Wine")
}

pub fn wine_in_app(app: &Path) -> Vec<PathBuf> {
    ["wine", "wine64"]
        .iter()
        .map(|b| app.join("Contents/Resources/wine/bin").join(b))
        .collect()
}

/// Which free Wine to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    /// Apple's Game Porting Toolkit build of Wine, with MSync and Apple's D3DMetal for DirectX
    /// 11/12 games. Apple chips, macOS 14 or later. It is Apple's software: Altrune never
    /// downloads it, and uses it only when it is already installed.
    Games,
    /// The WineHQ build: works on every Mac, fewer game fixes.
    Standard,
    /// Wine 11.0 built from open sources by the highball-engine project, with DXMT in front of
    /// Wine's own Direct3D: DirectX 10/11 drawn with Metal. WineHQ's build tops out at Direct3D 9
    /// on a Mac, which is why programs that draw their window with Direct2D (the Rockstar Games
    /// Launcher) cannot start on it.
    Metal,
}

impl Engine {
    pub fn app_name(self) -> &'static str {
        match self {
            Engine::Games => "Game Porting Toolkit.app",
            Engine::Standard => "Wine Stable.app",
            Engine::Metal => "Motor DXMT",
        }
    }
    /// The engine a Wine program belongs to, by where it lives.
    pub fn of(path: &Path) -> Engine {
        if metal_layer(path).is_some() {
            Engine::Metal
        } else if path.to_string_lossy().contains("Game Porting Toolkit") {
            Engine::Games
        } else {
            Engine::Standard
        }
    }
    /// The installed copy of this engine in the app's folder, if any.
    pub fn installed(self) -> Option<PathBuf> {
        if self == Engine::Metal {
            let wine = managed_dir().join(self.app_name()).join("engine/bin/wine");
            return (wine.is_file() && metal_layer(&wine).is_some()).then_some(wine);
        }
        wine_in_app(&managed_dir().join(self.app_name()))
            .into_iter()
            .find(|p| p.is_file())
    }
}

/// "wine-11.0", "wine-7.7 (Game Porting Toolkit 1.1)" → (11, 0), (7, 7).
pub fn wine_version_number(s: &str) -> Option<(u32, u32)> {
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let mut parts = s[start..].split(|c: char| !c.is_ascii_digit());
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().and_then(|m| m.parse().ok()).unwrap_or(0);
    Some((major, minor))
}

/// Running a prefix with an older Wine than it was last updated with can break it.
pub fn is_downgrade(last_used: &str, candidate: &str) -> bool {
    match (
        wine_version_number(last_used),
        wine_version_number(candidate),
    ) {
        (Some(a), Some(b)) => b < a,
        _ => false,
    }
}

/// The DXMT folder that belongs to this Wine, when it is the Metal engine: it is recognised by
/// its files (`<engine folder>/engine/bin/wine` beside `<engine folder>/dxmt`), not by its name.
pub fn metal_layer(wine: &Path) -> Option<PathBuf> {
    let layer = wine.parent()?.parent()?.parent()?.join("dxmt");
    (layer.join("x86_64-windows/d3d11.dll").is_file()
        && layer.join("x86_64-unix/winemetal.so").is_file())
    .then_some(layer)
}

/// The Mac libraries (FreeType, GnuTLS, MoltenVK…) of the standard Wine. The Metal engine ships
/// none of its own and borrows these.
fn host_libraries() -> Option<PathBuf> {
    let wine = Engine::Standard.installed()?;
    let lib = wine.parent()?.parent()?.join("lib");
    lib.is_dir().then_some(lib)
}

/// Controller libraries built on this Mac, when there are any (`mando/compilar-mando.sh`): SDL3
/// with libusb, and sdl2-compat, the SDL2 face of it that Wine loads.
///
/// macOS has a driver for Microsoft's own wired Xbox pads only. Other makers' "for Xbox" pads
/// speak the same protocol (GIP, a vendor USB interface, not HID), get no driver, and no program
/// sees them: an 8BitDo Ultimate Wired Controller for Xbox showed up on USB and nowhere else
/// (2026-11-15, macOS 27). SDL3 3.4.12 and later read such pads over USB themselves; the SDL 2.32
/// the standard Wine carries does not.
pub fn controller_libraries() -> Option<PathBuf> {
    let dir = managed_dir().join("Mandos");
    (dir.join("libSDL2-2.0.0.dylib").is_file() && dir.join("libSDL3.dylib").is_file())
        .then_some(dir)
}

/// The wired-controller libraries (SDL with libusb) are installed.
pub fn controllers_ready() -> bool {
    controller_libraries().is_some()
}

/// The folders the Metal engine finds its Mac libraries in, first match wins: the controller
/// libraries go before the standard Wine's, which has an SDL2 of the same name.
pub fn library_path(extra: &[Option<&Path>], host: &Path) -> String {
    let mut dirs: Vec<String> = extra
        .iter()
        .flatten()
        .copied()
        .chain([host])
        .map(|d| d.to_string_lossy().to_string())
        .collect();
    dirs.push("/usr/lib".to_string());
    dirs.join(":")
}

/// What a Wine needs in its environment just to work, besides the bottle's settings. Only the
/// Metal engine needs anything: DXMT in front of Wine's Direct3D, DXMT's drawing into another
/// process's window (how a launcher's browser shows its page), the borrowed Mac libraries and,
/// when Apple's D3DMetal is installed beside it, what the loader script reads to choose a
/// layer per program.
pub fn runtime_env(wine: &Path) -> Vec<(String, String)> {
    let Some(layer) = metal_layer(wine) else {
        return vec![];
    };
    let layer = layer.to_string_lossy().to_string();
    let mut env = vec![
        ("WINEDLLPATH_PREPEND".to_string(), layer.clone()),
        (
            "DXMT_ALLOW_CROSS_PROCESS_SWAPCHAIN".to_string(),
            "1".to_string(),
        ),
    ];
    let video = video_layer(wine);
    if let Some(lib) = host_libraries() {
        let libraries = library_path(&[controller_libraries().as_deref(), video.as_deref()], &lib);
        env.push(("DYLD_FALLBACK_LIBRARY_PATH".to_string(), libraries.clone()));
        env.push(("BOTELLA_BIBLIOTECAS".to_string(), libraries));
    }
    // GStreamer looks for plugins only here (not in a GStreamer installed on the Mac, which may
    // be another version), keeps its list of them beside them and builds it inside the program.
    if let Some(lib) = &video {
        let plugins = lib.join("gstreamer-1.0").to_string_lossy().to_string();
        env.push(("GST_PLUGIN_SYSTEM_PATH_1_0".to_string(), plugins.clone()));
        env.push(("GST_PLUGIN_PATH_1_0".to_string(), plugins));
        env.push((
            "GST_REGISTRY_1_0".to_string(),
            lib.with_file_name("registro-x86_64.bin")
                .to_string_lossy()
                .to_string(),
        ));
        env.push(("GST_REGISTRY_FORK".to_string(), "no".to_string()));
    }
    if let Some(d3d12) = d3d12_layer(wine) {
        env.push(("BOTELLA_CAPA".to_string(), layer));
        env.push((
            "BOTELLA_D3DMETAL".to_string(),
            d3d12.to_string_lossy().to_string(),
        ));
        env.push((
            "BOTELLA_D3DMETAL_PROGRAMAS".to_string(),
            D3D12_PROGRAMS.to_string(),
        ));
    }
    env
}

/// GStreamer's libraries for this Metal engine, when video was added to it (`lib`, with the
/// plugins in `lib/gstreamer-1.0`).
pub fn video_layer(wine: &Path) -> Option<PathBuf> {
    metal_layer(wine)?;
    let lib = wine.parent()?.parent()?.parent()?.join("gstreamer/lib");
    let plugins = lib.join("gstreamer-1.0");
    (lib.join("libgstreamer-1.0.0.dylib").is_file()
        && plugins.join("libgstcoreelements.dylib").is_file()
        && plugins.join("libgstisomp4.dylib").is_file()
        && plugins.join("libgstdeinterlace.dylib").is_file()
        && ["libav", "applemedia"]
            .iter()
            .any(|name| plugins.join(format!("libgst{name}.dylib")).is_file()))
    .then_some(lib)
}

/// The programs that need Direct3D 12, by file name: lower case, without ".exe", separated by
/// ":". DXMT stops at Direct3D 11, so these get D3DMetal while the rest of the bottle (their
/// launcher, which needs DXMT's Direct3D 10) stays as it is.
///
/// Red Dead Redemption 2: on DXMT it finds no Direct3D 12, falls back to Vulkan and crashes
/// while starting (2026-11-15, Apple M5).
pub const D3D12_PROGRAMS: &str = "rdr2:tlou-ii:tlou-ii-l";

/// The engine's real loader, once the loader script (`LOADER_SCRIPT`, in `wine_setup`) has taken its place.
pub const REAL_LOADER: &str = "wine-motor";

/// The file Wine runs to start each Windows program with this engine.
pub fn engine_loader(wine: &Path) -> Option<PathBuf> {
    Some(wine.parent()?.parent()?.join("lib/wine/x86_64-unix/wine"))
}

/// The D3DMetal folder of this Metal engine, when Apple's layer is installed beside it: the layer
/// next to the engine and the loader script that hands it to the programs that need it.
pub fn d3d12_layer(wine: &Path) -> Option<PathBuf> {
    metal_layer(wine)?;
    let layer = wine.parent()?.parent()?.parent()?.join("d3dmetal");
    (layer.join("wine/x86_64-windows/d3d12.dll").is_file()
        && layer.join("external/libd3dshared.dylib").is_file()
        && engine_loader(wine)?.with_file_name(REAL_LOADER).is_file())
    .then_some(layer)
}
