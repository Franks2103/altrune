//! A long session in a few seconds: one `App` is driven through about a hundred opens and
//! closes of a program, many dialogs, repeated reads of the lists and a few bottles made,
//! renamed, copied and deleted. Afterwards everything the app keeps in memory is back at its
//! resting size, no child process is left unreaped, and the process holds about as many open
//! files and threads as it did before the cycles.
//!
//! The bounds below come from reading the code, and each says where it comes from. None is
//! loosened to make a run pass: a number that grows with the cycles is a leak to fix.

use super::driver::{open_descriptors, thread_count, Flow};
use super::fake_wine::zombie_children;
use super::fixtures;
use crate::backend::own_bottles_dir;
use crate::Action;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

const LONG: Duration = Duration::from_secs(30);

/// Bottles the program cycles run in at once. The app opens and closes one bottle at a time
/// (its buttons act on the shown bottle), but each close is a worker that takes about a
/// quarter of a second, so a round does them all together and ten rounds are a hundred cycles
/// in the time of ten.
const BOTTLES: usize = 10;
/// Rounds of open-all, close-all.
const ROUNDS: usize = 10;
/// Times each dialog is opened and closed.
const DIALOG_TURNS: usize = 8;
/// Times the programs are read again and Wine is searched for again.
const REFRESH_TURNS: usize = 6;
/// Looks at the bottles that the window makes while it is left alone.
const IDLE_TURNS: usize = 6;
/// Programs installed with the guided installer.
const INSTALL_TURNS: usize = 1;
/// Bottles made, renamed, copied and deleted.
const BOTTLE_TURNS: usize = 2;

/// The program every bottle holds.
const PROGRAM: &str = "Programa de prueba";

/// What the app holds that must come back to its resting size, and what the process holds
/// open. Measured only when nothing is in flight (see `at_rest`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Resting {
    bottles: usize,
    running: usize,
    handed_over: usize,
    opening: usize,
    busy: usize,
    errors: usize,
    textures: usize,
    icons_requested: usize,
    engine_versions: usize,
    versions_requested: usize,
    /// Results, starts, jobs reading and tickets wanted of the diagnosis.
    diagnosis: [usize; 4],
    log_lines: usize,
    modal: bool,
    install_draft: bool,
    install_card: bool,
    settings_bottles: usize,
    /// Values the window keeps for its widgets (egui's temporary data).
    ui_data: usize,
}

impl Resting {
    fn of(f: &Flow) -> Self {
        let a = &f.app;
        Self {
            bottles: a.bottles.len(),
            running: a.running.len(),
            handed_over: a.handed_over.len(),
            opening: a.opening.len(),
            busy: a.busy.len(),
            errors: a.errors.len(),
            textures: a.textures.len(),
            icons_requested: a.icons_requested.len(),
            engine_versions: a.engine_versions.len(),
            versions_requested: a.versions_requested.len(),
            diagnosis: a.diagnosis.sizes(),
            log_lines: a.log.lock().unwrap().len(),
            modal: a.modal.is_some(),
            install_draft: a.install_draft.is_some(),
            install_card: a.install.is_some(),
            settings_bottles: a.cfg.bottles.len(),
            ui_data: a.ctx.data(|d| d.len()),
        }
    }
}

/// Runs frames until nothing is in flight: no program running or opening, no bottle busy, no
/// worker looking at the bottles or reading a diagnosis, no scan of an installation.
fn at_rest(f: &mut Flow) {
    f.wait_until("the app to be at rest", LONG, |a| {
        a.running.is_empty()
            && a.opening.is_empty()
            && a.busy.is_empty()
            && a.app_job.is_none()
            && !a.live_check_busy.load(Ordering::SeqCst)
            && !a.install_scanning
            && a.diagnosis.sizes()[2] == 0
            && a.wine_state != crate::wine_setup::WineState::Unknown
    });
    // The messages a worker sent just before it ended are read on the next frames.
    f.settle(Duration::from_millis(100));
}

