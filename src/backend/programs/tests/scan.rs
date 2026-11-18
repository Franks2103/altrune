//! Tests for the program scan, with fake bottles in temporary folders.

use super::super::shell_link::tests::sample_link;
use super::super::*;

/// A bottle folder with a few fake programs, in a folder of its own under the temp dir.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("bl-library-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("bottle")).unwrap();
        Scratch(dir)
    }
    fn prefix(&self) -> PathBuf {
        self.0.join("bottle")
    }
    fn file(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = self.prefix().join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, bytes).unwrap();
        p
    }
    fn exe(&self, rel: &str) {
        self.file(&format!("drive_c/{rel}"), b"MZ");
    }
    fn start_menu(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        self.file(
            &format!("drive_c/ProgramData/Microsoft/Windows/Start Menu/Programs/{rel}"),
            bytes,
        )
    }
    fn scan(&self, settings: Option<&BottleSettings>) -> Vec<Program> {
        scan_programs(&self.prefix(), settings)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn names(list: &[Program]) -> Vec<&str> {
    list.iter().map(|p| p.name.as_str()).collect()
}

#[test]
fn folding_ignores_case_and_accents() {
    for (a, b) in [
        ("Cámara", "CAMARA"),
        ("Ñandú", "nandu"),
        ("Çelik Öztürk", "celik ozturk"),
        ("àéîõü", "aeiou"),
        ("Año Nuevo", "ano nuevo"),
    ] {
        assert_eq!(fold_name(a), fold_name(b), "{a} / {b}");
    }
    assert_ne!(fold_name("Cámara"), fold_name("Camaro"));
}

#[test]
fn noise_is_judged_by_whole_words_and_by_what_the_shortcut_runs() {
    // (name, what it runs, is noise)
    type Runs<'a> = Option<(&'a str, &'a str)>;
    let table: [(&str, Runs, bool); 26] = [
        // Real programs, some with a noise word inside a longer word.
        ("Helper", None, false),
        ("Supportive Tools", None, false),
        ("HelpNDoc", None, false),
        ("Manualidades 3D", None, false),
        ("Photoshop Elements", None, false),
        ("Cámara", None, false),
        ("Steam", None, false),
        // Uninstallers, in both languages.
        ("Uninstall Foo", None, true),
        ("Desinstalar Foo", None, true),
        ("Desinstalación de Foo", None, true),
        ("Remove Foo", None, true),
        ("Eliminar Foo", None, true),
        ("Foo - Uninstaller", None, true),
        // Documents and help, by name when nothing else is known.
        ("Léame", None, true),
        ("Readme", None, true),
        ("Foo Manual", None, true),
        ("Ayuda de Foo", None, true),
        ("Licencia", None, true),
        ("Release Notes", None, true),
        ("Sitio web de Foo", None, true),
        ("Visit our Website", None, true),
        // What it runs decides when it is known.
        (
            "Foo",
            Some(("C:\\Program Files\\Foo\\unins000.exe", "")),
            true,
        ),
        (
            "Quitar",
            Some(("C:\\Windows\\System32\\msiexec.exe", "/x {1234-5678}")),
            true,
        ),
        (
            "Foo Help",
            Some(("C:\\Windows\\hh.exe", "C:\\Foo\\foo.chm")),
            true,
        ),
        (
            "Support Center",
            Some(("C:\\Program Files\\Foo\\center.exe", "")),
            false,
        ),
        (
            "Foo Setup",
            Some(("C:\\Windows\\System32\\msiexec.exe", "/i foo.msi")),
            false,
        ),
    ];
    for (name, runs, noise) in table {
        assert_eq!(is_noise(name, runs), noise, "{name} {runs:?}");
    }
}

#[test]
fn search_matches_names_without_accents_and_the_start_menu_folder() {
    let camera = Program {
        name: "Cámara Pro".into(),
        folder: "Fotografía › Herramientas".into(),
        ..Program::default()
    };
    assert!(matches_search(&camera, &fold_name("camara")));
    assert!(matches_search(&camera, &fold_name("CÁMARA")));
    assert!(matches_search(&camera, &fold_name("fotografia")));
    assert!(matches_search(&camera, ""));
    assert!(!matches_search(&camera, &fold_name("camaro")));
}

#[test]
fn shortcuts_are_read_ordered_by_name_and_noise_is_kept_apart() {
    let s = Scratch::new("order");
    s.exe("Program Files/Zeta/zeta.exe");
    s.exe("Program Files/Alfa/alfa.exe");
    s.exe("Program Files/Alfa/unins000.exe");
    s.start_menu(
        "zeta.lnk",
        &sample_link("C:\\Program Files\\Zeta\\zeta.exe", ""),
    );
    s.start_menu(
        "Árbol.lnk",
        &sample_link("C:\\Program Files\\Alfa\\alfa.exe", ""),
    );
    s.start_menu(
        "Alfa/Desinstalar Alfa.lnk",
        &sample_link("C:\\Program Files\\Alfa\\unins000.exe", ""),
    );
    s.start_menu("Helper.lnk", b"not a shortcut at all");
    s.start_menu("Alfa/Léame.lnk", b"");
    s.start_menu(
        "Sitio.url",
        b"[InternetShortcut]\r\nURL=https://example.org/\r\n",
    );
    let list = s.scan(None);
    // Shown first, by name without accents ("Árbol" sorts as "arbol", before "helper").
    assert_eq!(
        names(&list),
        [
            "Árbol",
            "Helper",
            "zeta",
            "Desinstalar Alfa",
            "Léame",
            "Sitio"
        ]
    );
    let listing: Vec<_> = list.iter().map(|p| p.listing).collect();
    assert_eq!(
        listing,
        [
            Listing::Shown,
            Listing::Shown,
            Listing::Shown,
            Listing::Noise,
            Listing::Noise,
            Listing::Noise
        ]
    );
    // The target of a readable shortcut is the file inside the bottle.
    assert_eq!(
        list[0].target.as_deref(),
        Some(
            s.prefix()
                .join("drive_c/Program Files/Alfa/alfa.exe")
                .as_path()
        )
    );
    assert_eq!(list[1].target, None);
    // The tabs count only what is shown.
    let bottle = Bottle {
        name: "t".into(),
        path: s.prefix(),
        windows: "win10".into(),
        msync: true,
        high_res: false,
        logpixels: 96,
        programs: list,
        env: vec![],
        prefix_complete: true,
    };
    assert_eq!(bottle.shown_count(), 3);
    assert_eq!(bottle.tucked_programs().count(), 3);
    assert_eq!(
        bottle.shown_programs().map(|(i, _)| i).collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn one_program_under_two_shortcuts_shows_once() {
    let s = Scratch::new("same-target");
    s.exe("Program Files/Foo/foo.exe");
    let link = sample_link("C:\\Program Files\\Foo\\foo.exe", "");
    s.start_menu("Foo.lnk", &link);
    s.start_menu("Juegos/Foo (acceso directo).lnk", &link);
    s.file("drive_c/users/me/Desktop/Foo.lnk", &link);
    // The same exe with other arguments is another program.
    s.start_menu(
        "Foo ventana.lnk",
        &sample_link("C:\\Program Files\\Foo\\foo.exe", "-windowed"),
    );
    let list = s.scan(None);
    assert_eq!(names(&list), ["Foo", "Foo ventana"]);
}

#[test]
fn wine_shortcuts_with_a_counted_nul_are_the_same_program() {
    // Steam's installer under Wine leaves Steam.lnk in the common Start Menu (arguments "\0")
    // and in the user's own Start Menu (no arguments): one program, one tile.
    let s = Scratch::new("wine-nul");
    s.exe("Program Files (x86)/Steam/steam.exe");
    s.start_menu(
        "Steam/Steam.lnk",
        &sample_link("C:\\Program Files (x86)\\Steam\\steam.exe", "\0"),
    );
    s.file(
        "drive_c/users/legacy/AppData/Roaming/Microsoft/Windows/Start Menu/Programs/Steam/Steam.lnk",
        &sample_link("C:\\Program Files (x86)\\Steam\\steam.exe", ""),
    );
    assert_eq!(names(&s.scan(None)), ["Steam"]);
}

#[test]
fn a_program_picked_by_hand_and_its_shortcut_are_one() {
    let s = Scratch::new("saved");
    s.exe("Program Files/Foo/foo.exe");
    s.start_menu(
        "Mi Foo.lnk",
        &sample_link("C:\\Program Files\\Foo\\foo.exe", ""),
    );
    let settings = BottleSettings {
        programs: vec![SavedProgram {
            name: "Foo".into(),
            path: s
                .prefix()
                .join("drive_c/Program Files/Foo/foo.exe")
                .to_string_lossy()
                .to_string(),
            args: "-x".into(),
            env: vec![],
        }],
        ..BottleSettings::default()
    };
    let list = s.scan(Some(&settings));
    assert_eq!(names(&list), ["Foo"]);
    assert!(list[0].saved);
    assert_eq!(list[0].args, "-x");
}

#[test]
fn two_different_programs_with_one_name_both_stay_and_say_where_from() {
    let s = Scratch::new("same-name");
    s.exe("Program Files/A/launcher.exe");
    s.exe("Program Files/B/launcher.exe");
    s.start_menu(
        "Alfa/Launcher.lnk",
        &sample_link("C:\\Program Files\\A\\launcher.exe", ""),
    );
    s.start_menu(
        "Beta/Launcher.lnk",
        &sample_link("C:\\Program Files\\B\\launcher.exe", ""),
    );
    s.start_menu("Solo.lnk", b"");
    let list = s.scan(None);
    assert_eq!(names(&list), ["Launcher", "Launcher", "Solo"]);
    assert_eq!(list[0].subtitle, "Alfa");
    assert_eq!(list[1].subtitle, "Beta");
    // A name that is alone gets no subtitle.
    assert_eq!(list[2].subtitle, "");
}

#[test]
fn a_steam_game_on_the_desktop_and_in_the_start_menu_shows_once() {
    let s = Scratch::new("steam-url");
    let url = b"[InternetShortcut]\nURL=steam://rungameid/42\n";
    s.start_menu("Steam/Hades.url", url);
    s.file("drive_c/users/me/Desktop/Hades.url", url);
    // The same name for another game is another program.
    s.file(
        "drive_c/users/me/Desktop/Hades II.url",
        b"[InternetShortcut]\nURL=steam://rungameid/43\n",
    );
    let list = s.scan(None);
    assert_eq!(names(&list), ["Hades", "Hades II"]);
}

#[cfg(unix)]
#[test]
fn drives_that_lead_out_of_the_bottle_are_not_resolved() {
    use std::os::unix::fs::symlink;
    let s = Scratch::new("drives");
    // A "Mac folder" holding a file, and a Z: drive that points at it.
    let mac = s.0.join("mac");
    fs::create_dir_all(&mac).unwrap();
    fs::write(mac.join("setup.exe"), b"MZ").unwrap();
    fs::create_dir_all(s.prefix().join("dosdevices")).unwrap();
    symlink(&mac, s.prefix().join("dosdevices/z:")).unwrap();
    // A D: drive that points inside the bottle.
    fs::create_dir_all(s.prefix().join("drive_c/Datos")).unwrap();
    s.exe("Datos/app.exe");
    symlink(
        s.prefix().join("drive_c/Datos"),
        s.prefix().join("dosdevices/d:"),
    )
    .unwrap();
    assert!(!drive_stays_in_bottle(&s.prefix(), "Z:\\setup.exe"));
    assert!(drive_stays_in_bottle(&s.prefix(), "C:\\Datos\\app.exe"));
    assert!(drive_stays_in_bottle(&s.prefix(), "D:\\app.exe"));
    assert!(!drive_stays_in_bottle(&s.prefix(), "E:\\none.exe"));
    s.start_menu("Fuera.lnk", &sample_link("Z:\\setup.exe", ""));
    s.start_menu("Dentro.lnk", &sample_link("D:\\app.exe", ""));
    let list = s.scan(None);
    let out = list.iter().find(|p| p.name == "Fuera").unwrap();
    assert!(out.target.is_none());
    let inside = list.iter().find(|p| p.name == "Dentro").unwrap();
    assert!(inside.target.is_some());
}

#[cfg(unix)]
#[test]
fn icons_and_paths_do_not_leave_the_bottle() {
    use std::os::unix::fs::symlink;
    let s = Scratch::new("icons-out");
    let mac = s.0.join("mac");
    fs::create_dir_all(&mac).unwrap();
    fs::write(mac.join("foto.png"), b"not really a picture").unwrap();
    fs::create_dir_all(s.prefix().join("dosdevices")).unwrap();
    symlink(&mac, s.prefix().join("dosdevices/z:")).unwrap();
    s.file("drive_c/x.txt", b"x");
    let link = |loc: &str| ShellLink {
        icon_location: Some(loc.to_string()),
        ..ShellLink::default()
    };
    // The Mac's drive is not read for icons, in shortcuts or in .url files.
    assert!(link_icon(&s.prefix(), &link("Z:\\foto.png")).is_none());
    let url = s.file(
        "drive_c/a.url",
        b"[InternetShortcut]\nICONFILE=Z:\\foto.png\n",
    );
    assert!(url_icon(&s.prefix(), &url).is_none());
    // `..` never resolves, even though the file would exist.
    assert!(windows_to_unix(&s.prefix(), "C:\\x.txt").is_some());
    assert!(windows_to_unix(&s.prefix(), "C:\\..\\drive_c\\x.txt").is_none());
    assert!(windows_to_unix(&s.prefix(), "C:\\..\\..\\mac\\foto.png").is_none());
}

#[test]
fn shortcuts_that_cannot_be_read_are_told_apart_by_name_and_folder() {
    let s = Scratch::new("unreadable");
    s.start_menu("Juego.lnk", b"");
    s.start_menu("Otros/Juego.lnk", b"");
    s.file("drive_c/users/me/Desktop/JUEGO.lnk", b"");
    let list = s.scan(None);
    // The desktop one is the Start Menu top one under another case; the folder one differs.
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].subtitle, "Menú Inicio");
    assert_eq!(list[1].subtitle, "Otros");
}

#[test]
fn removed_names_are_compared_without_accents_and_kept_apart() {
    let s = Scratch::new("removed");
    s.start_menu("CAMARA.lnk", b"");
    s.start_menu("Otro.lnk", b"");
    let settings = BottleSettings {
        hidden: vec!["Cámara".into()],
        ..BottleSettings::default()
    };
    let list = s.scan(Some(&settings));
    assert_eq!(names(&list), ["Otro", "CAMARA"]);
    assert_eq!(list[1].listing, Listing::Removed);
}

/// Two different programs called "Launcher", in two Start Menu folders.
fn two_launchers(s: &Scratch) -> (PathBuf, PathBuf) {
    s.exe("Program Files/A/launcher.exe");
    s.exe("Program Files/B/launcher.exe");
    let alfa = s.start_menu(
        "Alfa/Launcher.lnk",
        &sample_link("C:\\Program Files\\A\\launcher.exe", ""),
    );
    let beta = s.start_menu(
        "Beta/Launcher.lnk",
        &sample_link("C:\\Program Files\\B\\launcher.exe", ""),
    );
    (alfa, beta)
}

/// The program that lives in this Start Menu folder ("Alfa"). Programs kept apart (one shown,
/// one removed) are in different lists and need no subtitle, so the folder is what finds them.
fn in_folder<'a>(list: &'a [Program], folder: &str) -> &'a Program {
    list.iter().find(|p| p.folder == folder).unwrap()
}

