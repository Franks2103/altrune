//! Making, copying, renaming, saving and removing bottles.

use crate::backend::*;
use crate::*;

/// What to say when a change is asked of a bottle that another job is working on.
pub(crate) fn busy_text(name: &str) -> String {
    format!("«{name}» está ocupada ahora mismo: espera a que termine.")
}

/// Whether a job holds the bottle at `path`: the words that refuse a new job when one does.
/// Every action that changes a bottle asks this first, so a second job never starts on top of
/// the first (and never frees its mark when it ends).
pub(crate) fn hold_check(
    busy: &std::collections::BTreeMap<PathBuf, String>,
    path: &Path,
    name: &str,
) -> Result<(), String> {
    if busy.contains_key(path) {
        Err(busy_text(name))
    } else {
        Ok(())
    }
}

/// Whether a copy of the bottle may be saved now, with the words of the refusal when it may
/// not: a job holds the bottle, or programs run in it. Like a duplicate, a copy made while Wine
/// writes to the prefix comes out inconsistent, so it waits until they are closed.
pub(crate) fn export_check(
    busy: &std::collections::BTreeMap<PathBuf, String>,
    in_use: bool,
    path: &Path,
    name: &str,
) -> Result<(), String> {
    hold_check(busy, path, name)?;
    if in_use {
        return Err(export_in_use_text(name));
    }
    Ok(())
}

/// Why a copy cannot be saved while programs run in the bottle.
fn export_in_use_text(name: &str) -> String {
    format!(
        "Cierra los programas de «{name}» antes de guardar una copia: copiada abierta, la copia saldría dañada."
    )
}

/// The worker of "save a copy": archives the bottle to `dest`, unless a program runs in it.
pub(crate) fn export_job(wine: Option<&Path>, b: &Bottle, dest: &Path, log: &Log) -> Vec<Msg> {
    // The window's view of what runs is seconds old, and the save dialog may have been open for
    // longer: ask Wine again before archiving a prefix it may be writing to.
    if wine.is_some_and(|w| is_running(w, b)) {
        return vec![
            Msg::Busy(b.path.clone(), None),
            Msg::Fail(Some(b.path.clone()), export_in_use_text(&b.name)),
        ];
    }
    // Written beside the final file and renamed when it is whole, so a failure never leaves a
    // partial .tar.gz that looks like a good copy.
    let partial = partial_path(dest);
    let mut c = std::process::Command::new("/usr/bin/tar");
    // LC_CTYPE=UTF-8 so non-English file names survive.
    c.env("LC_CTYPE", "UTF-8");
    c.arg("-czf").arg(&partial).arg("-C");
    c.arg(b.path.parent().unwrap_or(Path::new("/")))
        .arg(&b.name);
    let ok = run_logged(c, log, "Exportar botella");
    let saved = finish_partial(&partial, dest, ok, log);
    vec![
        Msg::Busy(b.path.clone(), None),
        match saved {
            Ok(()) => Msg::Done("Copia guardada.".into()),
            Err(e) => Msg::Fail(Some(b.path.clone()), e),
        },
    ]
}

/// The tooltip of a control that cannot be used while a job holds the bottle: the job's own
/// words (its trailing «…» dropped, its first letter small) say what is going on.
pub(crate) fn busy_hold_text(job: &str) -> String {
    let job = job.trim_end_matches('…');
    let mut chars = job.chars();
    let job = match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect::<String>(),
        None => String::new(),
    };
    format!("Esta botella está ocupada ({job}): espera a que termine.")
}

/// Who asks for a new bottle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BottleAsker {
    /// The «Nueva botella» dialog's «Crear botella» button.
    Dialog,
    /// The guided installer, which has no dialog open when it needs its bottle.
    Installer,
}

/// Whether a request for a new bottle may go ahead. The dialog's request only counts while that
/// dialog is open, so a repeated action (Enter and a click on the same frame) cannot make a
/// second bottle. The installer's request is made once per session, with no dialog open, and
/// always counts.
pub(crate) fn new_bottle_allowed(asker: BottleAsker, modal: &Option<Modal>) -> bool {
    match asker {
        BottleAsker::Dialog => matches!(modal, Some(Modal::NewBottle)),
        BottleAsker::Installer => true,
    }
}

impl App {
    /// The job label of a bottle whose programs are being closed.
    pub(crate) const CLOSING: &'static str = "Cerrando…";
    /// The label of the app's own job while a bottle is imported.
    pub(crate) const IMPORT_JOB: &'static str = "Importando la botella…";

