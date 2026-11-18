//! The install flow: what the app does from the moment a file is picked or dropped until the
//! installation is over. It asks, makes the bottle, starts the installer, follows it and closes
//! it; the draft of the dialog it starts from is in `draft`.

use super::draft::{in_list, make_draft, shown_programs, suggested_bottle_name};
use crate::app::programs::StartJob;
use crate::backend::*;
use crate::domain::install::{
    closing_complete, entry, idle_looks, look_wait, starts_wine_install, wine_wait, BottleOption,
    Closing, Entry, InstallRequest, InstallSession, Programs, Stage, Target, WineFacts, WineWait,
    INSTALLER_EXTENSIONS,
};
use crate::*;

impl App {
    // ------------------------------------------------------------ the one way in

    /// The bottle whose page is on screen, when a page is.
    fn page_bottle(&self) -> Option<PathBuf> {
        if self.screen == Screen::Bottle {
            self.bottle().map(|b| b.path.clone())
        } else {
            None
        }
    }

    /// The «Instalar» buttons: ask for the file, then go the same way as any other file.
    pub(crate) fn pick_installer(&mut self) {
        // No point in asking for a file while one installation runs.
        if self.install.as_ref().is_some_and(|s| s.active()) {
            self.show_running_install();
            return;
        }
        if let Some(f) = crate::app::native_dialog(|| {
            rfd::FileDialog::new()
                .set_title("Elige el instalador de Windows")
                .add_filter("Instaladores", &INSTALLER_EXTENSIONS)
                .pick_file()
        }) {
            let page = self.page_bottle();
            self.request_install(f, page);
        }
    }

    pub(crate) fn dropped(&mut self, f: PathBuf) {
        let page = self.page_bottle();
        self.request_install(f, page);
    }

    /// A Windows file from Finder, `--open` or the program picker. `bottle` is the place in the
    /// list of the bottle it came from, if any.
    pub(crate) fn open_directly(&mut self, file: PathBuf, bottle: Option<usize>) {
        let page = bottle
            .and_then(|i| self.bottles.get(i))
            .map(|b| b.path.clone());
        self.request_install(file, page);
    }

    /// Every file that may be installed or opened comes here: the picker, a drop, Finder and the
    /// buttons. It looks at the first bytes of the file, and either shows the running
    /// installation, says why the file cannot be opened, or asks where it goes. Nothing starts
    /// before the person confirms.
    pub(crate) fn request_install(&mut self, file: PathBuf, page: Option<PathBuf>) {
        // A folder or disc: look for its installer (autorun.inf first, then the usual names).
        let file = if file.is_dir() {
            match find_installer_in(&file) {
                Some(inst) => inst,
                None => {
                    self.say("No encontré un instalador en esa carpeta.");
                    return;
                }
            }
        } else {
            file
        };
        if self.modal.is_some() {
            // Kept like a file from Finder: `open_pending` offers it once the window is closed.
            // One file waits at a time, and it is never replaced: the person was told it would
            // open, so a later one is asked to come again instead of taking its place.
            if let Some(waiting) = self.pending_open.as_ref().filter(|w| **w != file) {
                let name = waiting
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.say(format!(
                    "Ya hay un archivo esperando («{name}»). Cuando se abra, vuelve a soltar este."
                ));
                return;
            }
            self.pending_open = Some(file);
            self.say("Abriré ese archivo en cuanto cierres esta ventana.");
            return;
        }
        let detected = classify_file(&file);
        let running = self.install.as_ref().is_some_and(|s| s.active());
        match entry(&detected, running) {
            Entry::Running => self.show_running_install(),
            Entry::Refused(_) => {
                let name = suggested_bottle_name(&file);
                self.install_draft =
                    Some(make_draft(file, detected, None, None, vec![], None, name));
                self.modal = Some(Modal::Install);
            }
            Entry::Ask { .. } => {
                let recipe = recipes::for_installer(&file);
                let known = recipe.as_ref().map(|r| r.name.clone());
                let fixes = recipe
                    .as_ref()
                    .filter(|r| !r.registry.is_empty())
                    .map(|r| r.name.clone());
                // A bottle that never finished being made is not offered: nothing runs in it.
                let options = self
                    .bottles
                    .iter()
                    .filter(|b| self.bottle_status(b).blocks_use().is_none())
                    .map(|b| BottleOption {
                        path: b.path.clone(),
                        name: b.name.clone(),
                        prepared: recipe.as_ref().is_some_and(|r| {
                            self.cfg
                                .bottle_ref(&b.path)
                                .is_some_and(|s| s.recipes.contains(&r.id))
                        }),
                        is_default: self.is_default(b),
                    })
                    .collect();
                let name = suggested_bottle_name(&file);
                let mut draft =
                    make_draft(file, detected, known, fixes, options, page.as_deref(), name);
                draft.recipe_id = recipe.as_ref().map(|r| r.id.clone());
                self.install_draft = Some(draft);
                self.modal = Some(Modal::Install);
            }
        }
    }

