//! Installing a program, as data: what kind of file the user gave, which bottle it should go to
//! and why, and where an installation stands. Everything is decided from facts (the first bytes
//! of the file, what the bottles hold, what a process did); nothing is assumed to have worked.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The programs a bottle holds, by identity (the file of the shortcut in the Start Menu, or the
/// program itself when it was added by hand) with the name each is shown by.
pub type Programs = BTreeMap<String, String>;

// ------------------------------------------------------------------ the file

/// What a file is, as far as its name and first bytes show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// A Windows program (.exe that starts with the "MZ" mark). `installer_like`: its name says
    /// "setup" or "install"; the content is not inspected for that.
    Exe { installer_like: bool },
    /// A Windows Installer package (.msi).
    Msi,
    /// A script (.bat).
    Script,
    /// A shortcut (.lnk, .url).
    Shortcut,
    /// Not something this app opens; the reason is for the user.
    Unsupported(&'static str),
}

/// The processor a Windows program was built for (from its PE header).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Machine {
    X86,
    X64,
    Arm64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detected {
    pub kind: FileKind,
    /// Only for programs whose header could be read.
    pub machine: Option<Machine>,
}

impl Detected {
    /// What the file is, in the user's words.
    pub fn label(&self) -> String {
        let bits = match self.machine {
            Some(Machine::X86) => " · 32 bits",
            Some(Machine::X64) => " · 64 bits",
            Some(Machine::Arm64) => " · ARM",
            None => "",
        };
        match &self.kind {
            FileKind::Exe {
                installer_like: true,
            } => format!("Instalador de Windows (.exe){bits}"),
            FileKind::Exe {
                installer_like: false,
            } => format!("Programa de Windows (.exe){bits}"),
            FileKind::Msi => "Paquete de instalación de Windows (.msi)".into(),
            FileKind::Script => "Script de Windows (.bat)".into(),
            FileKind::Shortcut => "Acceso directo de Windows".into(),
            FileKind::Unsupported(_) => "No se puede abrir aquí".into(),
        }
    }

    /// Something the user should know before starting, when there is something.
    pub fn warning(&self) -> Option<&'static str> {
        match (&self.kind, self.machine) {
            (FileKind::Unsupported(why), _) => Some(why),
            (_, Some(Machine::Arm64)) => Some(
                "Este programa es para Windows en ARM. Los motores de esta app ejecutan programas de Intel (x86 o x64): busca esa versión en la página del programa.",
            ),
            _ => None,
        }
    }

    pub fn supported(&self) -> bool {
        !matches!(self.kind, FileKind::Unsupported(_))
    }

    /// Whether to look for new programs when it ends (an installer) or to keep the file itself
    /// in the library (a program that needs no installing).
    pub fn installs(&self) -> bool {
        matches!(
            self.kind,
            FileKind::Msi
                | FileKind::Exe {
                    installer_like: true
                }
        )
    }
}

/// What a request to open or install a file leads to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// An installation is already going on: it is shown, and nothing else starts.
    Running,
    /// The file is not something to open here: the reason is shown and nothing starts.
    Refused(&'static str),
    /// The person is asked where it goes. `installer`: look for new programs afterwards.
    Ask { installer: bool },
}

/// Where every way of giving a file to the app ends up: the picker, a drop, Finder and the
/// «Instalar» buttons all ask this one question.
pub fn entry(detected: &Detected, installation_running: bool) -> Entry {
    if installation_running {
        return Entry::Running;
    }
    match &detected.kind {
        FileKind::Unsupported(why) => Entry::Refused(why),
        _ => Entry::Ask {
            installer: detected.installs(),
        },
    }
}

/// Whether a file Finder hands over (`--open`) goes through the window: installers, to show
/// their progress, and files that cannot be opened, to say why. Any other program opens at once
/// without a window.
pub fn through_window(detected: &Detected) -> bool {
    !detected.supported() || detected.installs()
}

const UNREADABLE: &str =
    "No pude leer el archivo. Mira que siga ahí, que tengas permiso para abrirlo y que la descarga haya terminado.";

const NOT_WINDOWS: &str =
    "Tiene nombre de programa de Windows, pero su contenido no lo es. Puede que la descarga no terminara: bájalo otra vez.";

/// What a file is that could not be read at all: not the same as a file that is not Windows.
/// Reading the file is the backend's job (`backend::classify_file`); the domain only judges the
/// bytes it is given.
pub fn unreadable() -> Detected {
    Detected {
        kind: FileKind::Unsupported(UNREADABLE),
        machine: None,
    }
}

/// Says what a file is from its name and its first bytes (see `backend::classify_file`).
pub fn classify_bytes(name: &str, head: &[u8]) -> Detected {
    let lower = name.to_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    let plain = |kind| Detected {
        kind,
        machine: None,
    };
    match ext {
        "exe" => {
            if !head.starts_with(b"MZ") {
                return plain(FileKind::Unsupported(NOT_WINDOWS));
            }
            Detected {
                kind: FileKind::Exe {
                    installer_like: ["setup", "install", "instal"]
                        .iter()
                        .any(|w| lower.contains(w)),
                },
                machine: pe_machine(head),
            }
        }
        "msi" => {
            if head.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
                plain(FileKind::Msi)
            } else {
                plain(FileKind::Unsupported(NOT_WINDOWS))
            }
        }
        "bat" => plain(FileKind::Script),
        "lnk" | "url" => plain(FileKind::Shortcut),
        "dmg" | "pkg" | "app" => plain(FileKind::Unsupported(
            "Es un programa de Mac: ábrelo directamente desde Finder, no necesita Altrune.",
        )),
        "zip" | "rar" | "7z" => plain(FileKind::Unsupported(
            "Es un archivo comprimido. Ábrelo con doble clic en Finder y elige el instalador que trae dentro.",
        )),
        "iso" => plain(FileKind::Unsupported(
            "Es la imagen de un disco. Ábrela con doble clic en Finder y arrastra aquí el disco que aparece.",
        )),
        _ => plain(FileKind::Unsupported(
            "No es un archivo de Windows que esta app sepa abrir (.exe, .msi o .bat).",
        )),
    }
}

/// The machine field of a PE header, when the header is within `head`.
fn pe_machine(head: &[u8]) -> Option<Machine> {
    let at = u32::from_le_bytes(head.get(0x3C..0x40)?.try_into().ok()?) as usize;
    let pe = head.get(at..at + 6)?;
    if &pe[..4] != b"PE\0\0" {
        return None;
    }
    match u16::from_le_bytes([pe[4], pe[5]]) {
        0x014C => Some(Machine::X86),
        0x8664 => Some(Machine::X64),
        0xAA64 => Some(Machine::Arm64),
        _ => None,
    }
}

// ------------------------------------------------------------------ the bottle

/// Where a program goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Existing(PathBuf),
    /// A bottle to be made, with this name.
    New(String),
}

/// What is known about a bottle when choosing where to install.
#[derive(Clone, Debug)]
pub struct BottleOption {
    pub path: PathBuf,
    pub name: String,
    /// The fixes of the program being installed were already written into this bottle.
    pub prepared: bool,
    pub is_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recommendation {
    pub target: Target,
    /// Why, in one sentence the user can check.
    pub reason: String,
}

/// The bottle to propose for an installer. `page` is the bottle the user started from, if any;
/// `known` is the name of the program when the app has fixes for it; `new_name` is the name a
/// new bottle would get.
pub fn recommend(
    new_name: &str,
    known: Option<&str>,
    page: Option<&Path>,
    bottles: &[BottleOption],
) -> Recommendation {
    let new = |reason: String| Recommendation {
        target: Target::New(new_name.to_string()),
        reason,
    };
    let existing = |b: &BottleOption, reason: String| Recommendation {
        target: Target::Existing(b.path.clone()),
        reason,
    };
    if bottles.is_empty() {
        return new("Aún no tienes botellas propias: se crea una para este programa.".into());
    }
    if let Some(b) = page.and_then(|p| bottles.iter().find(|b| b.path == p)) {
        return existing(b, "Es la botella que tienes abierta.".into());
    }
    if let Some(program) = known {
        return match bottles.iter().find(|b| b.prepared) {
            Some(b) => existing(b, format!("Ya está preparada para {program}.")),
            None => new(format!(
                "{program} recibe ajustes propios: en su botella no afectan a tus otros programas."
            )),
        };
    }
    let b = bottles.iter().find(|b| b.is_default).unwrap_or(&bottles[0]);
    existing(b, "Es tu botella principal.".into())
}

// ------------------------------------------------------------------ the installation

/// Where an installation stands. Each stage is entered on an event the app observed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Wine itself is still being installed.
    WaitingWine,
    CreatingBottle,
    /// Writing the program's fixes into the bottle and starting the installer.
    Preparing,
    /// The installer's process is alive.
    InstallerOpen,
    /// Its first process ended and no new program has appeared, but the bottle is still busy:
    /// installers often hand over to a second process.
    Settling,
    Done(Outcome),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Programs that were not in the bottle before are there now (`InstallSession::found`).
    Installed,
    /// It ended and the bottle is idle, but nothing new appeared. `code` is how its first
    /// process ended, when known.
    NothingNew { code: Option<i32> },
    /// It never ran, or the bottle could not be made.
    Failed(String),
    /// The user ended it from the app.
    Stopped,
}

/// A program that appeared in the bottle while installing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// Its identity: the file of its shortcut.
    pub id: String,
    pub name: String,
}

