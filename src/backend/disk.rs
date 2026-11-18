//! Disk work the bottles need: copies, imports, exports, the Trash and the plain-words errors.

use super::*;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Copies a bottle for this app's own use. On APFS (the Mac's normal disk) `cp -c` makes a clone:
/// instant, and it takes no extra space until something changes. Other disks get a full copy.
/// The copy is made under a hidden name beside the final one, checked, and only then renamed, so
/// a half-made copy is never listed or used.
pub fn clone_bottle(src: &Path, dest: &Path, log: &Log) -> Result<(), String> {
    if dest.exists() {
        return Err("Ya existe una botella con ese nombre.".into());
    }
    let parent = dest.parent().ok_or("La carpeta de destino no es válida.")?;
    fs::create_dir_all(parent)
        .map_err(|e| io_error_logged(&e, log, "Crear la carpeta de la copia", parent))?;
    let staging = staging_path(parent, "copiando", dest);
    // A leftover of an earlier try (the app was closed in the middle) was never listed.
    discard_staging(&staging);
    let mut c = Command::new("/bin/cp");
    c.arg("-cR").arg(src).arg(&staging);
    if !run_logged(c, log, "Copia instantánea (APFS)") {
        discard_staging(&staging);
        let mut c = Command::new("/bin/cp");
        c.arg("-R").arg(src).arg(&staging);
        if !run_logged(c, log, "Copia completa") {
            discard_staging(&staging);
            return Err("No se pudo copiar la botella (¿falta espacio?).".into());
        }
    }
    finish_staged_copy(src, &staging, dest, log)
}

/// A hidden folder beside `dest` where something is made before it gets its real name. The scan
/// of bottles skips every folder whose name starts with a dot.
pub fn staging_path(parent: &Path, what: &str, dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    parent.join(format!(".{what}-{name}"))
}

/// Gets rid of a staging folder this app made and never showed. It goes to the Trash first; when
/// that is not possible it is removed, because nobody has ever seen or used it.
pub fn discard_staging(staging: &Path) {
    discard_staging_in(staging, &trash_dir());
}

/// What an unfinished copy is called in the Trash. New ones carry the app's present name; the
/// ones an older version left keep theirs.
const UNFINISHED_COPY: &str = "Copia sin terminar de Altrune";

fn discard_staging_in(staging: &Path, trash: &Path) {
    if fs::symlink_metadata(staging).is_err() {
        return;
    }
    if move_into_folder(staging, trash, UNFINISHED_COPY).is_err() {
        let _ = fs::remove_dir_all(staging);
    }
}

/// The copy in `staging` is checked against `src`, then takes the name `dest`. A bottle that
/// already has that name is never replaced. On any failure the staging folder is discarded.
pub fn finish_staged_copy(
    src: &Path,
    staging: &Path,
    dest: &Path,
    log: &Log,
) -> Result<(), String> {
    finish_staged_copy_in(src, staging, dest, &trash_dir(), log)
}

fn finish_staged_copy_in(
    src: &Path,
    staging: &Path,
    dest: &Path,
    trash: &Path,
    log: &Log,
) -> Result<(), String> {
    let checked = verify_copy(src, staging).and_then(|()| {
        if dest.exists() {
            Err("Ya existe una botella con ese nombre.".into())
        } else {
            fs::rename(staging, dest)
                .map_err(|e| io_error_logged(&e, log, "Dar su nombre a la copia", dest))
        }
    });
    if checked.is_err() {
        discard_staging_in(staging, trash);
    }
    checked
}

/// The hidden folder, inside the bottles folder, an archive is unpacked into before it is checked.
pub const IMPORT_STAGING: &str = ".importando";

