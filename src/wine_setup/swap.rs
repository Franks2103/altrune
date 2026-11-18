//! Putting a new engine folder in place of the old one, and clearing what interrupted installs
//! left behind.

use super::*;
use crate::backend::{data_dir, io_error_logged, Log};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

/// The folder of the app's own data where a Wine package is unpacked before it is swapped in.
pub(super) const STAGING: &str = "Wine.nuevo";

/// Where an engine's old copy waits while its replacement is put in place.
fn backup_of(dest: &Path) -> std::path::PathBuf {
    let mut name = dest.as_os_str().to_os_string();
    name.push(".anterior");
    std::path::PathBuf::from(name)
}

/// An engine folder that was swapped for a new one, with the old copy kept aside until the new
/// one is known to work.
pub(super) struct Swap {
    dest: std::path::PathBuf,
    /// The old copy, when there was one.
    backup: Option<std::path::PathBuf>,
}

impl Swap {
    /// The new copy works: the old one is deleted. It is the app's own engine folder, in the
    /// app's own folder, and the app can download it again, so it does not go to the Trash.
    pub(super) fn keep_new(self) {
        if let Some(b) = self.backup {
            let _ = std::fs::remove_dir_all(b);
        }
    }

    /// The new copy does not work: it goes, and the old one is back as it was. Returns whether
    /// there was an old one to put back.
    pub(super) fn restore_old(self) -> bool {
        let _ = std::fs::remove_dir_all(&self.dest);
        match self.backup {
            Some(b) => std::fs::rename(b, &self.dest).is_ok(),
            None => false,
        }
    }
}

/// Puts the folder `new` where `dest` is by renames: the old copy is renamed aside, never deleted,
/// so if anything goes wrong it is put back and the engine stays as it was. A rename that fails
/// is told to the person in plain words and its technical detail goes to the log.
pub(super) fn swap_in(new: &Path, dest: &Path, log: &Log) -> Result<Swap, String> {
    recover_in(dest);
    let backup = backup_of(dest);
    let had_old = dest.exists();
    if had_old {
        // `dest` exists, so a backup that is still here is a leftover of an earlier swap.
        let _ = std::fs::remove_dir_all(&backup);
        std::fs::rename(dest, &backup)
            .map_err(|e| io_error_logged(&e, log, "Apartar el motor anterior", dest))?;
    }
    if let Err(e) = std::fs::rename(new, dest) {
        if had_old {
            let _ = std::fs::rename(&backup, dest);
        }
        return Err(io_error_logged(&e, log, "Poner el motor nuevo", dest));
    }
    Ok(Swap {
        dest: dest.to_path_buf(),
        backup: had_old.then_some(backup),
    })
}

/// A swap that was interrupted between its two renames left no engine at `dest` but the old one
/// in its backup: it comes back. (A backup next to an engine is only a leftover.)
fn recover_in(dest: &Path) {
    let backup = backup_of(dest);
    if !dest.exists() && backup.exists() {
        let _ = std::fs::rename(&backup, dest);
    }
}

/// Puts back any engine whose swap was interrupted (the app was closed in the middle) and
/// clears what an interrupted install left in the app's temporary places. Runs on a worker when
/// the app starts.
pub fn recover_interrupted_installs() {
    for engine in [Engine::Standard, Engine::Games, Engine::Metal] {
        recover_in(&managed_dir().join(engine.app_name()));
    }
    sweep_once(&SWEPT, &data_dir(), &managed_dir(), SystemTime::now());
}

/// Whether this run of the app already swept the leftovers of earlier runs.
static SWEPT: AtomicBool = AtomicBool::new(false);

