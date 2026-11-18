//! The saved settings: programs added by hand, per-bottle choices and the app's config file.

use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedProgram {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub args: String,
    #[serde(default)]
    pub env: Vec<String>,
}

/// How the window looks: like the Mac (the default), always light or always dark. Stored as the
/// lowercase English word. Reading never fails: a word nobody offers, a misspelling or a value
/// of another type means "system", so this field can never make config.json unreadable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    /// In the order they are offered.
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];
}

impl<'de> Deserialize<'de> for Appearance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Any JSON value is accepted first and judged afterwards.
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(match value.as_str() {
            Some("light") => Self::Light,
            Some("dark") => Self::Dark,
            _ => Self::System,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BottleSettings {
    pub msync: Option<bool>,
    pub windows: Option<String>,
    #[serde(default)]
    pub programs: Vec<SavedProgram>,
    /// Programs taken off the list by their NAME. Older versions wrote only this; it is still
    /// read (every program with that name stays removed) but new removals use `hidden_files`.
    #[serde(default)]
    pub hidden: Vec<String>,
    /// Programs taken off the list by their file (the .lnk or .exe), relative to the bottle's
    /// folder so the choice survives renaming, duplicating or importing the bottle. Two programs
    /// with one name are different files, so removing one never removes the other.
    #[serde(default)]
    pub hidden_files: Vec<String>,
    /// Recipes already written into this bottle.
    #[serde(default)]
    pub recipes: Vec<String>,
    /// "games" to run this bottle with the games engine; otherwise the usual Wine.
    #[serde(default)]
    pub engine: Option<String>,
    /// The newest Wine this bottle was opened with ("wine-11.0"), to never run it with an older one.
    #[serde(default)]
    pub last_wine: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub wine_path: Option<String>,
    #[serde(default)]
    pub bottles: BTreeMap<String, BottleSettings>,
    /// The bottle used when none is chosen.
    #[serde(default)]
    pub default_bottle: Option<String>,
    /// Make a Mac app for every installed program.
    #[serde(default = "yes")]
    pub mac_launchers: bool,
    /// The games engine was already offered (installed by itself once).
    #[serde(default)]
    pub games_offered: bool,
    /// Light, dark or like the Mac (missing in older files: like the Mac).
    #[serde(default)]
    pub appearance: Appearance,
    /// The file on disk could not be read and no copy of it could be made, so a save must not
    /// replace it. Never written to the file.
    #[serde(skip)]
    pub(crate) file_not_kept: bool,
}

fn yes() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            wine_path: None,
            bottles: BTreeMap::new(),
            default_bottle: None,
            mac_launchers: true,
            games_offered: false,
            appearance: Appearance::System,
            file_not_kept: false,
        }
    }
}

/// What reading config.json found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigLoad {
    /// There was no file yet (the first start): the defaults are right.
    Missing,
    /// The file was read.
    Loaded,
    /// The file exists but could not be read. The defaults are used, and the person's file was
    /// copied (or moved) aside, to this path when that worked, before anything can overwrite it.
    Unreadable { saved_as: Option<PathBuf> },
}

/// The last save that failed and was not yet shown to the person. `SAVE_FAILING` makes a run of
/// failed saves count once; it is cleared by the next save that works.
static SAVE_FAILURE: Mutex<Option<String>> = Mutex::new(None);
static SAVE_FAILING: AtomicBool = AtomicBool::new(false);

