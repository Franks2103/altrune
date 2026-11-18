//! Wine and the other engines: finding them, installing them, and which one a bottle uses.

use crate::backend::*;
use crate::domain::engines::{bottle_engine_from, GAMES, LAUNCHERS};
use crate::*;

/// An older Wine than the one that last updated a bottle can break it: this says whether an
/// engine of version `ver` may take a bottle last used with `last`.
pub(crate) fn version_fits(last: Option<&str>, ver: &str) -> bool {
    !last.is_some_and(|l| is_downgrade(l, ver))
}

/// The job label of a bottle that waits while the engine it runs on is reinstalled.
pub(crate) const REINSTALLING: &str = "Reinstalando el motor…";

/// Why a reinstall of Wine cannot start now, or `None`. `installing` is the name of the install
/// that runs (empty when it has none yet), `app_job` the app's own job line. A reinstall that is
/// still closing bottles is such a job, so a second press of its button is turned away here.
pub(crate) fn reinstall_blocker(installing: Option<&str>, app_job: Option<&str>) -> Option<String> {
    match (installing, app_job) {
        (Some(job), _) => App::wait_reason(Some(job), None),
        (None, Some(_)) => Some("Espera a que termine lo que se está haciendo.".to_string()),
        (None, None) => None,
    }
}

/// The engine a bottle's settings ask for (what its `engine` setting holds), if any.
fn chosen_engine<'a>(cfg: &'a Config, b: &Bottle) -> Option<&'a str> {
    cfg.bottle_ref(&b.path).and_then(|s| s.engine.as_deref())
}

/// The Wine of the engine a bottle is set to use (when installed): the games engine (never
/// downloaded by this app, used when it is already there) or the DXMT one. Every other bottle
/// runs on the usual Wine. Looks at the disk; the window asks [`bottle_engine_from`] with a
/// snapshot instead.
pub(crate) fn bottle_engine_path(cfg: &Config, b: &Bottle) -> Option<PathBuf> {
    match chosen_engine(cfg, b) {
        Some("games") => wine_setup::Engine::Games.installed(),
        Some("dxmt") => wine_setup::Engine::Metal.installed(),
        _ => None,
    }
}

/// The state of Wine after the search for it failed inside: what the app already knew, so the
/// page never stays on «Buscando…».
pub(crate) fn state_after_failed_lookup(
    wine: Option<&Path>,
    version: &str,
) -> wine_setup::WineState {
    match wine {
        Some(path) => wine_setup::WineState::Ready {
            path: path.to_path_buf(),
            version: version.to_string(),
        },
        None => wine_setup::WineState::Missing,
    }
}

/// The bottle that waited for an engine, once an install ended: it keeps its request only when
/// the engine it asked for is the one that arrived. Whatever the outcome, the wait is over, so a
/// failed or cancelled install never leaves a request that a later install would carry out.
pub(crate) fn settle_pending(
    pending: Option<(PathBuf, wine_setup::Engine)>,
    installed: Option<wine_setup::Engine>,
) -> Option<(PathBuf, wine_setup::Engine)> {
    pending.filter(|(_, wanted)| Some(*wanted) == installed)
}

/// What closing the bottles of the Wine being replaced needs.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WineClose {
    /// Close these (possibly none) and go on.
    Proceed(Vec<PathBuf>),
    /// Programs the person was not told about are open: ask again with these bottles.
    Ask(Vec<PathBuf>),
}

/// `running` are the bottles of the old Wine that Wine answered for just now, `confirmed` the
/// ones a dialog named before anything was closed (None when no dialog was shown). Nothing is
/// closed unless every running bottle was named.
pub(crate) fn plan_wine_close(running: &[PathBuf], confirmed: Option<&[PathBuf]>) -> WineClose {
    if running.is_empty() {
        return WineClose::Proceed(vec![]);
    }
    match confirmed {
        Some(named) if running.iter().all(|r| named.contains(r)) => {
            WineClose::Proceed(running.to_vec())
        }
        _ => WineClose::Ask(running.to_vec()),
    }
}

impl App {
    /// The name the app's own job line gives to a Wine reinstall while it closes the bottles on
    /// the old one, before the download starts.
    pub(crate) const REINSTALL_JOB: &'static str = "Preparando la reinstalación…";