/// Sweeps the leftovers of earlier runs, and only the first time. The recovery runs again every
/// time the app looks for Wine, which can happen in the middle of this run's own install: the
/// first package of a two-package install sits idle while the second one downloads, long enough
/// to look stale, and deleting it would fail the install. Whatever an earlier run left is there
/// at the start, so once is enough.
fn sweep_once(done: &AtomicBool, data: &Path, managed: &Path, now: SystemTime) -> usize {
    if done.swap(true, Ordering::SeqCst) {
        return 0;
    }
    sweep_leftovers(data, managed, now)
}

/// A download that was not written to for this long is not running any more (curl writes all
/// the time while it downloads).
const STALE_FILE: Duration = Duration::from_secs(10 * 60);

/// A staging folder nothing was added to for this long was left by an install that ended: the
/// unpacking of the largest package takes a couple of minutes.
const STALE_STAGING: Duration = Duration::from_secs(60 * 60);

/// When something in `path` (or the folder itself) was last changed.
fn last_change(path: &Path) -> Option<SystemTime> {
    let own = std::fs::symlink_metadata(path).ok()?.modified().ok()?;
    let inside = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max();
    Some(inside.map_or(own, |i| i.max(own)))
}

fn older_than(path: &Path, age: Duration, now: SystemTime) -> bool {
    last_change(path)
        .and_then(|t| now.duration_since(t).ok())
        .is_some_and(|d| d >= age)
}

/// The staging folders inside the Metal engine's own folder: the Direct3D 12 one is only left
/// by older versions of the app, the video one by `add_video`.
const METAL_STAGING_FOLDERS: [&str; 2] = ["d3dmetal.nuevo", "gstreamer.nuevo"];

