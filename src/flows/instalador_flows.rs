//! The guided installer's card, end to end against the fake Wine: what its steps claim when an
//! installation ends without a result, what stopping the installer does, how fast the result
//! shows, what happens to a file dropped behind a dialog and to a worker that panics. They
//! check app state and the marks of the steps, not the words of the card.

use super::driver::Flow;
use super::fixtures;
use crate::domain::install::{Outcome, Stage, StepMark};
use crate::{Action, Modal, Msg, OnPanic, WORKER_FAILED_TEXT};
use std::time::Duration;

const LONG: Duration = Duration::from_secs(20);

/// Drops `file` on the app, confirms the dialog and waits for the installer to be open.
fn start_installer_of(f: &mut Flow, file: std::path::PathBuf) {
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.act(Action::Drop(file));
    f.act(Action::ConfirmInstall);
    f.wait_until("the installer to be open", LONG, |a| {
        a.install
            .as_ref()
            .is_some_and(|s| s.stage == Stage::InstallerOpen)
    });
}

flow_test!(
    an_installer_that_ends_with_nothing_new_shows_no_done_mark_for_the_search,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let installer = fixtures::installer(home, "instalador-falla.exe");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Drop(installer));
        f.act(Action::ConfirmInstall);
        f.wait_until("the installation to end", LONG, |a| {
            a.install.as_ref().is_some_and(|s| !s.active())
        });
        let session = f.app.install.clone().unwrap();
        assert!(
            matches!(session.stage, Stage::Done(Outcome::NothingNew { .. })),
            "{}",
            f.dump()
        );
        let steps = session.steps();
        let (installer_row, search_row) = (&steps[steps.len() - 2], &steps[steps.len() - 1]);
        assert_eq!(installer_row.0, StepMark::Done, "the installer did run");
        assert_ne!(
            search_row.0,
            StepMark::Done,
            "the search found nothing, so it is no finished step: {steps:?}"
        );
        // The card is on screen, and it says nothing else is done than what happened.
        f.frame();
        assert!(f.shows(&search_row.1), "{}", f.dump());
        f.finish();
    }
);

flow_test!(
    closing_everything_in_the_bottle_stops_the_installer_with_honest_steps,
    |home| {
        fixtures::bottle_with("Mi botella", &["Programa de prueba"]);
        let installer = fixtures::installer(home, "instalador-lento.exe");
        let mut f = Flow::start(home);
        start_installer_of(&mut f, installer);
        f.frame();
        assert!(
            f.shows("Cerrar todo en esta botella"),
            "the button says what it closes\n{}",
            f.dump()
        );
        assert!(!f.shows("Cerrar el instalador"));
        // Only the installer runs there: the card does not warn about other programs.
        assert!(!f.shows_part("También se cierran"), "{}", f.dump());
        // Another program of the same bottle is open: now the card says it closes too.
        let i = f
            .app
            .bottle()
            .unwrap()
            .programs
            .iter()
            .position(|p| p.name == "Programa de prueba")
            .expect("the program of the bottle");
        f.act(Action::RunProgram(i));
        f.wait_until("both programs to run", LONG, |a| a.running.len() == 2);
        f.frame();
        assert!(f.shows_part("También se cierran"), "{}", f.dump());

        f.act(Action::CloseInstaller);
        f.wait_until("the installation to end", LONG, |a| {
            a.install.as_ref().is_some_and(|s| !s.active())
        });
        let session = f.app.install.clone().unwrap();
        assert_eq!(session.stage, Stage::Done(Outcome::Stopped), "{}", f.dump());
        let steps = session.steps();
        let search = steps.last().unwrap();
        assert_ne!(search.0, StepMark::Done, "nothing was searched: {steps:?}");
        assert!(
            steps.iter().all(|(_, label)| !label.contains("terminó")),
            "the installer did not finish, it was closed: {steps:?}"
        );
        // The fake programs end when the close reaches them, a moment after the card has
        // settled: wait for that (bounded) instead of looking once.
        f.wait_for_fake("everything in the bottle to end", LONG, |n| n == 0);
        // The close job lets go of the bottle when its last step is read, a little later still.
        f.wait_until("the bottle to be free", LONG, |a| a.busy.is_empty());
        assert!(f.app.busy.is_empty(), "{}", f.dump());
        f.finish();
    }
);