    /// The file Finder handed over before Wine was ready, or one dropped while a window was open,
    /// goes through the dialog now. If a window is open, or an installation is running (it takes
    /// one at a time, so the file would only get «ya estoy instalando»), it stays waiting
    /// (`follow_install` tries again), so the file is never lost.
    pub(crate) fn open_pending(&mut self) -> bool {
        if self.wine.is_none() || self.modal.is_some() || self.install_running() {
            return false;
        }
        match self.pending_open.take() {
            // The file may have gone while it waited for Wine or for a dialog to close.
            Some(f) if !f.exists() => {
                self.say(crate::app::instance::missing_text(&f));
                false
            }
            Some(f) => {
                self.open_directly(f, None);
                true
            }
            None => false,
        }
    }

    /// Whether an installation is going on now (its card is not over).
    pub(crate) fn install_running(&self) -> bool {
        self.install.as_ref().is_some_and(|s| s.active())
    }

    /// A second request while one installation runs: show the running one, start nothing.
    fn show_running_install(&mut self) {
        let Some(s) = &self.install else { return };
        let (bottle, file) = (s.bottle.clone(), s.file_name());
        match self.bottles.iter().position(|b| b.path == bottle) {
            Some(i) => {
                self.selected = i;
                self.screen = Screen::Bottle;
                self.tab = BottleTab::Programs;
            }
            None => self.screen = Screen::Home,
        }
        self.say(format!(
            "Ya estoy instalando «{file}»: espera a que termine antes de empezar otro."
        ));
    }

    // ------------------------------------------------------------ from «Instalar» to the installer

    /// The dialog's «Instalar» (or «Abrir»): the installation begins, followed step by step.
    pub(crate) fn confirm_install(&mut self) {
        let Some(d) = self.install_draft.take() else {
            self.modal = None;
            return;
        };
        self.modal = None;
        if !d.detected.supported() {
            return;
        }
        if self.install.as_ref().is_some_and(|s| s.active()) {
            self.show_running_install();
            return;
        }
        let (bottle, name, new_bottle) = match d.target() {
            Target::Existing(path) => match self.bottle_at(&path).cloned() {
                Some(b) => (b.path, b.name, false),
                None => {
                    self.fail(None, "Esa botella cambió. Vuelve a elegir el archivo.");
                    return;
                }
            },
            Target::New(name) => {
                if sanitize_bottle_name(name.trim()).is_empty() {
                    self.say("Escribe un nombre para la botella.");
                    return;
                }
                let path = unique_bottle_path(name.trim());
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                (path, name, true)
            }
        };
        let id = self.install_next_id;
        self.install_next_id += 1;
        let mut session = InstallSession::begin(InstallRequest {
            id,
            file: d.file,
            bottle: bottle.clone(),
            bottle_name: name,
            new_bottle,
            known: d.fixes.filter(|_| d.as_installer),
            installs: d.as_installer,
            wine_ready: self.wine.is_some(),
        });
        session.recipe_id = d.recipe_id.filter(|_| session.known.is_some());
        self.install = Some(session);
        self.install_idle = 0;
        self.install_stopping = false;
        if self.wine.is_none() {
            // Wine is made on the way: the installer opens by itself when it is ready.
            if starts_wine_install(WineFacts {
                ready: false,
                installing: self.installing(),
                looked: self.wine_state != wine_setup::WineState::Unknown,
            }) {
                self.start_wine_install();
            }
            self.say("Lo abriré en cuanto Wine esté listo.");
        }
        match self.bottles.iter().position(|b| b.path == bottle) {
            Some(i) => {
                self.selected = i;
                self.screen = Screen::Bottle;
                self.tab = BottleTab::Programs;
            }
            None => self.screen = Screen::Home,
        }
        self.proceed_install();
        if self.wine.is_some() && self.pending_open.is_some() {
            self.say(
                "El otro archivo que dejaste esperando se abrirá cuando termine esta instalación.",
            );
        }
        // The dialog is gone and the card is there: draw it now, not at the next event.
        self.ctx.request_repaint();
    }

