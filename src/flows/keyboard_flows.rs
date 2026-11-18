//! The keyboard and the screen reader, end to end: Tab reaches every control in order, the page
//! follows the focus, every stop has a name, Shift-Tab goes back, Escape closes dialogs and the
//! arrows move between the tabs of a bottle. The checks read the accessibility tree (what
//! VoiceOver reads) and the app's state, never the words of a label, except the names this
//! package gives.

use super::driver::Flow;
use super::fixtures;
use crate::{Action, BottleTab};
use eframe::egui::{Key, Modifiers};
use std::time::Duration;

const LONG: Duration = Duration::from_secs(20);

/// The smallest window the app allows, so the pages are taller than the window.
const SMALL: (f32, f32) = (820.0, 540.0);

/// The most Tab presses a page may need to come back to its first stop.
const MOST_STOPS: usize = 80;

/// Where the focus is: the name of the control and its place on screen.
#[derive(Clone, Debug, PartialEq)]
struct Stop {
    name: String,
    at: (i64, i64, i64, i64),
}

fn stop(f: &Flow) -> Stop {
    let b = f.focused_bounds().unwrap_or_default();
    Stop {
        name: f.focused().unwrap_or_default(),
        at: (b.0 as i64, b.1 as i64, b.2 as i64, b.3 as i64),
    }
}

/// Presses Tab (or Shift-Tab) and lets the page finish what the press set going: the scroll
/// that follows the focus is an animation, and the control is in its place only when it ends.
fn press(f: &mut Flow, back: bool) -> Stop {
    let shift = if back {
        Modifiers::SHIFT
    } else {
        Modifiers::NONE
    };
    f.key_with(Key::Tab, shift);
    f.advance(1.0);
    f.frame();
    f.frame();
    stop(f)
}

/// A flow with Wine ready and a bottle with two programs, in the smallest window.
fn start(home: &std::path::Path) -> Flow {
    fixtures::bottle_with("Juegos de prueba", &["Programa de prueba", "Otro programa"]);
    let mut f = Flow::start(home);
    f.resize(SMALL.0, SMALL.1);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.advance(1.0);
    f.frame();
    f
}

/// Walks the page with Tab until the focus comes back to where it started and checks, at every
/// stop, what the keyboard and VoiceOver need: the control has a name, it is inside the window
/// (so the page followed the focus), it differs from the stop before, and Shift-Tab undoes Tab.
/// Returns the stops, in order.
fn walk(f: &mut Flow, page: &str) -> Vec<Stop> {
    let mut stops: Vec<Stop> = vec![];
    for n in 0..MOST_STOPS {
        let now = press(f, false);
        assert!(
            !now.name.trim().is_empty(),
            "{page}: Tab stop {n} has no name: {stops:?}\n{}",
            f.tree()
        );
        let (_, top, _, bottom) = now.at;
        assert!(
            top >= 0 && bottom <= SMALL.1 as i64,
            "{page}: stop {n} ({}) is out of the window at {:?}",
            now.name,
            now.at
        );
        if let Some(prev) = stops.last() {
            assert_ne!(prev, &now, "{page}: Tab got stuck at stop {n}: {stops:?}");
        }
        if stops.first() == Some(&now) {
            // The ring closed: the page scrolled back to the first stop.
            assert!(stops.len() >= 2, "{page}: only one stop: {stops:?}");
            return stops;
        }
        stops.push(now);
    }
    panic!("{page}: Tab never came back to the first stop: {stops:?}");
}