/// One installation, from the file chosen to the programs found.
#[derive(Clone, Debug)]
pub struct InstallSession {
    pub id: u64,
    pub file: PathBuf,
    pub bottle: PathBuf,
    pub bottle_name: String,
    /// The bottle is made for this installation.
    pub new_bottle: bool,
    /// The known program whose fixes are written first, if any.
    pub known: Option<String>,
    /// What the worker reported about writing those fixes (meaningful only with `known`).
    pub fixes: Fixes,
    /// The id of the recipe those fixes belong to, when the app knows it. A report for another
    /// recipe (a worker of an installation that was dismissed, say) is not about this one.
    pub recipe_id: Option<String>,
    /// The file is an installer: new programs are looked for when it ends. When it is not, the
    /// file is opened and kept in the bottle's list, and nothing else is followed.
    pub installs: bool,
    /// Wine was still being installed when it started, so that wait is one of the steps.
    pub waited_wine: bool,
    /// The programs the bottle had when the installation started.
    pub before: BTreeSet<String>,
    /// Programs that appeared since.
    pub found: Vec<Found>,
    pub stage: Stage,
    /// The stage that was in progress when it failed.
    pub failed_at: Option<Stage>,
    /// How the installer's first process ended, once it has.
    pub exit: Option<Option<i32>>,
    /// When the bottle was last looked at while settling (the app's clock, in seconds), and how
    /// many looks that was: they set the pace of the next one (`look_wait`).
    pub looked_at: f64,
    pub looks: u32,
}

/// What the card says when the person gave up an installation that had not started yet.
/// The endings of the files the «Instalar» picker offers; the hint on the status line names the
/// same ones, so what the app says it takes is what it takes.
pub const INSTALLER_EXTENSIONS: [&str; 3] = ["exe", "msi", "bat"];

pub const CANCELLED: &str = "Cancelaste la instalación. Si el instalador llega a abrirse, ciérralo desde la lista de programas.";

/// What the card says when the bottle was reported as made but its folder is not a complete
/// Wine prefix (no `drive_c` or no `system.reg`): the installer is not started in it.
pub const BOTTLE_INCOMPLETE: &str = "La botella se creó, pero quedó incompleta, así que el instalador no se abrió. Bórrala y vuelve a intentarlo.";

/// What to tell the person when the program's fixes could not be written in full.
pub const FIXES_ADVICE: &str = "Los ajustes del programa no se escribieron por completo, así que puede fallar. Si pasa, mira el diagnóstico de la botella.";

/// How a step of the installation is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepMark {
    Done,
    Now,
    Later,
    Failed,
    /// The step ran but its result is not what was asked for, and the installation went on
    /// (the program's fixes could not be written in full).
    NotApplied,
}

/// Whether the program's fixes were written into the bottle, as the worker that wrote them
/// reported it. Only a report moves it off `Pending`: starting the installer proves nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fixes {
    /// No report yet.
    Pending,
    /// Every value that can be written was written.
    Applied,
    /// Some or all of them could not be written.
    NotApplied,
}

/// What an installation waiting for Wine does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WineWait {
    /// Keep waiting: Wine is still being found or installed.
    Keep,
    /// Wine is ready: go on.
    Ready,
    /// Wine will not come (it is missing or broken and nothing is installing it): say so.
    Fail,
}

/// What the app knows about Wine while an installation waits for it. They are separate facts,
/// not a state: Wine can be ready while a reinstall runs, and a Wine nobody looked at yet is
/// neither ready nor missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WineFacts {
    /// Wine is installed and usable.
    pub ready: bool,
    /// Something is installing it right now.
    pub installing: bool,
    /// The app already found out what Wine state there is.
    pub looked: bool,
}

/// `install_ok`: the last Wine install ended well, so its result is about to arrive as a
/// message.
pub fn wine_wait(wine: WineFacts, install_ok: bool) -> WineWait {
    if wine.ready {
        WineWait::Ready
    } else if wine.installing || !wine.looked || install_ok {
        WineWait::Keep
    } else {
        WineWait::Fail
    }
}

/// Whether confirming an installation also starts installing Wine: it is not usable (missing or
/// broken, but not "not looked yet") and nothing is installing it.
pub fn starts_wine_install(wine: WineFacts) -> bool {
    !wine.ready && !wine.installing && wine.looked
}

/// The count of consecutive looks at the bottle that found no Wine running in it.
pub fn idle_looks(before: u8, alive: bool, needed: u8) -> u8 {
    if alive {
        0
    } else {
        before.saturating_add(1).min(needed)
    }
}

/// How many looks at a settling bottle come one second apart before they slow down: a launcher
/// that stays open after its installer must not be asked about every second for good.
const FAST_LOOKS: u32 = 10;

/// Seconds until the next look at a settling bottle is due (zero or less when it is due now):
/// about one second after the last look at first (so the result of an installation shows in
/// about two seconds), three seconds after that. An idle window sleeps exactly that long.
/// Times are the app's clock in seconds.
pub fn look_wait(looks: u32, last: f64, now: f64) -> f64 {
    last + if looks < FAST_LOOKS { 1.0 } else { 3.0 } - now
}

/// Whether closing everything in the bottle also closes something that is not the installer
/// (`installer_label`): another program the app follows there, or, once the installer's first
/// process is gone (`running` is empty), whatever keeps the bottle in use. While the installer
/// itself runs, the app cannot tell a program it does not follow from the installer's own
/// helpers, so only the programs it follows count then.
pub fn closes_others(installer_label: &str, running: &[&str], bottle_in_use: bool) -> bool {
    running.iter().any(|l| *l != installer_label) || (running.is_empty() && bottle_in_use)
}

/// Where closing the installer stands: what was asked and what the bottle still does. The three
/// are independent facts the app reads from different places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Closing {
    /// The person asked to close the installer.
    pub asked: bool,
    /// A worker holds the bottle (closing it, say).
    pub bottle_busy: bool,
    /// A program the app follows still runs in the bottle.
    pub still_running: bool,
}

/// Closing the installer is over: nothing is closing the bottle and nothing of it runs.
pub fn closing_complete(closing: Closing) -> bool {
    closing.asked && !closing.bottle_busy && !closing.still_running
}

/// The buttons that fit where an installation stands. The five are independent facts, not
/// states that exclude one another: «Cerrar todo» and «¿Ya terminó?» show together while the
/// bottle settles, and the diagnosis and the search by hand can both be offered next to
/// «Listo». An enum would need a variant for every combination, so each button keeps its flag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CardActions {
    /// «Cerrar todo en esta botella»: the installer is open, or something it started still runs.
    pub close: bool,
    /// «¿Ya terminó el instalador?»: its first window closed and the bottle stays busy.
    pub ask_done: bool,
    /// Nothing new appeared: offer to look for the program by hand.
    pub find_by_hand: bool,
    /// It failed or ended with an error code: the diagnosis may say why.
    pub diagnose: bool,
    /// It is over (or has not started, still waiting for Wine): the card can go away.
    pub dismiss: bool,
}

/// The order of the stages, for telling how far an installation got.
const ORDER_WINE: u8 = 0;
const ORDER_BOTTLE: u8 = 1;
const ORDER_PREPARING: u8 = 2;
const ORDER_OPEN: u8 = 3;
const ORDER_SEARCH: u8 = 4;
const ORDER_DONE: u8 = 5;

fn stage_order(stage: &Stage) -> u8 {
    match stage {
        Stage::WaitingWine => ORDER_WINE,
        Stage::CreatingBottle => ORDER_BOTTLE,
        Stage::Preparing => ORDER_PREPARING,
        Stage::InstallerOpen => ORDER_OPEN,
        Stage::Settling => ORDER_SEARCH,
        Stage::Done(_) => ORDER_DONE,
    }
}

fn found_since(before: &BTreeSet<String>, now: &Programs) -> Vec<Found> {
    now.iter()
        .filter(|(id, _)| !before.contains(*id))
        .map(|(id, name)| Found {
            id: id.clone(),
            name: name.clone(),
        })
        .collect()
}

/// What a new installation is made from: the file, the bottle it goes to and what the app
/// knows when the person confirms. The three bools are independent facts of that moment.
pub struct InstallRequest {
    pub id: u64,
    pub file: PathBuf,
    pub bottle: PathBuf,
    pub bottle_name: String,
    /// The bottle does not exist yet and has to be made first.
    pub new_bottle: bool,
    /// The program the app has fixes for, when those fixes are written first.
    pub known: Option<String>,
    /// The file is an installer: look for the programs it adds afterwards.
    pub installs: bool,
    /// Wine is installed, so the installation need not wait for it.
    pub wine_ready: bool,
}

impl InstallSession {
    /// A new installation.
    pub fn begin(request: InstallRequest) -> Self {
        let InstallRequest {
            id,
            file,
            bottle,
            bottle_name,
            new_bottle,
            known,
            installs,
            wine_ready,
        } = request;
        let mut s = Self {
            id,
            file,
            bottle,
            bottle_name,
            new_bottle,
            known,
            fixes: Fixes::Pending,
            recipe_id: None,
            installs,
            waited_wine: !wine_ready,
            before: BTreeSet::new(),
            found: vec![],
            stage: Stage::WaitingWine,
            failed_at: None,
            exit: None,
            looked_at: 0.0,
            looks: 0,
        };
        if wine_ready {
            s.wine_ready();
        }
        s
    }

