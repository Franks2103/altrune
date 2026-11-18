//! Tests of the engines, the environment, the guard of where Wine may run, and copies.

use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

mod engine_tests {
    use super::*;

    fn fake_wine(dir: &Path, marker: &[u8]) -> PathBuf {
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::create_dir_all(dir.join("lib/wine/x86_64-unix")).unwrap();
        let mut so = b"\x7fELF....".to_vec();
        so.extend_from_slice(marker);
        fs::write(dir.join("lib/wine/x86_64-unix/ntdll.so"), so).unwrap();
        fs::write(dir.join("bin/wine"), b"#!/bin/sh").unwrap();
        dir.join("bin/wine")
    }

    /// A Metal engine as the app installs it: `<dir>/engine/bin/wine` beside `<dir>/dxmt`.
    fn fake_metal_engine(dir: &Path) -> PathBuf {
        let wine = fake_wine(&dir.join("engine"), b"WINEMSYNC\0");
        for f in ["x86_64-windows/d3d11.dll", "x86_64-unix/winemetal.so"] {
            let f = dir.join("dxmt").join(f);
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(f, b"x").unwrap();
        }
        wine
    }

    fn value<'a>(env: &'a [(String, String)], name: &str) -> Option<&'a str> {
        let mut found = env.iter().filter(|(k, _)| k == name);
        let first = found.next().map(|(_, v)| v.as_str());
        assert!(found.next().is_none(), "{name} is set twice");
        first
    }

    /// The Rockstar Games Launcher draws its window with Direct2D, which needs Direct3D 10, and
    /// its browser's GPU process draws into a window of another process. Both only work when
    /// DXMT is in front of Wine's Direct3D and may present across processes.
    #[test]
    fn metal_engine_starts_with_dxmt_in_front() {
        let base = std::env::temp_dir().join(format!("bl-metal-{}", std::process::id()));
        let wine = fake_metal_engine(&base.join("Motor DXMT"));
        assert_eq!(Engine::of(&wine), Engine::Metal);
        let mut env = base_env(&base.join("bottle"), true);
        adapt_env_to_wine(&wine, &mut env);
        assert_eq!(
            value(&env, "WINEDLLPATH_PREPEND"),
            Some(&*base.join("Motor DXMT/dxmt").to_string_lossy())
        );
        assert_eq!(value(&env, "DXMT_ALLOW_CROSS_PROCESS_SWAPCHAIN"), Some("1"));
        assert_eq!(value(&env, "WINEMSYNC"), Some("1"));

        // What the bottle or the program sets itself is not overwritten.
        let mut env = vec![(
            "DXMT_ALLOW_CROSS_PROCESS_SWAPCHAIN".to_string(),
            "0".to_string(),
        )];
        adapt_env_to_wine(&wine, &mut env);
        assert_eq!(value(&env, "DXMT_ALLOW_CROSS_PROCESS_SWAPCHAIN"), Some("0"));

        // Half an engine (no DXMT beside it) is not taken for one.
        let alone = fake_wine(&base.join("solo/engine"), b"WINEMSYNC\0");
        assert_eq!(Engine::of(&alone), Engine::Standard);
        let _ = fs::remove_dir_all(&base);
    }

    /// Steam and everything else on the usual Wine start exactly as before the Metal engine.
    #[test]
    fn other_engines_get_nothing_from_the_metal_engine() {
        let base = std::env::temp_dir().join(format!("bl-plain-{}", std::process::id()));
        let wine = fake_wine(
            &base.join("Wine Stable.app/Contents/Resources/wine"),
            b"x\0",
        );
        let mut env = base_env(&base.join("bottle"), true);
        let mut expected = env.clone();
        expected.retain(|(k, _)| k != "WINEMSYNC");
        adapt_env_to_wine(&wine, &mut env);
        assert_eq!(env, expected);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sync_switch_follows_what_wine_reads() {
        let base = std::env::temp_dir().join(format!("bl-sync-{}", std::process::id()));
        let none = fake_wine(&base.join("a"), b"nothing\0");
        let esync = fake_wine(&base.join("b"), b"WINEESYNC\0");
        let msync = fake_wine(&base.join("c"), b"WINEMSYNC\0");
        assert_eq!(sync_var(&none), None);
        assert_eq!(sync_var(&esync), Some("WINEESYNC"));
        assert_eq!(sync_var(&msync), Some("WINEMSYNC"));
        let mut env = vec![("WINEMSYNC".to_string(), "1".to_string())];
        adapt_env_to_wine(&esync, &mut env);
        assert_eq!(env, vec![("WINEESYNC".to_string(), "1".to_string())]);
        let mut env = vec![("WINEMSYNC".to_string(), "1".to_string())];
        adapt_env_to_wine(&none, &mut env);
        assert!(env.is_empty());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn wine_versions_and_downgrades() {
        assert_eq!(wine_version_number("wine-11.0"), Some((11, 0)));
        assert_eq!(
            wine_version_number("wine-7.7 (Game Porting Toolkit 1.1)"),
            Some((7, 7))
        );
        assert!(is_downgrade(
            "wine-11.0",
            "wine-7.7 (Game Porting Toolkit 1.1)"
        ));
        assert!(!is_downgrade("wine-7.7", "wine-11.0"));
        assert!(!is_downgrade("wine-11.0", "wine-11.0"));
    }

    fn bottle_at(path: PathBuf) -> Bottle {
        Bottle {
            name: "x".into(),
            path,
            windows: "win10".into(),
            msync: true,
            high_res: false,
            logpixels: DEFAULT_LOGPIXELS,
            programs: vec![],
            env: vec![],
            prefix_complete: true,
        }
    }

    /// Prefixes that are not a bottle of the app: another app's folder, the app's own data that
    /// is not a bottle, the bottles folder itself, one that climbs out with `..`, and a folder
    /// of the temp dir.
    fn foreign_prefixes() -> Vec<PathBuf> {
        vec![
            home().join("Library/Application Support/Otra app/Bottles/Rockstar Games Launcher"),
            data_dir().join("Wine"),
            data_dir().join("config.json"),
            own_bottles_dir(),
            own_bottles_dir().join("../Wine"),
            home().join(".wine"),
            std::env::temp_dir().join(format!("bl-foreign-{}", std::process::id())),
        ]
    }

    /// No command built for a prefix outside the app's own bottles may start Wine, whatever the
    /// window allowed: it is the last line of defence, so no Wine writes into a folder that
    /// belongs to something else. Bottles of the app start Wine as ever.
    #[test]
    fn wine_never_starts_outside_the_apps_own_bottles() {
        let wine = Path::new("/opt/wine/bin/wine");
        for prefix in foreign_prefixes() {
            let theirs = bottle_at(prefix.clone());
            let exe = prefix.join("drive_c/Launcher.exe");
            let commands = [
                bottle_cmd(wine, &theirs),
                open_command(wine, &theirs, &exe, "", &[]),
                wine_cmd(wine, &prefix, true),
                terminal_command(wine, &theirs),
                Tool::Registry.command(wine, &theirs),
            ];
            for c in commands {
                assert_eq!(c.get_program(), "/usr/bin/false", "{}", prefix.display());
            }
            assert!(!is_own_prefix(&prefix), "{}", prefix.display());
        }
        let mine = bottle_at(own_bottles_dir().join("Rockstar Games Launcher (copia)"));
        let exe = mine.path.join("drive_c/Launcher.exe");
        assert_eq!(bottle_cmd(wine, &mine).get_program(), wine.as_os_str());
        assert_eq!(
            open_command(wine, &mine, &exe, "", &[]).get_program(),
            wine.as_os_str()
        );
        assert_eq!(
            wine_cmd(wine, &mine.path, true).get_program(),
            wine.as_os_str()
        );
        assert_eq!(
            Tool::Registry.command(wine, &mine).get_program(),
            wine.as_os_str()
        );
    }

    /// The wineserver of a prefix is the same case: asked about, killed or waited for only in a
    /// bottle of the app, and a foreign prefix is never "running" nor gets a server command.
    #[test]
    fn wineserver_is_only_asked_in_the_apps_own_bottles() {
        let base = std::env::temp_dir().join(format!("bl-ws-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("bin")).unwrap();
        let wine = base.join("bin/wine");
        fs::write(&wine, b"#!/bin/sh").unwrap();
        fs::write(base.join("bin/wineserver"), b"#!/bin/sh").unwrap();
        for prefix in foreign_prefixes() {
            assert!(
                wineserver_cmd(&wine, &prefix).is_none(),
                "{}",
                prefix.display()
            );
            assert!(!is_running_at(&wine, &prefix));
            assert!(!is_running(&wine, &bottle_at(prefix)));
        }
        let own = own_bottles_dir().join("Steam");
        let c = wineserver_cmd(&wine, &own).expect("a server for an own bottle");
        assert_eq!(c.get_program(), base.join("bin/wineserver").as_os_str());
        let _ = fs::remove_dir_all(&base);
    }

    /// A bottle is only made inside the app's bottles folder: anywhere else nothing is created.
    #[test]
    fn a_bottle_is_not_made_outside_the_apps_bottles() {
        let elsewhere = std::env::temp_dir().join(format!("bl-nomake-{}", std::process::id()));
        let _ = fs::remove_dir_all(&elsewhere);
        let err = create_bottle(
            Path::new("/opt/wine/bin/wine"),
            &elsewhere,
            "win10",
            &Log::default(),
        )
        .unwrap_err();
        assert_eq!(err, NOT_AN_OWN_BOTTLE);
        assert!(!elsewhere.exists());
    }

    /// Run by hand (it downloads about 350 MB into the app's own folder):
    /// `cargo test -- --ignored --nocapture metal_engine_installs_for_real`
    #[test]
    #[ignore = "downloads the real DXMT engine"]
    fn metal_engine_installs_for_real() {
        use crate::wine_setup::{install, Progress, Shared};
        let progress: Shared = Arc::new(Mutex::new(Progress {
            running: true,
            ..Default::default()
        }));
        let state = install(progress.clone(), Log::default(), Engine::Metal);
        assert!(
            state.path().is_some(),
            "{:?}",
            progress.lock().unwrap().error
        );
        assert_eq!(Engine::Metal.installed(), state.path());
    }

    /// Run by hand, on a copy of a bottle that has the launcher installed:
    /// `BOTELLA_PRUEBA=<bottle> cargo test -- --ignored --nocapture rockstar_launcher_opens_for_real`
    /// Starts Launcher.exe with the very command the app builds and passes only when the launcher
    /// reports its window active and the bottle is still running. The first Launcher.exe hands
    /// over to a second one (`-upgrade`) and leaves with code 0; code 1 was the failure.
    /// `BOTELLA_ESPERA=<seconds>` keeps the window up before closing the bottle, to look at it.
    #[test]
    #[ignore = "opens the real Rockstar Games Launcher"]
    fn rockstar_launcher_opens_for_real() {
        use std::time::{Duration, SystemTime};
        let prefix = PathBuf::from(std::env::var_os("BOTELLA_PRUEBA").expect("BOTELLA_PRUEBA"));
        let wine = Engine::Metal
            .installed()
            .expect("the DXMT engine is not installed");
        let b = bottle_at(prefix.clone());
        let exe = prefix.join("drive_c/Program Files/Rockstar Games/Launcher/Launcher.exe");
        let log = fs::read_dir(prefix.join("drive_c/users"))
            .unwrap()
            .flatten()
            .map(|u| {
                u.path()
                    .join("Documents/Rockstar Games/Launcher/launcher.log")
            })
            .find(|f| f.is_file())
            .expect("launcher.log");
        let started = SystemTime::now();
        let mut child = open_command(&wine, &b, &exe, "", &[])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let fresh = || {
            let written = fs::metadata(&log)
                .and_then(|m| m.modified())
                .map(|t| t >= started)
                .unwrap_or(false);
            if written {
                String::from_utf8_lossy(&fs::read(&log).unwrap_or_default()).to_string()
            } else {
                String::new()
            }
        };
        let active = (0..120).any(|_| {
            std::thread::sleep(Duration::from_secs(1));
            fresh().contains("SetState: WaitForWindow -> Active")
        });
        let hold = std::env::var("BOTELLA_ESPERA")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);
        std::thread::sleep(Duration::from_secs(hold));
        let text = fresh();
        let first = child.try_wait().unwrap();
        let service = is_running(&wine, &b);
        if let Some(ws) = wineserver_for(&wine) {
            let _ = Command::new(ws)
                .env("WINEPREFIX", &prefix)
                .arg("-k")
                .status();
        }
        let _ = child.wait();
        assert!(active, "the launcher never reported its window active");
        assert!(
            first.map(|s| s.success()).unwrap_or(true),
            "Launcher.exe failed: {first:?}"
        );
        assert!(service, "nothing was left running in the bottle");
        assert_eq!(crate::diagnostics::rockstar_ui_stalled(&text), None);
        assert!(!text.contains("Setting error condition"), "{text}");
    }

    /// Run by hand, on a copy of a bottle that has Steam installed:
    /// `BOTELLA_PRUEBA=<bottle> cargo test -- --ignored --nocapture steam_window_opens_for_real`
    /// Starts Steam on the DXMT engine with the very command the app builds and passes only when
    /// Steam announces a visible window whose web helper kept its GPU (the no-GPU start is the
    /// one that draws nothing). `BOTELLA_ESPERA=<seconds>` keeps the window up, to look at it.
    #[test]
    #[ignore = "starts the real Steam on the DXMT engine"]
    fn steam_window_opens_for_real() {
        use std::time::Duration;
        let prefix = PathBuf::from(std::env::var_os("BOTELLA_PRUEBA").expect("BOTELLA_PRUEBA"));
        let wine = Engine::Metal
            .installed()
            .expect("the DXMT engine is not installed");
        let b = bottle_at(prefix.clone());
        let exe = steam_exe(&prefix).expect("steam.exe");
        let log = exe.parent().unwrap().join("logs/webhelper.txt");
        let read = || String::from_utf8_lossy(&fs::read(&log).unwrap_or_default()).to_string();
        let before = read().len();
        let mut child = open_command(&wine, &b, &exe, "", &[])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let fresh = || read().get(before..).unwrap_or_default().to_string();
        let shown = (0..240).any(|_| {
            std::thread::sleep(Duration::from_secs(1));
            fresh().contains("WasHidden 0")
        });
        let hold = std::env::var("BOTELLA_ESPERA")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);
        std::thread::sleep(Duration::from_secs(hold));
        let text = fresh();
        let mut stop = bottle_cmd(&wine, &b);
        let _ = stop.arg(&exe).arg("-shutdown").status();
        std::thread::sleep(Duration::from_secs(6));
        if let Some(ws) = wineserver_for(&wine) {
            let _ = Command::new(ws)
                .env("WINEPREFIX", &prefix)
                .arg("-k")
                .status();
        }
        let _ = child.wait();
        let start = text
            .lines()
            .rfind(|l| l.contains("webhelper launched"))
            .unwrap_or_default();
        assert!(shown, "Steam never announced a visible window");
        assert!(!start.is_empty(), "the web helper never started");
        assert!(!start.contains("--disable-gpu "), "{start}");
        assert!(!text.contains("Disabling GPU acceleration"), "{text}");
    }

    /// Wine's diagnostics channel and Direct2D's warnings stay visible: the graphics diagnosis
    /// reads them.
    #[test]
    fn graphics_failures_are_not_silenced() {
        assert!(DEFAULT_WINEDEBUG.starts_with("fixme-all,"));
        assert!(DEFAULT_WINEDEBUG.contains("fixme+winediag"));
        assert!(DEFAULT_WINEDEBUG.contains("warn+d2d"));
    }

    #[test]
    fn copies_are_complete_and_checked() {
        let base = std::env::temp_dir().join(format!("bl-copy-{}", std::process::id()));
        let src = base.join("Orig Botella");
        fs::create_dir_all(src.join("drive_c/Program Files/App con espacios")).unwrap();
        fs::write(src.join("system.reg"), "WINE REGISTRY Version 2\n").unwrap();
        fs::write(src.join("user.reg"), "WINE REGISTRY Version 2\n#user\n").unwrap();
        fs::write(
            src.join("drive_c/Program Files/App con espacios/a.exe"),
            b"MZ",
        )
        .unwrap();
        let log: Log = Default::default();
        let dest = base.join("Copia");
        clone_bottle(&src, &dest, &log).unwrap();
        assert!(dest
            .join("drive_c/Program Files/App con espacios/a.exe")
            .is_file());
        // Copying onto an existing bottle is refused.
        assert!(clone_bottle(&src, &dest, &log).is_err());
        // A copy whose registry differs is caught.
        fs::write(dest.join("user.reg"), "x").unwrap();
        assert!(verify_copy(&src, &dest).is_err());
        let _ = fs::remove_dir_all(&base);
    }
}
