//! The bottle's page, end to end: what an unfinished bottle offers, where a failure shows up,
//! what a program tile says while it opens, and when a copy is refused. Each test checks what
//! the person can do (controls by the names this package gives them) and the app's state, not
//! the wording of explanations.

use super::driver::Flow;
use super::fixtures;
use crate::backend::own_bottles_dir;
use crate::{Action, BottleTab, Modal, Screen};
use std::time::Duration;

const LONG: Duration = Duration::from_secs(20);

/// A bottle whose folder has `drive_c` but no `system.reg`: a creation that was interrupted.
fn unfinished_bottle(name: &str) {
    std::fs::create_dir_all(own_bottles_dir().join(name).join("drive_c"))
        .expect("the unfinished bottle");
}

/// The place of a bottle in the app's list, by its name.
fn index_of(f: &Flow, name: &str) -> usize {
    f.app
        .bottles
        .iter()
        .position(|b| b.name == name)
        .unwrap_or_else(|| panic!("no bottle {name:?}\n{}", f.dump()))
}

flow_test!(
    an_unfinished_bottle_explains_itself_and_offers_a_way_out,
    |home| {
        fixtures::bottle_with("Terminada", &[]);
        unfinished_bottle("A medio crear");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        let i = index_of(&f, "A medio crear");
        assert!(!f.app.bottles[i].prefix_complete);
        f.act(Action::Select(i));
        f.frame();
        // The page has a way out of it: delete it (through the Trash dialog) or make another.
        assert!(f.shows("Eliminar…"), "{}", f.dump());
        assert!(f.shows("Crear otra botella"), "{}", f.dump());
        // Nothing can be installed or opened in it, and the reason is there to read.
        let b = f.app.bottles[i].clone();
        assert!(f.app.install_reason(&b).is_some());
        assert!(f.app.unfinished_reason(&b).is_some());
        // A finished bottle shows none of this.
        let ok = index_of(&f, "Terminada");
        f.act(Action::Select(ok));
        f.frame();
        assert!(!f.shows("Crear otra botella"), "{}", f.dump());
        assert!(f
            .app
            .unfinished_reason(&f.app.bottles[ok].clone())
            .is_none());

        f.act(Action::Select(i));
        f.frame();
        f.click("Crear otra botella");
        assert!(
            matches!(f.app.modal, Some(Modal::NewBottle)),
            "{}",
            f.dump()
        );
        f.key(eframe::egui::Key::Escape);
        f.frame();
        f.click("Eliminar…");
        assert!(
            matches!(&f.app.modal, Some(Modal::Delete(p)) if *p == b.path),
            "{}",
            f.dump()
        );
        // Asking only opens the dialog: the folder is still there.
        assert!(b.path.is_dir());
        f.finish();
    }
);

flow_test!(
    a_failure_of_a_listed_bottle_shows_on_the_overview_and_leads_to_its_diagnosis,
    |home| {
        let other = fixtures::bottle_with("Otra", &["Programa de prueba"]);
        let failing = fixtures::bottle_with("Con fallo", &["Programa de prueba"]);
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        let (a, b) = (index_of(&f, "Otra"), index_of(&f, "Con fallo"));
        // A program of that bottle failed to start (as a tile of the overview would report it:
        // the fake Wine cannot end a program with an error by its shortcut).
        f.act(Action::GoHome);
        f.app
            .fail(Some(&failing), "Programa de prueba no pudo abrirse.");
        f.act(Action::Select(a));
        f.act(Action::SetTab(BottleTab::Programs));
        f.act(Action::GoHome);
        f.frame();
        // The overview shows it (the bottle is in the list) with a way to the diagnosis.
        assert!(f.shows("Ver"), "{}", f.dump());
        assert!(f.shows_part("Con fallo"), "{}", f.dump());
        f.click("Ver");
        assert_eq!(f.app.screen, Screen::Bottle, "{}", f.dump());
        assert_eq!(f.app.selected, b, "it opens the bottle that failed");
        assert_eq!(f.app.tab, BottleTab::Diagnostics);
        assert_ne!(f.app.bottles[b].path, other);
        f.finish();
    }
);

flow_test!(a_program_tile_answers_from_the_click_on, |home| {
    fixtures::bottle_with("Mi botella", &["Programa de prueba"]);
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.act(Action::Select(0));
    f.frame();
    assert!(f.shows("Programa de prueba"), "{}", f.dump());
    let bottle = f.app.bottle().unwrap().path.clone();
    let file = f.app.bottle().unwrap().programs[0].path.clone();

    // The press registers the start on the same frame, before any worker answers.
    f.act(Action::RunProgram(0));
    assert!(
        f.app.opening.contains_key(&(bottle.clone(), file.clone())),
        "the press is remembered as opening: {}",
        f.dump()
    );
    f.wait_until("the program to run", LONG, |a| !a.running.is_empty());
    f.frame();
    assert!(f.shows("Programa de prueba, abierto"), "{}", f.dump());
    assert!(!f.shows("Programa de prueba, abriéndose"));

    // While the start is still being made, the tile says so (held here by hand, since the
    // worker answers faster than a frame can be looked at).
    f.act(Action::Quit(false));
    f.wait_until("the bottle to close", LONG, |a| {
        a.running.is_empty() && a.busy.is_empty()
    });
    f.frame();
    assert!(f.shows("Programa de prueba"), "{}", f.dump());
    f.app
        .opening
        .insert((bottle, file), std::time::Instant::now());
    f.frame();
    assert!(
        f.shows("Programa de prueba, abriéndose"),
        "the tile shows it is opening\n{}",
        f.dump()
    );
    assert!(!f.shows("Programa de prueba, abierto"));
    f.finish();
});