    /// The bottle at this folder, when it is still in the list.
    pub(crate) fn bottle_at(&self, path: &Path) -> Option<&Bottle> {
        self.bottles.iter().find(|b| b.path == path)
    }

    /// Whether anything is running in the bottle, or being started in it, as far as the window
    /// knows. Reads only what is in memory.
    pub(crate) fn bottle_in_use(&self, path: &Path) -> bool {
        self.live.iter().any(|p| p == path)
            || !self.running_in(path).is_empty()
            || self.opening.keys().any(|(b, _)| b == path)
    }

    /// Says why and answers true when a job holds the bottle: the caller stops there.
    pub(crate) fn refuse_if_held(&mut self, path: &Path, name: &str) -> bool {
        match hold_check(&self.busy, path, name) {
            Ok(()) => false,
            Err(why) => {
                self.say(why);
                true
            }
        }
    }

    /// Why nothing new may be done to this bottle now (the words of the job that holds it), or
    /// `None` when it is free. Reads only what is in memory, so every frame may ask.
    pub(crate) fn busy_reason(&self, path: &Path) -> Option<String> {
        self.busy.get(path).map(|job| busy_hold_text(job))
    }

    pub(crate) fn create_bottle(&mut self, asker: BottleAsker) {
        if !new_bottle_allowed(asker, &self.modal) {
            return;
        }
        if self.wine.is_none() {
            self.modal = Some(Modal::WineHelp);
            return;
        }
        // A new bottle starts on the engine chosen in the dialog.
        let use_games = self.new_games && wine_setup::Engine::Games.installed().is_some();
        let Some(wine) = (if use_games {
            wine_setup::Engine::Games.installed()
        } else {
            self.wine.clone()
        }) else {
            return;
        };
        let path = unique_bottle_path(&self.new_name);
        let windows = self.new_windows.clone();
        self.modal = None;
        // Only a folder this attempt made may be thrown away if the bottle fails.
        let fresh = !path.exists();
        let _ = std::fs::create_dir_all(&path);
        if fresh {
            // Settings left by an older bottle of the same name (its folder went to the Trash
            // from Finder) are not this bottle's: it starts clean.
            self.cfg.bottles.insert(
                path.to_string_lossy().to_string(),
                BottleSettings::default(),
            );
        }
        self.cfg.bottle(&path).windows = Some(windows.clone());
        self.cfg.bottle(&path).msync = Some(true);
        if use_games {
            self.cfg.bottle(&path).engine = Some("games".into());
        }
        self.note_wine(&path, &wine);
        self.cfg.save();
        self.busy
            .insert(path.clone(), crate::domain::status::CREATING.into());
        self.say("Creando la botella, tarda un minuto…");
        let p = path.clone();
        self.task_freeing(OnPanic::creating(&path, fresh), move |log| {
            let r = create_bottle(&wine, &p, &windows, log);
            let ok = r.is_ok();
            let mut msgs = vec![];
            match r {
                Ok(()) => msgs.push(Msg::Done("Botella lista.".into())),
                Err(e) => {
                    // The half-made folder holds nothing of the person's: it goes to the Trash
                    // so it is not listed, and its name is free again.
                    let removed = if fresh {
                        discard_failed_bottle(&p)
                    } else {
                        Ok(())
                    };
                    let mut text = format!("No se pudo crear la botella. {e}");
                    match removed {
                        Ok(()) => {
                            text.push_str(" Se quitó lo que quedó; puedes intentarlo otra vez.")
                        }
                        Err(why) => text.push_str(&format!(
                            " Además no pude quitar la carpeta que quedó: {why}"
                        )),
                    }
                    // The bottle is gone from the list by the time this is shown, so the
                    // error belongs to no bottle.
                    msgs.push(Msg::Created(p.clone(), false));
                    msgs.push(Msg::Fail(None, text));
                }
            }
            if ok {
                msgs.push(Msg::Created(p, true));
            }
            msgs
        });
        self.rescan();
        self.select_path(&path);
    }

