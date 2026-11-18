//! The environment and commands Wine programs start with, and the link between the Mac user's
//! profile folder and the one a bottle already has.

use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// Extra settings the Run Command window can add: the Metal performance HUD
/// (`MTL_HUD_ENABLED=1`) and hiding AVX from programs under Rosetta (`ROSETTA_ADVERTISE_AVX=0`).
/// They are appended to the environment text, space-separated.
pub const ENV_METAL_HUD: &str = "MTL_HUD_ENABLED=1";

pub const ENV_ROSETTA_AVX: &str = "ROSETTA_ADVERTISE_AVX=0";

/// What Wine prints by default here: no "fixme" chatter, except Wine's own diagnostics channel
/// and Direct2D's warnings. Those two say why a window cannot be drawn ("None of the requested
/// D3D feature levels is supported…", "d2d_factory_get_device Failed to create device") and
/// stay silent otherwise; diagnostics.rs reads them.
pub const DEFAULT_WINEDEBUG: &str = "fixme-all,fixme+winediag,warn+d2d";

/// What every program starts with, so nothing has to be tuned by hand: .NET 7/8 apps working
/// under Rosetta (`DOTNET_EnableWriteXorExecute=0`), and AVX advertised under Rosetta
/// (`ROSETTA_ADVERTISE_AVX=1`) unless a program asks for it off.
/// Wine's error messages are kept (only its "fixme" chatter is silenced) so the diagnostics can
/// read them.
pub fn base_env(prefix: &Path, msync: bool) -> Vec<(String, String)> {
    vec![
        ("WINEPREFIX".into(), prefix.to_string_lossy().to_string()),
        ("WINEMSYNC".into(), if msync { "1" } else { "0" }.into()),
        (
            "WINEDEBUG".into(),
            // A WINEDEBUG set in the shell that launched the app wins, for deeper debugging.
            std::env::var("WINEDEBUG").unwrap_or_else(|_| DEFAULT_WINEDEBUG.into()),
        ),
        ("DOTNET_EnableWriteXorExecute".into(), "0".into()),
        ("ROSETTA_ADVERTISE_AVX".into(), "1".into()),
    ]
}

/// Whether `name` can be written into shell text (a launcher's script, the Terminal's command)
/// as `export NAME=...` or `unset NAME`: letters, digits and `_`, not starting with a digit.
/// Anything else (a space, `;`, `$(...)`, a quote, an empty name) would be read by the shell as
/// code. `Command::env` needs no such check, because no shell reads those names.
pub fn is_shell_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Splits `vars` into the ones whose names are safe to write into shell text and the names that
/// are not (in their order, to be noted in a log). Values are never touched: they are always
/// quoted when written.
pub fn split_shell_env(vars: Vec<(String, String)>) -> (Vec<(String, String)>, Vec<String>) {
    let mut dropped = vec![];
    let kept = vars
        .into_iter()
        .filter(|(k, _)| {
            let ok = is_shell_env_name(k);
            if !ok {
                dropped.push(k.clone());
            }
            ok
        })
        .collect();
    (kept, dropped)
}

/// The line that tells the person (in a log) which variable names were left out of `place`.
pub fn dropped_names_note(place: &str, names: &[String]) -> String {
    // A name comes from a file somebody else made: it is cut short and has no line breaks or
    // other control characters, so it cannot spread over the log.
    let shown: Vec<String> = names
        .iter()
        .map(|n| {
            let clean: String = n
                .chars()
                .map(|c| if c.is_control() { '?' } else { c })
                .take(40)
                .collect();
            format!("«{clean}»")
        })
        .collect();
    format!(
        "Se omitieron variables de entorno con un nombre no válido en {place}: {}",
        shown.join(", ")
    )
}

