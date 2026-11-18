//! What a worker tells the window when it is done: one message per thing that changed.

use super::{Exit, OnPanic, Opened, WineChange};
use crate::backend::Program;
use crate::diagnostics;
use crate::domain;
use crate::wine_setup;
use eframe::egui;
use std::path::PathBuf;

pub(crate) enum Msg {
    /// An information line for the status bar.
    Status(String),
    /// Something finished well: a line for the status bar.
    Done(String),
    /// winecfg applied this Windows version to the bottle: now it is saved and shown.
    WindowsSet(PathBuf, String),
    /// Something failed (the bottle it concerns, when there is one): the error stays visible
    /// until the person closes it.
    Fail(Option<PathBuf>, String),
    /// The app itself is busy with something that is not a bottle's job (its label, or none
    /// when it ended).
    AppJob(Option<String>),
    Busy(PathBuf, Option<String>),
    Rescan,
    WineDetected(wine_setup::WineState, bool),
    /// A bottle finished being created (path, worked).
    Created(PathBuf, bool),
    Launchers(usize),
    EngineInstalled(wine_setup::WineState),
    /// Video (GStreamer) was added to the DXMT engine (or could not be).
    VideoAdded(wine_setup::WineState),
    /// A program started by a worker (after its recipe was written).
    Started(Opened),
    /// A worker could not start a program (bottle, the program's file, what to say).
    StartFailed(PathBuf, Option<PathBuf>, String),
    /// A recipe's values were written into a bottle (bottle, recipe id).
    RecipeApplied(PathBuf, String),
    /// A recipe's values could not all be written into a bottle (bottle, recipe id): the
    /// program still opens, but its fixes are not in place.
    RecipeNotApplied(PathBuf, String),
    /// A worker asks to open a program of the list again (after Steam closed for a restart).
    OpenProgram(PathBuf, Program),
    /// A program's first process ended and a worker looked at its bottle.
    Exited(Exit),
    /// Engine versions asked on a worker (path, version or None when it did not start).
    EngineVersions(Vec<(PathBuf, Option<String>)>),
    /// An engine was asked for its version before a bottle moves to it
    /// (bottle, engine, the Wine the bottle was last used with, the answer).
    EngineChecked(
        PathBuf,
        wine_setup::Engine,
        Option<String>,
        Result<String, String>,
    ),
    /// The programs of a bottle are closed: it can use this engine now.
    EngineSet(PathBuf, Option<wine_setup::Engine>),
    /// A Wine file picked by hand was asked for its version.
    WinePicked(PathBuf, Result<String, String>),
    /// The bottles that use the Wine being replaced are closed: the change can finish.
    WineChangeReady(WineChange),
    /// Programs that were not named are open in the bottles of the Wine being replaced
    /// (the change, the bottles).
    AskCloseForWine(WineChange, Vec<(PathBuf, String)>),
    /// A bottle was moved to the Trash (folder, name, result).
    Deleted(PathBuf, String, Result<(), String>),
    /// One of the app's own bottles was duplicated (original, copy, result).
    Duplicated(PathBuf, PathBuf, Result<(), String>),
    /// Bottles whose Wine is running right now.
    RunningSet(Vec<PathBuf>),
    /// What the diagnosis found in one bottle's messages.
    Diagnosis(diagnostics::Diagnosis),
    /// Program pictures decoded on a worker, ready to become textures.
    IconsLoaded(Vec<(PathBuf, Option<egui::ColorImage>)>),
    /// A worker looked at the bottle being installed into (installation, whether Wine still runs
    /// in it, the programs it holds).
    InstallScan(u64, bool, domain::install::Programs),
    /// A worker panicked before it could answer: what it had marked as busy is freed.
    WorkerFailed(OnPanic),
}
