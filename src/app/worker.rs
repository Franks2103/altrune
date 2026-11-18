//! Background jobs: how a worker is started, and what is freed when it panics.

use super::{App, Msg};
use crate::backend::{push_log, Log};
use crate::wine_setup;
use std::path::{Path, PathBuf};

/// What a worker marked while it works, and so what must be freed if the worker panics before it
/// answers: the busy marks of bottles, the app's own job line, and an install's progress.
#[derive(Clone, Default)]
pub(crate) struct OnPanic {
    pub(crate) busy: Vec<PathBuf>,
    pub(crate) app_job: bool,
    pub(crate) setup: Option<wine_setup::Shared>,
    /// The job is an engine install, so the bottles kept closed for its swap are freed too.
    pub(crate) engine_install: bool,
    /// A bottle this job was making from nothing: if the job panics, its leftovers go to the Trash.
    pub(crate) discard: Option<PathBuf>,
    /// The bottle whose messages a worker was reading for a diagnosis: if the job panics, the
    /// bottle stops showing as being analysed.
    pub(crate) diagnosis: Option<PathBuf>,
    /// The job was looking for Wine: if it panics, the state «buscando» ends.
    pub(crate) wine_lookup: bool,
    /// The job starts the installer of this installation: if it panics, the installation fails
    /// with a card that says so, instead of staying on its last step.
    pub(crate) install: Option<u64>,
    /// The job looks at the bottle being installed into: if it panics, the next look may start.
    pub(crate) install_look: bool,
    /// The job starts (or judges the exit of) this program of this bottle: if it panics, the
    /// «opening» mark goes, so the program may be started again at once.
    pub(crate) opening: Option<(PathBuf, PathBuf)>,
}

impl OnPanic {
    /// A job that only marked these bottles busy.
    pub(crate) fn busy(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            busy: paths.into_iter().collect(),
            ..Self::default()
        }
    }
    /// An install that keeps its progress in `setup`.
    pub(crate) fn setup(setup: &wine_setup::Shared) -> Self {
        Self {
            setup: Some(setup.clone()),
            ..Self::default()
        }
    }
    /// An engine install: like `setup`, and the bottles held back during its swap are freed too.
    pub(crate) fn engine_install(setup: &wine_setup::Shared) -> Self {
        Self {
            engine_install: true,
            ..Self::setup(setup)
        }
    }
    /// The creation of a new bottle at `path` (`fresh` when this attempt made the folder).
    pub(crate) fn creating(path: &Path, fresh: bool) -> Self {
        Self {
            discard: fresh.then(|| path.to_path_buf()),
            ..Self::busy([path.to_path_buf()])
        }
    }
    /// A worker that reads the messages of this bottle for a diagnosis or a report.
    pub(crate) fn diagnosing(bottle: &Path) -> Self {
        Self {
            diagnosis: Some(bottle.to_path_buf()),
            ..Self::default()
        }
    }
    /// The search for Wine on this Mac.
    pub(crate) fn wine_lookup() -> Self {
        Self {
            wine_lookup: true,
            ..Self::default()
        }
    }
    /// A look at the bottle of the installation going on.
    pub(crate) fn install_look() -> Self {
        Self {
            install_look: true,
            ..Self::default()
        }
    }
    /// This `on_panic` with the «opening» mark of `program` in `bottle` freed too. A start with
    /// no file (a tool) has no mark to free.
    pub(crate) fn opening(self, bottle: &Path, program: &Option<PathBuf>) -> Self {
        Self {
            opening: program.as_ref().map(|p| (bottle.to_path_buf(), p.clone())),
            ..self
        }
    }
    /// A job of the app itself, shown on the app's job line.
    pub(crate) fn app_job() -> Self {
        Self {
            app_job: true,
            ..Self::default()
        }
    }
}