    /// The name the optional addition goes by while it installs. The settings button compares
    /// it with the name of the job that runs, so it is set when the install is asked for, not
    /// when its download starts.
    pub(crate) const VIDEO_JOB: &'static str = "video";
    /// The label of the app's own job while a Wine picked by hand is asked for its version.
    pub(crate) const PICK_JOB: &'static str = "Comprobando ese Wine…";
    /// The labels of a bottle while its engine is asked for its version and then changed.
    pub(crate) const ENGINE_CHECK_JOB: &'static str = "Comprobando el motor…";
    pub(crate) const ENGINE_SWITCH_JOB: &'static str = "Cambiando de motor…";

    /// What the install's progress holds the instant it is asked for: running, and already named,
    /// so the button that asked shows its spinner on the same frame.
    pub(crate) fn started_progress(job: &str) -> wine_setup::Progress {
        wine_setup::Progress {
            running: true,
            flavour: job.to_string(),
            ..Default::default()
        }
    }

    /// The name an engine install goes by while it runs (empty for the usual Wine, which is named
    /// by the file the download finds, and for the games engine, which is never installed here).
    pub(crate) fn engine_job_name(engine: wine_setup::Engine) -> &'static str {
        match engine {
            wine_setup::Engine::Metal => LAUNCHERS,
            wine_setup::Engine::Standard | wine_setup::Engine::Games => "",
        }
    }

    /// The bottles that run on one of these Wines, from the snapshot on screen: (their Wine, the
    /// bottle).
    fn bottles_on(&self, wines: &[PathBuf]) -> Vec<(PathBuf, Bottle)> {
        self.bottles
            .iter()
            .filter_map(|b| self.bottle_wine_cached(b).map(|w| (w, b.clone())))
            .filter(|(w, _)| wines.contains(w))
            .collect()
    }

    pub(crate) fn reinstall_wine(&mut self) {
        // A reinstall that is still closing bottles has no install to show yet: its own job
        // keeps a second press from starting another.
        let job = self.installing().then(|| self.installing_job());
        if let Some(why) = reinstall_blocker(job.as_deref(), self.app_job.as_deref()) {
            self.say(why);
            return;
        }
        let engine = self
            .wine
            .as_deref()
            .map(wine_setup::Engine::of)
            .unwrap_or(wine_setup::Engine::Standard);
        // The games engine in use stays as it is: Altrune does not download it again.
        if engine == wine_setup::Engine::Games {
            self.say(games_not_installable_text());
            return;
        }
        self.start_wine_change(WineChange::Reinstall(engine), None);
    }

    /// Replaces the Wine that bottles run on. The bottles of that Wine that are in use close
    /// first, but only after a dialog named them; the checks and the closing run on a worker,
    /// and the change itself finishes when it answers.
    pub(crate) fn start_wine_change(
        &mut self,
        change: WineChange,
        confirmed: Option<Vec<PathBuf>>,
    ) {
        let mut wines: Vec<PathBuf> = self.wine.iter().cloned().collect();
        if let WineChange::Reinstall(engine) = &change {
            wines.extend(engine.installed().filter(|p| !wines.contains(p)));
        }
        let candidates = self.bottles_on(&wines);
        // A reinstall the person asked for waits until nothing else works on those bottles; a
        // switch after an install cannot be dropped, so it goes on with what is free to mark.
        let is_reinstall = matches!(change, WineChange::Reinstall(_));
        if is_reinstall
            && candidates
                .iter()
                .any(|(_, b)| self.busy.contains_key(&b.path))
        {
            self.say("Espera a que termine lo que se está haciendo en las botellas.");
            return;
        }
        if is_reinstall && self.app_job.is_some() {
            self.say("Espera a que termine lo que se está haciendo.");
            return;
        }
        if confirmed.is_none() {
            // What the last look at the bottles saw: enough to name them before asking Wine.
            let open: Vec<(PathBuf, String)> = candidates
                .iter()
                .filter(|(_, b)| {
                    self.live.contains(&b.path) || !self.running_in(&b.path).is_empty()
                })
                .map(|(_, b)| (b.path.clone(), b.name.clone()))
                .collect();
            if !open.is_empty() {
                self.modal = Some(Modal::CloseForWine(change, open));
                return;
            }
        }
        if candidates.is_empty() && !is_reinstall {
            self.finish_wine_change(change);
            return;
        }
        let marked: Vec<PathBuf> = candidates
            .iter()
            .map(|(_, b)| b.path.clone())
            .filter(|p| !self.busy.contains_key(p))
            .collect();
        for p in &marked {
            self.busy.insert(p.clone(), Self::CLOSING.into());
        }
        if confirmed.is_some() {
            // The person agreed to close them: that is no failure of what they were running.
            for (_, b) in &candidates {
                self.mark_closing(&b.path);
            }
        }
        if is_reinstall {
            self.app_job = Some(Self::REINSTALL_JOB.into());
            self.say("Preparando la reinstalación del motor…");
        }
        let on_panic = OnPanic {
            app_job: is_reinstall,
            ..OnPanic::busy(marked.clone())
        };
        self.task_freeing(on_panic, move |log| {
            let mut msgs: Vec<Msg> = marked.into_iter().map(|p| Msg::Busy(p, None)).collect();
            let running: Vec<&(PathBuf, Bottle)> = candidates
                .iter()
                .filter(|(w, b)| is_running(w, b))
                .collect();
            let paths: Vec<PathBuf> = running.iter().map(|(_, b)| b.path.clone()).collect();
            match plan_wine_close(&paths, confirmed.as_deref()) {
                WineClose::Ask(_) => {
                    // The dialog takes over: the job line is not needed any more.
                    if is_reinstall {
                        msgs.push(Msg::AppJob(None));
                    }
                    let names = running
                        .iter()
                        .map(|(_, b)| (b.path.clone(), b.name.clone()))
                        .collect();
                    msgs.push(Msg::AskCloseForWine(change, names));
                }
                WineClose::Proceed(_) => {
                    for (w, b) in &running {
                        quit_bottle(w, b, true, log);
                    }
                    // The engine is not removed here: the new one is downloaded and checked
                    // first, and only then does it take the old one's place.
                    msgs.push(Msg::WineChangeReady(change));
                }
            }
            msgs
        });
    }

    /// The bottles of the old Wine are closed: the Wine itself changes now.
    pub(crate) fn finish_wine_change(&mut self, change: WineChange) {
        match change {
            WineChange::Reinstall(engine) => {
                // The bottles are closed: the install that follows shows the work from here.
                self.clear_app_job(Self::REINSTALL_JOB);
                // The working engine stays (and the Wine the person picked stays remembered)
                // until the new one is downloaded and checked. Until then the bottles that run
                // on it are kept closed, because the swap renames its folder.
                let mut wines: Vec<PathBuf> = self.wine.iter().cloned().collect();
                wines.extend(engine.installed().filter(|p| !wines.contains(p)));
                let on_it: Vec<PathBuf> = self
                    .bottles_on(&wines)
                    .into_iter()
                    .map(|(_, b)| b.path)
                    .filter(|p| !self.busy.contains_key(p))
                    .collect();
                if self.installing() {
                    self.say("Ya hay otra instalación en marcha. Cuando termine, vuelve a reinstalar el motor.");
                    return;
                }
                for p in on_it {
                    self.busy.insert(p, REINSTALLING.into());
                }
                self.start_engine_install(engine);
            }
            WineChange::Switch(state) => {
                self.set_wine(state);
                self.sync_launchers();
                self.after_wine_switch(false);
            }
        }
    }

    /// What is said and done once the usual Wine changed after an install.
    pub(crate) fn after_wine_switch(&mut self, first: bool) {
        if first {
            self.done("¡Wine instalado! Ya puedes crear botellas y abrir programas.");
        } else {
            self.done(format!("Listo: ahora se usa {}.", self.wine_ver));
        }
        self.open_pending();
    }

    /// The dialog's «cerrar y cambiar»: the bottles it named close, then the change goes on.
    pub(crate) fn confirm_wine_change(&mut self) {
        if let Some(Modal::CloseForWine(change, named)) = self.modal.take() {
            let paths = named.into_iter().map(|(p, _)| p).collect();
            self.start_wine_change(change, Some(paths));
        }
    }

    pub(crate) fn cancel_wine_change(&mut self) {
        let Some(Modal::CloseForWine(change, _)) = self.modal.take() else {
            self.modal = None;
            return;
        };
        match change {
            WineChange::Reinstall(_) => self.say("No se cambió nada."),
            WineChange::Switch(_) => {
                self.say(format!("Sigues con {}; no se cambió nada.", self.wine_ver));
                self.open_pending();
            }
        }
    }

    /// The engine of the bottle on screen.
    pub(crate) fn use_engine(&mut self, engine: wine_setup::Engine) {
        let Some(b) = self.bottle().cloned() else {
            return;
        };
        if self.busy.contains_key(&b.path) {
            self.say(format!(
                "«{}» está ocupada ahora mismo: espera a que termine.",
                b.name
            ));
            return;
        }
        // Changing the engine closes the bottle's programs, so it waits until they are closed.
        if self.bottle_in_use(&b.path) {
            self.say(engine_change_in_use_text(&b.name));
            return;
        }
        match engine {
            wine_setup::Engine::Standard => {
                self.set_bottle_engine(&b.path, None);
            }
            _ => match engine.installed() {
                Some(path) => {
                    // Asking the engine for its version starts a process: a worker does it.
                    let last = self.bottle_last_wine(&b);
                    self.busy
                        .insert(b.path.clone(), Self::ENGINE_CHECK_JOB.into());
                    self.task_freeing(OnPanic::busy([b.path.clone()]), move |log| {
                        let version = wine_setup::probe_logged(&path, log);
                        vec![
                            Msg::Busy(b.path.clone(), None),
                            Msg::EngineChecked(b.path, engine, last, version),
                        ]
                    });
                }
                // Apple's engine is only used when it is already installed: it is never started
                // as a download, and no bottle waits for it.
                None if engine == wine_setup::Engine::Games => {
                    self.say(games_not_installable_text());
                }
                None => {
                    // Another install keeps going: this one is not queued behind it.
                    if self.installing() {
                        let job = self.installing_job();
                        self.say(Self::wait_reason(Some(&job), None).unwrap_or_default());
                        return;
                    }
                    self.pending_engine_bottle = Some((b.path.clone(), engine));
                    self.start_engine_install(engine);
                }
            },
        }
    }

    /// The answer to the engine's version: the bottle moves to it unless it is older than the
    /// Wine that last updated the bottle.
    pub(crate) fn engine_checked(
        &mut self,
        bottle: PathBuf,
        engine: wine_setup::Engine,
        last: Option<String>,
        version: Result<String, String>,
    ) {
        match version {
            Ok(ver) => {
                if !version_fits(last.as_deref(), &ver) {
                    // Refuse, and say what to do instead.
                    self.fail(
                        Some(&bottle),
                        engine_older_text(engine, &last.unwrap_or_default(), &ver),
                    );
                } else if self.bottle_at(&bottle).is_some() {
                    self.set_bottle_engine(&bottle, Some(engine));
                }
            }
            Err(reason) => self.fail(Some(&bottle), engine_unusable_text(engine, &reason)),
        }
    }

    pub(crate) fn add_video(&mut self) {
        if wine_setup::Engine::Metal.installed().is_none() {
            self.say(format!("Instala primero «{LAUNCHERS}»."));
            return;
        }
        if !self.begin_install(Self::VIDEO_JOB) {
            return;
        }
        // Nothing running changes: programs already open keep their environment, and
        // the next ones started from a freshly opened launcher get the video libraries.
        self.say(format!(
            "Descargando GStreamer (video para juegos) para «{LAUNCHERS}»…"
        ));
        let setup = self.setup.clone();
        self.task_freeing(OnPanic::setup(&setup), move |log| {
            vec![Msg::VideoAdded(wine_setup::install_video(
                setup,
                log.clone(),
            ))]
        });
    }

    pub(crate) fn pick_wine(&mut self) {
        if self.app_job.is_some() {
            self.say("Espera a que termine lo que se está haciendo.");
            return;
        }
        if let Some(f) = super::native_dialog(|| {
            rfd::FileDialog::new()
                .set_title("Elige el programa «wine»")
                .pick_file()
        }) {
            self.say(Self::PICK_JOB);
            self.app_job = Some(Self::PICK_JOB.into());
            self.task_freeing(OnPanic::app_job(), move |log| {
                let version = if wine_setup::is_free_wine(&f) {
                    wine_setup::probe_logged(&f, log)
                } else {
                    Err("Está dentro de otra app y Altrune solo usa Wine libre".to_string())
                };
                vec![Msg::WinePicked(f, version)]
            });
        }
    }

    pub(crate) fn look_again(&mut self) {
        // The search is already going: a second one would only race the first.
        if self.wine_state == wine_setup::WineState::Unknown {
            self.say("Ya estoy buscando Wine…");
            return;
        }
        self.wine_state = wine_setup::WineState::Unknown;
        self.say("Buscando Wine…");
        self.detect_wine(false);
        self.rescan();
    }

    /// The Wine a bottle runs on: the games engine when the bottle asks for it and it is
    /// installed, otherwise the usual one. Looks at the disk, so it is what launching, quitting
    /// and installing use.
    pub(crate) fn bottle_wine(&self, b: &Bottle) -> Option<PathBuf> {
        bottle_engine_path(&self.cfg, b).or_else(|| self.wine.clone())
    }

    /// Like [`Self::bottle_wine`], but reads the engines from the snapshot taken at the last
    /// rescan, so it is cheap enough for every frame. Only for what is shown on screen.
    pub(crate) fn bottle_wine_cached(&self, b: &Bottle) -> Option<PathBuf> {
        bottle_engine_from(chosen_engine(&self.cfg, b), &self.engines).or_else(|| self.wine.clone())
    }

    /// Checks (on a worker thread) for a Wine that really starts.
    pub(crate) fn detect_wine(&self, then_install: bool) {
        let cfg = self.cfg.clone();
        self.task_freeing(OnPanic::wine_lookup(), move |log| {
            // An engine swap the app was closed in the middle of is put back first.
            wine_setup::recover_interrupted_installs();
            // A loader script left by an older version of the app learns the new Direct3D 12
            // programs (only an engine that already has Apple's layer has one); it takes
            // effect for each program the next time it starts.
            if let Some(metal) = wine_setup::Engine::Metal.installed() {
                wine_setup::refresh_loader(&metal);
            }
            vec![Msg::WineDetected(
                wine_setup::detect(&cfg, log),
                then_install,
            )]
        });
    }

    pub(crate) fn installing(&self) -> bool {
        wine_setup::lock_progress(&self.setup).running
    }

    /// The name of the install that runs now (empty when it has none yet, or none runs).
    pub(crate) fn installing_job(&self) -> String {
        self.setup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .flavour
            .clone()
    }

    /// Marks an install as running, named `job`, on the frame it is asked for. When another one
    /// runs it changes nothing, says why and answers false: installs never queue up.
    fn begin_install(&mut self, job: &str) -> bool {
        let busy = {
            let mut g = wine_setup::lock_progress(&self.setup);
            if g.running {
                Some(g.flavour.clone())
            } else {
                *g = Self::started_progress(job);
                None
            }
        };
        match busy {
            Some(running) => {
                self.say(Self::wait_reason(Some(&running), None).unwrap_or_default());
                false
            }
            None => true,
        }
    }

    /// Ends the app's job line when it still shows `job` (another job's line stays).
    pub(crate) fn clear_app_job(&mut self, job: &str) {
        if self.app_job.as_deref() == Some(job) {
            self.app_job = None;
        }
    }

    /// Downloads and installs the free standard Wine into the app's own folder.
    pub(crate) fn start_wine_install(&mut self) {
        self.start_engine_install(wine_setup::Engine::Standard);
    }

    pub(crate) fn start_engine_install(&mut self, engine: wine_setup::Engine) {
        if engine == wine_setup::Engine::Games {
            self.say(games_not_installable_text());
            return;
        }
        if !self.begin_install(Self::engine_job_name(engine)) {
            return;
        }
        let upgrading = self.wine.is_some();
        if !upgrading {
            self.wine_state = wine_setup::WineState::Missing;
        }
        self.say(if upgrading {
            format!(
                "Instalando «{}» en segundo plano; mientras tanto todo sigue funcionando.",
                engine_name(engine)
            )
        } else {
            "Instalando Wine gratis. Puedes seguir usando la app mientras tanto.".into()
        });
        let setup = self.setup.clone();
        self.task_freeing(OnPanic::engine_install(&setup), move |log| {
            let state = wine_setup::install(setup, log.clone(), engine);
            vec![Msg::EngineInstalled(state)]
        });
    }

    /// Changes to another Wine. Bottles still running with the old one are closed first (two
    /// different Wine versions can't share a bottle), after a dialog named them. Returns true
    /// when the change is already done; false when it waits for a dialog or a worker.
    pub(crate) fn switch_wine(&mut self, state: wine_setup::WineState) -> bool {
        match (self.wine.clone(), state.path()) {
            (Some(old), Some(new)) if old != new => {
                self.start_wine_change(WineChange::Switch(state), None);
                false
            }
            _ => {
                self.set_wine(state);
                self.sync_launchers();
                true
            }
        }
    }

    pub(crate) fn set_wine(&mut self, state: wine_setup::WineState) {
        self.wine = state.path();
        self.wine_ver = match &state {
            wine_setup::WineState::Ready { version, .. } => version.clone(),
            _ => String::new(),
        };
        if let Some(p) = &self.wine {
            // Remember it, so the next start finds it at once.
            if self.cfg.wine_path.as_deref() != Some(&*p.to_string_lossy()) {
                self.cfg.wine_path = Some(p.to_string_lossy().to_string());
                self.cfg.save();
            }
        }
        self.wine_state = state;
    }

    /// Reads what the engines can do. Touches the disk, so it runs on events (start, rescan,
    /// installs), never per frame.
    pub(crate) fn refresh_engines(&mut self) {
        self.engines = domain::engines::EngineStatus::read();
        self.prime_engine_versions();
    }

    /// The Wine for the bottle on screen (asks for Wine when there is none at all).
    pub(crate) fn need_wine(&mut self) -> Option<PathBuf> {
        if self.wine.is_none() {
            self.modal = Some(Modal::WineHelp);
            return None;
        }
        match self.bottle().cloned() {
            Some(b) => self.bottle_wine(&b),
            None => self.wine.clone(),
        }
    }

    /// Remembers the newest Wine a bottle was opened with, so it is never run with an older one.
    pub(crate) fn note_wine(&mut self, bottle: &Path, wine: &Path) {
        let ver = self.wine_version_of(wine);
        if ver.is_empty() {
            return;
        }
        let s = self.cfg.bottle(bottle);
        let keep = s
            .last_wine
            .as_ref()
            .map(|old| old == &ver || is_downgrade(old, &ver))
            .unwrap_or(false);
        if !keep {
            s.last_wine = Some(ver);
            self.cfg.save();
        }
    }

    /// "wine-11.0" for the usual Wine; other engines are read from what a worker found out
    /// (see [`Self::prime_engine_versions`]). Empty while that is not known yet: asking the
    /// engine here would start a process on the window thread.
    pub(crate) fn wine_version_of(&self, wine: &Path) -> String {
        if Some(wine) == self.wine.as_deref() {
            return self.wine_ver.clone();
        }
        self.engine_versions.get(wine).cloned().unwrap_or_default()
    }

    /// Asks (on a worker, once per engine) the version of the installed engines, so the
    /// window thread never starts `wine --version`.
    pub(crate) fn prime_engine_versions(&mut self) {
        let wanted: Vec<PathBuf> = [self.engines.games.clone(), self.engines.metal.clone()]
            .into_iter()
            .flatten()
            .filter(|p| {
                Some(p.as_path()) != self.wine.as_deref()
                    && !self.engine_versions.contains_key(p)
                    && !self.versions_requested.contains(p)
            })
            .collect();
        if wanted.is_empty() {
            return;
        }
        self.versions_requested.extend(wanted.iter().cloned());
        self.task(move |log| {
            vec![Msg::EngineVersions(
                wanted
                    .into_iter()
                    .map(|p| {
                        let v = wine_setup::probe_logged(&p, log).ok();
                        (p, v)
                    })
                    .collect(),
            )]
        });
    }

    /// The Wine a bottle was last updated with, as this app recorded it.
    pub(crate) fn bottle_last_wine(&self, b: &Bottle) -> Option<String> {
        self.cfg
            .bottle_ref(&b.path)
            .and_then(|s| s.last_wine.clone())
    }

    /// Moves one bottle to another engine, only while nothing runs in it. The check runs on a
    /// worker; the setting changes when it answers. False when it refused for a program in use.
    pub(crate) fn set_bottle_engine(
        &mut self,
        bottle: &Path,
        engine: Option<wine_setup::Engine>,
    ) -> bool {
        let Some(b) = self.bottles.iter().find(|b| b.path == bottle).cloned() else {
            self.apply_bottle_engine(bottle, engine);
            return true;
        };
        // A copy, a save or a delete started during an engine download still holds the bottle:
        // its mark is not replaced, and the change waits (the caller says it was deferred).
        if self.refuse_if_held(&b.path, &b.name) {
            return false;
        }
        // Programs the person opened since asking (or during an engine download) are never
        // killed for this: the change waits until they are closed.
        if self.bottle_in_use(&b.path) {
            self.say(engine_change_in_use_text(&b.name));
            return false;
        }
        let old = self.bottle_wine(&b);
        self.busy
            .insert(b.path.clone(), Self::ENGINE_SWITCH_JOB.into());
        self.task_freeing(OnPanic::busy([b.path.clone()]), move |_| {
            // The window's view of what runs can be seconds old: look again here.
            if old.is_some_and(|old| is_running(&old, &b)) {
                return vec![
                    Msg::Busy(b.path.clone(), None),
                    Msg::Fail(None, engine_change_in_use_text(&b.name)),
                ];
            }
            vec![
                Msg::Busy(b.path.clone(), None),
                Msg::EngineSet(b.path, engine),
            ]
        });
        true
    }

    /// Writes a bottle's engine once its programs are closed.
    pub(crate) fn apply_bottle_engine(
        &mut self,
        bottle: &Path,
        engine: Option<wine_setup::Engine>,
    ) {
        self.cfg.bottle(bottle).engine = engine.map(|e| engine_key(e).to_string());
        self.cfg.save();
        // What was found under the old engine no longer applies, and the next diagnosis must
        // not read the old engine's messages as the new one's.
        self.diagnosis.forget(bottle);
        self.begin_run(bottle);
        self.rescan();
        self.clear_errors(bottle);
        self.done(format!(
            "Esta botella ahora usa «{}».",
            crate::domain::engines::engine_label(engine.map(engine_key))
        ));
    }
}