/// The words for a failed save, if one has not been shown yet. Cheap: it only looks in memory.
pub fn take_save_failure() -> Option<String> {
    SAVE_FAILURE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

fn note_save_result(result: &std::io::Result<()>) {
    match result {
        Ok(()) => SAVE_FAILING.store(false, Ordering::Relaxed),
        Err(e) => {
            if !SAVE_FAILING.swap(true, Ordering::Relaxed) {
                *SAVE_FAILURE.lock().unwrap_or_else(|p| p.into_inner()) = Some(format!(
                    "No se pudo guardar tu configuración: {}",
                    io_error_text(e)
                ));
            }
        }
    }
}

/// `YYYYMMDD-HHMMSS` (UTC) for a count of seconds since 1970, for the name of a saved copy.
fn dated_suffix(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    // Civil date from a day count (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Puts the unreadable config.json aside under a dated name (it is copied, so the unreadable file stays
/// until the next save replaces it). Returns where the copy is.
fn keep_unreadable(file: &Path) -> Option<PathBuf> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let stem = format!("config.json.ilegible-{}", dated_suffix(secs));
    let dir = file.parent()?;
    // The same broken file seen again (every load looks at it) keeps the copy it already has.
    if let Some(same) = existing_copy_of(file, dir) {
        return Some(same);
    }
    let mut target = dir.join(&stem);
    let mut n = 2;
    while fs::symlink_metadata(&target).is_ok() {
        target = dir.join(format!("{stem}-{n}"));
        n += 1;
    }
    // Moving it is the second choice: a copy never loses the file, a move does not need to read it.
    (fs::copy(file, &target).is_ok() || fs::rename(file, &target).is_ok()).then_some(target)
}

/// An earlier saved copy with exactly the bytes of `file`, if there is one.
fn existing_copy_of(file: &Path, dir: &Path) -> Option<PathBuf> {
    let bytes = fs::read(file).ok()?;
    let mut copies: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("config.json.ilegible-"))
        })
        .collect();
    copies.sort();
    copies
        .into_iter()
        .find(|p| fs::read(p).is_ok_and(|b| b == bytes))
}

impl BottleSettings {
    /// Whether the person removed this program's file from the list.
    pub fn hides_file(&self, prefix: &Path, file: &Path) -> bool {
        let id = file_identity(prefix, file);
        self.hidden_files
            .iter()
            .any(|h| h.eq_ignore_ascii_case(&id))
    }

    /// Takes a program's file off the list. Removing the same file twice writes it once.
    pub fn hide_file(&mut self, prefix: &Path, file: &Path) {
        if !self.hides_file(prefix, file) {
            self.hidden_files.push(file_identity(prefix, file));
        }
    }

    /// Puts a program's file back on the list; says whether it was removed by file.
    pub fn show_file(&mut self, prefix: &Path, file: &Path) -> bool {
        let id = file_identity(prefix, file);
        let before = self.hidden_files.len();
        self.hidden_files.retain(|h| !h.eq_ignore_ascii_case(&id));
        self.hidden_files.len() != before
    }

    /// Brings back one removed program, never the others that share its name. A program an older
    /// version removed by name is replaced by entries for the other removed programs in `listed`
    /// with that name, so those stay removed exactly as before.
    pub fn unhide_program(&mut self, prefix: &Path, program: &Program, listed: &[Program]) {
        self.show_file(prefix, &program.path);
        let name = fold_name(&program.name);
        if !self.hidden.iter().any(|h| fold_name(h) == name) {
            return;
        }
        self.hidden.retain(|h| fold_name(h) != name);
        for other in listed.iter().filter(|o| {
            o.listing == Listing::Removed && o.path != program.path && fold_name(&o.name) == name
        }) {
            self.hide_file(prefix, &other.path);
        }
    }

    /// Remembers a program added by hand. A program is its file: saving a file that is already
    /// saved updates that entry where it is, and two files with one name are two programs.
    pub fn save_program(&mut self, program: SavedProgram) {
        let same = |p: &SavedProgram| p.path.eq_ignore_ascii_case(&program.path);
        match self.programs.iter().position(same) {
            Some(i) => {
                self.programs[i] = program.clone();
                // An older file may hold the same file twice; the first entry is the one kept.
                let mut seen = false;
                self.programs
                    .retain(|p| !same(p) || !std::mem::replace(&mut seen, true));
            }
            None => self.programs.push(program),
        }
    }

    /// Forgets a program added by hand, by its file.
    pub fn forget_program(&mut self, file: &Path) {
        let file = file.to_string_lossy();
        self.programs
            .retain(|p| !p.path.eq_ignore_ascii_case(&file));
    }

