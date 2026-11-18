//! The first start and the overview, end to end: what a newcomer sees with and without Wine and
//! what the buttons do. The tests look at the accessible names the interface defines and at the
//! state of the app, not at whole sentences, so a change of wording does not break them.

use super::driver::Flow;
use super::fixtures;
use crate::wine_setup::Step;
use crate::{Action, Modal};
use eframe::egui::Key;
use std::time::Duration;

const LONG: Duration = Duration::from_secs(20);

/// How many controls or texts on screen are called exactly `name`.
fn count(f: &Flow, name: &str) -> usize {
    f.names().iter().filter(|n| n.as_str() == name).count()
}

/// Waits until the app has looked for Wine, found none, tried to install it by itself and was
/// refused (the flows never download anything).
fn without_wine(f: &mut Flow) {
    f.wait_until("the install attempt to fail", LONG, |a| {
        !a.errors.is_empty() && !a.installing()
    });
    f.frame();
}

flow_test!(without_wine_a_new_bottle_is_refused_and_says_why, |home| {
    let mut f = Flow::start_without_wine(home);
    without_wine(&mut f);
    // Whatever button or key asks for it, the dialog does not open: it could not be finished.
    f.act(Action::OpenNewBottle);
    f.frame();
    assert!(f.app.modal.is_none(), "{}", f.dump());
    assert!(!f.app.status.is_empty(), "the refusal is said");
    // The sidebar's button is pressed like a person would: nothing opens either.
    f.click("Nueva botella");
    f.settle(Duration::from_millis(200));
    assert!(f.app.modal.is_none(), "{}", f.dump());
    f.finish();
});

flow_test!(the_dialog_without_wine_offers_to_get_it, |home| {
    let mut f = Flow::start_without_wine(home);
    without_wine(&mut f);
    let before = count(&f, "Instalar Wine gratis");
    assert_eq!(
        before,
        1,
        "the card on the overview offers it\n{}",
        f.dump()
    );
    // The dialog can still be on screen when Wine goes away under it: its banner must lead
    // somewhere, not only say it.
    f.app.modal = Some(Modal::NewBottle);
    f.frame();
    f.frame();
    assert_eq!(
        count(&f, "Instalar Wine gratis"),
        before + 1,
        "the dialog's banner has the button too\n{}",
        f.dump()
    );
    f.finish();
});

flow_test!(the_first_start_has_one_main_way_in, |home| {
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.wait_for("the main button", LONG, |f| {
        f.shows("Instalar mi primer programa")
    });
    // What must not be there is looked at after the page has had its frames.
    f.settle(Duration::from_millis(200));
    assert!(f.app.bottles.is_empty());
    // The main button installs a program (the guided installer makes the bottle by itself); it
    // does not make an empty bottle.
    assert!(f.shows("Instalar mi primer programa"), "{}", f.dump());
    assert!(!f.shows("Crear mi primera botella"), "{}", f.dump());
    // The sidebar keeps the way to an empty bottle; nothing else on the page repeats it.
    assert_eq!(count(&f, "Nueva botella"), 1, "{}", f.dump());
    assert_eq!(count(&f, "Instalar un programa"), 0, "{}", f.dump());
    f.finish();
});

flow_test!(with_bottles_the_overview_does_not_repeat_wine, |home| {
    fixtures::bottle_with("Mi botella", &["Programa de prueba"]);
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.wait_for("the count of bottles in use", LONG, |f| {
        f.shows_part("en uso ahora")
    });
    // What must not be there is looked at after the page has had its frames.
    f.settle(Duration::from_millis(200));
    // Three numbers (bottles, programs, in use), none of them Wine: the sidebar says how it is.
    assert!(f.shows_part("en uso ahora"), "{}", f.dump());
    assert!(!f.shows_part("Wine: "), "{}", f.dump());
    // A bottle with nothing wrong has no «Lista» in its card or in the sidebar.
    assert!(!f.shows_part("Lista"), "{}", f.dump());
    f.finish();
});

flow_test!(a_bottle_with_a_problem_still_says_so, |home| {
    let bottle = fixtures::bottle_with("Mi botella", &[]);
    std::fs::remove_file(bottle.join("system.reg")).unwrap();
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.wait_for("the bottle to say it is incomplete", LONG, |f| {
        f.shows_part("Incompleta")
    });
    assert!(f.shows_part("Incompleta"), "{}", f.dump());
    f.finish();
});