/// What a bottle's `engine` setting holds for an engine.
fn engine_key(engine: wine_setup::Engine) -> &'static str {
    match engine {
        wine_setup::Engine::Games => "games",
        wine_setup::Engine::Standard => "standard",
        wine_setup::Engine::Metal => "dxmt",
    }
}

/// Why the games engine is not installed or reinstalled by a press: Apple's licence.
pub(crate) fn games_not_installable_text() -> String {
    format!("«{GAMES}» no se instala desde Altrune: es software de Apple y su licencia no permite que Altrune lo descargue.")
}

/// The engine's name as every screen writes it (see `domain::engines`).
fn engine_name(engine: wine_setup::Engine) -> &'static str {
    crate::domain::engines::engine_label(Some(engine_key(engine)))
}

/// Why a bottle is not moved to an engine whose Wine is older than the one it last used.
fn engine_older_text(engine: wine_setup::Engine, last: &str, version: &str) -> String {
    let name = engine_name(engine);
    format!(
        "Esta botella ya se usó con {last} y «{name}» es {version} (más antiguo): podría dañarla. Crea una botella nueva con «{name}»."
    )
}

/// Why a bottle is not moved to an engine that does not start. `reason` is the plain Spanish
/// reason of `wine_setup::probe`, which may already end with a full stop.
fn engine_unusable_text(engine: wine_setup::Engine, reason: &str) -> String {
    format!(
        "No se pudo usar «{}»: {}.",
        engine_name(engine),
        reason.trim_end_matches('.')
    )
}

