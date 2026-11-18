//! Video for games: GStreamer unpacked from the official package beside the Metal engine.

use super::*;
use crate::backend::{io_error_logged, Log};
use crate::domain::engines::LAUNCHERS;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Video for games: the official GStreamer for macOS (gstreamer.freedesktop.org, LGPL), the
/// libraries Wine's `winegstreamer` plays videos with (Media Foundation and DirectShow: MP4 and
/// H.264, WMV, WebM, Ogg). The Metal engine links them by name and ships none; without them
/// Resident Evil Requiem stops at its first video with "Failed to create SourceReader with
/// registered byte stream handler. error(c00d36bb)" and Wine says "GStreamer doesn't support
/// H.264 decoding" (2026-11-15, Apple M5). Only the parts below leave the package, never
/// installed system-wide: it is unpacked with `pkgutil --expand-full`.
const VIDEO_LAYER: Pinned = Pinned {
    name: "gstreamer-1.0-1.28.7-universal.pkg",
    url: "https://gstreamer.freedesktop.org/data/pkg/osx/1.28.7/gstreamer-1.0-1.28.7-universal.pkg",
    sha256: "529fdf4a4027d942e59b5b3564f6400adaa008f63ce5f3fed4ffe35d73911994",
    size: 153_594_157,
};
const VIDEO_VERSION: &str = "1.28.7";

/// The package's components that hold what Wine needs (GLib, GStreamer and its plugin sets).
/// Left out: the GPL ones, encoders, network and developer tools.
const VIDEO_COMPONENTS: [&str; 8] = [
    "base-system-1.0",
    "gstreamer-1.0-core",
    "gstreamer-1.0-playback",
    "gstreamer-1.0-effects",
    "gstreamer-1.0-codecs",
    "gstreamer-1.0-codecs-restricted",
    "gstreamer-1.0-libav",
    "gstreamer-1.0-system",
];

/// Plugins kept: containers, parsers, decoders and conversion, what games' videos go through.
/// `applemedia` decodes H.264 with VideoToolbox (hardware) and `libav` (FFmpeg) is the fallback.
const VIDEO_PLUGINS: [&str; 32] = [
    "coreelements",
    "typefindfunctions",
    "app",
    "playback",
    "videoconvertscale",
    "videofilter",
    "deinterlace",
    "audioconvert",
    "audioresample",
    "volume",
    "rawparse",
    "pbtypes",
    "isomp4",
    "matroska",
    "avi",
    "ogg",
    "flv",
    "asf",
    "mpegtsdemux",
    "mpegpsdemux",
    "id3demux",
    "apetag",
    "audioparsers",
    "videoparsersbad",
    "vorbis",
    "theora",
    "opus",
    "flac",
    "mpg123",
    "dav1d",
    "libav",
    "applemedia",
];

/// What `winegstreamer.so` itself links (by `@rpath` name).
const VIDEO_WINE_LIBRARIES: [&str; 7] = [
    "libgstreamer-1.0.0.dylib",
    "libgstvideo-1.0.0.dylib",
    "libgstaudio-1.0.0.dylib",
    "libgsttag-1.0.0.dylib",
    "libgstbase-1.0.0.dylib",
    "libgobject-2.0.0.dylib",
    "libglib-2.0.0.dylib",
];

/// The `@rpath` libraries a Mach-O file links, by file name.
pub(super) fn rpath_links(file: &Path) -> Vec<String> {
    let out = Command::new("/usr/bin/otool")
        .arg("-L")
        .arg(file)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    out.lines()
        .skip(1)
        .filter_map(|l| l.split_whitespace().next()?.strip_prefix("@rpath/"))
        .map(|n| n.rsplit('/').next().unwrap_or(n).to_string())
        .collect()
}

