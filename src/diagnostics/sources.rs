//! Reading what programs wrote besides Wine's own output: Steam's web helper and Chromium logs,
//! the Rockstar Games Launcher log, and the Windows user names of a bottle.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::redact::redact;
use super::rules::rules_hit;
use super::state::{Kept, KeptStore};

/// The lines of a bottle's own program logs (the ones that explain failures).
#[derive(Clone, Debug, Default)]
pub struct ProgramLogs {
    /// Steam's web helper and Chromium logs, last 200 lines each.
    pub steam: Vec<String>,
    /// The whole text of each Rockstar Games Launcher log (one per real file).
    pub rockstar: Vec<String>,
}

/// Whether a file was written at or after `since` (a file whose age cannot be read is kept).
fn written_since(path: &Path, since: Option<SystemTime>) -> bool {
    match since {
        None => true,
        Some(t) => std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|m| m >= t)
            .unwrap_or(true),
    }
}

/// Lines worth reading from a bottle besides what Wine printed: the programs' own logs that are
/// known to explain failures (Steam's web helper and Chromium logs, last 200 lines each, and the
/// Rockstar Games Launcher log). Files not written since `since` (the bottle's latest start) belong
/// to an older run and are skipped.
pub fn read_program_logs(prefix: &Path, since: Option<SystemTime>) -> ProgramLogs {
    let mut logs = ProgramLogs::default();
    for rel in [
        "drive_c/Program Files (x86)/Steam/logs/webhelper.txt",
        "drive_c/Program Files (x86)/Steam/logs/cef_log.txt",
    ] {
        let path = prefix.join(rel);
        if !written_since(&path, since) {
            continue;
        }
        if let Ok(text) = std::fs::read(&path) {
            let text = String::from_utf8_lossy(&text);
            let lines: Vec<&str> = text.lines().collect();
            logs.steam.extend(
                lines[lines.len().saturating_sub(200)..]
                    .iter()
                    .map(|l| l.to_string()),
            );
        }
    }
    for log in rockstar_launcher_logs(prefix) {
        if written_since(&log, since) {
            logs.rockstar.push(
                String::from_utf8_lossy(&std::fs::read(&log).unwrap_or_default()).to_string(),
            );
        }
    }
    logs
}

/// Where the Rockstar Games Launcher writes its own log: the Windows user's Documents folder
/// (a bottle may link that folder to the Mac's Documents, and a linked user folder shows the
/// same file twice).
fn rockstar_launcher_logs(prefix: &Path) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut out = vec![];
    if let Ok(users) = std::fs::read_dir(prefix.join("drive_c/users")) {
        for u in users.flatten() {
            let log = u
                .path()
                .join("Documents/Rockstar Games/Launcher/launcher.log");
            if let Ok(real) = std::fs::canonicalize(&log) {
                if seen.insert(real.clone()) {
                    out.push(real);
                }
            }
        }
    }
    out
}

/// The names of the Windows users of a bottle that identify a person: every user folder except
/// the ones Windows always has.
pub fn wine_user_names(prefix: &Path) -> Vec<String> {
    let Ok(users) = std::fs::read_dir(prefix.join("drive_c/users")) else {
        return vec![];
    };
    users
        .flatten()
        .map(|u| u.file_name().to_string_lossy().to_string())
        .filter(|n| {
            !matches!(
                n.to_lowercase().as_str(),
                "public" | "all users" | "default" | "default user" | "steamuser"
            )
        })
        .collect()
}

/// The launcher starts a new launcher.log on every run. When its interface cannot start, the
/// log reaches "[subsystem] Initializing group 1..." and the next thing the launcher does is
/// stop its service: no further "[subsystem]" step ever appears. Returns that last line.
///
/// RockstarService's "Last error: 183 (File already exists)" is deliberately not read here: it
/// is written while the service stops, after the launcher already gave up.
pub fn rockstar_ui_stalled(launcher_log: &str) -> Option<String> {
    let lines: Vec<&str> = launcher_log
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    let group1 = lines
        .iter()
        .rposition(|l| l.contains("[subsystem] Initializing group 1"))?;
    let after = &lines[group1 + 1..];
    let last = after.last()?;
    let gave_up = last.contains("[svc] Stopping the background service");
    let went_on = after.iter().any(|l| l.contains("[subsystem]"));
    (gave_up && !went_on).then(|| last.trim().to_string())
}

/// How the line that follows a start marker ends when the program was opened from the Finder
/// with no window of the app (`▶ <file> (abierto desde el Finder)`).
pub const FROM_FINDER: &str = "(abierto desde el Finder)";

/// How the activity log line that explains a disk failure starts (the technical detail the app
/// writes beside the plain sentence it shows). It describes the app's own file operation, not
/// what a program printed, so the rules never match on it.
pub const DISK_DETAIL_PREFIX: &str = "✗ Disco: ";

/// How much of the end of the log file is read to find a start with no window: a bound, so a
/// log of tens of megabytes is never read whole.
const LOG_TAIL_BYTES: u64 = 4 * 1024 * 1024;

/// How many lines of such a run are kept as its window (the same as the window of the app).
const RUN_LINES: usize = 400;

/// What a program opened from the Finder with no window printed, read back from the log file.
#[derive(Clone, Debug, Default)]
pub struct WindowlessRun {
    /// The last lines of the run, without its marker.
    pub ring: Vec<String>,
    /// What the rules matched anywhere in the run (kept apart, like the window's own matches).
    pub kept: Vec<Kept>,
}

/// The lines of a bottle's latest start when it was a program opened from the Finder with no
/// window. That process writes its output to the log file and ends, so the window of the app
/// never holds those lines. `marker_bottle` tells which bottle a start marker names. Only the
/// tail of the file is read; the answer is `None` when the bottle's latest start in it is not
/// such a run (a later start from the window has its lines in the window), when its marker is not
/// in the tail, or when the file cannot be read. The matches are redacted as they are kept,
/// hiding `mac_user` (the name of this Mac's user) in paths.
pub fn read_windowless_run(
    file: &Path,
    bottle: &str,
    marker_bottle: fn(&str) -> Option<&str>,
    mac_user: &str,
) -> Option<WindowlessRun> {
    let mut f = std::fs::File::open(file).ok()?;
    let len = f.metadata().ok()?.len();
    let start = len.saturating_sub(LOG_TAIL_BYTES);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    f.take(LOG_TAIL_BYTES).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut all = text.lines();
    if start > 0 {
        // The tail begins in the middle of a line.
        all.next();
    }
    let all: Vec<&str> = all.collect();
    let from = all.iter().rposition(|l| marker_bottle(l) == Some(bottle))? + 1;
    let end = all[from..]
        .iter()
        .position(|l| marker_bottle(l).is_some())
        .map_or(all.len(), |i| from + i);
    let run = &all[from..end];
    let first = run.iter().find(|l| !l.trim().is_empty())?;
    if !(first.starts_with("▶ ") && first.trim_end().ends_with(FROM_FINDER)) {
        return None;
    }
    let mut store = KeptStore::new();
    for line in run {
        if line.starts_with(DISK_DETAIL_PREFIX) {
            continue;
        }
        for rule in rules_hit(line) {
            store.add(Kept {
                rule,
                line: redact(mac_user, line.trim()),
                bottle: bottle.to_string(),
                run: 0,
                count: 1,
            });
        }
    }
    Some(WindowlessRun {
        ring: run[run.len().saturating_sub(RUN_LINES)..]
            .iter()
            .map(|l| l.to_string())
            .collect(),
        kept: store.for_run(bottle, 0),
    })
}
