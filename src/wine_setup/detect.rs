//! Finding a free Wine on this Mac and checking that it really runs.

use super::*;
use crate::backend::{home, io_error_logged, Config, Log};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub enum WineState {
    /// Not looked yet.
    Unknown,
    Ready {
        path: PathBuf,
        version: String,
    },
    /// A Wine is there but doesn't start (e.g. Rosetta missing or the file is blocked).
    Broken {
        path: PathBuf,
        reason: String,
    },
    Missing,
}

impl WineState {
    pub fn path(&self) -> Option<PathBuf> {
        match self {
            WineState::Ready { path, .. } => Some(path.clone()),
            _ => None,
        }
    }
}

/// The apps whose bundled Wine is a free build this app supports: WineHQ's builds and Apple's
/// Game Porting Toolkit. A Wine inside any other app is never used.
const FREE_WINE_APPS: [&str; 4] = [
    "Wine Stable",
    "Wine Staging",
    "Wine Devel",
    "Game Porting Toolkit",
];

/// The only places the app looks for a Wine in: the app's own folder, the free apps above in the
/// folders apps are installed in, Homebrew's and MacPorts' folders, Whisky's copy and the
/// folders of PATH. A Wine bundled in some other app is never one of them.
struct Places {
    managed: PathBuf,
    app_roots: Vec<PathBuf>,
    bins: Vec<PathBuf>,
    whisky: PathBuf,
    path_dirs: Vec<PathBuf>,
    /// Under test only: the folder that holds the stand-in Wine of the flow tests, which is no
    /// free Wine but is what a test is told to run. Nothing is ever searched for there.
    stand_ins: Vec<PathBuf>,
}

impl Places {
    /// The places on this Mac. Under test only the person's own (throwaway) home is looked at:
    /// the system folders are absolute and ignore HOME, so a Wine installed on the developer's
    /// Mac would be found and run by tests that expect no Wine; PATH is skipped for the same
    /// reason.
    fn current() -> Self {
        let mut app_roots = vec![home().join("Applications")];
        let mut bins = vec![];
        let mut path_dirs = vec![];
        let mut stand_ins = vec![];
        if cfg!(test) {
            stand_ins.push(home().join("wine-falso"));
        } else {
            app_roots.insert(0, PathBuf::from("/Applications"));
            bins = ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"]
                .map(PathBuf::from)
                .to_vec();
            path_dirs = std::env::var_os("PATH")
                .map(|paths| std::env::split_paths(&paths).collect())
                .unwrap_or_default();
        }
        Places {
            managed: managed_dir(),
            app_roots,
            bins,
            whisky: home().join(
                "Library/Application Support/com.isaacmarovitz.Whisky/Libraries/Wine/bin/wine64",
            ),
            path_dirs,
            stand_ins,
        }
    }

    /// Whether `wine` is in one of the places, and what it really is, once links are followed,
    /// is no Wine of an unknown app either: a link in Homebrew's folder or on PATH may lead
    /// into any app.
    fn allow(&self, wine: &Path) -> bool {
        self.in_a_place(wine) && self.resolves_to_free(wine)
    }

    /// Whether the path as written is in one of the places. A Wine inside an app bundle counts
    /// only when every bundle around it is one of the free apps, sitting directly in a folder of
    /// apps.
    fn in_a_place(&self, wine: &Path) -> bool {
        if wine.starts_with(&self.managed) || self.stand_ins.iter().any(|d| wine.starts_with(d)) {
            return true;
        }
        if inside_an_app(wine) {
            return self.bundles_are_free(wine);
        }
        let dir = wine.parent();
        wine == self.whisky
            || self
                .bins
                .iter()
                .chain(&self.path_dirs)
                .any(|d| Some(d.as_path()) == dir)
    }

    /// Whether every app bundle around `wine` is one of the free apps, sitting directly in a
    /// folder of apps (as written or with its links followed).
    fn bundles_are_free(&self, wine: &Path) -> bool {
        wine.ancestors().filter(|a| is_app_dir(a)).all(|app| {
            app.parent().is_some_and(|dir| {
                self.app_roots
                    .iter()
                    .any(|r| r == dir || std::fs::canonicalize(r).is_ok_and(|r| r == dir))
            }) && app
                .file_stem()
                .is_some_and(|n| FREE_WINE_APPS.iter().any(|f| n == *f))
        })
    }

