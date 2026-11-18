//! The main paths of the app, end to end against the fake Wine. Each test checks outcomes
//! (a bottle exists, a program was found, a process was reaped), not the words of a step, so a
//! change of wording or layout does not break them.

use super::driver::Flow;
use super::fake_wine::zombie_children;
use super::fixtures;
use crate::backend::{data_dir, own_bottles_dir};
use crate::domain::install::{Outcome, Stage};
use crate::Action;
use eframe::egui::Key;
use std::time::Duration;

const LONG: Duration = Duration::from_secs(20);

/// The place of the program in the shown bottle's list.
fn program_index(f: &Flow, name: &str) -> usize {
    f.app
        .bottle()
        .and_then(|b| b.programs.iter().position(|p| p.name == name))
        .unwrap_or_else(|| panic!("no program {name:?}\n{}", f.dump()))
}

/// Waits for a finished installation (whatever its outcome) and gives its stage.
fn wait_for_end_of_install(f: &mut Flow) -> Stage {
    f.wait_until("the installation to end", LONG, |a| {
        a.install
            .as_ref()
            .is_some_and(|s| matches!(s.stage, Stage::Done(_)))
    });
    f.app.install.as_ref().unwrap().stage.clone()
}

flow_test!(
    first_start_without_wine_offers_it_and_downloads_nothing,
    |home| {
        let mut f = Flow::start_without_wine(home);
        // The app looks for Wine, finds none, tries to install it by itself and is refused: the
        // guard of `download` answers at once, so the error is what proves nothing was fetched.
        f.wait_until("the install attempt to fail", LONG, |a| {
            !a.errors.is_empty() && !a.installing()
        });
        f.wait_for("the offer to install Wine", LONG, |f| {
            f.shows("Instalar Wine gratis")
        });
        // Nothing may have started in the meantime: that part is looked at after a while.
        f.settle(Duration::from_millis(300));
        assert!(f.app.bottles.is_empty(), "{}", f.dump());
        assert!(f.app.wine.is_none());
        assert_eq!(f.app.wine_state, crate::wine_setup::WineState::Missing);
        assert!(f.shows("Instalar Wine gratis"), "{}", f.dump());
        assert!(
            f.app.errors.iter().any(|e| e.text.contains("no descargan")),
            "{}",
            f.dump()
        );
        // Nothing was downloaded or installed, and no Wine was asked anything.
        let wine_dir = data_dir().join("Wine");
        assert!(
            !wine_dir.exists() || std::fs::read_dir(&wine_dir).unwrap().next().is_none(),
            "an engine appeared in {wine_dir:?}"
        );
        let downloads = data_dir().join("descarga");
        assert!(
            !downloads.exists() || std::fs::read_dir(&downloads).unwrap().next().is_none(),
            "something was downloaded"
        );
        assert!(f.fake.calls().is_empty());
        f.finish();
    }
);

flow_test!(
    creating_a_bottle_leaves_it_complete_selected_and_free,
    |home| {
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::OpenNewBottle);
        f.frame();
        assert!(f.app.modal.is_some(), "the new-bottle dialog opens");
        f.act(Action::CreateBottle);
        assert!(f.app.modal.is_none(), "the dialog closes at once");
        f.wait_until("the bottle to be made", LONG, |a| {
            a.busy.is_empty() && a.bottles.first().is_some_and(|b| b.prefix_complete)
        });
        assert_eq!(f.app.bottles.len(), 1);
        let bottle = f.app.bottle().expect("a bottle is selected").clone();
        assert_eq!(bottle.name, "Mi botella");
        assert_eq!(bottle.path, own_bottles_dir().join("Mi botella"));
        assert!(bottle.path.join("drive_c").is_dir());
        assert!(bottle.path.join("system.reg").is_file());
        assert!(f.app.busy.is_empty());
        assert!(f.app.errors.is_empty(), "{}", f.dump());
        assert_eq!(f.fake.count("wineboot --init"), 1);
        f.finish();
    }
);

