//! The steps and progress of an install, shared with the window, and how an install stops.

use super::*;
use crate::backend::{push_log, Log};
// The steps are plain words about an install and live in the domain; they are still reached as
// `wine_setup::Step`.
pub use crate::domain::step::Step;
use std::process::{Child, Command, ExitStatus};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
pub struct Progress {
    pub running: bool,
    pub step: Option<Step>,
    pub downloaded: u64,
    pub total: u64,
    pub flavour: String,
    pub error: Option<String>,
    pub cancel: bool,
    /// Whether this Mac still lacks Rosetta, as the install worker last found out. `None` until
    /// it has asked, so the window never starts a process to find out.
    pub rosetta_needed: Option<bool>,
}

pub type Shared = Arc<Mutex<Progress>>;

/// The shared progress, locked. A worker that panicked while holding the lock leaves it
/// poisoned, but every field is plain data that is valid whenever it is read, so the window
/// reads it anyway instead of panicking too.
pub fn lock_progress(p: &Shared) -> std::sync::MutexGuard<'_, Progress> {
    p.lock().unwrap_or_else(|e| e.into_inner())
}

pub(super) fn set_step(p: &Shared, s: Step) {
    lock_progress(p).step = Some(s);
}

pub(super) fn cancelled(p: &Shared) -> bool {
    p.lock().unwrap_or_else(|e| e.into_inner()).cancel
}

/// Why a download did not finish.
pub(super) enum Stopped {
    Cancelled,
    Failed(String),
}

pub(super) fn install_cancelled(p: &Shared) -> WineState {
    let mut g = p.lock().unwrap_or_else(|e| e.into_inner());
    g.running = false;
    g.error = Some("Instalación cancelada.".into());
    WineState::Missing
}

pub(super) fn install_failed(p: &Shared, log: &Log, msg: String) -> WineState {
    push_log(log, format!("✗ {msg}"));
    let mut g = p.lock().unwrap_or_else(|e| e.into_inner());
    g.error = Some(msg);
    g.running = false;
    WineState::Missing
}

// ---------------------------------------------------------------------------------------
// The child processes of an install
// ---------------------------------------------------------------------------------------

/// What the registry says about one child when it is asked.
pub(super) enum Poll {
    Running,
    /// The child ended (or could not be asked: the error says why) and is no longer tracked.
    Exited(std::io::Result<ExitStatus>),
    /// The child is not tracked any more: it was killed because the app is closing.
    Gone,
}

#[derive(Default)]
struct Tracked {
    /// Set when the app closes: nothing new may start after that.
    stopped: bool,
    next: u64,
    running: Vec<(u64, Child)>,
}

/// The processes an install has running (the download and the unpacking), kept in one
/// place so closing the window can end them. A worker thread dies with the app but the
/// processes it started do not: a download would go on to the end into the app's folder with
/// nobody to check or use it, and an unpacking would go on writing into a folder the next start
/// is about to clean.
pub(super) struct Children(Mutex<Tracked>);