    /// Whether `wine`, with its links followed, is not inside an app that is not a free one.
    /// A path that cannot be resolved (it is not there) is judged as written.
    fn resolves_to_free(&self, wine: &Path) -> bool {
        match std::fs::canonicalize(wine) {
            Ok(real) if real != wine && inside_an_app(&real) => self.bundles_are_free(&real),
            _ => true,
        }
    }

    /// Whether `wine`, which the person picked by hand or the settings remember from an earlier
    /// pick, may be used: it is in one of the places, or it is a Wine of the person's own
    /// somewhere else. What is never allowed, whoever asks, is a Wine inside an app that is not
    /// one of the free ones, also through a link.
    fn allow_chosen(&self, wine: &Path) -> bool {
        self.resolves_to_free(wine) && (self.in_a_place(wine) || !inside_an_app(wine))
    }
}

/// Whether `path` is an app bundle folder (its name ends in `.app`, in any letter case, as the
/// Mac's usual disks do not tell them apart).
fn is_app_dir(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("app"))
}

/// Whether `path` lies inside an app bundle.
fn inside_an_app(path: &Path) -> bool {
    path.ancestors().any(is_app_dir)
}

/// Whether `wine`, a path the person picked by hand or the settings remember, may be used (see
/// [`Places::allow_chosen`]): never a Wine bundled inside an app that is not a free one.
pub fn is_free_wine(wine: &Path) -> bool {
    Places::current().allow_chosen(wine)
}

/// Every place a free Wine usually lives, the app's own copy first, and the path the settings
/// remember when it may still be used.
pub fn candidates(cfg: &Config) -> Vec<PathBuf> {
    let places = Places::current();
    let mut v: Vec<PathBuf> = cfg
        .wine_path
        .iter()
        .map(PathBuf::from)
        .filter(|p| places.allow_chosen(p))
        .collect();
    let mut found = vec![];
    // The app's own engines; the standard one is the default for everything.
    for e in [Engine::Standard, Engine::Games] {
        found.extend(wine_in_app(&places.managed.join(e.app_name())));
    }
    for root in &places.app_roots {
        for app in FREE_WINE_APPS {
            found.extend(wine_in_app(&root.join(format!("{app}.app"))));
        }
    }
    for dir in &places.bins {
        found.push(dir.join("wine"));
        found.push(dir.join("wine64"));
    }
    // Whisky's free Wine, if the person used it before.
    found.push(places.whisky.clone());
    found.extend(places.path_dirs.iter().map(|dir| dir.join("wine")));
    // Only what the places allow: a folder of PATH may be inside any app.
    found.retain(|p| places.allow(p));
    v.extend(found);
    v.dedup();
    v
}

/// The line that points to the log when the reason itself says too little to act on.
const DETAILS_IN_LOG: &str = "Los detalles están en el registro de actividad (Ajustes).";

/// Why a Wine that was started gave no version, in plain Spanish. `raw` is what it printed on its
/// error output: it is English and technical, so it only goes to the log (see [`probe`]); when
/// the reason cannot say more than "it does not start", it points to the log instead.
fn failure_reason(raw: &str, exited: bool) -> String {
    let low = raw.to_lowercase();
    if low.contains("bad cpu type") || raw.contains("Rosetta") {
        "Falta Rosetta".into()
    } else if low.contains("operation not permitted") || low.contains("permission denied") {
        // The words alone do not say which file was refused, so the cause is only likely.
        "Puede que macOS no lo deje abrir".into()
    } else if low.contains("library not loaded") || low.contains("image not found") {
        "Le falta una librería del sistema".into()
    } else if raw.is_empty() && exited {
        "Se cierra nada más abrirlo".into()
    } else {
        format!("No arranca en este Mac. {DETAILS_IN_LOG}")
    }
}

/// Keeps what a Wine that did not start printed in the activity log, where it belongs: the
/// person sees the reason in plain words, and the raw text is there for whoever helps them. It
/// goes to a log of its own, so it reaches the log file but never adds a line to the window.
fn log_probe_failure(wine: &Path, raw: &str) {
    if raw.is_empty() {
        return;
    }
    let log = Log::default();
    crate::backend::push_tool_log(&log, format!("✗ {} no arrancó: {raw}", wine.display()));
}

/// Runs `wine --version` with a time limit; the version text when it works, otherwise the
/// reason in plain Spanish (the raw error text goes to the activity log only). For a caller with
/// no log of its own (opening a file with no window, the `--medir` mode): what it writes still
/// reaches the log file, only no window shows it.
pub fn probe(wine: &Path) -> Result<String, String> {
    probe_logged(wine, &Log::default())
}