flow_test!(the_guided_install_finds_the_new_program, |home| {
    fixtures::bottle_with("Mi botella", &[]);
    let installer = fixtures::installer(home, "instalador-prueba.exe");
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.act(Action::Drop(installer));
    assert!(
        f.app.install_draft.is_some(),
        "the install dialog asks first"
    );
    assert!(
        f.app.install.is_none(),
        "nothing starts before the person confirms"
    );
    f.act(Action::ConfirmInstall);
    let stage = wait_for_end_of_install(&mut f);
    assert_eq!(stage, Stage::Done(Outcome::Installed), "{}", f.dump());
    let found: Vec<_> = f
        .app
        .install
        .as_ref()
        .unwrap()
        .found
        .iter()
        .map(|p| p.name.clone())
        .collect();
    assert_eq!(found, ["Programa de prueba"]);
    // The bottle's own list has it too, with the file it runs.
    let program = &f.app.bottle().unwrap().programs[program_index(&f, "Programa de prueba")];
    assert!(program.target.as_ref().is_some_and(|t| t.is_file()));
    // The installer's process was waited for: no zombie child is left. The reaping and the
    // end of the job come a little after the card says it is over, so both are waited for.
    f.wait_for("the installer's process to be reaped", LONG, |_| {
        zombie_children().is_empty()
    });
    f.wait_until("the bottle to be free", LONG, |a| a.busy.is_empty());
    assert_eq!(zombie_children(), Vec::<u32>::new());
    assert!(f.app.busy.is_empty());
    f.finish();
});

flow_test!(
    a_failing_installer_ends_without_a_program_and_offers_the_diagnosis,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let installer = fixtures::installer(home, "instalador-falla.exe");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Drop(installer));
        f.act(Action::ConfirmInstall);
        wait_for_end_of_install(&mut f);
        let session = f.app.install.clone().unwrap();
        assert!(session.found.is_empty(), "{}", f.dump());
        assert_eq!(session.exit, Some(Some(3)), "{}", f.dump());
        assert!(
            session.card_actions().diagnose,
            "the card offers the diagnosis"
        );
        assert!(
            f.app.bottle().unwrap().programs.is_empty(),
            "a failed installer adds no program"
        );
        f.wait_for("the installer's process to be reaped", LONG, |_| {
            zombie_children().is_empty()
        });
        assert_eq!(zombie_children(), Vec::<u32>::new());
        f.finish();
    }
);

flow_test!(a_program_opens_shows_as_open_and_closes_for_good, |home| {
    fixtures::bottle_with("Mi botella", &["Programa de prueba"]);
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.act(Action::Select(0));
    let i = program_index(&f, "Programa de prueba");
    f.act(Action::RunProgram(i));
    f.wait_until("the program to run", LONG, |a| !a.running.is_empty());
    let bottle = f.app.bottle().unwrap().path.clone();
    assert_eq!(f.app.running_in(&bottle), ["Programa de prueba"]);
    f.wait_for_fake("one fake program to run", LONG, |n| n == 1);
    f.frame();
    assert!(
        f.shows_part("abierto"),
        "the tile says it is open\n{}",
        f.dump()
    );

    f.act(Action::Quit(false));
    f.wait_until("the bottle to close", LONG, |a| {
        a.running.is_empty() && a.busy.is_empty()
    });
    f.wait_for_fake("no fake program to remain", LONG, |n| n == 0);
    assert_eq!(f.fake.count("--end-session"), 1);
    f.wait_for("the closed programs to be reaped", LONG, |_| {
        zombie_children().is_empty()
    });
    assert_eq!(zombie_children(), Vec::<u32>::new());
    assert!(f.app.running_in(&bottle).is_empty());
    assert!(f.app.busy.is_empty());
    f.finish();
});

flow_test!(a_second_press_does_not_start_a_second_process, |home| {
    fixtures::bottle_with("Mi botella", &["Programa de prueba"]);
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.act(Action::Select(0));
    let i = program_index(&f, "Programa de prueba");
    // Two presses before any frame runs: the second finds the first still opening.
    f.act(Action::RunProgram(i));
    f.act(Action::RunProgram(i));
    f.wait_until("the program to run", LONG, |a| !a.running.is_empty());
    // The fake Wine records a start a little after the app follows it.
    f.wait_for("the one start to be recorded", LONG, |f| {
        f.fake.count("start /wait /unix") >= 1
    });
    // A third press while it is open is refused too: no second start comes, however long it
    // is looked at.
    f.act(Action::RunProgram(i));
    f.settle(Duration::from_millis(600));
    assert_eq!(f.app.running.len(), 1, "{}", f.dump());
    assert_eq!(f.fake.count("start /wait /unix"), 1, "{}", f.dump());
    f.wait_for_fake("one fake program to run", LONG, |n| n == 1);
    f.finish();
});