flow_test!(
    a_file_dropped_behind_a_dialog_is_offered_when_the_dialog_closes,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let installer = fixtures::installer(home, "instalador-prueba.exe");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::OpenNewBottle);
        f.frame();
        f.act(Action::Drop(installer.clone()));
        assert_eq!(f.app.pending_open.as_deref(), Some(installer.as_path()));
        assert!(f.app.install_draft.is_none());
        // While the dialog stays open the file waits.
        f.settle(Duration::from_millis(300));
        assert!(matches!(f.app.modal, Some(Modal::NewBottle)));
        assert_eq!(f.app.pending_open.as_deref(), Some(installer.as_path()));
        f.act(Action::CloseModal);
        f.wait_until("the file to be offered", LONG, |a| {
            matches!(a.modal, Some(Modal::Install))
        });
        let draft = f.app.install_draft.as_ref().expect("the install dialog");
        assert_eq!(draft.file, installer);
        assert!(f.app.pending_open.is_none());
        f.finish();
    }
);

// A file that waits behind a dialog is not lost when the person confirms the first install:
// it stays queued while the installation runs and opens its own dialog once that ends.
flow_test!(
    a_file_queued_behind_a_dialog_survives_the_install_the_dialog_started,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let first = fixtures::installer(home, "instalador-lento.exe");
        let second = fixtures::installer(home, "instalador-prueba.exe");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Drop(first));
        f.act(Action::Drop(second.clone()));
        assert_eq!(f.app.pending_open.as_deref(), Some(second.as_path()));
        f.act(Action::ConfirmInstall);
        f.wait_until("the installer to be open", LONG, |a| {
            a.install
                .as_ref()
                .is_some_and(|s| s.stage == Stage::InstallerOpen)
        });
        // The installation runs, the queued file waits, and no dialog opens over the card.
        f.settle(Duration::from_millis(500));
        assert_eq!(f.app.pending_open.as_deref(), Some(second.as_path()));
        assert!(f.app.modal.is_none(), "{}", f.dump());
        // When the installation is over, the queued file gets its dialog.
        f.act(Action::CloseInstaller);
        f.wait_until("the queued file to be offered", LONG, |a| {
            matches!(a.modal, Some(Modal::Install))
        });
        assert_eq!(f.app.install_draft.as_ref().unwrap().file, second);
        assert!(f.app.pending_open.is_none());
        f.finish();
    }
);

// One file waits at a time, and a later one never takes its place, whether the waiting one came
// from Finder or from a drop; the person is told to bring the later one again.
flow_test!(
    a_later_file_does_not_replace_the_one_already_waiting,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let first = fixtures::installer(home, "instalador-prueba.exe");
        let from_finder = fixtures::installer(home, "instalador-lento.exe");
        let dropped = fixtures::installer(home, "instalador-falla.exe");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Drop(first));
        assert!(matches!(f.app.modal, Some(Modal::Install)));
        crate::app::hand_over(&crate::app::requests_dir(), Some(&from_finder)).unwrap();
        f.wait_until("the Finder file to wait", LONG, |a| {
            a.pending_open.is_some()
        });
        assert_eq!(f.app.pending_open.as_deref(), Some(from_finder.as_path()));
        f.act(Action::Drop(dropped));
        assert_eq!(
            f.app.pending_open.as_deref(),
            Some(from_finder.as_path()),
            "{}",
            f.dump()
        );
        assert!(
            f.app.status.contains("instalador-lento.exe"),
            "{}",
            f.dump()
        );
        f.finish();
    }
);