    /// Does what the stage just entered asks for. Called once on each entry, never per frame.
    fn proceed_install(&mut self) {
        match self
            .install
            .as_ref()
            .filter(|s| s.active())
            .map(|s| s.stage.clone())
        {
            Some(Stage::CreatingBottle) => self.make_install_bottle(),
            Some(Stage::Preparing) => self.start_installer(),
            _ => {}
        }
    }

    /// The bottle for this installation is made by the usual path; `Msg::Created` answers.
    fn make_install_bottle(&mut self) {
        let Some(s) = self.install.as_mut() else {
            return;
        };
        let path = unique_bottle_path(&s.bottle_name);
        s.bottle_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        s.bottle = path;
        self.new_name = s.bottle_name.clone();
        self.new_windows = "win10".into();
        self.new_games = false;
        self.create_bottle(crate::app::bottles::BottleAsker::Installer);
    }

    /// The bottle was made (or not).
    pub(crate) fn install_bottle_made(&mut self, bottle: &Path, ok: bool) {
        let waiting = self
            .install
            .as_ref()
            .is_some_and(|s| s.stage == Stage::CreatingBottle && s.bottle == bottle);
        if !waiting {
            return;
        }
        if ok {
            // The worker said it worked; the step is done only if the folder looks like a whole
            // prefix. The caller rescanned just before, so the list shows what is on the disk.
            let complete = self.bottle_at(bottle).is_some_and(|b| b.prefix_complete);
            self.drive_install(|s| s.bottle_made(complete));
            self.select_path(bottle);
            self.screen = Screen::Bottle;
            self.tab = BottleTab::Programs;
            self.proceed_install();
        } else {
            self.drive_install(|s| {
                s.fail("No se pudo crear la botella, así que el instalador no se abrió. El aviso de la botella dice por qué.")
            });
        }
    }

    /// Writes the program's fixes (when it has them) and starts the installer, or opens the file
    /// when it is no installer. The worker does it; `Msg::Started` and `Msg::StartFailed` answer.
    fn start_installer(&mut self) {
        let Some((file, bottle, installs)) = self
            .install
            .as_ref()
            .map(|s| (s.file.clone(), s.bottle.clone(), s.installs))
        else {
            return;
        };
        let Some(b) = self.bottle_at(&bottle).cloned() else {
            self.drive_install(|s| s.fail("La botella ya no está."));
            return;
        };
        let Some(wine) = self.bottle_wine(&b) else {
            self.drive_install(|s| s.fail("Wine no está listo."));
            return;
        };
        self.select_path(&bottle);
        self.screen = Screen::Bottle;
        self.tab = BottleTab::Programs;
        if !installs {
            // A program, not an installer: open it and keep it in the list, like «Abrir un .exe».
            self.install = None;
            self.run_args.clear();
            // Keep it in the list, unless this very file is there: its options stay.
            self.run_save = !in_list(&b.programs, &file);
            if !self.run_save {
                self.say("Ese programa ya está guardado: lo abro sin cambiar sus opciones.");
            }
            self.run_hud = false;
            self.run_avx = false;
            self.modal = Some(Modal::Run(bottle, file));
            self.apply(Action::RunPicked);
            return;
        }
        let now = shown_programs(&b.programs);
        self.drive_install(|s| s.snapshot(&now));
        let label = self
            .install
            .as_ref()
            .map(|s| s.process_label())
            .unwrap_or_default();
        // The program's own fixes are written before its installer runs.
        // The recipe is recorded as applied once the worker wrote it.
        let recipe = recipes::for_installer(&file);
        let job = StartJob {
            wine,
            bottle: b,
            program: Some(file.clone()),
            file,
            args: String::new(),
            env: vec![],
            label,
            rescan_after: true,
            recipe,
            steam_move: None,
            prepare: false,
        };
        if !self.start_with_recipe(job) {
            self.drive_install(|s| {
                s.fail("No se pudo abrir el instalador: la botella está ocupada o ya abre este archivo. Espera un momento y vuelve a intentarlo.")
            });
        }
    }

