//! A fake Wine for the flow tests: two small `sh` scripts (`wine` and `wineserver`) that answer
//! what the app asks of a real Wine and run no Windows code.
//!
//! Every call is appended to HOME/wine-falso.log. Programs that "run" are a `sleep` whose PID is
//! recorded in HOME/wine-falso/pids/<prefix id>/<pid>, so the tests can tell what is still
//! alive and end only those PIDs. The scripts refuse to do anything (exit 99) unless HOME is
//! inside the temp dir and WINEPREFIX, when set, is inside HOME.

use super::fixtures;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What every script starts with: the safety checks, the log line and the helpers that find and
/// end the fake programs of one prefix. `@TMP@` is the temp dir, as a real path.
const PRELUDE: &str = r#"#!/bin/sh
fail() { echo "wine falso: $1" >&2; exit 99; }
home=$(cd "$HOME" 2>/dev/null && pwd -P) || fail "HOME no existe"
case "$home" in "@TMP@"/*) ;; *) fail "HOME fuera de la carpeta temporal" ;; esac
case "$WINEPREFIX" in
  "") ;;
  *..*) fail "WINEPREFIX con .." ;;
  "$HOME"/*|"$home"/*) ;;
  *) fail "WINEPREFIX fuera de HOME" ;;
esac
state="$HOME/wine-falso"
echo "$(basename "$0") $* [prefix=${WINEPREFIX:-}]" >> "$HOME/wine-falso.log"
pids_dir() { echo "$state/pids/$(printf %s "$WINEPREFIX" | cksum | cut -d' ' -f1)"; }
# Only a process that still looks like one of ours is ever signalled.
is_ours() { ps -o command= -p "$1" 2>/dev/null | grep -Eq 'sleep|wine-falso'; }
end_prefix() {
  for f in "$(pids_dir)"/*; do
    [ -f "$f" ] || continue
    pid=${f##*/}
    if is_ours "$pid"; then kill "$pid" 2>/dev/null; fi
    rm -f "$f"
  done
}
running_prefix() {
  for f in "$(pids_dir)"/*; do
    [ -f "$f" ] || continue
    pid=${f##*/}
    if is_ours "$pid" && kill -0 "$pid" 2>/dev/null; then return 0; fi
    rm -f "$f"
  done
  return 1
}
"#;

const WINE: &str = r#"
register() {
  mkdir -p "$(pids_dir)"
  pidfile="$(pids_dir)/$$"
  echo $$ > "$pidfile"
  trap 'rm -f "$pidfile"' EXIT
}
make_prefix() {
  p=$WINEPREFIX
  mkdir -p "$p/drive_c/windows/system32" "$p/drive_c/users/${USER:-usuario}" \
    "$p/drive_c/ProgramData/Microsoft/Windows/Start Menu/Programs"
  : > "$p/system.reg"; : > "$p/user.reg"; : > "$p/userdef.reg"
}
install_test_program() {
  menu="$WINEPREFIX/drive_c/ProgramData/Microsoft/Windows/Start Menu/Programs"
  dir="$WINEPREFIX/drive_c/Program Files/Programa de prueba"
  mkdir -p "$menu" "$dir"
  printf MZ > "$dir/programa.exe"
  cp "$state/fixtures/Programa de prueba.lnk" "$menu/Programa de prueba.lnk"
}
open_file() {
  case "${1##*/}" in
    instalador-prueba.exe) register; sleep 0.3; install_test_program; exit 0 ;;
    instalador-falla.exe)
      register; sleep 0.2
      echo 'wine: Unhandled page fault on read access to 0000000000000000 at address 00006FFFFFAB1234 (thread 0024), starting debugger...' >&2
      exit 3 ;;
    instalador-lento.exe|programa.exe|"Programa de prueba.lnk") register; exec sleep 600 ;;
    se-cierra.exe)
      register; sleep 0.4
      echo 'err:module:import_dll Library MSVCP140.dll (which is needed by L"C:\\programa.exe") not found' >&2
      exit 1 ;;
    *) exit 0 ;;
  esac
}
case "$1" in
  --version) echo "wine-9.0 (falso)"; exit 0 ;;
  wineboot|wineboot.exe)
    case "$*" in
      *--end-session*|*--kill*) end_prefix ;;
      *) sleep 0.1; make_prefix ;;
    esac
    exit 0 ;;
  reg)
    [ -f "$state/fallar-reg" ] && { echo "reg: no se pudo escribir" >&2; exit 1; }
    echo "$*" >> "$WINEPREFIX/registro-falso.txt"
    exit 0 ;;
  winecfg) exit 0 ;;
  msiexec) open_file "$3" ;;
  start) for last in "$@"; do :; done; open_file "$last" ;;
  *) open_file "$1" ;;