/// Removes what an install that was interrupted (the app closed, a crash) left behind: the
/// files in the download folder, the staging folders an unpacking works in (the Wine one, the
/// Metal engine's and the two inside the Metal engine's folder) and the `.anterior` copy of an
/// engine whose replacement was put in place and never confirmed. Only these names are touched,
/// and only when nothing was written to them for a while, so an install that is running now
/// (this app's, or another window's) keeps its files. Bottles and engines are never looked at.
/// A backup is removed only when its engine is there and has not changed for as long as the
/// backup itself: an engine that was swapped in a moment ago is fresh, and a backup without its
/// engine is what [`recover_in`] puts back. A link is skipped rather than followed. Returns how
/// many entries were removed.
pub(super) fn sweep_leftovers(data: &Path, managed: &Path, now: SystemTime) -> usize {
    let mut removed = 0;
    let mut remove = |path: &Path, age: Duration| {
        let is_link = std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
        if is_link || !older_than(path, age, now) {
            return;
        }
        let gone = if path.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        removed += usize::from(gone.is_ok());
    };
    // `read_dir` follows a link, so a download folder somebody pointed elsewhere is not listed:
    // what is in there is not the app's.
    let downloads = data.join(DOWNLOADS);
    let downloads_is_link =
        std::fs::symlink_metadata(&downloads).is_ok_and(|m| m.file_type().is_symlink());
    if let Ok(entries) = std::fs::read_dir(&downloads) {
        if !downloads_is_link {
            for e in entries.flatten() {
                remove(&e.path(), STALE_FILE);
            }
        }
    }
    remove(&data.join(STAGING), STALE_STAGING);
    let metal = managed.join(Engine::Metal.app_name());
    remove(
        &managed.join(format!("{}.nuevo", Engine::Metal.app_name())),
        STALE_STAGING,
    );
    for name in METAL_STAGING_FOLDERS {
        remove(&metal.join(name), STALE_STAGING);
    }
    for engine in [Engine::Standard, Engine::Games, Engine::Metal] {
        let dest = managed.join(engine.app_name());
        if dest.exists() && older_than(&dest, STALE_STAGING, now) {
            remove(&backup_of(&dest), STALE_STAGING);
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wine_setup::tests::support::scratch;
    use std::fs;

    fn engine(dir: &Path, marker: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("version"), marker).unwrap();
    }

    fn version(dir: &Path) -> String {
        fs::read_to_string(dir.join("version")).unwrap()
    }

    /// The old engine stays in place until the new one is known to work, and goes after that.
    #[test]
    fn the_old_engine_goes_only_after_the_new_one_works() {
        let root = scratch("keep");
        let (old, new) = (root.join("Wine Stable.app"), root.join("Wine.nuevo"));
        engine(&old, "viejo");
        engine(&new, "nuevo");
        let swap = swap_in(&new, &old, &Log::default()).unwrap();
        assert_eq!(version(&old), "nuevo");
        assert_eq!(version(&backup_of(&old)), "viejo");
        swap.keep_new();
        assert_eq!(version(&old), "nuevo");
        assert!(!backup_of(&old).exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// A new engine that does not run is removed and the old one is back as it was.
    #[test]
    fn a_new_engine_that_fails_leaves_the_old_one_as_it_was() {
        let root = scratch("rollback");
        let (old, new) = (root.join("Wine Stable.app"), root.join("Wine.nuevo"));
        engine(&old, "viejo");
        engine(&new, "nuevo");
        let swap = swap_in(&new, &old, &Log::default()).unwrap();
        assert!(swap.restore_old());
        assert_eq!(version(&old), "viejo");
        assert!(!backup_of(&old).exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// With no engine before, a failed one leaves none behind, and nothing is lost.
    #[test]
    fn a_first_install_that_fails_leaves_nothing() {
        let root = scratch("first");
        let (dest, new) = (root.join("Wine Stable.app"), root.join("Wine.nuevo"));
        engine(&new, "nuevo");
        let swap = swap_in(&new, &dest, &Log::default()).unwrap();
        assert!(!swap.restore_old());
        assert!(!dest.exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// When the new folder cannot be moved in, the old engine is put back at once.
    #[test]
    fn a_swap_that_cannot_finish_puts_the_old_engine_back() {
        let root = scratch("cannot");
        let old = root.join("Wine Stable.app");
        engine(&old, "viejo");
        let missing = root.join("no-existe");
        assert!(swap_in(&missing, &old, &Log::default()).is_err());
        assert_eq!(version(&old), "viejo");
        assert!(!backup_of(&old).exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// A swap that cannot move the new folder in says so in plain Spanish, as before, and the log
    /// keeps what the sentence leaves out: what was being done, where, and the system's code.
    #[test]
    fn a_swap_that_fails_leaves_its_detail_in_the_log() {
        let root = scratch("detail-missing");
        let old = root.join("Wine Stable.app");
        engine(&old, "viejo");
        let log = Log::default();
        let err = swap_in(&root.join("no-existe"), &old, &log).err().unwrap();
        assert_eq!(err, "No encuentro el archivo o la carpeta.");
        let lines = log.lock().unwrap().join("\n");
        assert!(
            lines.starts_with(crate::diagnostics::DISK_DETAIL_PREFIX),
            "{lines}"
        );
        assert!(lines.contains("Poner el motor nuevo"), "{lines}");
        assert!(lines.contains(&old.display().to_string()), "{lines}");
        assert!(lines.contains("Tipo: NotFound"), "{lines}");
        assert!(lines.contains("Código del sistema: 2."), "{lines}");
        let _ = fs::remove_dir_all(&root);
    }

    /// The old engine that cannot be moved aside (its folder is read-only) is told the same way.
    #[test]
    fn an_engine_that_cannot_be_moved_aside_leaves_its_detail_in_the_log() {
        use std::os::unix::fs::PermissionsExt;
        let root = scratch("detail-locked");
        let managed = root.join("Wine");
        let (old, new) = (managed.join("Wine Stable.app"), root.join("Wine.nuevo"));
        engine(&old, "viejo");
        engine(&new, "nuevo");
        fs::set_permissions(&managed, fs::Permissions::from_mode(0o555)).unwrap();
        let writable_anyway = fs::create_dir(managed.join("prueba")).is_ok();
        let log = Log::default();
        let result = swap_in(&new, &old, &log);
        fs::set_permissions(&managed, fs::Permissions::from_mode(0o755)).unwrap();
        if !writable_anyway {
            assert_eq!(result.err().unwrap(), "No tengo permiso para hacerlo.");
            let lines = log.lock().unwrap().join("\n");
            assert!(lines.contains("Apartar el motor anterior"), "{lines}");
            assert!(lines.contains("Tipo: PermissionDenied"), "{lines}");
            assert!(lines.contains("Código del sistema: 13."), "{lines}");
            assert_eq!(version(&old), "viejo");
        }
        let _ = fs::remove_dir_all(&root);
    }

    /// An app closed between the two renames: the old engine comes back at the next start.
    #[test]
    fn an_interrupted_swap_is_put_back() {
        let root = scratch("interrupted");
        let dest = root.join("Wine Stable.app");
        engine(&backup_of(&dest), "viejo");
        recover_in(&dest);
        assert_eq!(version(&dest), "viejo");
        // A backup beside a working engine is left alone by the recovery.
        engine(&backup_of(&dest), "sobra");
        recover_in(&dest);
        assert_eq!(version(&dest), "viejo");
        // The next swap clears that leftover instead of keeping it as the old engine.
        let new = root.join("Wine.nuevo");
        engine(&new, "nuevo");
        swap_in(&new, &dest, &Log::default()).unwrap().keep_new();
        assert_eq!(version(&dest), "nuevo");
        assert!(!backup_of(&dest).exists());
        let _ = fs::remove_dir_all(&root);
    }

    fn set_age(path: &Path, age: Duration) {
        let when = SystemTime::now() - age;
        let f = fs::File::open(path).unwrap();
        f.set_modified(when).unwrap();
    }

    /// The sweep removes old leftovers of interrupted installs by name and nothing else: not a
    /// download or a staging folder that is still being written, not a bottle, an engine or the
    /// old copy a swap keeps.
    #[test]
    fn the_sweep_removes_only_old_leftovers_of_the_app() {
        let root = scratch("sweep");
        let data = root.join("data");
        let managed = data.join("Wine");
        let hour = Duration::from_secs(3600);
        // Leftovers that are old.
        fs::create_dir_all(data.join("descarga")).unwrap();
        fs::write(data.join("descarga/viejo.tar.xz"), "x").unwrap();
        set_age(&data.join("descarga/viejo.tar.xz"), hour);
        fs::create_dir_all(data.join("Wine.nuevo/Wine Stable.app")).unwrap();
        fs::write(data.join("Wine.nuevo/Wine Stable.app/x"), "x").unwrap();
        for p in [
            "Wine.nuevo/Wine Stable.app/x",
            "Wine.nuevo/Wine Stable.app",
            "Wine.nuevo",
        ] {
            set_age(&data.join(p), 3 * hour);
        }
        fs::create_dir_all(managed.join("Motor DXMT.nuevo/engine")).unwrap();
        for p in ["Motor DXMT.nuevo/engine", "Motor DXMT.nuevo"] {
            set_age(&managed.join(p), 3 * hour);
        }
        // What must stay: a download in progress, things of other names, and the swap's copies.
        fs::write(data.join("descarga/reciente.tar.xz"), "x").unwrap();
        for keep in [
            "Bottles/Mi botella/system.reg",
            "Wine/Wine Stable.app/Contents/x",
            "Wine/Wine Stable.app.anterior/Contents/x",
            "Wine/Motor DXMT/engine/bin/wine",
            "Wine/Motor DXMT.anterior/engine/bin/wine",
            "config.json",
        ] {
            let f = data.join(keep);
            fs::create_dir_all(f.parent().unwrap()).unwrap();
            fs::write(&f, "x").unwrap();
            // Even old, they are none of the sweep's business.
            set_age(&f, 5 * hour);
        }
        let removed = sweep_leftovers(&data, &managed, SystemTime::now());
        assert_eq!(removed, 3);
        assert!(!data.join("descarga/viejo.tar.xz").exists());
        assert!(!data.join("Wine.nuevo").exists());
        assert!(!managed.join("Motor DXMT.nuevo").exists());
        assert!(data.join("descarga/reciente.tar.xz").exists());
        for keep in [
            "Bottles/Mi botella/system.reg",
            "Wine/Wine Stable.app/Contents/x",
            "Wine/Wine Stable.app.anterior/Contents/x",
            "Wine/Motor DXMT/engine/bin/wine",
            "Wine/Motor DXMT.anterior/engine/bin/wine",
            "config.json",
        ] {
            assert!(data.join(keep).exists(), "{keep} must stay");
        }
        // A staging folder that something is still unpacking into is not old, even if the
        // folder itself is.
        fs::create_dir_all(data.join("Wine.nuevo/Wine Stable.app")).unwrap();
        set_age(&data.join("Wine.nuevo"), 3 * hour);
        assert_eq!(sweep_leftovers(&data, &managed, SystemTime::now()), 0);
        assert!(data.join("Wine.nuevo").exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// Makes a folder with one file inside, both `age` old.
    fn old_folder(dir: &Path, age: Duration) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("x"), "x").unwrap();
        set_age(&dir.join("x"), age);
        set_age(dir, age);
    }

    /// The staging folders of Direct3D 12 and video, inside the Metal engine's folder, are
    /// leftovers like the others: removed when old, kept while something still writes to them.
    #[test]
    fn the_sweep_removes_old_staging_folders_inside_the_metal_engine() {
        let root = scratch("sweep-metal");
        let data = root.join("data");
        let managed = data.join("Wine");
        let metal = managed.join("Motor DXMT");
        let hour = Duration::from_secs(3600);
        fs::create_dir_all(metal.join("engine/bin")).unwrap();
        fs::write(metal.join("engine/bin/wine"), "x").unwrap();
        old_folder(&metal.join("d3dmetal.nuevo"), 3 * hour);
        old_folder(&metal.join("gstreamer.nuevo"), 3 * hour);
        let removed = sweep_leftovers(&data, &managed, SystemTime::now());
        assert_eq!(removed, 2);
        assert!(!metal.join("d3dmetal.nuevo").exists());
        assert!(!metal.join("gstreamer.nuevo").exists());
        assert!(metal.join("engine/bin/wine").exists());
        // An unpacking that is still writing is left alone, and so are the finished folders.
        fs::create_dir_all(metal.join("d3dmetal.nuevo")).unwrap();
        fs::write(metal.join("d3dmetal.nuevo/recien"), "x").unwrap();
        fs::create_dir_all(metal.join("gstreamer")).unwrap();
        fs::create_dir_all(metal.join("d3dmetal")).unwrap();
        set_age(&metal.join("d3dmetal.nuevo"), 3 * hour);
        assert_eq!(sweep_leftovers(&data, &managed, SystemTime::now()), 0);
        assert!(metal.join("d3dmetal.nuevo").exists());
        assert!(metal.join("gstreamer").exists() && metal.join("d3dmetal").exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// An `.anterior` copy is a leftover only when its engine is in place and neither of them
    /// changed for hours. One whose engine is missing is for [`recover_in`] to put back, and one
    /// next to an engine that was just swapped in belongs to a swap that may still be running.
    #[test]
    fn the_sweep_removes_an_old_backup_only_next_to_an_old_engine() {
        let root = scratch("sweep-backup");
        let data = root.join("data");
        let managed = data.join("Wine");
        let hour = Duration::from_secs(3600);
        // Old engine, old backup: the unconfirmed leftover.
        old_folder(&managed.join("Motor DXMT"), 5 * hour);
        old_folder(&managed.join("Motor DXMT.anterior"), 5 * hour);
        // Old backup, but its engine is missing: it comes back at the next start.
        old_folder(&managed.join("Wine Stable.app.anterior"), 5 * hour);
        // Old backup next to an engine that was swapped in just now.
        fs::create_dir_all(managed.join("Game Porting Toolkit.app")).unwrap();
        old_folder(&managed.join("Game Porting Toolkit.app.anterior"), 5 * hour);
        let removed = sweep_leftovers(&data, &managed, SystemTime::now());
        assert_eq!(removed, 1);
        assert!(!managed.join("Motor DXMT.anterior").exists());
        assert!(managed.join("Motor DXMT/x").exists());
        assert!(managed.join("Wine Stable.app.anterior").exists());
        assert!(managed.join("Game Porting Toolkit.app.anterior").exists());
        // A backup that is not old is the one a swap is using now.
        old_folder(&managed.join("Motor DXMT.anterior"), Duration::ZERO);
        assert_eq!(sweep_leftovers(&data, &managed, SystemTime::now()), 0);
        assert!(managed.join("Motor DXMT.anterior").exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// A link with a leftover's name is never followed or removed, and a missing folder is fine.
    #[test]
    fn the_sweep_skips_links_and_missing_folders() {
        let root = scratch("sweep-link");
        let (data, elsewhere) = (root.join("data"), root.join("otro"));
        fs::create_dir_all(&data).unwrap();
        assert_eq!(
            sweep_leftovers(&data, &data.join("Wine"), SystemTime::now()),
            0
        );
        fs::create_dir_all(elsewhere.join("dentro")).unwrap();
        fs::write(elsewhere.join("dentro/x"), "x").unwrap();
        std::os::unix::fs::symlink(&elsewhere, data.join("Wine.nuevo")).unwrap();
        set_age(&elsewhere, 3 * Duration::from_secs(3600));
        assert_eq!(
            sweep_leftovers(&data, &data.join("Wine"), SystemTime::now()),
            0
        );
        assert!(elsewhere.join("dentro/x").exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// A download folder somebody replaced with a link to another folder is not the app's, so
    /// what is in it is never removed.
    #[test]
    fn the_sweep_does_not_follow_a_linked_download_folder() {
        let root = scratch("sweep-downloads-link");
        let (data, elsewhere) = (root.join("data"), root.join("otro"));
        fs::create_dir_all(&data).unwrap();
        fs::create_dir_all(&elsewhere).unwrap();
        fs::write(elsewhere.join("foto.png"), "x").unwrap();
        set_age(&elsewhere.join("foto.png"), 5 * Duration::from_secs(3600));
        std::os::unix::fs::symlink(&elsewhere, data.join("descarga")).unwrap();
        assert_eq!(
            sweep_leftovers(&data, &data.join("Wine"), SystemTime::now()),
            0
        );
        assert!(elsewhere.join("foto.png").exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// Looking for Wine again in the middle of an install must not sweep: the first package of
    /// a two-package install sits idle while the second downloads and would look stale.
    #[test]
    fn the_sweep_runs_only_the_first_time() {
        let root = scratch("sweep-once");
        let data = root.join("data");
        let managed = data.join("Wine");
        fs::create_dir_all(data.join("descarga")).unwrap();
        let old = |name: &str| {
            let f = data.join("descarga").join(name);
            fs::write(&f, "x").unwrap();
            set_age(&f, 2 * Duration::from_secs(3600));
            f
        };
        let (first, second) = (old("uno.tar.xz"), old("dos.tar.xz"));
        let done = AtomicBool::new(false);
        assert_eq!(sweep_once(&done, &data, &managed, SystemTime::now()), 2);
        assert!(!first.exists() && !second.exists());
        let package = old("paquete-del-motor.tar.gz");
        assert_eq!(sweep_once(&done, &data, &managed, SystemTime::now()), 0);
        assert!(package.exists(), "a later look must not remove it");
        let _ = fs::remove_dir_all(&root);
    }
}
