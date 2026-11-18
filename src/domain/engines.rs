//! A snapshot of the installed engines and what they can do. Reading it touches the disk (that
//! part is `EngineStatus::read`, in `wine_setup`), so the app reads it on start, after a rescan
//! and after each install, and the window only reads the snapshot.

use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EngineStatus {
    /// An Apple chip and macOS 14 or later: the Metal-based engines can run here.
    pub metal_capable: bool,
    /// The games engine (Apple's Game Porting Toolkit), when it is installed. Altrune does not
    /// install it.
    pub games: Option<PathBuf>,
    /// The DXMT engine, when installed.
    pub metal: Option<PathBuf>,
    /// Apple's D3DMetal (Direct3D 12) is installed beside the DXMT engine.
    pub d3d12: bool,
    /// Video (GStreamer) was added to the DXMT engine.
    pub video: bool,
    /// The libraries that let Wine use wired Xbox-style controllers are installed.
    pub controllers: bool,
}

/// What a bottle's graphics setting is called, and what its engine is called on technical
/// screens. This table is the only place the names are written: every screen, message and
/// diagnosis uses these words, so a person who reads one finds the same one in the list.
pub struct EngineName {
    /// What the bottle's `engine` setting holds (`None`: nothing saved).
    pub stored: Option<&'static str>,
    /// The name a person sees everywhere.
    pub name: &'static str,
    /// The engine's own name, which may follow in parentheses on technical screens only.
    pub technical: Option<&'static str>,
}

/// The name of "no engine saved": the plain Wine, moved to DXMT by itself when Steam opens.
pub const AUTOMATIC: &str = "Automático";
/// The DXMT engine: DirectX 10/11 drawn with Metal, which Steam and the launchers need.
pub const LAUNCHERS: &str = "Steam y launchers";
/// The games engine (Apple's D3DMetal): DirectX 11/12 games.
pub const GAMES: &str = "Juegos DirectX 12";
/// The plain Wine, which older versions could save as a choice of its own.
pub const PLAIN: &str = "Wine normal";

/// Every engine a bottle can have, the automatic one first. A value nobody offers any more reads
/// as the first row.
pub const ENGINE_NAMES: [EngineName; 4] = [
    EngineName {
        stored: None,
        name: AUTOMATIC,
        technical: None,
    },
    EngineName {
        stored: Some("dxmt"),
        name: LAUNCHERS,
        technical: Some("DXMT"),
    },
    EngineName {
        stored: Some("games"),
        name: GAMES,
        technical: Some("D3DMetal"),
    },
    EngineName {
        stored: Some("standard"),
        name: PLAIN,
        technical: None,
    },
];

/// The row of the table that a bottle's `engine` setting stands for.
fn engine_name(engine: Option<&str>) -> &'static EngineName {
    ENGINE_NAMES
        .iter()
        .find(|row| row.stored.is_some() && row.stored == engine)
        .unwrap_or(&ENGINE_NAMES[0])
}

/// The graphics choice shown for a bottle's `engine` setting.
pub fn engine_label(engine: Option<&str>) -> &'static str {
    engine_name(engine).name
}

/// The engine's own name (DXMT, D3DMetal), for technical screens and small print.
pub fn engine_technical(engine: Option<&str>) -> Option<&'static str> {
    engine_name(engine).technical
}

/// The name with the engine's own name after it, for technical screens only.
pub fn engine_label_technical(engine: Option<&str>) -> String {
    let row = engine_name(engine);
    match row.technical {
        Some(technical) => format!("{} ({technical})", row.name),
        None => row.name.to_string(),
    }
}

/// The badge of a bottle's engine: nothing while the engine is the automatic one, because
/// "Automático" says nothing to someone who never chose anything.
pub fn engine_badge(engine: Option<&str>) -> Option<&'static str> {
    let row = engine_name(engine);
    (row.stored.is_some()).then_some(row.name)
}

/// Whether opening Steam moves its bottle to the DXMT engine: only while the bottle is on "Auto"
/// (no engine chosen). "standard", "games" and "dxmt" are the user's choice and stay.
pub fn steam_moves_to_dxmt(engine: Option<&str>) -> bool {
    engine.is_none()
}

