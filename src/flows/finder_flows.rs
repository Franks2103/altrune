//! A file opened from the Finder, with the app closed and with the app open: where it lands and
//! what is left of it. Checks outcomes (a dialog for that file, a file in the log), not wording.

use super::driver::Flow;
use super::fake_wine::FakeWine;
use super::fixtures;
use crate::app::{hand_over, requests_dir};
use crate::backend::log_file;
use crate::Action;
use std::path::Path;
use std::time::{Duration, Instant};

const LONG: Duration = Duration::from_secs(20);

/// The file the install dialog (or the direct open) is about, when one is open.
fn draft_file(f: &Flow) -> Option<&Path> {
    f.app.install_draft.as_ref().map(|d| d.file.as_path())
}

// A program opened from the Finder while the app is closed runs with no window, but what it
// prints is kept in the app's log file, so a failure can be read afterwards.
flow_test!(
    a_file_opened_without_a_window_leaves_its_output_in_the_log,
    |home| {
        let fake = FakeWine::write(home);
        fixtures::bottle_with("Mi botella", &[]);
        let file = fixtures::installer(home, "se-cierra.exe");
        assert!(
            crate::app::open_without_window(&file),
            "the file opens with no window"
        );
        let end = Instant::now() + LONG;
        let log = loop {
            let text = std::fs::read_to_string(log_file()).unwrap_or_default();
            if text.contains("MSVCP140.dll") || Instant::now() > end {
                break text;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        assert!(
            log.contains("MSVCP140.dll"),
            "the program's error is in the log file: {log:?}\n{}",
            fake.calls().join("\n")
        );
        // Wine got the file once, in the default bottle.
        assert_eq!(fake.count("se-cierra.exe"), 1, "{:?}", fake.calls());
        assert!(
            fake.calls()
                .iter()
                .any(|c| c.contains("se-cierra.exe") && c.ends_with("Bottles/Mi botella]")),
            "{:?}",
            fake.calls()
        );
        fake.end_all();
    }
);

// A second launch that hands a file to the open app gets the same dialog a Finder open gets.
flow_test!(
    a_request_from_a_second_launch_opens_the_install_dialog,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let installer = fixtures::installer(home, "instalador-prueba.exe");
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        assert!(f.app.install_draft.is_none());
        hand_over(&requests_dir(), Some(&installer)).unwrap();
        f.wait_until("the dialog for the handed-over file", LONG, |a| {
            a.install_draft.is_some()
        });
        assert_eq!(draft_file(&f), Some(installer.as_path()));
        assert!(f.app.modal.is_some(), "{}", f.dump());
        assert_eq!(
            std::fs::read_dir(requests_dir()).unwrap().count(),
            0,
            "the request was taken"
        );
        // Nothing starts before the person confirms, as with any other file.
        assert!(f.app.install.is_none());
        f.finish();
    }
);

// A request that arrives before Wine is ready waits for it, as the Finder's own file does.
flow_test!(a_request_waits_for_wine_like_a_finder_open, |home| {
    fixtures::bottle_with("Mi botella", &[]);
    let installer = fixtures::installer(home, "instalador-prueba.exe");
    // The request is already waiting when the app starts looking.
    hand_over(&requests_dir(), Some(&installer)).unwrap();
    let mut f = Flow::start(home);
    f.wait_until("the dialog for the file", LONG, |a| {
        a.install_draft.is_some()
    });
    assert_eq!(draft_file(&f), Some(installer.as_path()));
    f.finish();
});

// A second request while a dialog is open waits in its file, and is opened once the first is
// closed: neither is lost, and the first dialog is not replaced.
flow_test!(a_second_request_waits_for_the_open_dialog, |home| {
    fixtures::bottle_with("Mi botella", &[]);
    let first = fixtures::installer(home, "instalador-prueba.exe");
    let second = fixtures::installer(home, "instalador-lento.exe");
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    hand_over(&requests_dir(), Some(&first)).unwrap();
    f.wait_until("the first dialog", LONG, |a| a.install_draft.is_some());
    hand_over(&requests_dir(), Some(&second)).unwrap();
    f.settle(Duration::from_millis(1500));
    assert_eq!(draft_file(&f), Some(first.as_path()), "{}", f.dump());
    f.act(Action::CloseModal);
    f.wait_until("the second dialog", LONG, |a| {
        a.install_draft.as_ref().is_some_and(|d| d.file == second)
    });
    f.finish();
});

// A request for a file that is gone opens nothing and says so.
flow_test!(
    a_request_for_a_missing_file_is_dropped_with_a_message,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        let gone = home.join("Descargas/ya-no-esta.exe");
        hand_over(&requests_dir(), Some(&gone)).unwrap();
        f.wait_until("the message", LONG, |a| a.status.contains("ya-no-esta.exe"));
        f.settle(Duration::from_millis(300));
        assert!(f.app.install_draft.is_none() && f.app.modal.is_none());
        assert!(f.app.pending_open.is_none());
        assert_eq!(std::fs::read_dir(requests_dir()).unwrap().count(), 0);
        f.finish();
    }
);

// A file the Finder handed over that is gone by the time the app can open it is told about, not
// offered for installing.
flow_test!(a_file_that_vanished_while_it_waited_is_told_about, |home| {
    fixtures::bottle_with("Mi botella", &[]);
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.app.pending_open = Some(home.join("Descargas/se-fue.exe"));
    f.wait_until("the message", LONG, |a| a.status.contains("se-fue.exe"));
    assert!(f.app.install_draft.is_none() && f.app.modal.is_none());
    f.finish();
});

// A request with nothing readable in it is taken and thrown away without a sound.
flow_test!(a_request_with_bad_content_is_dropped_safely, |home| {
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    std::fs::create_dir_all(requests_dir()).unwrap();
    std::fs::write(requests_dir().join("0001.pedido"), "no/es/absoluta.exe").unwrap();
    std::fs::write(requests_dir().join("0002.pedido"), b"/con\0nulo").unwrap();
    f.wait_until("the requests to be taken", LONG, |_| {
        std::fs::read_dir(requests_dir())
            .map(|d| d.count() == 0)
            .unwrap_or(true)
    });
    f.settle(Duration::from_millis(300));
    assert!(f.app.install_draft.is_none() && f.app.modal.is_none());
    assert!(f.app.errors.is_empty(), "{}", f.dump());
    f.finish();
});
