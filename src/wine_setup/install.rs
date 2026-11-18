//! Installing the engines and what is added beside them: Rosetta, the two engine installs
//! (the standard Wine, and the Metal engine with its DXMT). Their
//! downloads are in `download.rs` and the swap of an engine folder in `swap.rs`.

use super::*;
use crate::backend::{data_dir, push_log, Log};
use crate::domain::engines::LAUNCHERS;
use std::path::{Path, PathBuf};
use std::process::Command;

fn install_rosetta(log: &Log) -> Result<(), String> {
    // Tests must never ask macOS for an administrator password.
    if cfg!(test) {
        return Err(NO_NETWORK_IN_TESTS.into());
    }
    push_log(log, "▶ Instalando Rosetta (macOS pedirá tu contraseña)");
    let st = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg("do shell script \"/usr/sbin/softwareupdate --install-rosetta --agree-to-license\" with administrator privileges")
        .status()
        .map_err(|e| e.to_string())?;
    if st.success() && !needs_rosetta() {
        Ok(())
    } else {
        Err("Rosetta no se instaló. Sin ella Wine no puede abrirse en este Mac.".into())
    }
}

/// Asks whether Rosetta is missing and keeps the answer in the shared progress, so the window can
/// read it without starting any process while it draws. It is asked again at each install
/// because installing Rosetta changes it.
fn probe_rosetta(p: &Shared) -> bool {
    let needed = needs_rosetta();
    p.lock().unwrap_or_else(|e| e.into_inner()).rosetta_needed = Some(needed);
    needed
}

/// What the Metal engine needs before anything is downloaded: a Mac that can run it, the
/// standard Wine for its Mac libraries (installed first when missing) and Rosetta. The error is
/// the state the install stops with.
fn metal_prerequisites(p: &Shared, log: &Log) -> Result<(), WineState> {
    if !games_engine_supported() {
        return Err(install_failed(
            p,
            log,
            format!("«{LAUNCHERS}» necesita un Mac con chip de Apple y macOS 14 o posterior."),
        ));
    }
    if Engine::Standard.installed().is_none() {
        push_log(
            log,
            format!("▶ «{LAUNCHERS}» usa las bibliotecas del Wine normal: lo instalo primero"),
        );
        if install(p.clone(), log.clone(), Engine::Standard)
            .path()
            .is_none()
        {
            return Err(WineState::Missing);
        }
        *p.lock().unwrap_or_else(|e| e.into_inner()) = Progress {
            running: true,
            ..Default::default()
        };
    }
    if probe_rosetta(p) {
        set_step(p, Step::Rosetta);
        if let Err(e) = install_rosetta(log) {
            return Err(install_failed(p, log, e));
        }
        p.lock().unwrap_or_else(|e| e.into_inner()).rosetta_needed = Some(false);
    }
    Ok(())
}

/// Downloads the two packages of the Metal engine (its Wine and its DXMT) into the app's
/// download folder, each checked against its SHA-256. Returns that folder.
fn download_metal_packages(p: &Shared, log: &Log) -> Result<PathBuf, WineState> {
    set_step(p, Step::Find);
    let packages = [&METAL_WINE, &METAL_LAYER];
    {
        let mut g = p.lock().unwrap_or_else(|e| e.into_inner());
        g.total = packages.iter().map(|k| k.size).sum();
        g.flavour = LAUNCHERS.into();
    }

    set_step(p, Step::Download);
    let tmp = data_dir().join(DOWNLOADS);
    let _ = std::fs::create_dir_all(&tmp);
    let mut before = 0;
    for k in packages {
        download_pinned(p, log, k, &tmp, before)?;
        before += k.size;
    }
    Ok(tmp)
}

