//! What the worker threads report back to the window.
//!
//! `poll` takes the messages one at a time, in the order they arrived, and hands each to the
//! method that handles it; the methods below are grouped by what the messages are about.

use crate::backend::*;
use crate::domain::engines::LAUNCHERS;
use crate::*;

impl App {
    pub(crate) fn poll(&mut self) {
        // Saving the settings failed since the last look: said once, in the person's words.
        if let Some(why) = take_save_failure() {
            self.fail(None, why);
        }
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Status(s) => self.say(s),
                Msg::Done(s) => self.done(s),
                Msg::Fail(bottle, s) => self.fail(bottle.as_deref(), s),
                Msg::AppJob(job) => self.app_job = job,
                Msg::Busy(p, s) => self.bottle_busy(p, s),
                Msg::Rescan => self.rescan(),
                // Bottles.
                Msg::WindowsSet(bottle, v) => self.windows_set(bottle, v),
                Msg::Created(p, ok) => self.bottle_created(p, ok),
                Msg::Duplicated(original, copy, result) => {
                    self.bottle_duplicated(original, copy, result)
                }
                Msg::Deleted(path, name, result) => self.bottle_deleted(path, name, result),
                Msg::RunningSet(v) => self.running_set(v),
                // Engines and Wine.
                Msg::WineDetected(state, then_install) => self.wine_detected(state, then_install),
                Msg::EngineInstalled(state) => self.engine_installed(state),
                Msg::VideoAdded(state) => self.video_added(state),
                Msg::EngineVersions(list) => self.engine_versions_known(list),
                Msg::EngineChecked(bottle, engine, last, version) => {
                    self.engine_checked(bottle, engine, last, version);
                }
                Msg::EngineSet(bottle, engine) => self.apply_bottle_engine(&bottle, engine),
                Msg::WinePicked(path, version) => self.wine_picked(path, version),
                Msg::WineChangeReady(change) => self.finish_wine_change(change),
                Msg::AskCloseForWine(change, bottles) => {
                    self.modal = Some(Modal::CloseForWine(change, bottles));
                }
                // Programs and exits.
                Msg::Started(o) => self.program_started(o),
                Msg::StartFailed(bottle, program, why) => {
                    self.program_start_failed(bottle, program, why)
                }
                Msg::RecipeApplied(bottle, id) => self.recipe_applied(bottle, id),
                Msg::RecipeNotApplied(bottle, id) => {
                    // Nothing is recorded in the settings: the bottle does not have the fixes.
                    self.install_recipe_outcome(&bottle, &id, false);
                }
                Msg::OpenProgram(bottle, program) => {
                    if let Some(b) = self.bottle_at(&bottle).cloned() {
                        self.open_program(b, program);
                    }
                }
                Msg::Exited(e) => self.program_exited(e),
                // The installer.
                Msg::InstallScan(id, alive, programs) => self.install_scanned(id, alive, programs),
                // Launchers, workers, diagnosis and pictures.
                Msg::Launchers(n) => self.launchers_synced(n),
                Msg::WorkerFailed(on_panic) => self.worker_failed(on_panic),
                Msg::Diagnosis(d) => self.diagnosis_found(d),
                Msg::IconsLoaded(list) => self.icons_loaded(list),
            }
        }
        self.prune_opening();
        self.follow_exits();
        self.follow_install();
    }

    // ---------------------------------------------------------------- Bottles

    /// winecfg applied a Windows version to a bottle: it is saved and shown.
    fn windows_set(&mut self, bottle: PathBuf, v: String) {
        self.cfg.bottle(&bottle).windows = Some(v.clone());
        self.cfg.save();
        self.rescan();
        self.done(format!("Ahora la botella usa {}.", windows_label(&v)));
    }

    /// Marks a bottle as busy with a job (its label), or free again when the job ended.
    fn bottle_busy(&mut self, p: PathBuf, s: Option<String>) {
        match s {
            Some(s) => self.busy.insert(p, s),
            None => self.busy.remove(&p),
        };
    }

    /// A bottle finished being made, or could not be.
    fn bottle_created(&mut self, p: PathBuf, ok: bool) {
        self.busy.remove(&p);
        // A bottle that could not be made leaves no settings and never becomes the
        // main one; its folder was already moved to the Trash by the worker.
        if !ok {
            self.cfg.bottle_removed(&p);
            self.cfg.save();
            self.rescan();
            self.install_bottle_made(&p, false);
            return;
        }
        self.rescan();
        self.select_path(&p);
        self.clear_errors(&p);
        if self.cfg.default_bottle.is_none() {
            self.cfg.default_bottle = Some(p.to_string_lossy().to_string());
            self.cfg.save();
        }
        self.install_bottle_made(&p, ok);
    }

    /// Which bottles have a running Wine right now.
    fn running_set(&mut self, v: Vec<PathBuf>) {
        // A bottle whose Wine just stopped may have installed programs while it ran
        // (a launcher installing a game): read the programs again.
        let stopped = super::library::some_bottle_stopped(&self.live, &v);
        // A program that handed over to another process stops showing as open
        // when its bottle stops (a moment's grace for a check that began earlier).
        self.handed_over
            .retain(|h| v.contains(&h.bottle) || h.since.elapsed().as_secs() < 5);
        self.live = v;
        if stopped {
            self.rescan();
        }
        self.install_tick();
    }

    // ---------------------------------------------------------------- Engines and Wine

    /// The Wine on this Mac was looked for: it is used, or installed, or the failure is said.
    fn wine_detected(&mut self, state: wine_setup::WineState, then_install: bool) {
        self.refresh_engines();
        let missing = state.path().is_none();
        let was_installing = self
            .setup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .step
            .is_some();
        let setup_error = self
            .setup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .error
            .clone();
        self.set_wine(state);
        if !missing {
            self.sync_launchers();
            if self.open_pending() {
                return;
            }
        }
        if !missing && was_installing {
            self.done("¡Wine instalado! Ya puedes crear botellas y abrir programas.");
        } else if !missing {
            self.done(format!("Wine listo: {}", self.wine_ver));
        } else if then_install && self.auto_install {
            self.start_wine_install();
        } else if let Some(e) = setup_error {
            self.setup_failed(e);
        }
    }

    /// An engine finished installing (or failed): a bottle waiting for it moves to it, and the Wine
    /// in use changes when it should.
    fn engine_installed(&mut self, state: wine_setup::WineState) {
        // The bottles kept closed during a reinstall are free again.
        self.busy
            .retain(|_, job| job != super::engines::REINSTALLING);
        // A reinstalled engine may be another version than the one remembered.
        self.engine_versions.clear();
        self.versions_requested.clear();
        self.refresh_engines();
        // The DXMT engine is only ever used by the bottles that ask for it; it never becomes
        // the Wine of everything else. (The games engine is never installed by the app.)
        let own = state
            .path()
            .map(|p| wine_setup::Engine::of(&p))
            .filter(|e| *e == wine_setup::Engine::Metal);
        // A bottle waiting for an engine stops waiting when the install ends, also
        // when it ended without that engine (failed, cancelled, another one).
        let waiting = super::engines::settle_pending(self.pending_engine_bottle.take(), own);
        if let Some(engine) = own {
            self.own_engine_installed(engine, waiting);
        } else if state.path().is_some() {
            let first = self.wine.is_none();
            // With programs open in the old Wine, the switch waits for a dialog
            // and a worker; it says its own words when it is done.
            if self.switch_wine(state) {
                self.after_wine_switch(first);
            }
        } else if self.wine.is_none() {
            self.wine_state = wine_setup::WineState::Missing;
            let error = self
                .setup
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .error
                .clone();
            if let Some(e) = error {
                self.setup_failed(e);
            }
        } else {
            let error = self
                .setup
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .error
                .clone();
            if let Some(e) = error {
                self.setup_failed(format!("{e} Sigues con {}.", self.wine_ver));
            }
        }
    }

    /// The DXMT engine finished installing: the bottle that waited for it moves to it, the
    /// person is told, and a Wine is looked for when the app had none.
    fn own_engine_installed(
        &mut self,
        engine: wine_setup::Engine,
        waiting: Option<(PathBuf, wine_setup::Engine)>,
    ) {
        let mut deferred = false;
        if let Some((bp, _)) = waiting {
            if !self.set_bottle_engine(&bp, Some(engine)) {
                // The bottle is in use now: it keeps its engine. The refusal
                // is already said; the install's own message follows it.
                deferred = true;
            }
        }
        self.done(if deferred {
            "Instalado. Cierra los programas de la botella y elige los gráficos otra vez para usarlo en ella.".to_string()
        } else {
            format!("«{LAUNCHERS}» instalado: esta botella ya dibuja DirectX 10/11 con Metal.")
        });
        if self.wine.is_none() {
            // The DXMT engine brought the standard Wine with it.
            self.detect_wine(false);
        }
    }

    /// Video was added to the DXMT engine, or could not be.
    fn video_added(&mut self, state: wine_setup::WineState) {
        self.refresh_engines();
        if state.path().is_some() {
            self.done(format!("Video añadido a «{LAUNCHERS}». Cierra y vuelve a abrir Steam (o el launcher) para que sus juegos lo usen."));
        } else {
            let why = self
                .setup
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .error
                .clone();
            self.setup_failed(why.unwrap_or_else(|| "No se pudo añadir el video.".into()));
        }
    }

    /// Engine versions came back from a worker.
    fn engine_versions_known(&mut self, list: Vec<(PathBuf, Option<String>)>) {
        for (path, version) in list {
            self.versions_requested.remove(&path);
            if let Some(v) = version {
                self.engine_versions.insert(path, v);
            }
        }
    }

    /// A Wine file picked by hand was asked for its version.
    fn wine_picked(&mut self, path: PathBuf, version: Result<String, String>) {
        self.app_job = None;
        match version {
            Ok(version) => {
                self.set_wine(wine_setup::WineState::Ready { path, version });
                self.done(format!("Usando {}", self.wine_ver));
            }
            Err(e) => {
                // `e` is the plain reason, which may already end with a full stop.
                let e = e.trim_end_matches('.');
                self.fail(
                    None,
                    format!("Ese archivo no es un Wine que funcione. {e}."),
                );
            }
        }
    }

    // ---------------------------------------------------------------- Programs and exits

    /// A worker started a program: it is followed from now on.
    fn program_started(&mut self, o: Opened) {
        self.forget_opening(&o.bottle, &o.program);
        self.install_started(&o.bottle, &o.program);
        let busy_before = self.live.contains(&o.bottle)
            || self.running.iter().any(|r| r.bottle == o.bottle)
            || self.handed_over.iter().any(|h| h.bottle == o.bottle);
        self.note_wine(&o.bottle, &o.wine);
        // It opened: an old error of this bottle no longer describes it.
        self.clear_errors(&o.bottle);
        self.say(format!("Abriendo {}…", o.label));
        self.running.push(Running {
            bottle: o.bottle,
            label: o.label,
            program: o.program,
            child: o.child,
            rescan_after: o.rescan_after,
            started: std::time::Instant::now(),
            closing: false,
            busy_before,
        });
    }

    /// A worker could not start a program.
    fn program_start_failed(&mut self, bottle: PathBuf, program: Option<PathBuf>, why: String) {
        self.forget_opening(&bottle, &program);
        // An installation says it on its own card; no second banner for it.
        if !self.install_start_failed(&bottle, &program, &why) {
            self.fail(Some(&bottle), why);
        }
    }

    /// A recipe was written into a bottle: the bottle remembers it.
    fn recipe_applied(&mut self, bottle: PathBuf, id: String) {
        let s = self.cfg.bottle(&bottle);
        if !s.recipes.contains(&id) {
            s.recipes.push(id.clone());
            self.cfg.save();
        }
        self.install_recipe_outcome(&bottle, &id, true);
    }

    // ---------------------------------------------------------------- Launchers, workers, diagnosis

    /// The launchers were written: how many exist, and another pass when one was asked for
    /// meanwhile.
    fn launchers_synced(&mut self, n: usize) {
        self.launcher_count = n;
        if self.launcher_pass.take_again() {
            self.sync_launchers();
        }
    }

    /// A worker panicked before it answered: what it held is freed and the person is told.
    fn worker_failed(&mut self, on_panic: OnPanic) {
        for p in &on_panic.busy {
            // A half-made bottle stays busy until its folder is out of the way.
            if on_panic.discard.as_ref() != Some(p) {
                self.busy.remove(p);
            }
        }
        if let Some(p) = on_panic.discard.clone() {
            self.discard_half_made(p);
        }
        // Only a failed engine install frees the bottles held back for its swap.
        if on_panic.engine_install {
            self.busy
                .retain(|_, job| job != super::engines::REINSTALLING);
        }
        if on_panic.app_job {
            self.app_job = None;
        }
        if on_panic.engine_install {
            self.pending_engine_bottle = None;
        }
        if let Some(bottle) = &on_panic.diagnosis {
            self.diagnosis.forget(bottle);
        }
        if on_panic.wine_lookup && self.wine_state == wine_setup::WineState::Unknown {
            self.wine_state =
                super::engines::state_after_failed_lookup(self.wine.as_deref(), &self.wine_ver);
        }
        if on_panic.install_look {
            self.install_scanning = false;
        }
        // A start that never answered is over: the same file may start again now.
        if let Some(key) = &on_panic.opening {
            self.opening.remove(key);
        }
        // A worker of the installation says so on the installation's card.
        let on_the_card = on_panic
            .install
            .is_some_and(|id| self.install_worker_failed(id));
        self.rescan();
        if !on_the_card {
            self.fail(None, WORKER_FAILED_TEXT);
        }
    }

    /// A diagnosis of one bottle came back: its status line is said when it is the one being
    /// waited for.
    fn diagnosis_found(&mut self, d: diagnostics::Diagnosis) {
        let status = d.status_line();
        if self.diagnosis.finish(d) {
            self.say(status);
        }
    }

    /// Program pictures were decoded on a worker: they become textures.
    fn icons_loaded(&mut self, list: Vec<(PathBuf, Option<egui::ColorImage>)>) {
        for (path, img) in list {
            // A picture whose program left while a worker decoded it (its bottle
            // was deleted or renamed) was forgotten by `prune_icons`; keeping it
            // now would hold its texture until some later reading of the lists.
            if !self.icons_requested.contains(&path) {
                continue;
            }
            let tex = img.map(|img| {
                self.ctx
                    .load_texture(path.to_string_lossy(), img, egui::TextureOptions::LINEAR)
            });
            self.textures.insert(path, tex);
        }
    }

    /// A start is over (it opened, or it did not): the program may be started again.
    fn forget_opening(&mut self, bottle: &Path, program: &Option<PathBuf>) {
        if let Some(p) = program {
            self.opening.remove(&(bottle.to_path_buf(), p.clone()));
        }
    }

    /// Notes the programs whose first process ended. Each exit is logged with its code; a quick
    /// one is judged on a worker (it waits a few seconds, then asks whether the bottle is still
    /// in use) before anything is reported.
    fn follow_exits(&mut self) {
        let ended = self.take_ended();
        let mut report = ExitReport::default();
        for e in ended {
            self.note_exit(e, &mut report);
        }
        self.report_exits(report);
    }

    /// Takes out of the running list every program whose first process has ended, in list order.
    fn take_ended(&mut self) -> Vec<Ended> {
        let mut ended: Vec<Ended> = vec![];
        let mut i = 0;
        while i < self.running.len() {
            match self.running[i].child.try_wait() {
                Ok(None) => i += 1,
                Ok(Some(status)) => {
                    use std::os::unix::process::ExitStatusExt;
                    let r = self.running.remove(i);
                    ended.push(Ended {
                        r,
                        success: status.success(),
                        code: status.code(),
                        phrase: super::programs::exit_phrase(status.code(), status.signal()),
                    });
                }
                Err(_) => {
                    let r = self.running.remove(i);
                    ended.push(Ended {
                        r,
                        success: false,
                        code: None,
                        phrase: super::programs::exit_phrase(None, None),
                    });
                }
            }
        }
        ended
    }

    /// Logs one program's exit, adds what it means to `report`, and sends a quick exit to be
    /// judged.
    fn note_exit(&mut self, e: Ended, report: &mut ExitReport) {
        let Ended {
            r,
            success,
            code,
            phrase,
        } = e;
        let secs = r.started.elapsed().as_secs();
        push_log(
            &self.log,
            format!("{} terminó ({phrase}, tras {secs} s)", r.label),
        );
        report.rescan |= r.rescan_after;
        let own_install = self.is_installer_run(&r.bottle, &r.program);
        if own_install {
            report.installer_exit = Some((r.bottle.clone(), code, r.closing));
        } else if r.rescan_after {
            report.announce = true;
            // Something the person closed is not a failure.
            if !success && !r.closing {
                report.ended_badly = Some((r.label.clone(), phrase.clone()));
            }
        }
        let key = r.program.clone().map(|p| (r.bottle.clone(), p));
        // The installer has its own card, which says how it ended: no second banner here.
        let judged = !r.closing && !own_install && secs < super::programs::EARLY_EXIT_SECS;
        if !judged {
            // Nothing to ask: it ran for a while, or the person closed it.
            if let Some(k) = &key {
                self.opening.remove(k);
            }
            return;
        }
        // Until the worker answers, the program still counts as starting, so a second
        // click does not open a second copy.
        if let Some(k) = &key {
            self.opening.insert(k.clone(), std::time::Instant::now());
        }
        self.judge_exit_later(r, success, secs);
    }

    /// Hands a quick exit to a worker: it waits a few seconds, asks whether the bottle is still in
    /// use, and answers with what the exit means.
    fn judge_exit_later(&mut self, r: Running, success: bool, secs: u64) {
        let busy_before = r.busy_before;
        let others_open = self.running.iter().any(|o| o.bottle == r.bottle)
            || self.handed_over.iter().any(|h| h.bottle == r.bottle);
        let bottle = self.bottles.iter().find(|b| b.path == r.bottle).cloned();
        let wine = bottle.as_ref().and_then(|b| self.bottle_wine_cached(b));
        let (label, program) = (r.label, r.program);
        let on_panic = OnPanic::default().opening(&r.bottle, &program);
        self.task_freeing(on_panic, move |_| {
            std::thread::sleep(std::time::Duration::from_secs(
                super::programs::JUDGE_WAIT_SECS,
            ));
            // No Wine to ask (the bottle is gone): nothing runs there.
            let in_use = match (&wine, &bottle) {
                (Some(w), Some(b)) => is_running(w, b),
                _ => false,
            };
            vec![Msg::Exited(Exit {
                bottle: bottle.map(|b| b.path).unwrap_or(r.bottle),
                label,
                program,
                verdict: super::programs::judge_exit(super::programs::ExitFacts {
                    success,
                    ran_secs: secs,
                    bottle_in_use: in_use,
                    others_open,
                    busy_before,
                }),
            })]
        });
    }

    /// Says what the exits of this look meant, once, after each of them was noted.
    fn report_exits(&mut self, report: ExitReport) {
        let ExitReport {
            rescan,
            announce,
            ended_badly,
            installer_exit,
        } = report;
        if rescan {
            if announce {
                match ended_badly {
                    Some((label, phrase)) => {
                        self.say(super::programs::ended_badly_text(&label, &phrase))
                    }
                    None => self.done("Listo. Revisa tus programas."),
                }
            }
            self.rescan();
        }
        if let Some((bottle, code, closing)) = installer_exit {
            self.install_process_ended(&bottle, code, closing);
        }
    }

    /// What a worker found out about a program's first process ending.
    fn program_exited(&mut self, e: Exit) {
        self.forget_opening(&e.bottle, &e.program);
        match e.verdict {
            ExitVerdict::Ended => {}
            ExitVerdict::HandedOver => {
                push_log(
                    &self.log,
                    format!("«{}» sigue abierto: su botella sigue en uso", e.label),
                );
                self.handed_over.push(HandedOver {
                    bottle: e.bottle,
                    label: e.label,
                    program: super::programs::handed_over_key(e.program),
                    since: std::time::Instant::now(),
                });
            }
            ExitVerdict::Failed => {
                push_log(&self.log, format!("✗ {} se cerró con un error", e.label));
                // Look for the reason right away, in the bottle that failed. The banner stays
                // until it is closed, so it only points to the diagnosis, which may find
                // nothing. The program's last lines may still be on their way from its pipes.
                let text = format!(
                    "«{}» se cerró con un error. Mira el diagnóstico por si encuentra la causa.",
                    e.label
                );
                self.fail(Some(&e.bottle), text);
                self.run_diagnosis(e.bottle, std::time::Duration::from_millis(500));
            }
        }
    }
}

/// A program whose first process ended, with how it ended.
struct Ended {
    r: Running,
    success: bool,
    code: Option<i32>,
    phrase: String,
}

/// What the exits of one look at the running programs add up to, said after all were noted.
#[derive(Default)]
struct ExitReport {
    /// A program that asked for it ended: the lists are read again.
    rescan: bool,
    /// A tool (not the installer, which has its own card) said something about how it ended.
    announce: bool,
    /// A tool that ended with an error: its name and how it ended.
    ended_badly: Option<(String, String)>,
    /// The installer of the installation going on ended: its bottle, its code, and whether the
    /// person asked to close it.
    installer_exit: Option<(PathBuf, Option<i32>, bool)>,
}
