//! Frame timing for comparing interface changes: how long building one frame of the window
//! takes on the CPU (layout and painting commands, not the GPU). Printed after a `--screenshot`
//! run; costs two clock reads per frame otherwise.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

static FRAMES: AtomicU64 = AtomicU64::new(0);
static TOTAL_US: AtomicU64 = AtomicU64::new(0);
static MAX_US: AtomicU64 = AtomicU64::new(0);
/// Frames after the first few (fonts and icons are loaded in those).
static WARM_FRAMES: AtomicU64 = AtomicU64::new(0);
static WARM_US: AtomicU64 = AtomicU64::new(0);

/// Measures one frame from creation to drop.
pub struct FrameTimer(Instant);

impl FrameTimer {
    pub fn start() -> Self {
        Self(Instant::now())
    }
}

impl Drop for FrameTimer {
    fn drop(&mut self) {
        let us = self.0.elapsed().as_micros() as u64;
        let n = FRAMES.fetch_add(1, Ordering::Relaxed);
        TOTAL_US.fetch_add(us, Ordering::Relaxed);
        MAX_US.fetch_max(us, Ordering::Relaxed);
        if n >= 10 {
            WARM_FRAMES.fetch_add(1, Ordering::Relaxed);
            WARM_US.fetch_add(us, Ordering::Relaxed);
        }
    }
}

/// Prints the frame times so far to the terminal.
pub fn report() {
    let n = FRAMES.load(Ordering::Relaxed).max(1);
    let warm = WARM_FRAMES.load(Ordering::Relaxed).max(1);
    eprintln!(
        "cuadros: {n} · media {} µs · después de calentar {} µs · peor {} µs",
        TOTAL_US.load(Ordering::Relaxed) / n,
        WARM_US.load(Ordering::Relaxed) / warm,
        MAX_US.load(Ordering::Relaxed)
    );
}

// ------------------------------------------------------------------ measurements
//
// `--medir` times the work the app does around the window (finding bottles, reading the library,
// decoding pictures, asking Wine whether a bottle is in use, writing the log) and prints one line
// per step. `--reposo <seconds>` opens the window, leaves it alone and reports how many frames it
// painted and how much processor time it used meanwhile. Both exist to compare before and after
// a change with the same steps.

/// Frames painted so far.
pub fn frames() -> u64 {
    FRAMES.load(Ordering::Relaxed)
}