/// Unpacks the two downloaded packages and puts the new engine together in a folder beside the
/// old one, ready to be swapped in. Returns that folder.
fn assemble_metal(p: &Shared, log: &Log, tmp: &Path) -> Result<PathBuf, WineState> {
    set_step(p, Step::Unpack);
    let staging = data_dir().join(STAGING);
    let _ = std::fs::remove_dir_all(&staging);
    let mut unpacked = true;
    for (k, sub) in [(&METAL_WINE, "wine"), (&METAL_LAYER, "dxmt")] {
        let dir = staging.join(sub);
        let _ = std::fs::create_dir_all(&dir);
        let file = tmp.join(k.name);
        unpacked &= run_status(
            Command::new("/usr/bin/tar")
                .arg("-xzf")
                .arg(&file)
                .arg("-C")
                .arg(&dir),
        )
        .map(|s| s.success())
        .unwrap_or(false);
        let _ = std::fs::remove_file(&file);
    }
    // The packages unpack to `engine/` (bin, lib, share) and `highball/` (the DXMT libraries).
    let new_wine = staging.join("wine/engine");
    let new_layer = staging.join("dxmt/highball");
    if !unpacked || !new_wine.join("bin/wine").is_file() || !new_layer.is_dir() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(install_failed(
            p,
            log,
            format!("No se pudo descomprimir «{LAUNCHERS}» (¿falta espacio en el disco?)."),
        ));
    }
    let _ = run_status(
        Command::new("/usr/bin/xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(&staging),
    );
    // The new engine is put together beside the old one, then swapped in by renames.
    let assembled = managed_dir().join(format!("{}.nuevo", Engine::Metal.app_name()));
    let _ = std::fs::remove_dir_all(&assembled);
    let moved = std::fs::create_dir_all(&assembled)
        .and_then(|_| std::fs::rename(&new_wine, assembled.join("engine")))
        .and_then(|_| std::fs::rename(&new_layer, assembled.join("dxmt")));
    let _ = std::fs::remove_dir_all(&staging);
    if let Err(e) = moved {
        let _ = std::fs::remove_dir_all(&assembled);
        let why = crate::backend::io_error_logged(&e, log, "Armar el motor", &assembled);
        return Err(install_failed(p, log, why));
    }
    Ok(assembled)
}

/// Installs the Metal engine: its Wine and its DXMT, both checked against their SHA-256, into one
/// folder of the app. The standard Wine is installed first when missing, for its Mac libraries.
fn install_metal(p: Shared, log: Log) -> WineState {
    if let Err(state) = metal_prerequisites(&p, &log) {
        return state;
    }
    let tmp = match download_metal_packages(&p, &log) {
        Ok(tmp) => tmp,
        Err(state) => return state,
    };
    let assembled = match assemble_metal(&p, &log, &tmp) {
        Ok(assembled) => assembled,
        Err(state) => return state,
    };
    let dest = managed_dir().join(Engine::Metal.app_name());
    let swap = match swap_in(&assembled, &dest, &log) {
        Ok(s) => s,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&assembled);
            return install_failed(&p, &log, e);
        }
    };

    set_step(&p, Step::Check);
    let Some(wine) = Engine::Metal.installed() else {
        let kept = swap.restore_old();
        return install_failed(
            &p,
            &log,
            keep_note(
                &format!("El paquete descargado no trae «{LAUNCHERS}»."),
                kept,
            ),
        );
    };
    match probe_logged(&wine, &log) {
        Ok(version) => {
            swap.keep_new();
            push_log(
                &log,
                format!("✔ {version} con DXMT listo en {}", wine.display()),
            );
            let mut g = p.lock().unwrap_or_else(|e| e.into_inner());
            g.step = Some(Step::Done);
            g.running = false;
            WineState::Ready {
                path: wine,
                version,
            }
        }
        Err(e) => {
            let kept = swap.restore_old();
            install_failed(
                &p,
                &log,
                keep_note(
                    &not_started_text(&format!("«{LAUNCHERS}» se instaló, pero no arrancó"), &e),
                    kept,
                ),
            )
        }
    }
}

/// Adds, to what failed, that the engine that was there before is back.
fn keep_note(text: &str, old_is_back: bool) -> String {
    if old_is_back {
        format!("{text} Se dejó el motor que tenías.")
    } else {
        text.to_string()
    }
}

/// The sentence for an engine that was installed but did not start: `first` says what happened
/// (no full stop), `reason` is the plain
/// Spanish reason of [`probe`]; the sentence reads whole, without saying "no arranca" twice.
fn not_started_text(first: &str, reason: &str) -> String {
    let reason = reason.trim();
    let reason = reason
        .strip_prefix("No arranca en este Mac.")
        .unwrap_or(reason)
        .trim()
        .trim_end_matches('.');
    if reason.is_empty() {
        format!("{first}.")
    } else {
        format!("{first}. {reason}.")
    }
}

