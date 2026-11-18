//! One app at a time. A second launch (a Finder double-click on a Windows file, or the icon
//! clicked again) must not start a second window with its own live check and its own writer of
//! `config.json`: it hands what it was asked to open to the app that is running and ends.
//!
//! The running app holds an exclusive lock on a file in its data folder. The lock belongs to the
//! process, so it disappears when the process does and can never go stale. A second launch that
//! finds the lock taken writes a small request file into the `abrir` folder; the running app
//! looks into that folder from `App::tick` and treats each request like a Finder open.
//!
//! Nothing here may stop the app from starting: when the lock file cannot even be made (a
//! read-only disk) the app starts as it always did.
//!
//! A Windows file opened from the Finder when no app is running does not need the window at all:
//! `open_without_window` starts it in the default bottle, the way a request would once the window
//! looked at it.

use super::bottle_engine_path;
use crate::backend::data_dir;
use crate::*;
use std::fs;
use std::io::Write;
use std::os::unix::ffi::OsStringExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Once, OnceLock};
use std::time::{Duration, SystemTime};

/// The lock file, in the app's data folder.
const LOCK_FILE: &str = "instancia.lock";

/// The folder where a second launch leaves its requests.
const REQUESTS_DIR: &str = "abrir";

/// The ending of a finished request. A request is written under a temporary name and renamed
/// to this, so the running app never reads half of one.
const REQUEST_EXT: &str = "pedido";

/// A request is a path. Anything bigger than this is not one.
const REQUEST_MAX_BYTES: u64 = 64 * 1024;

/// A temporary request that is older than this was left by a launch that died half way.
const STALE_TMP_SECS: u64 = 60;

/// A request older than this that was already there when this app started is dropped unread. It
/// was left for an app that ended before reading it, and opening its file days later, unasked,
/// would only be a surprise. A request written while this app runs never expires: the app may
/// be busy (a dialog is open, Wine is installing) and takes it when it can.
const REQUEST_MAX_AGE_SECS: u64 = 120;

/// How often the running app looks for requests, at most.
const POLL_SECS: f64 = 0.9;

/// The folder of the requests.
pub(crate) fn requests_dir() -> PathBuf {
    data_dir().join(REQUESTS_DIR)
}

/// Whether a launch with these arguments takes part in the one-app-at-a-time rule. The modes that
/// measure or take pictures run next to the person's own app on purpose and must neither wait
/// for it nor hand anything over; the test runs never touch the lock either.
pub(crate) fn takes_lock(args: &[String]) -> bool {
    !cfg!(test) && mode_takes_lock(args)
}

/// [`takes_lock`] without the test guard, so the decision itself can be tested.
fn mode_takes_lock(args: &[String]) -> bool {
    !args
        .iter()
        .any(|a| matches!(a.as_str(), "--screenshot" | "--reposo" | "--medir"))
}

/// What asking for the lock found.
#[derive(Debug)]
pub(crate) enum Claim {
    /// This process is the app now; the lock lasts as long as the file stays open.
    Held(fs::File),
    /// Another app holds it.
    Busy,
    /// The lock cannot be had at all (no folder, a read-only disk): start without it.
    Unavailable,
}

/// Tries to take the lock of the app whose data folder is `dir`, once (the tests' way to be the
/// app, or a launch that holds the lock for a moment).
#[cfg(test)]
pub(crate) fn claim(dir: &Path) -> Claim {
    claim_after(dir, 1, Duration::ZERO)
}

/// How many times a launch looks at a lock that is taken, and how long it waits between looks.
/// A launch that only starts a program holds the lock for a moment while it asks, so a lock
/// that is taken is only believed to belong to an app when it stays taken for a few of these.
const SETTLE_TRIES: u32 = 4;
const SETTLE_GAP: Duration = Duration::from_millis(25);

/// Like [`claim`], but a lock that is taken is looked at again a few times first. The Finder
/// starts the launches of files opened together side by side, and one that is only asking whether
/// an app runs holds the lock for a moment: another launch that happened to look at that moment
/// would hand its file to an app that does not exist, and the file would never open. A real app
/// keeps the lock, so it costs only a short wait before handing over.
pub(crate) fn claim_settled(dir: &Path) -> Claim {
    claim_after(dir, SETTLE_TRIES, SETTLE_GAP)
}