/// Everything a program in this bottle starts with: the base above, then the bottle's own
/// variables (bottle variables first, this app's own settings on top), then
/// the program's. `ROSETTA_ADVERTISE_AVX=0` (or empty) removes the variable.
pub fn program_env(b: &Bottle, extra: &[String]) -> (Vec<(String, String)>, Vec<String>) {
    let mut env = base_env(&b.path, b.msync);
    let set = |k: &str, v: &str, env: &mut Vec<(String, String)>| {
        env.retain(|(ek, _)| ek != k);
        env.push((k.to_string(), v.to_string()));
    };
    for (k, v) in &b.env {
        set(k, v, &mut env);
    }
    for pair in extra {
        if let Some((k, v)) = pair.split_once('=') {
            set(k.trim(), v.trim(), &mut env);
        }
    }
    let mut removed = vec![];
    if env
        .iter()
        .any(|(k, v)| k == "ROSETTA_ADVERTISE_AVX" && (v == "0" || v.is_empty()))
    {
        env.retain(|(k, _)| k != "ROSETTA_ADVERTISE_AVX");
        removed.push("ROSETTA_ADVERTISE_AVX".to_string());
    }
    (env, removed)
}

/// Which faster-sync switch a Wine build really reads, found in its own files (not assumed):
/// The Metal engine's Wine reads `WINEMSYNC`, the games engine (Game Porting Toolkit) `WINEESYNC`,
/// WineHQ's 11.0 build neither. Checked once per Wine.
pub fn sync_var(wine: &Path) -> Option<&'static str> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Mutex<std::collections::HashMap<PathBuf, Option<&'static str>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(v) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(wine) {
        return *v;
    }
    let root = wine
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    let files = [
        root.join("lib/wine/x86_64-unix/ntdll.so"),
        root.join("bin/wineserver"),
    ];
    let has = |needle: &[u8]| {
        files.iter().any(|f| {
            fs::read(f)
                .map(|b| b.windows(needle.len()).any(|w| w == needle))
                .unwrap_or(false)
        })
    };
    let found = if has(b"WINEMSYNC\0") {
        Some("WINEMSYNC")
    } else if has(b"WINEESYNC\0") {
        Some("WINEESYNC")
    } else {
        None
    };
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(wine.to_path_buf(), found);
    found
}

/// Fits a program's environment to the Wine that will run it: adds what that engine needs just
/// to work (only the Metal engine does: its DXMT and the Mac libraries it borrows; a variable the
/// bottle or the program sets itself wins), and puts the bottle's "faster sync" choice under the
/// name this Wine reads, or drops it when this Wine has no such feature.
pub fn adapt_env_to_wine(wine: &Path, env: &mut Vec<(String, String)>) {
    for (k, v) in runtime_env(wine) {
        if !env.iter().any(|(name, _)| *name == k) {
            env.push((k, v));
        }
    }
    let Some(pos) = env.iter().position(|(k, _)| k == "WINEMSYNC") else {
        return;
    };
    let (_, v) = env.remove(pos);
    if let Some(name) = sync_var(wine) {
        env.retain(|(k, _)| k != name);
        env.push((name.to_string(), v));
    }
}

/// The program a Wine command starts. Wine only ever runs in Altrune's own bottles: a command
/// aimed at a prefix anywhere else runs nothing (it just fails), whatever the window allowed, so
/// no Wine can write into a folder that belongs to something else.
pub(super) fn wine_program(wine: &Path, prefix: &Path) -> Command {
    if is_own_prefix(prefix) {
        Command::new(wine)
    } else {
        Command::new("/usr/bin/false")
    }
}

pub fn wine_cmd(wine: &Path, prefix: &Path, msync: bool) -> Command {
    let mut c = wine_program(wine, prefix);
    let mut env = base_env(prefix, msync);
    adapt_env_to_wine(wine, &mut env);
    c.envs(env).current_dir(prefix);
    c
}

pub fn mac_user() -> String {
    std::env::var("USER")
        .ok()
        .filter(|u| !u.is_empty())
        .or_else(|| {
            Command::new("/usr/bin/id")
                .arg("-un")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        })
        .unwrap_or_default()
}

/// The Mac user's name, looked up once per run: it does not change while the app is open, and
/// the log redacts every line it keeps with it.
pub fn mac_user_once() -> &'static str {
    static MAC_USER: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MAC_USER.get_or_init(mac_user)
}

/// Folders of `C:\users` that are Windows' own and never somebody's profile (lower case).
const SYSTEM_PROFILES: [&str; 5] = [
    "public",
    "all users",
    "default",
    "default user",
    "steamuser",
];

/// What a folder set aside by [`bridge_user_profile`] is called, so that it is never taken for a
/// profile itself.
const ASIDE_MARK: &str = " (antes de Botella Libre";