/// Something added beside the installed Metal engine: downloaded, checked against its pinned
/// SHA-256, unpacked by `add`, then the engine is started once to see it still works.
pub(super) fn install_addition(
    p: Shared,
    log: Log,
    what: &Pinned,
    flavour: &str,
    label: &str,
    add: fn(&Path, &Path, &Log) -> Result<(), String>,
    present: fn(&Path) -> bool,
) -> WineState {
    let Some(wine) = Engine::Metal.installed() else {
        return install_failed(&p, &log, format!("Instala primero «{LAUNCHERS}»."));
    };
    set_step(&p, Step::Find);
    {
        let mut g = p.lock().unwrap_or_else(|e| e.into_inner());
        g.total = what.size;
        g.flavour = flavour.into();
    }

    set_step(&p, Step::Download);
    let tmp = data_dir().join(DOWNLOADS);
    let _ = std::fs::create_dir_all(&tmp);
    let file = match download_pinned(&p, &log, what, &tmp, 0) {
        Ok(file) => file,
        Err(state) => return state,
    };

    set_step(&p, Step::Unpack);
    let added = add(&wine, &file, &log);
    let _ = std::fs::remove_file(&file);
    if let Err(e) = added {
        return install_failed(&p, &log, e);
    }

    set_step(&p, Step::Check);
    match probe_logged(&wine, &log) {
        Ok(version) if present(&wine) => {
            push_log(&log, format!("✔ {label} añadido a {version}"));
            let mut g = p.lock().unwrap_or_else(|e| e.into_inner());
            g.step = Some(Step::Done);
            g.running = false;
            WineState::Ready {
                path: wine,
                version,
            }
        }
        Ok(_) => install_failed(&p, &log, format!("{label} no quedó instalado.")),
        Err(e) => install_failed(
            &p,
            &log,
            not_started_text(
                &format!("{label} se añadió, pero «{LAUNCHERS}» no arrancó"),
                &e,
            ),
        ),
    }
}

/// Finds the release of the standard Wine and downloads it into the app's download
/// folder, checked against the SHA-256 GitHub publishes for it when it gave one. Returns the
/// file; the error is the state the install stops with.
fn download_engine(p: &Shared, log: &Log) -> Result<PathBuf, WineState> {
    set_step(p, Step::Find);
    let asset = match pick_asset(log) {
        Ok(a) => a,
        Err(e) => return Err(install_failed(p, log, e)),
    };
    push_log(log, format!("▶ Descargando {}", asset.name));
    {
        let mut g = p.lock().unwrap_or_else(|e| e.into_inner());
        g.total = asset.size;
        g.flavour = asset.name.split('-').nth(1).unwrap_or("").to_string();
    }

    set_step(p, Step::Download);
    let tmp = data_dir().join(DOWNLOADS);
    let _ = std::fs::create_dir_all(&tmp);
    let file = tmp.join(&asset.name);
    match download(p, &asset.url, &file, asset.size, 0) {
        Ok(()) => {}
        Err(Stopped::Cancelled) => return Err(install_cancelled(p)),
        Err(Stopped::Failed(e)) => return Err(install_failed(p, log, e)),
    }
    // GitHub publishes a SHA-256 for each release file: when it gave one, the download must match.
    if let Some(wanted) = &asset.digest {
        if sha256_of(&file).as_deref() != Some(wanted.as_str()) {
            let _ = std::fs::remove_file(&file);
            return Err(install_failed(
                p,
                log,
                format!(
                    "{} no es la versión que se publicó (llegó dañada o cambiada); no la instalo. Prueba otra vez.",
                    asset.name
                ),
            ));
        }
    }
    Ok(file)
}