/// Runs `f` `runs` times and prints the first, the fastest and the middle time.
pub fn timed<T>(label: &str, runs: usize, mut f: impl FnMut() -> T) -> T {
    let mut times = Vec::with_capacity(runs);
    let mut last = None;
    for _ in 0..runs.max(1) {
        let t = Instant::now();
        last = Some(f());
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let first = times[0];
    times.sort_by(|a, b| a.total_cmp(b));
    eprintln!(
        "{label}: primera {first:.2} ms · mínima {:.2} ms · mediana {:.2} ms ({} veces)",
        times[0],
        times[times.len() / 2],
        times.len()
    );
    last.expect("ran at least once")
}

/// What `ps` says about this process: memory in use (KiB) and processor time used (seconds).
pub fn process_usage() -> Option<(u64, f64)> {
    let out = std::process::Command::new("/bin/ps")
        .args(["-o", "rss=,time=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut parts = text.split_whitespace();
    let rss = parts.next()?.parse().ok()?;
    Some((rss, cpu_seconds(parts.next()?)?))
}

/// What the app holds besides memory: the things that would grow if something were left open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resources {
    pub threads: usize,
    /// Open files, pipes and sockets.
    pub files: usize,
    /// Processes started by the app that still exist.
    pub children: usize,
    /// Of those, the ones that ended and nobody waited for.
    pub zombies: usize,
}

/// Counts the processes whose parent is `me` in the output of `ps -A -o ppid=,stat=`, and how
/// many of them are zombies (their state starts with `Z`).
fn count_children(ps_output: &str, me: u32) -> (usize, usize) {
    let mut children = 0;
    let mut zombies = 0;
    for line in ps_output.lines() {
        let mut parts = line.split_whitespace();
        if parts.next().and_then(|p| p.parse::<u32>().ok()) == Some(me) {
            children += 1;
            zombies += usize::from(parts.next().is_some_and(|s| s.starts_with('Z')));
        }
    }
    (children, zombies)
}

/// Reads [`Resources`] of this process (for `--reposo`; it starts two small programs, so it is
/// never asked while the window draws).
pub fn process_resources() -> Option<Resources> {
    let me = std::process::id();
    let ps = |args: &[&str]| {
        std::process::Command::new("/bin/ps")
            .args(args)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
    };
    // `ps -M` prints a header and then one line for each thread.
    let threads = ps(&["-M", "-p", &me.to_string()])?
        .lines()
        .count()
        .saturating_sub(1);
    // Listing the folder opens one more file, which is not the app's.
    let files = std::fs::read_dir("/dev/fd").ok()?.count().saturating_sub(1);
    // The `ps` that lists them is one of them too.
    let (children, zombies) = count_children(&ps(&["-A", "-o", "ppid=,stat="])?, me);
    let children = children.saturating_sub(1);
    Some(Resources {
        threads,
        files,
        children,
        zombies,
    })
}

/// "1:02.50" or "1:01:02.50" as seconds.
fn cpu_seconds(text: &str) -> Option<f64> {
    text.split(':').try_fold(0.0, |total, part| {
        Some(total * 60.0 + part.parse::<f64>().ok()?)
    })
}

/// Watches the window while nobody touches it (`--reposo`).
pub struct IdleProbe {
    seconds: f64,
    shared: Arc<IdleShared>,
    /// The watchdog thread was started (at the first frame).
    watching: bool,
}

/// What the frames and the watchdog thread share.
#[derive(Default)]
struct IdleShared {
    start: Mutex<Option<IdleStart>>,
    /// The result was printed (by whoever got there first).
    reported: AtomicBool,
}

/// The state of the measurement when it began.
#[derive(Clone, Copy)]
struct IdleStart {
    /// The window's clock at that moment; none when the watchdog began the measurement because
    /// no frame came (a hidden window that macOS stopped waking).
    t0: Option<f64>,
    at: Instant,
    frames: u64,
    cpu: f64,
    res: Option<Resources>,
}

impl IdleStart {
    fn now(t0: Option<f64>) -> Self {
        Self {
            t0,
            at: Instant::now(),
            frames: frames(),
            cpu: process_usage().map(|u| u.1).unwrap_or(0.0),
            res: process_resources(),
        }
    }
}

/// The first seconds are start-up work, not rest.
const IDLE_WARM_UP: f64 = 5.0;

/// How long after its time the measurement may take before the watchdog does it without a frame.
/// A frame normally comes at the exact moment, so this only matters when none does.
const IDLE_GRACE: f64 = 3.0;

/// How long the watchdog gives the window to close after asking, before it ends the process.
const IDLE_CLOSE_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// What the watchdog found at one look.
#[derive(Debug, PartialEq)]
enum Watch {
    /// Nothing to do yet.
    Wait,
    /// The measurement began (no frame did it in time).
    Started,
    /// The measurement ended without a frame: print the result and close.
    Finish(f64),
    /// A frame already reported; the watchdog is not needed any more.
    Done,
}

/// One look of the watchdog, `now` being the moment of the look and `spawned` the moment it
/// began. It acts only when the frames are late by [`IDLE_GRACE`], so a run whose window keeps
/// drawing is measured by its frames alone, exactly as without it.
fn watch_once(shared: &IdleShared, spawned: Instant, seconds: f64, now: Instant) -> Watch {
    if shared.reported.load(Ordering::SeqCst) {
        return Watch::Done;
    }
    let mut start = shared.start.lock().unwrap_or_else(|e| e.into_inner());
    let since = |t: Instant| now.saturating_duration_since(t).as_secs_f64();
    match *start {
        None if since(spawned) >= IDLE_WARM_UP + IDLE_GRACE => {
            *start = Some(IdleStart::now(None));
            Watch::Started
        }
        Some(s) if since(s.at) >= seconds + IDLE_GRACE => {
            if shared.reported.swap(true, Ordering::SeqCst) {
                Watch::Done
            } else {
                Watch::Finish(since(s.at))
            }
        }
        _ => Watch::Wait,
    }
}

/// The result line of a measurement.
fn idle_line(secs: f64, frames: u64, cpu: f64, rss_kib: u64) -> String {
    format!(
        "reposo {secs:.1} s: {frames} cuadros ({:.2} por segundo) · procesador {cpu:.2} s ({:.2} %) · memoria {:.1} MB",
        frames as f64 / secs,
        cpu / secs * 100.0,
        rss_kib as f64 / 1024.0
    )
}

/// Prints what the window cost since `start`, `secs` seconds ago.
fn print_idle_result(start: &IdleStart, secs: f64) {
    let (rss, cpu) = process_usage().unwrap_or((0, start.cpu));
    eprintln!(
        "{}",
        idle_line(secs, frames() - start.frames, cpu - start.cpu, rss)
    );
    if let (Some(a), Some(b)) = (start.res, process_resources()) {
        eprintln!(
            "  recursos: hilos {} → {} · archivos abiertos {} → {} · procesos hijos {} → {} (zombis {} → {})",
            a.threads, b.threads, a.files, b.files, a.children, b.children, a.zombies, b.zombies
        );
    }
}

/// A hidden window that nobody looks at can stop being woken by macOS (App Nap): no frame comes,
/// and a measurement that ends from a frame never ends. This thread looks every moment whether
/// the frames are late and then measures and closes by itself. It does nothing while they come.
fn spawn_watchdog(ctx: eframe::egui::Context, shared: Arc<IdleShared>, seconds: f64) {
    let spawned = Instant::now();
    let _ = std::thread::Builder::new()
        .name("reposo".into())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(250));
            match watch_once(&shared, spawned, seconds, Instant::now()) {
                Watch::Done => return,
                Watch::Wait | Watch::Started => {}
                Watch::Finish(secs) => {
                    let start = *shared.start.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(start) = start {
                        print_idle_result(&start, secs);
                    }
                    ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
                    ctx.request_repaint();
                    // The close is also a thing a window that sleeps does not get to.
                    std::thread::sleep(IDLE_CLOSE_WAIT);
                    std::process::exit(0);
                }
            }
        });
}

