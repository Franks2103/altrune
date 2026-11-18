//! How Steam, its shortcuts and Steam games are started.

use std::fs;
use std::path::{Path, PathBuf};

/// Steam's own program inside a bottle, if installed.
pub fn steam_exe(prefix: &Path) -> Option<PathBuf> {
    [
        "drive_c/Program Files (x86)/Steam/steam.exe",
        "drive_c/Program Files/Steam/steam.exe",
    ]
    .iter()
    .map(|p| prefix.join(p))
    .find(|p| p.is_file())
}

/// The `steam://…` address a `.url` shortcut points at.
fn steam_url(file: &Path) -> Option<String> {
    let text = String::from_utf8_lossy(&fs::read(file).ok()?).to_string();
    text.lines()
        .find_map(|l| l.trim().strip_prefix("URL=").map(|u| u.trim().to_string()))
        .filter(|u| u.to_lowercase().starts_with("steam://"))
}

/// Steam (its shortcut, its .exe or a Steam game's `steam://` shortcut) starts as steam.exe from
/// Steam's folder, with no options for its web helper.
///
/// Its window is drawn by a separate GPU process of that helper. Steam no longer has a
/// `-cef-in-process-gpu` option that would run the helper's GPU work in its own process.
/// Measured on 2026-11-15 with client build 1794536281 on a clone of a real bottle: with
/// `-cef-disable-gpu -cef-disable-gpu-compositing` the window is black on the DXMT engine and
/// on the usual Wine; with no options it draws on the DXMT engine and stays black on the usual
/// Wine. So Steam gets no options here and its bottle
/// goes to the DXMT engine (see `Action::RunProgram`).
fn steam_launch(prefix: &Path, file: &Path, args: &str) -> Option<(Vec<String>, Option<PathBuf>)> {
    let ext = file
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let url = if ext == "url" {
        Some(steam_url(file)?)
    } else {
        None
    };
    if url.is_none() && !(stem == "steam" && (ext == "lnk" || ext == "exe")) {
        return None;
    }
    let exe = steam_exe(prefix)?;
    let mut v = vec![exe.to_string_lossy().to_string()];
    v.extend(url);
    v.extend(args.split_whitespace().map(|s| s.to_string()));
    Some((v, exe.parent().map(|p| p.to_path_buf())))
}

/// True when this file opens Steam (so it gets Steam's own start).
pub fn opens_steam(prefix: &Path, file: &Path) -> bool {
    steam_launch(prefix, file, "").is_some()
}

/// The arguments that open a file the way `wine --start` does: installers through
/// msiexec, shortcuts and links through Windows' own `start`, programs directly from their folder.
pub fn open_args(prefix: &Path, file: &Path, args: &str) -> (Vec<String>, Option<PathBuf>) {
    if let Some(steam) = steam_launch(prefix, file, args) {
        return steam;
    }
    let ext = file
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let f = file.to_string_lossy().to_string();
    let (mut v, dir) = match ext.as_str() {
        "msi" => (
            vec!["msiexec".into(), "/i".into(), f],
            file.parent().map(|p| p.to_path_buf()),
        ),
        "lnk" | "bat" | "url" => (
            vec!["start".into(), "/wait".into(), "/unix".into(), f],
            None,
        ),
        _ => (vec![f], file.parent().map(|p| p.to_path_buf())),
    };
    v.extend(args.split_whitespace().map(|s| s.to_string()));
    (v, dir)
}

#[cfg(test)]
mod steam_tests {
    use super::*;

    #[test]
    fn steam_starts_from_its_folder_without_web_helper_options() {
        let dir = std::env::temp_dir().join(format!("bl-steam-{}-argv", std::process::id()));
        let steam = dir.join("drive_c/Program Files (x86)/Steam");
        fs::create_dir_all(&steam).unwrap();
        fs::write(steam.join("steam.exe"), b"MZ").unwrap();
        let game = dir.join("Juego.url");
        fs::write(&game, "[InternetShortcut]\nURL=steam://rungameid/1790600\n").unwrap();
        let (argv, cwd) = open_args(&dir, &dir.join("x/Steam.lnk"), "");
        assert!(argv[0].ends_with("steam.exe"));
        // The no-GPU options are what leaves the window black on the DXMT engine.
        assert_eq!(argv.len(), 1, "{argv:?}");
        assert_eq!(cwd.unwrap(), steam);
        let (argv, _) = open_args(&dir, &game, "");
        assert_eq!(argv.last().unwrap(), "steam://rungameid/1790600");
        // Other programs are untouched.
        let (argv, _) = open_args(&dir, &dir.join("x/Otro.lnk"), "");
        assert_eq!(argv[0], "start");
        let _ = fs::remove_dir_all(&dir);
    }
}
