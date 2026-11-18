//! Runs a flow test in a child copy of the test binary whose HOME is a fresh folder.
//!
//! Every path the app uses comes from HOME, so inside the child nothing of the person's can be
//! reached. The parent makes the folder, starts the child on exactly this one test, waits for
//! it with a time limit and checks that one test really ran (a filter that matches nothing must
//! not pass). The child refuses to run unless HOME is that folder and the folder is inside the
//! temp dir.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Set in the child to the folder that is its HOME; its presence is what makes a process "the
/// child".
const CHILD_HOME: &str = "BOTELLA_FLOW_HOME";

/// Makes the probe flow of the tests die without unwinding (see the tests below).
#[cfg(test)]
const DIE: &str = "BOTELLA_FLOW_DIE";

/// How long a flow may take before the parent gives up and kills the child it started.
const TIME_LIMIT: Duration = Duration::from_secs(90);

/// Declares a flow test: `flow_test!(name, |home| { ... })`. The body runs in the child, with
/// `home` the throwaway HOME.
macro_rules! flow_test {
    ($name:ident, |$home:ident| $body:block) => {
        #[test]
        fn $name() {
            $crate::flows::isolated::run(
                module_path!(),
                stringify!($name),
                |$home: &::std::path::Path| $body,
            );
        }
    };
}

/// The parent and the child side of one flow test.
pub fn run(module: &str, name: &str, body: impl FnOnce(&Path)) {
    match std::env::var_os(CHILD_HOME) {
        Some(folder) => child(Path::new(&folder), body),
        None => parent(module, name),
    }
}

/// The path the test filter knows: `module_path!()` starts with the crate's name, filters do not.
fn test_path(module: &str, name: &str) -> String {
    let inside = module.split_once("::").map(|(_, rest)| rest).unwrap_or("");
    if inside.is_empty() {
        name.to_string()
    } else {
        format!("{inside}::{name}")
    }
}

fn parent(module: &str, name: &str) {
    if let Err(why) = launch(module, name, &[]) {
        panic!("{why}");
    }
}

/// The throwaway HOME the parent makes for the flow at `path`.
fn home_folder(path: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "botella-flow-{}-{}",
        std::process::id(),
        path.replace("::", "-")
    ))
}

