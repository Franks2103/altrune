//! Tests of video (GStreamer) added beside the Metal engine.

use super::support::*;
use crate::backend::{runtime_env, Log};
use crate::wine_setup::*;
use std::path::PathBuf;
use std::process::Command;

/// Video added beside an engine is found, and GStreamer is told to look only there.
#[test]
fn video_is_found_beside_the_engine() {
    let dir = std::env::temp_dir().join(format!("bl-video-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let wine = fake_engine(&dir.join("Motor DXMT"));
    assert_eq!(video_layer(&wine), None);
    assert!(!runtime_env(&wine)
        .iter()
        .any(|(k, _)| k.starts_with("GST_")));
    let lib = dir.join("Motor DXMT/gstreamer/lib");
    write_exe(&lib.join("libgstreamer-1.0.0.dylib"), "x");
    write_exe(&lib.join("gstreamer-1.0/libgstcoreelements.dylib"), "x");
    assert_eq!(video_layer(&wine), None);
    write_exe(&lib.join("gstreamer-1.0/libgstisomp4.dylib"), "x");
    write_exe(&lib.join("gstreamer-1.0/libgstlibav.dylib"), "x");
    assert_eq!(video_layer(&wine), None);
    write_exe(&lib.join("gstreamer-1.0/libgstdeinterlace.dylib"), "x");
    assert_eq!(video_layer(&wine), Some(lib.clone()));
    let env = runtime_env(&wine);
    let value = |name: &str| {
        env.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let plugins = lib.join("gstreamer-1.0").to_string_lossy().to_string();
    assert_eq!(value("GST_PLUGIN_SYSTEM_PATH_1_0"), plugins);
    assert_eq!(value("GST_PLUGIN_PATH_1_0"), plugins);
    assert_eq!(value("GST_REGISTRY_FORK"), "no");
    assert!(value("GST_REGISTRY_1_0").ends_with("gstreamer/registro-x86_64.bin"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Manual, with the real package, on a copy of the engine:
/// `BOTELLA_MOTOR=<copy of the "Motor DXMT" folder> BOTELLA_PAQUETE=<the GStreamer .pkg>
/// cargo test -- --ignored --nocapture video_is_added_for_real`.
/// Every kept file is Intel only, and every library Wine and the plugins link is there.
#[test]
#[ignore = "unpacks the real GStreamer package"]
fn video_is_added_for_real() {
    let folder = PathBuf::from(std::env::var("BOTELLA_MOTOR").expect("BOTELLA_MOTOR"));
    let package = PathBuf::from(std::env::var("BOTELLA_PAQUETE").expect("BOTELLA_PAQUETE"));
    let wine = folder.join("engine/bin/wine");
    add_video(&wine, &package, &Log::default()).unwrap();
    let lib = video_layer(&wine).expect("video not found");
    let mut files = vec![];
    for dir in [lib.clone(), lib.join("gstreamer-1.0")] {
        files.extend(std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()));
    }
    let engine_module = folder.join("engine/lib/wine/x86_64-unix/winegstreamer.so");
    files.push(engine_module);
    for f in files
        .iter()
        .filter(|f| f.is_file() && f.extension().is_some_and(|e| e == "dylib" || e == "so"))
    {
        for name in rpath_links(f) {
            let own = f.file_name().unwrap().to_string_lossy() == name.as_str();
            let engine = name == "ntdll.so" || name == "winegstreamer.so";
            assert!(
                own || engine || lib.join(&name).exists(),
                "{} needs {name}",
                f.display()
            );
        }
    }
    for f in files
        .iter()
        .filter(|f| f.starts_with(&lib) && f.is_file() && !f.is_symlink())
    {
        let archs = Command::new("/usr/bin/lipo")
            .arg("-archs")
            .arg(f)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&archs.stdout).trim(),
            "x86_64",
            "{}",
            f.display()
        );
    }
    for p in [
        "libav",
        "applemedia",
        "isomp4",
        "videoparsersbad",
        "asf",
        "deinterlace",
    ] {
        assert!(
            lib.join(format!("gstreamer-1.0/libgst{p}.dylib")).is_file(),
            "{p}"
        );
    }
    let size: u64 = files
        .iter()
        .filter(|f| f.starts_with(&lib))
        .filter_map(|f| f.symlink_metadata().ok())
        .map(|m| m.len())
        .sum();
    println!("{} files, {} MB", files.len() - 1, size / 1_000_000);
}

/// A folder that cannot be written to makes the setup fail in plain Spanish, never with the
/// system's own English text («Permission denied (os error 13)»).
#[test]
fn video_that_cannot_be_unpacked_is_explained_in_spanish() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("bl-video-ro-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let folder = dir.join("Motor DXMT");
    let wine = fake_engine(&folder);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
    let writable = std::fs::create_dir(folder.join("prueba")).is_ok();
    let result = add_video(&wine, &dir.join("no-importa.pkg"), &Log::default());
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    if !writable {
        let err = result.unwrap_err();
        assert_eq!(err, "No tengo permiso para hacerlo.");
    }
    // An engine folder that cannot be found names the choice as the list does.
    let err = add_video(
        std::path::Path::new("/wine"),
        &dir.join("no-importa.pkg"),
        &Log::default(),
    )
    .unwrap_err();
    assert!(err.contains(crate::domain::engines::LAUNCHERS), "{err}");
    assert!(!err.contains("DXMT"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The sentence of a disk failure while video is added is the plain one, and the log keeps its
/// detail: what was being done, where, the kind of the error and the system's own code.
#[test]
fn a_video_disk_failure_leaves_its_detail_in_the_log() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("bl-video-detail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let folder = dir.join("Motor DXMT");
    let wine = fake_engine(&folder);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
    let writable = std::fs::create_dir(folder.join("prueba")).is_ok();
    let log = Log::default();
    let result = add_video(&wine, &dir.join("no-importa.pkg"), &log);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    if !writable {
        assert_eq!(result.unwrap_err(), "No tengo permiso para hacerlo.");
        let lines = log.lock().unwrap().join("\n");
        assert!(lines.contains("Preparar la carpeta del video"), "{lines}");
        assert!(lines.contains(&folder.join("gstreamer.nuevo").display().to_string()));
        assert!(lines.contains("Tipo: PermissionDenied"), "{lines}");
        assert!(lines.contains("Código del sistema: 13."), "{lines}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