fn listing_of(list: &[Program], folder: &str) -> Listing {
    in_folder(list, folder).listing
}

#[test]
fn removing_one_of_two_same_name_programs_removes_only_that_one() {
    let s = Scratch::new("remove-by-file");
    let (alfa, _beta) = two_launchers(&s);
    let mut settings = BottleSettings::default();
    settings.hide_file(&s.prefix(), &alfa);
    let list = s.scan(Some(&settings));
    assert_eq!(list.len(), 2);
    assert_eq!(listing_of(&list, "Alfa"), Listing::Removed);
    assert_eq!(listing_of(&list, "Beta"), Listing::Shown);
}

#[test]
fn a_name_removed_by_an_older_version_still_hides_every_program_with_it() {
    let s = Scratch::new("legacy-name");
    two_launchers(&s);
    let settings = BottleSettings {
        hidden: vec!["launcher".into()],
        ..BottleSettings::default()
    };
    let list = s.scan(Some(&settings));
    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|p| p.listing == Listing::Removed));
}

#[test]
fn showing_one_of_two_programs_removed_by_name_leaves_the_other_removed() {
    let s = Scratch::new("legacy-unhide");
    two_launchers(&s);
    let mut settings = BottleSettings {
        hidden: vec!["Launcher".into()],
        ..BottleSettings::default()
    };
    let list = s.scan(Some(&settings));
    let alfa = in_folder(&list, "Alfa").clone();
    settings.unhide_program(&s.prefix(), &alfa, &list);
    // The name entry is gone, replaced by the file of the one that stays removed.
    assert!(settings.hidden.is_empty());
    assert_eq!(settings.hidden_files.len(), 1);
    let list = s.scan(Some(&settings));
    assert_eq!(listing_of(&list, "Alfa"), Listing::Shown);
    assert_eq!(listing_of(&list, "Beta"), Listing::Removed);
}

