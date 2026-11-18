//! Which Wine engines exist, the exact packages they come from, and where the release of the
//! usual Wine is found.

use super::*;
use crate::backend::{io_error_logged, Log};
use crate::domain::engines::EngineStatus;
use std::path::Path;
use std::process::Command;

/// Free Wine builds for macOS (open source, published on GitHub).
const RELEASES_API: &str =
    "https://api.github.com/repos/Gcenx/macOS_Wine_builds/releases?per_page=40";
/// Preferred kinds, best first.
const FLAVOURS: [&str; 3] = ["wine-stable", "wine-staging", "wine-devel"];

pub(super) struct Asset {
    pub(super) name: String,
    pub(super) url: String,
    pub(super) size: u64,
    /// The SHA-256 GitHub publishes for the file (lower-case hex), when it gives one.
    pub(super) digest: Option<String>,
}

/// GitHub's `digest` of a release file ("sha256:…") as plain lower-case hex, or None when it is
/// missing or is not a SHA-256.
pub(super) fn parse_digest(digest: Option<&str>) -> Option<String> {
    let hex = digest?.strip_prefix("sha256:")?.to_lowercase();
    (hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some(hex)
}

/// A package of one exact version, checked against its published SHA-256 before it is unpacked.
/// The versions are the ones the Rockstar Games Launcher was verified with (2026-11-15, Apple M5).
pub(super) struct Pinned {
    pub(super) name: &'static str,
    pub(super) url: &'static str,
    pub(super) sha256: &'static str,
    pub(super) size: u64,
}

/// github.com/gauthierpiarrette/highball-engine (LGPL-2.1): Wine 11.0, x86_64.
pub(super) const METAL_WINE: Pinned = Pinned {
    name: "engine-wine-11.0-20261112T101620Z-gst-rpath-0030.tar.gz",
    url: "https://github.com/gauthierpiarrette/highball-engine/releases/download/engine-wine-11.0-20261112T101620Z-gst-rpath-0030/engine-wine-11.0-20261112T101620Z-gst-rpath-0030.tar.gz",
    sha256: "94936e0dc184e9fba4e202f8ec165c5e8bbbfea1bbed43730ceaa14b8a3bbd46",
    size: 335_456_362,
};

impl EngineStatus {
    /// Looks at the disk for each engine and layer. The struct is the domain's; the folders come
    /// from `backend::engine_layout` and `games_engine_supported`, and the read sits here because
    /// the domain cannot name the backend or the layers above it.
    pub fn read() -> Self {
        let metal = Engine::Metal.installed();
        Self {
            metal_capable: games_engine_supported(),
            games: Engine::Games.installed(),
            d3d12: metal.as_deref().is_some_and(|w| d3d12_layer(w).is_some()),
            video: metal.as_deref().is_some_and(|w| video_layer(w).is_some()),
            controllers: controllers_ready(),
            metal,
        }
    }
}

/// The Metal engines need an Apple chip and macOS 14 (Sonoma) or later. Asked once per run: the
/// window used to ask on every frame of a bottle's page, starting two processes each time
/// (about 4.8 ms of a 4.9 ms frame, measured on 2026-11-15).
pub fn games_engine_supported() -> bool {
    static SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SUPPORTED.get_or_init(ask_games_engine_supported)
}

fn ask_games_engine_supported() -> bool {
    let arm = Command::new("/usr/bin/uname")
        .arg("-m")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("arm64"))
        .unwrap_or(false);
    let major = Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .split('.')
                .next()
                .and_then(|m| m.parse::<u32>().ok())
        })
        .unwrap_or(0);
    arm && major >= 14
}

fn github_releases(api: &str, log: &Log) -> Result<Vec<serde_json::Value>, String> {
    // Tests never reach the network.
    if cfg!(test) {
        return Err("Las pruebas no descargan nada.".into());
    }
    let out = Command::new("/usr/bin/curl")
        .args([
            "-fsSL",
            "--max-time",
            "30",
            "-H",
            "Accept: application/vnd.github+json",
            api,
        ])
        .output()
        .map_err(|e| io_error_logged(&e, log, "Pedir las versiones a GitHub", Path::new(api)))?;
    if !out.status.success() {
        return Err("No hay conexión a internet o GitHub no respondió.".into());
    }
    parse_releases(&out.stdout)
}

/// The releases in GitHub's answer. A page that is not JSON (the login page of a public network,
/// say) is not GitHub's answer: the parser's own English text is never shown.
fn parse_releases(answer: &[u8]) -> Result<Vec<serde_json::Value>, String> {
    let releases: serde_json::Value = serde_json::from_slice(answer)
        .map_err(|_| "GitHub no dio una respuesta que pueda leer: prueba más tarde.".to_string())?;
    Ok(releases.as_array().cloned().unwrap_or_default())
}

/// The release of the usual Wine to download. Altrune downloads no other Wine from here: the
/// games engine (Apple's Game Porting Toolkit) is never fetched, only used when it is installed.
pub(super) fn pick_asset(log: &Log) -> Result<Asset, String> {
    let releases = github_releases(RELEASES_API, log)?;
    for flavour in FLAVOURS {
        for r in &releases {
            if r["draft"].as_bool().unwrap_or(false) {
                continue;
            }
            for a in r["assets"].as_array().cloned().unwrap_or_default() {
                let name = a["name"].as_str().unwrap_or("");
                if name.starts_with(flavour) && name.ends_with(".tar.xz") && name.contains("osx64")
                {
                    return Ok(Asset {
                        name: name.to_string(),
                        url: a["browser_download_url"].as_str().unwrap_or("").to_string(),
                        size: a["size"].as_u64().unwrap_or(0),
                        digest: parse_digest(a["digest"].as_str()),
                    });
                }
            }
        }
    }
    Err("No encontré una versión de Wine para Mac.".into())
}

#[cfg(test)]
mod digest_tests {
    use super::{parse_digest, parse_releases};

    #[test]
    fn a_page_that_is_not_json_is_explained_in_spanish() {
        let err = parse_releases(b"<html>Inicia sesion en la red</html>").unwrap_err();
        assert!(err.contains("GitHub"), "{err}");
        assert!(!err.contains("expected value"), "{err}");
        assert_eq!(parse_releases(b"[{\"draft\": false}]").unwrap().len(), 1);
    }

    #[test]
    fn only_a_real_sha256_digest_is_used() {
        let hex = "ab".repeat(32);
        assert_eq!(
            parse_digest(Some(&format!("sha256:{hex}"))),
            Some(hex.clone())
        );
        assert_eq!(
            parse_digest(Some(&format!("sha256:{}", hex.to_uppercase()))),
            Some(hex)
        );
        assert_eq!(parse_digest(None), None);
        assert_eq!(parse_digest(Some("sha1:abcd")), None);
        assert_eq!(parse_digest(Some("sha256:1234")), None);
        assert_eq!(
            parse_digest(Some(&format!("sha256:{}", "zz".repeat(32)))),
            None
        );
    }
}