flow_test!(
    the_header_actions_are_visited_in_the_order_they_read,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.wait_for("the install button", LONG, |f| {
            f.shows("Instalar un programa")
        });
        f.settle(Duration::from_millis(200));
        let stops: Vec<Option<String>> = (0..30)
            .map(|_| {
                f.key(Key::Tab);
                f.focused()
            })
            .collect();
        let install = stops
            .iter()
            .position(|s| s.as_deref() == Some("Instalar un programa"))
            .unwrap_or_else(|| panic!("Tab never reached the install button: {stops:?}"));
        assert_eq!(
            stops.get(install + 1).and_then(|s| s.as_deref()),
            Some("Nueva botella"),
            "the new-bottle button reads after the install one: {stops:?}"
        );
        f.finish();
    }
);

/// Sets what the Wine install says it is doing, as its worker would.
fn installing_wine(f: &mut Flow, step: Step, downloaded: u64, total: u64) {
    {
        let mut g = f.app.setup.lock().unwrap();
        g.running = true;
        g.step = Some(step);
        g.downloaded = downloaded;
        g.total = total;
        g.rosetta_needed = Some(step == Step::Rosetta);
        g.flavour = "Wine de prueba".into();
    }
    f.frame();
    f.frame();
}

flow_test!(the_wine_card_tells_the_truth_about_the_password, |home| {
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    installing_wine(&mut f, Step::Rosetta, 0, 0);
    // The Mac may ask for its password to prepare itself: the row says so, and nothing claims
    // that no password is ever needed.
    assert!(f.shows_part("contraseña"), "{}", f.dump());
    assert!(!f.shows_part("ni contraseña"), "{}", f.dump());
    f.finish();
});

flow_test!(a_percentage_is_shown_only_while_downloading, |home| {
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    // Steps whose end cannot be measured turn instead of counting: no percentage anywhere, in
    // the card or in the sidebar.
    for step in [Step::Rosetta, Step::Find, Step::Unpack, Step::Check] {
        installing_wine(&mut f, step, 0, 0);
        assert!(
            !f.shows_part("%"),
            "{step:?} shows a percentage\n{}",
            f.dump()
        );
    }
    // The download knows how far it is.
    installing_wine(&mut f, Step::Download, 50, 100);
    assert!(f.shows_part("50%"), "{}", f.dump());
    // A download of unknown size has no percentage either.
    installing_wine(&mut f, Step::Download, 50, 0);
    assert!(!f.shows_part("%"), "{}", f.dump());
    f.finish();
});

flow_test!(the_sidebar_chip_never_cuts_a_step_name, |home| {
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    // The chip has room for 28 characters: every step's line must fit whole, so the notice
    // about the password is never cut in the middle of a word.
    for step in Step::LIST {
        installing_wine(&mut f, step, 0, 0);
        let sub = f.app.wine_summary().sub;
        assert!(sub.chars().count() <= 28, "{step:?}: «{sub}»");
    }
    f.finish();
});

flow_test!(
    a_wine_that_does_not_start_is_not_told_to_be_fetched,
    |home| {
        let mut f = Flow::start_without_wine(home);
        without_wine(&mut f);
        f.app.wine_state = crate::wine_setup::WineState::Broken {
            path: home.join("wine"),
            reason: "prueba".into(),
        };
        f.frame();
        assert!(
            !f.app.idle_hint().contains("Consigue"),
            "{}",
            f.app.idle_hint()
        );
        f.finish();
    }
);

flow_test!(the_status_line_hint_follows_what_can_be_done, |home| {
    let mut f = Flow::start_without_wine(home);
    without_wine(&mut f);
    // Dragging an installer in leads nowhere without Wine.
    assert!(!f.app.idle_hint().contains(".exe"), "{}", f.app.idle_hint());
    f.finish();
});

flow_test!(
    with_wine_the_status_line_hint_invites_to_drop_an_installer,
    |home| {
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        assert!(f.app.idle_hint().contains(".exe"), "{}", f.app.idle_hint());
        f.finish();
    }
);