#[test]
fn showing_a_program_removed_by_file_brings_back_only_it() {
    let s = Scratch::new("unhide-file");
    let (alfa, beta) = two_launchers(&s);
    let mut settings = BottleSettings::default();
    settings.hide_file(&s.prefix(), &alfa);
    settings.hide_file(&s.prefix(), &beta);
    // Removing a file twice writes it once.
    settings.hide_file(&s.prefix(), &beta);
    assert_eq!(settings.hidden_files.len(), 2);
    let list = s.scan(Some(&settings));
    let alfa_program = in_folder(&list, "Alfa").clone();
    settings.unhide_program(&s.prefix(), &alfa_program, &list);
    let list = s.scan(Some(&settings));
    assert_eq!(listing_of(&list, "Alfa"), Listing::Shown);
    assert_eq!(listing_of(&list, "Beta"), Listing::Removed);
}

/// A program added by hand under `rel`, named "Juego".
fn saved(s: &Scratch, rel: &str) -> SavedProgram {
    s.exe(rel);
    SavedProgram {
        name: "Juego".into(),
        path: s
            .prefix()
            .join("drive_c")
            .join(rel)
            .to_string_lossy()
            .to_string(),
        args: String::new(),
        env: vec![],
    }
}

#[test]
fn two_programs_added_by_hand_with_one_name_are_both_listed_and_told_apart() {
    let s = Scratch::new("saved-same-name");
    let mut settings = BottleSettings::default();
    settings.save_program(saved(&s, "Program Files/A/game.exe"));
    settings.save_program(saved(&s, "Program Files/B/game.exe"));
    let list = s.scan(Some(&settings));
    assert_eq!(names(&list), ["Juego", "Juego"]);
    assert!(list.iter().all(|p| p.saved && p.is_shown()));
    assert_ne!(list[0].subtitle, list[1].subtitle);
    assert!(list.iter().all(|p| !p.subtitle.is_empty()));
    // A shortcut to one of them is still that program, not a third one.
    s.start_menu(
        "Juego.lnk",
        &sample_link("C:\\Program Files\\B\\game.exe", ""),
    );
    assert_eq!(s.scan(Some(&settings)).len(), 2);
}

