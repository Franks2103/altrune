//! A bottle made long ago, as the ones in Altrune's own folder that came from elsewhere: it has
//! the settings file `cxbottle.conf`, a Windows profile folder that the Mac user's folder is a
//! link to, and a recipe id that an older version recorded. It must load, keep MSync and start
//! its programs with the environment it always had.

use super::driver::Flow;
use super::fake_wine::FakeWine;
use super::fixtures;
use crate::backend::{
    bridge_user_profile, data_dir, mac_user, open_command, program_env, DEFAULT_WINEDEBUG,
};
use crate::Action;
use std::collections::BTreeMap;
use std::fs;
use std::time::Duration;

const LONG: Duration = Duration::from_secs(20);

/// The id an older version recorded for the Steam recipe: it is no id of this version.
const OLD_RECIPE_ID: &str = "com.example.antigua.206";

/// The settings file such a bottle carries: MSync on, one variable of its own, and one of the
/// kind a bottle keeps for its maker's graphics backend (never passed on).
const BOTTLE_CONF: &str = r#"[Bottle]
"Template" = "win10_64"
"Updated" = "2025-11-02"

[EnvironmentVariables]
"WINEMSYNC" = "1"
"MTL_HUD_ENABLED" = "1"
"CX_GRAPHICS_BACKEND" = "d3dmetal"
"#;

/// The home of the fake Wine, made to answer that it reads `WINEMSYNC` (it looks for the word in
/// its own files), as a Wine with MSync does.
fn fake_wine_with_msync(home: &std::path::Path) {
    let ntdll = home.join("wine-falso/lib/wine/x86_64-unix/ntdll.so");
    fs::create_dir_all(ntdll.parent().unwrap()).expect("the Wine's library folder");
    fs::write(ntdll, b"\x7fELF....WINEMSYNC\0").expect("ntdll.so");
}

flow_test!(
    a_bottle_made_long_ago_loads_keeps_msync_and_starts_programs_as_before,
    |home| {
        let fake = FakeWine::write(home);
        fake_wine_with_msync(home);
        let bottle = fixtures::bottle_with("Steam (copia)", &["Steam"]);
        fs::write(bottle.join("cxbottle.conf"), BOTTLE_CONF).expect("cxbottle.conf");
        // The profile the programs were installed with has a folder of its own; the Mac user's
        // folder is a link to it.
        let me = mac_user();
        let users = bottle.join("drive_c/users");
        let _ = fs::remove_dir_all(users.join(&me));
        fs::create_dir_all(users.join("perfil-antiguo/AppData/Local/Steam")).unwrap();
        fs::write(users.join("perfil-antiguo/AppData/Local/Steam/datos"), "x").unwrap();
        std::os::unix::fs::symlink("perfil-antiguo", users.join(&me)).unwrap();
        // The settings an older version wrote: a switch and an origin of features that are gone,
        // and the recipe id it recorded.
        let file = data_dir().join("config.json");
        let mut cfg: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        let key = bottle.to_string_lossy().to_string();
        cfg["mostrar_botellas_de_otra_app"] = true.into();
        cfg["default_bottle"] = key.clone().into();
        cfg["bottles"] = serde_json::json!({
            key.clone(): {
                "msync": null,
                "windows": "win10",
                "recipes": [OLD_RECIPE_ID],
                "copied_from": "/otra/carpeta/Steam",
            }
        });
        fs::write(&file, cfg.to_string()).unwrap();

        let mut f = Flow::start_prepared(fake);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());

        // It loads: listed, complete, with its programs, its MSync and its own variables.
        assert_eq!(f.app.bottles.len(), 1, "{}", f.dump());
        let b = f.app.bottles[0].clone();
        assert_eq!(b.name, "Steam (copia)");
        assert!(b.prefix_complete && b.msync);
        assert_eq!(b.windows, "win10");
        assert_eq!(b.env, [("MTL_HUD_ENABLED".to_string(), "1".to_string())]);
        assert!(b.programs.iter().any(|p| p.name == "Steam"), "{b:?}");
        assert_eq!(
            f.app.cfg.default_bottle.as_deref(),
            Some(key.as_str()),
            "the settings were read in full"
        );
        // The profile link is left as it was.
        assert!(bridge_user_profile(&bottle).is_none());
        assert_eq!(
            fs::read_link(users.join(&me)).unwrap(),
            std::path::Path::new("perfil-antiguo")
        );

        // The program starts in the same Wine with the same environment as before.
        let wine = f.app.wine.clone().expect("Wine");
        let exe = bottle.join("drive_c/Program Files/Steam/programa.exe");
        let command = open_command(&wine, &b, &exe, "", &[]);
        assert_eq!(command.get_program(), wine.as_os_str());
        let env: BTreeMap<String, String> = command
            .get_envs()
            .filter_map(|(k, v)| {
                Some((
                    k.to_string_lossy().to_string(),
                    v?.to_string_lossy().to_string(),
                ))
            })
            .collect();
        let debug = std::env::var("WINEDEBUG").unwrap_or_else(|_| DEFAULT_WINEDEBUG.to_string());
        let expected: BTreeMap<String, String> = [
            ("WINEPREFIX", bottle.to_string_lossy().to_string()),
            ("WINEMSYNC", "1".to_string()),
            ("WINEDEBUG", debug),
            ("DOTNET_EnableWriteXorExecute", "0".to_string()),
            ("ROSETTA_ADVERTISE_AVX", "1".to_string()),
            ("MTL_HUD_ENABLED", "1".to_string()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        assert_eq!(env, expected);
        // The same variables come out of the environment the program is given before the Wine
        // adapts it (the settings file's own variables on top of the base ones).
        let (plain, removed) = program_env(&b, &[]);
        assert!(removed.is_empty());
        assert!(plain.contains(&("WINEMSYNC".to_string(), "1".to_string())));
        assert!(!plain.iter().any(|(k, _)| k.starts_with("CX_")));

        // Opening its program writes the recipe again, because the id it recorded is not one
        // of this version's: the same values with /f, which changes nothing. Both ids are kept.
        f.act(Action::Select(0));
        let i = f.app.bottles[0]
            .programs
            .iter()
            .position(|p| p.name == "Steam")
            .expect("Steam");
        f.act(Action::RunProgram(i));
        f.wait_for("the recipe to be recorded", LONG, |f| {
            f.app
                .cfg
                .bottle_ref(&bottle)
                .is_some_and(|s| s.recipes.iter().any(|r| r == "steam"))
        });
        let recipes = f.app.cfg.bottle_ref(&bottle).unwrap().recipes.clone();
        assert_eq!(recipes, [OLD_RECIPE_ID, "steam"]);
        let written = fs::read_to_string(bottle.join("registro-falso.txt")).unwrap_or_default();
        assert_eq!(written.matches("reg add").count(), 3, "{written}");
        assert!(
            written.contains("GPUAccelWebViews") && written.contains("/f"),
            "{written}"
        );
        // The fake Wine ends this shortcut at once; what matters is that it was started once.
        f.wait_for("the program to be started", LONG, |f| {
            f.fake.count("Steam.lnk") == 1
        });
        f.finish();
    }
);