    /// The worker making a new bottle panicked: what it left goes to the Trash, off the window
    /// thread, and the bottle leaves the settings like any failed creation. The bottle stays
    /// busy until its folder is out of the list.
    pub(crate) fn discard_half_made(&mut self, path: PathBuf) {
        self.cfg.bottle_removed(&path);
        self.cfg.save();
        // An installation waiting for this bottle stops here, with the reason on its card.
        self.install_bottle_made(&path, false);
        self.task(move |_| {
            let mut msgs = vec![Msg::Busy(path.clone(), None), Msg::Rescan];
            if let Err(why) = discard_failed_bottle(&path) {
                msgs.push(Msg::Fail(
                    None,
                    format!("No pude quitar la carpeta que quedó de la botella: {why}"),
                ));
            }
            msgs
        });
    }

    pub(crate) fn set_default(&mut self) {
        if let Some(b) = self.bottle().cloned() {
            self.cfg.default_bottle = Some(b.path.to_string_lossy().to_string());
            self.cfg.save();
            self.done(format!(
                "«{}» es ahora tu botella principal: ahí se abren los .exe con doble clic.",
                b.name
            ));
        }
    }

    pub(crate) fn rename_bottle(&mut self) {
        let Some(Modal::Rename(path)) = self.modal.clone() else {
            return;
        };
        // The dialog renames the bottle it was opened for, if that one is still there.
        let Some(b) = self.bottle_at(&path).cloned() else {
            self.modal = None;
            self.fail(None, "Esa botella cambió. Vuelve a intentarlo.");
            return;
        };
        if self.refuse_if_held(&b.path, &b.name) {
            return;
        }
        if self
            .install
            .as_ref()
            .is_some_and(|s| s.active() && s.bottle == b.path)
        {
            self.say(format!(
                "Se está instalando un programa en «{}»: espera a que termine para cambiarle el nombre.",
                b.name
            ));
            return;
        }
        // Programs open in the bottle still hold its old path: renaming would break them.
        if self.bottle_in_use(&b.path) {
            self.say(format!(
                "Cierra los programas de «{}» antes de cambiarle el nombre.",
                b.name
            ));
            return;
        }
        let name = self.rename_text.trim().to_string();
        if name == b.name {
            self.modal = None;
            return;
        }
        // A name is accepted only when it is already clean and free.
        if !is_valid_new_bottle_name(&name) {
            self.say("Ese nombre no se puede usar (ya existe o tiene signos no permitidos).");
            return;
        }
        self.modal = None;
        let dest = b.path.with_file_name(&name);
        let was_selected = self.bottle().is_some_and(|x| x.path == b.path);
        match std::fs::rename(&b.path, &dest) {
            Ok(()) => {
                self.bottle_renamed(&b.path, &dest);
                self.rescan();
                if was_selected {
                    self.select_path(&dest);
                }
                self.clear_errors(&b.path);
                self.done(format!("Ahora se llama «{name}»."));
            }
            Err(e) => self.fail(
                Some(&b.path),
                format!(
                    "No se pudo renombrar: {}",
                    io_error_logged(&e, &self.log, "Renombrar la botella", &b.path)
                ),
            ),
        }
    }

    /// The folder of a bottle has a new path: everything that is keyed by the path follows it, in
    /// one place (the settings, the main bottle, and what waits for the bottle).
    pub(crate) fn bottle_renamed(&mut self, old: &Path, new: &Path) {
        self.cfg.bottle_renamed(old, new);
        self.cfg.save();
        if let Some((bp, _)) = self
            .pending_engine_bottle
            .as_mut()
            .filter(|(bp, _)| bp == old)
        {
            *bp = new.to_path_buf();
        }
        if let Some(s) = self.install.as_mut().filter(|s| s.bottle == old) {
            s.bottle = new.to_path_buf();
        }
    }

    pub(crate) fn duplicate_bottle(&mut self) {
        let Some(b) = self.bottle().cloned() else {
            return;
        };
        if self.refuse_if_held(&b.path, &b.name) {
            return;
        }
        // A prefix copied while Wine writes to it comes out broken.
        if self.bottle_in_use(&b.path) {
            self.say(format!(
                "Cierra los programas de «{}» antes de duplicarla: copiada abierta, la copia saldría dañada.",
                b.name
            ));
            return;
        }
        // unique_bottle_name gives "<name>-2", "<name>-3"….
        let dest = unique_bottle_path(&b.name);
        let wine = self.bottle_wine(&b);
        self.busy.insert(b.path.clone(), "Copiando…".into());
        self.say(format!("Duplicando «{}»…", b.name));
        // The copy is made under a hidden name and only listed (and given its settings) when the
        // worker answers that it is complete.
        self.task_freeing(OnPanic::busy([b.path.clone()]), move |log| {
            let result = if wine.as_ref().is_some_and(|w| is_running(w, &b)) {
                Err(format!(
                    "Cierra los programas de «{}» antes de duplicarla.",
                    b.name
                ))
            } else {
                clone_bottle(&b.path, &dest, log)
            };
            vec![
                Msg::Busy(b.path.clone(), None),
                Msg::Duplicated(b.path, dest, result),
            ]
        });
    }

