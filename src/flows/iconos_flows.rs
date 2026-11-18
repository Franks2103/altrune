//! A bottle wears the picture of its program: the choice is made from what the scan found, and
//! the texture is the one the window already decodes for the program tiles.

use super::driver::Flow;
use super::fixtures;
use crate::Action;
use std::path::PathBuf;
use std::time::Duration;

const LONG: Duration = Duration::from_secs(20);

/// A real PNG inside the bottle, and a `.url` shortcut called `program` that points at it (the
/// way the soak test gives a program a picture).
fn program_with_picture(bottle: &std::path::Path, program: &str, shade: u8) {
    let png = bottle.join(format!("drive_c/{program}.png"));
    image::RgbaImage::from_pixel(8, 8, image::Rgba([shade, 90, 200, 255]))
        .save(&png)
        .expect("the picture");
    std::fs::write(
        fixtures::start_menu(bottle).join(format!("{program}.url")),
        format!(
            "[InternetShortcut]\r\nURL=steam://rungameid/{shade}\r\nICONFILE=C:\\{program}.png\r\n"
        ),
    )
    .expect("the shortcut with a picture");
}

/// The bottle's name and the face its picture has been decoded for, once the window has them.
fn face_of(f: &Flow, name: &str) -> (Option<PathBuf>, bool) {
    let b = f
        .app
        .bottles
        .iter()
        .find(|b| b.name == name)
        .expect("the bottle");
    (
        b.face().map(|p| p.to_path_buf()),
        f.app.bottle_picture(b).is_some(),
    )
}

// A bottle named like its program (a copy mark aside) wears that program's picture, decoded
// at start by the request the program tiles already make, with Inicio not even looked at.
flow_test!(a_bottle_named_like_its_program_wears_its_picture, |home| {
    for name in ["Steam", "Steam (copia)"] {
        let bottle = fixtures::bottle_with(name, &[]);
        program_with_picture(&bottle, "Steam", 40);
    }
    // A bottle with two unrelated games keeps its letter.
    let games = fixtures::bottle_with("Juegos", &[]);
    program_with_picture(&games, "Uno", 10);
    program_with_picture(&games, "Dos", 20);
    // A bottle whose only picture is of another name wears it (single pictured program).
    let single = fixtures::bottle_with("Mi botella", &[]);
    program_with_picture(&single, "Otro nombre", 60);

    let mut f = Flow::start(home);
    f.wait_until("the pictures to be decoded", LONG, |a| {
        a.textures.values().filter(|t| t.is_some()).count() >= 3
    });
    f.frame();
    for name in ["Steam", "Steam (copia)", "Mi botella"] {
        let (face, decoded) = face_of(&f, name);
        let face = face.unwrap_or_else(|| panic!("{name} has a face\n{}", f.dump()));
        assert!(face.is_file(), "{face:?}");
        assert!(decoded, "{name}'s picture is a texture\n{}", f.dump());
    }
    let (face, decoded) = face_of(&f, "Juegos");
    assert!(face.is_none() && !decoded, "{face:?}");
    // Looks of the bottle only: the row, the card and the page are still called by the name.
    assert!(f.shows("Steam"), "{}", f.dump());
    f.finish();
});

// A program the person took off the list stops being the bottle's face, and the bottle goes
// back to its letter.
flow_test!(
    a_removed_program_is_no_longer_the_face_of_its_bottle,
    |home| {
        let bottle = fixtures::bottle_with("Steam", &[]);
        program_with_picture(&bottle, "Steam", 40);
        let mut f = Flow::start(home);
        f.wait_until("the picture", LONG, |a| {
            a.bottles
                .first()
                .is_some_and(|b| a.bottle_picture(b).is_some())
        });
        f.act(Action::Select(0));
        let at = f
            .app
            .bottle()
            .and_then(|b| b.programs.iter().position(|p| p.name == "Steam"));
        f.act(Action::HideProgram(at.expect("the program is listed")));
        f.wait_until("the program to leave the list", LONG, |a| {
            a.bottle().is_some_and(|b| b.shown_count() == 0)
        });
        let (face, decoded) = face_of(&f, "Steam");
        assert!(face.is_none() && !decoded, "{face:?}\n{}", f.dump());
        f.finish();
    }
);
