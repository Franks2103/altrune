//! The state of the window: `App` and the small structs it holds, and how it is built.

use super::{BottleTab, InstallDraft, Modal, Msg, Notice, NoticeKind, OnePass, Screen};
use crate::backend::{scan_bottles, Bottle, Config, ConfigLoad, Log};
use crate::measure::perf;
use crate::ui;
use crate::wine_setup;
use crate::{diagnostics, domain};
use eframe::egui;
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// How the window starts, from the command line (`--tema claro|oscuro`, `--vista …`): used to
/// check every screen in both appearances with `--screenshot`.
#[derive(Default)]
pub(crate) struct StartOptions {
    pub(crate) theme: Option<egui::Theme>,
    pub(crate) view: Option<String>,
    /// `--reposo <seconds>`: leave the window alone that long, report what it cost and close.
    pub(crate) idle: Option<f64>,
}

pub(crate) struct Running {
    pub(crate) bottle: PathBuf,
    pub(crate) label: String,
    /// The file of the program of the list this started, for the double-start guard.
    pub(crate) program: Option<PathBuf>,
    pub(crate) child: Child,
    pub(crate) rescan_after: bool,
    pub(crate) started: std::time::Instant,
    /// The person asked to close the bottle: its first process ending is no failure.
    pub(crate) closing: bool,
    /// The bottle was already in use when this started (another program, one of an earlier
    /// session). Its being in use later then says nothing about a handover.
    pub(crate) busy_before: bool,
}

/// A program started by a worker, ready to be followed by the window.
pub(crate) struct Opened {
    pub(crate) bottle: PathBuf,
    pub(crate) label: String,
    pub(crate) program: Option<PathBuf>,
    pub(crate) child: Child,
    pub(crate) rescan_after: bool,
    /// The Wine it was started with.
    pub(crate) wine: PathBuf,
}

/// A program whose first process ended while its bottle stays in use: a launcher that handed
/// over to the real program. It keeps showing as open until the bottle stops.
pub(crate) struct HandedOver {
    pub(crate) bottle: PathBuf,
    pub(crate) label: String,
    pub(crate) program: Option<PathBuf>,
    pub(crate) since: std::time::Instant,
}

/// What a program's first process ending means once its bottle was looked at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExitVerdict {
    /// It ran and ended; nothing to say.
    Ended,
    /// The bottle stays in use: a launcher handed over to the real program.
    HandedOver,
    /// It stopped within seconds with an error and nothing else explains it.
    Failed,
}

/// How a program's first process ended, once a worker looked at its bottle.
pub(crate) struct Exit {
    pub(crate) bottle: PathBuf,
    pub(crate) label: String,
    pub(crate) program: Option<PathBuf>,
    pub(crate) verdict: ExitVerdict,
}

/// What the name dialogs found out about the text typed so far. Finding it reads the bottle
/// folders, so it is redone only when the text changes, not on every frame.
#[derive(Clone, Default)]
pub(crate) struct NameCheck {
    /// The trimmed text this answer belongs to.
    pub(crate) input: String,
    /// The text cleaned of signs a folder name cannot have.
    pub(crate) clean: String,
    /// The first free name for the cleaned text (empty when there is none to show).
    pub(crate) unique: String,
    /// The text is already a good, free name for a new bottle.
    pub(crate) valid: bool,
}