    /// The name the installer's process is shown by in the list of open programs.
    pub fn process_label(&self) -> String {
        format!("Instalador {}", self.file_name())
    }

    pub fn file_name(&self) -> String {
        self.file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    /// Still going: the same file must not be started again in this bottle.
    pub fn active(&self) -> bool {
        !matches!(self.stage, Stage::Done(_))
    }

    /// The bottle's state shows this installation.
    pub fn shows_on(&self, bottle: &Path) -> bool {
        self.active() && self.installs && self.bottle == bottle
    }

    /// The programs of the bottle are worth reading again now and then.
    pub fn watches_programs(&self) -> bool {
        self.installs && matches!(self.stage, Stage::InstallerOpen | Stage::Settling)
    }

    /// Wine is there: on to making the bottle, or to starting.
    pub fn wine_ready(&mut self) {
        if self.stage == Stage::WaitingWine {
            self.stage = if self.new_bottle {
                Stage::CreatingBottle
            } else {
                Stage::Preparing
            };
        }
    }

    /// The creation worker reported success. `complete`: the folder looks like a whole Wine
    /// prefix (see `prefix_complete`). Only then is the bottle a finished step and the
    /// installation goes on; a folder that is not complete stops it here.
    pub fn bottle_made(&mut self, complete: bool) {
        if self.stage != Stage::CreatingBottle {
            return;
        }
        if complete {
            self.stage = Stage::Preparing;
        } else {
            self.fail(BOTTLE_INCOMPLETE);
        }
    }

    /// The worker that writes the program's fixes before the installer starts reported whether
    /// it wrote them all. Anything outside the preparing stage, or for a program with no
    /// fixes, is not about this installation.
    pub fn recipe_outcome(&mut self, applied: bool) {
        if self.stage == Stage::Preparing && self.known.is_some() && self.fixes == Fixes::Pending {
            self.fixes = if applied {
                Fixes::Applied
            } else {
                Fixes::NotApplied
            };
        }
    }

    /// Like `recipe_outcome`, for a report that names the recipe it is about: a report for
    /// another recipe than this installation's is ignored.
    pub fn recipe_reported(&mut self, id: &str, applied: bool) {
        if self.recipe_id.as_deref().is_none_or(|own| own == id) {
            self.recipe_outcome(applied);
        }
    }

    /// The fixes as the card must show them: once the installation moved past writing them
    /// without a report, they are not claimed as written.
    fn fixes_shown(&self) -> Fixes {
        if self.fixes == Fixes::Pending && self.known.is_some() && self.reached() > ORDER_PREPARING
        {
            Fixes::NotApplied
        } else {
            self.fixes
        }
    }

    /// The programs the bottle holds right now, just before the installer starts.
    pub fn snapshot(&mut self, now: &Programs) {
        self.before = now.keys().cloned().collect();
    }

    /// The installer's process is alive.
    pub fn installer_started(&mut self) {
        if self.stage == Stage::Preparing {
            self.stage = Stage::InstallerOpen;
        }
    }

    /// The installer's first process ended (`code`: its exit code, None when a signal ended it).
    /// `alive`: the bottle still has Wine running. `now`: the bottle's programs at this moment.
    pub fn process_ended(&mut self, code: Option<i32>, alive: bool, now: &Programs) {
        if self.stage != Stage::InstallerOpen {
            return;
        }
        self.exit = Some(code);
        self.settle(alive, now);
    }

    /// A later look at the bottle, while waiting for it to become idle.
    pub fn bottle_checked(&mut self, alive: bool, now: &Programs) {
        if self.stage == Stage::Settling {
            self.settle(alive, now);
        }
    }

    /// Programs that appeared count at any moment, also while the installer is still open
    /// (some never close by themselves).
    pub fn programs_changed(&mut self, now: &Programs) {
        if matches!(self.stage, Stage::InstallerOpen | Stage::Settling) {
            self.found = found_since(&self.before, now);
        }
    }

    fn settle(&mut self, alive: bool, now: &Programs) {
        self.found = found_since(&self.before, now);
        self.stage = if !self.found.is_empty() {
            Stage::Done(Outcome::Installed)
        } else if alive {
            Stage::Settling
        } else {
            Stage::Done(Outcome::NothingNew {
                code: self.exit.flatten(),
            })
        };
    }

    /// The user says the installer is finished (for those that leave something running).
    pub fn finish_by_user(&mut self, now: &Programs) {
        if self.stage == Stage::Settling {
            self.settle(false, now);
        }
    }

    /// The installer was closed on the user's order and what it ran is gone.
    pub fn stopped_by_user(&mut self, now: &Programs) {
        // Only an installer that was opened can be closed.
        if matches!(self.stage, Stage::InstallerOpen | Stage::Settling) {
            self.found = found_since(&self.before, now);
            self.stage = Stage::Done(Outcome::Stopped);
        }
    }

    pub fn fail(&mut self, why: impl Into<String>) {
        if self.active() {
            self.failed_at = Some(self.stage.clone());
            self.stage = Stage::Done(Outcome::Failed(why.into()));
        }
    }

    /// The buttons to show now.
    pub fn card_actions(&self) -> CardActions {
        let nothing = matches!(
            self.stage,
            Stage::Done(Outcome::NothingNew { .. } | Outcome::Stopped)
        ) && self.found.is_empty();
        CardActions {
            close: matches!(self.stage, Stage::InstallerOpen | Stage::Settling),
            ask_done: self.stage == Stage::Settling,
            find_by_hand: nothing,
            diagnose: match &self.stage {
                Stage::Done(Outcome::Failed(_)) => true,
                Stage::Done(Outcome::NothingNew { code }) => code.is_some_and(|c| c != 0),
                _ => false,
            },
            // While it waits for Wine, or for the bottle and the fixes, no installer is open
            // yet; if a worker never answers, this is the only way out.
            dismiss: !self.active()
                || matches!(
                    self.stage,
                    Stage::WaitingWine | Stage::CreatingBottle | Stage::Preparing
                ),
        }
    }

    /// How far the installation got: the order of the step in progress, or of the one that
    /// failed. A finished installation is past every step.
    fn reached(&self) -> u8 {
        match &self.stage {
            Stage::Done(Outcome::Failed(_)) => self
                .failed_at
                .as_ref()
                .map(stage_order)
                .unwrap_or(ORDER_OPEN),
            other => stage_order(other),
        }
    }

    /// What the card's dismiss button says. Before the installer opens, the button cancels the
    /// installation (it never hides it: a hidden card would still open the installer), and it
    /// says so; once the installation is over it only closes the card.
    pub fn dismiss_label(&self) -> &'static str {
        match self.stage {
            Stage::WaitingWine | Stage::CreatingBottle | Stage::Preparing => "Cancelar",
            _ => "Listo",
        }
    }

    /// The steps to show, each with its mark. Only steps that apply are listed. A step is done
    /// only once its operation finished and, where the app can look, its result was seen: Wine
    /// found ready, the bottle's folder complete, the fixes reported as written, the installer's
    /// first process ended, and a look at the bottle after that.
    pub fn steps(&self) -> Vec<(StepMark, String)> {
        let failed = matches!(self.stage, Stage::Done(Outcome::Failed(_)));
        let at = self.reached();
        let fixes = self.fixes_shown();
        // A step at `order`: done before it, in progress or failed at it, to come after it.
        let by_order = |order: u8| {
            if order < at {
                StepMark::Done
            } else if order == at {
                if failed {
                    StepMark::Failed
                } else {
                    StepMark::Now
                }
            } else {
                StepMark::Later
            }
        };
        let mut steps = vec![];
        if self.waited_wine {
            steps.push((
                by_order(ORDER_WINE),
                "Esperar a que Wine termine de instalarse".into(),
            ));
        }
        if self.new_bottle {
            steps.push((
                by_order(ORDER_BOTTLE),
                format!("Crear la botella «{}»", self.bottle_name),
            ));
        }
        if let Some(name) = &self.known {
            let apply = format!("Aplicar los ajustes de {name}");
            steps.push(if at < ORDER_PREPARING {
                (StepMark::Later, apply)
            } else {
                match fixes {
                    Fixes::Applied => (StepMark::Done, apply),
                    Fixes::NotApplied => (
                        StepMark::NotApplied,
                        format!("Ajustes de {name}: no se aplicaron del todo"),
                    ),
                    // Being written; or the installation failed before the worker could have
                    // written anything, so the failure is not this step's.
                    Fixes::Pending if failed => (StepMark::Later, apply),
                    Fixes::Pending => (StepMark::Now, apply),
                }
            });
        }
        if self.installs {
            // While the fixes are being written the installer is not being opened yet.
            let opening = at == ORDER_PREPARING
                && (self.known.is_none() || fixes != Fixes::Pending || failed);
            let (mark, label) = if opening {
                if failed {
                    (StepMark::Failed, "Abrir el instalador")
                } else {
                    (StepMark::Now, "Abriendo el instalador…")
                }
            } else if at < ORDER_OPEN {
                (StepMark::Later, "Abrir el instalador")
            } else if at == ORDER_OPEN {
                if failed {
                    (StepMark::Failed, "Abrir el instalador")
                } else {
                    (
                        StepMark::Now,
                        "Instalador abierto: sigue los pasos en su ventana",
                    )
                }
            } else if self.stage == Stage::Done(Outcome::Stopped) {
                (StepMark::Done, "Instalador cerrado")
            } else {
                (StepMark::Done, "El instalador terminó")
            };
            steps.push((mark, label.to_string()));
            // The search is a finished step only when it found something. An ending with
            // nothing new, or one the person cut short, must not show a check for work that
            // gave nothing: the row stays pending and says so.
            let search = match &self.stage {
                Stage::Done(Outcome::NothingNew { .. }) => (
                    StepMark::Later,
                    "Buscar los programas nuevos: no apareció ninguno".to_string(),
                ),
                Stage::Done(Outcome::Stopped) if self.found.is_empty() => (
                    StepMark::Later,
                    "Buscar los programas nuevos: no llegó a hacerse".to_string(),
                ),
                _ => (by_order(ORDER_SEARCH), "Buscar los programas nuevos".into()),
            };
            steps.push(search);
        }
        steps
    }