flow_test!(a_copy_of_a_bottle_with_programs_open_is_refused, |home| {
    fixtures::bottle_with("Mi botella", &["Programa de prueba"]);
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.act(Action::Select(0));
    // Closed: a copy may be made (only the decision is asked: no dialog opens here).
    assert!(f.app.export_target().is_some());
    f.act(Action::RunProgram(0));
    f.wait_until("the program to run", LONG, |a| !a.running.is_empty());
    assert!(f.app.export_target().is_none(), "{}", f.dump());
    assert!(
        f.app.status.contains("Cierra los programas"),
        "the person is told why: {:?}",
        f.app.status
    );
    f.act(Action::Quit(false));
    // The bottle is in use until the look at its wineserver (every few seconds) says it is not.
    let bottle = f.app.bottles[0].path.clone();
    f.wait_until("the bottle to close", LONG, |a| {
        a.running.is_empty() && a.busy.is_empty() && !a.bottle_in_use(&bottle)
    });
    assert!(f.app.export_target().is_some(), "{}", f.dump());
    f.finish();
});

flow_test!(
    nothing_starts_wine_in_an_unfinished_bottle_by_any_path,
    |home| {
        fixtures::bottle_with("Terminada", &["Programa de prueba"]);
        let broken = fixtures::bottle_with("A medio crear", &["Programa de prueba"]);
        // It has a program and a drive, but its registry never got written.
        std::fs::remove_file(broken.join("system.reg")).expect("the registry file");
        let installer = fixtures::installer(home, "instalador-prueba.exe");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        let (ok, i) = (index_of(&f, "Terminada"), index_of(&f, "A medio crear"));
        assert!(!f.app.bottles[i].prefix_complete);
        // The live looks at the bottles are questions, not Wine starting: they are not counted.
        let before = f.fake.acts().len();

        // The tile of the overview (Action::RunFrom), the tile of the page, a Windows tool, and
        // the two settings that run Wine in the bottle.
        f.act(Action::GoHome);
        f.act(Action::RunFrom(i, 0));
        f.act(Action::Select(i));
        f.act(Action::RunProgram(0));
        f.act(Action::Tool(crate::backend::Tool::ALL[0]));
        f.act(Action::SetWindows("win7".into()));
        f.act(Action::SetHighRes(true));
        f.settle(Duration::from_millis(500));
        assert!(f.app.opening.is_empty(), "{}", f.dump());
        assert!(f.app.busy.is_empty(), "{}", f.dump());
        assert_eq!(f.fake.acts().len(), before, "Wine was not started");

        // A file handed over while that bottle is open is not offered that bottle as a target.
        f.act(Action::Drop(installer));
        f.frame();
        let draft = f.app.install_draft.as_ref().expect("the install dialog");
        assert!(draft
            .options
            .iter()
            .all(|o| o.path != f.app.bottles[i].path));
        assert!(draft
            .options
            .iter()
            .any(|o| o.path == f.app.bottles[ok].path));
        f.finish();
    }
);

// Every button that asks for a file or a place with a native dialog does nothing under test:
// no window opens and nothing waits for a click that never comes.
flow_test!(buttons_that_ask_for_a_file_open_no_native_dialog, |home| {
    fixtures::bottle_with("Mi botella", &["Programa de prueba"]);
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.act(Action::Select(0));
    let before = f.fake.acts().len();
    for action in [
        Action::Install,
        Action::PickExe,
        Action::Export,
        Action::Import,
        Action::PickWine,
        Action::ExportReport,
    ] {
        f.act(action);
        f.frame();
    }
    f.settle(Duration::from_millis(300));
    assert!(f.app.install_draft.is_none(), "{}", f.dump());
    assert!(f.app.install.is_none(), "{}", f.dump());
    assert!(f.app.busy.is_empty(), "{}", f.dump());
    assert!(f.app.opening.is_empty(), "{}", f.dump());
    assert_eq!(f.fake.acts().len(), before, "nothing was started");
    f.finish();
});

flow_test!(
    controls_held_by_a_job_stay_reachable_with_tab_and_say_why,
    |home| {
        unfinished_bottle("A medio crear");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        let i = index_of(&f, "A medio crear");
        f.act(Action::Select(i));
        f.act(Action::SetTab(BottleTab::Settings));
        f.advance(1.0);
        f.frame();
        // The switch and the Windows version cannot be changed in a bottle that was never
        // finished; both still take the focus, so Tab and a screen reader reach the reason.
        let mut reached = vec![];
        for _ in 0..60 {
            f.key_with(eframe::egui::Key::Tab, eframe::egui::Modifiers::NONE);
            f.advance(1.0);
            f.frame();
            f.frame();
            if let Some(name) = f.focused() {
                reached.push(name);
            }
        }
        assert!(
            reached.iter().any(|n| n == "Alta resolución"),
            "{reached:?}\n{}",
            f.tree()
        );
        assert!(
            reached.iter().any(|n| n.starts_with("Versión de Windows")),
            "{reached:?}\n{}",
            f.tree()
        );
        f.finish();
    }
);
