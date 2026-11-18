//! The log the window shows and the log file that keeps everything the programs print.

use super::*;
use crate::diagnostics;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub type Log = Arc<Mutex<Vec<String>>>;

/// Everything the app, Wine and the programs print is also kept in this file, so a program that
/// crashed can be looked at after its window is gone (the window keeps only the last lines).
pub fn log_file() -> PathBuf {
    data_dir().join("registro.txt")
}

/// The log file grows up to this size; then it becomes `registro.anterior.txt` (one older copy
/// is kept) and a new one starts.
const LOG_FILE_MAX: u64 = 32 * 1024 * 1024;

/// A program that the app starts without a window writes straight into the log file, so the
/// size limit above cannot reach what it prints. It only does so while the file is smaller than
/// this, which leaves room for it to grow; a log that is already large gets none of that
/// program's output, so a chatty one cannot fill the disk.
pub const LOG_DIRECT_OUTPUT_MAX: u64 = LOG_FILE_MAX / 4;

/// The open log file: kept open between lines, because Wine can print thousands of lines a
/// second and opening, writing and closing the file for each of them is most of the cost.
#[derive(Default)]
struct LogWriter {
    /// The path the open file belongs to.
    path: PathBuf,
    file: Option<fs::File>,
    /// Bytes in the file so far, to know when it is time to roll over.
    written: u64,
    /// Lines written since the path was last looked at.
    since_check: u32,
}

/// Every this many lines the writer looks whether its file is still where it was, so a log
/// that was deleted or moved while the app runs comes back instead of being written into a
/// file nobody can see.
const CHECK_EVERY: u32 = 64;

impl LogWriter {
    fn open(&mut self, path: &Path) {
        self.file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok();
        self.written = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        self.path = path.to_path_buf();
    }

    /// Appends one line to `path`, with the time (UTC) in front, keeping it under `max` bytes:
    /// past that, the file becomes `registro.anterior.txt` (one older copy is kept) and a new
    /// one starts. The line goes out in one unbuffered write, so nothing is lost if the app dies.
    fn write_line(&mut self, path: &Path, line: &str, max: u64) {
        use std::io::Write;
        self.since_check += 1;
        if self.since_check >= CHECK_EVERY {
            self.since_check = 0;
            if !path.exists() {
                self.file = None;
            }
        }
        if self.file.is_none() || self.path != path {
            self.open(path);
        }
        if self.written > max {
            self.file = None;
            let _ = fs::rename(path, path.with_file_name("registro.anterior.txt"));
            self.open(path);
        }
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() % 86_400)
            .unwrap_or(0);
        let text = format!(
            "{:02}:{:02}:{:02}Z {line}\n",
            secs / 3600,
            secs / 60 % 60,
            secs % 60
        );
        if let Some(f) = &mut self.file {
            if f.write_all(text.as_bytes()).is_ok() {
                self.written += text.len() as u64;
            } else {
                // The file went away or the disk refused: open it again on the next line.
                self.file = None;
            }
        }
    }
}

/// Writes `lines` lines the way the log is written, to time it (`--medir`).
pub fn log_bench(file: &Path, lines: usize) {
    let mut writer = LogWriter::default();
    for i in 0..lines {
        writer.write_line(
            file,
            &format!("0024:fixme:d3d:wined3d_swapchain_present línea de prueba número {i} con el largo de una línea corriente"),
            LOG_FILE_MAX,
        );
    }
}

/// How many lines the window keeps.
const WINDOW_LINES: usize = 400;

/// The app's own log file, open for the whole run.
static LOG_WRITER: Mutex<LogWriter> = Mutex::new(LogWriter {
    path: PathBuf::new(),
    file: None,
    written: 0,
    since_check: 0,
});

// ---------------------------------------------------------------------------------------
// Runs: which bottle the lines belong to
// ---------------------------------------------------------------------------------------

/// Which start the lines now arriving belong to. Wine's output carries no bottle name, so the
/// app writes a marker line when it starts something in a bottle and everything after it is
/// taken to be that bottle's. Limits: two bottles running at the same time share one stream, so
/// lines of the first are read as the second's after the second one starts; and the output of
/// the app's own tools (duplicating a bottle, changing its Windows version, closing programs…)
/// is not a start, so it is neither kept as decisive lines nor read from the window (see
/// [`push_tool_log`]); an identical line a program prints is also left out of the window then,
/// which only matters for lines that are repeated (the kept lines still have it).
struct Runs {
    counter: u64,
    /// The bottle (by name) and run of the latest start.
    current: Option<(String, u64)>,
    /// The latest run of each bottle.
    latest: BTreeMap<String, u64>,
}

