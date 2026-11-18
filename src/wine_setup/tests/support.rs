//! Helpers the tests share: stand-in executables, a stand-in Metal engine and its D3DMetal layer.

use crate::wine_setup::*;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub(super) fn write_exe(file: &Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Runs the loader script as Wine does (the program's command line as its arguments) in
/// front of a stand-in loader, and returns what that loader received: the layer, the
/// libd3dshared variable, and its arguments.
pub(super) fn loader_run(dir: &Path, env: &[(&str, &str)], args: &[&str]) -> Vec<String> {
    write_exe(&dir.join("wine"), LOADER_SCRIPT);
    write_exe(
            &dir.join(REAL_LOADER),
            "#!/bin/sh\nprintf '%s\\n' \"$WINEDLLPATH_PREPEND\" \"$CX_APPLEGPTK_LIBD3DSHARED_PATH\" \"$@\"\n",
        );
    let out = Command::new(dir.join("wine"))
        .args(args)
        .env_clear()
        .envs(env.iter().copied())
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// A Metal engine as the app installs it, with stand-in files.
pub(super) fn fake_engine(folder: &Path) -> PathBuf {
    write_exe(&folder.join("engine/bin/wine"), "#!/bin/sh\n");
    write_exe(&folder.join("engine/lib/wine/x86_64-unix/wine"), "LOADER");
    for f in ["x86_64-windows/d3d11.dll", "x86_64-unix/winemetal.so"] {
        write_exe(&folder.join("dxmt").join(f), "x");
    }
    folder.join("engine/bin/wine")
}

/// The files of an Apple D3DMetal layer beside a Metal engine, as an earlier install left them
/// (stand-ins). Returns the layer's folder.
pub(super) fn fake_installed_d3d12(folder: &Path) -> PathBuf {
    let layer = folder.join("d3dmetal");
    write_exe(&layer.join("wine/x86_64-windows/d3d12.dll"), "dll");
    write_exe(&layer.join("external/libd3dshared.dylib"), "shared");
    layer
}

/// Puts the loader script in front of an engine's loader, which moves aside, as it was when the
/// layer was installed.
pub(super) fn put_loader_script(loader: &Path) {
    std::fs::rename(loader, loader.with_file_name(REAL_LOADER)).unwrap();
    write_exe(loader, LOADER_SCRIPT);
}

/// An empty folder of the system's temporary files, named for the test and the process.
pub(in crate::wine_setup) fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bl-setup-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// What `ps` says about a process: its state letters, or nothing when it is gone.
pub(in crate::wine_setup) fn process_state(pid: u32) -> String {
    let out = Command::new("/bin/ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Waits (up to ten seconds) until `ready` says so; the test fails when it never does.
pub(in crate::wine_setup) fn wait_for(what: &str, mut ready: impl FnMut() -> bool) {
    let end = std::time::Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(
            std::time::Instant::now() < end,
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