/// [`probe`] writing the detail of a disk error behind the plain sentence to `log`. The raw text
/// of a Wine that did not start still goes only to the log file, as it always did.
pub fn probe_logged(wine: &Path, log: &Log) -> Result<String, String> {
    let mut child = Command::new(wine)
        .arg("--version")
        .env("WINEDEBUG", "-all")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| io_error_logged(&e, log, "Arrancar Wine", wine))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = child
                    .wait_with_output()
                    .map_err(|e| io_error_logged(&e, log, "Leer lo que dijo Wine", wine))?;
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                return if status.success() && !text.is_empty() {
                    Ok(text)
                } else {
                    log_probe_failure(wine, &format!("{status}: {err}"));
                    Err(failure_reason(&err, true))
                };
            }
            Ok(None) if start.elapsed() > Duration::from_secs(20) => {
                let _ = child.kill();
                let _ = child.wait();
                log_probe_failure(wine, "no respondió en 20 segundos");
                return Err("No respondió".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(io_error_logged(&e, log, "Esperar a Wine", wine)),
        }
    }
}

/// Looks for a Wine that really starts. What a Wine that does not start leaves to say goes to `log`.
pub fn detect(cfg: &Config, log: &Log) -> WineState {
    let mut broken = None;
    for c in candidates(cfg) {
        if !c.is_file() {
            continue;
        }
        match probe_logged(&c, log) {
            Ok(version) => return WineState::Ready { path: c, version },
            Err(reason) => {
                if broken.is_none() {
                    broken = Some(WineState::Broken { path: c, reason });
                }
            }
        }
    }
    broken.unwrap_or(WineState::Missing)
}