    /// The answer to a duplicate: the copy gets the settings of the original once it is complete.
    pub(crate) fn bottle_duplicated(
        &mut self,
        original: PathBuf,
        copy: PathBuf,
        result: Result<(), String>,
    ) {
        match result {
            Ok(()) => {
                let mut settings = self.cfg.bottle_ref(&original).cloned().unwrap_or_default();
                // Saved programs of the original's own folder point into the copy.
                settings.rebase_programs(&original, &copy);
                self.cfg
                    .bottles
                    .insert(copy.to_string_lossy().to_string(), settings);
                self.cfg.save();
                self.rescan();
                self.select_path(&copy);
                self.clear_errors(&original);
                self.done("Copia lista.");
            }
            Err(e) => self.fail(Some(&original), format!("No se pudo duplicar: {e}")),
        }
    }

    /// The bottle to save a copy of, when a copy may be made now. Otherwise the person is told
    /// why and nothing else happens: this is asked before the save dialog opens.
    pub(crate) fn export_target(&mut self) -> Option<Bottle> {
        let b = self.bottle().cloned()?;
        let in_use = self.bottle_in_use(&b.path);
        // Two saves of the same bottle at once would free its mark when the first ends.
        match export_check(&self.busy, in_use, &b.path, &b.name) {
            Ok(()) => Some(b),
            Err(why) => {
                self.say(why);
                None
            }
        }
    }

    pub(crate) fn export_bottle(&mut self) {
        let Some(b) = self.export_target() else {
            return;
        };
        if let Some(dest) = super::native_dialog(|| {
            rfd::FileDialog::new()
                .set_title("Guardar una copia de la botella")
                .set_file_name(format!("{}.tar.gz", b.name))
                .save_file()
        }) {
            self.busy.insert(b.path.clone(), "Guardando copia…".into());
            self.say(format!("Guardando una copia de «{}»…", b.name));
            let wine = self.bottle_wine(&b);
            self.task_freeing(OnPanic::busy([b.path.clone()]), move |log| {
                export_job(wine.as_deref(), &b, &dest, log)
            });
        }
    }

    pub(crate) fn import_bottle(&mut self) {
        if self.app_job.is_some() {
            self.say("Espera a que termine lo que se está haciendo.");
            return;
        }
        if let Some(f) = super::native_dialog(|| {
            rfd::FileDialog::new()
                .set_title("Elige una copia de botella")
                .add_filter("Copias de botella", &["gz", "tgz", "tar"])
                .pick_file()
        }) {
            self.modal = None;
            // Settings of bottles whose folder is gone must not pass to an imported bottle that
            // takes the freed name. Only a stat per saved bottle, once per import.
            if self
                .cfg
                .forget_missing_in(&own_bottles_dir(), |p| p.exists())
            {
                self.cfg.save();
            }
            self.say(Self::IMPORT_JOB);
            self.app_job = Some(Self::IMPORT_JOB.into());
            self.task_freeing(OnPanic::app_job(), move |log| {
                let result = import_archive(&f, &own_bottles_dir(), log);
                vec![
                    Msg::AppJob(None),
                    Msg::Rescan,
                    match result {
                        Ok(names) if names.len() == 1 => {
                            Msg::Done(format!("Botella importada como «{}».", names[0]))
                        }
                        Ok(names) => Msg::Done(format!("{} botellas importadas.", names.len())),
                        Err(e) => Msg::Fail(None, format!("No se pudo importar la copia. {e}")),
                    },
                ]
            });
        }
    }