/// The one profile folder a bottle already has besides the Mac user's own: a real folder in
/// `C:\users` that is not one of Windows' own and not one set aside earlier. `None` when there
/// is none, or when there are several and so no way to tell which one is meant.
fn single_profile(users: &Path, user: &str) -> Option<String> {
    let mut found = fs::read_dir(users)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| {
            let low = n.to_lowercase();
            !n.starts_with('.')
                && !low.eq_ignore_ascii_case(user)
                && !SYSTEM_PROFILES.contains(&low.as_str())
                && !n.contains(ASIDE_MARK)
        });
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

/// Whether anything but folders and links is somewhere inside `dir`: what a program saved,
/// as opposed to the empty folders Wine makes for a new profile. Links are not followed.
fn holds_files(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|e| match e.file_type() {
            Ok(t) if t.is_dir() => holds_files(&e.path()),
            Ok(t) => !t.is_symlink(),
            Err(_) => true,
        })
    })
}

/// Makes `C:\users\<Mac user>` the same folder as the profile a bottle already has (the one
/// real profile folder besides the Mac user's own), so free Wine finds the profile the programs
/// were installed with: a bottle brought in from another Mac or another Wine names its user
/// differently, and Wine would start an empty profile beside it. A profile free Wine already
/// started there is kept beside it (renamed), never deleted, and only when nothing was saved
/// in it: a profile with files in it is left as it is. Returns what it did.
pub fn bridge_user_profile(prefix: &Path) -> Option<String> {
    // Only inside Altrune's own bottles.
    if !is_own_prefix(prefix) {
        return None;
    }
    bridge_profile_of(prefix, &mac_user())
}

/// [`bridge_user_profile`] for the Windows user called `user`, in any folder.
fn bridge_profile_of(prefix: &Path, user: &str) -> Option<String> {
    if user.is_empty() {
        return None;
    }
    let users = prefix.join("drive_c/users");
    let mine = users.join(user);
    // A link there is already somebody's choice (this bridge's own, or a person's).
    if mine.is_symlink() {
        return None;
    }
    let old = single_profile(&users, user)?;
    let mut note = String::new();
    if mine.exists() {
        // A folder with something saved in it is somebody's profile, not an empty one: the
        // bottle is left as it is.
        if holds_files(&mine) {
            return None;
        }
        let mut aside = users.join(format!("{user}{ASIDE_MARK})"));
        let mut n = 2;
        while aside.exists() {
            aside = users.join(format!("{user}{ASIDE_MARK} {n})"));
            n += 1;
        }
        fs::rename(&mine, &aside).ok()?;
        note = format!(
            " (el perfil vacío quedó en «{}»)",
            aside.file_name()?.to_string_lossy()
        );
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&old, &mine).ok()?;
    Some(format!(
        "Perfil de Windows «{old}» conectado para {user}{note}"
    ))
}