/// Downloads and installs a free Wine into the app's folder. Runs on a worker thread. The games
/// engine (Apple's Game Porting Toolkit) is never installed from here.
pub fn install(p: Shared, log: Log, engine: Engine) -> WineState {
    match engine {
        Engine::Metal => return install_metal(p, log),
        Engine::Games => return install_failed(&p, &log, APPLE_SOFTWARE_NOT_INSTALLED.into()),
        Engine::Standard => {}
    }
    let fail = |p: &Shared, msg: String| install_failed(p, &log, msg);

    if probe_rosetta(&p) {
        set_step(&p, Step::Rosetta);
        if let Err(e) = install_rosetta(&log) {
            return fail(&p, e);
        }
        p.lock().unwrap_or_else(|e| e.into_inner()).rosetta_needed = Some(false);
    }

    let file = match download_engine(&p, &log) {
        Ok(file) => file,
        Err(state) => return state,
    };

    set_step(&p, Step::Unpack);
    // Unpack beside the old copy first, then swap, so a failure never leaves a half Wine.
    let staging = data_dir().join(STAGING);
    let _ = std::fs::remove_dir_all(&staging);
    let _ = std::fs::create_dir_all(&staging);
    let st = run_status(
        Command::new("/usr/bin/tar")
            .arg("-xJf")
            .arg(&file)
            .arg("-C")
            .arg(&staging),
    );
    let _ = std::fs::remove_file(&file);
    if !st.map(|s| s.success()).unwrap_or(false) {
        let _ = std::fs::remove_dir_all(&staging);
        return fail(
            &p,
            "No se pudo descomprimir Wine (¿falta espacio en el disco?).".into(),
        );
    }
    let _ = run_status(
        Command::new("/usr/bin/xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(&staging),
    );
    // Move the new engine in, replacing only the same engine (other engines stay installed).
    let Some(new_app) = std::fs::read_dir(&staging).ok().and_then(|rd| {
        rd.flatten()
            .map(|e| e.path())
            .find(|p| p.extension().map(|x| x == "app").unwrap_or(false))
    }) else {
        let _ = std::fs::remove_dir_all(&staging);
        return fail(&p, "El paquete descargado no trae Wine.".into());
    };
    let _ = std::fs::create_dir_all(managed_dir());
    let Some(app_name) = new_app.file_name() else {
        let _ = std::fs::remove_dir_all(&staging);
        return fail(&p, "El paquete descargado no trae Wine.".into());
    };
    let dest = managed_dir().join(app_name);
    // The old copy is renamed aside and only deleted once the new one runs.
    let swap = match swap_in(&new_app, &dest, &log) {
        Ok(s) => s,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return fail(&p, e);
        }
    };
    let _ = std::fs::remove_dir_all(&staging);

    set_step(&p, Step::Check);
    let Some(wine) = wine_in_app(&dest).into_iter().find(|p| p.is_file()) else {
        let kept = swap.restore_old();
        return fail(&p, keep_note("El paquete descargado no trae Wine.", kept));
    };
    match probe_logged(&wine, &log) {
        Ok(version) => {
            swap.keep_new();
            push_log(&log, format!("✔ {version} listo en {}", wine.display()));
            let mut g = p.lock().unwrap_or_else(|e| e.into_inner());
            g.step = Some(Step::Done);
            g.running = false;
            WineState::Ready {
                path: wine,
                version,
            }
        }
        Err(e) => {
            let kept = swap.restore_old();
            fail(
                &p,
                keep_note(
                    &not_started_text("Wine se instaló, pero no arrancó", &e),
                    kept,
                ),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reason of `probe` is its own sentence: "no arranca" is said once and the line ends
    /// in a full stop, whichever reason it is.
    #[test]
    fn an_engine_that_does_not_start_is_told_once() {
        let first = format!("«{LAUNCHERS}» se instaló, pero no arrancó");
        let fallback = probe(Path::new("/no/hay/wine")).unwrap_err();
        for reason in [
            fallback.as_str(),
            "No arranca en este Mac. Los detalles están en el registro de actividad (Ajustes).",
            "Falta Rosetta",
            "Se cierra nada más abrirlo.",
        ] {
            let text = not_started_text(&first, reason);
            assert!(text.starts_with(&first), "{text}");
            assert!(text.ends_with('.') && !text.ends_with(".."), "{text}");
            assert_eq!(
                text.to_lowercase().matches("no arranca").count(),
                0,
                "{text}"
            );
            assert!(!text.contains(": "), "{text}");
        }
        assert_eq!(
            not_started_text(&first, "Falta Rosetta"),
            format!("{first}. Falta Rosetta.")
        );
        assert_eq!(not_started_text(&first, ""), format!("{first}."));
    }
}