/// Asks for the lock up to `tries` times, `gap` apart, and reports it taken only if it stayed so.
fn claim_after(dir: &Path, tries: u32, gap: Duration) -> Claim {
    app_started();
    if fs::create_dir_all(dir).is_err() {
        return Claim::Unavailable;
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(dir.join(LOCK_FILE));
    let Ok(file) = file else {
        return Claim::Unavailable;
    };
    for attempt in 0..tries.max(1) {
        if attempt > 0 {
            std::thread::sleep(gap);
        }
        match file.try_lock() {
            Ok(()) => return Claim::Held(file),
            Err(fs::TryLockError::WouldBlock) => {}
            Err(fs::TryLockError::Error(_)) => return Claim::Unavailable,
        }
    }
    Claim::Busy
}

/// For a launch that only starts a program and ends (a file opened with no window): asks whether
/// the app is running and, if so, hands it the file and returns true, so this launch ends. It
/// lets the lock go at once. A launch that held it while it started Wine would turn away a second
/// one to a process that never reads requests, and the file would be lost.
pub(crate) fn ask_running_app(lock_dir: &Path, requests: &Path, file: Option<&Path>) -> bool {
    match claim_settled(lock_dir) {
        Claim::Busy => hand_over(requests, file).is_ok(),
        Claim::Held(_) | Claim::Unavailable => false,
    }
}

/// A numbering that keeps the requests of one launch apart from the next one's.
static REQUEST_SEQ: AtomicU64 = AtomicU64::new(0);

/// Leaves a request for the running app in `dir`: the file to open, or none to only bring the
/// window forward. The file is written under a temporary name and then renamed, so it appears
/// whole.
pub(crate) fn hand_over(dir: &Path, file: Option<&Path>) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = format!(
        "{nanos:024}-{}-{}",
        std::process::id(),
        REQUEST_SEQ.fetch_add(1, Ordering::SeqCst)
    );
    // The path is made absolute here, where this launch's own folder is known.
    let content = match file {
        Some(f) => std::path::absolute(f)?.into_os_string().into_vec(),
        None => vec![],
    };
    let tmp = dir.join(format!(".{name}.tmp"));
    let mut out = fs::File::create(&tmp)?;
    let written = out.write_all(&content).and_then(|()| out.flush());
    drop(out);
    if let Err(e) =
        written.and_then(|()| fs::rename(&tmp, dir.join(format!("{name}.{REQUEST_EXT}"))))
    {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// What the requests of one look into the folder came to.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Taken {
    /// Something was asked, so the window comes forward.
    pub(crate) show: bool,
    /// The file to open. Only one at a time: the next request waits in its file.
    pub(crate) open: Option<PathBuf>,
    /// Files that were asked for and are not there any more.
    pub(crate) missing: Vec<PathBuf>,
}

/// One request file, read.
enum Request {
    /// Only bring the window forward.
    Show,
    Open(PathBuf),
    /// Not a request: wrong size or not an absolute path.
    Bad,
}

fn parse(bytes: Vec<u8>) -> Request {
    if bytes.is_empty() {
        return Request::Show;
    }
    if bytes.contains(&0) {
        return Request::Bad;
    }
    let path = PathBuf::from(std::ffi::OsString::from_vec(bytes));
    if path.is_absolute() {
        Request::Open(path)
    } else {
        Request::Bad
    }
}

/// When this app started (set the first time it is asked, which is at startup).
fn app_started() -> SystemTime {
    static STARTED: OnceLock<SystemTime> = OnceLock::new();
    *STARTED.get_or_init(SystemTime::now)
}

/// Reads and removes the requests in `dir`, oldest first. A request that was left before
/// `started` and is older than [`REQUEST_MAX_AGE_SECS`] is dropped unread. When `want_open` is false (the app is
/// still holding the file of an earlier request) a request for a file stays where it is; one
/// that only asks for the window is always taken. A request that cannot be removed is not acted
/// on, so a file the app cannot delete cannot be opened over and over.
pub(crate) fn take_requests(dir: &Path, want_open: bool, started: SystemTime) -> Taken {
    let mut taken = Taken::default();
    let Ok(entries) = fs::read_dir(dir) else {
        return taken;
    };
    let mut names: Vec<PathBuf> = vec![];
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == REQUEST_EXT) {
            names.push(path);
        } else {
            sweep_stale(&path);
        }
    }
    names.sort();
    for path in names {
        let left_before_start = fs::metadata(&path)
            .and_then(|m| m.modified())
            .is_ok_and(|written| written < started);
        if left_before_start && age_secs(&path).is_some_and(|age| age > REQUEST_MAX_AGE_SECS) {
            let _ = fs::remove_file(&path);
            continue;
        }
        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let request = if size > REQUEST_MAX_BYTES {
            Request::Bad
        } else {
            match fs::read(&path) {
                Ok(bytes) => parse(bytes),
                Err(_) => continue,
            }
        };
        if matches!(request, Request::Open(_)) && (!want_open || taken.open.is_some()) {
            continue;
        }
        if fs::remove_file(&path).is_err() {
            continue;
        }
        taken.show = true;
        if let Request::Open(file) = request {
            if file.exists() {
                taken.open = Some(file);
            } else {
                taken.missing.push(file);
            }
        }
    }
    taken
}