impl IdleProbe {
    pub fn new(seconds: f64) -> Self {
        Self {
            seconds: seconds.max(1.0),
            shared: Arc::default(),
            watching: false,
        }
    }

    /// Called once per frame; wakes the window only twice (start and end of the measurement).
    pub fn step(&mut self, ctx: &eframe::egui::Context) {
        if self.shared.reported.load(Ordering::SeqCst) {
            return;
        }
        if !self.watching {
            self.watching = true;
            spawn_watchdog(ctx.clone(), self.shared.clone(), self.seconds);
        }
        let now = ctx.input(|i| i.time);
        let end = IDLE_WARM_UP + self.seconds;
        let wake = |secs: f64| {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(secs.max(0.0)));
        };
        let start = *self.shared.start.lock().unwrap_or_else(|e| e.into_inner());
        match start {
            None if now >= IDLE_WARM_UP => {
                let begun = IdleStart::now(Some(now));
                let mut slot = self.shared.start.lock().unwrap_or_else(|e| e.into_inner());
                slot.get_or_insert(begun);
                wake(end - now);
            }
            None => wake(IDLE_WARM_UP - now),
            Some(s) => {
                // A measurement the watchdog began has no window clock to count from.
                let (secs, left) = match s.t0 {
                    Some(t0) => (now - t0, end - now),
                    None => {
                        let secs = s.at.elapsed().as_secs_f64();
                        (secs, self.seconds - secs)
                    }
                };
                if left > 0.0 {
                    wake(left);
                    return;
                }
                if self.shared.reported.swap(true, Ordering::SeqCst) {
                    return;
                }
                print_idle_result(&s, secs);
                ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn children_and_zombies_are_counted_by_parent() {
        let ps = "  1 Ss\n 77 S\n 77 Z\n 77 Z+\n 78 Z\n    \nx y\n";
        assert_eq!(count_children(ps, 77), (3, 2));
        assert_eq!(count_children(ps, 1), (1, 0));
        assert_eq!(count_children(ps, 99), (0, 0));
    }

    /// The app can read its own resources (the numbers themselves vary).
    #[test]
    fn the_app_reads_its_own_resources() {
        let r = process_resources().expect("ps and /dev/fd answer");
        assert!(r.threads >= 1 && r.files >= 3, "{r:?}");
    }

    #[test]
    fn processor_time_is_read_in_seconds() {
        assert_eq!(cpu_seconds("0:00.25"), Some(0.25));
        assert_eq!(cpu_seconds("1:02.50"), Some(62.5));
        assert_eq!(cpu_seconds("1:01:02.50"), Some(3662.5));
        assert_eq!(cpu_seconds("x"), None);
    }

    /// The result line keeps its wording and numbers.
    #[test]
    fn the_idle_result_line_reads_as_before() {
        assert_eq!(
            idle_line(20.0, 7, 0.5, 51_200),
            "reposo 20.0 s: 7 cuadros (0.35 por segundo) · procesador 0.50 s (2.50 %) · memoria 50.0 MB"
        );
    }

    /// While frames come the watchdog does nothing at all, however late it looks: the
    /// measurement is theirs.
    #[test]
    fn the_watchdog_leaves_a_run_that_still_draws_alone() {
        let shared = IdleShared::default();
        let t = Instant::now();
        let at = |s: f64| t + std::time::Duration::from_secs_f64(s);
        // A frame began the measurement at 5.2 s and will end it at 25 s.
        *shared.start.lock().unwrap() = Some(IdleStart {
            t0: Some(5.2),
            at: at(5.2),
            frames: 0,
            cpu: 0.0,
            res: None,
        });
        for s in [1.0, 5.0, 8.0, 15.0, 20.0, 25.0, 28.0] {
            assert_eq!(watch_once(&shared, t, 20.0, at(s)), Watch::Wait, "{s}");
        }
        // The frame ended it: nothing is left for the watchdog.
        shared.reported.store(true, Ordering::SeqCst);
        assert_eq!(watch_once(&shared, t, 20.0, at(60.0)), Watch::Done);
    }

    /// With no frame at all (the hidden window is not woken any more) the watchdog begins the
    /// measurement late, ends it on time without a frame, and does so only once.
    #[test]
    fn the_watchdog_measures_and_ends_a_run_that_gets_no_frames() {
        let shared = IdleShared::default();
        let t = Instant::now();
        let at = |s: f64| t + std::time::Duration::from_secs_f64(s);
        // The warm-up and the grace pass with no frame.
        assert_eq!(watch_once(&shared, t, 600.0, at(1.0)), Watch::Wait);
        assert_eq!(watch_once(&shared, t, 600.0, at(7.9)), Watch::Wait);
        assert!(shared.start.lock().unwrap().is_none());
        assert_eq!(watch_once(&shared, t, 600.0, at(8.1)), Watch::Started);
        let started = shared.start.lock().unwrap().unwrap();
        assert!(started.t0.is_none());
        // The measurement runs its length plus the grace, counted from when it began.
        let begun = started.at;
        let after = |s: f64| begun + std::time::Duration::from_secs_f64(s);
        assert_eq!(watch_once(&shared, t, 600.0, after(300.0)), Watch::Wait);
        assert_eq!(watch_once(&shared, t, 600.0, after(602.9)), Watch::Wait);
        match watch_once(&shared, t, 600.0, after(603.1)) {
            Watch::Finish(secs) => assert!((secs - 603.1).abs() < 1e-6, "{secs}"),
            other => panic!("{other:?}"),
        }
        // Reported once: a second look, or a frame that comes back late, finds it done.
        assert_eq!(watch_once(&shared, t, 600.0, after(700.0)), Watch::Done);
        assert!(shared.reported.load(Ordering::SeqCst));
    }
}