// While the installation waits for Wine, the card's button says «Cancelar» and does cancel: the
// installer is never opened afterwards, and the person is told so.
flow_test!(
    the_button_of_a_card_waiting_for_wine_cancels_and_says_so,
    |home| {
        let installer = fixtures::installer(home, "instalador-prueba.exe");
        let mut f = Flow::start_without_wine(home);
        f.wait_until("the search for Wine to end", LONG, |a| {
            !a.errors.is_empty() && !a.installing()
        });
        f.act(Action::Drop(installer));
        f.act(Action::ConfirmInstall);
        f.frame();
        let stage = f.app.install.as_ref().map(|s| s.stage.clone());
        assert_eq!(stage, Some(Stage::WaitingWine), "{}", f.dump());
        assert!(f.shows("Cancelar"), "{}", f.dump());
        assert!(!f.shows("Ocultar este aviso"), "{}", f.dump());
        f.act(Action::DismissInstall);
        assert!(f.app.install.is_none());
        assert!(f.app.status.contains("no se abrirá"), "{}", f.dump());
        f.finish();
    }
);

// A worker that panicked while it held the install's progress leaves that lock poisoned. The
// window thread reads the progress while it waits for Wine, and it must read it anyway (the
// way every other reader does) instead of panicking and taking the whole window down.
flow_test!(
    a_poisoned_install_progress_is_read_while_waiting_for_wine,
    |home| {
        let installer = fixtures::installer(home, "instalador-prueba.exe");
        let mut f = Flow::start_without_wine(home);
        f.wait_until("the search for Wine to end", LONG, |a| {
            !a.errors.is_empty() && !a.installing()
        });
        f.act(Action::Drop(installer));
        f.act(Action::ConfirmInstall);
        f.frame();
        let shared = f.app.setup.clone();
        let poisoner = std::thread::spawn(move || {
            let mut held = shared.lock().unwrap();
            held.step = Some(crate::wine_setup::Step::Done);
            panic!("a worker died holding the install progress");
        });
        assert!(poisoner.join().is_err());
        assert!(f.app.setup.is_poisoned());
        // The install that ended well is about to deliver its result, so the card keeps waiting:
        // it only does so if the step the dead worker left behind was read.
        f.app.follow_install();
        let stage = f.app.install.as_ref().map(|s| s.stage.clone());
        assert_eq!(stage, Some(Stage::WaitingWine), "{}", f.dump());
        f.finish();
    }
);

// «Cerrar todo» while another job holds the bottle is refused and closes nothing, so the card
// must not behave as if the person had closed the installer.
flow_test!(
    a_refused_close_does_not_mark_the_installation_as_closing,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let installer = fixtures::installer(home, "instalador-lento.exe");
        let mut f = Flow::start(home);
        start_installer_of(&mut f, installer);
        let bottle = f.app.install.as_ref().unwrap().bottle.clone();
        f.app.busy.insert(bottle.clone(), "Preparando…".into());
        f.act(Action::CloseInstaller);
        assert!(!f.app.install_stopping, "{}", f.dump());
        assert_eq!(
            f.app.busy.get(&bottle).map(String::as_str),
            Some("Preparando…"),
            "the other job's mark is left as it was"
        );
        // The installer still runs, and the card still says so.
        f.settle(Duration::from_millis(300));
        assert_eq!(
            f.app.install.as_ref().unwrap().stage,
            Stage::InstallerOpen,
            "{}",
            f.dump()
        );
        assert!(!f.fake.alive().is_empty(), "nothing was closed");
        // Once the bottle is free, the same press does close it.
        f.app.busy.remove(&bottle);
        f.act(Action::CloseInstaller);
        assert!(f.app.install_stopping, "{}", f.dump());
        f.wait_until("the installation to end", LONG, |a| {
            a.install.as_ref().is_some_and(|s| !s.active())
        });
        f.finish();
    }
);