/// Apple chips run these Wine builds through Rosetta.
pub fn needs_rosetta() -> bool {
    let arm = Command::new("/usr/bin/uname")
        .arg("-m")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("arm64"))
        .unwrap_or(false);
    if !arm {
        return false;
    }
    !Command::new("/usr/bin/arch")
        .args(["-x86_64", "/usr/bin/true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Whether this Mac lacks Rosetta, but only when the app could really tell: `None` on a Mac
/// that is not an Apple chip (it does not need Rosetta at all) and when the check itself could
/// not run. Unlike [`needs_rosetta`], a failed check is not taken for a missing Rosetta.
pub fn rosetta_missing_checked() -> Option<bool> {
    let arm = Command::new("/usr/bin/uname")
        .arg("-m")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("arm64"))?;
    if !arm {
        return None;
    }
    let status = Command::new("/usr/bin/arch")
        .args(["-x86_64", "/usr/bin/true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?;
    Some(!status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in `wine` file in its own temp folder; the folder goes away with the value, so
    /// runs do not pile folders up in the temp dir.
    struct StandIn(PathBuf);

    impl std::ops::Deref for StandIn {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for StandIn {
        fn drop(&mut self) {
            if let Some(dir) = self.0.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }

    fn stand_in(name: &str, script: &str) -> StandIn {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("bl-probe-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("wine");
        std::fs::write(&file, script).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        StandIn(file)
    }

    /// The stand-in's folder is removed when it goes out of scope.
    #[test]
    fn a_stand_in_leaves_no_folder_behind() {
        let wine = stand_in("leak", "#!/bin/sh\necho wine-11.0\n");
        let dir = wine.parent().unwrap().to_path_buf();
        assert!(dir.is_dir());
        drop(wine);
        assert!(!dir.exists());
    }

    /// Tests never look for a Wine outside the (throwaway) home: the absolute system folders and
    /// PATH would find the developer's own Wine and run it.
    #[test]
    fn candidates_stay_inside_the_home_under_test() {
        let cfg = Config::default();
        let home = home();
        let managed = managed_dir();
        for c in candidates(&cfg) {
            assert!(
                c.starts_with(&home) || c.starts_with(&managed),
                "a candidate outside the home: {}",
                c.display()
            );
        }
    }

    fn places() -> Places {
        Places {
            managed: PathBuf::from("/h/Library/Application Support/Botella Libre/Wine"),
            app_roots: vec![
                PathBuf::from("/Applications"),
                PathBuf::from("/h/Applications"),
            ],
            bins: ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"]
                .map(PathBuf::from)
                .to_vec(),
            whisky: PathBuf::from(
                "/h/Library/Application Support/Whisky/Libraries/Wine/bin/wine64",
            ),
            path_dirs: vec![PathBuf::from("/Users/x/tools/bin")],
            stand_ins: vec![],
        }
    }

    /// Only the free Wines the app supports are taken: its own folder, WineHQ's and Apple's
    /// apps, Homebrew, MacPorts, Whisky and the folders of PATH.
    #[test]
    fn the_free_wines_the_app_supports_are_allowed() {
        let p = places();
        for ok in [
            "/h/Library/Application Support/Botella Libre/Wine/Wine Stable.app/Contents/Resources/wine/bin/wine",
            "/h/Library/Application Support/Botella Libre/Wine/Motor DXMT/engine/bin/wine",
            "/Applications/Wine Stable.app/Contents/Resources/wine/bin/wine64",
            "/Applications/Wine Devel.app/Contents/Resources/wine/bin/wine",
            "/h/Applications/Wine Staging.app/Contents/Resources/wine/bin/wine",
            "/Applications/Game Porting Toolkit.app/Contents/Resources/wine/bin/wine64",
            "/opt/homebrew/bin/wine",
            "/usr/local/bin/wine64",
            "/opt/local/bin/wine",
            "/h/Library/Application Support/Whisky/Libraries/Wine/bin/wine64",
            "/Users/x/tools/bin/wine",
        ] {
            assert!(p.allow(Path::new(ok)), "{ok}");
        }
    }

    /// A Wine bundled inside an app the app does not know is never picked, wherever the path
    /// came from, and nothing outside the places is either.
    #[test]
    fn a_wine_inside_an_unknown_app_is_never_allowed() {
        let p = places();
        for bad in [
            "/Applications/Some Paid App.app/Contents/SharedSupport/bin/wine",
            "/Applications/Some Paid App.app/Contents/Resources/wine/bin/wine64",
            "/h/Applications/Another Product.app/Contents/Resources/wine/bin/wine",
            // A free app's name in a folder that is not one of apps, or nested in another app.
            "/tmp/Wine Stable.app/Contents/Resources/wine/bin/wine",
            "/Applications/Some Paid App.app/Contents/Wine Stable.app/Contents/Resources/wine/bin/wine",
            // A PATH entry inside an unknown app is still inside that app.
            "/Applications/Some Paid App.app/Contents/bin/wine",
            // Not in any of the places.
            "/opt/elsewhere/bin/wine",
            "/Library/Application Support/Some Product/bin/wine",
            "/Users/x/tools/bin/other/wine",
        ] {
            assert!(!p.allow(Path::new(bad)), "{bad}");
        }
        let mut p = places();
        p.path_dirs.push(PathBuf::from(
            "/Applications/Some Paid App.app/Contents/bin",
        ));
        assert!(!p.allow(Path::new(
            "/Applications/Some Paid App.app/Contents/bin/wine"
        )));
    }

    /// A Wine the person picked by hand may be anywhere of their own, but never inside an app
    /// that is not one of the free ones.
    #[test]
    fn a_chosen_wine_may_be_anywhere_except_inside_an_unknown_app() {
        let p = places();
        for ok in [
            "/opt/elsewhere/bin/wine",
            "/Users/x/mi-wine/bin/wine64",
            "/Applications/Wine Stable.app/Contents/Resources/wine/bin/wine",
            "/opt/homebrew/bin/wine",
        ] {
            assert!(p.allow_chosen(Path::new(ok)), "{ok}");
        }
        for bad in [
            "/Applications/Some Paid App.app/Contents/SharedSupport/bin/wine",
            "/Users/x/Downloads/Some Paid App.app/Contents/Resources/wine/bin/wine",
            "/tmp/Wine Stable.app/Contents/Resources/wine/bin/wine",
        ] {
            assert!(!p.allow_chosen(Path::new(bad)), "{bad}");
        }
    }

    /// A link in a folder of the places (Homebrew's, one of PATH) or in the person's own folders
    /// is judged by what it leads to: into an unknown app it is refused, into a free app it is
    /// fine. An app folder written in other letter case is still an app.
    #[test]
    fn a_link_into_an_unknown_apps_wine_is_never_allowed() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!("bl-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let apps = root.join("apps");
        let bin = root.join("bin");
        let mine = root.join("mine");
        let wine_in = |app: &str| {
            let file = apps.join(app).join("Contents/Resources/wine/bin/wine");
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, "").unwrap();
            file
        };
        let paid = wine_in("Some Paid App.app");
        let loud = wine_in("Some Paid App.APP");
        let free = wine_in("Wine Stable.app");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&mine).unwrap();
        symlink(&paid, bin.join("wine")).unwrap();
        symlink(&free, bin.join("wine64")).unwrap();
        symlink(&paid, mine.join("wine")).unwrap();
        symlink(&free, mine.join("wine64")).unwrap();
        let p = Places {
            app_roots: vec![apps.clone()],
            path_dirs: vec![bin.clone()],
            ..places()
        };
        assert!(!p.allow(&bin.join("wine")));
        assert!(p.allow(&bin.join("wine64")));
        assert!(!p.allow_chosen(&mine.join("wine")));
        assert!(p.allow_chosen(&mine.join("wine64")));
        assert!(!p.allow_chosen(&paid) && !p.allow_chosen(&loud));
        assert!(p.allow_chosen(&free));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A remembered path in the settings goes through the same check, and what the search tries
    /// never holds a Wine of an unknown app.
    #[test]
    fn candidates_drop_the_settings_path_of_an_unknown_app() {
        let cfg = Config {
            wine_path: Some(
                "/Applications/Some Paid App.app/Contents/SharedSupport/bin/wine".into(),
            ),
            ..Config::default()
        };
        let found = candidates(&cfg);
        assert!(
            found
                .iter()
                .all(|c| !c.to_string_lossy().contains("Some Paid App")),
            "{found:?}"
        );
        assert!(!is_free_wine(Path::new(
            "/Applications/Some Paid App.app/Contents/SharedSupport/bin/wine"
        )));
        // The app's own copy is still the first candidate.
        let own = managed_dir().join("Wine Stable.app/Contents/Resources/wine/bin/wine");
        let cfg = Config {
            wine_path: Some(own.to_string_lossy().to_string()),
            ..Config::default()
        };
        assert_eq!(candidates(&cfg).first(), Some(&own));
    }

    /// A Wine that fails says why in plain Spanish: what it printed is English and stays in the
    /// log, and the person is pointed to the log when the reason says too little.
    #[test]
    fn a_broken_wine_is_explained_in_spanish() {
        let wine = stand_in(
            "lib",
            "#!/bin/sh\necho 'dyld[1]: Library not loaded: @rpath/libX.dylib' >&2\nexit 1\n",
        );
        let reason = probe(&wine).unwrap_err();
        assert_eq!(reason, "Le falta una librería del sistema");
        let wine = stand_in(
            "other",
            "#!/bin/sh\necho 'wine: cannot find the thing' >&2\nexit 3\n",
        );
        let reason = probe(&wine).unwrap_err();
        assert!(reason.starts_with("No arranca en este Mac."), "{reason}");
        assert!(reason.contains("registro de actividad (Ajustes)"));
        assert!(!reason.contains("cannot find"), "{reason}");
        // It ends without saying anything, and without a version.
        let wine = stand_in("silent", "#!/bin/sh\nexit 1\n");
        assert_eq!(probe(&wine).unwrap_err(), "Se cierra nada más abrirlo");
        let wine = stand_in(
            "rosetta",
            "#!/bin/sh\necho 'Bad CPU type in executable' >&2\nexit 1\n",
        );
        assert_eq!(probe(&wine).unwrap_err(), "Falta Rosetta");
        // A refusal that names some file may be about anything: the cause is not stated as fact.
        let wine = stand_in(
            "denied",
            "#!/bin/sh\necho 'cannot write /tmp/x.log: Permission denied' >&2\nexit 1\n",
        );
        let reason = probe(&wine).unwrap_err();
        assert!(reason.starts_with("Puede que"), "{reason}");
        assert!(!reason.contains("Permission"), "{reason}");
    }

    /// A file that cannot be run is explained like any other error from the system.
    #[test]
    fn a_wine_that_cannot_be_run_has_no_system_text() {
        let wine = stand_in("noexec", "not a program");
        let dir = wine.parent().unwrap().to_path_buf();
        let _ = std::fs::remove_file(&*wine);
        let reason = probe(&dir.join("wine")).unwrap_err();
        assert_eq!(reason, "No encuentro el archivo o la carpeta.");
        assert!(!reason.contains("os error"));
    }

    /// A working Wine gives its version.
    #[test]
    fn a_working_wine_gives_its_version() {
        let wine = stand_in("ok", "#!/bin/sh\necho wine-11.0\n");
        assert_eq!(probe(&wine).unwrap(), "wine-11.0");
    }
}