impl Children {
    pub(super) const fn new() -> Self {
        Children(Mutex::new(Tracked {
            stopped: false,
            next: 0,
            running: Vec::new(),
        }))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Tracked> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Starts `cmd` and tracks it; the number is how to ask about it later. Refused once the
    /// registry was stopped.
    pub(super) fn start(&self, cmd: &mut Command) -> std::io::Result<u64> {
        let mut g = self.lock();
        if g.stopped {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "the app is closing",
            ));
        }
        let child = cmd.spawn()?;
        g.next += 1;
        let id = g.next;
        g.running.push((id, child));
        Ok(id)
    }

    /// Asks whether a tracked child ended. The child is reaped when it did, so none is left as
    /// a zombie.
    pub(super) fn poll(&self, id: u64) -> Poll {
        let (mut child, e) = {
            let mut g = self.lock();
            let Some(i) = g.running.iter().position(|(n, _)| *n == id) else {
                return Poll::Gone;
            };
            match g.running[i].1.try_wait() {
                Ok(None) => return Poll::Running,
                Ok(Some(status)) => {
                    g.running.remove(i);
                    return Poll::Exited(Ok(status));
                }
                Err(e) => {
                    // It cannot be asked: end it and reap it rather than leave it behind, but
                    // wait for it only once the registry is unlocked.
                    let (_, mut child) = g.running.remove(i);
                    let _ = child.kill();
                    (child, e)
                }
            }
        };
        end_and_reap(std::slice::from_mut(&mut child), REAP_LIMIT);
        Poll::Exited(Err(e))
    }

    /// Ends one tracked child (an install the person cancelled) and reaps it.
    pub(super) fn kill(&self, id: u64) {
        self.kill_then(id, |mut child| {
            end_and_reap(std::slice::from_mut(&mut child), REAP_LIMIT)
        });
    }

    /// Takes the child out of the registry and hands it to `finish`, which runs after the
    /// registry is unlocked: ending a child can take a while, and every other install command
    /// would wait for it if the lock were held meanwhile. The kill signal itself goes out
    /// before the unlock (it never blocks), so no moment exists in which the child is neither
    /// tracked nor ended and a `stop` would miss it.
    pub(super) fn kill_then(&self, id: u64, finish: impl FnOnce(Child)) {
        let taken = {
            let mut g = self.lock();
            let i = g.running.iter().position(|(n, _)| *n == id);
            i.map(|i| {
                let mut child = g.running.remove(i).1;
                let _ = child.kill();
                child
            })
        };
        if let Some(child) = taken {
            finish(child);
        }
    }

    /// Runs `cmd` to the end and says how it ended, like `Command::status`, but a window that
    /// closes meanwhile can end it. The wait is short at first so quick commands do not slow
    /// the install down.
    pub(super) fn run(&self, cmd: &mut Command) -> std::io::Result<ExitStatus> {
        let id = self.start(cmd)?;
        let mut pause = Duration::from_millis(2);
        loop {
            match self.poll(id) {
                Poll::Running => {
                    std::thread::sleep(pause);
                    pause = (pause * 2).min(Duration::from_millis(100));
                }
                Poll::Exited(r) => return r,
                Poll::Gone => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "the app is closing",
                    ))
                }
            }
        }
    }

    /// The app is closing: ends everything that runs and refuses anything new. Returns how many
    /// children it ended. It does not wait for a child that does not die (one stuck in the
    /// kernel): the app is leaving, and the system reaps what it leaves.
    pub(super) fn stop(&self) -> usize {
        let mut all: Vec<Child> = {
            let mut g = self.lock();
            g.stopped = true;
            g.running
                .drain(..)
                .map(|(_, mut c)| {
                    let _ = c.kill();
                    c
                })
                .collect()
        };
        end_and_reap(&mut all, REAP_LIMIT);
        all.len()
    }

    /// The process numbers of the children that run now.
    #[cfg(test)]
    pub(super) fn pids(&self) -> Vec<u32> {
        self.lock().running.iter().map(|(_, c)| c.id()).collect()
    }
}

/// How long a child that was sent a kill gets to be reaped before the app stops waiting for it.
const REAP_LIMIT: Duration = Duration::from_secs(1);

/// Kills `children` and reaps them, waiting at most `limit` in all for the lot. Never call it
/// with the registry locked.
fn end_and_reap(children: &mut [Child], limit: Duration) {
    for c in children.iter_mut() {
        let _ = c.kill();
    }
    let deadline = std::time::Instant::now() + limit;
    for c in children.iter_mut() {
        while matches!(c.try_wait(), Ok(None)) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

pub(super) static CHILDREN: Children = Children::new();

/// Runs a command of the install to the end (see [`Children::run`]).
pub(super) fn run_status(cmd: &mut Command) -> std::io::Result<ExitStatus> {
    CHILDREN.run(cmd)
}

/// The app is closing: ends the download or unpacking that is running, so nothing the app
/// started goes on after it is gone. Call it once, from the window's exit.
pub fn stop_install_children() -> usize {
    CHILDREN.stop()
}
