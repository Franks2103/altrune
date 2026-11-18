//! Opening programs and Windows tools in a bottle, and closing them.

use super::bottles::busy_text;
use crate::backend::*;
use crate::domain::engines::{steam_moves_to_dxmt, LAUNCHERS};
use crate::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// (bottle folder, program file): what the double-start guard compares.
pub(crate) type StartKey = (PathBuf, PathBuf);

/// What a request to open a program is told, given what is already opening or open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartGuard {
    Go,
    /// The same program is being started right now (or its first process just ended and is
    /// being looked at).
    Opening,
    /// The same program is open in that bottle.
    Open,
}

/// Decides whether a program may start: the same file in the same bottle never starts a second
/// copy while the first one is starting or open. Another program of the same bottle may.
pub(crate) fn start_guard(
    key: &StartKey,
    opening: &HashMap<StartKey, Instant>,
    open: &[StartKey],
) -> StartGuard {
    if opening.contains_key(key) {
        StartGuard::Opening
    } else if open.contains(key) {
        StartGuard::Open
    } else {
        StartGuard::Go
    }
}

/// What «Cerrar programas» does, given whether another job holds the bottle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuitPlan {
    /// Ask the programs to close, and keep the bottle marked until they have.
    Graceful,
    /// Another job holds the bottle (a copy, a close already going): a second graceful close
    /// would free that job's mark when it ends, so it waits.
    Refuse,
    /// Stop the processes at once. It is the way out of a stuck bottle, so it is allowed while
    /// a job holds it, and it never marks the bottle (so it never frees another job's mark).
    Force,
}

pub(crate) fn quit_plan(held_by_job: bool, force: bool) -> QuitPlan {
    match (force, held_by_job) {
        (true, _) => QuitPlan::Force,
        (false, true) => QuitPlan::Refuse,
        (false, false) => QuitPlan::Graceful,
    }
}

/// A start that never answered (its worker died) stops counting after this long.
const OPENING_MAX: Duration = Duration::from_secs(180);

/// After Steam was asked to close politely and the wait ended: what happens next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SteamRestart {
    /// Nothing runs in the bottle any more: open Steam again.
    Relaunch,
    /// The bottle is still in use: leave it alone and tell the person. Closing it would end
    /// every program there, a game included.
    StillOpen,
}

pub(crate) fn after_steam_shutdown(bottle_in_use: bool) -> SteamRestart {
    if bottle_in_use {
        SteamRestart::StillOpen
    } else {
        SteamRestart::Relaunch
    }
}

/// Steam's bottle on "Automático" moves to the DXMT engine only while nothing runs in it: moving
/// closes the bottle first, which would end a game being played.
pub(crate) fn steam_moves_now(moves_to_dxmt: bool, bottle_in_use: bool) -> bool {
    moves_to_dxmt && !bottle_in_use
}

/// A first process that ends within this many seconds either failed to start (when it ends
/// with an error) or handed over to another process (when its bottle stays in use).
pub(crate) const EARLY_EXIT_SECS: u64 = 20;
/// How long to wait after a first process ended before asking whether the bottle is still in
/// use: Wine's server lingers a few seconds after its last process.
pub(crate) const JUDGE_WAIT_SECS: u64 = 5;

/// What is known when a first process has ended, to judge what its ending means. Named, because
/// four yes/no answers in a row would compile in any order and swapping two would silently turn
/// a failure into a handover.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ExitFacts {
    /// The process ended with a success status.
    pub(crate) success: bool,
    /// How long it ran, in whole seconds.
    pub(crate) ran_secs: u64,
    /// The answer after the wait: something still runs in the program's bottle.
    pub(crate) bottle_in_use: bool,
    /// The app itself still follows other programs of that bottle, which then explain why the
    /// bottle is in use.
    pub(crate) others_open: bool,
    /// The bottle was already in use when the program started, so something the app does not
    /// follow may be what keeps it in use: that is no proof of a handover, and the exit code
    /// decides.
    pub(crate) busy_before: bool,
}

/// What a first process ending means.
pub(crate) fn judge_exit(facts: ExitFacts) -> ExitVerdict {
    let ExitFacts {
        success,
        ran_secs,
        bottle_in_use,
        others_open,
        busy_before,
    } = facts;
    if ran_secs >= EARLY_EXIT_SECS {
        ExitVerdict::Ended
    } else if bottle_in_use && !others_open && !busy_before {
        ExitVerdict::HandedOver
    } else if !success {
        ExitVerdict::Failed
    } else {
        ExitVerdict::Ended
    }
}

