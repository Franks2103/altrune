//! How the program starts: the command line (`--medir`, `--screenshot`, `--reposo`, `--open`,
//! `--tema`, `--vista`, `--tamano`), the one-app rule, and the window's first settings.

use crate::app::{self, open_without_window};
use crate::backend::data_dir;
use crate::{domain, measure, App, StartOptions, APP_NAME};
use eframe::egui;
use std::path::{Path, PathBuf};

/// The icon eframe gives the Dock and the app switcher while the app runs. eframe paints its own
/// egui logo there unless told otherwise, over whatever the bundle declares. Inside Altrune.app
/// the answer is an empty icon, which tells eframe to leave the icon alone, so macOS shows the
/// bundle's Altrune.icns in the person's icon style (default, dark or tinted). Run from cargo,
/// with no bundle around it, the Dock gets the Altrune icon from the brand assets instead.
fn dock_icon(exe: Option<&Path>) -> egui::IconData {
    let in_bundle = exe.is_some_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"));
    if in_bundle {
        return egui::IconData::default();
    }
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/branding/AppIcon-1024.png"))
        .unwrap_or_default()
}

/// Takes the lock of the one running app. Breaks when another app holds it and the request (the
/// file, if any) was handed over, so this launch ends; otherwise the lock (when there is one) is
/// the caller's to keep or drop. A request that cannot be written is no reason to lose the file:
/// the launch goes on with its own window.
fn one_app(
    args: &[String],
    file: Option<&Path>,
) -> std::ops::ControlFlow<(), Option<std::fs::File>> {
    use std::ops::ControlFlow;
    if !app::takes_lock(args) {
        return ControlFlow::Continue(None);
    }
    match app::claim_settled(&data_dir()) {
        app::Claim::Held(lock) => ControlFlow::Continue(Some(lock)),
        app::Claim::Busy => match app::hand_over(&app::requests_dir(), file) {
            Ok(()) => ControlFlow::Break(()),
            Err(_) => ControlFlow::Continue(None),
        },
        app::Claim::Unavailable => ControlFlow::Continue(None),
    }
}

/// The value after `name` on the command line, if the flag is there.
fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// `--medir`: times the app's work without a window. Returns whether this launch was one.
fn measure_only(args: &[String]) -> bool {
    let Some(i) = args.iter().position(|a| a == "--medir") else {
        return false;
    };
    let scratch = args
        .get(i + 1)
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    measure::run(&scratch, !args.iter().any(|a| a == "--sin-wine"));
    true
}

/// `--open <file>` that never needs the window: another app is running and takes the request, or
/// the file opens straight away with no window at all. Returns whether this launch is done.
fn opened_without_window(args: &[String], file: &Path) -> bool {
    // Installers go through the window (to show progress and pick up the new programs), and
    // so do files that cannot be opened (the window says why); anything else opens directly.
    if domain::install::through_window(&crate::backend::classify_file(file)) {
        return false;
    }
    // A launch that only starts a program and ends must not count as the app: a request
    // handed to it would never be read. It only asks whether the app is running, and lets
    // the lock go at once, so two files opened together both run.
    if app::takes_lock(args) && app::ask_running_app(&data_dir(), &app::requests_dir(), Some(file))
    {
        return true;
    }
    open_without_window(file)
}

/// The window's first settings. `quiet` (a picture or a measurement of the idle window) keeps it
/// from taking the focus.
fn native_options(size: [f32; 2], quiet: bool) -> eframe::NativeOptions {
    // The content runs under a transparent title bar, as in the Mac's own apps: the window
    // buttons sit over the sidebar.
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_icon(dock_icon(std::env::current_exe().ok().as_deref()))
        .with_inner_size(size)
        .with_min_inner_size([820.0, 540.0])
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false);
    if quiet {
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    let mut options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    options.run_and_return = true;
    #[cfg(target_os = "macos")]
    if quiet {
        options.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            b.with_activate_ignoring_other_apps(false);
        }));
    }
    options
}

/// Reads the command line and runs the program: a measurement, a direct open, or the window.
pub(crate) fn run() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let screenshot = flag_value(&args, "--screenshot").map(PathBuf::from);
    if measure_only(&args) {
        return Ok(());
    }
    let idle = flag_value(&args, "--reposo").and_then(|s| s.parse::<f64>().ok());
    let quiet = screenshot.is_some() || idle.is_some();
    let open_file = flag_value(&args, "--open").map(PathBuf::from);
    // One app at a time: when another one is running, the file (if any) goes to its window and
    // this launch ends without one.
    if let Some(f) = &open_file {
        if opened_without_window(&args, f) {
            return Ok(());
        }
    }
    // The lock lasts as long as this file stays open, which is until the process ends.
    let _lock = match one_app(&args, open_file.as_deref()) {
        std::ops::ControlFlow::Break(()) => return Ok(()),
        std::ops::ControlFlow::Continue(file) => file,
    };

    let start = StartOptions {
        theme: match flag_value(&args, "--tema").as_deref() {
            Some("claro") => Some(egui::Theme::Light),
            Some("oscuro") => Some(egui::Theme::Dark),
            _ => None,
        },
        view: flag_value(&args, "--vista"),
        idle,
    };
    let size = flag_value(&args, "--tamano")
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some([w.parse().ok()?, h.parse().ok()?])
        })
        .unwrap_or([1120.0, 740.0]);
    eframe::run_native(
        APP_NAME,
        native_options(size, quiet),
        Box::new(move |cc| Ok(Box::new(App::new(cc, screenshot, open_file, start)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inside Altrune.app the Dock keeps the bundle's own icon (eframe is given an empty one);
    /// from cargo the Dock shows the Altrune icon instead of egui's logo.
    #[test]
    fn the_dock_icon_is_the_bundles_inside_the_app_and_altrunes_outside() {
        let bundled = dock_icon(Some(Path::new(
            "/Applications/Altrune.app/Contents/MacOS/Altrune",
        )));
        assert_eq!(bundled, egui::IconData::default());
        let from_cargo = dock_icon(Some(Path::new("/x/app/target/debug/altrune")));
        assert_eq!((from_cargo.width, from_cargo.height), (1024, 1024));
        assert_eq!(from_cargo.rgba.len(), 1024 * 1024 * 4);
        assert_eq!(dock_icon(None).width, 1024);
    }

    /// The value after a flag is read only when the flag is there.
    #[test]
    fn a_flag_takes_the_word_after_it() {
        let args: Vec<String> = ["altrune", "--tema", "claro", "--vista"]
            .map(String::from)
            .to_vec();
        assert_eq!(flag_value(&args, "--tema").as_deref(), Some("claro"));
        assert_eq!(flag_value(&args, "--vista"), None);
        assert_eq!(flag_value(&args, "--open"), None);
    }
}
