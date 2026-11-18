//! The programs area: what a Finder open does with an unfinished bottle, what a panicking start
//! leaves behind, and what an engine change or a saved copy does to a bottle another job holds.
//! Checks state and outcomes, not wording.

use super::driver::Flow;
use super::fake_wine::FakeWine;
use super::fixtures;
use crate::app::export_job;
use crate::backend::{own_bottles_dir, Log};
use crate::wine_setup::Engine;
use crate::{Action, Msg, OnPanic};
use std::time::Duration;

const LONG: Duration = Duration::from_secs(20);

/// A bottle with a drive and no registry file: its creation was interrupted.
fn unfinished(name: &str) -> std::path::PathBuf {
    let path = fixtures::bottle_with(name, &[]);
    std::fs::remove_file(path.join("system.reg")).expect("the registry file");
    path
}

// A file opened from the Finder with the app closed never starts Wine in a bottle that was not
// finished: the window is left to open and explain.
flow_test!(
    a_finder_open_never_starts_wine_in_an_unfinished_bottle,
    |home| {
        let fake = FakeWine::write(home);
        unfinished("Mi botella");
        let file = fixtures::installer(home, "instalador-lento.exe");
        assert!(!crate::app::open_without_window(&file));
        assert_eq!(fake.count("instalador-lento.exe"), 0, "{:?}", fake.calls());
        fake.end_all();
    }
);

// Without a main bottle the file goes to the first of the app's own bottles. When that one never
// finished being made, nothing runs and no other bottle is picked behind the person's back: the
// window opens instead and explains (see bottle_for_open).
flow_test!(a_finder_open_never_runs_in_an_unfinished_bottle, |home| {
    let fake = FakeWine::write(home);
    unfinished("A medio crear");
    fixtures::bottle_with("Terminada", &[]);
    let file = fixtures::installer(home, "instalador-lento.exe");
    assert!(!crate::app::open_without_window(&file));
    std::thread::sleep(Duration::from_millis(300));
    let calls = fake.calls();
    assert!(
        calls.iter().all(|c| !c.contains("instalador-lento.exe")),
        "{calls:?}"
    );
    fake.end_all();
});

// A worker that panics while a program starts frees the «opening» mark, so the same program may
// be started again at once and the bottle is not held for minutes.
flow_test!(
    a_panic_while_a_program_starts_frees_its_opening_mark,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let installer = fixtures::installer(home, "instalador-se-rompe.exe");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Drop(installer));
        f.act(Action::ConfirmInstall);
        f.wait_until("the card to fail", LONG, |a| {
            a.install.as_ref().is_some_and(|s| !s.active())
        });
        let bottle = f.app.bottles[0].path.clone();
        f.wait_for("the opening mark to be freed", LONG, |f| {
            f.app.opening.is_empty() && !f.app.bottle_in_use(&bottle)
        });
        assert!(
            f.app.opening.is_empty(),
            "{:?}\n{}",
            f.app.opening,
            f.dump()
        );
        assert!(!f.app.bottle_in_use(&bottle), "{}", f.dump());
        f.finish();
    }
);

// The report of a panicking worker names the program it was starting, and only that one.
flow_test!(
    a_worker_report_frees_only_the_opening_mark_it_names,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        let bottle = f.app.bottles[0].path.clone();
        let (one, other) = (home.join("uno.exe"), home.join("otro.exe"));
        for file in [&one, &other] {
            f.app
                .opening
                .insert((bottle.clone(), file.clone()), std::time::Instant::now());
        }
        let report = OnPanic::default().opening(&bottle, &Some(one.clone()));
        f.app.tx.send(Msg::WorkerFailed(report)).unwrap();
        f.wait_until("the report to free its mark", LONG, |a| {
            !a.opening.contains_key(&(bottle.clone(), one.clone()))
        });
        // The mark of the other program must stay: that part is looked at after a while.
        f.settle(Duration::from_millis(300));
        assert!(!f.app.opening.contains_key(&(bottle.clone(), one)));
        assert!(f.app.opening.contains_key(&(bottle, other)));
        f.finish();
    }
);

// An engine that finishes downloading does not replace the mark of a copy or a delete that began
// during the download.
flow_test!(an_engine_change_leaves_a_held_bottle_as_it_is, |home| {
    let path = fixtures::bottle_with("Mi botella", &[]);
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.app.busy.insert(path.clone(), "Copiando…".into());
    assert!(!f.app.set_bottle_engine(&path, Some(Engine::Metal)));
    assert_eq!(f.app.busy.get(&path).map(String::as_str), Some("Copiando…"));
    assert!(f
        .app
        .cfg
        .bottle_ref(&path)
        .and_then(|s| s.engine.clone())
        .is_none());
    f.finish();
});

// A deleted bottle no longer waits for an engine that is still downloading.
flow_test!(a_deleted_bottle_stops_waiting_for_an_engine, |home| {
    let path = fixtures::bottle_with("Mi botella", &[]);
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.app.pending_engine_bottle = Some((path.clone(), Engine::Metal));
    f.app
        .bottle_deleted(path.clone(), "Mi botella".into(), Ok(()));
    assert!(f.app.pending_engine_bottle.is_none());
    // Another bottle's wait is not touched.
    let other = own_bottles_dir().join("Otra");
    f.app.pending_engine_bottle = Some((other.clone(), Engine::Metal));
    f.app.bottle_deleted(path, "Mi botella".into(), Ok(()));
    assert_eq!(
        f.app.pending_engine_bottle.as_ref().map(|p| &p.0),
        Some(&other)
    );
    f.finish();
});

// A copy is not archived while a program runs in the bottle, even one that started after the
// window last looked (the save dialog may have been open for a while).
flow_test!(a_saved_copy_asks_wine_again_before_archiving, |home| {
    fixtures::bottle_with("Mi botella", &["Programa de prueba"]);
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.act(Action::Select(0));
    f.act(Action::RunProgram(0));
    f.wait_until("the program to open", LONG, |a| !a.running.is_empty());
    let wine = f.app.wine.clone().expect("Wine");
    let bottle = f.app.bottles[0].clone();
    let dest = home.join("copia.tar.gz");
    let msgs = export_job(Some(&wine), &bottle, &dest, &Log::default());
    assert!(
        msgs.iter()
            .any(|m| matches!(m, Msg::Fail(Some(p), _) if *p == bottle.path)),
        "the copy is refused"
    );
    assert!(msgs.iter().all(|m| !matches!(m, Msg::Done(_))));
    assert!(!dest.exists(), "nothing was archived");

    // With nothing running, the same job saves a whole copy.
    f.fake.end_all();
    let msgs = export_job(Some(&wine), &bottle, &dest, &Log::default());
    assert!(msgs.iter().any(|m| matches!(m, Msg::Done(_))));
    assert!(dest.exists());
    f.finish();
});