flow_test!(
    a_tall_dialog_scrolls_and_keeps_its_buttons_in_view,
    |home| {
        use crate::WineChange;
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        // A dialog that lists far more than the window can show.
        let bottles = (0..120)
            .map(|i| {
                (
                    home.join(format!("b{i}")),
                    format!("Botella con un nombre largo {i}"),
                )
            })
            .collect();
        f.app.modal = Some(Modal::CloseForWine(
            WineChange::Reinstall(crate::wine_setup::Engine::Games),
            bottles,
        ));
        // The dialog fades in and its size settles over a few frames; then «Cancelar» is inside
        // the window, not below its edge.
        let mut seen = None;
        let mut steady = 0;
        for _ in 0..150 {
            f.frame();
            let now = f.bounds_in_window("Cancelar");
            steady = if now.is_some() && now == seen {
                steady + 1
            } else {
                0
            };
            seen = now;
            if steady >= 10 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let (button, window) = seen.expect("the dialog has «Cancelar»");
        assert!(
            window.contains_rect(button),
            "{button:?} is outside {window:?}"
        );
        // On screen, so a press reaches it and closes the dialog.
        f.click("Cancelar");
        f.wait_until("the dialog to close", LONG, |a| a.modal.is_none());
        f.finish();
    }
);

flow_test!(a_program_tile_on_inicio_says_it_is_opening, |home| {
    fixtures::bottle_with("Mi botella", &["Programa de prueba"]);
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.act(Action::GoHome);
    f.wait_for("the program's tile", LONG, |f| {
        f.shows("Programa de prueba")
    });
    assert!(f.shows("Programa de prueba"), "{}", f.dump());
    // While the start is being made (held here by hand: the worker answers faster than a
    // frame can be looked at) the tile on Inicio says so, as the one on the bottle's page does.
    let bottle = f.app.bottles[0].path.clone();
    let file = f.app.bottles[0].programs[0].path.clone();
    f.app
        .opening
        .insert((bottle, file), std::time::Instant::now());
    f.frame();
    assert!(
        f.shows("Programa de prueba, abriéndose"),
        "the tile shows it is opening\n{}",
        f.dump()
    );
    assert!(!f.shows("Programa de prueba"));
    f.finish();
});

flow_test!(a_tile_pressed_on_inicio_does_not_replay_the_page, |home| {
    fixtures::bottle_with("Alfa", &["Programa A"]);
    fixtures::bottle_with("Beta", &["Programa B"]);
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.act(Action::GoHome);
    f.wait_for("the tiles of both bottles", LONG, |f| {
        f.shows("Programa A") && f.shows("Programa B")
    });
    assert_eq!(f.app.screen, crate::Screen::Home);
    let other = (0..2).find(|&i| i != f.app.selected).unwrap();
    let since = f.app.view_since;
    // The press of a tile of a bottle that is not the selected one selects it, but the page
    // is the same page: it must not fade in again.
    f.act(Action::RunFrom(other, 0));
    f.advance(0.05);
    f.frame();
    assert_eq!(f.app.selected, other);
    assert_eq!(f.app.screen, crate::Screen::Home);
    assert_eq!(f.app.view_since, since, "the page was not restarted");
    // Going to a bottle's page still does.
    f.act(Action::Select(other));
    f.advance(0.05);
    f.frame();
    assert_ne!(f.app.view_since, since, "a new page fades in");
    f.finish();
});

flow_test!(lists_say_nothing_about_the_automatic_engine, |home| {
    fixtures::bottle_with("Alfa", &["Programa A"]);
    let steam = fixtures::bottle_with("Beta", &["Programa B"]);
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.app.cfg.bottle(&steam).engine = Some("dxmt".into());
    f.act(Action::GoHome);
    f.wait_for("the sidebar to name the chosen engine", LONG, |f| {
        f.names()
            .iter()
            .any(|n| n.starts_with("Beta") && n.contains("Steam y launchers"))
    });
    // The «Automático» that must not be anywhere is looked for after the page has had its frames.
    f.settle(Duration::from_millis(300));
    let names = f.names();
    // The bottle on the default engine: its row and its card name no engine.
    for name in names.iter().filter(|n| n.contains("Alfa")) {
        assert!(!name.contains("Automático"), "{name:?}\n{}", f.dump());
    }
    assert!(
        !names.iter().any(|n| n.contains("Automático")),
        "{}",
        f.dump()
    );
    // A chosen engine is still named in the sidebar row.
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("Beta") && n.contains("Steam y launchers")),
        "{names:?}"
    );
    f.finish();
});