/// After a walk, Shift-Tab goes back stop by stop (the focus is on the first stop again). Only
/// the controls are compared, not their places: going back scrolls the page the other way.
fn walk_back(f: &mut Flow, page: &str, stops: &[Stop]) {
    // From the first stop, Shift-Tab goes to the last.
    let last = press(f, true);
    assert_eq!(
        last.name,
        stops.last().unwrap().name,
        "{page}: Shift-Tab from the first stop goes to the last"
    );
    assert!(
        last.at.1 >= 0 && last.at.3 <= SMALL.1 as i64,
        "{page}: {last:?} is out of the window"
    );
    for (n, expected) in stops.iter().rev().enumerate().skip(1).take(4) {
        let now = press(f, true);
        assert_eq!(
            now.name, expected.name,
            "{page}: Shift-Tab number {n} goes back one stop"
        );
        assert!(
            now.at.1 >= 0 && now.at.3 <= SMALL.1 as i64,
            "{page}: {now:?} is out of the window"
        );
    }
}

/// Tab through a page and back, as `walk` and `walk_back` say.
fn check_page(f: &mut Flow, page: &str) {
    let stops = walk(f, page);
    walk_back(f, page, &stops);
}

flow_test!(
    every_tab_stop_of_inicio_and_of_each_bottle_tab_has_a_name_and_is_in_view,
    |home| {
        let mut f = start(home);
        f.act(Action::GoHome);
        f.advance(1.0);
        f.frame();
        check_page(&mut f, "Inicio");
        f.act(Action::Select(0));
        for (tab, page) in [
            (BottleTab::Programs, "Programas"),
            (BottleTab::Settings, "Configuración"),
            (BottleTab::Diagnostics, "Diagnóstico"),
        ] {
            f.act(Action::SetTab(tab));
            f.advance(1.0);
            f.frame();
            check_page(&mut f, page);
        }
        f.finish();
    }
);

flow_test!(
    every_tab_stop_of_ajustes_and_of_the_new_bottle_dialog_has_a_name_and_is_in_view,
    |home| {
        let mut f = start(home);
        f.act(Action::OpenSettings);
        f.advance(1.0);
        f.frame();
        check_page(&mut f, "Ajustes");
        f.act(Action::GoHome);
        f.act(Action::OpenNewBottle);
        f.advance(1.0);
        f.frame();
        assert!(f.app.modal.is_some());
        // The dialog traps the focus: its own controls only, all named.
        let stops = walk(&mut f, "Nueva botella");
        assert!(
            stops.iter().any(|s| s.name == "Nombre de la botella"),
            "{stops:?}"
        );
        walk_back(&mut f, "Nueva botella", &stops);
        f.finish();
    }
);

flow_test!(
    escape_closes_every_dialog_that_needs_no_file_dialog,
    |home| {
        fixtures::bottle_with("Juegos de prueba", &["Programa de prueba"]);
        let installer = fixtures::installer(home, "instalador-prueba.exe");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Select(0));
        f.advance(1.0);
        f.frame();
        let dialogs: Vec<(&str, Action)> = vec![
            ("Nueva botella", Action::OpenNewBottle),
            ("Instalar un programa", Action::Drop(installer)),
            ("Abrir con opciones", Action::RunWithOptions(0)),
            ("Cambiar el nombre", Action::OpenRename),
            ("Borrar la botella", Action::AskDelete),
            ("Estado de Wine", Action::OpenWineHelp),
        ];
        for (what, action) in dialogs {
            f.act(action);
            f.advance(1.0);
            f.frame();
            assert!(f.app.modal.is_some(), "{what}: the dialog opens");
            f.key(Key::Escape);
            f.advance(1.0);
            f.frame();
            assert!(f.app.modal.is_none(), "{what}: Escape closes the dialog");
            assert!(
                f.app.install.is_none() && f.app.bottles.len() == 1,
                "{what}: closing it did nothing else"
            );
        }
        f.finish();
    }
);

