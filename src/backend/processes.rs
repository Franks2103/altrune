//! Starting, running, creating, resizing and stopping the programs and servers of a bottle.

use super::*;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

pub const HIGHRES_FACTOR: u32 = 2;

/// How long to wait for one bottle to shut down before giving up on it.
pub const QUIT_WAIT_SECS: u64 = 60;

/// Starts a command and copies its output into the log. Returns the running child.
pub fn spawn_logged(cmd: Command, log: &Log, label: &str) -> std::io::Result<Child> {
    spawn_logged_as(cmd, log, label, true)
}

/// [`spawn_logged`]; `program` is false for the app's own tools, whose output the diagnosis
/// must not take for the output of a program started in a bottle.
fn spawn_logged_as(
    mut cmd: Command,
    log: &Log,
    label: &str,
    program: bool,
) -> std::io::Result<Child> {
    let push = if program { push_log } else { push_tool_log };
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let mut child = cmd.spawn()?;
    push(log, format!("▶ {label}"));
    for stream in [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let log = log.clone();
        std::thread::spawn(move || read_lines_lossy(stream, |line| push(&log, line)));
    }
    Ok(child)
}

/// The longest line kept from a program's output. A program that prints without ever ending a
/// line (a stray binary dump, a runaway loop) must not grow the buffer, the log window or the
/// log file without limit; what is past this many bytes of a line is dropped.
const LINE_MAX: usize = 16 * 1024;

/// Reads a stream line by line and hands every non-blank line to `on_line`. Bytes that are not
/// valid UTF-8 are replaced instead of ending the loop, so one odd byte from a program cannot
/// stop its output from being read (and fill the pipe until the program blocks). A line longer
/// than [`LINE_MAX`] is cut there, marked with "…", and the rest of it is read and thrown away.
fn read_lines_lossy(stream: impl Read, mut on_line: impl FnMut(String)) {
    let mut reader = BufReader::new(stream);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match (&mut reader)
            .take(LINE_MAX as u64)
            .read_until(b'\n', &mut buf)
        {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let cut = buf.len() >= LINE_MAX && buf.last() != Some(&b'\n');
                let line = String::from_utf8_lossy(&buf);
                let line = line.trim_end_matches(['\n', '\r']);
                if !line.trim().is_empty() {
                    on_line(if cut {
                        format!("{line}…")
                    } else {
                        line.to_string()
                    });
                }
                if cut && reader.skip_until(b'\n').is_err() {
                    break;
                }
            }
        }
    }
}

/// Starts a command whose result nobody needs (opening a folder in the Finder) and has a
/// short-lived thread wait for it. A `Child` that is dropped without being waited for stays in
/// the process table as a zombie until the app quits, one for every click. The command gets no
/// input and no output pipes, so it holds nothing of the app open. Returns the process number
/// and the thread that waits for it.
pub fn spawn_reaped(mut cmd: Command) -> std::io::Result<(u32, std::thread::JoinHandle<()>)> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let pid = child.id();
    let done = std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok((pid, done))
}

/// Runs a command to the end; true when it succeeded.
pub fn run_logged(cmd: Command, log: &Log, label: &str) -> bool {
    match spawn_logged_as(cmd, log, label, false) {
        Ok(mut c) => c.wait().map(|s| s.success()).unwrap_or(false),
        Err(e) => {
            push_tool_log(log, format!("✗ {label}: {e}"));
            false
        }
    }
}

pub fn wineserver_for(wine: &Path) -> Option<PathBuf> {
    let p = wine.parent()?.join("wineserver");
    p.is_file().then_some(p)
}

/// The command of the Wine's `wineserver` for a prefix. Like every command that starts Wine, it
/// exists only for a prefix inside the app's own bottles: for any other there is none.
pub(super) fn wineserver_cmd(wine: &Path, prefix: &Path) -> Option<Command> {
    if !is_own_prefix(prefix) {
        return None;
    }
    let mut c = Command::new(wineserver_for(wine)?);
    c.env("WINEPREFIX", prefix);
    Some(c)
}