static RUNS: Mutex<Runs> = Mutex::new(Runs {
    counter: 0,
    current: None,
    latest: BTreeMap::new(),
});

/// The latest lines the app's own tools printed, so the diagnosis can leave them out of what it
/// reads from the window (the window itself keeps them for the person to see).
static TOOL_LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// How many tool lines are remembered.
const TOOL_LINES_KEPT: usize = 400;

/// Matches of the diagnostic rules, kept apart from the window.
static KEPT: Mutex<diagnostics::KeptStore> = Mutex::new(diagnostics::KeptStore::new());

const MARKER_START: &str = "◆ Nuevo intento en la botella «";

/// The line that marks the start of a run. It names the bottle, never its folder.
fn run_marker(bottle_name: &str) -> String {
    format!("{MARKER_START}{bottle_name}»")
}

/// The bottle a marker line names (None for any other line).
pub(crate) fn marker_bottle(line: &str) -> Option<&str> {
    line.strip_prefix(MARKER_START)?.strip_suffix('»')
}

/// Notes that something starts in a bottle (a program, a tool, an engine change): its
/// diagnosis then looks only at what is printed from here on.
pub fn note_run_start(log: &Log, bottle_name: &str) {
    {
        let mut runs = RUNS.lock().unwrap_or_else(|e| e.into_inner());
        runs.counter += 1;
        let id = runs.counter;
        runs.current = Some((bottle_name.to_string(), id));
        runs.latest.insert(bottle_name.to_string(), id);
    }
    push_log(log, run_marker(bottle_name));
}

/// The run that started last in this bottle, if the app started anything in it.
pub fn latest_run(bottle_name: &str) -> Option<u64> {
    RUNS.lock()
        .unwrap_or_else(|e| e.into_inner())
        .latest
        .get(bottle_name)
        .copied()
}

/// The lines printed since the bottle's latest start (without the marker), up to the next start
/// of anything else. When the marker already scrolled out of the window, the lines before the
/// first marker still in the window are the tail of that run (all of the window if it holds no
/// marker). No start, no lines.
pub fn lines_since_run(window: &[String], bottle_name: &str) -> Vec<String> {
    if latest_run(bottle_name).is_none() {
        return vec![];
    }
    let from = window
        .iter()
        .rposition(|l| marker_bottle(l) == Some(bottle_name))
        .map_or(0, |i| i + 1);
    let end = window[from..]
        .iter()
        .position(|l| marker_bottle(l).is_some())
        .map_or(window.len(), |i| from + i);
    let tools = TOOL_LINES.lock().unwrap_or_else(|e| e.into_inner());
    window[from..end]
        .iter()
        .filter(|l| !tools.contains(l))
        .cloned()
        .collect()
}

/// What the rules matched in the bottle's latest run, including lines that left the window.
pub fn kept_for_bottle(bottle_name: &str) -> Vec<diagnostics::Kept> {
    match latest_run(bottle_name) {
        Some(run) => KEPT
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .for_run(bottle_name, run),
        None => vec![],
    }
}

/// Runs the diagnostic rules on a line as it arrives and keeps what matches.
fn keep_matches(line: &str) {
    let hits = diagnostics::rules_hit(line);
    if hits.is_empty() {
        return;
    }
    let Some((bottle, run)) = RUNS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .current
        .clone()
    else {
        return;
    };
    let shown = diagnostics::redact(mac_user_once(), line.trim());
    let mut store = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    for rule in hits {
        store.add(diagnostics::Kept {
            rule,
            line: shown.clone(),
            bottle: bottle.clone(),
            run,
            count: 1,
        });
    }
}

pub fn push_log(log: &Log, line: impl Into<String>) {
    push_line(log, line.into(), true);
}

/// Like [`push_log`] for what one of the app's own tools prints (a registry change, a copy of a
/// bottle, closing programs). That output is not about the bottle that started last, so the
/// diagnostic rules do not keep it.
pub fn push_tool_log(log: &Log, line: impl Into<String>) {
    push_line(log, line.into(), false);
}

/// Writes `line` to the terminal the app was started from, if there is one. `eprintln!` panics
/// when the terminal is gone (a closed window, a broken pipe), and a panic in the thread that
/// reads a program's output ends the reading: the pipe then fills up and the program blocks on
/// its next line. A line that cannot be shown is simply not shown.
fn echo_to(out: &mut impl std::io::Write, line: &str) {
    let _ = writeln!(out, "{line}");
}