/// What an unpacked archive holds, checked and moved in: every top-level folder that looks like a
/// bottle gets its own free name (`pick_name` turns the folder's name into one) and is renamed
/// into `bottles`. An existing bottle is never touched. Returns the names the bottles got. Files
/// and folders that are not bottles stay in `staging` for the caller to discard.
pub fn import_staged(
    staging: &Path,
    bottles: &Path,
    pick_name: &dyn Fn(&str) -> String,
    log: &Log,
) -> Result<Vec<String>, String> {
    let mut found: Vec<(String, PathBuf)> = fs::read_dir(staging)
        .map_err(|e| io_error_logged(&e, log, "Leer la copia desempaquetada", staging))?
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
        .filter(|(_, p)| is_bottle_dir(p))
        .collect();
    if found.is_empty() {
        return Err("Esa copia no parece una botella o está incompleta.".into());
    }
    found.sort();
    let mut names: Vec<String> = vec![];
    // A failure after some bottles were already moved in says which ones are there now.
    let failed = |why: String, done: &[String]| {
        if done.is_empty() {
            why
        } else {
            format!("{why} Ya se importó: {}.", done.join(", "))
        }
    };
    for (top, path) in found {
        let name = pick_name(&top);
        let dest = bottles.join(&name);
        if fs::symlink_metadata(&dest).is_ok() {
            return Err(failed(
                "Ya existe una botella con ese nombre.".into(),
                &names,
            ));
        }
        fs::rename(&path, &dest).map_err(|e| {
            failed(
                io_error_logged(&e, log, "Mover la botella importada", &dest),
                &names,
            )
        })?;
        names.push(name);
    }
    Ok(names)
}

/// Unpacks an archive of a bottle into a hidden folder inside `bottles`, checks that it holds a
/// bottle, and gives it a free name. An existing bottle is never touched, and what could not be
/// imported leaves nothing behind that is listed. Returns the names the bottles got.
pub fn import_archive(archive: &Path, bottles: &Path, log: &Log) -> Result<Vec<String>, String> {
    import_archive_in(archive, bottles, &trash_dir(), &free_bottle_name, log)
}

/// The name a bottle called `top` in an archive gets here: cleaned, and free.
fn free_bottle_name(top: &str) -> String {
    let clean = sanitize_bottle_name(top);
    unique_bottle_name(if clean.is_empty() {
        "Botella importada"
    } else {
        &clean
    })
}

fn import_archive_in(
    archive: &Path,
    bottles: &Path,
    trash: &Path,
    pick_name: &dyn Fn(&str) -> String,
    log: &Log,
) -> Result<Vec<String>, String> {
    fs::create_dir_all(bottles)
        .map_err(|e| io_error_logged(&e, log, "Crear la carpeta de botellas", bottles))?;
    let staging = bottles.join(IMPORT_STAGING);
    // What an interrupted import left was never listed: it goes to the Trash.
    discard_staging_in(&staging, trash);
    fs::create_dir_all(&staging)
        .map_err(|e| io_error_logged(&e, log, "Crear la carpeta de importación", &staging))?;
    let mut c = Command::new("/usr/bin/tar");
    c.arg("-xzf").arg(archive).arg("-C").arg(&staging);
    let result = if run_logged(c, log, "Importar botella") {
        import_staged(&staging, bottles, pick_name, log)
    } else {
        Err("No se pudo abrir esa copia. Puede estar incompleta o dañada, o faltar espacio en el disco.".into())
    };
    // The staging folder is empty after a good import (anything else the archive held goes to the
    // Trash with it), and on a failure it holds only the half-unpacked archive.
    if fs::remove_dir(&staging).is_err() {
        discard_staging_in(&staging, trash);
    }
    result
}

/// A file being written next to the place it will end up (`x.tar.gz` → `x.tar.gz.parcial`).
pub fn partial_path(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_os_string();
    name.push(".parcial");
    PathBuf::from(name)
}

/// The end of writing a file through `partial_path`: when it worked the file takes its real
/// name (replacing an older one in a single step); when it did not, the partial file goes.
pub fn finish_partial(partial: &Path, dest: &Path, worked: bool, log: &Log) -> Result<(), String> {
    if worked {
        fs::rename(partial, dest).map_err(|e| {
            let _ = fs::remove_file(partial);
            io_error_logged(&e, log, "Guardar la copia", dest)
        })
    } else {
        let _ = fs::remove_file(partial);
        Err("No se pudo guardar la copia (¿falta espacio?).".into())
    }
}