// The hint on the status line names every kind of file the picker offers, no more and no fewer.
flow_test!(the_hint_and_the_picker_name_the_same_files, |home| {
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    let hint = f.app.idle_hint();
    let named: Vec<&str> = hint
        .split(' ')
        .map(|w| w.trim_matches(['(', ')', ',']))
        .filter_map(|w| w.strip_prefix('.'))
        .filter(|e| !e.is_empty())
        .collect();
    assert_eq!(
        named,
        crate::domain::install::INSTALLER_EXTENSIONS,
        "the hint says «{hint}»"
    );
    f.finish();
});

flow_test!(
    the_result_of_an_installation_shows_within_two_looks_at_the_bottle,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let installer = fixtures::installer(home, "instalador-falla.exe");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Drop(installer));
        f.act(Action::ConfirmInstall);
        f.wait_until("the first process to end", LONG, |a| {
            a.install
                .as_ref()
                .is_some_and(|s| s.stage == Stage::Settling)
        });
        // The slow look at all bottles (every 3 s of the app's clock) is put out of the picture,
        // so the result can only come from the installation's own, quicker look at its bottle.
        // That way the test does not depend on how fast the machine runs: a wall-clock limit
        // fails on a busy machine although the feature works.
        f.app.last_live_check = f64::MAX / 2.0;
        f.wait_until("the result", LONG, |a| {
            a.install.as_ref().is_some_and(|s| !s.active())
        });
        f.finish();
    }
);

flow_test!(
    a_worker_that_panics_preparing_the_installer_fails_its_card,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        // The fake name makes the worker that starts it panic (see `run_start_job`).
        let installer = fixtures::installer(home, "instalador-se-rompe.exe");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Drop(installer));
        f.act(Action::ConfirmInstall);
        f.wait_until("the card to fail", LONG, |a| {
            a.install.as_ref().is_some_and(|s| !s.active())
        });
        let session = f.app.install.clone().unwrap();
        assert_eq!(
            session.stage,
            Stage::Done(Outcome::Failed(WORKER_FAILED_TEXT.into())),
            "{}",
            f.dump()
        );
        assert!(session.card_actions().dismiss);
        // The card says it: no second banner, and the bottle is free again.
        f.wait_until("the bottle to be free", LONG, |a| {
            a.busy.is_empty() && a.opening.is_empty()
        });
        f.settle(Duration::from_millis(300));
        assert!(f.app.errors.is_empty(), "{}", f.dump());
        assert!(f.app.busy.is_empty(), "{}", f.dump());
        // The start is over, so the double-start guard let go of the file too.
        assert!(f.app.opening.is_empty(), "{}", f.dump());
        f.finish();
    }
);

// After a worker that starts the installer panicked, the same file can be tried again at once:
// the guard does not answer «ya se está abriendo» for the next three minutes.
flow_test!(
    a_second_try_after_a_panicking_start_is_not_held_back_by_the_guard,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let installer = fixtures::installer(home, "instalador-se-rompe.exe");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Drop(installer.clone()));
        f.act(Action::ConfirmInstall);
        f.wait_until("the card to fail", LONG, |a| {
            a.install.as_ref().is_some_and(|s| !s.active())
        });
        f.act(Action::DismissInstall);
        // The worker's report frees the start mark of the file; the second try must find it free.
        f.wait_until("the start mark to be freed", LONG, |a| {
            a.opening.is_empty() && a.busy.is_empty()
        });
        // The same file again: the start is tried (and breaks again), not refused by the guard.
        f.act(Action::Drop(installer));
        f.act(Action::ConfirmInstall);
        f.wait_until("the second card to fail", LONG, |a| {
            a.install.as_ref().is_some_and(|s| !s.active())
        });
        let session = f.app.install.clone().unwrap();
        assert_eq!(
            session.stage,
            Stage::Done(Outcome::Failed(WORKER_FAILED_TEXT.into())),
            "the second try reached the worker: {}",
            f.dump()
        );
        assert!(
            !f.app.status.contains("ya se está abriendo"),
            "{}",
            f.dump()
        );
        f.finish();
    }
);