#[test]
fn removing_one_program_added_by_hand_keeps_the_other_with_its_name() {
    let s = Scratch::new("saved-forget");
    let mut settings = BottleSettings::default();
    let first = saved(&s, "Program Files/A/game.exe");
    settings.save_program(first.clone());
    settings.save_program(saved(&s, "Program Files/B/game.exe"));
    settings.forget_program(Path::new(&first.path));
    let list = s.scan(Some(&settings));
    assert_eq!(list.len(), 1);
    assert!(list[0].path.ends_with("B/game.exe"));
    assert_eq!(list[0].subtitle, "");
}

#[test]
fn saving_the_same_file_again_updates_it_instead_of_adding_another() {
    let s = Scratch::new("saved-twice");
    let mut settings = BottleSettings::default();
    let mut first = saved(&s, "Program Files/A/game.exe");
    settings.save_program(first.clone());
    settings.save_program(saved(&s, "Program Files/B/game.exe"));
    first.args = "-windowed".into();
    settings.save_program(first);
    assert_eq!(settings.programs.len(), 2);
    // The entry keeps its place and takes the new options.
    assert_eq!(settings.programs[0].args, "-windowed");
    let list = s.scan(Some(&settings));
    assert_eq!(list.len(), 2);
    assert!(list.iter().any(|p| p.args == "-windowed"));
    // A file saved twice by an older version is listed once.
    settings.programs.push(settings.programs[0].clone());
    assert_eq!(s.scan(Some(&settings)).len(), 2);
}