    /// The card's title.
    pub fn title(&self) -> String {
        match &self.stage {
            Stage::Done(Outcome::Installed) => "Instalación terminada".into(),
            Stage::Done(Outcome::NothingNew { .. }) => "El instalador terminó".into(),
            Stage::Done(Outcome::Failed(_)) => "No se pudo instalar".into(),
            Stage::Done(Outcome::Stopped) => "Instalador cerrado".into(),
            _ if self.installs => format!("Instalando {}", self.file_name()),
            _ => format!("Abriendo {}", self.file_name()),
        }
    }

    /// One line saying what is happening or what happened, from what was observed.
    pub fn headline(&self) -> String {
        match &self.stage {
            Stage::WaitingWine => "Esperando a que Wine termine de instalarse…".into(),
            Stage::CreatingBottle => format!("Creando la botella «{}»…", self.bottle_name),
            Stage::Preparing if !self.installs => "Abriendo el programa…".into(),
            Stage::Preparing => match (&self.known, self.fixes) {
                (Some(name), Fixes::Pending) => format!("Aplicando los ajustes de {name}…"),
                _ => "Abriendo el instalador…".into(),
            },
            Stage::InstallerOpen => {
                "El instalador está abierto. Sigue sus pasos en su ventana.".into()
            }
            Stage::Settling => {
                "El instalador se cerró, pero la botella sigue ocupada: puede que continúe.".into()
            }
            Stage::Done(Outcome::Installed) => match self.found.as_slice() {
                [one] => format!("Se añadió «{}» a la lista de programas.", one.name),
                many => format!("Se añadieron {} programas a la lista de programas.", many.len()),
            },
            Stage::Done(Outcome::NothingNew { code }) => match code {
                Some(c) if *c != 0 => format!(
                    "El instalador terminó con el código {c} y no apareció ningún programa nuevo."
                ),
                _ => "El instalador terminó y no apareció ningún programa nuevo en la lista de programas de la botella.".into(),
            },
            Stage::Done(Outcome::Failed(why)) => why.clone(),
            Stage::Done(Outcome::Stopped) => {
                "Cerraste el instalador antes de que terminara.".into()
            }
        }
    }