    /// Takes a program off the list for good, by its file. A program added by hand is forgotten
    /// together with the twins the list folded into it. When that file is a shortcut, the shortcut
    /// itself is removed too: otherwise the scan would list it again as a plain program (a
    /// shortcut opened with options is saved under its own file), and «Volver a mostrar» could
    /// not bring it back.
    pub fn remove_program(&mut self, prefix: &Path, program: &Program) {
        let mut files = vec![program.path.clone()];
        if program.saved {
            files.extend(
                saved_twins(prefix, self, &program.path)
                    .into_iter()
                    .map(PathBuf::from),
            );
            for f in &files {
                self.forget_program(f);
            }
        }
        for f in files {
            let is_shortcut = f
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("lnk") || e.eq_ignore_ascii_case("url"));
            if !program.saved || is_shortcut {
                self.hide_file(prefix, &f);
            }
        }
    }

    /// Saved programs that live inside the bottle's old folder now point into `new`: a bottle
    /// that moved or was copied keeps its programs pointing at its own files. Paths outside the
    /// old folder (a program kept elsewhere) stay as they are.
    pub fn rebase_programs(&mut self, old: &Path, new: &Path) {
        for p in &mut self.programs {
            if let Ok(rest) = Path::new(&p.path).strip_prefix(old) {
                p.path = new.join(rest).to_string_lossy().to_string();
            }
        }
    }
}

impl Config {
    pub fn load() -> Self {
        Self::load_noting().0
    }

    /// Reads the settings and says what was found, so an unreadable file can be told to the person.
    pub fn load_noting() -> (Self, ConfigLoad) {
        Self::load_from(&data_dir())
    }