    // ------------------------------------------------------------ what the app observes

    /// The programs a bottle shows now, from the list already read.
    fn programs_of(&self, bottle: &Path) -> Programs {
        self.bottle_at(bottle)
            .map(|b| shown_programs(&b.programs))
            .unwrap_or_default()
    }

    /// Changes the installation and, if that ended it, reads the programs again once so the list
    /// shows what was found.
    fn drive_install(&mut self, f: impl FnOnce(&mut InstallSession)) {
        let Some(s) = self.install.as_mut() else {
            return;
        };
        let was_active = s.active();
        f(s);
        if was_active && !s.active() {
            self.install_finished();
        }
    }

    fn install_finished(&mut self) {
        self.install_stopping = false;
        self.rescan();
        if let Some(s) = &self.install {
            if matches!(s.stage, Stage::Done(domain::install::Outcome::Installed)) {
                let line = s.headline();
                self.done(line);
            }
        }
    }

    /// The worker that wrote the program's fixes into this installation's bottle reported
    /// whether they are all there.
    pub(crate) fn install_recipe_outcome(&mut self, bottle: &Path, id: &str, applied: bool) {
        if self.install.as_ref().is_some_and(|s| s.bottle == bottle) {
            self.drive_install(|s| s.recipe_reported(id, applied));
        }
    }

    /// The installer's own process was started by a worker.
    pub(crate) fn install_started(&mut self, bottle: &Path, program: &Option<PathBuf>) {
        if self.is_installer_run(bottle, program) {
            self.drive_install(|s| s.installer_started());
        }
    }

    /// A worker could not start the installer. True when the installation took it as its own
    /// failure (its card says so, so no second banner is made).
    pub(crate) fn install_start_failed(
        &mut self,
        bottle: &Path,
        program: &Option<PathBuf>,
        why: &str,
    ) -> bool {
        if !self.is_installer_run(bottle, program) {
            return false;
        }
        let why = why.to_string();
        self.drive_install(|s| s.fail(why));
        true
    }

    /// The process of this bottle and file is the one of the installation.
    pub(crate) fn is_installer_run(&self, bottle: &Path, program: &Option<PathBuf>) -> bool {
        self.install.as_ref().is_some_and(|s| {
            s.installs
                && s.active()
                && s.bottle == bottle
                && program.as_deref() == Some(s.file.as_path())
        })
    }

    /// The installer's first process ended: what is in the bottle now decides what comes next.
    /// The caller already read the programs again.
    pub(crate) fn install_process_ended(
        &mut self,
        bottle: &Path,
        code: Option<i32>,
        closing: bool,
    ) {
        let now = self.programs_of(bottle);
        let at = self.ctx.input(|i| i.time);
        self.install_idle = 0;
        self.drive_install(|s| {
            if closing {
                s.stopped_by_user(&now);
            } else {
                // The bottle may well still be in use for a few seconds, whatever happens next;
                // the first look at it comes a second from now.
                s.looked_at = at;
                s.looks = 0;
                s.process_ended(code, true, &now);
            }
        });
    }

    /// Checks that run when the live set arrives (every few seconds): while the installer is
    /// open, a worker reads the bottle's programs. A settling bottle is looked at on its own
    /// faster clock (`look_settling`).
    pub(crate) fn install_tick(&mut self) {
        if self
            .install
            .as_ref()
            .is_some_and(|s| s.stage == Stage::Settling)
        {
            return;
        }
        self.install_look();
    }