esac
"#;

const WINESERVER: &str = r#"
case "$1" in
  -k0) running_prefix; exit $? ;;
  -k) end_prefix; exit 0 ;;
  -w)
    n=0
    while running_prefix && [ "$n" -lt 50 ]; do sleep 0.1; n=$((n + 1)); done
    exit 0 ;;
  *) exit 0 ;;
esac
"#;

/// The fake Wine written into a HOME, and what the tests ask of it afterwards.
pub struct FakeWine {
    home: PathBuf,
}

/// The temp dir as the scripts see it (`pwd -P`), so a symlinked `/var` still matches.
fn real_temp_dir() -> PathBuf {
    std::env::temp_dir()
        .canonicalize()
        .expect("the temp dir exists")
}

fn write_script(path: &Path, text: &str) {
    fs::write(path, text).expect("a fake Wine script");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("its permissions");
}

impl FakeWine {
    /// Writes the scripts, the shortcut fixture and a config.json that points at them (Mac
    /// launchers off, the games engine already "offered").
    pub fn write(home: &Path) -> Self {
        Self::write_with(home, true)
    }

    /// Like `write`, without telling the config where Wine is (the first start of the app).
    pub fn write_without_config_path(home: &Path) -> Self {
        Self::write_with(home, false)
    }

    fn write_with(home: &Path, with_path: bool) -> Self {
        let root = home.join("wine-falso");
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("the fake Wine folder");
        fs::create_dir_all(root.join("fixtures")).expect("the fixtures folder");
        let prelude = PRELUDE.replace("@TMP@", &real_temp_dir().to_string_lossy());
        write_script(&bin.join("wine"), &format!("{prelude}{WINE}"));
        write_script(&bin.join("wineserver"), &format!("{prelude}{WINESERVER}"));
        fs::write(
            root.join("fixtures/Programa de prueba.lnk"),
            fixtures::test_program_lnk(),
        )
        .expect("the shortcut fixture");
        let wine = bin.join("wine");
        let config = serde_json::json!({
            "wine_path": with_path.then(|| wine.to_string_lossy().to_string()),
            "mac_launchers": false,
            "games_offered": true,
        });
        let dir = home.join("Library/Application Support/Botella Libre");
        fs::create_dir_all(&dir).expect("the app's data folder");
        fs::write(dir.join("config.json"), config.to_string()).expect("config.json");
        Self {
            home: home.to_path_buf(),
        }
    }

