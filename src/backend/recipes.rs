//! Per-program fixes: before a known program is installed (or first opened in a bottle that
//! never had them), its registry tweaks are written into the bottle.
//!
//! The recipes are built in, for the programs people use most (each value is explained where it
//! is set). A recipe's id is stored in `config.json` for each bottle it was written into, so it
//! is written once; a bottle that recorded an id this version does not know simply gets the
//! recipe again, which is harmless: the same values are written over themselves.

use crate::backend::{bottle_cmd, push_log, run_logged, Bottle, Log};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct RegValue {
    pub key: String,
    pub name: String,
    /// Registry-file notation: `dword:1`, `hex:…`, or plain text.
    pub data: String,
}

#[derive(Clone, Debug)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    /// Installer file names it applies to (`*` wildcards).
    pub globs: Vec<String>,
    /// Lower-case words that identify the program by its name.
    pub words: Vec<String>,
    pub registry: Vec<RegValue>,
}

fn rv(key: &str, name: &str, data: &str) -> RegValue {
    RegValue {
        key: key.into(),
        name: name.into(),
        data: data.into(),
    }
}

/// The recipes for the programs people use most.
pub fn builtin() -> Vec<Recipe> {
    vec![
        Recipe {
            id: "steam".into(),
            name: "Steam".into(),
            globs: vec!["SteamSetup*.exe".into(), "SteamInstall*.msi".into()],
            words: vec!["steam".into()],
            registry: vec![
                // Steam draws its web views (store, library, friends) without GPU acceleration.
                rv(
                    "HKEY_CURRENT_USER\\Software\\Valve\\Steam",
                    "GPUAccelWebViews",
                    "dword:0",
                ),
                // Mark steamwebhelper.exe as large-address-aware, so it can use more than 2 GB of
                // memory instead of the limit of an ordinary 32-bit program.
                rv(
                    "HKEY_CURRENT_USER\\Software\\Wine\\AppDefaults\\steamwebhelper.exe",
                    "LargeAddressAware",
                    "dword:1",
                ),
                // Draw Lucida Console text with the MS Sans Serif font.
                rv(
                    "HKEY_CURRENT_USER\\Software\\Wine\\Fonts\\Replacements",
                    "Lucida Console",
                    "MS Sans Serif",
                ),
            ],
        },
        Recipe {
            id: "rockstar-games-launcher".into(),
            name: "Rockstar Games Launcher".into(),
            globs: vec!["Rockstar-Games-Launcher*.exe".into()],
            words: vec!["rockstar".into()],
            // No registry value is needed; the program only wants extra components (Visual C++
            // runtime, fonts, d3dcompiler), which are installed separately.
            registry: vec![],
        },
    ]
}

fn glob_match(glob: &str, name: &str) -> bool {
    let g = glob.to_lowercase();
    let n = name.to_lowercase();
    let parts: Vec<&str> = g.split('*').collect();
    if parts.len() == 1 {
        return g == n;
    }
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match n[pos..].find(part) {
            Some(found) => {
                if i == 0 && found != 0 {
                    return false;
                }
                pos += found + part.len();
            }
            None => return false,
        }
    }
    parts
        .last()
        .map(|l| l.is_empty() || n.ends_with(l))
        .unwrap_or(true)
}

/// The recipe for an installer file, by its file name.
pub fn for_installer(file: &Path) -> Option<Recipe> {
    let file_name = file.file_name()?.to_string_lossy().to_string();
    builtin()
        .into_iter()
        .find(|r| r.globs.iter().any(|g| glob_match(g, &file_name)))
}

/// The built-in recipe for a program by its name (e.g. a Start Menu shortcut "Steam").
pub fn for_program(name: &str) -> Option<Recipe> {
    let n = name.to_lowercase();
    builtin()
        .into_iter()
        .find(|r| r.words.iter().any(|w| n.contains(w.as_str())))
}