/// "código 1" or "señal 9": how a process ended, to follow "con un error".
pub(crate) fn exit_phrase(code: Option<i32>, signal: Option<i32>) -> String {
    match (code, signal) {
        (Some(c), _) => format!("código {c}"),
        (None, Some(s)) => format!("señal {s}"),
        (None, None) => "sin código".to_string(),
    }
}

/// The line for an installer or tool that ended badly. `phrase` comes from `exit_phrase`.
pub(crate) fn ended_badly_text(label: &str, phrase: &str) -> String {
    if phrase == "sin código" {
        format!("«{label}» terminó con un error. Mira si se instaló algo en tus programas.")
    } else {
        format!(
            "«{label}» terminó con un error ({phrase}). Mira si se instaló algo en tus programas."
        )
    }
}

/// A recipe counts as applied only when every value it can write was written (`total` counts
/// the writable values, not the ones the app leaves out).
pub(crate) fn recipe_took(written: usize, total: usize) -> bool {
    written == total
}

/// What the worker tells the window after writing a recipe: that it is in place, or that it is
/// not (fully), so that a card showing the installation never claims fixes that are missing.
pub(crate) fn recipe_report(bottle: &Path, id: &str, written: usize, writable: usize) -> Msg {
    if recipe_took(written, writable) {
        Msg::RecipeApplied(bottle.to_path_buf(), id.to_string())
    } else {
        Msg::RecipeNotApplied(bottle.to_path_buf(), id.to_string())
    }
}

/// Why Steam's bottle on "Automático" could not move to "Steam y launchers" (DXMT).
#[derive(Debug, PartialEq)]
enum SteamStays {
    /// The bottle was last used with a Wine newer than the engine's.
    NewerWine,
    /// The engine did not answer when asked for its version.
    EngineFails(String),
}

/// What the person is told when Steam opens on the usual Wine for lack of the DXMT engine.
fn steam_stays_text(why: &SteamStays) -> String {
    let reason = match why {
        SteamStays::NewerWine => {
            format!("esta botella ya se usó con un Wine más nuevo que el de «{LAUNCHERS}»")
        }
        SteamStays::EngineFails(_) => format!("«{LAUNCHERS}» no arrancó"),
    };
    format!(
        "Steam necesita «{LAUNCHERS}» y no se pudo usar en esta botella: {reason}. Steam se abre con el Wine normal y su ventana puede verse negra."
    )
}

/// Everything a worker needs to open a program. The installer flow builds one too, so the
/// struct and its fields are visible to the app.
pub(crate) struct StartJob {
    pub(crate) wine: PathBuf,
    pub(crate) bottle: Bottle,
    pub(crate) file: PathBuf,
    pub(crate) args: String,
    pub(crate) env: Vec<String>,
    pub(crate) label: String,
    pub(crate) rescan_after: bool,
    pub(crate) recipe: Option<recipes::Recipe>,
    /// The file of the program of the list this opens, for the double-start guard.
    pub(crate) program: Option<PathBuf>,
    /// Steam's bottle on "Automático": the DXMT engine it may move to, and the Wine the bottle was
    /// last used with (a move never goes to an older Wine).
    pub(crate) steam_move: Option<(PathBuf, Option<String>)>,
    /// The bottle shows its busy state while the worker prepares it.
    pub(crate) prepare: bool,
}