/// A command inside a bottle with all of its settings.
pub fn bottle_cmd(wine: &Path, b: &Bottle) -> Command {
    let _ = bridge_user_profile(&b.path);
    let (mut env, removed) = program_env(b, &[]);
    adapt_env_to_wine(wine, &mut env);
    let mut c = wine_program(wine, &b.path);
    c.envs(env).current_dir(&b.path);
    for r in removed {
        c.env_remove(r);
    }
    c
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bl-profile-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_only_other_profile_is_shared_and_the_empty_one_kept() {
        let dir = scratch("share");
        let users = dir.join("drive_c/users");
        fs::create_dir_all(users.join("legacy/AppData/Local/Rockstar Games")).unwrap();
        fs::create_dir_all(users.join("Public")).unwrap();
        fs::create_dir_all(users.join("ana").join("Documents")).unwrap();
        let note = bridge_profile_of(&dir, "ana").expect("bridged");
        assert!(note.contains("antes de Botella Libre"));
        assert!(note.contains("«legacy»"), "{note}");
        assert_eq!(
            fs::read_link(users.join("ana")).unwrap(),
            Path::new("legacy")
        );
        assert!(users.join("ana/AppData/Local/Rockstar Games").is_dir());
        assert!(users
            .join("ana (antes de Botella Libre)")
            .join("Documents")
            .is_dir());
        // Second time: nothing to do, and the folder set aside is no profile to share.
        assert!(bridge_profile_of(&dir, "ana").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    /// A Mac user's folder with something saved in it is a real profile: nothing is moved.
    #[test]
    fn a_profile_with_saved_files_is_not_set_aside() {
        let dir = scratch("saved");
        let users = dir.join("drive_c/users");
        fs::create_dir_all(users.join("stray")).unwrap();
        fs::create_dir_all(users.join("ana/AppData/Local/Game")).unwrap();
        fs::write(users.join("ana/AppData/Local/Game/save.dat"), "x").unwrap();
        assert!(bridge_profile_of(&dir, "ana").is_none());
        assert!(!users.join("ana").is_symlink());
        assert!(users.join("ana/AppData/Local/Game/save.dat").is_file());
        assert!(!users.join("ana (antes de Botella Libre)").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// A user folder that is not there yet is simply linked.
    #[test]
    fn a_missing_user_folder_is_linked_without_setting_anything_aside() {
        let dir = scratch("missing");
        let users = dir.join("drive_c/users");
        fs::create_dir_all(users.join("legacy")).unwrap();
        let note = bridge_profile_of(&dir, "ana").expect("bridged");
        assert!(!note.contains("vacío"), "{note}");
        assert_eq!(
            fs::read_link(users.join("ana")).unwrap(),
            Path::new("legacy")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// A bottle that already has its Mac user's folder and no other profile is left alone, and so
    /// is one with several possible profiles: there is no telling which one is meant.
    #[test]
    fn nothing_is_linked_unless_one_profile_is_clearly_the_old_one() {
        let alone = scratch("alone");
        fs::create_dir_all(alone.join("drive_c/users/ana")).unwrap();
        fs::create_dir_all(alone.join("drive_c/users/Public")).unwrap();
        assert!(bridge_profile_of(&alone, "ana").is_none());
        let many = scratch("many");
        for n in ["uno", "otro"] {
            fs::create_dir_all(many.join("drive_c/users").join(n)).unwrap();
        }
        assert!(bridge_profile_of(&many, "ana").is_none());
        assert!(!many.join("drive_c/users/ana").exists());
        // A link somebody made is theirs.
        let linked = scratch("linked");
        let users = linked.join("drive_c/users");
        fs::create_dir_all(users.join("viejo")).unwrap();
        fs::create_dir_all(users.join("otro-sitio")).unwrap();
        std::os::unix::fs::symlink("otro-sitio", users.join("ana")).unwrap();
        assert!(bridge_profile_of(&linked, "ana").is_none());
        assert_eq!(
            fs::read_link(users.join("ana")).unwrap(),
            Path::new("otro-sitio")
        );
        assert!(bridge_profile_of(&alone, "").is_none());
        for d in [alone, many, linked] {
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// Outside the app's own bottles folder nothing is changed, whoever the user is.
    #[test]
    fn a_prefix_outside_the_apps_bottles_is_not_touched() {
        let dir = scratch("outside");
        fs::create_dir_all(dir.join("drive_c/users/legacy")).unwrap();
        assert!(bridge_user_profile(&dir).is_none());
        assert!(!dir.join("drive_c/users").join(mac_user()).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod shell_name_tests {
    use super::*;

    #[test]
    fn only_plain_names_are_safe_for_a_shell() {
        for ok in [
            "A",
            "_",
            "WINEPREFIX",
            "DOTNET_EnableWriteXorExecute",
            "a1_B2",
        ] {
            assert!(is_shell_env_name(ok), "{ok}");
        }
        for bad in [
            "",
            "1A",
            "A B",
            "A-B",
            "A=B",
            "X;touch /tmp/pwn",
            "$(id)",
            "`id`",
            "A\nB",
            "Ñ",
            "A'B",
        ] {
            assert!(!is_shell_env_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn splitting_keeps_valid_variables_and_their_values_in_order() {
        let (kept, dropped) = split_shell_env(vec![
            ("B".into(), "2; rm".into()),
            ("A B".into(), "x".into()),
            ("A".into(), "1".into()),
        ]);
        assert_eq!(
            kept,
            [("B".into(), "2; rm".into()), ("A".into(), "1".into())]
        );
        assert_eq!(dropped, ["A B"]);
    }
}