/// Builds the command that opens a program, shortcut or installer inside a bottle.
/// `env` holds `NAME=value` pairs, like the Run Command environment field.
pub fn open_command(wine: &Path, b: &Bottle, file: &Path, args: &str, env: &[String]) -> Command {
    let _ = bridge_user_profile(&b.path);
    let (mut vars, removed) = program_env(b, env);
    adapt_env_to_wine(wine, &mut vars);
    let mut c = wine_program(wine, &b.path);
    c.envs(vars).current_dir(&b.path);
    for r in removed {
        c.env_remove(r);
    }
    let (argv, dir) = open_args(&b.path, file, args);
    if let Some(d) = dir {
        c.current_dir(d);
    }
    c.args(argv);
    c
}

/// What the person is told when a bottle is asked for in a folder outside the app's bottles.
pub const NOT_AN_OWN_BOTTLE: &str =
    "Las botellas de Altrune solo se crean en su propia carpeta de botellas.";

/// What the person is told when Wine could not set a bottle up: its own words are English and go
/// to the log (every line it prints does), so the text only says where to look.
pub const BOTTLE_NOT_PREPARED: &str = "Wine no pudo preparar Windows en ella. Los detalles están en el registro de actividad (Ajustes).";

pub fn create_bottle(wine: &Path, path: &Path, windows: &str, log: &Log) -> Result<(), String> {
    if !is_own_prefix(path) {
        return Err(NOT_AN_OWN_BOTTLE.into());
    }
    fs::create_dir_all(path)
        .map_err(|e| io_error_logged(&e, log, "Crear la carpeta de la botella", path))?;
    if !run_logged(
        {
            let mut c = wine_cmd(wine, path, true);
            c.args(["wineboot", "--init"]);
            c
        },
        log,
        "Preparando Windows…",
    ) {
        return Err(BOTTLE_NOT_PREPARED.into());
    }
    let mut c = wine_cmd(wine, path, true);
    c.args(["winecfg", "/v", windows]);
    run_logged(
        c,
        log,
        &format!("Versión de Windows: {}", windows_label(windows)),
    );
    // Drive Y: is the person's home folder.
    let y = path.join("dosdevices/y:");
    let _ = fs::remove_file(&y);
    #[cfg(unix)]
    let _ = std::os::unix::fs::symlink(home(), &y);
    if let Some(mut c) = wineserver_cmd(wine, path) {
        c.arg("-w");
        run_logged(c, log, "Terminando de guardar…");
    }
    Ok(())
}

pub fn set_high_res(wine: &Path, b: &Bottle, on: bool, log: &Log) -> bool {
    let cur = b.logpixels.max(1);
    let new_lp = if on {
        cur * HIGHRES_FACTOR
    } else {
        (cur / HIGHRES_FACTOR).max(DEFAULT_LOGPIXELS)
    };
    let mut ok = true;
    let mut c = bottle_cmd(wine, b);
    if on {
        c.args([
            "reg",
            "add",
            "HKCU\\Software\\Wine\\Mac Driver",
            "/v",
            "RetinaMode",
            "/t",
            "REG_SZ",
            "/d",
            "y",
            "/f",
        ]);
    } else {
        c.args([
            "reg",
            "delete",
            "HKCU\\Software\\Wine\\Mac Driver",
            "/v",
            "RetinaMode",
            "/f",
        ]);
    }
    ok &= run_logged(c, log, "Alta resolución");
    let mut c = bottle_cmd(wine, b);
    c.args([
        "reg",
        "add",
        "HKCU\\Control Panel\\Desktop",
        "/v",
        "LogPixels",
        "/t",
        "REG_DWORD",
        "/d",
        &new_lp.to_string(),
        "/f",
    ]);
    ok &= run_logged(c, log, &format!("Tamaño de letra: {new_lp} ppp"));
    ok
}

/// The folder where Wine keeps the socket of the wineserver of one prefix:
/// `<root>/.wine-<uid>/server-<dev>-<inode>/socket`, where `dev` and `inode` are the device and
/// inode numbers of the prefix folder, written in lower-case hex without padding.
pub fn wineserver_socket_path(root: &Path, uid: u32, dev: u64, ino: u64) -> PathBuf {
    root.join(format!(".wine-{uid}"))
        .join(format!("server-{dev:x}-{ino:x}"))
        .join("socket")
}