    /// What to try when the end was not a clear success.
    pub fn advice(&self) -> Option<&'static str> {
        match &self.stage {
            Stage::Done(Outcome::NothingNew { code }) => Some(match code {
                Some(c) if *c != 0 => {
                    "Mira el diagnóstico: puede decir por qué se detuvo. También puedes probar otra versión del instalador."
                }
                _ => {
                    "Si el programa quedó instalado, búscalo en la botella con el botón de abajo. Si no, abre el instalador otra vez o prueba otra versión."
                }
            }),
            Stage::Done(Outcome::Stopped) if self.found.is_empty() => {
                Some("Cuando quieras terminar la instalación, abre el instalador otra vez.")
            }
            _ => None,
        }
    }

    /// The note about fixes that were not written, shown beside the advice (never instead of
    /// it: a failed installer may well be the result of the missing fixes). A failed
    /// installation says why on its own line.
    pub fn fixes_note(&self) -> Option<&'static str> {
        let after_start = matches!(
            self.stage,
            Stage::InstallerOpen
                | Stage::Settling
                | Stage::Done(Outcome::Installed | Outcome::NothingNew { .. } | Outcome::Stopped)
        );
        (after_start && self.fixes_shown() == Fixes::NotApplied).then_some(FIXES_ADVICE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal PE header: "MZ", the header offset at 0x3C, "PE\0\0" and the machine.
    fn exe(machine: u16) -> Vec<u8> {
        let mut b = vec![0u8; 0x100];
        b[0] = b'M';
        b[1] = b'Z';
        b[0x3C] = 0x80;
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        b[0x84..0x86].copy_from_slice(&machine.to_le_bytes());
        b
    }

    #[test]
    fn a_mac_program_is_told_to_open_without_the_app_by_its_present_name() {
        for name in ["Cosa.dmg", "Cosa.pkg", "Cosa.app"] {
            let why = classify_bytes(name, b"").warning().expect("unsupported");
            assert!(why.contains("ábrelo directamente desde Finder"), "{why}");
            assert!(why.contains("no necesita Altrune"), "{why}");
            assert!(!why.contains("Botella Libre"), "{why}");
        }
    }

    #[test]
    fn files_are_recognised_by_name_and_first_bytes() {
        let setup = classify_bytes("SteamSetup.exe", &exe(0x014C));
        assert_eq!(
            setup.kind,
            FileKind::Exe {
                installer_like: true
            }
        );
        assert_eq!(setup.machine, Some(Machine::X86));
        assert!(setup.installs() && setup.supported() && setup.warning().is_none());

        let program = classify_bytes("notepad++.exe", &exe(0x8664));
        assert_eq!(
            program.kind,
            FileKind::Exe {
                installer_like: false
            }
        );
        assert!(!program.installs());
        assert!(program.label().contains("64 bits"));

        // A web page saved as .exe, or a download cut short.
        let broken = classify_bytes("juego.exe", b"<!DOCTYPE html>");
        assert!(!broken.supported());
        assert!(broken.warning().is_some());

        let msi = classify_bytes(
            "x.MSI",
            &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 0],
        );
        assert_eq!(msi.kind, FileKind::Msi);
        assert!(msi.installs());
        assert!(!classify_bytes("x.msi", b"PK\x03\x04").supported());

        assert_eq!(
            classify_bytes("run.bat", b"@echo off").kind,
            FileKind::Script
        );
        assert_eq!(
            classify_bytes("Steam.lnk", b"L\0\0\0").kind,
            FileKind::Shortcut
        );
        for mac in [
            "App.dmg",
            "App.pkg",
            "datos.zip",
            "disco.iso",
            "foto.png",
            "sinextension",
        ] {
            assert!(!classify_bytes(mac, b"....").supported(), "{mac}");
        }
        // ARM programs are named, not refused: the user decides.
        let arm = classify_bytes("app.exe", &exe(0xAA64));
        assert!(arm.supported());
        assert!(arm.warning().is_some_and(|w| w.contains("ARM")));
        // A header cut short gives no machine and no panic.
        assert_eq!(classify_bytes("a.exe", b"MZ").machine, None);
    }

    fn bottle(name: &str, prepared: bool, is_default: bool) -> BottleOption {
        BottleOption {
            path: PathBuf::from(format!("/b/{name}")),
            name: name.into(),
            prepared,
            is_default,
        }
    }

    #[test]
    fn the_bottle_proposed_follows_what_is_known() {
        // No bottle yet: a new one.
        let r = recommend("Juego", None, None, &[]);
        assert_eq!(r.target, Target::New("Juego".into()));

        let bottles = [bottle("Oficina", false, true), bottle("Steam", true, false)];
        // Started from a bottle's page: that bottle.
        let r = recommend("X", Some("Steam"), Some(Path::new("/b/Oficina")), &bottles);
        assert_eq!(r.target, Target::Existing("/b/Oficina".into()));
        // A page that is no bottle of the list never becomes the target.
        let r = recommend("X", None, Some(Path::new("/b/Otra")), &bottles);
        assert_eq!(r.target, Target::Existing("/b/Oficina".into()));
        // A known program goes where its fixes already are…
        let r = recommend("Steam", Some("Steam"), None, &bottles);
        assert_eq!(r.target, Target::Existing("/b/Steam".into()));
        assert!(r.reason.contains("Steam"));
        // …or to a bottle of its own.
        let fresh = [bottle("Oficina", false, true)];
        let r = recommend("Rockstar Games Launcher", Some("Rockstar"), None, &fresh);
        assert_eq!(r.target, Target::New("Rockstar Games Launcher".into()));
        // Anything else: the main bottle.
        let r = recommend("Notepad", None, None, &bottles);
        assert_eq!(r.target, Target::Existing("/b/Oficina".into()));
    }

    fn session(new_bottle: bool, known: Option<&str>) -> InstallSession {
        let mut s = InstallSession::begin(InstallRequest {
            id: 1,
            file: PathBuf::from("/d/SteamSetup.exe"),
            bottle: PathBuf::from("/b/Steam"),
            bottle_name: "Steam".into(),
            new_bottle,
            known: known.map(String::from),
            installs: true,
            wine_ready: true,
        });
        s.snapshot(&set(&["Bloc de notas"]));
        s.stage = Stage::InstallerOpen;
        s
    }

    /// Programs as the bottle holds them: the identity and the name are the same here.
    fn set(names: &[&str]) -> Programs {
        names
            .iter()
            .map(|s| (s.to_string(), s.to_string()))
            .collect()
    }

    fn names(s: &InstallSession) -> Vec<&str> {
        s.found.iter().map(|f| f.name.as_str()).collect()
    }

    #[test]
    fn an_installation_ends_only_on_what_was_observed() {
        // Its process ended and a new program is there: installed.
        let mut s = session(false, None);
        s.process_ended(Some(0), true, &set(&["Bloc de notas", "Steam"]));
        assert_eq!(s.stage, Stage::Done(Outcome::Installed));
        assert_eq!(names(&s), ["Steam"]);
        assert!(s.headline().contains("«Steam»"));
        assert!(!s.active());

        // The first process ended, nothing new, the bottle still busy: not finished.
        let mut s = session(false, None);
        s.process_ended(Some(0), true, &set(&["Bloc de notas"]));
        assert_eq!(s.stage, Stage::Settling);
        assert!(s.active());
        // Later the programs appear.
        s.bottle_checked(true, &set(&["Bloc de notas", "Juego", "Launcher"]));
        assert_eq!(s.stage, Stage::Done(Outcome::Installed));
        assert!(s.headline().contains("2 programas"));

        // The bottle went idle with nothing new: said as it is, with the exit code.
        let mut s = session(false, None);
        s.process_ended(Some(3), true, &set(&["Bloc de notas"]));
        s.bottle_checked(false, &set(&["Bloc de notas"]));
        assert_eq!(s.stage, Stage::Done(Outcome::NothingNew { code: Some(3) }));
        assert!(s.headline().contains("código 3"));

        // A check while the installer is still open changes nothing.
        let mut s = session(false, None);
        s.bottle_checked(false, &set(&[]));
        assert_eq!(s.stage, Stage::InstallerOpen);
        // Programs that appear meanwhile are listed, but it is not over.
        s.programs_changed(&set(&["Bloc de notas", "Steam"]));
        assert_eq!(names(&s), ["Steam"]);
        assert_eq!(s.stage, Stage::InstallerOpen);

        // The user can end the wait; a finished session ignores later events.
        let mut s = session(false, None);
        s.process_ended(None, true, &set(&["Bloc de notas"]));
        s.finish_by_user(&set(&["Bloc de notas"]));
        assert_eq!(s.stage, Stage::Done(Outcome::NothingNew { code: None }));
        s.process_ended(Some(0), false, &set(&["Bloc de notas", "Otro"]));
        assert_eq!(s.stage, Stage::Done(Outcome::NothingNew { code: None }));
    }

    #[test]
    fn steps_show_how_far_it_got() {
        let mut s = session(true, Some("Steam"));
        s.stage = Stage::CreatingBottle;
        let marks: Vec<StepMark> = s.steps().into_iter().map(|(m, _)| m).collect();
        assert_eq!(
            marks,
            [
                StepMark::Now,
                StepMark::Later,
                StepMark::Later,
                StepMark::Later
            ]
        );
        s.stage = Stage::InstallerOpen;
        // The fixes were reported as written: only then are they a finished step.
        s.fixes = Fixes::Applied;
        let marks: Vec<StepMark> = s.steps().into_iter().map(|(m, _)| m).collect();
        assert_eq!(
            marks,
            [
                StepMark::Done,
                StepMark::Done,
                StepMark::Now,
                StepMark::Later
            ]
        );
        s.stage = Stage::Done(Outcome::Installed);
        assert!(s.steps().iter().all(|(m, _)| *m == StepMark::Done));

        // No bottle to make and no fixes: two steps. "Preparing" shows as the first of them.
        let mut s = session(false, None);
        assert_eq!(s.steps().len(), 2);
        s.stage = Stage::Preparing;
        let marks: Vec<StepMark> = s.steps().into_iter().map(|(m, _)| m).collect();
        assert_eq!(marks, [StepMark::Now, StepMark::Later]);

        // The bottle could not be made: that step failed, the rest never ran.
        let mut s = session(true, None);
        s.stage = Stage::CreatingBottle;
        s.fail("No se pudo crear la botella.");
        let marks: Vec<StepMark> = s.steps().into_iter().map(|(m, _)| m).collect();
        assert_eq!(marks, [StepMark::Failed, StepMark::Later, StepMark::Later]);
        assert_eq!(s.headline(), "No se pudo crear la botella.");
    }

    /// Every way of giving a file to the app asks the same question and gets the same answer.
    #[test]
    fn a_file_leads_to_one_of_three_places() {
        let installer = classify_bytes("SteamSetup.exe", &exe(0x8664));
        let program = classify_bytes("notepad++.exe", &exe(0x8664));
        let script = classify_bytes("run.bat", b"@echo off");
        let broken = classify_bytes("juego.exe", b"<!DOCTYPE html>");
        let mac = classify_bytes("App.dmg", b"....");
        let zip = classify_bytes("datos.zip", b"PK");

        assert_eq!(entry(&installer, false), Entry::Ask { installer: true });
        assert_eq!(entry(&program, false), Entry::Ask { installer: false });
        assert_eq!(entry(&script, false), Entry::Ask { installer: false });
        // A file that cannot be opened says why, and nothing starts.
        assert!(matches!(entry(&broken, false), Entry::Refused(w) if w.contains("descarga")));
        assert!(matches!(entry(&mac, false), Entry::Refused(w) if w.contains("Mac")));
        assert!(matches!(entry(&zip, false), Entry::Refused(w) if w.contains("comprimido")));
        // While an installation runs, nothing else starts, whatever the file is.
        for d in [&installer, &program, &broken] {
            assert_eq!(entry(d, true), Entry::Running);
        }
    }

    /// What Finder hands over: installers and unreadable files go through the window, other
    /// programs open at once.
    #[test]
    fn finder_files_go_through_the_window_only_when_they_need_it() {
        let msi_head = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        assert!(through_window(&classify_bytes("x.msi", &msi_head)));
        assert!(through_window(&classify_bytes("setup.exe", &exe(0x014C))));
        assert!(through_window(&classify_bytes(
            "Instalar juego.exe",
            &exe(0x014C)
        )));
        assert!(!through_window(&classify_bytes(
            "notepad++.exe",
            &exe(0x8664)
        )));
        assert!(!through_window(&classify_bytes("run.bat", b"@echo off")));
        // An .msi that is not one, or a page saved as .exe: the window tells why.
        assert!(through_window(&classify_bytes("x.msi", b"PK\x03\x04")));
        assert!(through_window(&classify_bytes("juego.exe", b"<html>")));
    }

    /// The events of an installation in the order they happen, from Wine missing to the end.
    #[test]
    fn an_installation_follows_the_events_in_order() {
        // Wine is still being installed and the bottle does not exist: each wait is a step.
        let mut s = InstallSession::begin(InstallRequest {
            id: 7,
            file: PathBuf::from("/d/SteamSetup.exe"),
            bottle: PathBuf::from("/b/Steam"),
            bottle_name: "Steam".into(),
            new_bottle: true,
            known: Some("Steam".into()),
            installs: true,
            wine_ready: false,
        });
        assert_eq!(s.stage, Stage::WaitingWine);
        assert!(s.headline().contains("Wine"));
        let marks = |s: &InstallSession| -> Vec<StepMark> {
            s.steps().into_iter().map(|(m, _)| m).collect()
        };
        assert_eq!(marks(&s)[0], StepMark::Now);
        assert_eq!(s.steps().len(), 5);

        // Events that do not belong to the stage change nothing.
        s.bottle_made(true);
        s.installer_started();
        s.process_ended(Some(0), false, &set(&["X"]));
        assert_eq!(s.stage, Stage::WaitingWine);
        assert_eq!(s.exit, None);

        s.wine_ready();
        assert_eq!(s.stage, Stage::CreatingBottle);
        assert_eq!(marks(&s)[..2], [StepMark::Done, StepMark::Now]);
        s.bottle_made(true);
        assert_eq!(s.stage, Stage::Preparing);
        s.snapshot(&set(&["Bloc de notas"]));
        s.recipe_outcome(true);
        s.installer_started();
        assert_eq!(s.stage, Stage::InstallerOpen);
        assert!(s.card_actions().close && !s.card_actions().ask_done);
        // The installer's first window closes while the bottle stays busy.
        s.process_ended(Some(0), true, &set(&["Bloc de notas"]));
        assert_eq!(s.stage, Stage::Settling);
        assert!(s.card_actions().ask_done && s.card_actions().close);
        // The bottle settles and the program is there.
        s.bottle_checked(false, &set(&["Bloc de notas", "Steam"]));
        assert_eq!(s.stage, Stage::Done(Outcome::Installed));
        assert_eq!(names(&s), ["Steam"]);
        assert_eq!(s.title(), "Instalación terminada");
        let a = s.card_actions();
        assert!(a.dismiss && !a.close && !a.find_by_hand && !a.diagnose);
        assert!(s.advice().is_none());
    }

    /// A bottle that cannot be made stops the installation at that step, with the reason.
    #[test]
    fn a_failure_stops_at_the_step_that_failed() {
        let mut s = InstallSession::begin(InstallRequest {
            id: 1,
            file: PathBuf::from("/d/x.msi"),
            bottle: PathBuf::from("/b/X"),
            bottle_name: "X".into(),
            new_bottle: true,
            known: None,
            installs: true,
            wine_ready: false,
        });
        s.wine_ready();
        s.fail("No se pudo crear la botella.");
        let marks: Vec<StepMark> = s.steps().into_iter().map(|(m, _)| m).collect();
        assert_eq!(
            marks,
            [
                StepMark::Done,
                StepMark::Failed,
                StepMark::Later,
                StepMark::Later
            ]
        );
        assert_eq!(s.title(), "No se pudo instalar");
        let a = s.card_actions();
        assert!(a.diagnose && a.dismiss && !a.find_by_hand);
        // Later events do not bring it back.
        s.bottle_made(true);
        s.installer_started();
        assert!(!s.active());

        // The installer never opened: the step of the installer failed, not the search.
        let mut s = session(false, None);
        s.stage = Stage::Preparing;
        s.fail("No se pudo abrir.");
        let marks: Vec<StepMark> = s.steps().into_iter().map(|(m, _)| m).collect();
        assert_eq!(marks, [StepMark::Failed, StepMark::Later]);
    }

    /// Installers that start the program at the end leave the bottle in use: a new program
    /// still means installed. With nothing new the person is asked, and answers.
    #[test]
    fn a_busy_bottle_is_asked_about_only_when_nothing_new_appeared() {
        // Steam: the program is in the list while the bottle still runs.
        let mut s = session(false, None);
        s.process_ended(Some(0), true, &set(&["Bloc de notas", "Steam"]));
        assert_eq!(s.stage, Stage::Done(Outcome::Installed));

        // Nothing new and the bottle stays in use: the question, and no guess.
        let mut s = session(false, None);
        s.process_ended(Some(0), true, &set(&["Bloc de notas"]));
        s.bottle_checked(true, &set(&["Bloc de notas"]));
        s.bottle_checked(true, &set(&["Bloc de notas"]));
        assert_eq!(s.stage, Stage::Settling);
        assert!(s.card_actions().ask_done);
        // «Sí, ya terminó»: said as it is, with what to try.
        s.finish_by_user(&set(&["Bloc de notas"]));
        assert_eq!(s.stage, Stage::Done(Outcome::NothingNew { code: Some(0) }));
        let a = s.card_actions();
        assert!(a.find_by_hand && a.dismiss && !a.diagnose);
        assert!(s.advice().is_some_and(|t| t.contains("búscalo")));
        assert_eq!(s.title(), "El instalador terminó");

        // With an error code the diagnosis is offered.
        let mut s = session(false, None);
        s.process_ended(Some(1), false, &set(&["Bloc de notas"]));
        assert_eq!(s.stage, Stage::Done(Outcome::NothingNew { code: Some(1) }));
        assert!(s.card_actions().diagnose && s.card_actions().find_by_hand);
        assert!(s.advice().is_some_and(|t| t.contains("diagnóstico")));
    }

    /// Closing the installer on purpose is its own ending; what it left is still listed.
    #[test]
    fn closing_the_installer_is_not_a_failure() {
        let mut s = session(false, None);
        s.stopped_by_user(&set(&["Bloc de notas"]));
        assert_eq!(s.stage, Stage::Done(Outcome::Stopped));
        assert!(s.card_actions().find_by_hand && !s.card_actions().diagnose);
        assert!(s.headline().contains("Cerraste"));
        assert!(s.advice().is_some());
        // A program that appeared before it was closed is kept and offered.
        let mut s = session(false, None);
        s.stopped_by_user(&set(&["Bloc de notas", "Juego"]));
        assert_eq!(names(&s), ["Juego"]);
        assert!(!s.card_actions().find_by_hand);
        assert!(s.advice().is_none());
        // A finished session is not changed by it.
        let mut s = session(false, None);
        s.process_ended(Some(0), false, &set(&["Bloc de notas"]));
        s.stopped_by_user(&set(&["Bloc de notas", "Otro"]));
        assert_eq!(s.stage, Stage::Done(Outcome::NothingNew { code: Some(0) }));
    }

    /// A file that is no installer is opened and kept: no search for programs, no installer step.
    #[test]
    fn a_plain_program_has_no_installer_steps() {
        let s = InstallSession::begin(InstallRequest {
            id: 2,
            file: PathBuf::from("/d/notepad.exe"),
            bottle: PathBuf::from("/b/Notepad"),
            bottle_name: "Notepad".into(),
            new_bottle: true,
            known: None,
            installs: false,
            wine_ready: true,
        });
        assert_eq!(s.stage, Stage::CreatingBottle);
        assert_eq!(s.steps().len(), 1);
        assert!(!s.watches_programs());
        assert_eq!(s.title(), "Abriendo notepad.exe");
        let mut s = s;
        s.bottle_made(true);
        assert_eq!(s.headline(), "Abriendo el programa…");
        // Only an installer is shown as installing in the bottle's state.
        assert!(!s.shows_on(Path::new("/b/Notepad")));
        assert!(session(false, None).shows_on(Path::new("/b/Steam")));
        assert!(!session(false, None).shows_on(Path::new("/b/Otra")));
    }

    /// The programs are looked at only while something may still change them.
    #[test]
    fn programs_are_watched_while_the_installer_runs_or_settles() {
        let mut s = session(false, None);
        assert!(s.watches_programs());
        s.process_ended(Some(0), true, &set(&["Bloc de notas"]));
        assert!(s.watches_programs());
        s.finish_by_user(&set(&["Bloc de notas"]));
        assert!(!s.watches_programs());
    }

    /// Waiting for Wine ends in every state: ready goes on, a Wine that is not coming fails, and
    /// only a Wine still being found or installed (or just installed) keeps waiting.
    #[test]
    fn waiting_for_wine_always_ends() {
        use WineWait::*;
        assert_eq!(
            wine_wait(
                WineFacts {
                    ready: true,
                    installing: false,
                    looked: true
                },
                false
            ),
            Ready
        );
        // Not looked yet, or an install running: keep waiting.
        assert_eq!(
            wine_wait(
                WineFacts {
                    ready: false,
                    installing: false,
                    looked: false
                },
                false
            ),
            Keep
        );
        assert_eq!(
            wine_wait(
                WineFacts {
                    ready: false,
                    installing: true,
                    looked: true
                },
                false
            ),
            Keep
        );
        // The install ended well: its result is about to arrive.
        assert_eq!(
            wine_wait(
                WineFacts {
                    ready: false,
                    installing: false,
                    looked: true
                },
                true
            ),
            Keep
        );
        // Missing or broken, nothing installing it: it will not come.
        assert_eq!(
            wine_wait(
                WineFacts {
                    ready: false,
                    installing: false,
                    looked: true
                },
                false
            ),
            Fail
        );
        // Both a missing and a broken Wine start an install; an unknown one does not.
        assert!(starts_wine_install(WineFacts {
            ready: false,
            installing: false,
            looked: true
        }));
        assert!(!starts_wine_install(WineFacts {
            ready: false,
            installing: false,
            looked: false
        }));
        assert!(!starts_wine_install(WineFacts {
            ready: false,
            installing: true,
            looked: true
        }));
        assert!(!starts_wine_install(WineFacts {
            ready: true,
            installing: false,
            looked: true
        }));
    }

    /// A session that waits for Wine can be given up, because nothing has started yet.
    #[test]
    fn waiting_for_wine_can_be_given_up() {
        let s = InstallSession::begin(InstallRequest {
            id: 1,
            file: PathBuf::from("/d/setup.exe"),
            bottle: PathBuf::from("/b/X"),
            bottle_name: "X".into(),
            new_bottle: true,
            known: None,
            installs: true,
            wine_ready: false,
        });
        assert_eq!(s.stage, Stage::WaitingWine);
        assert!(s.active() && s.card_actions().dismiss && !s.card_actions().close);
        let mut s = s;
        s.wine_ready();
        // Making the bottle and preparing can be given up too, in case a worker never answers.
        assert_eq!(s.stage, Stage::CreatingBottle);
        assert!(s.card_actions().dismiss && !s.card_actions().close);
        s.bottle_made(true);
        assert!(s.card_actions().dismiss && !s.card_actions().close);
        s.installer_started();
        assert!(!s.card_actions().dismiss && s.card_actions().close);
    }

    /// Giving up while it prepares ends the installation, so another one may start.
    #[test]
    fn giving_up_while_preparing_frees_the_next_installation() {
        let mut s = session(true, None);
        s.stage = Stage::Preparing;
        assert!(s.card_actions().dismiss);
        s.fail(CANCELLED);
        assert!(!s.active());
        assert!(matches!(s.stage, Stage::Done(Outcome::Failed(_))));
        assert!(s.card_actions().dismiss);
    }

    /// The idle count restarts whenever Wine runs, and stops at the number needed.
    #[test]
    fn idle_looks_count_in_a_row() {
        assert_eq!(idle_looks(0, false, 2), 1);
        assert_eq!(idle_looks(1, false, 2), 2);
        assert_eq!(idle_looks(2, false, 2), 2);
        assert_eq!(idle_looks(2, true, 2), 0);
    }

    /// Closing the installer is complete only when it was asked and nothing of the bottle is left.
    #[test]
    fn closing_is_complete_when_nothing_is_left() {
        assert!(closing_complete(Closing {
            asked: true,
            bottle_busy: false,
            still_running: false
        }));
        assert!(!closing_complete(Closing {
            asked: false,
            bottle_busy: false,
            still_running: false
        }));
        assert!(!closing_complete(Closing {
            asked: true,
            bottle_busy: true,
            still_running: false
        }));
        assert!(!closing_complete(Closing {
            asked: true,
            bottle_busy: false,
            still_running: true
        }));
    }

    // ---------------------------------------------------------------- the marks

    fn marks(s: &InstallSession) -> Vec<StepMark> {
        s.steps().into_iter().map(|(m, _)| m).collect()
    }

    fn labels(s: &InstallSession) -> Vec<String> {
        s.steps().into_iter().map(|(_, l)| l).collect()
    }

    /// A session that goes through every step: Wine to wait for, a bottle to make, fixes to
    /// write, an installer to follow. Built only through its events.
    fn full(wine_ready: bool) -> InstallSession {
        InstallSession::begin(InstallRequest {
            id: 9,
            file: PathBuf::from("/d/SteamSetup.exe"),
            bottle: PathBuf::from("/b/Steam"),
            bottle_name: "Steam".into(),
            new_bottle: true,
            known: Some("Steam".into()),
            installs: true,
            wine_ready,
        })
    }

    use StepMark::{Done as D, Failed as F, Later as L, NotApplied as N, Now};

    /// Wine's step is done when Wine was found ready, and not before.
    #[test]
    fn waiting_for_wine_is_done_only_once_wine_is_ready() {
        let mut s = full(false);
        assert_eq!(marks(&s), [Now, L, L, L, L]);
        // Nothing but Wine being ready moves it.
        s.bottle_made(true);
        s.recipe_outcome(true);
        s.installer_started();
        assert_eq!(marks(&s), [Now, L, L, L, L]);
        s.wine_ready();
        assert_eq!(marks(&s), [D, Now, L, L, L]);
        // Wine that was ready from the start has no step at all.
        assert_eq!(marks(&full(true)), [Now, L, L, L]);
    }

    /// The bottle's step is done only when the worker said it worked and its folder is whole.
    #[test]
    fn the_bottle_is_a_finished_step_only_when_its_folder_is_complete() {
        let mut s = full(true);
        assert_eq!(s.stage, Stage::CreatingBottle);
        assert_eq!(marks(&s), [Now, L, L, L]);
        s.bottle_made(true);
        assert_eq!(s.stage, Stage::Preparing);
        assert_eq!(marks(&s)[0], D);

        // The worker reported success but there is no whole prefix: the step failed, the rest
        // never ran, and the card says why.
        let mut s = full(true);
        s.bottle_made(false);
        assert!(!s.active());
        assert_eq!(s.failed_at, Some(Stage::CreatingBottle));
        assert_eq!(marks(&s), [F, L, L, L]);
        assert_eq!(s.headline(), BOTTLE_INCOMPLETE);
        assert_eq!(s.title(), "No se pudo instalar");
        assert!(s.card_actions().diagnose && s.card_actions().dismiss);
        // The installer is never started after that.
        s.installer_started();
        assert_eq!(
            s.stage,
            Stage::Done(Outcome::Failed(BOTTLE_INCOMPLETE.into()))
        );

        // The answer of a bottle that is not being made changes nothing.
        let mut s = full(false);
        s.bottle_made(false);
        assert_eq!(s.stage, Stage::WaitingWine);
    }

    /// The fixes are a finished step only when the worker said every value was written.
    #[test]
    fn the_fixes_are_done_only_when_the_worker_says_so() {
        let mut s = full(true);
        s.bottle_made(true);
        // Being written: the fixes are in progress and the installer is not being opened yet.
        assert_eq!(s.stage, Stage::Preparing);
        assert_eq!(marks(&s), [D, Now, L, L]);
        assert_eq!(s.headline(), "Aplicando los ajustes de Steam…");
        assert!(labels(&s)[2].contains("Abrir el instalador"));

        // Reported as written: they are done and the installer is being opened.
        s.recipe_outcome(true);
        assert_eq!(marks(&s), [D, D, Now, L]);
        assert_eq!(s.headline(), "Abriendo el instalador…");
        assert_eq!(labels(&s)[2], "Abriendo el instalador…");
        s.installer_started();
        assert_eq!(marks(&s), [D, D, Now, L]);
        assert_eq!(
            labels(&s)[2],
            "Instalador abierto: sigue los pasos en su ventana"
        );
        assert!(s.advice().is_none());
    }

    /// The fixes that could not be written are not claimed: the step says so and the
    /// installation goes on.
    #[test]
    fn fixes_that_were_not_written_are_not_shown_as_done() {
        let mut s = full(true);
        s.bottle_made(true);
        s.recipe_outcome(false);
        assert_eq!(marks(&s), [D, N, Now, L]);
        assert!(labels(&s)[1].contains("no se aplicaron del todo"));
        assert!(labels(&s)[1].contains("Steam"));
        // The installer still opens, and the note stays to the end.
        s.installer_started();
        assert_eq!(marks(&s), [D, N, Now, L]);
        assert_eq!(s.fixes_note(), Some(FIXES_ADVICE));
        s.process_ended(Some(0), false, &set(&["Steam"]));
        assert_eq!(s.stage, Stage::Done(Outcome::Installed));
        assert_eq!(marks(&s), [D, N, D, D]);
        assert_eq!(s.fixes_note(), Some(FIXES_ADVICE));
        // A later report does not change what was said.
        s.recipe_outcome(true);
        assert_eq!(s.fixes, Fixes::NotApplied);
    }

    /// A report that never came is not taken for success: the installer being open proves
    /// nothing about the fixes.
    #[test]
    fn an_installer_that_opens_does_not_make_the_fixes_done() {
        let mut s = full(true);
        s.bottle_made(true);
        s.installer_started();
        assert_eq!(s.stage, Stage::InstallerOpen);
        assert_eq!(s.fixes, Fixes::Pending);
        assert_eq!(marks(&s), [D, N, Now, L]);
        assert_eq!(s.fixes_note(), Some(FIXES_ADVICE));
    }

    /// A report counts only for this installation's fixes, while they are being written.
    #[test]
    fn a_report_outside_its_moment_is_ignored() {
        // Before the preparing stage.
        let mut s = full(false);
        s.recipe_outcome(true);
        assert_eq!(s.fixes, Fixes::Pending);
        // A program with no fixes has none to report.
        let mut s = session(false, None);
        s.stage = Stage::Preparing;
        s.recipe_outcome(false);
        assert_eq!(s.fixes, Fixes::Pending);
        assert_eq!(marks(&s), [Now, L]);
        // Once the installer is open.
        let mut s = full(true);
        s.bottle_made(true);
        s.installer_started();
        s.recipe_outcome(true);
        assert_eq!(s.fixes, Fixes::Pending);
    }

    /// With no fixes to write, the installer is «being opened» while preparing, and «open» only
    /// once its process started.
    #[test]
    fn the_installer_is_open_only_after_it_started() {
        let mut s = session(false, None);
        s.stage = Stage::Preparing;
        assert_eq!(marks(&s), [Now, L]);
        assert_eq!(labels(&s)[0], "Abriendo el instalador…");
        assert_eq!(s.headline(), "Abriendo el instalador…");
        assert!(labels(&s)
            .iter()
            .all(|l| !l.starts_with("Instalador abierto")));

        s.installer_started();
        assert_eq!(s.stage, Stage::InstallerOpen);
        assert_eq!(marks(&s), [Now, L]);
        assert_eq!(
            labels(&s)[0],
            "Instalador abierto: sigue los pasos en su ventana"
        );

        // It is done once its first process ended, and the search is then in progress.
        s.process_ended(Some(0), true, &set(&["Bloc de notas"]));
        assert_eq!(s.stage, Stage::Settling);
        assert_eq!(marks(&s), [D, Now]);
        assert_eq!(labels(&s)[1], "Buscar los programas nuevos");
        // And done when a look at the bottle after that has finished.
        s.bottle_checked(false, &set(&["Bloc de notas", "Steam"]));
        assert_eq!(marks(&s), [D, D]);
    }

    /// An installer that ended with nothing new, or that the person closed, did not give the
    /// search a result: the search row is no finished step, and no row claims more than what
    /// happened.
    #[test]
    fn an_ending_without_a_result_shows_no_check_for_the_search() {
        let mut s = session(false, None);
        s.process_ended(Some(2), false, &set(&["Bloc de notas"]));
        assert_eq!(marks(&s), [D, L]);
        assert_eq!(labels(&s)[0], "El instalador terminó");
        assert!(labels(&s)[1].contains("no apareció"));
        // The person closed it: it did not «terminar», and nothing was searched.
        let mut s = session(false, None);
        s.stopped_by_user(&set(&["Bloc de notas"]));
        assert_eq!(marks(&s), [D, L]);
        assert_eq!(labels(&s)[0], "Instalador cerrado");
        assert!(!labels(&s).iter().any(|l| l.contains("terminó")));
        // Closed after a program appeared: the search found something, so it is done.
        let mut s = session(false, None);
        s.stopped_by_user(&set(&["Bloc de notas", "Programa"]));
        assert_eq!(marks(&s), [D, D]);
        assert_eq!(labels(&s)[1], "Buscar los programas nuevos");
        // An ending that found programs is all done.
        let mut s = session(false, None);
        s.process_ended(Some(0), false, &set(&["Bloc de notas", "Programa"]));
        assert_eq!(marks(&s), [D, D]);
        // Closing an installer that was never opened is not a thing: nothing changes.
        let mut s = session(false, None);
        s.stage = Stage::Preparing;
        s.stopped_by_user(&set(&["Bloc de notas"]));
        assert_eq!(s.stage, Stage::Preparing);
    }

    /// Closing everything in a bottle warns only when more than the installer runs there, and a
    /// settling bottle is looked at about once a second, then more slowly.
    #[test]
    fn the_card_warns_about_other_programs_and_looks_at_a_settling_bottle_often() {
        let own = "Instalador SteamSetup.exe";
        assert!(!closes_others(own, &[own], true));
        assert!(!closes_others(own, &[], false));
        assert!(closes_others(own, &[own, "Steam"], true));
        assert!(closes_others(own, &["Steam"], true));
        // Something runs that the app does not follow.
        assert!(closes_others(own, &[], true));

        assert!((look_wait(0, 10.0, 10.25) - 0.75).abs() < 1e-9);
        assert!(look_wait(FAST_LOOKS, 10.0, 10.0) > 2.9);
        assert!(look_wait(0, 10.0, 10.5) > 0.0);
        assert!(look_wait(0, 10.0, 11.0) <= 0.0);
        assert!(look_wait(FAST_LOOKS - 1, 10.0, 11.0) <= 0.0);
        // After a while the looks slow down.
        assert!(look_wait(FAST_LOOKS, 10.0, 12.0) > 0.0);
        assert!(look_wait(FAST_LOOKS, 10.0, 13.0) <= 0.0);
        // The installer's own process is named the same way everywhere.
        assert_eq!(session(false, None).process_label(), own);
    }

    /// The dismiss button promises only what it does: it cancels while Wine is awaited and while
    /// the bottle or the fixes are made, and closes a finished card.
    #[test]
    fn the_dismiss_button_says_what_it_does() {
        let mut s = full(false);
        assert_eq!(s.dismiss_label(), "Cancelar");
        s.wine_ready();
        assert_eq!(s.stage, Stage::CreatingBottle);
        assert_eq!(s.dismiss_label(), "Cancelar");
        s.bottle_made(true);
        assert_eq!(s.stage, Stage::Preparing);
        assert_eq!(s.dismiss_label(), "Cancelar");
        s.fail("x");
        assert_eq!(s.dismiss_label(), "Listo");
    }

    /// A failure shows at the step that was in progress, whatever that step was; what came
    /// before shows as done only if it was, and what comes after never ran.
    #[test]
    fn a_failure_shows_at_the_step_that_failed_in_every_stage() {
        // Waiting for Wine.
        let mut s = full(false);
        s.fail("Wine no está instalado.");
        assert_eq!(marks(&s), [F, L, L, L, L]);
        // Making the bottle.
        let mut s = full(false);
        s.wine_ready();
        s.fail("x");
        assert_eq!(marks(&s), [D, F, L, L, L]);
        // Before the worker wrote anything (no report yet): the fixes are not shown as failed,
        // the start of the installer is.
        let mut s = full(true);
        s.bottle_made(true);
        s.fail("La botella ya no está.");
        assert_eq!(marks(&s), [D, L, F, L]);
        assert_eq!(labels(&s)[1], "Aplicar los ajustes de Steam");
        assert_eq!(labels(&s)[2], "Abrir el instalador");
        // The installer could not start after the fixes were written: that step failed.
        let mut s = full(true);
        s.bottle_made(true);
        s.recipe_outcome(true);
        s.fail("No se pudo abrir el instalador.");
        assert_eq!(marks(&s), [D, D, F, L]);
        assert_eq!(labels(&s)[2], "Abrir el instalador");
        assert_eq!(s.headline(), "No se pudo abrir el instalador.");
        // …or after they were only partly written: the note stays and the installer failed.
        let mut s = full(true);
        s.bottle_made(true);
        s.recipe_outcome(false);
        s.fail("No se pudo abrir el instalador.");
        assert_eq!(marks(&s), [D, N, F, L]);
        // The note does not say the installer opens: it may not have.
        assert!(!labels(&s)[1].contains("se abre"));
        assert_eq!(s.fixes_note(), None);
        // The installer open when the bottle disappeared.
        let mut s = full(true);
        s.bottle_made(true);
        s.recipe_outcome(true);
        s.installer_started();
        s.fail("La botella ya no está.");
        assert_eq!(marks(&s), [D, D, F, L]);
        // The installer ended and the bottle disappeared while looking at it.
        let mut s = full(true);
        s.bottle_made(true);
        s.recipe_outcome(true);
        s.snapshot(&set(&["Bloc de notas"]));
        s.installer_started();
        s.process_ended(Some(0), true, &set(&["Bloc de notas"]));
        s.fail("La botella ya no está.");
        assert_eq!(marks(&s), [D, D, D, F]);
        // Every failure ends the installation, so another may start, and none says «Done».
        assert!(!s.active() && s.card_actions().dismiss);
    }

    /// Where an installation stands decides what its title, line and buttons say.
    #[test]
    fn the_title_the_line_and_the_buttons_follow_the_stage() {
        let mut s = full(false);
        let at = |s: &InstallSession| (s.title(), s.headline(), s.card_actions());
        let (t, h, a) = at(&s);
        assert_eq!(t, "Instalando SteamSetup.exe");
        assert!(h.contains("Esperando") && a.dismiss && !a.close);
        s.wine_ready();
        let (_, h, a) = at(&s);
        assert!(h.contains("Creando la botella") && a.dismiss && !a.close);
        s.bottle_made(true);
        let (_, h, a) = at(&s);
        assert!(h.starts_with("Aplicando los ajustes") && a.dismiss && !a.close);
        s.recipe_outcome(true);
        s.snapshot(&set(&["Bloc de notas"]));
        let (_, h, a) = at(&s);
        assert_eq!(h, "Abriendo el instalador…");
        assert!(a.dismiss && !a.close && !a.ask_done);
        s.installer_started();
        let (t, h, a) = at(&s);
        assert_eq!(t, "Instalando SteamSetup.exe");
        assert!(h.contains("está abierto") && a.close && !a.dismiss);
        s.process_ended(Some(0), true, &set(&["Bloc de notas"]));
        let (_, h, a) = at(&s);
        assert!(h.contains("sigue ocupada") && a.close && a.ask_done && !a.dismiss);
        s.bottle_checked(false, &set(&["Bloc de notas", "Steam"]));
        let (t, _, a) = at(&s);
        assert_eq!(t, "Instalación terminada");
        assert!(a.dismiss && !a.close && !a.ask_done && !a.diagnose);
    }

    /// A program that is only opened has no installer or search step to mark.
    #[test]
    fn a_plain_program_marks_only_the_steps_it_has() {
        let mut s = InstallSession::begin(InstallRequest {
            id: 3,
            file: PathBuf::from("/d/notepad.exe"),
            bottle: PathBuf::from("/b/Notepad"),
            bottle_name: "Notepad".into(),
            new_bottle: true,
            known: None,
            installs: false,
            wine_ready: false,
        });
        assert_eq!(marks(&s), [Now, L]);
        s.wine_ready();
        s.bottle_made(true);
        assert_eq!(marks(&s), [D, D]);
    }

    /// Missing fixes are told next to the advice for a bad ending, not hidden by it.
    #[test]
    fn the_note_about_missing_fixes_comes_with_every_ending_after_the_start() {
        let ended_empty = || {
            let mut s = full(true);
            s.bottle_made(true);
            s.recipe_outcome(false);
            s.snapshot(&set(&[]));
            s.installer_started();
            s
        };
        // The installer ended with a code and nothing new: the advice and the note both show.
        let mut s = ended_empty();
        s.process_ended(Some(1), false, &set(&[]));
        assert!(matches!(s.stage, Stage::Done(Outcome::NothingNew { .. })));
        assert!(s.advice().is_some());
        assert_eq!(s.fixes_note(), Some(FIXES_ADVICE));
        // The person closed it.
        let mut s = ended_empty();
        s.stopped_by_user(&set(&[]));
        assert!(s.advice().is_some());
        assert_eq!(s.fixes_note(), Some(FIXES_ADVICE));
        // Fixes that were written leave no note, and neither does a failed start.
        let mut s = full(true);
        s.bottle_made(true);
        s.recipe_outcome(true);
        s.installer_started();
        assert_eq!(s.fixes_note(), None);
        let mut s = full(true);
        s.bottle_made(true);
        s.recipe_outcome(false);
        s.fail("No se pudo abrir el instalador.");
        assert_eq!(s.fixes_note(), None);
        // The row's text is the same before and after the installer ended.
        let s = ended_empty();
        assert_eq!(labels(&s)[1], "Ajustes de Steam: no se aplicaron del todo");
    }

    /// A report names the recipe it is about; another recipe's report (from a worker of an
    /// installation that was dismissed, say) is not this installation's.
    #[test]
    fn a_report_for_another_recipe_is_ignored() {
        let mut s = full(true);
        s.recipe_id = Some("steam".into());
        s.bottle_made(true);
        s.recipe_reported("rockstar", true);
        assert_eq!(s.fixes, Fixes::Pending);
        s.recipe_reported("rockstar", false);
        assert_eq!(s.fixes, Fixes::Pending);
        s.recipe_reported("steam", false);
        assert_eq!(s.fixes, Fixes::NotApplied);
        // Its own id, applied.
        let mut s = full(true);
        s.recipe_id = Some("steam".into());
        s.bottle_made(true);
        s.recipe_reported("steam", true);
        assert_eq!(s.fixes, Fixes::Applied);
    }
}