/// Unpacks GStreamer from the official package beside the Metal engine (`<engine
/// folder>/gstreamer`): the components above merged, cut down to the plugins listed and exactly
/// the libraries they and Wine link, and to the Intel half of each file (the engine is Intel
/// code under Rosetta).
pub(super) fn add_video(wine: &Path, package: &Path, log: &Log) -> Result<(), String> {
    let folder = wine
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| format!("No encuentro «{LAUNCHERS}»."))?;
    let staging = folder.join("gstreamer.nuevo");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)
        .map_err(|e| io_error_logged(&e, log, "Preparar la carpeta del video", &staging))?;
    let fail = |e: String| {
        let _ = std::fs::remove_dir_all(&staging);
        e
    };
    let expanded = staging.join("paquete");
    let ok = run_status(
        Command::new("/usr/sbin/pkgutil")
            .arg("--expand-full")
            .arg(package)
            .arg(&expanded)
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
    .is_ok_and(|s| s.success());
    if !ok {
        return Err(fail("No pude abrir el paquete de GStreamer.".into()));
    }
    let lib = staging.join("lib");
    for c in VIDEO_COMPONENTS {
        let from = expanded.join(format!("{c}-{VIDEO_VERSION}-universal.pkg/Payload/lib"));
        let merged = from.is_dir()
            && run_status(Command::new("/usr/bin/ditto").arg(&from).arg(&lib))
                .is_ok_and(|s| s.success());
        if !merged {
            return Err(fail(format!("Al paquete de GStreamer le falta «{c}».")));
        }
    }
    let _ = std::fs::remove_dir_all(&expanded);

    // Keep the plugins listed and every library they and Wine reach.
    let plugins = lib.join("gstreamer-1.0");
    let kept_plugins: Vec<PathBuf> = VIDEO_PLUGINS
        .iter()
        .map(|p| plugins.join(format!("libgst{p}.dylib")))
        .filter(|p| p.is_file())
        .collect();
    let mut needed: std::collections::BTreeSet<String> =
        VIDEO_WINE_LIBRARIES.iter().map(|s| s.to_string()).collect();
    let mut todo: Vec<PathBuf> = kept_plugins.clone();
    todo.extend(VIDEO_WINE_LIBRARIES.iter().map(|n| lib.join(n)));
    while let Some(f) = todo.pop() {
        for name in rpath_links(&f) {
            if lib.join(&name).exists() && needed.insert(name.clone()) {
                todo.push(lib.join(&name));
            }
        }
    }
    let real = |n: &str| std::fs::canonicalize(lib.join(n)).ok();
    let keep: std::collections::BTreeSet<PathBuf> = needed.iter().filter_map(|n| real(n)).collect();
    for entry in std::fs::read_dir(&lib)
        .map_err(|e| {
            fail(io_error_logged(
                &e,
                log,
                "Leer las bibliotecas del video",
                &lib,
            ))
        })?
        .flatten()
    {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "gstreamer-1.0" {
            continue;
        }
        let wanted = needed.contains(&name)
            || std::fs::canonicalize(&p).is_ok_and(|r| keep.contains(&r) && !p.is_symlink());
        if !wanted {
            let _ = if p.is_dir() && !p.is_symlink() {
                std::fs::remove_dir_all(&p)
            } else {
                std::fs::remove_file(&p)
            };
        }
    }
    for entry in std::fs::read_dir(&plugins)
        .map_err(|e| {
            fail(io_error_logged(
                &e,
                log,
                "Leer los complementos del video",
                &plugins,
            ))
        })?
        .flatten()
    {
        if !kept_plugins.contains(&entry.path()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let missing: Vec<&str> = VIDEO_WINE_LIBRARIES
        .iter()
        .copied()
        .filter(|n| !lib.join(n).is_file())
        .collect();
    let decoders = ["libav", "applemedia"]
        .iter()
        .any(|p| plugins.join(format!("libgst{p}.dylib")).is_file());
    if !missing.is_empty() || !decoders || !plugins.join("libgstdeinterlace.dylib").is_file() {
        return Err(fail(format!(
            "El paquete de GStreamer no trae lo necesario para video ({}).",
            missing.join(", ")
        )));
    }

    // Only the Intel half is used: the engine runs as Intel code under Rosetta.
    let mut files = vec![];
    for dir in [&lib, &plugins] {
        for entry in std::fs::read_dir(dir)
            .map_err(|e| fail(io_error_logged(&e, log, "Leer los archivos del video", dir)))?
            .flatten()
        {
            let p = entry.path();
            if p.is_file() && !p.is_symlink() {
                files.push(p);
            }
        }
    }
    for f in files {
        let archs = Command::new("/usr/bin/lipo")
            .arg("-archs")
            .arg(&f)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        if archs.split_whitespace().count() > 1 && archs.contains("x86_64") {
            let thinned = run_status(
                Command::new("/usr/bin/lipo")
                    .args(["-thin", "x86_64"])
                    .arg(&f)
                    .arg("-output")
                    .arg(&f),
            )
            .is_ok_and(|s| s.success());
            if !thinned {
                return Err(fail(format!("No pude preparar {}.", f.display())));
            }
        }
    }
    let _ = run_status(
        Command::new("/usr/bin/xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(&staging),
    );
    let dest = folder.join("gstreamer");
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&staging, &dest)
        .map_err(|e| io_error_logged(&e, log, "Poner el video en su sitio", &dest))
}

/// Downloads GStreamer (video for games) and puts it beside the Metal engine.
pub fn install_video(p: Shared, log: Log) -> WineState {
    install_addition(
        p,
        log,
        &VIDEO_LAYER,
        "video",
        "Video (GStreamer)",
        add_video,
        |w| video_layer(w).is_some(),
    )
}