/// The log file's writer. A thread that panicked while writing leaves the lock poisoned; the
/// writer is still usable (it reopens its file on a failed write), so the lock is taken anyway
/// instead of panicking every thread that logs afterwards.
fn log_writer() -> std::sync::MutexGuard<'static, LogWriter> {
    LOG_WRITER.lock().unwrap_or_else(|e| e.into_inner())
}

fn push_line(log: &Log, line: String, attributed: bool) {
    // Also echo to the terminal so `cargo run` shows everything Wine and the programs print.
    echo_to(&mut std::io::stderr(), &line);
    if !cfg!(test) {
        log_writer().write_line(&log_file(), &line, LOG_FILE_MAX);
    }
    if attributed {
        keep_matches(&line);
    } else {
        let mut tools = TOOL_LINES.lock().unwrap_or_else(|e| e.into_inner());
        tools.push(line.clone());
        let n = tools.len();
        if n > TOOL_LINES_KEPT {
            tools.drain(0..n - TOOL_LINES_KEPT);
        }
    }
    let mut l = log.lock().unwrap_or_else(|e| e.into_inner());
    l.push(line);
    let n = l.len();
    if n > WINDOW_LINES {
        l.drain(0..n - WINDOW_LINES);
    }
}

#[cfg(test)]
mod log_file_tests {
    use super::*;

    /// A stand-in for a terminal that is gone: every write fails.
    struct GoneTerminal;

    impl std::io::Write for GoneTerminal {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
    }

    /// Showing a line on a terminal that is gone must not panic: the thread that reads a
    /// program's output logs through it, and if it died the program would block on a full pipe.
    #[test]
    fn a_terminal_that_is_gone_does_not_stop_the_log() {
        echo_to(&mut GoneTerminal, "err:module:load_dll fallo");
    }

    /// One thread that panicked while holding the log file's lock must not make every later
    /// line panic as well.
    #[test]
    fn a_poisoned_log_lock_does_not_stop_the_log() {
        let _ = std::thread::spawn(|| {
            let _held = LOG_WRITER.lock().unwrap();
            panic!("a thread died while writing the log");
        })
        .join();
        assert!(LOG_WRITER.is_poisoned());
        drop(log_writer());
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bl-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn log_lines_are_kept_in_a_file_that_rolls_over() {
        let dir = temp("log");
        let file = dir.join("registro.txt");
        let mut w = LogWriter::default();
        w.write_line(&file, "err:module:load_dll fallo", 100);
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.ends_with(" err:module:load_dll fallo\n"), "{text}");
        assert_eq!(text.find(':'), Some(2), "{text}");
        // Written at once, not held in a buffer: another reader sees it already.
        assert_eq!(w.written, text.len() as u64);
        // Past the limit, the file becomes the older copy and a new one starts.
        w.write_line(&file, &"x".repeat(120), 100);
        w.write_line(&file, "nueva", 100);
        let now = fs::read_to_string(&file).unwrap();
        assert!(now.ends_with(" nueva\n") && !now.contains("fallo"), "{now}");
        let old = fs::read_to_string(dir.join("registro.anterior.txt")).unwrap();
        assert!(old.contains("fallo") && old.contains("xxx"));
        let _ = fs::remove_dir_all(&dir);
    }