/// Waits until the count of descriptors or threads is at most `limit`, or gives what it was.
/// A thread that has answered its message may still be winding down for a moment, so a count
/// is only judged once it has had a short time to settle.
fn wait_for_at_most(f: &mut Flow, limit: usize, count: impl Fn() -> usize) -> usize {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        f.frame();
        let now = count();
        if now <= limit || Instant::now() > end {
            return now;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A bottle with one program that has a picture of its own: a `.url` shortcut whose icon is a
/// real PNG inside the bottle, so the window decodes it into a texture.
fn bottle_with_picture(name: &str) {
    let bottle = fixtures::bottle_with(name, &[]);
    let png = bottle.join("drive_c/imagen.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([40, 90, 200, 255]))
        .save(&png)
        .expect("the picture");
    std::fs::write(
        fixtures::start_menu(&bottle).join("Con imagen.url"),
        "[InternetShortcut]\r\nURL=steam://rungameid/1\r\nICONFILE=C:\\imagen.png\r\n",
    )
    .expect("the shortcut with a picture");
}

/// One round: the program is opened in every bottle, the messages of one bottle are read (of
/// every bottle in the first round), and then every bottle is closed.
fn program_round(f: &mut Flow, round: usize) {
    for i in 0..BOTTLES {
        f.act(Action::Select(i));
        let p = f
            .app
            .bottle()
            .and_then(|b| b.programs.iter().position(|p| p.name == PROGRAM))
            .unwrap_or_else(|| panic!("bottle {i} lists the program\n{}", f.dump()));
        f.act(Action::RunProgram(p));
    }
    f.wait_until("every program to run", LONG, |a| a.running.len() == BOTTLES);
    // The app follows a program from the moment its process starts, but the fake Wine only
    // records it a little later. A close asked in between would find nothing open (as it
    // would with a real Wine that has not started its server yet), so the round waits.
    f.wait_for_fake("every fake program to start", LONG, |n| n >= BOTTLES);
    // The round also reads the messages of one of the bottles it just opened; the first round
    // reads every bottle's, so what the diagnosis keeps for them is in the resting size.
    for i in 0..BOTTLES {
        if round == 0 || i == round % BOTTLES {
            f.act(Action::Select(i));
            f.act(Action::Diagnose);
        }
    }
    f.frame();
    for i in 0..BOTTLES {
        f.act(Action::Select(i));
        f.act(Action::Quit(false));
    }
    f.wait_until("every bottle to close", LONG, |a| {
        a.running.is_empty() && a.busy.is_empty()
    });
    assert!(
        f.fake.alive().is_empty(),
        "a fake program outlived its close"
    );
}

/// Opens and closes every dialog that needs no file dialog, and visits the pages, many times.
fn dialog_turns(f: &mut Flow, installer: &std::path::Path, turns: usize) {
    f.act(Action::Select(0));
    for turn in 0..turns {
        for open in [
            Action::OpenNewBottle,
            Action::OpenRename,
            Action::AskDelete,
            Action::OpenWineHelp,
            Action::RunWithOptions(0),
            Action::Drop(installer.to_path_buf()),
        ] {
            f.act(open);
            f.frame();
            assert!(f.app.modal.is_some(), "turn {turn}: a dialog opened");
            f.act(Action::CloseModal);
            f.frame();
            assert!(f.app.modal.is_none(), "turn {turn}: the dialog closed");
        }
        f.act(Action::OpenSettings);
        f.frame();
        f.act(Action::SetTab(crate::BottleTab::Settings));
        f.act(Action::Select(turn % BOTTLES));
        f.frame();
        f.act(Action::SetTab(crate::BottleTab::Diagnostics));
        f.frame();
        f.act(Action::SetTab(crate::BottleTab::Programs));
        f.act(Action::GoHome);
        f.frame();
    }
}

/// Reads the programs and looks for Wine again, many times.
fn refresh_turns(f: &mut Flow) {
    for _ in 0..REFRESH_TURNS {
        f.act(Action::RefreshPrograms);
        f.frame();
    }
    for _ in 0..REFRESH_TURNS {
        f.act(Action::Rescan);
        f.wait_until("the search for Wine to end", LONG, |a| {
            a.wine_state != crate::wine_setup::WineState::Unknown
        });
    }
}

/// Leaves the window alone for many minutes of its own clock: the app looks at its bottles
/// every 3 s, and each look must end before the next one starts.
fn idle_turns(f: &mut Flow) {
    for _ in 0..IDLE_TURNS {
        // The frames that ran since the last turn (the settling of `at_rest`, for the first) can
        // have started a look, and the calls of a look that began before this turn are not this
        // turn's. When the frame that ended this wait started none, nothing is in flight, and
        // the count below starts from a quiet log.
        f.wait_until("no look at the bottles to be in flight", LONG, |a| {
            !a.live_check_busy.load(Ordering::SeqCst)
        });
        let asked = f.fake.count("wineserver -k0");
        let last = f.app.last_live_check;
        f.skip_clock(3.5);
        // The look starts on the frame after the clock moved (the app reads the time the
        // window was last drawn with). Wait for it instead of counting frames: on a busy
        // machine a fixed number of frames can end before it starts. The looks are counted as
        // they start: on a busy machine one look can take longer than 3 s of the clock, and
        // the next one then follows it, which is right. What must hold is that each look
        // asks each bottle at most once.
        let starts = std::cell::Cell::new(0usize);
        f.wait_until("a look at the bottles to start", LONG, |a| {
            a.last_live_check > last
        });
        let seen = std::cell::Cell::new(f.app.last_live_check);
        f.wait_until("the look at the bottles to end", LONG, |a| {
            if a.last_live_check != seen.get() {
                seen.set(a.last_live_check);
                starts.set(starts.get() + 1);
            }
            !a.live_check_busy.load(Ordering::SeqCst)
        });
        // The first look was seen by the wait above, the ones after it by this one.
        let looks_started = 1 + starts.get();
        let asked_now = f.fake.count("wineserver -k0") - asked;
        assert!(
            asked_now <= BOTTLES * looks_started,
            "{looks_started} look(s) asked each bottle at most once, not {asked_now} times"
        );
    }
}

/// Installs a program with the guided installer and closes its card, as a person would.
fn install_turn(f: &mut Flow, installer: &std::path::Path) {
    f.act(Action::Select(0));
    f.act(Action::Drop(installer.to_path_buf()));
    f.act(Action::ConfirmInstall);
    f.wait_until("the installation to end", LONG, |a| {
        a.install
            .as_ref()
            .is_some_and(|s| matches!(s.stage, crate::domain::install::Stage::Done(_)))
    });
    f.act(Action::DismissInstall);
    f.frame();
    assert!(f.app.install.is_none(), "the card is closed\n{}", f.dump());
    at_rest(f);
}

/// Makes a bottle, renames it, copies it and moves both to the Trash.
fn bottle_turn(f: &mut Flow, n: usize) {
    let made = format!("Temporal {n}");
    f.act(Action::OpenNewBottle);
    f.app.new_name = made.clone();
    f.act(Action::CreateBottle);
    f.wait_until("the bottle to be made", LONG, |a| {
        a.busy.is_empty()
            && a.bottles
                .iter()
                .any(|b| b.name == made && b.prefix_complete)
    });
    // A program in the new bottle, opened and closed, and its messages read: so the app holds
    // a start and a diagnosis for a bottle that is about to be renamed, copied and deleted.
    let bottle = f
        .app
        .bottle()
        .expect("the new bottle is shown")
        .path
        .clone();
    fixtures::fake_exe(
        &bottle
            .join("drive_c/Program Files")
            .join(PROGRAM)
            .join("programa.exe"),
    );
    std::fs::write(
        fixtures::start_menu(&bottle).join(format!("{PROGRAM}.lnk")),
        fixtures::test_program_lnk(),
    )
    .unwrap();
    f.act(Action::RefreshPrograms);
    let p = f
        .app
        .bottle()
        .and_then(|b| b.programs.iter().position(|p| p.name == PROGRAM))
        .unwrap_or_else(|| panic!("the new bottle lists its program\n{}", f.dump()));
    f.act(Action::RunProgram(p));
    f.wait_until("the program to run", LONG, |a| !a.running.is_empty());
    f.act(Action::Diagnose);
    f.act(Action::Quit(false));
    at_rest(f);
    // The live check (every 3 s of app time) may still count the bottle as in use for a moment
    // after its programs closed, and a rename is refused while it does. On a slow machine (a CI
    // runner) the first rename came too soon: wait, as a person waits for «En uso» to go away.
    f.wait_until("the bottle to be free", LONG, |a| !a.bottle_in_use(&bottle));
    let before = f.app.bottles.len();

    f.act(Action::OpenRename);
    let renamed = format!("Renombrada {n}");
    f.app.rename_text = renamed.clone();
    f.act(Action::DoRename);
    assert!(
        f.app.bottles.iter().any(|b| b.name == renamed),
        "renamed\n{}",
        f.dump()
    );

    f.act(Action::Duplicate);
    f.wait_until("the copy to be made", LONG, |a| {
        a.busy.is_empty() && a.bottles.len() == before + 1
    });

    // The copy is the shown bottle now; the original is the one named `renamed`.
    for _ in 0..2 {
        f.act(Action::AskDelete);
        f.act(Action::DoDelete);
        f.wait_until("the bottle to go to the Trash", LONG, |a| a.busy.is_empty());
        let shown = f.app.bottle().map(|b| b.name.clone());
        if let Some(i) = f
            .app
            .bottles
            .iter()
            .position(|b| b.name == renamed)
            .filter(|_| shown.as_deref() != Some(renamed.as_str()))
        {
            f.act(Action::Select(i));
        }
    }
    assert!(
        f.app
            .bottles
            .iter()
            .all(|b| !b.name.starts_with("Renombrada") && !b.name.starts_with("Temporal")),
        "both copies are gone\n{}",
        f.dump()
    );
}

flow_test!(a_long_session_leaves_the_app_where_it_started, |home| {
    for i in 0..BOTTLES - 1 {
        fixtures::bottle_with(&format!("Botella {i}"), &[PROGRAM]);
    }
    bottle_with_picture("Botella con imagen");
    // The bottle with a picture also holds the test program, to open and close.
    let with_picture = own_bottles_dir().join("Botella con imagen");
    let exe = with_picture.join("drive_c/Program Files").join(PROGRAM);
    fixtures::fake_exe(&exe.join("programa.exe"));
    std::fs::write(
        fixtures::start_menu(&with_picture).join(format!("{PROGRAM}.lnk")),
        fixtures::lnk_bytes(
            "C:\\Program Files\\Programa de prueba\\programa.exe",
            PROGRAM,
        ),
    )
    .unwrap();
    let installer = fixtures::installer(home, "instalador-lento.exe");

    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    assert_eq!(f.app.bottles.len(), BOTTLES);
    // The home page draws the pictures; give the worker that decodes them its time.
    f.act(Action::GoHome);
    f.wait_until("the picture to be decoded", LONG, |a| {
        a.textures.values().any(|t| t.is_some())
    });

    // One round first: whatever the app starts once and keeps for good (its first worker
    // threads, the log file) exists before the resting point is taken.
    program_round(&mut f, 0);
    // The window keeps a small state for each widget that has one (a fold, a field, a page),
    // created the first time that screen is drawn: so every dialog and page is visited twice
    // before the resting point, and from there nothing may be added.
    dialog_turns(&mut f, &installer, 2);
    at_rest(&mut f);
    let resting = Resting::of(&f);
    let fds = open_descriptors();
    let threads = thread_count();
    assert!(resting.textures >= 1, "the picture is held: {resting:?}");
    assert_eq!(resting.running, 0);
    assert_eq!(resting.busy, 0);

    for round in 1..ROUNDS {
        program_round(&mut f, round);
    }
    at_rest(&mut f);
    dialog_turns(&mut f, &installer, DIALOG_TURNS);
    refresh_turns(&mut f);
    at_rest(&mut f);
    idle_turns(&mut f);
    let guided = fixtures::installer(home, "instalador-prueba.exe");
    for _ in 0..INSTALL_TURNS {
        install_turn(&mut f, &guided);
    }
    // The selected bottle and the page are the person's place, not a resource.
    f.act(Action::GoHome);
    for n in 0..BOTTLE_TURNS {
        bottle_turn(&mut f, n);
    }
    at_rest(&mut f);

    // 1. Memory the app keeps. Each collection has an honest bound, and at rest it is the size
    // it had at the start:
    // - running, opening, busy, errors, handed_over: they follow what is happening now; a
    //   program that ended, a job that answered or a bottle that went frees its entry (errors
    //   hold one banner for each bottle at most, see `put_error`).
    // - textures and icons_requested: one for each picture of a program that is listed now
    //   (`prune_icons` runs on every reading of the lists); a bottle deleted or renamed takes
    //   its pictures with it.
    // - engine_versions and versions_requested: one for each installed engine, at most two
    //   (the games engine and the DXMT one); no engine is installed in these flows.
    // - diagnosis: one result and one start time for each bottle that exists (the tables are
    //   cut to the listed bottles on every reading), never for one that is gone.
    // - settings (`cfg.bottles`): one entry for each bottle that exists.
    // - ui_data: egui's temporary values, one for each widget with a state of its own; the
    //   screens fix that set, so it stops growing once each has been seen.
    // - the log window: a ring of at most 400 lines (`WINDOW_LINES`), full at this point.
    let end = Resting::of(&f);
    assert!(end.log_lines <= 400, "the window keeps at most 400 lines");
    let mut expected = resting.clone();
    expected.log_lines = end.log_lines;
    assert_eq!(end, expected, "something kept growing\n{}", f.dump());

    // 2. No child is left unreaped (a zombie), and no fake program is left running.
    assert_eq!(zombie_children(), Vec::<u32>::new());
    assert!(f.fake.alive().is_empty());

    // 3. Descriptors and threads come back to the count they had after the first round. Nothing
    // is allowed on top: every worker ends with its job, every pipe reader ends when its
    // program does, and the listing of /dev/fd costs the same descriptor each time.
    let cycles = ROUNDS * BOTTLES;
    let now_fds = wait_for_at_most(&mut f, fds, open_descriptors);
    assert!(
        now_fds <= fds,
        "open files grew from {fds} to {now_fds} over {cycles} cycles"
    );
    let now_threads = wait_for_at_most(&mut f, threads, thread_count);
    assert!(
        now_threads <= threads,
        "threads grew from {threads} to {now_threads} over {cycles} cycles"
    );
    f.finish();
});

flow_test!(a_picture_for_a_program_that_left_is_not_kept, |home| {
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    // A worker decoded the picture of a program whose bottle was deleted meanwhile: nothing
    // asks for it any more (`prune_icons` already forgot the request), so it must not stay.
    let gone = std::path::PathBuf::from("/bottle/that/left/icon.png");
    let picture = eframe::egui::ColorImage::filled([8, 8], eframe::egui::Color32::BLUE);
    f.app
        .tx
        .send(crate::Msg::IconsLoaded(vec![(
            gone.clone(),
            Some(picture.clone()),
        )]))
        .unwrap();
    f.frame();
    assert!(f.app.textures.is_empty(), "{:?}", f.app.textures.keys());
    // The same picture, still asked for, is kept.
    f.app.icons_requested.insert(gone.clone());
    f.app
        .tx
        .send(crate::Msg::IconsLoaded(vec![(gone.clone(), Some(picture))]))
        .unwrap();
    f.frame();
    assert!(f.app.textures.get(&gone).is_some_and(|t| t.is_some()));
    f.finish();
});

flow_test!(
    dialogs_opened_again_and_again_add_nothing_to_the_windows_memory,
    |home| {
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        // The first dialogs may create what the window keeps for good (its fold states, the
        // field's first focus); from there on the same dialogs must not add anything.
        for open in [Action::OpenNewBottle, Action::OpenWineHelp] {
            f.act(open);
            f.frame();
            f.act(Action::CloseModal);
            f.frame();
        }
        let resting = f.app.ctx.data(|d| d.len());
        for _ in 0..40 {
            for open in [Action::OpenNewBottle, Action::OpenWineHelp] {
                f.act(open);
                // Several frames pass while the dialog is open, as when a person reads it.
                f.settle(Duration::from_millis(30));
                f.act(Action::CloseModal);
                f.frame();
            }
        }
        assert_eq!(
            f.app.ctx.data(|d| d.len()),
            resting,
            "each dialog opened left something in the window's memory"
        );
        f.finish();
    }
);