pub(crate) struct App {
    pub(crate) cfg: Config,
    pub(crate) bottles: Vec<Bottle>,
    pub(crate) selected: usize,
    pub(crate) wine: Option<PathBuf>,
    pub(crate) wine_ver: String,
    pub(crate) log: Log,
    pub(crate) tx: Sender<Msg>,
    pub(crate) rx: Receiver<Msg>,
    pub(crate) running: Vec<Running>,
    /// Programs started by the app whose first process ended while the bottle stays in use.
    pub(crate) handed_over: Vec<HandedOver>,
    /// Programs being started, or whose exit is being judged (bottle, file) and since when:
    /// together with `running` and `handed_over` this stops a second copy from starting.
    pub(crate) opening: std::collections::HashMap<(PathBuf, PathBuf), std::time::Instant>,
    pub(crate) busy: std::collections::BTreeMap<PathBuf, String>,
    /// The line in the status bar. Set it with `say`, `done` or `fail`; a line written straight
    /// into it counts as plain information.
    pub(crate) status: String,
    /// The kind of the status line, with the text it was set for (see `App::status_kind`).
    pub(crate) status_kind: (String, NoticeKind),
    /// Errors waiting to be seen, at most one for each bottle (and one for none).
    pub(crate) errors: Vec<Notice>,
    /// What the app itself is busy with that belongs to no bottle (importing a bottle…).
    pub(crate) app_job: Option<String>,
    pub(crate) modal: Option<Modal>,
    pub(crate) new_name: String,
    /// The last answer for the name being typed in a dialog (cleared when the dialog closes).
    pub(crate) name_check: Option<NameCheck>,
    pub(crate) new_windows: String,
    /// New bottle on the games engine.
    pub(crate) new_games: bool,
    pub(crate) run_args: String,
    pub(crate) run_save: bool,
    pub(crate) run_hud: bool,
    pub(crate) run_avx: bool,
    pub(crate) rename_text: String,
    pub(crate) wine_state: wine_setup::WineState,
    pub(crate) setup: wine_setup::Shared,
    pub(crate) auto_install: bool,
    pub(crate) ctx: egui::Context,
    pub(crate) screenshot: Option<PathBuf>,
    pub(crate) frames: u64,
    pub(crate) shot_done: Arc<AtomicBool>,
    pub(crate) shot_requested: bool,
    /// A Windows file Finder handed over before Wine was found: it is offered once Wine is.
    pub(crate) pending_open: Option<PathBuf>,
    /// The install dialog's choices while it is open.
    pub(crate) install_draft: Option<InstallDraft>,
    /// The installation going on, or the last one until the person closes its card.
    pub(crate) install: Option<domain::install::InstallSession>,
    pub(crate) install_next_id: u64,
    /// A worker is reading the programs of the bottle being installed into.
    pub(crate) install_scanning: bool,
    /// In a row, how many checks found the bottle idle after the installer's first process ended.
    pub(crate) install_idle: u8,
    /// The person asked to close the installer: it is over once the bottle has closed.
    pub(crate) install_stopping: bool,
    pub(crate) textures: std::collections::HashMap<PathBuf, Option<egui::TextureHandle>>,
    pub(crate) launcher_count: usize,
    /// One sync of the Mac launchers at a time (see `OnePass`).
    pub(crate) launcher_pass: Arc<OnePass>,
    /// The bottle waiting for the games engine to finish installing.
    /// The bottle waiting for an engine that is still being downloaded.
    pub(crate) pending_engine_bottle: Option<(PathBuf, wine_setup::Engine)>,
    /// Bottles with Wine running (checked every few seconds, also for programs opened elsewhere).
    pub(crate) live: Vec<PathBuf>,
    pub(crate) last_live_check: f64,
    /// True while a live check is still running on its worker, so the next tick is skipped
    /// instead of piling up more checks.
    pub(crate) live_check_busy: Arc<AtomicBool>,
    pub(crate) engine_versions: std::collections::HashMap<PathBuf, String>,
    /// Engines whose version a worker is asked for (or was), so each is asked once.
    pub(crate) versions_requested: std::collections::HashSet<PathBuf>,
    pub(crate) screen: Screen,
    pub(crate) tab: BottleTab,
    /// Filters the programs of the bottle shown.
    pub(crate) search: String,
    /// What the installed engines can do (read on start, rescans and installs, not per frame).
    pub(crate) engines: domain::engines::EngineStatus,
    /// The diagnosis of each bottle, which ones are being read, and when each last started.
    pub(crate) diagnosis: diagnostics::Shown,
    /// Program pictures already asked for (decoded on a worker).
    pub(crate) icons_requested: std::collections::HashSet<PathBuf>,
    /// When the page, the dialog and the status line last changed (for their transitions).
    pub(crate) view_key: Option<(Screen, usize, BottleTab)>,
    pub(crate) view_since: f64,
    pub(crate) modal_kind: Option<std::mem::Discriminant<Modal>>,
    pub(crate) modal_since: f64,
    pub(crate) status_shown: String,
    pub(crate) status_since: f64,
    /// Measures the window at rest (`--reposo`).
    pub(crate) idle_probe: Option<perf::IdleProbe>,
    /// `--vista acerca`: Ajustes opens scrolled to its end, to look at «Acerca de» in a
    /// screenshot (nothing else sets it).
    pub(crate) settings_at_end: bool,
}