flow_test!(the_arrows_move_between_the_tabs_of_a_bottle, |home| {
    let mut f = start(home);
    f.act(Action::Select(0));
    f.advance(1.0);
    f.frame();
    assert_eq!(f.app.tab, BottleTab::Programs);
    // Tab until a tab of the bottle has the focus (its name starts with the tab's word).
    let mut found = false;
    for _ in 0..MOST_STOPS {
        let now = press(&mut f, false);
        if now.name.starts_with("Programas") {
            found = true;
            break;
        }
    }
    assert!(found, "Tab reaches the tabs\n{}", f.tree());
    let mut visited = vec![];
    for _ in 0..4 {
        f.key(Key::ArrowRight);
        f.advance(1.0);
        f.frame();
        visited.push((f.app.tab, f.focused().unwrap_or_default()));
    }
    let tabs: Vec<_> = visited.iter().map(|(t, _)| *t).collect();
    assert_eq!(
        tabs,
        [
            BottleTab::Settings,
            BottleTab::Diagnostics,
            BottleTab::Programs,
            BottleTab::Settings
        ],
        "→ goes to the next tab and the row is a ring"
    );
    // The focus went with the choice.
    assert_eq!(visited[0].1, "Configuración");
    assert!(visited[2].1.starts_with("Programas"), "{visited:?}");
    f.key(Key::ArrowLeft);
    f.advance(1.0);
    f.frame();
    assert_eq!(f.app.tab, BottleTab::Programs);
    f.key(Key::ArrowLeft);
    f.advance(1.0);
    f.frame();
    assert_eq!(
        f.app.tab,
        BottleTab::Diagnostics,
        "← from the first is the last"
    );
    f.finish();
});

flow_test!(
    the_screen_reader_hears_the_current_page_titles_and_one_wine_chip,
    |home| {
        let mut f = start(home);
        f.act(Action::GoHome);
        f.advance(1.0);
        f.frame();
        // The sidebar entry of the page the person is on says so; the others do not.
        let spoken = f.spoken();
        assert!(
            spoken
                .iter()
                .any(|n| n.starts_with("Inicio") && n.contains("página actual")),
            "{spoken:?}"
        );
        assert_eq!(
            spoken
                .iter()
                .filter(|n| n.contains("página actual"))
                .count(),
            1,
            "{spoken:?}"
        );
        // Wine's state in the sidebar is the chip's button alone: its two texts are not read again.
        let summary = f.app.wine_summary();
        let (title, sub) = (summary.title, summary.sub);
        let spoken = f.spoken();
        let chip = format!("{title}. {sub}");
        assert_eq!(
            spoken.iter().filter(|n| **n == chip).count(),
            1,
            "{spoken:?}"
        );
        assert!(!spoken.contains(&title), "{spoken:?}");
        assert!(!spoken.contains(&sub), "{spoken:?}");

        // The page title is a heading of the first level.
        let titles = f.headings();
        assert!(
            titles.iter().any(|(t, level)| t == "Inicio" && *level == 1),
            "{titles:?}"
        );

        f.act(Action::Select(0));
        f.advance(1.0);
        f.frame();
        let spoken = f.spoken();
        assert!(
            spoken
                .iter()
                .any(|n| n.starts_with("Juegos de prueba,") && n.contains("página actual")),
            "{spoken:?}"
        );
        assert!(
            !spoken
                .iter()
                .any(|n| n.starts_with("Inicio,") && n.contains("página actual")),
            "{spoken:?}"
        );
        assert!(
            f.headings()
                .iter()
                .any(|(t, l)| t == "Juegos de prueba" && *l == 1),
            "{:?}",
            f.headings()
        );

        f.act(Action::OpenSettings);
        f.advance(1.0);
        f.frame();
        assert!(
            f.spoken()
                .iter()
                .any(|n| n.starts_with("Ajustes") && n.contains("página actual")),
            "{:?}",
            f.spoken()
        );
        assert!(
            f.headings().iter().any(|(t, l)| t == "Ajustes" && *l == 1),
            "{:?}",
            f.headings()
        );

        f.finish();
    }
);