#[test]
fn the_file_identity_survives_renaming_the_bottle_folder() {
    let s = Scratch::new("renamed");
    let (alfa, _beta) = two_launchers(&s);
    let mut settings = BottleSettings::default();
    settings.hide_file(&s.prefix(), &alfa);
    // The identity is inside the bottle, never an absolute path.
    assert!(settings.hidden_files[0].starts_with("drive_c/"));
    let moved = s.0.join("otra-botella");
    fs::rename(s.prefix(), &moved).unwrap();
    let list = scan_programs(&moved, Some(&settings));
    assert_eq!(list.len(), 2);
    assert_eq!(listing_of(&list, "Alfa"), Listing::Removed);
    assert_eq!(listing_of(&list, "Beta"), Listing::Shown);
}

#[cfg(unix)]
#[test]
fn the_scan_never_follows_links_out_of_the_bottle() {
    use std::os::unix::fs::symlink;
    let s = Scratch::new("links");
    // A "Mac folder" outside the bottle, with a shortcut at its top and one deeper.
    let mac = s.0.join("mac-desktop");
    fs::create_dir_all(mac.join("deep")).unwrap();
    fs::write(mac.join("Top.lnk"), b"").unwrap();
    fs::write(mac.join("deep/Deep.lnk"), b"").unwrap();
    fs::write(
        mac.join("Web.url"),
        "[InternetShortcut]\nURL=https://example.org\n",
    )
    .unwrap();
    fs::write(
        mac.join("Game.url"),
        "[InternetShortcut]\nURL=steam://rungameid/42\n",
    )
    .unwrap();
    let users = s.prefix().join("drive_c/users");
    fs::create_dir_all(users.join("legacy")).unwrap();
    // The Desktop is a link to the Mac folder; the user "me" is a link to "legacy".
    symlink(&mac, users.join("legacy/Desktop")).unwrap();
    symlink("legacy", users.join("me")).unwrap();
    // A Start Menu folder that is a link to the Mac folder.
    let programs = s
        .prefix()
        .join("drive_c/ProgramData/Microsoft/Windows/Start Menu/Programs");
    fs::create_dir_all(&programs).unwrap();
    symlink(&mac, programs.join("Linked")).unwrap();
    s.start_menu("Real.lnk", b"");

    let list = s.scan(None);
    // Only the real shortcut: on a desktop that is a Mac folder, programs are not listed, the
    // web link is dropped, the Steam game belongs to a bottle that has Steam, and nothing
    // deeper or behind a link is read.
    assert_eq!(names(&list), ["Real"]);

    // With Steam installed here, the desktop's Steam game shows up (top level only).
    s.exe("Program Files (x86)/Steam/steam.exe");
    let list = s.scan(None);
    assert_eq!(names(&list), ["Game", "Real"]);
}