impl App {
    pub(crate) fn new(
        cc: &eframe::CreationContext<'_>,
        screenshot: Option<PathBuf>,
        pending_open: Option<PathBuf>,
        start: StartOptions,
    ) -> Self {
        Self::with_context(cc.egui_ctx.clone(), screenshot, pending_open, start)
    }

    /// Builds the app from an egui context alone, so the flow tests can run it without a window.
    pub(crate) fn with_context(
        ctx: egui::Context,
        screenshot: Option<PathBuf>,
        pending_open: Option<PathBuf>,
        start: StartOptions,
    ) -> Self {
        let (cfg, cfg_load) = Config::load_noting();
        ui::theme::install(&ctx, start.theme, cfg.appearance);
        let wine = None;
        let wine_ver = String::new();
        let bottles = scan_bottles(&cfg);
        let (tx, rx) = channel();
        let auto_install = screenshot.is_none() && start.idle.is_none();
        let app = Self {
            cfg,
            bottles,
            selected: 0,
            wine,
            wine_ver,
            log: Default::default(),
            tx,
            rx,
            running: vec![],
            handed_over: vec![],
            opening: Default::default(),
            busy: Default::default(),
            status: String::new(),
            status_kind: (String::new(), NoticeKind::Info),
            errors: vec![],
            app_job: None,
            modal: None,
            new_name: "Mi botella".into(),
            name_check: None,
            new_windows: "win10".into(),
            new_games: false,
            run_args: String::new(),
            run_save: true,
            run_hud: false,
            run_avx: false,
            rename_text: String::new(),
            wine_state: wine_setup::WineState::Unknown,
            setup: Default::default(),
            auto_install,
            ctx,
            screenshot,
            frames: 0,
            shot_done: Arc::new(AtomicBool::new(false)),
            shot_requested: false,
            pending_open,
            install_draft: None,
            install: None,
            install_next_id: 1,
            install_scanning: false,
            install_idle: 0,
            install_stopping: false,
            textures: Default::default(),
            launcher_count: 0,
            launcher_pass: Default::default(),
            pending_engine_bottle: None,
            live: vec![],
            last_live_check: -10.0,
            live_check_busy: Arc::new(AtomicBool::new(false)),
            engine_versions: Default::default(),
            versions_requested: Default::default(),
            screen: Screen::Home,
            tab: BottleTab::Programs,
            search: String::new(),
            engines: Default::default(),
            diagnosis: Default::default(),
            icons_requested: Default::default(),
            view_key: None,
            view_since: 0.0,
            modal_kind: None,
            modal_since: 0.0,
            status_shown: String::new(),
            status_since: 0.0,
            idle_probe: start.idle.map(perf::IdleProbe::new),
            settings_at_end: start.view.as_deref() == Some("acerca"),
        };
        let mut app = app;
        // A settings file that could not be read was kept aside; the person is told once.
        if let ConfigLoad::Unreadable { saved_as } = cfg_load {
            app.tell_unreadable_settings(saved_as.as_deref());
        }
        if let Some(i) = app.default_index() {
            app.selected = i;
        }
        app.refresh_engines();
        match start.view.as_deref() {
            Some("botella") => app.screen = Screen::Bottle,
            Some("config") => (app.screen, app.tab) = (Screen::Bottle, BottleTab::Settings),
            Some("diagnostico") => (app.screen, app.tab) = (Screen::Bottle, BottleTab::Diagnostics),
            Some("ajustes" | "acerca") => app.screen = Screen::Settings,
            Some("nueva") => app.modal = Some(Modal::NewBottle),
            _ => {}
        }
        // Look for Wine without freezing the window; install it by itself if it's missing.
        app.detect_wine(auto_install);
        app
    }
}