    /// The writer keeps one file open, so it must follow when the path changes or the file is
    /// removed, and must continue counting when it opens a file that already has text.
    #[test]
    fn writer_follows_the_path_and_counts_existing_bytes() {
        let dir = temp("log-path");
        let (a, b) = (dir.join("a.txt"), dir.join("b.txt"));
        fs::write(&a, "x".repeat(50)).unwrap();
        let mut w = LogWriter::default();
        w.write_line(&a, "uno", 10_000);
        assert!(w.written > 50, "the existing bytes count: {}", w.written);
        w.write_line(&b, "dos", 10_000);
        assert!(fs::read_to_string(&b).unwrap().ends_with(" dos\n"));
        assert!(fs::read_to_string(&a).unwrap().ends_with(" uno\n"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn writer_comes_back_when_the_file_is_deleted() {
        let dir = temp("log-deleted");
        let file = dir.join("registro.txt");
        let mut w = LogWriter::default();
        w.write_line(&file, "antes", 10_000);
        fs::remove_file(&file).unwrap();
        for i in 0..(CHECK_EVERY * 2) {
            w.write_line(&file, &format!("despues {i}"), 10_000);
        }
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains("despues"), "{text}");
        assert!(!text.contains("antes"), "{text}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bench_writes_every_line() {
        let dir = temp("log-bench");
        let file = dir.join("registro.txt");
        log_bench(&file, 500);
        assert_eq!(fs::read_to_string(&file).unwrap().lines().count(), 500);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn marker_names_the_bottle_and_never_a_folder() {
        let m = run_marker("Juegos");
        assert_eq!(marker_bottle(&m), Some("Juegos"));
        assert_eq!(marker_bottle("▶ Juegos"), None);
        assert!(!m.contains('/'));
    }

    fn window(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| s.to_string()).collect()
    }

    /// Only one test starts runs: they share the program-wide state.
    #[test]
    fn diagnosis_sees_only_the_latest_run_of_its_bottle() {
        let log: Log = Default::default();
        let (a, b) = ("bl-test-uno", "bl-test-dos");
        assert!(
            lines_since_run(&window(&["x"]), a).is_empty(),
            "never started"
        );
        assert!(kept_for_bottle(a).is_empty());

        note_run_start(&log, a);
        push_log(&log, "0024:fixme:winediag:wined3d_select_feature_level None of the requested D3D feature levels is supported");
        push_log(&log, "otra línea");
        // The decisive line is kept even when the window scrolls past it.
        for i in 0..(WINDOW_LINES + 50) {
            push_log(&log, format!("ruido {i}"));
        }
        let ring = log.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert_eq!(ring.len(), WINDOW_LINES);
        assert!(!ring.iter().any(|l| l.contains("wined3d_select")));
        let kept = kept_for_bottle(a);
        assert_eq!(kept.len(), 1);
        assert!(kept[0].line.contains("wined3d_select_feature_level"));
        assert_eq!(kept[0].bottle, a);

        // A new start of the same bottle drops the older run.
        note_run_start(&log, a);
        push_log(&log, "0024:err:module:import_dll Library A.dll not found");
        let ring = log.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let seen = lines_since_run(&ring, a);
        assert_eq!(
            seen,
            vec!["0024:err:module:import_dll Library A.dll not found"]
        );
        let kept = kept_for_bottle(a);
        assert_eq!(kept.len(), 1);
        assert!(kept[0].line.contains("import_dll"));

        // Another bottle has its own run.
        assert!(kept_for_bottle(b).is_empty());
        note_run_start(&log, b);
        push_log(&log, "err:module:load_dll Library B.dll failed to load");
        assert_eq!(kept_for_bottle(b).len(), 1);
        // What one of the app's own tools prints is not decisive for the bottle that started
        // last.
        push_tool_log(&log, "err:module:load_dll Library T.dll failed to load");
        push_tool_log(&log, "permission denied");
        assert_eq!(kept_for_bottle(b).len(), 1);
        assert_eq!(kept_for_bottle(a).len(), 1);
        let ring = log.lock().unwrap_or_else(|e| e.into_inner()).clone();
        assert_eq!(
            lines_since_run(&ring, b).len(),
            1,
            "tool lines are not the bottle's"
        );

        // The marker may have scrolled out of the window: then all of the window is after it.
        let c = "bl-test-tres";
        note_run_start(&log, c);
        let mut w: Vec<String> = (0..10).map(|i| format!("linea {i}")).collect();
        assert_eq!(lines_since_run(&w, c).len(), 10);
        w.insert(3, run_marker(c));
        assert_eq!(lines_since_run(&w, c).len(), 7);

        // Bottles started one after the other: each one sees only its own lines.
        let (d, e) = ("bl-test-cuatro", "bl-test-cinco");
        note_run_start(&log, d);
        push_log(&log, "linea de d");
        note_run_start(&log, e);
        push_log(&log, "linea de e");
        let ring = log.lock().unwrap_or_else(|x| x.into_inner()).clone();
        assert_eq!(lines_since_run(&ring, d), vec!["linea de d"]);
        assert_eq!(lines_since_run(&ring, e), vec!["linea de e"]);
        // With the first marker scrolled out, the lines before the next marker are still d's.
        let cut: Vec<String> = ring
            .iter()
            .skip_while(|l| marker_bottle(l) != Some(d))
            .skip(1)
            .cloned()
            .collect();
        assert_eq!(lines_since_run(&cut, d), vec!["linea de d"]);
        assert_eq!(lines_since_run(&cut, e), vec!["linea de e"]);
    }
}