    /// Moves the bottle the dialog was opened for to the Trash. Closing its programs and moving
    /// the folder happen on a worker; the list changes when it answers.
    pub(crate) fn delete_bottle(&mut self) {
        let Some(Modal::Delete(path)) = self.modal.take() else {
            self.modal = None;
            return;
        };
        let Some(b) = self.bottle_at(&path).cloned() else {
            self.fail(None, "Esa botella cambió. Vuelve a intentarlo.");
            return;
        };
        if self.refuse_if_held(&b.path, &b.name) {
            return;
        }
        let wine = self.bottle_wine(&b);
        self.mark_closing(&b.path);
        self.busy.insert(b.path.clone(), "Eliminando…".into());
        self.say(format!("Moviendo «{}» a la Papelera…", b.name));
        self.task_freeing(OnPanic::busy([b.path.clone()]), move |log| {
            if let Some(w) = &wine {
                quit_bottle(w, &b, true, log);
            }
            let r = move_to_trash(&b.path);
            vec![
                Msg::Busy(b.path.clone(), None),
                Msg::Deleted(b.path, b.name, r),
            ]
        });
    }

    /// The bottle's folder was moved (or could not be): the list and the settings follow.
    pub(crate) fn bottle_deleted(
        &mut self,
        path: PathBuf,
        name: String,
        result: Result<(), String>,
    ) {
        match result {
            Ok(()) => {
                // Its settings go, and it is no longer the main bottle.
                self.cfg.bottle_removed(&path);
                self.cfg.save();
                if self.bottle().is_some_and(|b| b.path == path) {
                    self.selected = 0;
                }
                self.handed_over.retain(|h| h.bottle != path);
                // An engine that was still downloading for this bottle has nobody to go to now.
                if self
                    .pending_engine_bottle
                    .as_ref()
                    .is_some_and(|(p, _)| *p == path)
                {
                    self.pending_engine_bottle = None;
                }
                self.clear_errors(&path);
                self.rescan();
                self.done(format!("«{name}» se movió a la Papelera."));
            }
            Err(e) => self.fail(Some(&path), format!("No se pudo eliminar: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn busy_text_names_the_bottle() {
        assert_eq!(
            busy_text("Steam (copia)"),
            "«Steam (copia)» está ocupada ahora mismo: espera a que termine."
        );
    }

    /// A bottle that a job holds takes no second job (a second duplicate of a bottle would
    /// pick the same name as the first; a second save would free the first one's mark); another
    /// bottle is free.
    #[test]
    fn a_held_bottle_refuses_a_second_job() {
        let mut busy = BTreeMap::new();
        let held = PathBuf::from("/b/Steam");
        busy.insert(held.clone(), "Copiando…".to_string());
        assert_eq!(hold_check(&busy, &held, "Steam"), Err(busy_text("Steam")));
        assert_eq!(hold_check(&busy, Path::new("/b/otra"), "otra"), Ok(()));
        busy.remove(&held);
        assert_eq!(hold_check(&busy, &held, "Steam"), Ok(()));
    }

    /// A copy of an open bottle is refused like a duplicate is; a free one is allowed.
    #[test]
    fn a_copy_of_a_bottle_with_programs_open_is_refused() {
        let mut busy = BTreeMap::new();
        let path = PathBuf::from("/b/Steam");
        assert_eq!(export_check(&busy, false, &path, "Steam"), Ok(()));
        let why = export_check(&busy, true, &path, "Steam").unwrap_err();
        assert!(why.contains("Cierra los programas de «Steam»"), "{why}");
        // A job that holds the bottle is said first.
        busy.insert(path.clone(), "Copiando…".to_string());
        assert_eq!(
            export_check(&busy, true, &path, "Steam"),
            Err(busy_text("Steam"))
        );
    }

    /// The tooltip of a grey control says what holds the bottle, in the job's own words.
    #[test]
    fn a_held_control_names_the_job() {
        let t = busy_hold_text("Copiando…");
        assert!(t.contains("(copiando)"), "{t}");
        assert!(t.ends_with("espera a que termine."));
        // Only the first letter is made small: «Windows» keeps its capital.
        assert!(busy_hold_text("Cambiando la versión de Windows…").contains("Windows)"));
    }

    /// «Crear botella» only acts from its own dialog: an action that arrives after the dialog
    /// closed (Enter and a click on one frame) does not make a second bottle. The guided
    /// installer has no dialog and must still get its bottle.
    #[test]
    fn a_bottle_is_made_from_its_dialog_or_the_installer() {
        use BottleAsker::{Dialog, Installer};
        assert!(new_bottle_allowed(Dialog, &Some(Modal::NewBottle)));
        assert!(!new_bottle_allowed(Dialog, &None));
        assert!(!new_bottle_allowed(Dialog, &Some(Modal::WineHelp)));
        assert!(!new_bottle_allowed(Dialog, &Some(Modal::Install)));
        assert!(new_bottle_allowed(Installer, &None));
    }
}