#[test]
fn a_desktop_inside_the_bottle_shows_only_steam_games_and_shortcuts_at_its_top() {
    let s = Scratch::new("desktop");
    s.file(
        "drive_c/users/me/Desktop/Web.url",
        b"[InternetShortcut]\nURL=https://example.org\n",
    );
    s.file(
        "drive_c/users/me/Desktop/Game.url",
        b"[InternetShortcut]\nURL=steam://rungameid/7\n",
    );
    s.file("drive_c/users/me/Desktop/Prog.lnk", b"");
    s.file("drive_c/users/me/Desktop/Folder/Hidden.lnk", b"");
    let list = s.scan(None);
    assert_eq!(names(&list), ["Game", "Prog"]);
    assert!(list.iter().all(|p| p.is_shown()));
}

fn shown(list: &[Program]) -> Vec<&Program> {
    list.iter().filter(|p| p.is_shown()).collect()
}

#[test]
fn removing_a_shortcut_opened_with_options_does_not_bring_the_shortcut_back() {
    let s = Scratch::new("saved-shortcut");
    s.exe("Program Files/A/game.exe");
    let lnk = s.start_menu(
        "Juego.lnk",
        &sample_link("C:\\Program Files\\A\\game.exe", ""),
    );
    // «Abrir con opciones» saves the shortcut under its own file.
    let mut settings = BottleSettings::default();
    settings.save_program(SavedProgram {
        name: "Juego".into(),
        path: lnk.to_string_lossy().to_string(),
        args: "-windowed".into(),
        env: vec![],
    });
    let list = s.scan(Some(&settings));
    assert_eq!(list.len(), 1);
    assert!(list[0].saved);
    settings.remove_program(&s.prefix(), &list[0]);
    let list = s.scan(Some(&settings));
    assert!(shown(&list).is_empty(), "{list:?}");
    // It can come back from the removed ones, as the plain shortcut.
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].listing, Listing::Removed);
    settings.unhide_program(&s.prefix(), &list[0].clone(), &list);
    assert_eq!(shown(&s.scan(Some(&settings))).len(), 1);
}

