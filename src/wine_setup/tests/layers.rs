//! Tests of the Mac libraries the engine borrows, the controller libraries and the runtime environment.

use crate::backend::{controller_libraries, library_path, runtime_env};
use crate::wine_setup::*;
use std::path::Path;
use std::process::Command;

/// A wired "for Xbox" pad only reaches Wine through the SDL built with libusb, so that one
/// must be found before the standard Wine's SDL2.
#[test]
fn controller_libraries_come_before_the_standard_ones() {
    let host = Path::new("/w/Wine Stable.app/Contents/Resources/wine/lib");
    assert_eq!(
        library_path(&[Some(Path::new("/w/Mandos")), None], host),
        "/w/Mandos:/w/Wine Stable.app/Contents/Resources/wine/lib:/usr/lib"
    );
    assert_eq!(
        library_path(&[None, None], host),
        "/w/Wine Stable.app/Contents/Resources/wine/lib:/usr/lib"
    );
    // GStreamer (video) comes after the pad's SDL and before the standard Wine's libraries.
    assert_eq!(
        library_path(
            &[
                Some(Path::new("/w/Mandos")),
                Some(Path::new("/m/gstreamer/lib"))
            ],
            host
        ),
        "/w/Mandos:/m/gstreamer/lib:/w/Wine Stable.app/Contents/Resources/wine/lib:/usr/lib"
    );
}

/// Manual, with the controller plugged in and the libraries installed:
/// `BOTELLA_PRUEBA=<scratch bottle> cargo test -- --ignored wired_xbox_pad_reaches_wine`.
/// Wine's own trace must show the pad arriving as an XInput device.
#[test]
#[ignore = "runs the real engine and needs a wired \"for Xbox\" controller plugged in"]
fn wired_xbox_pad_reaches_wine_for_real() {
    let wine = Engine::Metal.installed().expect("the Metal engine");
    assert!(controller_libraries().is_some(), "no controller libraries");
    let prefix = std::env::var("BOTELLA_PRUEBA").expect("BOTELLA_PRUEBA");
    let out = Command::new(&wine)
        .args(["cmd", "/c", "ping -n 7 127.0.0.1 >nul"])
        .envs(runtime_env(&wine))
        .env("WINEPREFIX", &prefix)
        .env("WINEDEBUG", "-all,+hid")
        .output()
        .unwrap();
    let trace = String::from_utf8_lossy(&out.stderr).to_string();
    let found = trace
        .lines()
        .find(|l| l.contains("WINEXINPUT") && l.contains("&IG_"));
    assert!(found.is_some(), "no XInput device in Wine's trace");
    println!("{}", found.unwrap());
}

/// Only the Metal engine changes a program's environment.
#[test]
fn only_the_metal_engine_needs_an_environment() {
    assert!(runtime_env(Path::new("/opt/homebrew/bin/wine")).is_empty());
    assert_eq!(
        Engine::of(Path::new("/opt/homebrew/bin/wine")),
        Engine::Standard
    );
    assert_eq!(
        Engine::of(Path::new(
            "/x/Game Porting Toolkit.app/Contents/Resources/wine/bin/wine64"
        )),
        Engine::Games
    );
}