/// How many seconds ago a file was last written, when that can be told.
fn age_secs(path: &Path) -> Option<u64> {
    let modified = fs::metadata(path).and_then(|m| m.modified()).ok()?;
    Some(modified.elapsed().ok()?.as_secs())
}

/// Removes a temporary request that a launch left behind when it died.
fn sweep_stale(path: &Path) {
    let old = age_secs(path).is_some_and(|age| age > STALE_TMP_SECS);
    if old && path.extension().is_some_and(|e| e == "tmp") {
        let _ = fs::remove_file(path);
    }
}

/// Whether a request waits in `dir`.
fn has_requests(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|mut d| {
        d.any(|e| e.is_ok_and(|e| e.path().extension().is_some_and(|x| x == REQUEST_EXT)))
    })
}

/// The time (the window's clock, as bits) of the last look into the folder.
static LAST_LOOK: AtomicU64 = AtomicU64::new(0);

/// Whether it is time to look into the folder again: at most about once a second.
fn look_is_due(now: f64) -> bool {
    let last = f64::from_bits(LAST_LOOK.load(Ordering::SeqCst));
    if now - last < POLL_SECS && now >= last {
        return false;
    }
    LAST_LOOK.store(now.to_bits(), Ordering::SeqCst);
    true
}

/// Wakes the window when a request arrives. A window at rest draws a frame only every few
/// seconds, which would leave a second launch waiting that long; this thread costs one folder
/// look a second and a frame only when there is something to read.
fn watch(ctx: &egui::Context) {
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("peticiones".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_secs(1));
                if has_requests(&requests_dir()) {
                    ctx.request_repaint();
                }
            });
        drop(spawned);
    });
}

/// What the person is told about a file that was asked for and is gone.
pub(crate) fn missing_text(file: &Path) -> String {
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| file.to_string_lossy().to_string());
    format!("No encuentro «{name}»: ya no está donde estaba.")
}

impl App {
    /// Between two frames: what another launch asked for. The file goes the way a Finder open
    /// goes (`pending_open`, then the install dialog or the direct open) and the window comes
    /// forward. Only the normal mode looks: the measuring and picture modes have no other launch.
    pub(crate) fn follow_requests(&mut self, ctx: &egui::Context) {
        if self.screenshot.is_some() || self.idle_probe.is_some() {
            return;
        }
        watch(ctx);
        if !look_is_due(ctx.input(|i| i.time)) {
            return;
        }
        let taken = take_requests(&requests_dir(), self.pending_open.is_none(), app_started());
        if !taken.show {
            return;
        }
        for file in &taken.missing {
            self.say(missing_text(file));
        }
        if let Some(file) = taken.open {
            self.pending_open = Some(file);
            if self.install_running() {
                self.say("Abriré ese archivo cuando termine la instalación que está en marcha.");
            }
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }
}

/// The bottle a file opened from the Finder with no window goes to: the default one, or else the
/// first one. A bottle that never finished being made has no Windows in it (Wine would quietly
/// build one with its own settings), so there is no bottle then: the window opens and says why,
/// as it does for any other way of opening a file.
fn bottle_for_open<'a>(default: Option<&str>, bottles: &'a [Bottle]) -> Option<&'a Bottle> {
    default
        .and_then(|d| bottles.iter().find(|b| b.path.to_string_lossy() == d))
        .or_else(|| bottles.first())
        .filter(|b| b.prefix_complete)
}