/// A bottle whose creation failed goes to the Trash. It was made by that attempt and holds no
/// programs or files of the person, but it is moved and not deleted all the same.
pub fn discard_failed_bottle(path: &Path) -> Result<(), String> {
    discard_failed_bottle_in(path, &trash_dir())
}

fn discard_failed_bottle_in(path: &Path, trash: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_err() {
        return Ok(());
    }
    move_into_folder(path, trash, "Botella sin terminar")
}

/// The copy has the parts every bottle needs, and its registry files match the original's.
pub fn verify_copy(src: &Path, dest: &Path) -> Result<(), String> {
    if !dest.join("drive_c").is_dir() {
        return Err("La copia no tiene disco C:.".into());
    }
    for reg in ["system.reg", "user.reg"] {
        let a = fs::metadata(src.join(reg)).map(|m| m.len()).ok();
        let b = fs::metadata(dest.join(reg)).map(|m| m.len()).ok();
        if a != b {
            return Err(format!("La copia de {reg} no coincide con el original."));
        }
    }
    Ok(())
}

pub fn move_to_trash(path: &Path) -> Result<(), String> {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    move_to_trash_as(path, &name)
}

/// Moves a folder to the Trash under a name the person can read there (a hidden name would not
/// show in the Finder).
pub fn move_to_trash_as(path: &Path, name: &str) -> Result<(), String> {
    move_into_folder(path, &trash_dir(), name)
}

fn trash_dir() -> PathBuf {
    home().join(".Trash")
}

/// The part of the Trash move that does not depend on where the Trash is.
fn move_into_folder(path: &Path, trash: &Path, name: &str) -> Result<(), String> {
    let _ = fs::create_dir_all(trash);
    let name = if name.trim_start_matches('.').is_empty() {
        APP_NAME
    } else {
        name.trim_start_matches('.')
    };
    let mut dest = trash.join(name);
    let mut n = 2;
    while fs::symlink_metadata(&dest).is_ok() {
        dest = trash.join(format!("{name} {n}"));
        n += 1;
    }
    fs::rename(path, &dest).map_err(|e| match e.kind() {
        io::ErrorKind::CrossesDevices => {
            "Está en otro disco y no pude moverla a la Papelera. Arrástrala desde el Finder.".into()
        }
        _ => io_error_text(&e),
    })
}

/// What an operating-system error means, in short plain Spanish, for the errors that reach the
/// person (trash moves, exports, copies, imports). The system's own English text never does.
pub fn io_error_text(e: &io::Error) -> String {
    use io::ErrorKind::*;
    let text = match e.kind() {
        NotFound => "No encuentro el archivo o la carpeta.",
        PermissionDenied => "No tengo permiso para hacerlo.",
        StorageFull => "No hay espacio libre en el disco.",
        AlreadyExists => "Ya existe algo con ese nombre.",
        ReadOnlyFilesystem => "El disco es de solo lectura.",
        CrossesDevices => "Está en otro disco.",
        FileTooLarge => "El archivo es demasiado grande para ese disco.",
        InvalidFilename => "Ese nombre no se puede usar.",
        NotADirectory | IsADirectory => "Eso no es lo que esperaba encontrar.",
        TimedOut => "Tardó demasiado en responder.",
        Interrupted => "Se interrumpió.",
        _ => match e.raw_os_error() {
            // ENOSPC, EDQUOT, EROFS, EXDEV, EACCES and EPERM, when the kind is not specific.
            Some(28) | Some(69) => "No hay espacio libre en el disco.",
            Some(30) => "El disco es de solo lectura.",
            Some(18) => "Está en otro disco.",
            Some(13) | Some(1) => "No tengo permiso para hacerlo.",
            _ => "Algo falló al usar el disco.",
        },
    };
    text.into()
}