// The message a panicking worker sends reaches the installation it names, and no other.
flow_test!(
    a_report_of_another_worker_does_not_fail_the_installation,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let installer = fixtures::installer(home, "instalador-lento.exe");
        let mut f = Flow::start(home);
        start_installer_of(&mut f, installer);
        let id = f.app.install.as_ref().unwrap().id;
        let other = OnPanic {
            install: Some(id + 1),
            ..OnPanic::default()
        };
        f.app.tx.send(Msg::WorkerFailed(other)).unwrap();
        f.settle(Duration::from_millis(300));
        assert_eq!(
            f.app.install.as_ref().unwrap().stage,
            Stage::InstallerOpen,
            "{}",
            f.dump()
        );
        f.finish();
    }
);

flow_test!(
    the_name_of_a_new_bottle_takes_the_keyboard_when_it_appears,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let installer = fixtures::installer(home, "instalador-prueba.exe");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Drop(installer));
        f.wait_until("the install dialog", LONG, |a| {
            matches!(a.modal, Some(Modal::Install))
        });
        f.settle(Duration::from_millis(200));
        // The proposal is an existing bottle: there is no name field yet.
        assert!(matches!(f.app.modal, Some(Modal::Install)));
        assert!(!f.shows("Nombre de la botella"), "{}", f.dump());
        // The person chooses «Nueva botella»: the field appears, named, with the cursor in it.
        f.app.install_draft.as_mut().unwrap().choice = None;
        f.frame();
        f.frame();
        assert!(f.shows("Nombre de la botella"), "{}", f.dump());
        assert_eq!(f.focused().as_deref(), Some("Nombre de la botella"));
        // Typing goes into it, and Enter with the cursor there confirms.
        f.type_text("X");
        assert!(f.app.install_draft.as_ref().unwrap().name.ends_with('X'));
        f.key(eframe::egui::Key::Enter);
        f.wait_until("the installation to start", LONG, |a| a.install.is_some());
        // The new bottle is still being made and the installer has yet to run: let the
        // installation end, so no fake Wine is left writing into this home after it is removed.
        f.wait_until("the installation to end", LONG, |a| {
            a.install.as_ref().is_some_and(|s| !s.active())
        });
        f.finish();
    }
);

flow_test!(
    the_name_field_takes_the_keyboard_again_when_a_queued_file_reopens_the_dialog,
    |home| {
        // No bottle: the proposal is a new one, so the name field is there from the start.
        let first = fixtures::installer(home, "instalador-prueba.exe");
        let second = fixtures::installer(home, "instalador-lento.exe");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Drop(first));
        f.frame();
        f.frame();
        assert!(matches!(f.app.modal, Some(Modal::Install)));
        assert_eq!(f.focused().as_deref(), Some("Nombre de la botella"));
        // A second file waits behind the dialog; the person closes it, and the queued file
        // opens a new dialog in the same frame, with no frame in between that shows no dialog.
        f.act(Action::Drop(second.clone()));
        assert_eq!(f.app.pending_open.as_deref(), Some(second.as_path()));
        // The person tabbed to the buttons before pressing «Cancelar».
        f.key(eframe::egui::Key::Tab);
        assert_ne!(f.focused().as_deref(), Some("Nombre de la botella"));
        f.act(Action::CloseModal);
        f.frame();
        f.frame();
        assert!(matches!(f.app.modal, Some(Modal::Install)));
        assert_eq!(f.app.install_draft.as_ref().unwrap().file, second);
        assert_eq!(f.focused().as_deref(), Some("Nombre de la botella"));
        f.finish();
    }
);