/// `--open <file>`: what a Finder double-click sends. When Wine and a bottle are ready the file
/// opens straight away with no window at all (through `wine --start`); otherwise the app window
/// opens and finishes the job. What the program prints goes to the app's log file under the
/// bottle's start marker, like a program opened from the window, so a crash can be looked at
/// afterwards. Wine receives exactly the command the window would build: only where its output
/// goes differs.
pub(crate) fn open_without_window(file: &Path) -> bool {
    let cfg = Config::load();
    let Some(wine) = cfg
        .wine_path
        .as_ref()
        .map(PathBuf::from)
        .filter(|p| wine_setup::is_free_wine(p) && wine_setup::probe(p).is_ok())
    else {
        return false;
    };
    let bottles = scan_bottles(&cfg);
    let Some(b) = bottle_for_open(cfg.default_bottle.as_deref(), &bottles) else {
        return false;
    };
    let wine = bottle_engine_path(&cfg, b).unwrap_or(wine);
    let mut cmd = open_command(&wine, b, file, "", &[]);
    cmd.stdin(std::process::Stdio::null());
    let log = Log::default();
    note_run_start(&log, &b.name);
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| file.to_string_lossy().to_string());
    push_log(&log, format!("▶ {name} {}", diagnostics::FROM_FINDER));
    // The output goes to the file only after the app's own lines are in it: when those lines
    // roll an oversized log over, the program must write into the new file and not the old one.
    send_output_to_log(&mut cmd, &log_file());
    cmd.spawn().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bl-instance-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// True once the lock of `dir` can be had, waiting up to 2 s for it. The lock belongs to the
    /// open file, and a test thread that forks a child (every test that starts a process does)
    /// shares that file for an instant, so the lock may outlive `drop` by a moment.
    fn lock_is_free_soon(dir: &Path) -> bool {
        let end = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if matches!(claim(dir), Claim::Held(_)) {
                return true;
            }
            if std::time::Instant::now() > end {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// A lock that is let go a moment late is found free by the helper the other tests use.
    #[test]
    fn a_lock_let_go_late_is_waited_for() {
        let dir = scratch("late");
        let holder = claim(&dir);
        assert!(matches!(holder, Claim::Held(_)));
        assert!(matches!(claim(&dir), Claim::Busy));
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            drop(holder);
        });
        assert!(lock_is_free_soon(&dir));
        release.join().unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// The measuring, picture and idle modes never take the lock; a normal start and a start
    /// that only opens a file do.
    #[test]
    fn the_measuring_and_picture_modes_do_not_take_the_lock() {
        assert!(mode_takes_lock(&args(&["app"])));
        assert!(mode_takes_lock(&args(&["app", "--open", "/x/a.exe"])));
        assert!(mode_takes_lock(&args(&["app", "--tema", "oscuro"])));
        for mode in [
            ["app", "--screenshot", "/x/a.png"],
            ["app", "--reposo", "5"],
            ["app", "--medir", "/x"],
        ] {
            assert!(!mode_takes_lock(&args(&mode)), "{mode:?}");
        }
        // And under test nothing takes it at all.
        assert!(!takes_lock(&args(&["app"])));
    }

    /// The lock is exclusive, is let go when the holder's file closes, and a folder that cannot
    /// be made is no reason to refuse to start.
    #[test]
    fn the_lock_is_exclusive_and_ends_with_its_holder() {
        let dir = scratch("lock");
        let first = claim(&dir);
        assert!(matches!(first, Claim::Held(_)), "{first:?}");
        assert!(matches!(claim(&dir), Claim::Busy));
        drop(first);
        assert!(lock_is_free_soon(&dir));
        // A "folder" that is a file: no lock file can be made, and that is not an error.
        let blocker = dir.join("archivo");
        fs::write(&blocker, "").unwrap();
        assert!(matches!(claim(&blocker.join("datos")), Claim::Unavailable));
        let _ = fs::remove_dir_all(&dir);
    }

    /// A request goes in whole and comes out as the file it named, once, and the file is gone.
    #[test]
    fn a_request_makes_the_round_trip() {
        let dir = scratch("trip");
        let target = dir.join("Instalador.exe");
        fs::write(&target, "MZ").unwrap();
        let queue = dir.join("abrir");
        hand_over(&queue, Some(&target)).unwrap();
        // Nothing half written is left to be read.
        assert!(fs::read_dir(&queue)
            .unwrap()
            .flatten()
            .all(|e| e.path().extension().is_some_and(|x| x == REQUEST_EXT)));
        let taken = take_requests(&queue, true, SystemTime::now());
        assert_eq!(taken.open.as_deref(), Some(target.as_path()));
        assert!(taken.show && taken.missing.is_empty());
        assert_eq!(fs::read_dir(&queue).unwrap().count(), 0);
        assert_eq!(
            take_requests(&queue, true, SystemTime::now()),
            Taken::default()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// A launch without a file only asks for the window; a relative path is made absolute.
    #[test]
    fn a_request_without_a_file_only_shows_the_window() {
        let dir = scratch("show");
        hand_over(&dir, None).unwrap();
        let taken = take_requests(&dir, false, SystemTime::now());
        assert!(taken.show && taken.open.is_none() && taken.missing.is_empty());
        hand_over(&dir, Some(Path::new("relativo.exe"))).unwrap();
        let name = fs::read_dir(&dir).unwrap().flatten().next().unwrap().path();
        assert!(Path::new(&OsStr::from_bytes(&fs::read(name).unwrap())).is_absolute());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Requests are taken in the order they were made, one file at a time; the next one waits in
    /// its file until the app can take it, and a held-up app still takes the plain "show" ones.
    #[test]
    fn requests_for_files_are_taken_one_at_a_time_in_order() {
        let dir = scratch("order");
        let queue = dir.join("abrir");
        let (a, b) = (dir.join("a.exe"), dir.join("b.exe"));
        fs::write(&a, "MZ").unwrap();
        fs::write(&b, "MZ").unwrap();
        hand_over(&queue, Some(&a)).unwrap();
        hand_over(&queue, Some(&b)).unwrap();
        hand_over(&queue, None).unwrap();
        // The app is still holding an earlier file: only the plain request goes.
        let held = take_requests(&queue, false, SystemTime::now());
        assert!(held.show && held.open.is_none());
        assert_eq!(fs::read_dir(&queue).unwrap().count(), 2);
        let first = take_requests(&queue, true, SystemTime::now());
        assert_eq!(first.open.as_deref(), Some(a.as_path()));
        let second = take_requests(&queue, true, SystemTime::now());
        assert_eq!(second.open.as_deref(), Some(b.as_path()));
        assert_eq!(
            take_requests(&queue, true, SystemTime::now()),
            Taken::default()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// A file that is gone, a request that is not a path, a huge one and a stranger's file in
    /// the folder are all handled without opening anything and without a panic.
    #[test]
    fn requests_with_a_missing_file_or_bad_content_are_dropped() {
        let dir = scratch("bad");
        let queue = dir.join("abrir");
        fs::create_dir_all(&queue).unwrap();
        hand_over(&queue, Some(&dir.join("ya-no-esta.exe"))).unwrap();
        fs::write(queue.join("1.pedido"), "relativo/a.exe").unwrap();
        fs::write(queue.join("2.pedido"), b"/con\0nulo.exe").unwrap();
        fs::write(queue.join("3.pedido"), vec![b'/'; 100_000]).unwrap();
        fs::write(queue.join("nota.txt"), "no es un pedido").unwrap();
        let taken = take_requests(&queue, true, SystemTime::now());
        assert!(taken.open.is_none());
        assert_eq!(taken.missing, vec![dir.join("ya-no-esta.exe")]);
        assert!(taken.show);
        // Every request is removed; what is not a request stays.
        let left: Vec<_> = fs::read_dir(&queue)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(left, vec![std::ffi::OsString::from("nota.txt")]);
        // A folder that does not exist has nothing to take.
        assert_eq!(
            take_requests(&dir.join("nada"), true, SystemTime::now()),
            Taken::default()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// A request that nobody read in two minutes belongs to an app that ended: it is removed
    /// without opening anything, and a fresh one next to it still works.
    #[test]
    fn an_old_request_is_dropped_unread() {
        let dir = scratch("old");
        let queue = dir.join("abrir");
        let (old, fresh) = (dir.join("vieja.exe"), dir.join("nueva.exe"));
        fs::write(&old, "MZ").unwrap();
        fs::write(&fresh, "MZ").unwrap();
        hand_over(&queue, Some(&old)).unwrap();
        let stale = fs::read_dir(&queue)
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path();
        let long_ago = std::time::SystemTime::now() - Duration::from_secs(600);
        fs::File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        hand_over(&queue, Some(&fresh)).unwrap();
        let taken = take_requests(&queue, true, SystemTime::now());
        assert_eq!(taken.open.as_deref(), Some(fresh.as_path()));
        assert!(taken.missing.is_empty());
        assert!(!stale.exists());
        assert_eq!(fs::read_dir(&queue).unwrap().count(), 0);
        // On its own, an old request does not even bring the window forward.
        hand_over(&queue, Some(&old)).unwrap();
        let stale = fs::read_dir(&queue)
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path();
        fs::File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        assert_eq!(
            take_requests(&queue, true, SystemTime::now()),
            Taken::default()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// A request written while the app runs waits as long as the app needs (a dialog is open,
    /// Wine is installing) and is opened when the app can take it; only requests left before
    /// the app started expire.
    #[test]
    fn a_request_waiting_for_a_busy_app_does_not_expire() {
        let dir = scratch("busy");
        let queue = dir.join("abrir");
        let target = dir.join("espera.exe");
        fs::write(&target, "MZ").unwrap();
        hand_over(&queue, Some(&target)).unwrap();
        let waiting = fs::read_dir(&queue)
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path();
        // The request is ten minutes old, but it was written after the app started.
        let ten_minutes_ago = SystemTime::now() - Duration::from_secs(600);
        fs::File::options()
            .write(true)
            .open(&waiting)
            .unwrap()
            .set_modified(ten_minutes_ago)
            .unwrap();
        let started = ten_minutes_ago - Duration::from_secs(60);
        // The app is not ready to open files: the request stays, as it did when it was new.
        let held = take_requests(&queue, false, started);
        assert!(held.open.is_none() && !held.show);
        assert!(waiting.exists());
        // Once it is ready, the request is read, not thrown away.
        let taken = take_requests(&queue, true, started);
        assert_eq!(taken.open.as_deref(), Some(target.as_path()));
        assert!(!waiting.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// A launch that only starts a program asks for the app and does not keep the lock, so a
    /// second one that arrives while it starts Wine still runs on its own.
    #[test]
    fn a_windowless_launch_hands_over_to_the_app_but_never_keeps_the_lock() {
        let dir = scratch("ask");
        let queue = dir.join("abrir");
        let target = dir.join("a.exe");
        // No app: nothing is handed over, and the lock is free afterwards.
        assert!(!ask_running_app(&dir, &queue, Some(&target)));
        assert!(!queue.exists());
        let app = claim(&dir);
        assert!(matches!(app, Claim::Held(_)));
        // An app is running: the file goes to it.
        assert!(ask_running_app(&dir, &queue, Some(&target)));
        assert_eq!(
            take_requests(&queue, true, SystemTime::now()).missing,
            vec![target]
        );
        drop(app);
        assert!(lock_is_free_soon(&dir));
        // Asking twice in a row never leaves the lock taken.
        assert!(!ask_running_app(&dir, &queue, None));
        assert!(!ask_running_app(&dir, &queue, None));
        assert!(lock_is_free_soon(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    /// A lock that another launch holds only for a moment (it was just asking) is not an app: the
    /// launch that met it looks again, finds the lock free and runs its own file instead of
    /// handing it over to nobody. A lock that stays taken is an app and the file goes to it.
    #[test]
    fn a_lock_held_for_a_moment_is_not_an_app() {
        let dir = scratch("moment");
        let queue = dir.join("abrir");
        let target = dir.join("a.exe");
        let momentary = claim(&dir);
        assert!(matches!(momentary, Claim::Held(_)));
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            drop(momentary);
        });
        // Looks every 20 ms and stops as soon as the lock is free: the number of looks is only
        // the bound, so a machine that is slow to run the releasing thread still passes.
        let got = claim_after(&dir, 1000, Duration::from_millis(20));
        release.join().unwrap();
        assert!(matches!(got, Claim::Held(_)), "{got:?}");
        assert!(!queue.exists(), "nothing was handed over to nobody");
        drop(got);
        // The launch itself uses the settled look and so does not hand over either.
        let momentary = claim(&dir);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<()>();
        let release = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(5));
            drop(momentary);
        });
        // The look starts once the holder runs, so the time that a slow machine takes to start
        // the thread is not counted against the moment the lock is held.
        ready_rx.recv().unwrap();
        assert!(!ask_running_app(&dir, &queue, Some(&target)));
        release.join().unwrap();
        assert!(!queue.exists());
        // A lock that stays taken is an app.
        let app = claim(&dir);
        assert!(matches!(claim_settled(&dir), Claim::Busy));
        assert!(ask_running_app(&dir, &queue, Some(&target)));
        assert_eq!(
            take_requests(&queue, true, SystemTime::now()).missing,
            vec![target]
        );
        drop(app);
        let _ = fs::remove_dir_all(&dir);
    }

    /// The text for a vanished file names the file, not its folder.
    #[test]
    fn the_missing_file_text_names_the_file() {
        let text = missing_text(Path::new("/Users/yo/Descargas/Setup.exe"));
        assert!(text.contains("«Setup.exe»") && !text.contains("Descargas"));
    }

    /// The window looks into the folder about once a second, whatever the frame rate.
    #[test]
    fn the_look_is_due_about_once_a_second() {
        LAST_LOOK.store(0, Ordering::SeqCst);
        assert!(look_is_due(100.0));
        assert!(!look_is_due(100.2));
        assert!(!look_is_due(100.8));
        assert!(look_is_due(101.0));
        // A clock that went back is not stuck waiting.
        assert!(look_is_due(5.0));
    }

    fn bottle_at(name: &str, prefix_complete: bool) -> Bottle {
        Bottle {
            name: name.into(),
            path: PathBuf::from("/b").join(name),
            windows: "win10".into(),
            msync: true,
            high_res: false,
            logpixels: 96,
            programs: vec![],
            env: vec![],
            prefix_complete,
        }
    }

    /// A file opened from the Finder with no window never goes to a bottle that was not finished
    /// (Wine would build a Windows in it on its own).
    #[test]
    fn a_finder_open_without_a_window_skips_unfinished_bottles() {
        let unfinished = [bottle_at("Aa", false), bottle_at("Bb", true)];
        // No default yet and the first bottle never finished: the window explains.
        assert!(bottle_for_open(None, &unfinished).is_none());
        // A default that never finished is refused too, not replaced by another bottle.
        assert!(bottle_for_open(Some("/b/Aa"), &unfinished).is_none());
        // A finished default is used, wherever it is in the list.
        assert_eq!(
            bottle_for_open(Some("/b/Bb"), &unfinished).map(|b| b.name.as_str()),
            Some("Bb")
        );
        // Without a default the first bottle is used when it is finished.
        let ready = [bottle_at("Xx", true), bottle_at("Yy", true)];
        assert_eq!(
            bottle_for_open(None, &ready).map(|b| b.name.as_str()),
            Some("Xx")
        );
        assert!(bottle_for_open(None, &[]).is_none());
    }
}
