//! Fetching a package: curl run with its progress, the retries and the stall limit, and what
//! arrived checked against its published size and SHA-256.

use super::*;
use crate::backend::{push_log, Log};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// The folder of the app's own data where packages are downloaded to.
pub(super) const DOWNLOADS: &str = "descarga";

/// What a download or system install answers under `cargo test`, where nothing may leave the machine.
pub(super) const NO_NETWORK_IN_TESTS: &str = "Las pruebas no descargan ni instalan nada.";

/// A download that gets fewer bytes a second than this for [`STALL_SECS`] seconds has stalled.
const STALL_BYTES_PER_SEC: &str = "1024";

/// How long a download may crawl or stand still before it is given up. A dead connection (a
/// dropped Wi-Fi, a server that stopped answering) never makes curl fail by itself, so without
/// this the spinner would stay until the person cancelled.
const STALL_SECS: u32 = 60;

/// curl's exit code for "operation timed out", which is also how a stalled transfer ends.
const CURL_TIMED_OUT: i32 = 28;

/// What the person is told when a download did not end well. A stall gets its own text, because
/// the advice differs: the connection is not just flaky, it stopped.
fn download_failure_text(curl_exit: Option<i32>) -> String {
    match curl_exit {
        Some(CURL_TIMED_OUT) => {
            "La descarga se quedó parada: no llegaban datos. Revisa tu conexión e inténtalo otra vez."
                .into()
        }
        _ => "La descarga no terminó bien. Revisa tu conexión e inténtalo otra vez.".into(),
    }
}

/// Downloads one file with curl, showing its progress. `before` is what earlier files of the
/// same install already downloaded.
pub(super) fn download(
    p: &Shared,
    url: &str,
    file: &Path,
    size: u64,
    before: u64,
) -> Result<(), Stopped> {
    // Tests never reach the network: every engine, layer and video download goes through here.
    if cfg!(test) {
        return Err(Stopped::Failed(NO_NETWORK_IN_TESTS.into()));
    }
    download_with(&CHILDREN, p, url, file, size, before, STALL_SECS)
}

/// How many times a download is started at most: the first try and three more, as curl's own
/// `--retry 3` did.
const DOWNLOAD_TRIES: u32 = 4;

/// How many of those tries may end in a stall. A connection that stood still for a minute is not
/// worth three more minutes of the same, so after the second stall the download gives up.
const STALLS_ALLOWED: u32 = 2;

/// How one run of curl ended.
enum Ran {
    /// curl reported success.
    Done,
    /// curl failed, with this exit code when it had one.
    Failed(Option<i32>),
}