    /// A file that cannot be read is never silently replaced: it is kept under a dated name first.
    pub(crate) fn load_from(dir: &Path) -> (Self, ConfigLoad) {
        let file = dir.join("config.json");
        let unreadable = || {
            let saved_as = keep_unreadable(&file);
            // Without a copy, the person's file must stay where it is until they decide.
            let cfg = Self {
                file_not_kept: saved_as.is_none(),
                ..Self::default()
            };
            (cfg, ConfigLoad::Unreadable { saved_as })
        };
        match fs::read(&file) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(cfg) => (cfg, ConfigLoad::Loaded),
                Err(_) => unreadable(),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (Self::default(), ConfigLoad::Missing)
            }
            Err(_) => unreadable(),
        }
    }

    /// Saves the settings. A failure is kept for `take_save_failure`, so the window can say it.
    pub fn save(&self) {
        note_save_result(&self.save_to(&data_dir()));
    }

    /// Writes to a temporary file in the same folder, flushes it to the disk, then renames it over
    /// config.json: a crash or a full disk leaves either the old file or the new one, never half.
    pub(crate) fn save_to(&self, dir: &Path) -> std::io::Result<()> {
        if self.file_not_kept {
            // The old file could not be read or copied aside: replacing it would lose it.
            return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        }
        fs::create_dir_all(dir)?;
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = dir.join("config.json.nuevo");
        let written = fs::File::create(&tmp).and_then(|mut f| {
            f.write_all(text.as_bytes())?;
            f.sync_all()
        });
        let done = written.and_then(|()| fs::rename(&tmp, dir.join("config.json")));
        if done.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        done
    }

    /// A bottle's folder was renamed: its settings and the main-bottle choice follow it.
    pub fn bottle_renamed(&mut self, old: &Path, new: &Path) {
        let (old, new) = (
            old.to_string_lossy().to_string(),
            new.to_string_lossy().to_string(),
        );
        if let Some(mut s) = self.bottles.remove(&old) {
            s.rebase_programs(Path::new(&old), Path::new(&new));
            self.bottles.insert(new.clone(), s);
        }
        if self.default_bottle.as_deref() == Some(old.as_str()) {
            self.default_bottle = Some(new);
        }
    }

    /// Drops the settings of bottles that were inside `dir` and whose folder is gone (moved to
    /// the Trash from Finder, say), so a new bottle with the same name starts clean. Does
    /// nothing when `dir` itself is missing: an unmounted disk is not a deleted bottle. Returns
    /// whether anything changed.
    pub fn forget_missing_in(&mut self, dir: &Path, exists: impl Fn(&Path) -> bool) -> bool {
        if !exists(dir) {
            return false;
        }
        let gone: Vec<String> = self
            .bottles
            .keys()
            .filter(|k| Path::new(k.as_str()).starts_with(dir) && !exists(Path::new(k.as_str())))
            .cloned()
            .collect();
        for k in &gone {
            self.bottle_removed(Path::new(k));
        }
        !gone.is_empty()
    }

    /// A bottle is gone (deleted, or never finished): its settings leave, and it is no longer
    /// the main bottle.
    pub fn bottle_removed(&mut self, path: &Path) {
        let key = path.to_string_lossy().to_string();
        self.bottles.remove(&key);
        if self.default_bottle.as_deref() == Some(key.as_str()) {
            self.default_bottle = None;
        }
    }

    pub fn bottle(&mut self, path: &Path) -> &mut BottleSettings {
        self.bottles
            .entry(path.to_string_lossy().to_string())
            .or_default()
    }
    pub fn bottle_ref(&self, path: &Path) -> Option<&BottleSettings> {
        self.bottles.get(&path.to_string_lossy().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bl-settings-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample() -> Config {
        let mut cfg = Config {
            wine_path: Some("/x/wine".into()),
            default_bottle: Some("/b/uno".into()),
            ..Config::default()
        };
        cfg.bottle(Path::new("/b/uno")).hidden.push("Algo".into());
        cfg
    }

    #[test]
    fn save_then_load_keeps_everything_and_leaves_no_temp_file() {
        let dir = temp("roundtrip");
        sample().save_to(&dir).unwrap();
        assert!(!dir.join("config.json.nuevo").exists());
        let (cfg, how) = Config::load_from(&dir);
        assert_eq!(how, ConfigLoad::Loaded);
        assert_eq!(cfg.wine_path.as_deref(), Some("/x/wine"));
        assert_eq!(
            cfg.bottle_ref(Path::new("/b/uno")).unwrap().hidden,
            ["Algo"]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// A save replaces the old file whole, also when a stale temporary file is lying there.
    #[test]
    fn a_save_replaces_the_file_whole() {
        let dir = temp("replace");
        fs::write(dir.join("config.json"), "{}").unwrap();
        fs::write(dir.join("config.json.nuevo"), "half").unwrap();
        sample().save_to(&dir).unwrap();
        let text = fs::read_to_string(dir.join("config.json")).unwrap();
        assert!(text.contains("/x/wine"));
        assert!(!dir.join("config.json.nuevo").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_save_says_so_and_leaves_the_old_file() {
        let dir = temp("failed");
        sample().save_to(&dir).unwrap();
        let before = fs::read_to_string(dir.join("config.json")).unwrap();
        // A folder where the temporary file goes makes the write fail.
        fs::remove_file(dir.join("config.json.nuevo")).ok();
        fs::create_dir(dir.join("config.json.nuevo")).unwrap();
        assert!(Config::default().save_to(&dir).is_err());
        assert_eq!(fs::read_to_string(dir.join("config.json")).unwrap(), before);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_is_just_the_defaults() {
        let dir = temp("missing");
        let (cfg, how) = Config::load_from(&dir);
        assert_eq!(how, ConfigLoad::Missing);
        assert!(cfg.mac_launchers && cfg.bottles.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    /// An unreadable file is copied under a dated name before the defaults can overwrite it.
    #[test]
    fn an_unreadable_file_is_kept_aside_and_the_defaults_start() {
        let dir = temp("broken");
        fs::write(dir.join("config.json"), "{\"wine_path\": \"/x/w").unwrap();
        let (cfg, how) = Config::load_from(&dir);
        assert!(cfg.wine_path.is_none());
        let ConfigLoad::Unreadable {
            saved_as: Some(copy),
        } = how
        else {
            panic!("expected a saved copy");
        };
        assert!(copy
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("config.json.ilegible-"));
        assert_eq!(fs::read_to_string(&copy).unwrap(), "{\"wine_path\": \"/x/w");
        // The next save writes the new file and leaves the copy alone.
        cfg.save_to(&dir).unwrap();
        assert!(copy.is_file());
        // A second unreadable file gets its own copy, never the same name.
        fs::write(dir.join("config.json"), "nope").unwrap();
        let (_, again) = Config::load_from(&dir);
        let ConfigLoad::Unreadable {
            saved_as: Some(second),
        } = again
        else {
            panic!("expected a second copy");
        };
        assert_ne!(copy, second);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Looking at the same broken file again does not pile up copies.
    #[test]
    fn the_same_broken_file_is_copied_once() {
        let dir = temp("broken-twice");
        fs::write(dir.join("config.json"), "{ broken").unwrap();
        let (_, first) = Config::load_from(&dir);
        let (_, second) = Config::load_from(&dir);
        assert_eq!(first, second);
        let copies = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("ilegible"))
            .count();
        assert_eq!(copies, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    /// When the unreadable file could not be kept, a save leaves it where it is.
    #[test]
    fn a_save_never_replaces_a_file_that_was_not_kept() {
        let dir = temp("not-kept");
        fs::write(dir.join("config.json"), "{ precious").unwrap();
        let cfg = Config {
            file_not_kept: true,
            ..Config::default()
        };
        assert!(cfg.save_to(&dir).is_err());
        assert_eq!(
            fs::read_to_string(dir.join("config.json")).unwrap(),
            "{ precious"
        );
        assert!(!dir.join("config.json.nuevo").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// A file written by an older version, without the newer fields, still reads.
    #[test]
    fn an_older_file_without_newer_fields_still_reads() {
        let dir = temp("older");
        fs::write(
            dir.join("config.json"),
            r#"{"wine_path":"/w","bottles":{}}"#,
        )
        .unwrap();
        let (cfg, how) = Config::load_from(&dir);
        assert_eq!(how, ConfigLoad::Loaded);
        assert_eq!(cfg.wine_path.as_deref(), Some("/w"));
        assert!(cfg.mac_launchers);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A file written by an older version still has fields that no longer exist (a switch of a
    /// removed feature, the origin of a copy) and recipe ids that older versions recorded. It
    /// reads without error, the other settings are kept, and saving it again drops what is gone.
    #[test]
    fn a_file_with_removed_fields_reads_and_keeps_the_rest() {
        let dir = temp("removed-fields");
        fs::write(
            dir.join("config.json"),
            r#"{"wine_path":"/w","mostrar_botellas_de_otra_app":true,"mac_launchers":false,
                "default_bottle":"/b/Steam (copia)","games_offered":true,
                "bottles":{"/b/Steam (copia)":{"msync":false,"windows":"win11",
                "recipes":["com.example.old.206"],"last_wine":"wine-11.0",
                "copied_from":"/otra/Steam","campo_nuevo_de_otra_version":1}}}"#,
        )
        .unwrap();
        let (cfg, how) = Config::load_from(&dir);
        assert_eq!(how, ConfigLoad::Loaded);
        assert_eq!(cfg.wine_path.as_deref(), Some("/w"));
        assert!(!cfg.mac_launchers && cfg.games_offered);
        assert_eq!(cfg.default_bottle.as_deref(), Some("/b/Steam (copia)"));
        let b = cfg.bottles.get("/b/Steam (copia)").expect("the bottle");
        assert_eq!(b.msync, Some(false));
        assert_eq!(b.windows.as_deref(), Some("win11"));
        assert_eq!(b.recipes, ["com.example.old.206"]);
        assert_eq!(b.last_wine.as_deref(), Some("wine-11.0"));
        cfg.save_to(&dir).unwrap();
        let saved = fs::read_to_string(dir.join("config.json")).unwrap();
        assert!(!saved.contains("copied_from") && !saved.contains("mostrar_botellas"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_saved_copy_name_has_a_readable_date() {
        assert_eq!(dated_suffix(0), "19700101-000000");
        assert_eq!(dated_suffix(1_700_000_000), "20231114-221320");
        assert_eq!(dated_suffix(951_782_400 + 3_661), "20000229-010101");
    }

    /// Renaming moves the settings and the main bottle; deleting clears both.
    #[test]
    fn rename_and_removal_keep_settings_and_default_in_step() {
        let mut cfg = sample();
        cfg.bottle_renamed(Path::new("/b/uno"), Path::new("/b/dos"));
        assert!(cfg.bottle_ref(Path::new("/b/uno")).is_none());
        assert_eq!(
            cfg.bottle_ref(Path::new("/b/dos")).unwrap().hidden,
            ["Algo"]
        );
        assert_eq!(cfg.default_bottle.as_deref(), Some("/b/dos"));
        // Another bottle's rename does not touch the main bottle.
        cfg.bottle_renamed(Path::new("/b/otra"), Path::new("/b/tres"));
        assert_eq!(cfg.default_bottle.as_deref(), Some("/b/dos"));
        cfg.bottle_removed(Path::new("/b/dos"));
        assert!(cfg.bottles.is_empty() && cfg.default_bottle.is_none());
    }

    /// Saved programs follow their bottle when it is renamed, and only those inside its folder.
    #[test]
    fn rename_rebases_saved_programs_inside_the_bottle() {
        let mut cfg = Config::default();
        let s = cfg.bottle(Path::new("/b/uno"));
        for path in [
            "/b/uno/drive_c/Games/x.exe",
            "/b/uno-2/drive_c/y.exe",
            "/otro/z.exe",
        ] {
            s.programs.push(SavedProgram {
                name: "p".into(),
                path: path.into(),
                args: String::new(),
                env: vec![],
            });
        }
        cfg.bottle_renamed(Path::new("/b/uno"), Path::new("/b/dos"));
        let paths: Vec<&str> = cfg.bottles["/b/dos"]
            .programs
            .iter()
            .map(|p| p.path.as_str())
            .collect();
        assert_eq!(
            paths,
            [
                "/b/dos/drive_c/Games/x.exe",
                "/b/uno-2/drive_c/y.exe",
                "/otro/z.exe"
            ]
        );
    }

    /// Settings of a folder that is gone are dropped, but only under the folder given and only
    /// when that folder is there.
    #[test]
    fn settings_of_missing_bottle_folders_are_forgotten() {
        let mut cfg = Config::default();
        cfg.bottle(Path::new("/own/viva")).msync = Some(true);
        cfg.bottle(Path::new("/own/perdida")).msync = Some(true);
        cfg.bottle(Path::new("/fuera/otra")).msync = Some(true);
        cfg.default_bottle = Some("/own/perdida".into());
        let there = |p: &Path| p == Path::new("/own") || p == Path::new("/own/viva");
        // The folder that holds the bottles is not there: nothing is touched.
        assert!(!cfg.clone().forget_missing_in(Path::new("/own"), |_| false));
        assert!(cfg.forget_missing_in(Path::new("/own"), there));
        assert!(cfg.bottle_ref(Path::new("/own/viva")).is_some());
        assert!(cfg.bottle_ref(Path::new("/own/perdida")).is_none());
        assert!(cfg.bottle_ref(Path::new("/fuera/otra")).is_some());
        assert!(cfg.default_bottle.is_none());
        assert!(!cfg.forget_missing_in(Path::new("/own"), there));
    }

    /// A config.json written before programs were removed by file still loads, with nothing
    /// removed by file and every name it had.
    #[test]
    fn an_old_config_loads_with_no_hidden_files() {
        let old = r#"{"wine_path":null,"bottles":{"/b/uno":{"msync":null,"windows":null,"hidden":["Algo"],"programs":[{"name":"P","path":"/b/uno/p.exe"}]}}}"#;
        let cfg: Config = serde_json::from_str(old).unwrap();
        let s = cfg.bottle_ref(Path::new("/b/uno")).unwrap();
        assert!(s.hidden_files.is_empty());
        assert_eq!(s.hidden, ["Algo"]);
        assert_eq!(s.programs.len(), 1);
    }

    /// A removed file follows the bottle when it is renamed or copied: it is written relative.
    #[test]
    fn hidden_files_are_relative_to_the_bottle_and_follow_a_rename() {
        let mut cfg = Config::default();
        let s = cfg.bottle(Path::new("/b/uno"));
        s.hide_file(Path::new("/b/uno"), Path::new("/b/uno/drive_c/Menu/A.lnk"));
        assert_eq!(s.hidden_files, ["drive_c/Menu/A.lnk"]);
        cfg.bottle_renamed(Path::new("/b/uno"), Path::new("/b/dos"));
        let s = cfg.bottle_ref(Path::new("/b/dos")).unwrap();
        assert!(s.hides_file(Path::new("/b/dos"), Path::new("/b/dos/drive_c/Menu/A.lnk")));
        assert!(!s.hides_file(Path::new("/b/dos"), Path::new("/b/dos/drive_c/Menu/B.lnk")));
    }

    /// A run of failed saves is told once; a save that works starts the count again.
    #[test]
    fn a_failed_save_is_reported_once_per_run_of_failures() {
        let fail = || Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        let _ = take_save_failure();
        note_save_result(&Ok(()));
        note_save_result(&fail());
        note_save_result(&fail());
        let first = take_save_failure().unwrap();
        assert!(first.starts_with("No se pudo guardar tu configuración"));
        assert!(first.contains("permiso"));
        assert!(take_save_failure().is_none());
        note_save_result(&fail());
        assert!(take_save_failure().is_none());
        note_save_result(&Ok(()));
        note_save_result(&fail());
        assert!(take_save_failure().is_some());
        note_save_result(&Ok(()));
    }

    /// Loads a config.json holding `appearance_json` as the value of `appearance` through the
    /// real loader, and says whether the file was read as it is.
    fn load_appearance(name: &str, appearance_json: &str) -> (Appearance, ConfigLoad) {
        let dir = temp(name);
        fs::write(
            dir.join("config.json"),
            format!(r#"{{"wine_path":"/w","appearance":{appearance_json}}}"#),
        )
        .unwrap();
        let (cfg, how) = Config::load_from(&dir);
        // Whatever the value, the rest of the file is still read and nothing is set aside.
        if how == ConfigLoad::Loaded {
            assert_eq!(cfg.wine_path.as_deref(), Some("/w"));
        }
        let aside = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("ilegible"))
            .count();
        assert_eq!(aside, 0, "nothing was backed up");
        let _ = fs::remove_dir_all(&dir);
        (cfg.appearance, how)
    }

    #[test]
    fn a_file_without_appearance_follows_the_mac() {
        let cfg: Config = serde_json::from_str(r#"{"wine_path":"/w"}"#).unwrap();
        assert_eq!(cfg.appearance, Appearance::System);
        assert_eq!(Config::default().appearance, Appearance::System);
    }

    #[test]
    fn each_appearance_is_saved_as_a_lowercase_word_and_read_back() {
        for (value, word) in [
            (Appearance::System, "system"),
            (Appearance::Light, "light"),
            (Appearance::Dark, "dark"),
        ] {
            let dir = temp(&format!("appearance-{word}"));
            let cfg = Config {
                appearance: value,
                ..Config::default()
            };
            cfg.save_to(&dir).unwrap();
            let text = fs::read_to_string(dir.join("config.json")).unwrap();
            assert!(
                text.contains(&format!("\"appearance\": \"{word}\"")),
                "{text}"
            );
            let (back, how) = Config::load_from(&dir);
            assert_eq!(how, ConfigLoad::Loaded);
            assert_eq!(back.appearance, value);
            let _ = fs::remove_dir_all(&dir);
        }
    }

    /// A word nobody offers, a misspelling, another case or a value of another type never makes
    /// the file unreadable: it reads as «like the Mac» and the file is not backed up or reset.
    #[test]
    fn a_strange_appearance_falls_back_to_the_mac_and_never_breaks_the_file() {
        for (i, json) in [
            r#""sepia""#,
            r#""dakr""#,
            r#""Dark""#,
            r#""""#,
            "null",
            "true",
            "3",
            "2.5",
            r#"["dark"]"#,
            r#"{"dark":true}"#,
        ]
        .into_iter()
        .enumerate()
        {
            let (appearance, how) = load_appearance(&format!("odd-appearance-{i}"), json);
            assert_eq!(how, ConfigLoad::Loaded, "{json}");
            assert_eq!(appearance, Appearance::System, "{json}");
        }
        // The exact words still count.
        assert_eq!(load_appearance("ok-dark", r#""dark""#).0, Appearance::Dark);
        assert_eq!(
            load_appearance("ok-light", r#""light""#).0,
            Appearance::Light
        );
    }
}