    /// While the installer's first process is gone and the bottle is still busy, the result is
    /// waited for: a look about once a second (`look_wait`) shows it in about two seconds rather
    /// than after two of the slow live checks.
    fn look_settling(&mut self) {
        let now = self.ctx.input(|i| i.time);
        let Some(wait) = self
            .install
            .as_ref()
            .filter(|s| s.stage == Stage::Settling)
            .map(|s| look_wait(s.looks, s.looked_at, now))
        else {
            return;
        };
        if wait <= 0.0 {
            self.install_look();
        }
        // The window may be idle otherwise: it must wake up for the next look, and no sooner.
        // (When a look is due but one is still out, the answer wakes it; a short wait covers
        // the case that it does not.)
        self.ctx
            .request_repaint_after(std::time::Duration::from_secs_f64(wait.clamp(0.1, 3.0)));
    }

    /// One look at the bottle of the installation, on a worker and never more than one at a
    /// time: whether Wine still runs there (when settling) and the programs it holds.
    fn install_look(&mut self) {
        let Some(s) = self.install.as_ref().filter(|s| s.watches_programs()) else {
            return;
        };
        if self.install_scanning {
            return;
        }
        let (id, bottle, settling) = (s.id, s.bottle.clone(), s.stage == Stage::Settling);
        let settings = self.cfg.bottle_ref(&bottle).cloned();
        // The live set is the answer when the app has no Wine to ask for this bottle.
        let known_alive = self.live.contains(&bottle);
        let wine = self
            .bottle_at(&bottle)
            .and_then(|b| self.bottle_wine_cached(b))
            .filter(|_| settling);
        let at = self.ctx.input(|i| i.time);
        if let Some(s) = self.install.as_mut() {
            s.looked_at = at;
            s.looks = s.looks.saturating_add(1);
        }
        self.install_scanning = true;
        self.task_freeing(OnPanic::install_look(), move |_| {
            let alive = match &wine {
                Some(w) => is_running_at(w, &bottle),
                None => known_alive,
            };
            let programs = shown_programs(&scan_programs(&bottle, settings.as_ref()));
            vec![Msg::InstallScan(id, alive, programs)]
        });
    }

    /// A worker looked at the bottle: whether Wine runs in it, and its programs.
    pub(crate) fn install_scanned(&mut self, id: u64, alive: bool, programs: Programs) {
        self.install_scanning = false;
        let Some(stage) = self
            .install
            .as_ref()
            .filter(|s| s.id == id && s.watches_programs())
            .map(|s| s.stage.clone())
        else {
            return;
        };
        if stage == Stage::Settling {
            // One idle look is not enough: an installer may hand over to a second process.
            self.install_idle = idle_looks(self.install_idle, alive, INSTALL_IDLE_CHECKS);
            let busy = alive || self.install_idle < INSTALL_IDLE_CHECKS;
            self.drive_install(|s| s.bottle_checked(busy, &programs));
        } else {
            self.drive_install(|s| s.programs_changed(&programs));
        }
    }

    /// A worker that starts the installer panicked: the card says so instead of staying on its
    /// last mark. True when it was this installation's worker.
    pub(crate) fn install_worker_failed(&mut self, id: u64) -> bool {
        if !self
            .install
            .as_ref()
            .is_some_and(|s| s.id == id && s.active())
        {
            return false;
        }
        self.drive_install(|s| s.fail(WORKER_FAILED_TEXT));
        true
    }

    /// `on_panic` for a job that starts this bottle's file: when it is the installer of the
    /// installation going on, it carries the installation's id, so a panic fails that card.
    pub(crate) fn guard_installer_job(
        &self,
        bottle: &Path,
        program: &Option<PathBuf>,
        on_panic: OnPanic,
    ) -> OnPanic {
        if self.is_installer_run(bottle, program) {
            OnPanic {
                install: self.install.as_ref().map(|s| s.id),
                ..on_panic
            }
        } else {
            on_panic
        }
    }