    /// Every call the scripts logged, one per line.
    pub fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.home.join("wine-falso.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// How many logged calls contain `needle`.
    pub fn count(&self, needle: &str) -> usize {
        self.calls().iter().filter(|l| l.contains(needle)).count()
    }

    /// The calls that did something. `wineserver -k0` is left out: it is the question the app
    /// asks to know whether a bottle is in use (the live look every few seconds), so it is
    /// neither Wine starting nor a program running.
    pub fn acts(&self) -> Vec<String> {
        let mut calls = self.calls();
        calls.retain(|l| !l.starts_with("wineserver -k0 "));
        calls
    }

    /// Makes `wine reg add` fail from now on (a recipe that cannot be written).
    pub fn fail_registry_writes(&self) {
        fs::write(self.home.join("wine-falso/fallar-reg"), "").expect("the marker");
    }

    /// The recorded PIDs of programs that are alive now (a zombie is not alive).
    pub fn alive(&self) -> Vec<u32> {
        alive_in(&self.home)
    }

    /// Ends every fake program that is still alive, see `end_all_in`.
    pub fn end_all(&self) {
        end_all_in(&self.home);
    }
}

/// The PIDs the fake programs recorded under `home`.
fn recorded_pids_in(home: &Path) -> Vec<u32> {
    let mut pids = vec![];
    let Ok(prefixes) = fs::read_dir(home.join("wine-falso/pids")) else {
        return pids;
    };
    for prefix in prefixes.flatten() {
        for file in fs::read_dir(prefix.path()).into_iter().flatten().flatten() {
            if let Some(pid) = file.file_name().to_str().and_then(|n| n.parse().ok()) {
                pids.push(pid);
            }
        }
    }
    pids
}

pub fn alive_in(home: &Path) -> Vec<u32> {
    recorded_pids_in(home)
        .into_iter()
        .filter(|pid| is_alive(*pid))
        .collect()
}

/// Ends every fake program recorded under `home` that is still alive: only PIDs the fake
/// recorded, and only when the process still looks like a fake program, so a reused PID is
/// never touched. The parent of a flow test also calls it on a child it had to kill, because
/// the child's own cleanup never ran.
pub fn end_all_in(home: &Path) {
    for pid in alive_in(home) {
        if looks_fake(pid) {
            let _ = Command::new("/bin/kill").arg(pid.to_string()).status();
        }
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !alive_in(home).is_empty() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// `ps` state of a process, `None` when there is none.
fn state_of(pid: u32) -> Option<String> {
    let out = Command::new("/bin/ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn is_alive(pid: u32) -> bool {
    state_of(pid).is_some_and(|s| !s.starts_with('Z'))
}

fn looks_fake(pid: u32) -> bool {
    Command::new("/bin/ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .output()
        .map(|o| {
            let text = String::from_utf8_lossy(&o.stdout);
            text.contains("sleep") || text.contains("wine-falso")
        })
        .unwrap_or(false)
}

/// Zombie children of this process (`ps` state starting with Z): a child the app started and
/// never waited for.
pub fn zombie_children() -> Vec<u32> {
    let me = std::process::id();
    // A `ps` that cannot run must fail the test: an empty answer would pass it for nothing.
    let out = Command::new("/bin/ps")
        .args(["-axo", "pid,ppid,stat"])
        .output()
        .expect("ps must run to look for zombie children");
    assert!(
        out.status.success(),
        "ps must succeed to look for zombie children"
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let (pid, ppid, stat) = (f.next()?, f.next()?, f.next()?);
            (ppid.parse::<u32>().ok()? == me && stat.starts_with('Z')).then(|| pid.parse().ok())?
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(tag: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!("bl-fake-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        home
    }

    fn status(home: &Path, env_home: &Path, prefix: Option<&Path>) -> i32 {
        let mut c = Command::new(home.join("wine-falso/bin/wine"));
        c.arg("--version").env("HOME", env_home);
        if let Some(p) = prefix {
            c.env("WINEPREFIX", p);
        }
        c.output().unwrap().status.code().unwrap()
    }

    /// The scripts do nothing at all unless HOME is in the temp dir and the prefix in HOME.
    #[test]
    fn the_fake_refuses_a_home_or_prefix_outside_its_sandbox() {
        let home = temp_home("a");
        FakeWine::write(&home);
        assert_eq!(status(&home, &home, None), 0);
        assert_eq!(status(&home, &home, Some(&home.join("botella"))), 0);
        // A HOME outside the temp dir, a prefix outside HOME and one that climbs out of it.
        assert_eq!(status(&home, Path::new("/usr"), None), 99);
        assert_eq!(status(&home, &home, Some(Path::new("/usr/local"))), 99);
        assert_eq!(status(&home, &home, Some(&home.join("../fuera"))), 99);
        // The refusals wrote nothing outside the sandbox: the log has only the two good calls.
        let log = fs::read_to_string(home.join("wine-falso.log")).unwrap();
        assert_eq!(log.lines().count(), 2, "{log}");
        let _ = fs::remove_dir_all(&home);
    }

    /// The cleanup a parent runs on a killed child ends a recorded program, and only that one.
    #[test]
    fn the_cleanup_ends_a_recorded_program_by_its_pid() {
        let home = temp_home("b");
        let mut fake = Command::new("sleep").arg("600").spawn().unwrap();
        let mut other = Command::new("sleep").arg("600").spawn().unwrap();
        let dir = home.join("wine-falso/pids/p1");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(fake.id().to_string()), "").unwrap();
        end_all_in(&home);
        // `wait` returns at once only if the recorded process was really ended.
        let ended = fake.wait().unwrap();
        assert!(!ended.success());
        assert!(
            other.try_wait().unwrap().is_none(),
            "an unrecorded one was touched"
        );
        let _ = other.kill();
        let _ = other.wait();
        let _ = fs::remove_dir_all(&home);
    }
}
