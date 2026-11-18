//! Making Windows programs feel like Mac apps:
//!
//! * every installed program gets its own small Mac app (each pointing at the shortcut it
//!   opens), so it opens from Launchpad, Spotlight or the Dock with no window in between;
//! * a Windows program double-clicked in Finder opens straight away (a small viewer app
//!   declares itself the handler of `com.microsoft.windows-executable`, `.lnk` and discs in its
//!   Info.plist).

use crate::backend::{
    dropped_names_note, home, is_shell_env_name, open_args, program_env, sh_quote, split_shell_env,
    Bottle, Program,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Marker key written into every launcher this app makes, so it never touches anyone else's apps.
const MARK: &str = "BotellaLibreLauncher";

pub fn root() -> PathBuf {
    home().join("Applications/Botella Libre")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn clean_file_name(s: &str) -> String {
    s.chars()
        .map(|c| if c == '/' || c == ':' { '-' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

/// The script inside a launcher: the same environment and arguments the app itself would use,
/// plus the names of the variables left out because they could not be written into a shell
/// script safely (a crafted bottle or program option can name one `A;cmd`).
fn launcher_script(
    wine: &Path,
    b: &Bottle,
    file: &Path,
    args: &str,
    extra_env: &[String],
) -> (String, Vec<String>) {
    let (env, removed) = program_env(b, extra_env);
    let mut env = env;
    crate::backend::adapt_env_to_wine(wine, &mut env);
    let (env, mut dropped) = split_shell_env(env);
    let (argv, dir) = open_args(&b.path, file, args);
    let mut s = String::from(
        "#!/bin/sh\n# Made by Botella Libre: opens a Windows program with free Wine.\n",
    );
    for (k, v) in env {
        s.push_str(&format!("export {k}={}\n", sh_quote(&v)));
    }
    for r in removed {
        if is_shell_env_name(&r) {
            s.push_str(&format!("unset {r}\n"));
        } else {
            dropped.push(r);
        }
    }
    s.push_str(&format!(
        "cd {} 2>/dev/null || cd {}\n",
        sh_quote(&dir.unwrap_or(b.path.clone()).to_string_lossy()),
        sh_quote(&b.path.to_string_lossy())
    ));
    s.push_str(&format!("exec {}", sh_quote(&wine.to_string_lossy())));
    for a in argv {
        s.push(' ');
        s.push_str(&sh_quote(&a));
    }
    s.push('\n');
    (s, dropped)
}

fn info_plist(name: &str, id: &str, bottle: &str, target: &str, has_icon: bool) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>{name}</string>
  <key>CFBundleDisplayName</key><string>{name}</string>
  <key>CFBundleIdentifier</key><string>{id}</string>
  <key>CFBundleExecutable</key><string>run</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>1.0</string>
  {icon}<key>LSUIElement</key><true/>
  <key>{MARK}</key><true/>
  <key>BotellaLibreBottle</key><string>{bottle}</string>
  <key>BotellaLibreTarget</key><string>{target}</string>
</dict>
</plist>
"#,
        name = xml_escape(name),
        id = xml_escape(id),
        bottle = xml_escape(bottle),
        target = xml_escape(target),
        icon = if has_icon {
            "<key>CFBundleIconFile</key><string>icon</string>\n  "
        } else {
            ""
        },
    )
}

fn stable_id(s: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn is_ours(app: &Path) -> bool {
    fs::read_to_string(app.join("Contents/Info.plist"))
        .map(|s| s.contains(MARK))
        .unwrap_or(false)
}

/// Everything one launcher is written from. `name` is the Mac app's name, which may differ from
/// the program's own when two programs of a bottle share one; the rest is the program as the
/// app itself would open it.
struct LauncherSpec<'a> {
    /// The folder all launchers live in.
    root: &'a Path,
    /// The Wine the bottle runs on.
    wine: &'a Path,
    bottle: &'a Bottle,
    name: &'a str,
    /// The Windows file the launcher opens.
    file: &'a Path,
    args: &'a str,
    /// The program's own `NAME=value` options, on top of the bottle's.
    env: &'a [String],
    /// A picture to turn into the app's icon.
    icon: Option<&'a Path>,
}

/// Writes one launcher; only touches files whose content changed. Returns its path.
fn write_launcher(spec: &LauncherSpec, notes: &mut Vec<String>) -> Option<PathBuf> {
    let LauncherSpec {
        root,
        wine,
        bottle: b,
        name,
        file,
        args,
        env,
        icon,
    } = *spec;
    let app = root
        .join(clean_file_name(&b.name))
        .join(format!("{}.app", clean_file_name(name)));
    if app.exists() && !is_ours(&app) {
        return None; // someone else's app with that name: leave it alone
    }
    let macos = app.join("Contents/MacOS");
    let res = app.join("Contents/Resources");
    fs::create_dir_all(&macos).ok()?;
    fs::create_dir_all(&res).ok()?;
    let icns = res.join("icon.icns");
    if let Some(png) = icon {
        if !icns.is_file() {
            let _ = Command::new("/usr/bin/sips")
                .args(["-s", "format", "icns"])
                .arg(png)
                .arg("--out")
                .arg(&icns)
                .output();
        }
    }
    let id = format!(
        "com.botellalibre.app.{}",
        stable_id(&format!("{}|{}", b.path.display(), file.display()))
    );
    let plist = info_plist(name, &id, &b.name, &file.to_string_lossy(), icns.is_file());
    let (script, dropped) = launcher_script(wine, b, file, args, env);
    let run = macos.join("run");
    if fs::read_to_string(app.join("Contents/Info.plist"))
        .ok()
        .as_deref()
        != Some(&plist)
    {
        fs::write(app.join("Contents/Info.plist"), plist).ok()?;
    }
    if fs::read_to_string(&run).ok().as_deref() != Some(&script) {
        fs::write(&run, script).ok()?;
        // Said once, when the script that leaves the names out is written, not on every sync.
        if !dropped.is_empty() {
            notes.push(dropped_names_note(
                &format!("el lanzador de «{name}» (botella «{}»)", b.name),
                &dropped,
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&run, fs::Permissions::from_mode(0o755));
        }
    }
    Some(app)
}

/// The longest place a launcher's name carries. A program told apart by its file has a long
/// subtitle on screen, but a Mac file name is limited to 255 bytes and a name too long makes the
/// app fail to be written at all.
const PLACE_CHARS: usize = 40;

/// The end of a place (where the file is, the part that tells programs apart), cut to fit in a
/// file name. The number added after it keeps names that still collide apart.
fn short_place(place: &str) -> String {
    let count = place.chars().count();
    if count <= PLACE_CHARS {
        return place.to_string();
    }
    let tail: String = place.chars().skip(count - PLACE_CHARS).collect();
    tail.trim_start_matches(['/', ' ', '·']).to_string()
}

/// The name of a program's Mac app. Two programs of one bottle with the same name get the
/// place they come from added, and a number if even that is not enough, so that each keeps
/// an app of its own. `taken` holds the names already used in this bottle.
fn launcher_name(
    name: &str,
    subtitle: &str,
    taken: &mut std::collections::HashSet<String>,
) -> String {
    // Mac file names ignore case, so names that differ only in case are the same here.
    let key = |n: &str| clean_file_name(n).to_lowercase();
    let mut candidate = name.to_string();
    if taken.contains(&key(&candidate)) && !subtitle.is_empty() {
        candidate = format!("{name} ({})", short_place(subtitle));
    }
    let base = candidate.clone();
    let mut n = 2;
    while taken.contains(&key(&candidate)) {
        candidate = format!("{base} {n}");
        n += 1;
    }
    taken.insert(key(&candidate));
    candidate
}

/// The Mac apps this app already made for a bottle: the name each one has, by the program it
/// starts. Reading it keeps an app's name tied to its program when another program with the
/// same name shows up.
fn existing_names(root: &Path, b: &Bottle) -> std::collections::HashMap<String, String> {
    let mut found = std::collections::HashMap::new();
    let Ok(apps) = fs::read_dir(root.join(clean_file_name(&b.name))) else {
        return found;
    };
    const OPEN: &str = "<key>BotellaLibreTarget</key><string>";
    for a in apps.flatten() {
        let app = a.path();
        let Some(stem) = app
            .file_stem()
            .filter(|_| app.extension().is_some_and(|e| e == "app"))
        else {
            continue;
        };
        let Ok(plist) = fs::read_to_string(app.join("Contents/Info.plist")) else {
            continue;
        };
        if !plist.contains(MARK) {
            continue;
        }
        if let Some(rest) = plist.split(OPEN).nth(1) {
            if let Some(target) = rest.split("</string>").next() {
                found.insert(target.to_string(), stem.to_string_lossy().into_owned());
            }
        }
    }
    found
}

/// Makes or refreshes a launcher for every program of every bottle under `root`, and removes the
/// launchers this app made for programs that are gone. Returns how many launchers exist.
/// Each bottle comes with the Wine it runs on.
pub fn sync(root: &Path, bottles: &[(Bottle, PathBuf)]) -> usize {
    sync_noting(root, bottles, &mut vec![])
}

/// [`sync`], and one line in `notes` for each launcher that left variables out of its script
/// because their names were not safe to write there.
pub fn sync_noting(root: &Path, bottles: &[(Bottle, PathBuf)], notes: &mut Vec<String>) -> usize {
    let mut keep = std::collections::HashSet::new();
    for (b, wine) in bottles {
        let wine: &Path = wine;
        // Launchers open programs without this window, so connect the profile folders here too.
        let _ = crate::backend::bridge_user_profile(&b.path);
        // Only the programs on the list: uninstallers and removed ones get no Mac app.
        let mut taken = std::collections::HashSet::new();
        let shown: Vec<&Program> = b.programs.iter().filter(|p| p.is_shown()).collect();
        // A program keeps the app name it already has, so a Dock or Launchpad entry does not
        // start another program when a second one with the same name appears.
        let before = existing_names(root, b);
        let mut kept: Vec<Option<String>> = vec![None; shown.len()];
        for (i, p) in shown.iter().enumerate() {
            let target = xml_escape(&p.path.to_string_lossy());
            let Some(stem) = before.get(&target) else {
                continue;
            };
            let key = clean_file_name(stem).to_lowercase();
            if taken.insert(key) {
                // Keep the program's own name when it spells the same file name.
                kept[i] = Some(if *stem == clean_file_name(&p.name) {
                    p.name.clone()
                } else {
                    stem.clone()
                });
            }
        }
        for (i, p) in shown.iter().enumerate() {
            let name = match kept[i].take() {
                Some(name) => name,
                None => launcher_name(&p.name, &p.subtitle, &mut taken),
            };
            let spec = LauncherSpec {
                root,
                wine,
                bottle: b,
                name: &name,
                file: &p.path,
                args: &p.args,
                env: &p.env,
                icon: p.icon.as_deref(),
            };
            if let Some(app) = write_launcher(&spec, notes) {
                keep.insert(app);
            }
        }
    }
    // Tidy up what we made before and no longer applies.
    if let Ok(dirs) = fs::read_dir(root) {
        for d in dirs.flatten() {
            if let Ok(apps) = fs::read_dir(d.path()) {
                for a in apps.flatten() {
                    let p = a.path();
                    if p.extension().map(|e| e == "app").unwrap_or(false)
                        && is_ours(&p)
                        && !keep.contains(&p)
                    {
                        let _ = fs::remove_dir_all(&p);
                    }
                }
            }
            let _ = fs::remove_dir(d.path()); // only succeeds when empty
        }
    }
    keep.len()
}

/// Removes every launcher this app made.
pub fn remove_all(root: &Path) {
    sync(root, &[]);
}

// ---------------------------------------------------------------- Finder double-click

/// The opener's file name without «.app». It never changes: it is where every earlier «Abrir con»
/// choice points.
const OPENER_FILE_STEM: &str = "Abrir con Botella Libre";

pub fn opener_app() -> PathBuf {
    root().join(format!("{OPENER_FILE_STEM}.app"))
}

/// The name Finder and the «Abrir con» menu show for the opener. The file keeps its old name
/// (`opener_app`) and its bundle identifier, so every «Abrir con» choice the person made earlier
/// still points at it; only what is shown changes.
///
/// Measured on macOS: Finder takes the localized name from the language folders only when the
/// bundle's plain `CFBundleDisplayName` is the file's own name; with «Altrune» there too it keeps
/// showing the file name. So the plain display name stays the file's name and the strings files
/// carry «Altrune»; `CFBundleName` (menus, the name the system gives the app) is «Altrune».
pub const OPENER_SHOWN_NAME: &str = "Altrune";

/// Marks the look an opener was built with. When it changes, openers built before it are built
/// again once (the program's own path alone would never tell them apart).
const OPENER_BRAND_STAMP: &str = "brand=Altrune-1";

/// The languages the opener's shown name is written for.
const OPENER_LANGUAGES: [&str; 2] = ["es", "en"];

/// What the stamp file of an opener built for the program at `exe` holds.
fn opener_stamp(exe: &str) -> String {
    format!("{exe}\n{OPENER_BRAND_STAMP}")
}

/// Whether the opener already there can stay: it was built for this program's path and this
/// brand (`stamp` is what its stamp file holds) and is still on disk.
fn opener_is_current(stamp: Option<&str>, exe: &str, app_exists: bool) -> bool {
    app_exists && stamp == Some(opener_stamp(exe).as_str())
}

/// The keys written into the opener's `Info.plist` with `plutil -replace`: key, kind, value.
/// For the names see `OPENER_SHOWN_NAME`.
fn opener_plist_values() -> Vec<(&'static str, &'static str, String)> {
    let types = r#"[{"CFBundleTypeName":"Programa de Windows","CFBundleTypeRole":"Viewer","LSHandlerRank":"Default","LSItemContentTypes":["com.microsoft.windows-executable","com.microsoft.windows-dynamic-link-library"],"CFBundleTypeExtensions":["exe","msi","lnk","bat","url"]}]"#;
    let languages = format!(
        "[{}]",
        OPENER_LANGUAGES
            .iter()
            .map(|l| format!("\"{l}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    vec![
        ("CFBundleDocumentTypes", "-json", types.to_string()),
        // The identifier stays: Launch Services knows the opener by it.
        (
            "CFBundleIdentifier",
            "-string",
            "com.botellalibre.opener".into(),
        ),
        ("CFBundleName", "-string", OPENER_SHOWN_NAME.into()),
        ("CFBundleDisplayName", "-string", OPENER_FILE_STEM.into()),
        // Finder takes the shown name from the language folders, whatever the file is called.
        ("LSHasLocalizedDisplayName", "-bool", "true".into()),
        ("CFBundleLocalizations", "-json", languages),
        ("LSUIElement", "-bool", "true".into()),
    ]
}

/// The text of `InfoPlist.strings` that gives the opener its shown name.
fn opener_strings_file() -> String {
    format!(
        "CFBundleName = \"{OPENER_SHOWN_NAME}\";\nCFBundleDisplayName = \"{OPENER_SHOWN_NAME}\";\n"
    )
}

/// Builds a tiny Mac app that receives the Windows files the person double-clicks in Finder and
/// hands them to this program (`--open <file>`), declaring itself a viewer of Windows programs,
/// shortcuts and installers. Finder shows it as
/// «Altrune» (see `OPENER_SHOWN_NAME`) although its file keeps the old name.
pub fn install_opener() -> Result<PathBuf, String> {
    // Tests never build or register Mac apps (osacompile, plutil, lsregister).
    if cfg!(test) {
        return Err("Las pruebas no crean apps de Mac.".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let app = opener_app();
    let script = format!(
        "on open theFiles\n  repeat with f in theFiles\n    do shell script {} & \" --open \" & quoted form of POSIX path of f & \" > /dev/null 2>&1 &\"\n  end repeat\nend open\non run\n  do shell script {} & \" > /dev/null 2>&1 &\"\nend run\n",
        applescript_string(&sh_quote(&exe.to_string_lossy())),
        applescript_string(&sh_quote(&exe.to_string_lossy())),
    );
    // Rebuild only when this program moved (the script embeds its path) or the brand changed.
    let stamp = app.join("Contents/Resources/botella-libre-path");
    let exe_text = exe.to_string_lossy();
    if opener_is_current(
        fs::read_to_string(&stamp).ok().as_deref(),
        &exe_text,
        app.is_dir(),
    ) {
        return Ok(app);
    }
    let _ = fs::remove_dir_all(&app);
    fs::create_dir_all(root()).map_err(|e| e.to_string())?;
    build_opener(&app, &script)?;
    let _ = fs::write(&stamp, opener_stamp(&exe_text).as_bytes());
    // Tell macOS about it so it shows up in "Open With".
    let lsregister = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    let _ = Command::new(lsregister).arg("-f").arg(&app).output();
    Ok(app)
}

/// Compiles the opener's script into `app` and gives it its identity and shown name. It does not
/// register the app with Launch Services.
fn build_opener(app: &Path, script: &str) -> Result<(), String> {
    let out = Command::new("/usr/bin/osacompile")
        .arg("-o")
        .arg(app)
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).to_string());
    }
    let plist = app.join("Contents/Info.plist");
    for (key, kind, value) in opener_plist_values() {
        let _ = Command::new("/usr/bin/plutil")
            .args(["-replace", key, kind, &value])
            .arg(&plist)
            .output();
    }
    for language in OPENER_LANGUAGES {
        let folder = app.join(format!("Contents/Resources/{language}.lproj"));
        if fs::create_dir_all(&folder).is_ok() {
            let _ = fs::write(folder.join("InfoPlist.strings"), opener_strings_file());
        }
    }
    Ok(())
}

fn applescript_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Program;

    fn bottle(dir: &Path) -> Bottle {
        Bottle {
            name: "Juegos".into(),
            path: dir.join("Juegos"),
            windows: "win10".into(),
            msync: true,
            high_res: false,
            logpixels: 96,
            programs: vec![Program {
                name: "Mi Juego".into(),
                path: dir.join("Juegos/drive_c/juego.exe"),
                args: "-windowed".into(),
                env: vec!["ROSETTA_ADVERTISE_AVX=0".into()],
                saved: true,
                icon: None,
                ..Program::default()
            }],
            env: vec![("FOO".into(), "bar".into())],
            prefix_complete: true,
        }
    }

    #[test]
    fn launcher_runs_program_with_bottle_settings() {
        let dir = std::env::temp_dir().join(format!("bl-test-{}", std::process::id()));
        let root = dir.join("apps");
        let b = bottle(&dir);
        assert_eq!(
            sync(&root, &[(b.clone(), PathBuf::from("/opt/wine/bin/wine"))]),
            1
        );
        let app = root.join("Juegos/Mi Juego.app");
        let script = fs::read_to_string(app.join("Contents/MacOS/run")).unwrap();
        assert!(script.contains("export WINEPREFIX="));
        // A Wine with no MSync/ESync gets no sync switch at all.
        assert!(!script.contains("WINEMSYNC") && !script.contains("WINEESYNC"));
        assert!(script.contains("export DOTNET_EnableWriteXorExecute='0'"));
        assert!(script.contains("export FOO='bar'"));
        assert!(script.contains("unset ROSETTA_ADVERTISE_AVX"));
        assert!(script.contains("'-windowed'"));
        assert!(is_ours(&app));
        // Programs that disappear lose their launcher.
        let mut gone = b;
        gone.programs.clear();
        assert_eq!(
            sync(&root, &[(gone, PathBuf::from("/opt/wine/bin/wine"))]),
            0
        );
        assert!(!app.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Variable names come from a bottle's `cxbottle.conf` and from a program's saved options, so
    /// a crafted one must never reach the launcher's shell script; valid ones and every value
    /// stay exactly as before, and each dropped name is noted.
    #[test]
    fn a_launcher_never_writes_a_variable_name_the_shell_would_run() {
        let dir = std::env::temp_dir().join(format!("bl-test-names-{}", std::process::id()));
        let mut b = bottle(&dir);
        b.env = vec![
            ("FOO".into(), "it's".into()),
            ("X;touch /tmp/pwn".into(), "1".into()),
            ("$(id)".into(), "2".into()),
            ("9LIVES".into(), "3".into()),
        ];
        let extra = vec!["A B=4".to_string(), "=5".to_string(), "OK_2=6".to_string()];
        let (script, dropped) = launcher_script(
            Path::new("/opt/wine/bin/wine"),
            &b,
            Path::new("/b/drive_c/a.exe"),
            "",
            &extra,
        );
        for bad in ["pwn", "$(id)", "A B", "9LIVES", "export =5"] {
            assert!(!script.contains(bad), "{bad} reached the script:\n{script}");
        }
        assert!(script.contains("export FOO='it'\\''s'\n"));
        assert!(script.contains("export OK_2='6'\n"));
        assert!(script.contains("export WINEPREFIX="));
        assert_eq!(dropped, ["X;touch /tmp/pwn", "$(id)", "9LIVES", "A B", ""]);
    }

    /// The dropped names reach the log: `sync_noting` hands back a line for the launcher that
    /// lost them, and none for a launcher that lost nothing.
    #[test]
    fn a_launcher_that_drops_a_name_says_so() {
        let dir = std::env::temp_dir().join(format!("bl-test-notes-{}", std::process::id()));
        let root = dir.join("apps");
        let wine = PathBuf::from("/opt/wine/bin/wine");
        let mut b = bottle(&dir);
        let mut notes = vec![];
        assert_eq!(
            sync_noting(&root, &[(b.clone(), wine.clone())], &mut notes),
            1
        );
        assert!(notes.is_empty());
        // A launcher that is written with a name left out says so once.
        let _ = fs::remove_dir_all(&root);
        b.env.push(("A;B\nx".into(), "1".into()));
        assert_eq!(
            sync_noting(&root, &[(b.clone(), wine.clone())], &mut notes),
            1
        );
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("«A;B?x»") && notes[0].contains("Mi Juego"));
        assert!(!notes[0].contains('\n'));
        // The launcher is already as it should be: syncing again does not repeat the line.
        sync_noting(&root, &[(b, wine)], &mut notes);
        assert_eq!(notes.len(), 1, "the same line was logged again: {notes:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// The launcher's two files, byte for byte: what a launcher says is what the Dock and
    /// Launchpad run, so a refactor of how it is written must not move a character. The paths are
    /// fixed (the folders need not exist; only the Mac app is written, under a temporary root).
    #[test]
    fn a_launcher_is_written_with_exactly_this_script_and_plist() {
        let dir = std::env::temp_dir().join(format!("bl-test-exact-{}", std::process::id()));
        let root = dir.join("apps");
        let b = Bottle {
            name: "Juegos".into(),
            path: PathBuf::from("/bottles/Juegos"),
            windows: "win10".into(),
            msync: true,
            high_res: false,
            logpixels: 96,
            programs: vec![],
            env: vec![("FOO".into(), "bar".into())],
            prefix_complete: true,
        };
        let mut notes = vec![];
        let env = ["ROSETTA_ADVERTISE_AVX=0".to_string(), "A;B=1".to_string()];
        let spec = LauncherSpec {
            root: &root,
            wine: Path::new("/opt/wine/bin/wine"),
            bottle: &b,
            name: "Mi & Juego",
            file: Path::new("/bottles/Juegos/drive_c/juego.exe"),
            args: "-windowed -x",
            env: &env,
            icon: None,
        };
        let app = write_launcher(&spec, &mut notes).unwrap();
        assert_eq!(app, root.join("Juegos/Mi & Juego.app"));
        let script = fs::read_to_string(app.join("Contents/MacOS/run")).unwrap();
        let plist = fs::read_to_string(app.join("Contents/Info.plist")).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            script,
            "#!/bin/sh
# Made by Botella Libre: opens a Windows program with free Wine.
export WINEPREFIX='/bottles/Juegos'
export WINEDEBUG='fixme-all,fixme+winediag,warn+d2d'
export DOTNET_EnableWriteXorExecute='0'
export FOO='bar'
unset ROSETTA_ADVERTISE_AVX
cd '/bottles/Juegos/drive_c' 2>/dev/null || cd '/bottles/Juegos'
exec '/opt/wine/bin/wine' '/bottles/Juegos/drive_c/juego.exe' '-windowed' '-x'
"
        );
        // The bundle id comes from `stable_id`, which hashes with std's DefaultHasher. If this
        // assertion breaks after a toolchain update while nothing here changed, the hash
        // changed and every existing launcher would get a new id: give `stable_id` a fixed
        // hash instead of editing the expected text.
        assert_eq!(
            plist,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Mi &amp; Juego</string>
  <key>CFBundleDisplayName</key><string>Mi &amp; Juego</string>
  <key>CFBundleIdentifier</key><string>com.botellalibre.app.09ed622afb440d85</string>
  <key>CFBundleExecutable</key><string>run</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>1.0</string>
  <key>LSUIElement</key><true/>
  <key>BotellaLibreLauncher</key><true/>
  <key>BotellaLibreBottle</key><string>Juegos</string>
  <key>BotellaLibreTarget</key><string>/bottles/Juegos/drive_c/juego.exe</string>
</dict>
</plist>
"#
        );
        assert_eq!(
            notes,
            ["Se omitieron variables de entorno con un nombre no válido en el lanzador de «Mi & Juego» (botella «Juegos»): «A;B»"]
        );
    }

    /// The plist of a launcher that has an icon gains one key, and nothing else changes.
    #[test]
    fn a_launcher_plist_with_an_icon_names_it() {
        let plain = info_plist("P", "com.botellalibre.app.x", "B", "/t.exe", false);
        let with_icon = info_plist("P", "com.botellalibre.app.x", "B", "/t.exe", true);
        assert_eq!(
            with_icon,
            plain.replace(
                "  <key>LSUIElement</key>",
                "  <key>CFBundleIconFile</key><string>icon</string>\n  <key>LSUIElement</key>"
            )
        );
        assert!(!plain.contains("CFBundleIconFile"));
    }

    #[test]
    fn programs_with_one_name_get_apps_of_their_own() {
        let mut taken = std::collections::HashSet::new();
        assert_eq!(launcher_name("Launcher", "Alfa", &mut taken), "Launcher");
        assert_eq!(
            launcher_name("launcher", "Beta", &mut taken),
            "launcher (Beta)"
        );
        assert_eq!(
            launcher_name("Launcher", "Alfa", &mut taken),
            "Launcher (Alfa)"
        );
        // Without a place to add, a number tells them apart.
        assert_eq!(launcher_name("Launcher", "", &mut taken), "Launcher 2");
        assert_eq!(launcher_name("Otro", "", &mut taken), "Otro");
    }

    #[test]
    fn two_programs_with_one_name_write_two_apps() {
        let dir = std::env::temp_dir().join(format!("bl-test-same-{}", std::process::id()));
        let root = dir.join("apps");
        let mut b = bottle(&dir);
        b.programs = ["Alfa", "Beta"]
            .iter()
            .map(|folder| Program {
                name: "Launcher".into(),
                path: dir.join(format!("Juegos/drive_c/{folder}/launcher.exe")),
                subtitle: folder.to_string(),
                ..Program::default()
            })
            .collect();
        assert_eq!(sync(&root, &[(b, PathBuf::from("/opt/wine/bin/wine"))]), 2);
        assert!(root.join("Juegos/Launcher.app").is_dir());
        assert!(root.join("Juegos/Launcher (Beta).app").is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_app_keeps_its_program_when_a_same_name_program_sorts_first() {
        let dir = std::env::temp_dir().join(format!("bl-test-keep-{}", std::process::id()));
        let root = dir.join("apps");
        let wine = PathBuf::from("/opt/wine/bin/wine");
        let program = |folder: &str, subtitle: &str| Program {
            name: "Launcher".into(),
            path: dir.join(format!("Juegos/drive_c/{folder}/launcher.exe")),
            subtitle: subtitle.to_string(),
            ..Program::default()
        };
        let plist =
            |app: &str| fs::read_to_string(root.join(app).join("Contents/Info.plist")).unwrap();
        let mut b = bottle(&dir);
        b.programs = vec![program("Zeta", "")];
        assert_eq!(sync(&root, &[(b.clone(), wine.clone())]), 1);
        assert!(plist("Juegos/Launcher.app").contains("Zeta"));
        // A second program with the same name sorts first: Zeta still has the bare name.
        b.programs = vec![program("Alfa", "Alfa"), program("Zeta", "Zeta")];
        assert_eq!(sync(&root, &[(b, wine)]), 2);
        assert!(plist("Juegos/Launcher.app").contains("Zeta"));
        assert!(plist("Juegos/Launcher (Alfa).app").contains("Alfa"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_long_place_does_not_make_a_name_too_long_for_a_mac_file() {
        let mut taken = std::collections::HashSet::new();
        assert_eq!(launcher_name("Guardado", "", &mut taken), "Guardado");
        let place = format!("Guardado · Program Files/{}/game.exe", "Studio/".repeat(60));
        let name = launcher_name("Guardado", &place, &mut taken);
        assert!(name.len() < 80, "{name}");
        assert!(name.ends_with("game.exe)"), "{name}");
        // A short place is kept whole.
        assert_eq!(
            launcher_name("Guardado", "Alfa", &mut taken),
            "Guardado (Alfa)"
        );
    }

    fn value_of(key: &str) -> Option<(&'static str, String)> {
        opener_plist_values()
            .into_iter()
            .find(|(k, _, _)| *k == key)
            .map(|(_, kind, v)| (kind, v))
    }

    /// Finder and «Abrir con» read the name from the bundle: the language folders' strings and
    /// the bundle name say «Altrune», and the bundle says it has a localized display name.
    #[test]
    fn the_opener_is_shown_as_altrune() {
        assert_eq!(
            value_of("CFBundleName"),
            Some(("-string", "Altrune".into()))
        );
        // Finder only reads the localized name when this is the file's own name.
        assert_eq!(
            value_of("CFBundleDisplayName"),
            Some((
                "-string",
                opener_app().file_stem().unwrap().to_string_lossy().into()
            ))
        );
        assert_eq!(
            value_of("LSHasLocalizedDisplayName"),
            Some(("-bool", "true".into()))
        );
        let (kind, languages) = value_of("CFBundleLocalizations").unwrap();
        assert_eq!(kind, "-json");
        assert_eq!(languages, r#"["es","en"]"#);
        let strings = opener_strings_file();
        assert!(
            strings.contains("CFBundleDisplayName = \"Altrune\";"),
            "{strings}"
        );
        assert!(strings.contains("CFBundleName = \"Altrune\";"), "{strings}");
        assert!(strings.ends_with(";\n"));
        assert!(!strings.contains("Botella"), "{strings}");
    }

    /// What keeps every earlier «Abrir con» choice working: the identifier, the viewer role for
    /// the Windows types and the opener's file path do not change.
    #[test]
    fn the_opener_keeps_its_identity_and_its_path() {
        assert_eq!(
            value_of("CFBundleIdentifier"),
            Some(("-string", "com.botellalibre.opener".into()))
        );
        let (kind, types) = value_of("CFBundleDocumentTypes").unwrap();
        assert_eq!(kind, "-json");
        for part in [
            "Viewer",
            "exe",
            "msi",
            "lnk",
            "com.microsoft.windows-executable",
        ] {
            assert!(types.contains(part), "{part}");
        }
        assert_eq!(value_of("LSUIElement"), Some(("-bool", "true".into())));
        // Every value is written exactly once, and the keys are the ones plutil replaces.
        let keys: Vec<_> = opener_plist_values().iter().map(|v| v.0).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(keys.len(), sorted.len());
        assert!(opener_app().ends_with("Applications/Botella Libre/Abrir con Botella Libre.app"));
    }

    /// An opener made by an older version has the program's path in its stamp and nothing else:
    /// it is built again once; the new one stays until the program moves.
    #[test]
    fn an_old_opener_is_rebuilt_once_and_a_new_one_stays() {
        let exe = "/Applications/Altrune.app/Contents/MacOS/Altrune";
        let old_stamp = exe; // what the stamp file held before the brand was stamped
        assert!(!opener_is_current(Some(old_stamp), exe, true), "old opener");
        let new_stamp = opener_stamp(exe);
        assert!(opener_is_current(Some(&new_stamp), exe, true), "new opener");
        // The program moved: the script inside has the wrong path.
        assert!(!opener_is_current(
            Some(&new_stamp),
            "/tmp/other/altrune",
            true
        ));
        // No stamp, or the app was deleted.
        assert!(!opener_is_current(None, exe, true));
        assert!(!opener_is_current(Some(&new_stamp), exe, false));
    }

    /// Tests never build the opener (osacompile, plutil, lsregister).
    #[test]
    fn tests_do_not_build_the_opener() {
        assert!(install_opener().is_err());
    }
}
