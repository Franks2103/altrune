//! Tests of the Direct3D 12 layer (Apple's D3DMetal) that a Mac may already have beside the Metal
//! engine. Altrune never downloads or installs it: it only detects it and keeps using it.

use super::support::*;
use crate::backend::{runtime_env, Log, D3D12_PROGRAMS};
use crate::wine_setup::*;
use std::sync::{Arc, Mutex};

fn names(env: &[(String, String)]) -> Vec<String> {
    env.iter().map(|(k, _)| k.clone()).collect()
}

/// A layer that is already on the disk is detected, makes the engine's environment name it and
/// its programs, and keeps its loader script up to date.
#[test]
fn an_installed_layer_is_detected_and_used() {
    let dir = scratch("d3d12-installed");
    let folder = dir.join("Motor DXMT");
    let wine = fake_engine(&folder);
    let loader = folder.join("engine/lib/wine/x86_64-unix/wine");
    assert_eq!(d3d12_layer(&wine), None);
    assert!(!names(&runtime_env(&wine)).contains(&"BOTELLA_CAPA".to_string()));

    // The layer's files without the loader script in front of the engine's loader do not count.
    let layer = fake_installed_d3d12(&folder);
    assert_eq!(d3d12_layer(&wine), None);
    put_loader_script(&loader);
    assert_eq!(d3d12_layer(&wine), Some(layer.clone()));
    // The engine's own loader was moved aside, not lost.
    assert_eq!(
        std::fs::read_to_string(loader.with_file_name(REAL_LOADER)).unwrap(),
        "LOADER"
    );

    let env = runtime_env(&wine);
    let value = |name: &str| {
        env.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    assert_eq!(value("BOTELLA_CAPA"), value("WINEDLLPATH_PREPEND"));
    assert_eq!(value("BOTELLA_D3DMETAL"), layer.to_string_lossy());
    assert_eq!(value("BOTELLA_D3DMETAL_PROGRAMAS"), D3D12_PROGRAMS);

    // A script from an older version of the app is brought up to date, once.
    std::fs::write(&loader, "#!/bin/sh\n# old\n").unwrap();
    assert!(refresh_loader(&wine));
    assert_eq!(std::fs::read_to_string(&loader).unwrap(), LOADER_SCRIPT);
    assert!(!refresh_loader(&wine));
    let _ = std::fs::remove_dir_all(&dir);
}

/// An engine without the layer is left exactly as it is: no loader script is written for it.
#[test]
fn an_engine_without_the_layer_keeps_its_own_loader() {
    let dir = scratch("d3d12-absent");
    let folder = dir.join("Motor DXMT");
    let wine = fake_engine(&folder);
    let loader = folder.join("engine/lib/wine/x86_64-unix/wine");
    assert!(!refresh_loader(&wine));
    assert_eq!(std::fs::read_to_string(&loader).unwrap(), "LOADER");
    assert!(!loader.with_file_name(REAL_LOADER).exists());
    assert_eq!(d3d12_layer(&wine), None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Nothing starts a download of Apple's software any more: the games engine is refused by the
/// install itself, with the reason, and nothing is left running.
#[test]
fn the_games_engine_is_never_installed() {
    let progress: Shared = Arc::new(Mutex::new(Progress {
        running: true,
        ..Default::default()
    }));
    let log = Log::default();
    let state = install(progress.clone(), log.clone(), Engine::Games);
    assert_eq!(state.path(), None);
    let g = progress.lock().unwrap();
    assert!(!g.running);
    assert_eq!(g.error.as_deref(), Some(APPLE_SOFTWARE_NOT_INSTALLED));
    assert_eq!(g.step, None, "no step of a download began");
    drop(g);
    let lines = log.lock().unwrap().join("\n");
    assert!(!lines.contains("Descargando"), "{lines}");
}

/// The reason is one plain sentence about Apple's licence.
#[test]
fn the_reason_is_about_apples_licence() {
    assert!(APPLE_SOFTWARE_NOT_INSTALLED.starts_with("No se instala desde Altrune"));
    assert!(APPLE_SOFTWARE_NOT_INSTALLED.contains("software de Apple"));
    assert!(APPLE_SOFTWARE_NOT_INSTALLED.contains("licencia"));
    assert!(!APPLE_SOFTWARE_NOT_INSTALLED.contains("gratuito"));
}
