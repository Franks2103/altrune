//! The tools menu: Wine's own utilities, the Terminal window and the Finder.

use super::*;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, Debug)]
pub enum Tool {
    WineConfig,
    ControlPanel,
    TaskManager,
    Registry,
    Uninstaller,
    GameControllers,
    CommandPrompt,
    Terminal,
    Reboot,
}

impl Tool {
    pub const ALL: [Tool; 9] = [
        Tool::WineConfig,
        Tool::ControlPanel,
        Tool::Uninstaller,
        Tool::GameControllers,
        Tool::TaskManager,
        Tool::Registry,
        Tool::CommandPrompt,
        Tool::Terminal,
        Tool::Reboot,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Tool::WineConfig => "Configuración de Windows",
            Tool::ControlPanel => "Panel de control",
            Tool::TaskManager => "Administrador de tareas",
            Tool::Registry => "Editor del registro",
            Tool::Uninstaller => "Quitar programas",
            Tool::GameControllers => "Mandos de juego",
            Tool::CommandPrompt => "Símbolo del sistema",
            Tool::Terminal => "Terminal del Mac",
            Tool::Reboot => "Simular reinicio",
        }
    }
    /// `.exe` applets start directly, `wineboot.exe` with `--restart`, and `.cpl` applets through
    /// `rundll32 shell32.dll,Control_RunDLL`.
    pub fn command(self, wine: &Path, b: &Bottle) -> Command {
        if let Tool::Terminal = self {
            return terminal_command(wine, b);
        }
        let mut c = bottle_cmd(wine, b);
        match self {
            Tool::WineConfig => c.arg("winecfg.exe"),
            Tool::ControlPanel => c.arg("control.exe"),
            Tool::TaskManager => c.arg("taskmgr.exe"),
            Tool::Registry => c.arg("regedit.exe"),
            Tool::Uninstaller => c.arg("uninstaller.exe"),
            Tool::GameControllers => c.args(["rundll32", "shell32.dll,Control_RunDLL", "joy.cpl"]),
            Tool::CommandPrompt => c.args(["start", "cmd.exe"]),
            Tool::Reboot => c.args(["wineboot.exe", "--restart"]),
            Tool::Terminal => unreachable!(),
        };
        c
    }
}

fn applescript_quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Wraps text in single quotes so a shell reads it as one word, whatever it holds.
pub(crate) fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// A Terminal window for a bottle, titled with the bottle's name, with the bottle selected, Wine
/// on the PATH and the bottle as current folder. For a bottle outside the app's own folder it
/// runs nothing: Wine is only ever set up for the app's own bottles.
pub fn terminal_command(wine: &Path, b: &Bottle) -> Command {
    if !is_own_prefix(&b.path) {
        return Command::new("/usr/bin/false");
    }
    let (setup, _) = terminal_setup(wine, b);
    terminal_command_for(&setup, b)
}

/// The names of the engine's variables the Terminal's command leaves out because they are not
/// safe to write into shell text (none, with the engines the app makes itself).
pub fn terminal_dropped_names(wine: &Path) -> Vec<String> {
    split_shell_env(runtime_env(wine)).1
}

/// `export NAME='value'; ` for each variable whose name is safe to write into shell text, and the
/// names that were left out.
fn engine_exports(vars: Vec<(String, String)>) -> (String, Vec<String>) {
    let (kept, dropped) = split_shell_env(vars);
    let text = kept
        .iter()
        .map(|(k, v)| format!("export {k}={}; ", sh_quote(v)))
        .collect();
    (text, dropped)
}

/// The shell text the Terminal window runs first, and the variable names left out of it.
fn terminal_setup(wine: &Path, b: &Bottle) -> (String, Vec<String>) {
    let bin = wine
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    // What this engine needs to start at all, so `wine` typed in that window works too.
    let (engine, dropped) = engine_exports(runtime_env(wine));
    let setup = format!(
        "export WINEPREFIX={}; export WINEMSYNC={}; {engine}export PATH={}\":$PATH\"; cd {}; clear",
        sh_quote(&b.path.to_string_lossy()),
        if b.msync { 1 } else { 0 },
        sh_quote(&bin),
        sh_quote(&b.path.to_string_lossy())
    );
    (setup, dropped)
}

fn terminal_command_for(setup: &str, b: &Bottle) -> Command {
    let script = format!(
        "tell app \"Terminal\"\nactivate\ndelay 0.1\ndo script \"{}\"\nset the custom title of the front window to \"{}\"\nend tell",
        applescript_quote(setup),
        applescript_quote(&b.name)
    );
    // Under `cargo test` the command is one that does nothing, so no Terminal window opens.
    let mut c = Command::new(if cfg!(test) {
        "/usr/bin/true"
    } else {
        "/usr/bin/osascript"
    });
    c.arg("-e").arg(script);
    c
}

/// The command that shows a file or folder in the Finder.
fn finder_command(path: &Path) -> Command {
    let mut c = Command::new("/usr/bin/open");
    c.arg(path);
    c
}

/// Opens a file or folder in the Finder. The `open` process is waited for on a short-lived
/// thread, so it does not stay in the process table after every click.
pub fn open_in_finder(path: &Path) {
    // Tests never open Finder windows.
    if cfg!(test) {
        return;
    }
    let _ = spawn_reaped(finder_command(path));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A variable name is written into shell text as it is, so one that is not a plain name never
    /// reaches the Terminal's command; the values stay quoted and the valid names stay.
    #[test]
    fn the_terminal_exports_only_variables_with_plain_names() {
        let vars = vec![
            ("GOOD_1".to_string(), "it's".to_string()),
            ("X;touch /tmp/pwn".to_string(), "1".to_string()),
            ("A B".to_string(), "1".to_string()),
            ("$(id)".to_string(), "1".to_string()),
            ("_ok".to_string(), "v".to_string()),
        ];
        let (text, dropped) = engine_exports(vars);
        assert_eq!(text, "export GOOD_1='it'\\''s'; export _ok='v'; ");
        assert_eq!(dropped, ["X;touch /tmp/pwn", "A B", "$(id)"]);
        assert!(!text.contains("pwn") && !text.contains("$(id)"));
    }

    /// The path goes as one argument, whatever it holds, to the system's own `open`.
    #[test]
    fn the_finder_command_opens_the_path_as_one_argument() {
        let c = finder_command(Path::new("/b/Mi botella; rm -rf"));
        assert_eq!(c.get_program(), "/usr/bin/open");
        let args: Vec<_> = c.get_args().collect();
        assert_eq!(args, ["/b/Mi botella; rm -rf"]);
    }
}