flow_test!(
    tab_in_a_dialog_goes_through_the_buttons_in_the_order_they_are_drawn,
    |home| {
        let installer = fixtures::installer(home, "instalador-prueba.exe");
        let mut f = start(home);
        f.act(Action::Select(0));
        f.advance(1.0);
        f.frame();
        // The delete dialog: «Cancelar» is on the left and the first stop; the destructive
        // action to its right is the second one, never the one Tab lands on first.
        f.act(Action::AskDelete);
        f.advance(1.0);
        f.frame();
        f.frame();
        let first = press(&mut f, false);
        let second = press(&mut f, false);
        assert_eq!(first.name, "Cancelar", "{}", f.tree());
        assert_eq!(second.name, "Mover a la Papelera", "{}", f.tree());
        assert!(
            first.at.0 < second.at.0,
            "the first stop is the one on the left: {first:?} then {second:?}"
        );
        // The third Tab starts again; ↩ on «Cancelar» closes the dialog and touches nothing.
        assert_eq!(press(&mut f, false).name, "Cancelar");
        f.key(Key::Enter);
        f.advance(1.0);
        f.frame();
        assert!(f.app.modal.is_none(), "{}", f.dump());
        assert_eq!(f.app.bottles.len(), 1);
        assert!(f.app.bottles[0].path.exists(), "the bottle is still there");

        // The install dialog has the same two buttons, in the same order.
        f.act(Action::Drop(installer));
        f.advance(1.0);
        f.frame();
        f.frame();
        assert!(f.app.modal.is_some(), "{}", f.dump());
        let stops = walk(&mut f, "Instalar un programa");
        let cancel = stops.iter().position(|s| s.name == "Cancelar");
        let main = stops
            .iter()
            .position(|s| s.name == "Instalar" || s.name == "Abrir");
        let (Some(cancel), Some(main)) = (cancel, main) else {
            panic!("the dialog's buttons are stops: {stops:?}");
        };
        assert!(cancel < main, "«Cancelar» comes first: {stops:?}");
        assert!(stops[cancel].at.0 < stops[main].at.0, "{stops:?}");
        f.finish();
    }
);

flow_test!(
    the_focus_comes_back_to_the_control_that_opened_a_dialog,
    |home| {
        let mut f = start(home);
        f.act(Action::GoHome);
        f.advance(1.0);
        f.frame();
        let mut reached = false;
        for _ in 0..MOST_STOPS {
            if press(&mut f, false).name == "Nueva botella" {
                reached = true;
                break;
            }
        }
        assert!(reached, "Tab reaches «Nueva botella»\n{}", f.tree());
        f.key(Key::Enter);
        f.advance(1.0);
        f.frame();
        assert!(f.app.modal.is_some(), "↩ opens the dialog\n{}", f.dump());
        f.key(Key::Escape);
        f.advance(1.0);
        f.frame();
        f.frame();
        assert!(f.app.modal.is_none());
        // Tab carries on from the button that opened the dialog, not from the top of the window.
        assert_eq!(
            f.focused().as_deref(),
            Some("Nueva botella"),
            "{}",
            f.tree()
        );
        f.finish();
    }
);

flow_test!(
    a_settings_row_with_two_controls_is_visited_left_to_right,
    |home| {
        let mut f = start(home);
        f.app.cfg.mac_launchers = true;
        f.act(Action::OpenSettings);
        f.advance(1.0);
        f.frame();
        let stops = walk(&mut f, "Ajustes");
        let see = stops.iter().position(|s| s.name == "Ver");
        let switch = stops
            .iter()
            .position(|s| s.name == "Programas en Launchpad y Aplicaciones");
        let (Some(see), Some(switch)) = (see, switch) else {
            panic!("both controls are stops: {stops:?}");
        };
        assert!(
            stops[see].at.0 < stops[switch].at.0,
            "«Ver» is drawn to the left of the switch: {stops:?}"
        );
        assert!(see < switch, "and Tab reaches it first: {stops:?}");
        // The switch stays at the right edge of the card, as the switches of the other rows.
        assert!(stops[switch].at.2 > 740, "{stops:?}");
        f.finish();
    }
);