/// The worker's part of opening a program, in this order: engine check, recipe, start.
fn run_start_job(job: StartJob, log: &Log) -> Vec<Msg> {
    // A file only the flow tests have: its worker panics, to see what the window does then.
    #[cfg(test)]
    if job
        .file
        .file_name()
        .is_some_and(|n| n == "instalador-se-rompe.exe")
    {
        panic!("a worker that panics, on purpose");
    }
    let StartJob {
        mut wine,
        bottle,
        file,
        args,
        env,
        label,
        rescan_after,
        recipe,
        program,
        steam_move,
        prepare,
    } = job;
    let mut msgs = vec![];
    if let Some((metal, last)) = steam_move {
        let version = wine_setup::probe(&metal);
        msgs.push(Msg::EngineVersions(vec![(
            metal.clone(),
            version.clone().ok(),
        )]));
        let problem = match &version {
            Ok(ver) if super::engines::version_fits(last.as_deref(), ver) => None,
            Ok(_) => Some(SteamStays::NewerWine),
            Err(e) => Some(SteamStays::EngineFails(e.clone())),
        };
        if let Some(why) = problem {
            // Steam still opens, on the usual Wine, where its window is black: say why.
            push_log(log, steam_stays_text(&why));
            if let SteamStays::EngineFails(e) = &why {
                push_log(log, format!("Respuesta del motor: {e}"));
            }
            msgs.push(Msg::Fail(Some(bottle.path.clone()), steam_stays_text(&why)));
        } else {
            if steam_moves_now(true, is_running(&wine, &bottle)) {
                msgs.push(Msg::EngineSet(
                    bottle.path.clone(),
                    Some(wine_setup::Engine::Metal),
                ));
                wine = metal;
            } else {
                push_log(
                    log,
                    "Steam ya está en uso: se queda con su motor hasta que se cierre.",
                );
            }
        }
    }
    if let Some(r) = recipe {
        let written = if r.registry.is_empty() {
            0
        } else {
            recipes::apply(&wine, &bottle, &r, log)
        };
        // Values of a kind the app never writes cannot be counted against the recipe.
        let writable = r
            .registry
            .iter()
            .filter(|v| recipes::reg_add_args(v).is_some())
            .count();
        msgs.push(recipe_report(&bottle.path, &r.id, written, writable));
    }
    if prepare {
        msgs.push(Msg::Busy(bottle.path.clone(), None));
    }
    let cmd = open_command(&wine, &bottle, &file, &args, &env);
    let program_path = PathBuf::from(cmd.get_program());
    match spawn_logged(cmd, log, &label) {
        Ok(child) => msgs.push(Msg::Started(Opened {
            bottle: bottle.path.clone(),
            label,
            program,
            child,
            rescan_after,
            wine,
        })),
        Err(e) => msgs.push(Msg::StartFailed(
            bottle.path.clone(),
            program,
            format!(
                "No se pudo abrir {label}. {}",
                io_error_logged(&e, log, &format!("Abrir {label}"), &program_path)
            ),
        )),
    }
    msgs
}

/// The file a handed-over program keeps in the double-start guard. A Windows tool is known by its
/// label, not by a file (see [`App::tool_key`]); its first process ending only means it handed over
/// to another, so the tool stays shown as open but is not guarded: pressing it again opens it
/// again, as it did before the guard, instead of refusing until every program in the bottle ends.
pub(crate) fn handed_over_key(program: Option<PathBuf>) -> Option<PathBuf> {
    program.filter(|p| p.is_absolute())
}

impl App {
    /// The file a Windows tool is known by in the double-start guard. It has no file of its own, so
    /// its label stands in (never an absolute path, so it cannot be a program of the list): pressing
    /// a tool twice opens it once, as pressing a program twice does.
    pub(crate) fn tool_key(label: &str) -> PathBuf {
        PathBuf::from(label)
    }

    /// Picking a program opens it at once and keeps it in "Mis programas"; options live in
    /// the program's right-click menu.
    pub(crate) fn pick_exe(&mut self) {
        let start = self
            .bottle()
            .map(|b| b.path.join("drive_c"))
            .unwrap_or_default();
        if let Some(f) = super::native_dialog(|| {
            rfd::FileDialog::new()
                .set_title("Elige el programa (.exe)")
                .set_directory(start)
                .add_filter("Programas de Windows", &["exe", "msi", "bat", "lnk"])
                .pick_file()
        }) {
            let bottle = self.bottle().map(|b| b.path.clone());
            let Some(bottle) = bottle.filter(|_| self.wine.is_some()) else {
                self.open_directly(f, None);
                return;
            };
            self.run_args.clear();
            self.run_save = true;
            self.run_hud = false;
            self.run_avx = false;
            self.modal = Some(Modal::Run(bottle, f));
            self.apply(Action::RunPicked);
        }
    }

    pub(crate) fn open_run_options(&mut self, i: usize) {
        let Some(b) = self.bottle().cloned() else {
            return;
        };
        if let Some(p) = b.programs.get(i) {
            self.run_args = p.args.clone();
            self.run_save = true;
            self.run_hud = p.env.iter().any(|e| e == ENV_METAL_HUD);
            self.run_avx = p.env.iter().any(|e| e == ENV_ROSETTA_AVX);
            self.modal = Some(Modal::Run(b.path.clone(), p.path.clone()));
        }
    }