#[test]
fn removing_a_program_added_by_hand_also_removes_its_twin_saved_for_the_same_target() {
    let s = Scratch::new("saved-twin");
    s.exe("Program Files/A/game.exe");
    let lnk = s.start_menu(
        "Juego.lnk",
        &sample_link("C:\\Program Files\\A\\game.exe", ""),
    );
    let exe = s.prefix().join("drive_c/Program Files/A/game.exe");
    let mut settings = BottleSettings::default();
    for file in [&exe, &lnk] {
        settings.save_program(SavedProgram {
            name: "Juego".into(),
            path: file.to_string_lossy().to_string(),
            args: String::new(),
            env: vec![],
        });
    }
    // Two entries for one program are listed once.
    let list = s.scan(Some(&settings));
    assert_eq!(shown(&list).len(), 1);
    settings.remove_program(&s.prefix(), shown(&list)[0]);
    let list = s.scan(Some(&settings));
    assert!(shown(&list).is_empty(), "{list:?}");
    assert!(settings.programs.is_empty());
}

#[test]
fn removing_a_program_added_by_hand_keeps_one_with_another_target() {
    let s = Scratch::new("saved-not-twin");
    let mut settings = BottleSettings::default();
    let first = saved(&s, "Program Files/A/game.exe");
    settings.save_program(first.clone());
    settings.save_program(saved(&s, "Program Files/B/game.exe"));
    let list = s.scan(Some(&settings));
    let program = list
        .iter()
        .find(|p| p.path.ends_with("A/game.exe"))
        .unwrap();
    settings.remove_program(&s.prefix(), program);
    let list = s.scan(Some(&settings));
    assert_eq!(shown(&list).len(), 1);
    assert!(list[0].path.ends_with("B/game.exe"));
}