/// [`io_error_text`] for an error that is about to be shown to the person, which also writes
/// the technical detail the sentence leaves out to the activity log: what was being done, on
/// which path, the kind of the error and the operating system's own code and text. The sentence
/// is the same one; the detail is only for whoever reads the log afterwards.
pub fn io_error_logged(e: &io::Error, log: &Log, what: &str, path: &Path) -> String {
    let code = e
        .raw_os_error()
        .map_or_else(|| "ninguno".to_string(), |c| c.to_string());
    push_tool_log(
        log,
        format!(
            "{}{what} ({}) falló. Tipo: {:?}. Código del sistema: {code}. Texto: {e}",
            crate::diagnostics::DISK_DETAIL_PREFIX,
            path.display(),
            e.kind()
        ),
    );
    io_error_text(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder under a throwaway root, with a bottle-shaped folder in it.
    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bl-recover-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake_bottle(p: &Path, user_reg: &str) {
        fs::create_dir_all(p.join("drive_c/users")).unwrap();
        fs::write(p.join("system.reg"), "system").unwrap();
        fs::write(p.join("user.reg"), user_reg).unwrap();
    }

    /// A failure that reaches the person says it in one plain sentence (the same one as ever),
    /// and the log keeps what the sentence leaves out: what was being done, where, the kind of
    /// the error and the system's own code. Here: a folder cannot be made under a read-only one.
    #[cfg(unix)]
    #[test]
    fn a_disk_failure_leaves_its_detail_in_the_log() {
        use std::os::unix::fs::PermissionsExt;
        let base = scratch("detail");
        let locked = base.join("solo-lectura");
        fs::create_dir_all(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        let dest = locked.join("nueva/Copia");
        let log = Log::default();
        let result = clone_bottle(&base, &dest, &log);
        let writable_anyway = fs::create_dir(locked.join("probe")).is_ok();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        let _ = fs::remove_dir_all(&base);
        if writable_anyway {
            return; // Running as root: the folder was not read-only for us.
        }
        assert_eq!(result.unwrap_err(), "No tengo permiso para hacerlo.");
        let lines = log.lock().unwrap().join("\n");
        assert!(lines.contains("Crear la carpeta de la copia"), "{lines}");
        assert!(lines.contains(&locked.join("nueva").display().to_string()));
        assert!(lines.contains("Tipo: PermissionDenied"), "{lines}");
        assert!(lines.contains("Código del sistema: 13"), "{lines}");
    }

    /// The sentence is the same with or without the detail, and an error with no system code
    /// says so in the log instead of leaving the field out.
    #[test]
    fn the_logged_sentence_is_the_plain_one() {
        let log = Log::default();
        let with_code = io::Error::from_raw_os_error(28);
        let without = io::Error::from(io::ErrorKind::NotFound);
        let path = Path::new("/x/y");
        assert_eq!(
            io_error_logged(&with_code, &log, "Copiar", path),
            io_error_text(&with_code)
        );
        assert_eq!(
            io_error_logged(&without, &log, "Mover", path),
            io_error_text(&without)
        );
        let lines = log.lock().unwrap().clone();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Código del sistema: 28"));
        assert!(lines[1].contains("Código del sistema: ninguno") && lines[1].contains("NotFound"));
    }

    #[test]
    fn os_errors_come_out_in_spanish() {
        use io::ErrorKind::*;
        let words = |k| io_error_text(&io::Error::from(k));
        assert_eq!(words(NotFound), "No encuentro el archivo o la carpeta.");
        assert_eq!(words(PermissionDenied), "No tengo permiso para hacerlo.");
        assert_eq!(words(StorageFull), "No hay espacio libre en el disco.");
        assert_eq!(words(AlreadyExists), "Ya existe algo con ese nombre.");
        // The system's own wording never reaches the person.
        let raw = io::Error::from_raw_os_error(28);
        assert_eq!(io_error_text(&raw), "No hay espacio libre en el disco.");
        assert!(!io_error_text(&io::Error::other("Cross-device link")).contains("link"));
    }

    #[test]
    fn the_trash_move_never_replaces_and_hides_nothing() {
        let root = scratch("trash");
        let trash = root.join("Trash");
        for i in 0..2 {
            let hidden = root.join(format!(".copiando-{i}"));
            fs::create_dir_all(&hidden).unwrap();
            move_into_folder(&hidden, &trash, ".copiando-x").unwrap();
        }
        let mut names: Vec<String> = fs::read_dir(&trash)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(names, ["copiando-x", "copiando-x 2"]);
        let missing = move_into_folder(&root.join("nada"), &trash, "x").unwrap_err();
        assert_eq!(missing, "No encuentro el archivo o la carpeta.");
        let _ = fs::remove_dir_all(&root);
    }

    /// A name made only of dots has nothing to show: the Trash gets the app's present name, never
    /// the old one.
    #[test]
    fn a_nameless_item_goes_to_the_trash_under_the_apps_name() {
        let root = scratch("trash-nameless");
        let trash = root.join("Trash");
        let item = root.join("sin-nombre");
        fs::create_dir_all(&item).unwrap();
        move_into_folder(&item, &trash, "..").unwrap();
        assert!(trash.join("Altrune").is_dir());
        assert!(!trash.join("Botella Libre").exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// A copy takes its real name only after it was checked; a bad one goes to the Trash and the
    /// name stays free.
    #[test]
    fn a_copy_is_listed_only_when_complete() {
        let root = scratch("copy");
        let trash = root.join("Trash");
        let (src, dest) = (root.join("orig"), root.join("orig-2"));
        fake_bottle(&src, "user");
        let staging = staging_path(&root, "copiando", &dest);
        assert_eq!(staging.file_name().unwrap(), ".copiando-orig-2");
        assert!(is_work_in_progress(&staging));
        // A good copy.
        fake_bottle(&staging, "user");
        finish_staged_copy_in(&src, &staging, &dest, &trash, &Log::default()).unwrap();
        assert!(dest.join("drive_c").is_dir() && !staging.exists());
        // A copy whose registry differs from the original is refused and trashed.
        let bad = root.join(".copiando-otra");
        fake_bottle(&bad, "mas largo que el original");
        let err = finish_staged_copy_in(&src, &bad, &root.join("otra"), &trash, &Log::default())
            .unwrap_err();
        assert!(err.contains("user.reg"));
        assert!(!bad.exists() && !root.join("otra").exists());
        assert!(trash.join("Copia sin terminar de Altrune").is_dir());
        // A name that is taken is never replaced.
        let again = root.join(".copiando-orig-2");
        fake_bottle(&again, "user");
        let err = finish_staged_copy_in(&src, &again, &dest, &trash, &Log::default()).unwrap_err();
        assert_eq!(err, "Ya existe una botella con ese nombre.");
        assert!(dest.join("user.reg").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    /// The real copy command goes through the hidden folder and ends with the finished bottle.
    #[test]
    fn cloning_a_bottle_ends_with_a_complete_copy_and_no_leftovers() {
        let root = scratch("clone");
        let (src, dest) = (root.join("orig"), root.join("Bottles/orig (copia)"));
        fake_bottle(&src, "user");
        clone_bottle(&src, &dest, &Log::default()).unwrap();
        assert!(dest.join("drive_c").is_dir());
        let left: Vec<_> = fs::read_dir(root.join("Bottles"))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(left.len(), 1);
        // An existing bottle with that name is refused before anything is copied.
        assert!(clone_bottle(&src, &dest, &Log::default()).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_import_never_touches_an_existing_bottle() {
        let root = scratch("import");
        let (bottles, staging) = (root.join("Bottles"), root.join("Bottles/.importando"));
        fake_bottle(&bottles.join("Mi botella"), "del usuario");
        fake_bottle(&staging.join("Mi botella"), "importada");
        fs::write(staging.join("._Mi botella"), "x").unwrap();
        let pick = |top: &str| {
            let mut n = top.to_string();
            let mut i = 1;
            while bottles.join(&n).exists() {
                i += 1;
                n = format!("{top}-{i}");
            }
            n
        };
        let names = import_staged(&staging, &bottles, &pick, &Log::default()).unwrap();
        assert_eq!(names, ["Mi botella-2"]);
        assert_eq!(
            fs::read_to_string(bottles.join("Mi botella/user.reg")).unwrap(),
            "del usuario"
        );
        assert_eq!(
            fs::read_to_string(bottles.join("Mi botella-2/user.reg")).unwrap(),
            "importada"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// When a later bottle of an archive cannot be imported, the message names the ones that were.
    #[test]
    fn a_partial_import_says_what_is_already_there() {
        let root = scratch("import-partial");
        let (bottles, staging) = (root.join("Bottles"), root.join("Bottles/.importando"));
        fake_bottle(&staging.join("A"), "a");
        fake_bottle(&staging.join("B"), "b");
        // The second name is taken by something the picker did not see.
        fs::create_dir_all(bottles.join("B")).unwrap();
        let err =
            import_staged(&staging, &bottles, &|t| t.to_string(), &Log::default()).unwrap_err();
        assert_eq!(
            err,
            "Ya existe una botella con ese nombre. Ya se importó: A."
        );
        assert!(bottles.join("A").is_dir());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_archive_without_a_bottle_is_refused() {
        let root = scratch("import-none");
        let staging = root.join(".importando");
        fs::create_dir_all(staging.join("fotos/2024")).unwrap();
        fs::write(staging.join("notas.txt"), "x").unwrap();
        let err = import_staged(&staging, &root, &|t| t.to_string(), &Log::default()).unwrap_err();
        assert_eq!(err, "Esa copia no parece una botella o está incompleta.");
        // Nothing was moved out of the staging folder.
        assert!(staging.join("fotos").is_dir());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failed_export_leaves_no_partial_file() {
        let root = scratch("export");
        let dest = root.join("copia.tar.gz");
        let partial = partial_path(&dest);
        assert_eq!(partial.file_name().unwrap(), "copia.tar.gz.parcial");
        fs::write(&partial, "half").unwrap();
        let err = finish_partial(&partial, &dest, false, &Log::default()).unwrap_err();
        assert!(err.contains("falta espacio"));
        assert!(!partial.exists() && !dest.exists());
        // A good one takes the real name, replacing an older file in one step.
        fs::write(&dest, "old").unwrap();
        fs::write(&partial, "new").unwrap();
        finish_partial(&partial, &dest, true, &Log::default()).unwrap();
        assert_eq!(fs::read_to_string(&dest).unwrap(), "new");
        assert!(!partial.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failed_new_bottle_goes_to_the_trash() {
        let root = scratch("failed-bottle");
        let (trash, path) = (root.join("Trash"), root.join("Bottles/Mi botella"));
        fs::create_dir_all(path.join("drive_c")).unwrap();
        discard_failed_bottle_in(&path, &trash).unwrap();
        assert!(!path.exists());
        assert!(trash.join("Botella sin terminar/drive_c").is_dir());
        // Nothing to move is not an error.
        discard_failed_bottle_in(&path, &trash).unwrap();
        let _ = fs::remove_dir_all(&root);
    }

    /// The real unpacking command: a good archive ends as a listed bottle under a free name; a
    /// broken one leaves nothing listed and the existing bottle alone.
    #[test]
    fn importing_an_archive_end_to_end() {
        let root = scratch("import-e2e");
        let (bottles, trash) = (root.join("Bottles"), root.join("Trash"));
        fake_bottle(&bottles.join("Juegos"), "del usuario");
        let src = root.join("origen");
        fake_bottle(&src.join("Juegos"), "importada");
        let archive = root.join("copia.tar.gz");
        let made = Command::new("/usr/bin/tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&src)
            .arg("Juegos")
            .status()
            .unwrap();
        assert!(made.success());
        let pick = |top: &str| format!("{top}-2");
        let names = import_archive_in(&archive, &bottles, &trash, &pick, &Log::default()).unwrap();
        assert_eq!(names, ["Juegos-2"]);
        assert_eq!(
            fs::read_to_string(bottles.join("Juegos/user.reg")).unwrap(),
            "del usuario"
        );
        assert!(!bottles.join(IMPORT_STAGING).exists());
        // A file that is not an archive fails and lists nothing new.
        let junk = root.join("roto.tar.gz");
        fs::write(&junk, "esto no es un archivo comprimido").unwrap();
        let err = import_archive_in(&junk, &bottles, &trash, &pick, &Log::default()).unwrap_err();
        assert!(err.contains("No se pudo abrir esa copia"));
        let listed: Vec<String> = fs::read_dir(&bottles)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(listed.len(), 2, "{listed:?}");
        let _ = fs::remove_dir_all(&root);
    }
}