    pub(crate) fn run_picked(&mut self) {
        let Some(Modal::Run(bottle, f)) = self.modal.clone() else {
            return;
        };
        self.modal = None;
        // The dialog acts on the bottle it was opened for, and only if it is still one of ours.
        let Some(b) = self.bottle_at(&bottle).cloned() else {
            self.fail(None, "Esa botella cambió. Vuelve a elegir el programa.");
            return;
        };
        let Some(wine) = self.bottle_wine(&b) else {
            return;
        };
        // Keep the name it already has in the list (a shortcut opened with options).
        let name = b
            .programs
            .iter()
            .find(|p| p.path == f)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| {
                f.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            });
        if !self.may_start(&b, Some(&f), &name, true) {
            return;
        }
        let mut env = vec![];
        if self.run_hud {
            env.push(ENV_METAL_HUD.to_string());
        }
        if self.run_avx {
            env.push(ENV_ROSETTA_AVX.to_string());
        }
        if self.run_save {
            let s = self.cfg.bottle(&b.path);
            // A program is its file: saving it again updates its entry, and another program with
            // the same name is left alone.
            s.save_program(SavedProgram {
                name: name.clone(),
                path: f.to_string_lossy().to_string(),
                args: self.run_args.clone(),
                env: env.clone(),
            });
            s.show_file(&b.path, &f);
            self.cfg.save();
            self.rescan();
        }
        let cmd = open_command(&wine, &b, &f, &self.run_args, &env);
        self.launch(b.path.clone(), wine, cmd, name, false, Some(f));
    }

    pub(crate) fn run_program(&mut self, i: usize) {
        let Some(_) = self.need_wine() else {
            return;
        };
        let Some(b) = self.bottle().cloned() else {
            return;
        };
        if let Some(p) = b.programs.get(i).cloned() {
            self.open_program(b, p);
        }
    }

    /// Opens a program of a bottle's list. Nothing here waits on Wine: whatever needs a process
    /// (the engine check, the recipe, the start itself) runs on a worker that answers with a
    /// message.
    pub(crate) fn open_program(&mut self, b: Bottle, p: Program) {
        let Some(wine) = self.bottle_wine(&b) else {
            return;
        };
        let steam = opens_steam(&b.path, &p.path);
        // Steam's window only draws on the DXMT engine (see `steam_launch`): a bottle still on
        // "Automático" moves to it, when the worker finds that it fits and nothing runs in the bottle.
        // An engine the user chose is kept.
        let mut steam_move = None;
        if steam {
            let engine = self.cfg.bottle_ref(&b.path).and_then(|s| s.engine.clone());
            if steam_moves_to_dxmt(engine.as_deref()) {
                match wine_setup::Engine::Metal.installed() {
                    Some(metal) => steam_move = Some((metal, self.bottle_last_wine(&b))),
                    None if wine_setup::games_engine_supported() => {
                        self.fail(Some(&b.path), format!("Steam necesita «{LAUNCHERS}» para dibujar su ventana. Elígelo en «Gráficos» de esta botella y ábrelo otra vez."));
                        return;
                    }
                    None => {}
                }
            }
        }
        // A known program in a bottle that never got its fixes gets them once, before it opens.
        // They are recorded as applied only after the worker wrote them.
        let recipe = recipes::for_program(&p.name).filter(|r| {
            !self
                .cfg
                .bottle_ref(&b.path)
                .map(|s| s.recipes.contains(&r.id))
                .unwrap_or(false)
        });
        // Steam is single-instance: a second click only starts a steam.exe that hands over to
        // the open one, so it is not held back like the other programs.
        if !self.may_start(&b, Some(&p.path), &p.name, !steam) {
            return;
        }
        self.start_job(StartJob {
            wine,
            bottle: b,
            file: p.path.clone(),
            args: p.args.clone(),
            env: p.env.clone(),
            label: p.name.clone(),
            rescan_after: false,
            recipe,
            program: Some(p.path.clone()),
            steam_move,
            prepare: false,
        });
    }

    /// The tile menu's «Reiniciar Steam»: asks Steam to close by itself, waits, and opens it
    /// again. If the bottle is still in use afterwards nothing is forced: the person is told
    /// that «Cerrar programas» closes everything in it.
    pub(crate) fn restart_steam(&mut self, i: usize) {
        let Some(wine) = self.need_wine() else {
            return;
        };
        let Some(b) = self.bottle().cloned() else {
            return;
        };
        let Some(p) = b.programs.get(i).cloned() else {
            return;
        };
        if !opens_steam(&b.path, &p.path) {
            return;
        }
        if self.busy.contains_key(&b.path) {
            self.say(format!(
                "«{}» está ocupada ahora mismo: espera a que termine.",
                b.name
            ));
            return;
        }
        self.say("Reiniciando Steam…");
        self.busy
            .insert(b.path.clone(), "Reiniciando Steam…".into());
        self.begin_run(&b.path);
        self.task_freeing(OnPanic::busy([b.path.clone()]), move |log| {
            if is_running(&wine, &b) {
                if let Some(exe) = steam_exe(&b.path) {
                    let mut c = bottle_cmd(&wine, &b);
                    c.arg(&exe).arg("-shutdown");
                    run_logged(c, log, "Cerrar Steam");
                    let t = Instant::now();
                    while is_running(&wine, &b) && t.elapsed().as_secs() < 20 {
                        std::thread::sleep(Duration::from_millis(500));
                    }
                }
            }
            match after_steam_shutdown(is_running(&wine, &b)) {
                SteamRestart::Relaunch => {
                    vec![Msg::Busy(b.path.clone(), None), Msg::OpenProgram(b.path, p)]
                }
                SteamRestart::StillOpen => vec![
                    Msg::Busy(b.path.clone(), None),
                    Msg::Status(
                        "La botella sigue en uso, así que no volví a abrir Steam. Haz clic en Steam para abrirlo otra vez; «Cerrar programas» cierra todo lo que hay abierto en esta botella, juegos incluidos."
                            .into(),
                    ),
                ],
            }
        });
    }

    pub(crate) fn hide_program(&mut self, i: usize) {
        let Some(b) = self.bottle().cloned() else {
            return;
        };
        if let Some(p) = b.programs.get(i) {
            let s = self.cfg.bottle(&b.path);
            // Always by file: two programs can share a name, and only this one is removed.
            s.remove_program(&b.path, p);
            self.cfg.save();
            self.rescan();
        }
    }

    pub(crate) fn run_tool(&mut self, t: Tool) {
        let Some(wine) = self.need_wine() else { return };
        let Some(b) = self.bottle().cloned() else {
            return;
        };
        let key = Self::tool_key(t.label());
        if !self.may_start(&b, Some(&key), t.label(), true) {
            return;
        }
        if matches!(t, Tool::Terminal) {
            let dropped = crate::backend::terminal_dropped_names(&wine);
            if !dropped.is_empty() {
                let note = dropped_names_note("la terminal", &dropped);
                push_tool_log(&self.log, note);
            }
        }
        let cmd = t.command(&wine, &b);
        self.launch(
            b.path.clone(),
            wine,
            cmd,
            t.label().to_string(),
            matches!(t, Tool::Uninstaller),
            Some(key),
        );
    }

    /// The programs of a bottle that is about to be closed on purpose: their first processes
    /// ending is what the person asked for, not a failure.
    pub(crate) fn mark_closing(&mut self, bottle: &Path) {
        for r in self.running.iter_mut().filter(|r| r.bottle == bottle) {
            r.closing = true;
        }
    }

    pub(crate) fn quit_programs(&mut self, force: bool) {
        let Some(wine) = self.need_wine() else { return };
        let Some(b) = self.bottle().cloned() else {
            return;
        };
        let plan = quit_plan(self.busy.contains_key(&b.path), force);
        if plan == QuitPlan::Refuse {
            self.say(busy_text(&b.name));
            return;
        }
        // The first processes ending now is what the person asked for, not a failure.
        for r in self.running.iter_mut().filter(|r| r.bottle == b.path) {
            r.closing = true;
            if force {
                // The killed child is reaped by the `try_wait` poll on the next tick: waiting
                // here could freeze the window if the process is stuck in the kernel.
                let _ = r.child.kill();
            }
        }
        self.handed_over.retain(|h| h.bottle != b.path);
        self.say(if force {
            "Forzando el cierre…"
        } else {
            "Cerrando los programas…"
        });
        if plan == QuitPlan::Graceful {
            self.busy.insert(b.path.clone(), Self::CLOSING.into());
        }
        // Without `force` the bottle was marked busy above, so a panic must free it.
        let on_panic = if plan == QuitPlan::Graceful {
            OnPanic::busy([b.path.clone()])
        } else {
            OnPanic::default()
        };
        self.task_freeing(on_panic, move |log| {
            let forced = quit_bottle(&wine, &b, force, log);
            let mut msgs = vec![];
            // Only a close that marked the bottle frees it: a forced one never marked it, and
            // freeing here would free the mark of whatever else holds the bottle.
            if plan == QuitPlan::Graceful {
                msgs.push(Msg::Busy(b.path.clone(), None));
            }
            msgs.push(if forced && !force {
                Msg::Status(format!(
                    "No se cerró en {QUIT_WAIT_SECS} segundos, así que se forzó el cierre."
                ))
            } else {
                Msg::Done("Programas cerrados.".into())
            });
            msgs
        });
    }

    /// Starts a ready command (a tool, or a program picked with options) on a worker.
    pub(crate) fn launch(
        &mut self,
        bottle: PathBuf,
        wine: PathBuf,
        cmd: std::process::Command,
        label: String,
        rescan_after: bool,
        program: Option<PathBuf>,
    ) {
        self.begin_run(&bottle);
        self.say(format!("Abriendo {label}…"));
        let on_panic = OnPanic::default().opening(&bottle, &program);
        let program_path = PathBuf::from(cmd.get_program());
        self.task_freeing(on_panic, move |log| match spawn_logged(cmd, log, &label) {
            Ok(child) => vec![Msg::Started(Opened {
                bottle,
                label,
                program,
                child,
                rescan_after,
                wine,
            })],
            Err(e) => vec![Msg::StartFailed(
                bottle,
                program,
                format!(
                    "No se pudo abrir {label}. {}",
                    io_error_logged(&e, log, &format!("Abrir {label}"), &program_path)
                ),
            )],
        });
    }

    /// The programs shown as open in a bottle: those the app follows, and those whose first
    /// process ended while the bottle stays in use.
    pub(crate) fn running_in(&self, p: &Path) -> Vec<&str> {
        self.running
            .iter()
            .filter(|r| r.bottle == p)
            .map(|r| r.label.as_str())
            .chain(
                self.handed_over
                    .iter()
                    .filter(|h| h.bottle == p)
                    .map(|h| h.label.as_str()),
            )
            .collect()
    }

    /// (bottle, file) of every program that is open now, for the double-start guard.
    fn open_keys(&self) -> Vec<StartKey> {
        self.running
            .iter()
            .filter_map(|r| Some((r.bottle.clone(), r.program.clone()?)))
            .chain(
                self.handed_over
                    .iter()
                    .filter_map(|h| Some((h.bottle.clone(), h.program.clone()?))),
            )
            .collect()
    }

    /// Says whether something may start in this bottle now, and when `guard` is set and a file
    /// is given, remembers that it is starting. A bottle that a worker is busy with (copying,
    /// closing, restarting) takes no new program, and the same program never starts twice.
    pub(crate) fn may_start(
        &mut self,
        b: &Bottle,
        program: Option<&Path>,
        label: &str,
        guard: bool,
    ) -> bool {
        if self.busy.contains_key(&b.path) {
            self.say(format!(
                "«{}» está ocupada ahora mismo: espera a que termine.",
                b.name
            ));
            return false;
        }
        // Nothing runs in a bottle that never finished being made, whichever way it is asked.
        if self.refuse_if_unfinished(b) {
            return false;
        }
        let Some(file) = program.filter(|_| guard) else {
            return true;
        };
        let key = (b.path.clone(), file.to_path_buf());
        match start_guard(&key, &self.opening, &self.open_keys()) {
            StartGuard::Go => {}
            StartGuard::Opening => {
                self.say(format!("{label} ya se está abriendo."));
                return false;
            }
            StartGuard::Open => {
                self.say(format!("{label} ya está abierto."));
                return false;
            }
        }
        self.opening.insert(key, Instant::now());
        true
    }

    /// Forgets starts that never answered.
    pub(crate) fn prune_opening(&mut self) {
        self.opening
            .retain(|_, since| since.elapsed() < OPENING_MAX);
    }

    fn start_job(&mut self, mut job: StartJob) {
        self.begin_run(&job.bottle.path);
        let writes = job
            .recipe
            .as_ref()
            .filter(|r| !r.registry.is_empty())
            .map(|r| r.name.clone());
        job.prepare = writes.is_some() || job.steam_move.is_some();
        self.say(match writes {
            Some(name) => format!("Preparando la botella para {name}…"),
            None => format!("Abriendo {}…", job.label),
        });
        let on_panic = if job.prepare {
            self.busy
                .insert(job.bottle.path.clone(), "Preparando…".into());
            OnPanic::busy([job.bottle.path.clone()])
        } else {
            OnPanic::default()
        };
        let on_panic = on_panic.opening(&job.bottle.path, &job.program);
        let on_panic = self.guard_installer_job(&job.bottle.path, &job.program, on_panic);
        self.task_freeing(on_panic, move |log| run_start_job(job, log));
    }

    /// Starts a program on a worker thread, writing its recipe into the bottle first if needed.
    /// False when it did not start (the same program is already opening, or the bottle is busy).
    pub(crate) fn start_with_recipe(&mut self, job: StartJob) -> bool {
        if !self.may_start(&job.bottle, Some(&job.file), &job.label, true) {
            return false;
        }
        self.start_job(job);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_on_plain_wine_says_why() {
        let newer = steam_stays_text(&SteamStays::NewerWine);
        assert!(newer.contains("más nuevo"));
        assert!(newer.contains("negra"));
        let broken = steam_stays_text(&SteamStays::EngineFails("x".into()));
        assert!(broken.contains("no arrancó"));
        // The engine's raw answer stays in the log, not in the banner.
        assert!(!broken.contains(": x"));
    }

    /// «Cerrar programas» never starts on top of another job, and «Forzar cierre» (the way out of
    /// a stuck bottle) never marks the bottle, so it never frees the mark of the job that holds it.
    #[test]
    fn closing_a_held_bottle_waits_and_forcing_never_marks() {
        assert_eq!(quit_plan(false, false), QuitPlan::Graceful);
        assert_eq!(quit_plan(true, false), QuitPlan::Refuse);
        assert_eq!(quit_plan(false, true), QuitPlan::Force);
        assert_eq!(quit_plan(true, true), QuitPlan::Force);
    }

    /// A Windows tool is told apart by its label, and its label is never the file of a program:
    /// pressing «Panel de control» twice opens it once, and another tool still opens.
    #[test]
    fn a_tool_does_not_open_twice() {
        let keys: Vec<PathBuf> = Tool::ALL.iter().map(|t| App::tool_key(t.label())).collect();
        for (i, k) in keys.iter().enumerate() {
            assert!(k.is_relative(), "{k:?}");
            assert!(!keys[..i].contains(k), "two tools share {k:?}");
        }
        let bottle = "/b/uno";
        let panel = (
            PathBuf::from(bottle),
            App::tool_key(Tool::ControlPanel.label()),
        );
        let mut opening: HashMap<StartKey, Instant> = HashMap::new();
        assert_eq!(start_guard(&panel, &opening, &[]), StartGuard::Go);
        opening.insert(panel.clone(), Instant::now());
        assert_eq!(start_guard(&panel, &opening, &[]), StartGuard::Opening);
        let task = (
            PathBuf::from(bottle),
            App::tool_key(Tool::TaskManager.label()),
        );
        assert_eq!(start_guard(&task, &opening, &[]), StartGuard::Go);
        // Once it is open, the second press is told so.
        opening.clear();
        assert_eq!(
            start_guard(&panel, &opening, std::slice::from_ref(&panel)),
            StartGuard::Open
        );
    }

    fn key(b: &str, f: &str) -> StartKey {
        (PathBuf::from(b), PathBuf::from(f))
    }

    /// The same program in the same bottle never starts twice; another program may.
    #[test]
    fn a_program_does_not_start_twice_in_a_bottle() {
        let mut opening: HashMap<StartKey, Instant> = HashMap::new();
        let open = vec![key("/b/one", "/b/one/game.exe")];
        // Nothing going on: go.
        assert_eq!(
            start_guard(&key("/b/one", "/b/one/app.exe"), &opening, &[]),
            StartGuard::Go
        );
        // Already open.
        assert_eq!(
            start_guard(&key("/b/one", "/b/one/game.exe"), &opening, &open),
            StartGuard::Open
        );
        // A different program in the same bottle, and the same file in another bottle, may start.
        assert_eq!(
            start_guard(&key("/b/one", "/b/one/app.exe"), &opening, &open),
            StartGuard::Go
        );
        assert_eq!(
            start_guard(&key("/b/two", "/b/one/game.exe"), &opening, &open),
            StartGuard::Go
        );
        // Starting right now (a click that a worker has not answered yet).
        opening.insert(key("/b/one", "/b/one/app.exe"), Instant::now());
        assert_eq!(
            start_guard(&key("/b/one", "/b/one/app.exe"), &opening, &open),
            StartGuard::Opening
        );
        // Starting wins over open: the message says it is still opening.
        opening.insert(key("/b/one", "/b/one/game.exe"), Instant::now());
        assert_eq!(
            start_guard(&key("/b/one", "/b/one/game.exe"), &opening, &open),
            StartGuard::Opening
        );
    }

    /// Restarting Steam never ends the other programs of its bottle.
    #[test]
    fn a_steam_restart_never_forces_the_bottle_closed() {
        assert_eq!(after_steam_shutdown(false), SteamRestart::Relaunch);
        assert_eq!(after_steam_shutdown(true), SteamRestart::StillOpen);
    }

    /// Moving Steam's bottle to DXMT closes it, so it waits for a bottle nobody is using.
    #[test]
    fn steam_moves_to_dxmt_only_in_a_free_bottle() {
        assert!(steam_moves_now(true, false));
        assert!(!steam_moves_now(true, true));
        assert!(!steam_moves_now(false, false));
    }

    #[test]
    fn a_quick_exit_is_judged_by_its_bottle() {
        use ExitVerdict::*;
        // A quick, clean exit with the bottle free and nothing else open: the base of each case.
        let quick = ExitFacts {
            success: true,
            ran_secs: 4,
            bottle_in_use: false,
            others_open: false,
            busy_before: false,
        };
        // A launcher that hands over: its first process ends, the bottle stays in use.
        assert_eq!(
            judge_exit(ExitFacts {
                bottle_in_use: true,
                ..quick
            }),
            HandedOver
        );
        assert_eq!(
            judge_exit(ExitFacts {
                success: false,
                bottle_in_use: true,
                ..quick
            }),
            HandedOver
        );
        // Quick error and nothing runs any more: it failed.
        assert_eq!(
            judge_exit(ExitFacts {
                success: false,
                ..quick
            }),
            Failed
        );
        // Another program of the app keeps the bottle in use: that explains nothing.
        assert_eq!(
            judge_exit(ExitFacts {
                success: false,
                bottle_in_use: true,
                others_open: true,
                ..quick
            }),
            Failed
        );
        assert_eq!(
            judge_exit(ExitFacts {
                bottle_in_use: true,
                others_open: true,
                ..quick
            }),
            Ended
        );
        // Quick and clean, bottle free: just ended.
        assert_eq!(judge_exit(quick), Ended);
        // After a while nothing is reported either way.
        assert_eq!(
            judge_exit(ExitFacts {
                success: false,
                ran_secs: EARLY_EXIT_SECS,
                ..quick
            }),
            Ended
        );
        assert_eq!(
            judge_exit(ExitFacts {
                ran_secs: 300,
                bottle_in_use: true,
                ..quick
            }),
            Ended
        );
        // The bottle was already in use before the program started (Steam or a game the app
        // does not follow): that is no handover, so a quick error is still a failure.
        assert_eq!(
            judge_exit(ExitFacts {
                success: false,
                bottle_in_use: true,
                busy_before: true,
                ..quick
            }),
            Failed
        );
        assert_eq!(
            judge_exit(ExitFacts {
                bottle_in_use: true,
                busy_before: true,
                ..quick
            }),
            Ended
        );
    }

    #[test]
    fn exits_are_written_with_their_code_or_signal() {
        assert_eq!(exit_phrase(Some(0), None), "código 0");
        assert_eq!(exit_phrase(Some(1), Some(9)), "código 1");
        assert_eq!(exit_phrase(None, Some(9)), "señal 9");
        assert_eq!(exit_phrase(None, None), "sin código");
        assert_eq!(
            ended_badly_text("Setup", &exit_phrase(Some(1), None)),
            "«Setup» terminó con un error (código 1). Mira si se instaló algo en tus programas."
        );
        assert_eq!(
            ended_badly_text("Setup", &exit_phrase(None, None)),
            "«Setup» terminó con un error. Mira si se instaló algo en tus programas."
        );
    }

    /// A recipe is recorded only when all of its values were written.
    #[test]
    fn a_recipe_counts_only_when_written() {
        assert!(recipe_took(3, 3));
        assert!(recipe_took(0, 0));
        assert!(!recipe_took(2, 3));
        assert!(!recipe_took(0, 3));
    }

    /// The window is told whether the fixes are in place, whichever way the writing ended.
    #[test]
    fn the_worker_reports_whether_the_recipe_took() {
        let bottle = Path::new("/b/Steam");
        assert!(matches!(
            recipe_report(bottle, "steam", 3, 3),
            Msg::RecipeApplied(b, id) if b == bottle && id == "steam"
        ));
        for (written, writable) in [(0, 3), (2, 3)] {
            assert!(matches!(
                recipe_report(bottle, "steam", written, writable),
                Msg::RecipeNotApplied(b, id) if b == bottle && id == "steam"
            ));
        }
    }

    /// A tool whose first process handed over is not kept in the guard, but a program is.
    #[test]
    fn a_handed_over_tool_is_not_guarded() {
        let tool = App::tool_key("Símbolo del sistema");
        assert_eq!(handed_over_key(Some(tool)), None);
        let exe = PathBuf::from("/Applications/Juego/juego.exe");
        assert_eq!(handed_over_key(Some(exe.clone())), Some(exe));
        assert_eq!(handed_over_key(None), None);
    }
}