/// `reg add` arguments for one value, or None for kinds this app doesn't write.
pub fn reg_add_args(v: &RegValue) -> Option<Vec<String>> {
    let (kind, data) = if let Some(d) = v.data.strip_prefix("dword:") {
        // Registry files write dwords in hex.
        let n = u32::from_str_radix(d.trim(), 16).ok()?;
        ("REG_DWORD", n.to_string())
    } else if let Some(h) = v.data.strip_prefix("hex:") {
        ("REG_BINARY", h.replace([',', ' ', '\\', '\n'], ""))
    } else if v.data.starts_with("str(") {
        return None; // expandable/multi strings: left out
    } else {
        ("REG_SZ", v.data.clone())
    };
    Some(vec![
        "reg".into(),
        "add".into(),
        v.key.clone(),
        "/v".into(),
        v.name.clone(),
        "/t".into(),
        kind.into(),
        "/d".into(),
        data,
        "/f".into(),
    ])
}

/// Writes a recipe's registry values into a bottle. Returns how many were written.
pub fn apply(wine: &Path, b: &Bottle, r: &Recipe, log: &Log) -> usize {
    let mut n = 0;
    for v in &r.registry {
        let Some(args) = reg_add_args(v) else {
            continue;
        };
        let mut c = bottle_cmd(wine, b);
        c.args(&args);
        if run_logged(c, log, &format!("Receta {}: {}\\{}", r.name, v.key, v.name)) {
            n += 1;
        }
    }
    push_log(
        log,
        format!("✔ Receta de {} aplicada ({n} ajustes)", r.name),
    );
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_match_installers() {
        assert!(glob_match("SteamSetup*.exe", "SteamSetup.exe"));
        assert!(glob_match(
            "Rockstar-Games-Launcher*.exe",
            "Rockstar-Games-Launcher.exe"
        ));
        assert!(!glob_match("SteamSetup*.exe", "MySteamSetup.exe"));
        assert!(glob_match(
            "Diablo-III-Setup*.exe",
            "Diablo-III-Setup-esES.exe"
        ));
    }

    #[test]
    fn steam_recipe_found_and_written_as_reg_add() {
        let r = for_installer(Path::new("/x/SteamSetup.exe")).expect("steam");
        assert_eq!(r.name, "Steam");
        let gpu = r
            .registry
            .iter()
            .find(|v| v.name == "GPUAccelWebViews")
            .unwrap();
        let args = reg_add_args(gpu).unwrap();
        assert_eq!(args[6], "REG_DWORD");
        assert_eq!(args[8], "0");
        assert!(for_program("Steam").is_some());
    }

    /// The ids are Altrune's own, and each recipe is found by its installer and by its name.
    #[test]
    fn the_two_recipes_have_their_own_ids() {
        let ids: Vec<String> = builtin().into_iter().map(|r| r.id).collect();
        assert_eq!(ids, ["steam", "rockstar-games-launcher"]);
        let rockstar =
            for_installer(Path::new("/x/Rockstar-Games-Launcher.exe")).expect("rockstar");
        assert_eq!(rockstar.id, "rockstar-games-launcher");
        assert!(rockstar.registry.is_empty());
        assert_eq!(
            for_program("Rockstar Games Launcher").map(|r| r.id),
            Some("rockstar-games-launcher".to_string())
        );
        assert_eq!(
            for_installer(Path::new("/x/SteamSetup.exe")).unwrap().id,
            "steam"
        );
        assert!(for_installer(Path::new("/x/otro.exe")).is_none());
    }

    /// Every registry value of the Steam recipe, as `reg add` writes it.
    #[test]
    fn the_steam_recipe_keeps_every_value() {
        let r = for_program("Steam").expect("steam");
        let args: Vec<(String, String, String, String)> = r
            .registry
            .iter()
            .map(|v| {
                let a = reg_add_args(v).unwrap();
                (a[2].clone(), a[4].clone(), a[6].clone(), a[8].clone())
            })
            .collect();
        let row = |k: &str, n: &str, t: &str, d: &str| (k.into(), n.into(), t.into(), d.into());
        assert_eq!(
            args,
            [
                row(
                    "HKEY_CURRENT_USER\\Software\\Valve\\Steam",
                    "GPUAccelWebViews",
                    "REG_DWORD",
                    "0"
                ),
                row(
                    "HKEY_CURRENT_USER\\Software\\Wine\\AppDefaults\\steamwebhelper.exe",
                    "LargeAddressAware",
                    "REG_DWORD",
                    "1"
                ),
                row(
                    "HKEY_CURRENT_USER\\Software\\Wine\\Fonts\\Replacements",
                    "Lucida Console",
                    "REG_SZ",
                    "MS Sans Serif"
                ),
            ]
        );
    }
}