flow_test!(
    the_keyboard_reaches_named_controls_and_escape_closes_the_dialog,
    |home| {
        fixtures::bottle_with("Mi botella", &["Programa de prueba"]);
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Select(0));
        f.wait_for("the bottle's page", LONG, |f| f.shows("Programa de prueba"));
        f.settle(Duration::from_millis(200));
        let mut stops = vec![];
        for _ in 0..40 {
            f.key(Key::Tab);
            stops.push(f.focused());
        }
        // Every stop is a control with a name (a screen reader says it), and Tab never stays put.
        for (n, stop) in stops.iter().enumerate() {
            assert!(stop.is_some(), "Tab stop {n} has no name: {stops:?}");
        }
        let distinct: std::collections::BTreeSet<_> = stops.iter().flatten().collect();
        assert!(distinct.len() >= 5, "focus reached only {distinct:?}");
        // The ring closes: after the last control the focus comes back to the first.
        assert!(
            stops[1..].contains(&stops[0]),
            "Tab never wrapped: {stops:?}"
        );
        for pair in stops.windows(2) {
            assert_ne!(
                pair[0], pair[1],
                "Tab got stuck on {:?}: {stops:?}",
                pair[0]
            );
        }

        f.act(Action::OpenNewBottle);
        f.frame();
        assert!(f.app.modal.is_some());
        f.key(Key::Escape);
        f.frame();
        assert!(f.app.modal.is_none(), "Escape closes the new-bottle dialog");
        f.finish();
    }
);

/// Installs "SteamSetup.exe" (an installer with fixes of its own, which asks for a bottle made
/// for it) and answers whether the app recorded its fixes as applied, and what the fake registry received.
fn steam_installer_fixes(home: &std::path::Path, registry_fails: bool) -> (bool, String) {
    let installer = fixtures::installer(home, "SteamSetup.exe");
    let mut f = Flow::start(home);
    if registry_fails {
        f.fake.fail_registry_writes();
    }
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.act(Action::Drop(installer.clone()));
    f.act(Action::ConfirmInstall);
    wait_for_end_of_install(&mut f);
    let bottle = f.app.install.as_ref().unwrap().bottle.clone();
    assert!(
        bottle.join("system.reg").is_file(),
        "the bottle was made first"
    );
    // The installer itself ran in both cases: a fix that cannot be written does not block it.
    assert_eq!(f.fake.count("SteamSetup.exe"), 1, "{}", f.dump());
    let id = crate::backend::recipes::for_installer(&installer)
        .unwrap()
        .id;
    // The bottle remembers the fixes when the worker's report is read: that must happen when
    // they were written, and must not when they were not.
    if !registry_fails {
        f.wait_for("the fixes to be recorded", LONG, |f| {
            f.app
                .cfg
                .bottle_ref(&bottle)
                .is_some_and(|s| s.recipes.contains(&id))
        });
    }
    f.settle(Duration::from_millis(300));
    let recorded = f
        .app
        .cfg
        .bottle_ref(&bottle)
        .is_some_and(|s| s.recipes.contains(&id));
    let written = std::fs::read_to_string(bottle.join("registro-falso.txt")).unwrap_or_default();
    f.finish();
    (recorded, written)
}

flow_test!(fixes_are_recorded_only_when_they_were_written, |home| {
    let (recorded, written) = steam_installer_fixes(home, false);
    assert!(recorded, "the fixes were written, so they count as applied");
    assert!(written.contains("reg add"), "{written:?}");
});

flow_test!(fixes_that_cannot_be_written_are_not_recorded, |home| {
    let (recorded, written) = steam_installer_fixes(home, true);
    assert!(!recorded, "a fix that failed must be tried again next time");
    assert!(written.is_empty(), "{written:?}");
});

flow_test!(
    the_mouse_and_typing_make_a_bottle_from_the_sidebar,
    |home| {
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.click("Nueva botella");
        f.wait_until("the dialog", LONG, |a| a.modal.is_some());
        f.wait_for("the name field to have the focus", LONG, |f| {
            f.focused().as_deref() == Some("Nombre de la botella")
        });
        assert!(f.app.modal.is_some(), "{}", f.dump());
        // The name field has the keyboard focus as soon as the dialog opens: typing goes into it.
        assert_eq!(f.focused().as_deref(), Some("Nombre de la botella"));
        f.type_text("X");
        assert_eq!(f.app.new_name, "Mi botellaX");
        f.click("Crear botella");
        f.wait_until("the bottle to be made", LONG, |a| {
            a.busy.is_empty() && a.bottles.first().is_some_and(|b| b.prefix_complete)
        });
        assert_eq!(f.app.bottle().unwrap().name, "Mi botellaX");
        assert!(f.app.modal.is_none());
        f.finish();
    }
);
