//! What the person reads when something goes wrong: plain Spanish, never the operating
//! system's own English. The tests look for what must not be on screen, not for exact wording.

use super::driver::Flow;
use super::fixtures;
use crate::backend::own_bottles_dir;
use crate::Action;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

const LONG: Duration = Duration::from_secs(20);

/// Words only the system or Wine print.
const RAW: [&str; 4] = [
    "os error",
    "Permission denied",
    "No such file",
    "Not a directory",
];

fn assert_no_raw_text(f: &Flow) {
    for e in &f.app.errors {
        for raw in RAW {
            assert!(
                !e.text.contains(raw),
                "raw text «{raw}» reached the screen: «{}»\n{}",
                e.text,
                f.dump()
            );
        }
    }
}

flow_test!(
    a_program_that_cannot_be_started_is_explained_in_spanish,
    |home| {
        fixtures::bottle_with("Mi botella", &["Programa de prueba"]);
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Select(0));
        let i = f
            .app
            .bottle()
            .and_then(|b| {
                b.programs
                    .iter()
                    .position(|p| p.name == "Programa de prueba")
            })
            .expect("the program is listed");
        // The system refuses to run the Wine (its file lost its permissions).
        let wine = home.join("wine-falso/bin/wine");
        std::fs::set_permissions(&wine, std::fs::Permissions::from_mode(0o000)).unwrap();
        f.act(Action::RunProgram(i));
        f.wait_until("the failure to be reported", LONG, |a| !a.errors.is_empty());
        assert!(
            f.app
                .errors
                .iter()
                .any(|e| e.text.contains("No se pudo abrir")),
            "{}",
            f.dump()
        );
        assert_no_raw_text(&f);
        std::fs::set_permissions(&wine, std::fs::Permissions::from_mode(0o755)).unwrap();
        f.finish();
    }
);

flow_test!(
    a_bottle_that_cannot_be_made_is_explained_in_spanish,
    |home| {
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        // A file where the bottles' folder should be: the folder cannot be created.
        let folder = own_bottles_dir();
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::write(&folder, "no soy una carpeta").unwrap();
        f.act(Action::OpenNewBottle);
        f.frame();
        f.act(Action::CreateBottle);
        f.wait_until("the failure to be reported", LONG, |a| {
            !a.errors.is_empty() && a.busy.is_empty()
        });
        assert!(
            f.app
                .errors
                .iter()
                .any(|e| e.text.contains("No se pudo crear la botella")),
            "{}",
            f.dump()
        );
        assert_no_raw_text(&f);
        f.finish();
    }
);

flow_test!(a_bottle_wine_cannot_prepare_points_to_the_log, |home| {
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    // A Wine that answers its version and then fails at everything else.
    let wine = home.join("wine-falso/bin/wine");
    std::fs::write(
        &wine,
        "#!/bin/sh\ncase \"$1\" in --version) echo wine-9.0 ;; *) echo 'wine: something broke' >&2; exit 1 ;; esac\n",
    )
    .unwrap();
    f.act(Action::OpenNewBottle);
    f.frame();
    f.act(Action::CreateBottle);
    f.wait_until("the failure to be reported", LONG, |a| {
        !a.errors.is_empty() && a.busy.is_empty()
    });
    let text = f.app.errors[0].text.clone();
    assert!(text.contains("No se pudo crear la botella"), "{text}");
    assert!(
        text.contains("registro de actividad"),
        "the failure says where to look: {text}"
    );
    assert!(!text.contains("something broke"), "{text}");
    f.finish();
});

flow_test!(
    the_automatic_engine_has_no_badge_on_the_bottle_page,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Select(0));
        f.wait_for("the bottle's page", LONG, |f| f.shows_part("Mi botella"));
        // What must not be there is looked at after the page has had its frames.
        f.settle(Duration::from_millis(200));
        // No engine chosen: nothing says "Auto" or names an engine in the header.
        for name in f.names() {
            assert_ne!(name, "Auto", "{}", f.dump());
            assert_ne!(name, "Automático", "{}", f.dump());
        }
        f.finish();
    }
);

flow_test!(
    an_engine_is_named_as_the_list_names_it_while_it_installs,
    |home| {
        use crate::domain::engines::LAUNCHERS;
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        // Straight after the press, before any frame: the line, the job and the download step.
        f.app.start_engine_install(crate::wine_setup::Engine::Metal);
        let said = f.app.status.clone();
        assert!(said.contains(&format!("«{LAUNCHERS}»")), "{said}");
        assert!(
            !said.to_lowercase().contains("directx 10") && !said.to_lowercase().contains("dxmt"),
            "{said}"
        );
        assert_eq!(f.app.installing_job(), LAUNCHERS);
        // A second press says what runs now with the same name.
        f.app
            .start_engine_install(crate::wine_setup::Engine::Standard);
        assert!(f.app.status.contains(LAUNCHERS), "{}", f.app.status);
        f.wait_until("the install to end", LONG, |a| !a.installing());
        f.finish();
    }
);

flow_test!(
    an_addition_that_cannot_start_names_the_engine_as_the_list_does,
    |home| {
        use crate::domain::engines::LAUNCHERS;
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        // This Mac has no DXMT engine installed: the addition says what to choose.
        f.app.add_video();
        let text = f.app.status.clone();
        assert!(text.contains(&format!("«{LAUNCHERS}»")), "{text}");
        assert!(!text.contains("motor DXMT"), "{text}");
        assert!(!f.app.installing(), "nothing started");
        f.finish();
    }
);