/// Says whether the wineserver of `prefix` has its socket under `root`, without starting any
/// process. `None` means this cannot be told (the prefix or the Wine folder cannot be read), so
/// the caller must ask the wineserver itself.
///
/// This relies on how Wine behaves on macOS: each running wineserver keeps a folder named after
/// the prefix's device and inode, and a `socket` file in it exists only while that server runs.
/// The folder and its `lock` file stay after the server exits. A crash can leave the socket
/// behind, so a present socket is only a hint that the server may be running.
fn wineserver_socket_state(root: &Path, uid: u32, prefix: &Path) -> Option<bool> {
    let meta = fs::metadata(prefix).ok()?;
    let socket = wineserver_socket_path(root, uid, meta.dev(), meta.ino());
    // Without Wine's base folder this Mac does not follow the layout above: ask the server.
    fs::metadata(root.join(format!(".wine-{uid}"))).ok()?;
    match fs::symlink_metadata(&socket) {
        Ok(_) => Some(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(false),
        Err(_) => None,
    }
}

/// Where Wine keeps its `.wine-<uid>` folder. Tests look inside the throwaway home instead of
/// the real `/tmp`, so what they see does not depend on whether this Mac ever ran Wine: there
/// is no such folder there, and the wineserver (the fake one in flows) is always asked.
fn socket_root() -> PathBuf {
    if cfg!(test) {
        home()
    } else {
        PathBuf::from("/tmp")
    }
}

/// True/false when the socket of the prefix's wineserver is (not) there; `None` when unknown.
pub fn wineserver_socket_exists(prefix: &Path) -> Option<bool> {
    let uid = fs::metadata(home()).ok()?.uid();
    wineserver_socket_state(&socket_root(), uid, prefix)
}

/// `wineserver -k0` succeeds only while the bottle's server runs.
/// Always asks the server, so decisions that act on the answer (quit, restart, copy) never rely
/// on where a Wine build keeps its socket.
pub fn is_running(wine: &Path, b: &Bottle) -> bool {
    ask_wineserver(wine, &b.path)
}

/// Quick variant of [`is_running`] for the periodic "in use" display: the server is only asked
/// when its socket exists (or when that cannot be checked), so an idle bottle costs a couple of
/// file lookups instead of a process. A wrong "no" only delays a label, so it is not used to
/// decide anything.
pub fn is_running_at(wine: &Path, prefix: &Path) -> bool {
    if wineserver_for(wine).is_some() && wineserver_socket_exists(prefix) == Some(false) {
        return false;
    }
    ask_wineserver(wine, prefix)
}

/// Asks the prefix's wineserver with `-k0`; true when there is none to ask (old behaviour), and
/// false for a prefix outside the app's own bottles: nothing of the app runs there.
fn ask_wineserver(wine: &Path, prefix: &Path) -> bool {
    if !is_own_prefix(prefix) {
        return false;
    }
    let Some(mut c) = wineserver_cmd(wine, prefix) else {
        return true;
    };
    // A socket left behind by a crash must not count: the server has to answer.
    c.arg("-k0")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Forces a bottle closed with `wineserver -k`.
fn kill_bottle(wine: &Path, b: &Bottle, log: &Log) {
    if let Some(mut c) = wineserver_cmd(wine, &b.path) {
        c.arg("-k");
        run_logged(c, log, "Forzar cierre");
    } else {
        let mut c = bottle_cmd(wine, b);
        c.args(["wineboot", "--kill", "--force"]);
        run_logged(c, log, "Forzar cierre");
    }
}

/// Quit: a bottle is closed with `wineboot.exe --end-session --shutdown --force --kill`, only
/// when the bottle is running, and waits up to 60 s for it; past that it forces the shutdown.
/// Returns true when it had to force.
pub fn quit_bottle(wine: &Path, b: &Bottle, force: bool, log: &Log) -> bool {
    if !is_running(wine, b) {
        push_log(log, format!("{}: no hay nada abierto", b.name));
        return false;
    }
    if force {
        kill_bottle(wine, b, log);
        return true;
    }
    let mut c = bottle_cmd(wine, b);
    c.args([
        "wineboot.exe",
        "--end-session",
        "--shutdown",
        "--force",
        "--kill",
    ]);
    let started = std::time::Instant::now();
    let mut timed_out = false;
    if let Ok(mut child) = spawn_logged_as(c, log, "Cerrar programas", false) {
        loop {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => break,
                Ok(None) if started.elapsed().as_secs() >= QUIT_WAIT_SECS => {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break;
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(250)),
            }
        }
    }
    if timed_out {
        push_log(log, format!("No se cerró en {QUIT_WAIT_SECS} s: forzando"));
        kill_bottle(wine, b, log);
        return true;
    }
    false
}

/// Makes a command's output go to the app's log file instead of nowhere. The file is opened for
/// appending, so every line lands whole after the app's own lines, and the program keeps writing
/// to it with no helper process left running to read a pipe. Because the program writes straight
/// into the file, the app's size limit cannot apply while it runs. What bounds the growth is
/// where it starts: when the log is already larger than [`LOG_DIRECT_OUTPUT_MAX`], or cannot be
/// opened, the output is thrown away, as it was before.
pub fn send_output_to_log(cmd: &mut std::process::Command, log: &Path) {
    use std::process::Stdio;
    if let Some(dir) = log.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let roomy = std::fs::metadata(log).map_or(true, |m| m.len() <= LOG_DIRECT_OUTPUT_MAX);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log);
    match file.and_then(|f| f.try_clone().map(|g| (f, g))) {
        Ok((out, err)) if roomy => {
            cmd.stdout(Stdio::from(out)).stderr(Stdio::from(err));
        }
        _ => {
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_wait_is_sixty_seconds() {
        assert_eq!(QUIT_WAIT_SECS * 1_000_000_000, 0xD_F847_5800);
    }

    #[test]
    fn socket_path_uses_lowercase_hex_without_padding() {
        let p = wineserver_socket_path(Path::new("/tmp"), 501, 0x0100_000f, 0x0252_89b6);
        assert_eq!(p, Path::new("/tmp/.wine-501/server-100000f-25289b6/socket"));
        let q = wineserver_socket_path(Path::new("/r"), 0, 0xAB, 0x1);
        assert_eq!(q, Path::new("/r/.wine-0/server-ab-1/socket"));
    }

    fn unique_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bl-proc-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Under test the socket folder is looked for in the home, never in the real `/tmp`, so the
    /// quick look gives the same answer on a Mac that ran Wine and on one that never did.
    #[test]
    fn tests_never_read_the_real_socket_folder() {
        assert_eq!(socket_root(), home());
        let prefix = unique_dir("root");
        let uid = fs::metadata(home()).unwrap().uid();
        let expected = wineserver_socket_state(&home(), uid, &prefix);
        assert_eq!(wineserver_socket_exists(&prefix), expected);
        if !home().join(format!(".wine-{uid}")).exists() {
            assert_eq!(expected, None, "the wineserver has to be asked");
        }
        let _ = fs::remove_dir_all(&prefix);
    }

    #[test]
    fn socket_file_decides_without_starting_anything() {
        let root = unique_dir("sock");
        let prefix = root.join("prefix");
        fs::create_dir_all(&prefix).unwrap();
        let meta = fs::metadata(&prefix).unwrap();
        let uid = meta.uid();
        // Wine's base folder missing: cannot be told.
        assert_eq!(wineserver_socket_state(&root, uid, &prefix), None);
        // Folder there but no server folder for this prefix: not running.
        fs::create_dir_all(root.join(format!(".wine-{uid}"))).unwrap();
        assert_eq!(wineserver_socket_state(&root, uid, &prefix), Some(false));
        // The server folder and its lock stay after the server exits: still not running.
        let socket = wineserver_socket_path(&root, uid, meta.dev(), meta.ino());
        fs::create_dir_all(socket.parent().unwrap()).unwrap();
        fs::write(socket.with_file_name("lock"), "").unwrap();
        assert_eq!(wineserver_socket_state(&root, uid, &prefix), Some(false));
        // The socket exists only while the server runs.
        fs::write(&socket, "").unwrap();
        assert_eq!(wineserver_socket_state(&root, uid, &prefix), Some(true));
        // A prefix that cannot be read cannot be told either.
        assert_eq!(
            wineserver_socket_state(&root, uid, &root.join("missing")),
            None
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// What `ps` says about a process: its state letters, or nothing when it is gone.
    fn process_state(pid: u32) -> String {
        let out = Command::new("/bin/ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// A finished child that nobody waits for stays as a zombie; the helper leaves none.
    #[test]
    fn a_command_nobody_waits_for_is_reaped() {
        // The check itself: a dropped child is still in the process table, marked `Z`.
        let mut lost = Command::new("/bin/sh");
        lost.args(["-c", "exit 0"]);
        let lost = lost.spawn().unwrap();
        let lost_pid = lost.id();
        drop(lost);
        // It becomes a zombie when it ends, a moment after it was started: wait (bounded) for it.
        let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !process_state(lost_pid).starts_with('Z') && std::time::Instant::now() < end {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            process_state(lost_pid).starts_with('Z'),
            "a dropped child should be a zombie, ps says {:?}",
            process_state(lost_pid)
        );

        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "exit 0"]);
        let (pid, done) = spawn_reaped(cmd).unwrap();
        done.join().unwrap();
        assert_eq!(process_state(pid), "", "the child must be gone");
    }

    /// A command that does not exist is an error, not a thread waiting for nothing.
    #[test]
    fn a_missing_command_is_reported() {
        assert!(spawn_reaped(Command::new("/nonexistent/botella")).is_err());
    }

    /// A program that never ends its line cannot make the app keep the whole stream.
    #[test]
    fn a_line_that_never_ends_is_cut() {
        let mut data = vec![b'x'; 3 * LINE_MAX + 100];
        data.extend_from_slice(b"\nnext\n");
        let mut lines = vec![];
        read_lines_lossy(&data[..], |l| lines.push(l));
        assert_eq!(lines.len(), 2, "{} lines", lines.len());
        assert!(lines[0].ends_with('…') && lines[0].len() <= LINE_MAX + 4);
        assert_eq!(lines[1], "next");
        // A line just under the limit is not marked as cut.
        let mut exact = vec![b'y'; LINE_MAX - 1];
        exact.push(b'\n');
        let mut lines = vec![];
        read_lines_lossy(&exact[..], |l| lines.push(l));
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].ends_with('…'));
    }

    #[test]
    fn lossy_reader_survives_bytes_that_are_not_utf8() {
        let data: &[u8] = b"first\n\xff\xfe broken\r\n\n   \nlast without newline";
        let mut lines = vec![];
        read_lines_lossy(data, |l| lines.push(l));
        assert_eq!(
            lines,
            vec![
                "first".to_string(),
                "\u{fffd}\u{fffd} broken".to_string(),
                "last without newline".to_string()
            ]
        );
    }

    fn run_into_log(log: &Path) {
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.args(["-c", "echo salida; echo error >&2"]);
        send_output_to_log(&mut cmd, log);
        assert!(cmd.status().unwrap().success());
    }

    /// A program opened without a window writes into the log while the log is small, and gets
    /// nothing of it once the log is large, so it cannot fill the disk.
    #[test]
    fn a_windowless_program_writes_into_the_log_only_while_it_is_small() {
        let dir = std::env::temp_dir().join(format!("bl-direct-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = dir.join("datos/registro.txt");
        run_into_log(&log);
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(
            text.contains("salida") && text.contains("error"),
            "{text:?}"
        );
        // The same log, grown past the limit (a sparse file, so it costs nothing): the program
        // still runs and the file is not touched.
        let big = std::fs::OpenOptions::new().write(true).open(&log).unwrap();
        big.set_len(LOG_DIRECT_OUTPUT_MAX + 1).unwrap();
        run_into_log(&log);
        assert_eq!(
            std::fs::metadata(&log).unwrap().len(),
            LOG_DIRECT_OUTPUT_MAX + 1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