/// Why the engine of a bottle is not changed while programs run in it.
pub(crate) fn engine_change_in_use_text(bottle: &str) -> String {
    format!(
        "Cierra primero los programas de «{bottle}»: cambiar el motor los cerraría de golpe y perderías lo que no hayas guardado."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engine_change_refusal_names_the_bottle() {
        let t = engine_change_in_use_text("Steam (copia)");
        assert!(t.contains("«Steam (copia)»"));
        assert!(t.contains("Cierra"));
    }

    /// An engine that does not start is explained once, in the words of the graphics list, and
    /// the reason is neither nested in parentheses nor closed by two full stops.
    #[test]
    fn an_engine_that_does_not_start_is_named_as_the_list_names_it() {
        let t = engine_unusable_text(
            wine_setup::Engine::Metal,
            "No arranca en este Mac. Los detalles están en el registro de actividad (Ajustes).",
        );
        assert!(t.starts_with("No se pudo usar «Steam y launchers»: No arranca"));
        assert!(t.ends_with("(Ajustes)."), "{t}");
        assert!(!t.contains("(No arranca"), "{t}");
        assert!(!t.contains(".."), "{t}");
        let older = engine_older_text(wine_setup::Engine::Games, "wine-11.0", "wine-7.7");
        assert!(older.contains("«Juegos DirectX 12»"), "{older}");
        for e in [
            wine_setup::Engine::Games,
            wine_setup::Engine::Standard,
            wine_setup::Engine::Metal,
        ] {
            let all = format!(
                "{}{}",
                engine_unusable_text(e, "x"),
                engine_older_text(e, "a", "b")
            );
            assert!(!all.contains("motor"), "{all}");
        }
    }

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    /// The rule that never moves a bottle to an older Wine than its last one.
    #[test]
    fn an_older_engine_does_not_fit_a_bottle() {
        assert!(version_fits(None, "wine-9.0"));
        assert!(version_fits(Some("wine-11.0"), "wine-11.0"));
        assert!(version_fits(Some("wine-10.0"), "wine-11.0"));
        assert!(!version_fits(
            Some("wine-11.0"),
            "wine-7.7 (Game Porting Toolkit 1.1)"
        ));
        // A version that cannot be read does not block.
        assert!(version_fits(Some("wine-11.0"), ""));
    }

    #[test]
    fn nothing_closes_without_naming_the_open_bottles() {
        let (a, b) = (p("/b/a"), p("/b/b"));
        // Nothing open: nothing to name.
        assert_eq!(plan_wine_close(&[], None), WineClose::Proceed(vec![]));
        // Open and no dialog yet: ask.
        assert_eq!(
            plan_wine_close(std::slice::from_ref(&a), None),
            WineClose::Ask(vec![a.clone()])
        );
        // Named by the dialog: close exactly those.
        assert_eq!(
            plan_wine_close(&[a.clone(), b.clone()], Some(&[b.clone(), a.clone()])),
            WineClose::Proceed(vec![a.clone(), b.clone()])
        );
        // One more bottle opened since the dialog: ask again, with every open bottle.
        assert_eq!(
            plan_wine_close(&[a.clone(), b.clone()], Some(std::slice::from_ref(&a))),
            WineClose::Ask(vec![a, b])
        );
    }

    /// A second install is never queued behind the first: asking says why and changes nothing.
    #[test]
    fn a_reinstall_waits_for_what_runs() {
        assert_eq!(reinstall_blocker(None, None), None);
        // Another install runs, named or not yet.
        let named = reinstall_blocker(Some("video"), None).unwrap();
        assert!(named.contains("(video)"));
        assert!(reinstall_blocker(Some(""), None).is_some());
        // The reinstall itself is closing bottles: a second press does not start another.
        let again = reinstall_blocker(None, Some(App::REINSTALL_JOB));
        assert!(again.is_some_and(|t| t.starts_with("Espera")));
    }

    /// If the search for Wine fails inside, the page goes back to what it knew instead of
    /// staying on «Buscando Wine…».
    #[test]
    fn a_failed_search_for_wine_ends_the_waiting() {
        use wine_setup::WineState;
        let wine = PathBuf::from("/w/wine");
        assert_eq!(
            state_after_failed_lookup(Some(&wine), "wine-11.0"),
            WineState::Ready {
                path: wine,
                version: "wine-11.0".into()
            }
        );
        assert_eq!(state_after_failed_lookup(None, ""), WineState::Missing);
    }

    /// A bottle that waits for an engine stops waiting when the install ends. It is carried out
    /// only when that very engine arrived, so a failed install never leaves a request that a
    /// later install of the same engine would carry out without anyone asking.
    #[test]
    fn a_waiting_bottle_is_settled_when_the_install_ends() {
        use wine_setup::Engine;
        let bottle = PathBuf::from("/b/uno");
        let waiting = || Some((bottle.clone(), Engine::Metal));
        assert_eq!(
            settle_pending(waiting(), Some(Engine::Metal)),
            Some((bottle.clone(), Engine::Metal))
        );
        // Failed or cancelled: nothing arrived.
        assert_eq!(settle_pending(waiting(), None), None);
        // Another engine arrived.
        assert_eq!(settle_pending(waiting(), Some(Engine::Games)), None);
        assert_eq!(settle_pending(None, Some(Engine::Metal)), None);
    }
}