/// The Wine of the engine a bottle is set to use (`chosen` is its saved `engine` setting), taken
/// from a snapshot of the engines already read, so the window can ask on every frame without
/// touching the disk. `None` for every bottle that runs on the usual Wine.
pub fn bottle_engine_from(chosen: Option<&str>, engines: &EngineStatus) -> Option<PathBuf> {
    match chosen {
        Some("games") => engines.games.clone(),
        Some("dxmt") => engines.metal.clone(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The labels match the choices offered in a bottle's graphics setting.
    #[test]
    fn every_saved_engine_has_its_label() {
        assert_eq!(engine_label(None), "Automático");
        assert_eq!(engine_label(Some("games")), "Juegos DirectX 12");
        assert_eq!(engine_label(Some("dxmt")), "Steam y launchers");
        assert_eq!(engine_label(Some("standard")), "Wine normal");
        assert_eq!(engine_label(Some("otro")), "Automático");
    }

    /// The technical name follows in parentheses only where asked for.
    #[test]
    fn the_technical_name_only_follows_when_asked() {
        assert_eq!(
            engine_label_technical(Some("dxmt")),
            "Steam y launchers (DXMT)"
        );
        assert_eq!(
            engine_label_technical(Some("games")),
            "Juegos DirectX 12 (D3DMetal)"
        );
        assert_eq!(engine_label_technical(None), "Automático");
        assert_eq!(engine_technical(Some("dxmt")), Some("DXMT"));
        assert_eq!(engine_technical(None), None);
        for row in &ENGINE_NAMES {
            assert!(!row.name.contains('('), "{}", row.name);
        }
    }

    /// The automatic engine has no badge; the others are named.
    #[test]
    fn the_automatic_engine_has_no_badge() {
        assert_eq!(engine_badge(None), None);
        assert_eq!(engine_badge(Some("otro")), None);
        assert_eq!(engine_badge(Some("dxmt")), Some("Steam y launchers"));
        assert_eq!(engine_badge(Some("standard")), Some("Wine normal"));
    }

    /// Each engine has a name of its own and a value of its own in the table.
    #[test]
    fn the_names_are_all_different() {
        for (i, a) in ENGINE_NAMES.iter().enumerate() {
            for b in &ENGINE_NAMES[i + 1..] {
                assert_ne!(a.name, b.name);
                assert_ne!(a.stored, b.stored);
            }
        }
    }
    /// Every Rust file of the program, but the flow tests and the files that are only compiled
    /// for tests. Folders are walked, so a file added or split off later is checked without
    /// anyone editing a list.
    fn program_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .flatten()
            .map(|e| e.path())
            .collect();
        entries.sort();
        for path in entries {
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if path.is_dir() {
                if name != "flows" && name != "tests" {
                    program_files(&path, out);
                }
            } else if path.extension().is_some_and(|e| e == "rs")
                && name != "tests"
                && !name.ends_with("_tests")
            {
                out.push(path);
            }
        }
    }

    /// The part of a source file before its test module. Only a `#[cfg(test)]` that opens a
    /// module with a body ends the program's code: the same attribute on a single helper or on a
    /// `mod x;` line leaves the rest of the file to be read.
    fn before_test_module(text: &str) -> &str {
        for (at, _) in text.match_indices("#[cfg(test)]") {
            let after = text[at + "#[cfg(test)]".len()..].trim_start();
            let item = after.lines().next().unwrap_or("");
            if item.starts_with("mod ") && item.trim_end().ends_with('{') {
                return &text[..at];
            }
        }
        text
    }

    /// A test attribute on a helper does not hide the rest of the file from the scan, and the
    /// test module does.
    #[test]
    fn the_scan_reads_past_a_test_helper_and_stops_at_the_test_module() {
        let file = "#[cfg(test)]\nmod flows;\nfn a() {}\n#[cfg(test)]\npub fn b() {}\nfn c() {}\n\n#[cfg(test)]\nmod tests {\n fn d() {}\n}\n";
        let code = before_test_module(file);
        assert!(code.contains("fn a()") && code.contains("fn b()") && code.contains("fn c()"));
        assert!(!code.contains("fn d()"));
        assert_eq!(before_test_module("fn only() {}"), "fn only() {}");
    }

    /// The screens and messages that speak of an engine use the names of the table: none of the
    /// names this app used before (each its own way of saying the same thing) is left.
    #[test]
    fn no_screen_or_message_uses_an_old_name() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![];
        program_files(&root, &mut files);
        // The scan must not pass by finding nothing: the folders hold well over this many.
        assert!(
            files.len() > 60,
            "the scan found only {} files",
            files.len()
        );
        let old = [
            "Motor para juegos",
            "motor para juegos",
            "Motor DXMT",
            "motor DXMT",
            "Juegos con DirectX 12",
            "Gráficos (DirectX)",
            "motor normal",
            "\"Auto\"",
            "Wine estándar",
            "Falta el motor",
        ];
        for path in files {
            let file = path.strip_prefix(&root).unwrap().display().to_string();
            let text = std::fs::read_to_string(&path).unwrap();
            // The tests of a file may quote the old names to say they are gone.
            let code = before_test_module(&text);
            // The folder the DXMT engine is installed in is a name on the disk, not on a screen.
            let code = code.replace("Engine::Metal => \"Motor DXMT\",", "");
            // The doc of `steam_moves_to_dxmt` in this file says the bottle is "on Auto" for the
            // automatic engine: it is prose for the reader of the code, not a text of a screen.
            let code = code.replace("only while the bottle is on \"Auto\"", "");
            for name in old {
                assert!(!code.contains(name), "{file} still says «{name}»");
            }
        }
    }

    /// Steam takes a bottle on "Auto" to the DXMT engine, never one the user set.
    #[test]
    fn steam_keeps_an_engine_the_user_chose() {
        assert!(steam_moves_to_dxmt(None));
        for chosen in ["standard", "games", "dxmt"] {
            assert!(!steam_moves_to_dxmt(Some(chosen)));
        }
    }

    /// A bottle takes the Wine of the engine it chose, when that engine is installed, and the
    /// usual Wine (no engine here) in every other case.
    #[test]
    fn a_bottle_takes_the_wine_of_the_engine_it_chose() {
        let engines = EngineStatus {
            games: Some(PathBuf::from("/e/games/wine")),
            metal: Some(PathBuf::from("/e/metal/wine")),
            ..EngineStatus::default()
        };
        assert_eq!(
            bottle_engine_from(Some("games"), &engines),
            Some(PathBuf::from("/e/games/wine"))
        );
        assert_eq!(
            bottle_engine_from(Some("dxmt"), &engines),
            Some(PathBuf::from("/e/metal/wine"))
        );
        for usual in [None, Some("standard"), Some("otro")] {
            assert_eq!(bottle_engine_from(usual, &engines), None);
        }
        // An engine that is not installed gives no Wine: the bottle falls back to the usual one.
        assert_eq!(
            bottle_engine_from(Some("games"), &EngineStatus::default()),
            None
        );
    }
}