/// Runs one flow in its child process and says why it failed, if it did. `extra_env` is added
/// to the child's environment. Whenever the child does not end well, the fake programs it
/// recorded are ended (only those pids, and only while they still look like fakes): its own
/// cleanup does not run when it is killed or dies without unwinding.
fn launch(module: &str, name: &str, extra_env: &[(&str, &str)]) -> Result<(), String> {
    let path = test_path(module, name);
    let folder = home_folder(&path);
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("the throwaway HOME");
    let mut child = Command::new(std::env::current_exe().expect("the test binary"))
        .args([&path, "--exact", "--nocapture", "--test-threads=1"])
        .env("HOME", &folder)
        .env(CHILD_HOME, &folder)
        // No Wine of the person's can be found through PATH; the fake scripts need the basics.
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .envs(extra_env.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the child test process");
    let mut out = child.stdout.take().expect("its stdout");
    let mut err = child.stderr.take().expect("its stderr");
    let out = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.read_to_string(&mut s);
        s
    });
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait().expect("waiting for the child") {
            Some(status) => break Some(status),
            None if started.elapsed() > TIME_LIMIT => {
                // Only the child this test started.
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    // Before reading what the child printed: a fake program it left behind may still hold its
    // output pipes, and the reading would wait for it.
    if !status.is_some_and(|s| s.success()) {
        super::fake_wine::end_all_in(&folder);
    }
    let (out, err) = (
        out.join().unwrap_or_default(),
        err.join().unwrap_or_default(),
    );
    let one_test = out.contains("running 1 test\n") && out.contains("1 passed;");
    if status.is_some_and(|s| s.success()) && one_test {
        // Only the folder this test made.
        let _ = std::fs::remove_dir_all(&folder);
        return Ok(());
    }
    let report = format!(
        "the flow {path} failed ({}); its HOME was kept at {}\n--- stdout ---\n{out}\n--- stderr ---\n{err}",
        match status {
            Some(s) if s.success() => "it ran no test, or not exactly one".to_string(),
            Some(s) => s.to_string(),
            None => format!("no answer in {} s", TIME_LIMIT.as_secs()),
        },
        folder.display()
    );
    // The same report is kept beside the folder's contents: a failure that shows up once in a
    // while, in a run whose terminal output is gone, still leaves its exact reason behind.
    let _ = std::fs::write(folder.join(FAILURE_REPORT), &report);
    Err(report)
}

/// Where a failed flow's output is kept, inside the HOME folder that is kept with it.
const FAILURE_REPORT: &str = "salida-de-la-prueba.txt";

fn child(folder: &Path, body: impl FnOnce(&Path)) {
    let temp = std::env::temp_dir()
        .canonicalize()
        .expect("the temp dir exists");
    let real = folder.canonicalize().expect("the throwaway HOME exists");
    assert_eq!(
        std::env::var_os("HOME").as_deref(),
        Some(folder.as_os_str()),
        "the child's HOME must be its throwaway folder"
    );
    assert!(
        real.starts_with(&temp) && real != temp,
        "the throwaway HOME must be inside the temp dir"
    );
    body(folder);
}

#[cfg(test)]
mod tests {
    use super::*;

    flow_test!(probe, |home| {
        std::fs::write(home.join("ran"), "").unwrap();
    });

    /// The child refuses to run its body when its HOME is not the folder it was given, so a
    /// flow can never run against any other HOME. Both folders here are throwaway ones.
    #[test]
    fn the_child_refuses_a_home_that_is_not_its_folder() {
        let tag = std::process::id();
        let folder = std::env::temp_dir().join(format!("botella-refusal-{tag}-a"));
        let other = std::env::temp_dir().join(format!("botella-refusal-{tag}-b"));
        for dir in [&folder, &other] {
            let _ = std::fs::remove_dir_all(dir);
            std::fs::create_dir_all(dir).unwrap();
        }
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "flows::isolated::tests::probe",
                "--exact",
                "--test-threads=1",
            ])
            .env("HOME", &other)
            .env(CHILD_HOME, &folder)
            .output()
            .unwrap();
        assert!(!out.status.success(), "the child must refuse");
        assert!(String::from_utf8_lossy(&out.stdout).contains("throwaway folder"));
        assert!(!folder.join("ran").exists() && !other.join("ran").exists());
        for dir in [&folder, &other] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// Not a test of its own: with the variable set, a flow whose child leaves a fake program
    /// running and then dies without unwinding. Without it, it does nothing.
    #[test]
    // The fake program is left running on purpose: that is what the parent has to clean up.
    #[allow(clippy::zombie_processes)]
    fn dies_without_unwinding() {
        if std::env::var_os(DIE).is_none() {
            return;
        }
        run(module_path!(), "dies_without_unwinding", |home| {
            let pids = home.join("wine-falso/pids/x");
            std::fs::create_dir_all(&pids).unwrap();
            let fake = Command::new("/bin/sleep")
                .arg("600")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            std::fs::write(pids.join(fake.id().to_string()), "").unwrap();
            // The way a stack overflow or an out-of-memory kill ends a process: no unwinding,
            // no Drop. Only this process is signalled, by its own pid.
            let _ = Command::new("/bin/kill")
                .args(["-9", &std::process::id().to_string()])
                .status();
            std::thread::sleep(Duration::from_secs(30));
        });
    }

    /// A child that dies without unwinding leaves its fake programs running; the parent ends
    /// them before it reports the failure.
    #[test]
    fn a_child_that_dies_hard_leaves_no_fake_program() {
        let result = launch(
            "altrune::flows::isolated::tests",
            "dies_without_unwinding",
            &[(DIE, "1")],
        );
        let folder = home_folder("flows::isolated::tests::dies_without_unwinding");
        let left = super::super::fake_wine::alive_in(&folder);
        // The failure's report is kept in the folder too, for a failure seen only once.
        let kept = std::fs::read_to_string(folder.join(FAILURE_REPORT)).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&folder);
        assert!(result.is_err(), "the flow must be reported as failed");
        assert!(left.is_empty(), "fake programs still running: {left:?}");
        assert!(
            kept.contains("dies_without_unwinding failed"),
            "no report kept: {kept:?}"
        );
    }

    #[test]
    fn the_filter_path_drops_the_crate_name() {
        assert_eq!(
            test_path("altrune::flows::main_flows", "x"),
            "flows::main_flows::x"
        );
        assert_eq!(test_path("altrune", "x"), "x");
    }
}