    /// Run after every batch of messages: things the installation waits for.
    pub(crate) fn follow_install(&mut self) {
        self.open_pending();
        let Some((stage, bottle)) = self
            .install
            .as_ref()
            .filter(|s| s.active())
            .map(|s| (s.stage.clone(), s.bottle.clone()))
        else {
            self.install_stopping = false;
            return;
        };
        match stage {
            Stage::WaitingWine => {
                let install_ok =
                    wine_setup::lock_progress(&self.setup).step == Some(wine_setup::Step::Done);
                let wine = WineFacts {
                    ready: self.wine.is_some(),
                    installing: self.installing(),
                    looked: self.wine_state != wine_setup::WineState::Unknown,
                };
                match wine_wait(wine, install_ok) {
                    WineWait::Ready => {
                        self.drive_install(|s| s.wine_ready());
                        self.proceed_install();
                    }
                    WineWait::Fail => {
                        let broken =
                            matches!(self.wine_state, wine_setup::WineState::Broken { .. });
                        self.drive_install(|s| {
                            s.fail(if broken {
                                "Wine está instalado pero no arranca, así que el instalador no se abrió. Mira el aviso de Wine para ver qué le falta."
                            } else {
                                "Wine no está instalado y no se pudo instalar ahora, así que el instalador no se abrió. Mira el aviso de Wine o instálalo desde Ajustes."
                            })
                        });
                    }
                    WineWait::Keep => {}
                }
            }
            Stage::Preparing | Stage::InstallerOpen | Stage::Settling => {
                if self.bottle_at(&bottle).is_none() {
                    self.drive_install(|s| s.fail("La botella ya no está."));
                } else if stage == Stage::Settling {
                    self.look_settling();
                }
            }
            Stage::CreatingBottle | Stage::Done(_) => {}
        }
        // The person asked to close the installer: it is over once the bottle has closed.
        if closing_complete(Closing {
            asked: self.install_stopping,
            bottle_busy: self.busy.contains_key(&bottle),
            still_running: self.running.iter().any(|r| r.bottle == bottle),
        }) {
            let now = self.programs_of(&bottle);
            self.install_stopping = false;
            self.drive_install(|s| s.stopped_by_user(&now));
        }
    }

    // ------------------------------------------------------------ the card's buttons

    /// «Cerrar todo en esta botella»: the usual way to close the programs of that one bottle.
    pub(crate) fn close_installer(&mut self) {
        let Some(bottle) = self
            .install
            .as_ref()
            .filter(|s| s.card_actions().close)
            .map(|s| s.bottle.clone())
        else {
            return;
        };
        let Some(i) = self.bottles.iter().position(|b| b.path == bottle) else {
            return;
        };
        self.selected = i;
        // Another job may hold the bottle: then the closing is refused and nothing is closed.
        let held_before = self.busy.contains_key(&bottle);
        self.quit_programs(false);
        // Only if this press began the closing (it marks the bottle busy until it is done): a
        // mark that was there before belongs to another job, and the closing did not happen.
        self.install_stopping = !held_before && self.busy.contains_key(&bottle);
    }

    /// «Sí, ya terminó»: the person says the installer is done; the bottle is read once more.
    pub(crate) fn finish_install(&mut self) {
        let Some(bottle) = self.install.as_ref().map(|s| s.bottle.clone()) else {
            return;
        };
        self.rescan();
        let now = self.programs_of(&bottle);
        self.drive_install(|s| s.finish_by_user(&now));
    }

    /// «Listo»: the card goes away once the installation is over.
    pub(crate) fn dismiss_install(&mut self) {
        let Some(s) = self.install.as_ref().filter(|s| s.card_actions().dismiss) else {
            return;
        };
        if s.stage == Stage::CreatingBottle || s.stage == Stage::Preparing {
            // A worker is on it and may never answer: end the installation, so that another one
            // can start. The bottle itself is not touched.
            self.drive_install(|s| s.fail(domain::install::CANCELLED));
        } else {
            // Over: the card can go.
            let waiting = s.stage == Stage::WaitingWine;
            let file = s.file_name();
            self.install = None;
            // Still waiting for Wine: nothing started, and the installer will not open later.
            if waiting {
                self.say(format!(
                    "Cancelaste la instalación: «{file}» no se abrirá. Vuelve a elegirlo cuando quieras."
                ));
            }
        }
    }
}

/// Idle looks in a row, after the installer's first process ended, before the bottle counts as
/// idle (the looks are a few seconds apart).
const INSTALL_IDLE_CHECKS: u8 = 2;