/// [`download()`] with the registry that holds curl: closing the window ends it there, whatever
/// the download is doing, and the half file goes with it. `stall_secs` is how long the transfer
/// may stand still before it fails.
///
/// The retries are made here and not by curl's `--retry`: curl counts its retry time from the very
/// first try, so a long download that dropped after a few minutes would not be retried at all,
/// and it retries a stall as often as any other error, which would keep the spinner up for
/// minutes. Here a dropped connection is retried and a stall is bounded.
fn download_with(
    children: &Children,
    p: &Shared,
    url: &str,
    file: &Path,
    size: u64,
    before: u64,
    stall_secs: u32,
) -> Result<(), Stopped> {
    let mut stalls = 0;
    let mut last_exit = None;
    let mut done = false;
    for attempt in 0..DOWNLOAD_TRIES {
        if attempt > 0 && last_exit != Some(CURL_TIMED_OUT) {
            // A short, growing pause, as curl made, which the Cancel button cuts short. After a
            // stall there is no pause: the person has already waited.
            let pause = Duration::from_secs(1 << (attempt - 1));
            let until = std::time::Instant::now() + pause;
            while std::time::Instant::now() < until {
                if cancelled(p) {
                    return Err(Stopped::Cancelled);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        match run_curl(children, p, url, file, before, stall_secs)? {
            Ran::Done => {
                done = true;
                break;
            }
            Ran::Failed(code) => {
                last_exit = code;
                if code == Some(CURL_TIMED_OUT) {
                    stalls += 1;
                    if stalls >= STALLS_ALLOWED {
                        break;
                    }
                }
            }
        }
    }
    let got = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    if !done || (size > 0 && got != size) {
        let _ = std::fs::remove_file(file);
        return Err(Stopped::Failed(download_failure_text(last_exit)));
    }
    Ok(())
}

/// One run of curl on `url`, with the progress shown while it goes. Cancelling or closing the
/// window ends it and removes the half file.
fn run_curl(
    children: &Children,
    p: &Shared,
    url: &str,
    file: &Path,
    before: u64,
    stall_secs: u32,
) -> Result<Ran, Stopped> {
    let _ = std::fs::remove_file(file);
    let mut curl = Command::new("/usr/bin/curl");
    curl.args([
        "-fL",
        "--connect-timeout",
        "20",
        "--speed-limit",
        STALL_BYTES_PER_SEC,
        "--speed-time",
    ])
    .arg(stall_secs.to_string())
    .args(["-sS", "-o"])
    .arg(file)
    .arg(url)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    let id = children
        .start(&mut curl)
        .map_err(|e| Stopped::Failed(e.to_string()))?;
    loop {
        if cancelled(p) {
            children.kill(id);
            let _ = std::fs::remove_file(file);
            return Err(Stopped::Cancelled);
        }
        let got = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
        p.lock().unwrap_or_else(|e| e.into_inner()).downloaded = before + got;
        match children.poll(id) {
            Poll::Exited(Ok(s)) if s.success() => return Ok(Ran::Done),
            Poll::Exited(Ok(s)) => return Ok(Ran::Failed(s.code())),
            Poll::Exited(Err(_)) => return Ok(Ran::Failed(None)),
            Poll::Running => std::thread::sleep(Duration::from_millis(300)),
            Poll::Gone => {
                // The app is closing and ended curl.
                let _ = std::fs::remove_file(file);
                return Err(Stopped::Cancelled);
            }
        }
    }
}

/// The SHA-256 of a file, as lower-case hex.
pub fn sha256_of(file: &Path) -> Option<String> {
    let out = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(file)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let sum = text.split_whitespace().next()?.to_lowercase();
    (out.status.success() && sum.len() == 64).then_some(sum)
}

/// Downloads one pinned package into the folder `into` and checks it against its SHA-256: the
/// file is returned when it is the published one. `before` is what earlier packages of the same
/// install already downloaded. Any other end (cancelled, failed, not the published file) is the
/// state the install stops with, already recorded in the shared progress.
pub(super) fn download_pinned(
    p: &Shared,
    log: &Log,
    what: &Pinned,
    into: &Path,
    before: u64,
) -> Result<PathBuf, WineState> {
    let file = into.join(what.name);
    push_log(log, format!("▶ Descargando {}", what.name));
    match download(p, what.url, &file, what.size, before) {
        Ok(()) => {}
        Err(Stopped::Cancelled) => return Err(install_cancelled(p)),
        Err(Stopped::Failed(e)) => return Err(install_failed(p, log, e)),
    }
    if sha256_of(&file).as_deref() != Some(what.sha256) {
        let _ = std::fs::remove_file(&file);
        push_log(
            log,
            format!("✗ {}: la suma SHA-256 no es la publicada", what.name),
        );
        return Err(install_failed(
            p,
            log,
            "La descarga no es la que se publicó: no la instalo. Prueba otra vez más tarde.".into(),
        ));
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wine_setup::tests::support::{process_state, scratch, wait_for};
    use std::fs;
    use std::sync::Arc;

    /// Closing the window in the middle of a download ends curl with it: the download does not
    /// go on alone, and the worker is told it was stopped. The server accepts the connection
    /// and never answers, so the download cannot finish by itself.
    #[test]
    fn closing_the_app_ends_a_download_in_progress() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://127.0.0.1:{}/paquete.tar.xz",
            server.local_addr().unwrap().port()
        );
        let dir = scratch("download");
        let file = dir.join("paquete.tar.xz");
        let children = Arc::new(Children::new());
        let progress: Shared = Default::default();
        let worker = {
            let (children, progress, file) = (children.clone(), progress.clone(), file.clone());
            std::thread::spawn(move || {
                download_with(&children, &progress, &url, &file, 1000, 0, STALL_SECS)
            })
        };
        wait_for("curl to start", || !children.pids().is_empty());
        let curl = children.pids()[0];
        assert_ne!(process_state(curl), "", "curl runs while the download does");

        assert_eq!(children.stop(), 1);
        let result = worker.join().unwrap();
        assert!(matches!(result, Err(Stopped::Cancelled)));
        assert_eq!(process_state(curl), "", "curl must be gone and reaped");
        assert!(!file.exists(), "no half file is left");
        drop(server);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A download whose connection goes dead fails by itself, with its own message and no half
    /// file, instead of keeping the spinner until the person cancels. The server accepts the
    /// connection and never answers; the stall time is a second so the test stays short. Without
    /// the stall limit curl waits for ever, and the guard below cancels it so the test fails
    /// instead of hanging.
    #[test]
    fn a_stalled_download_fails_by_itself_and_can_be_tried_again() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://127.0.0.1:{}/paquete.tar.xz",
            server.local_addr().unwrap().port()
        );
        let dir = scratch("stall");
        let file = dir.join("paquete.tar.xz");
        let children = Arc::new(Children::new());
        let progress: Shared = Default::default();
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let (children, progress, file) = (children.clone(), progress.clone(), file.clone());
            std::thread::spawn(move || {
                let _ = tx.send(download_with(&children, &progress, &url, &file, 1000, 0, 1));
            });
        }
        let result = rx.recv_timeout(Duration::from_secs(15));
        if result.is_err() {
            progress.lock().unwrap().cancel = true;
            panic!("the stalled download never ended by itself");
        }
        match result.unwrap() {
            Err(Stopped::Failed(text)) => assert_eq!(text, download_failure_text(Some(28))),
            Ok(()) => panic!("a download that got nothing cannot succeed"),
            Err(Stopped::Cancelled) => panic!("the download was cancelled, not stalled"),
        }
        assert!(!file.exists(), "no half file is left");
        assert!(children.pids().is_empty(), "curl is gone");
        // The same progress can start another download: the failure left nothing blocking it.
        assert!(!progress.lock().unwrap().cancel);
        // It was tried once more and then given up, not retried for minutes: two connections.
        server.set_nonblocking(true).unwrap();
        let tries = std::iter::from_fn(|| server.accept().ok()).count();
        assert_eq!(tries, STALLS_ALLOWED as usize);
        drop(server);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A connection that drops is not a stall: the download starts again and finishes, as it did
    /// when curl made the retries. The first connection is closed with no answer, the second
    /// is served.
    #[test]
    fn a_dropped_connection_is_retried_and_the_download_finishes() {
        use std::io::{Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://127.0.0.1:{}/paquete.bin",
            server.local_addr().unwrap().port()
        );
        let helper = std::thread::spawn(move || {
            drop(server.accept().unwrap());
            let (mut conn, _) = server.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = conn.read(&mut request);
            let _ = conn.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
            );
        });
        let dir = scratch("retry");
        let file = dir.join("paquete.bin");
        let children = Children::new();
        let progress: Shared = Default::default();
        assert!(download_with(&children, &progress, &url, &file, 5, 0, STALL_SECS).is_ok());
        assert_eq!(fs::read(&file).unwrap(), b"hello");
        helper.join().unwrap();
        let _ = fs::remove_dir_all(&dir);
    }

    /// A stall and any other failure are told apart, and both say what to do next.
    #[test]
    fn the_stall_message_differs_from_the_general_one() {
        let stalled = download_failure_text(Some(28));
        let other = download_failure_text(Some(22));
        assert_ne!(stalled, other);
        assert_eq!(other, download_failure_text(None));
        for text in [stalled, other] {
            assert!(text.contains("inténtalo otra vez"));
        }
    }

    /// The Cancel button ends curl too, and nothing of it is left running.
    #[test]
    fn cancelling_ends_curl_and_removes_the_half_file() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://127.0.0.1:{}/paquete.tar.xz",
            server.local_addr().unwrap().port()
        );
        let dir = scratch("cancel");
        let file = dir.join("paquete.tar.xz");
        let children = Arc::new(Children::new());
        let progress: Shared = Default::default();
        let worker = {
            let (children, progress, file) = (children.clone(), progress.clone(), file.clone());
            std::thread::spawn(move || {
                download_with(&children, &progress, &url, &file, 1000, 0, STALL_SECS)
            })
        };
        wait_for("curl to start", || !children.pids().is_empty());
        let curl = children.pids()[0];
        progress.lock().unwrap().cancel = true;
        assert!(matches!(worker.join().unwrap(), Err(Stopped::Cancelled)));
        assert_eq!(process_state(curl), "");
        assert!(children.pids().is_empty());
        drop(server);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A download that works still works through the registry, and leaves nothing tracked.
    #[test]
    fn a_download_that_finishes_leaves_nothing_tracked() {
        let dir = scratch("finish");
        let source = dir.join("origen.bin");
        fs::write(&source, vec![7u8; 4096]).unwrap();
        let file = dir.join("copia.bin");
        let children = Children::new();
        let progress: Shared = Default::default();
        let url = format!("file://{}", source.display());
        assert!(download_with(&children, &progress, &url, &file, 4096, 100, STALL_SECS).is_ok());
        assert_eq!(fs::read(&file).unwrap().len(), 4096);
        assert!(children.pids().is_empty());
        // The wrong size is a failed download, and the file does not stay.
        assert!(matches!(
            download_with(&children, &progress, &url, &file, 5000, 0, STALL_SECS),
            Err(Stopped::Failed(_))
        ));
        assert!(!file.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// What a person reads when a download is not the published one says no hash: the
    /// comparison is in the log, the screen says what it means.
    #[test]
    fn a_download_that_is_not_the_published_one_is_told_without_jargon() {
        let code = include_str!("download.rs");
        let code = code.split("#[cfg(test)]").next().unwrap();
        assert!(!code.contains("suma SHA-256 distinta"));
        assert!(code.contains("La descarga no es la que se publicó"));
        // The engine installs say it in their own words, and without it either.
        let install = include_str!("install.rs");
        let install = install.split("#[cfg(test)]").next().unwrap();
        assert!(!install.contains("suma SHA-256 distinta"));
    }
}