/// Runs a worker's closure and turns a panic in it into a message that frees what the job had
/// marked, so a bottle never stays «Creando…» and an install never stays running for good.
fn run_guarded(f: impl FnOnce(&Log) -> Vec<Msg>, log: &Log, on_panic: OnPanic) -> Vec<Msg> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(log))) {
        Ok(msgs) => msgs,
        Err(_) => {
            // What frees the job comes first; the log line is the last, least important step.
            if let Some(setup) = &on_panic.setup {
                let mut g = wine_setup::lock_progress(setup);
                g.running = false;
                g.error = Some(WORKER_FAILED_TEXT.into());
            }
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                push_log(log, "✗ Una tarea en segundo plano falló por dentro")
            }));
            vec![Msg::WorkerFailed(on_panic)]
        }
    }
}

/// What the person reads when a background job panicked.
pub(crate) const WORKER_FAILED_TEXT: &str = "Algo falló por dentro. Vuelve a intentarlo.";

impl App {
    /// Runs `f` on a worker thread; its message comes back to the window.
    pub(crate) fn task(&self, f: impl FnOnce(&Log) -> Vec<Msg> + Send + 'static) {
        self.task_freeing(OnPanic::default(), f);
    }

    /// Like `task`, for a job that marked something as busy: if `f` panics, `on_panic` says what
    /// to free, and the person is told that something failed.
    pub(crate) fn task_freeing(
        &self,
        on_panic: OnPanic,
        f: impl FnOnce(&Log) -> Vec<Msg> + Send + 'static,
    ) {
        let tx = self.tx.clone();
        let log = self.log.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            for m in run_guarded(f, &log, on_panic) {
                let _ = tx.send(m);
            }
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A job that panics answers with a message that frees what it marked, and an install's
    /// progress stops running with an error to show.
    #[test]
    fn a_panicking_worker_frees_what_it_marked() {
        let log = Log::default();
        let setup: wine_setup::Shared = Default::default();
        setup.lock().unwrap().running = true;
        let on_panic = OnPanic {
            busy: vec![PathBuf::from("/b/uno")],
            app_job: true,
            setup: Some(setup.clone()),
            engine_install: true,
            discard: None,
            diagnosis: None,
            wine_lookup: false,
            install: Some(7),
            install_look: false,
            opening: None,
        };
        let msgs = run_guarded(|_| panic!("boom"), &log, on_panic);
        match msgs.as_slice() {
            [Msg::WorkerFailed(p)] => {
                assert_eq!(p.busy, [PathBuf::from("/b/uno")]);
                assert!(p.app_job);
                assert!(p.engine_install);
                assert_eq!(p.install, Some(7));
            }
            _ => panic!("expected one WorkerFailed message"),
        }
        let g = setup.lock().unwrap();
        assert!(!g.running);
        assert_eq!(g.error.as_deref(), Some(WORKER_FAILED_TEXT));
    }

    /// A diagnosis or a report that panics frees the bottle it was reading, and a search for Wine
    /// that panics says so: otherwise the spinner of «Buscar problemas» (or «Buscando Wine…»)
    /// would stay for good.
    #[test]
    fn a_panicking_reading_or_search_says_what_to_free() {
        let bottle = Path::new("/b/uno");
        let msgs = run_guarded(
            |_| panic!("boom"),
            &Log::default(),
            OnPanic::diagnosing(bottle),
        );
        match msgs.as_slice() {
            [Msg::WorkerFailed(p)] => {
                assert_eq!(p.diagnosis.as_deref(), Some(bottle));
                assert!(!p.wine_lookup);
            }
            _ => panic!("expected one WorkerFailed message"),
        }
        let msgs = run_guarded(|_| panic!("boom"), &Log::default(), OnPanic::wine_lookup());
        match msgs.as_slice() {
            [Msg::WorkerFailed(p)] => {
                assert!(p.wine_lookup);
                assert!(p.diagnosis.is_none());
            }
            _ => panic!("expected one WorkerFailed message"),
        }
    }

    /// A job that finishes keeps its own messages.
    #[test]
    fn a_worker_that_finishes_answers_as_usual() {
        let msgs = run_guarded(
            |_| vec![Msg::Rescan, Msg::Rescan],
            &Log::default(),
            OnPanic::default(),
        );
        assert_eq!(msgs.len(), 2);
        assert!(msgs.iter().all(|m| matches!(m, Msg::Rescan)));
    }
}
