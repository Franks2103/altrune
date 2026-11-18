//! Static checks on the installer image: tools/crear-dmg.sh, tools/dmg/ and assets/dmg/. They
//! only read files (no hdiutil, no network, no build of the image):
//! the image itself is built and verified by `sh tools/crear-dmg.sh`.

use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn read_bytes(rel: &str) -> Vec<u8> {
    std::fs::read(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The lines of a shell script or Python file that are not comments.
fn code(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect()
}

/// The lines of a Swift file that are not comments.
fn code_slash(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect()
}

/// Width and height from the IHDR chunk of a PNG.
fn png_size(rel: &str) -> (u32, u32) {
    let b = read_bytes(rel);
    assert_eq!(&b[..8], b"\x89PNG\r\n\x1a\n", "{rel} is not a PNG");
    assert_eq!(&b[12..16], b"IHDR", "{rel}: the first chunk is not IHDR");
    let w = u32::from_be_bytes(b[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(b[20..24].try_into().unwrap());
    (w, h)
}

/// The (width, height) of every image (IFD) of a TIFF file, in file order.
fn tiff_sizes(rel: &str) -> Vec<(u32, u32)> {
    let b = read_bytes(rel);
    let little = match &b[..2] {
        b"II" => true,
        b"MM" => false,
        other => panic!("{rel}: not a TIFF ({other:?})"),
    };
    let u16_at = |at: usize| {
        let v = [b[at], b[at + 1]];
        if little {
            u16::from_le_bytes(v)
        } else {
            u16::from_be_bytes(v)
        }
    };
    let u32_at = |at: usize| {
        let v = [b[at], b[at + 1], b[at + 2], b[at + 3]];
        if little {
            u32::from_le_bytes(v)
        } else {
            u32::from_be_bytes(v)
        }
    };
    assert_eq!(u16_at(2), 42, "{rel}: bad TIFF magic");
    let mut sizes = vec![];
    let mut ifd = u32_at(4) as usize;
    while ifd != 0 {
        let entries = u16_at(ifd) as usize;
        let (mut w, mut h) = (0, 0);
        for i in 0..entries {
            let e = ifd + 2 + i * 12;
            let value = match u16_at(e + 2) {
                3 => u16_at(e + 8) as u32,
                4 => u32_at(e + 8),
                _ => continue,
            };
            match u16_at(e) {
                256 => w = value,
                257 => h = value,
                _ => {}
            }
        }
        sizes.push((w, h));
        ifd = u32_at(ifd + 2 + entries * 12) as usize;
    }
    sizes
}

#[test]
fn requirements_pin_every_package_with_its_hash() {
    let req = read("tools/dmg/requirements.txt");
    let packages: Vec<&str> = req
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("--hash"))
        .collect();
    assert!(
        packages.iter().any(|p| p.starts_with("dmgbuild==2.0.0")),
        "dmgbuild 2.0.0 is pinned: {packages:?}"
    );
    for p in &packages {
        let spec = p.trim_end_matches('\\').trim();
        assert!(spec.contains("=="), "{spec}: pin an exact version");
        assert!(!spec.contains("://") && !spec.starts_with('-'), "{spec}");
    }
    let hashes: Vec<&str> = req
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("--hash=sha256:"))
        .collect();
    assert!(hashes.len() >= packages.len(), "a hash for every package");
    for h in hashes {
        let hex = h
            .trim_start_matches("--hash=sha256:")
            .trim_end_matches('\\')
            .trim();
        assert_eq!(hex.len(), 64, "{h}");
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()), "{h}");
    }
}

#[test]
fn the_installer_script_installs_only_hashed_wheels() {
    let script = read("tools/crear-dmg.sh");
    let pip = code(&script)
        .into_iter()
        .find(|l| l.contains("pip install"))
        .expect("the script installs dmgbuild with pip");
    for flag in [
        "--require-hashes",
        "--only-binary=:all:",
        "tools/dmg/requirements.txt",
    ] {
        assert!(pip.contains(flag), "pip install lacks {flag}: {pip}");
    }
    assert!(!pip.contains("--no-verify") && !pip.contains("--trusted-host"));
    assert!(script.contains("target/dmg-venv"));
}

#[test]
fn nothing_hides_extensions_or_runs_setfile() {
    // SetFile -a E on the app writes com.apple.FinderInfo on the bundle and breaks
    // `codesign --verify --strict`.
    for rel in [
        "tools/dmg/ajustes.py",
        "tools/crear-dmg.sh",
        "tools/dmg/captura.sh",
    ] {
        let text = read(rel);
        for line in code(&text) {
            assert!(!line.contains("hide_extensions"), "{rel}: {line}");
            assert!(!line.contains("SetFile"), "{rel}: {line}");
        }
    }
}

#[test]
fn settings_have_no_absolute_paths() {
    let settings = read("tools/dmg/ajustes.py");
    for line in code(&settings) {
        let code_part = line.split(" #").next().unwrap();
        for quote in ['"', '\''] {
            for (i, piece) in code_part.split(quote).enumerate() {
                // Odd pieces are the contents of a string literal.
                if i % 2 == 1 && piece.starts_with('/') {
                    assert_eq!(
                        piece, "/Applications",
                        "an absolute path in ajustes.py: {line}"
                    );
                }
            }
        }
        for banned in ["/Users", "/private", "/tmp", "/var/", "/Volumes", "~/"] {
            assert!(!code_part.contains(banned), "{banned} in {line}");
        }
    }
    // Everything that depends on the machine comes in through -D defines.
    for define in ["app", "icono", "fondo", "formato", "tamano"] {
        assert!(
            settings.contains(&format!("defines[\"{define}\"]")),
            "{define} must come from -D"
        );
    }
}

#[test]
fn the_window_matches_the_background_design() {
    let settings = read("tools/dmg/ajustes.py");
    // Frame height = 400 content + the 32 point title bar; icons on the quarter points.
    assert!(settings.contains("window_rect = ((200, 120), (660, 432))"));
    assert!(settings.contains("\"Altrune.app\": (165, 190)"));
    assert!(settings.contains("\"Applications\": (495, 190)"));
    assert!(settings.contains("icon_size = 128"));
    assert!(settings.contains("\"Applications\": \"/Applications\""));
    let svg = read("assets/dmg/fondo.svg");
    assert!(svg.contains("(165, 190)") && svg.contains("(495, 190)"));
}

#[test]
fn the_installer_script_never_installs_opens_or_signs() {
    for rel in ["tools/crear-dmg.sh", "tools/dmg/captura.sh"] {
        let text = read(rel);
        let lines = code(&text);
        for line in &lines {
            // /Applications is only read (the link target is compared), never written to.
            let line = &line.replace("->", "to");
            if line.contains("/Applications") {
                for writer in [
                    "cp ",
                    "ditto",
                    "mv ",
                    "rsync",
                    "install ",
                    "ln ",
                    "> /Applications",
                ] {
                    assert!(
                        !line.contains(writer),
                        "{rel} writes to /Applications: {line}"
                    );
                }
            }
            assert!(!line.contains("~/Applications"), "{rel}: {line}");
            for banned in [
                "osascript",
                "killall",
                "pkill",
                "defaults write",
                "defaults delete",
                "security ",
                "notarytool",
                "productsign",
                "tell application",
            ] {
                assert!(
                    !line.contains(banned),
                    "{rel} must not use {banned}: {line}"
                );
            }
            if line.contains("codesign") {
                assert!(line.contains("--verify"), "{rel} may only verify: {line}");
                assert!(
                    !line.contains("--sign") && !line.contains("--force"),
                    "{line}"
                );
            }
        }
    }
    // crear-dmg.sh does not even call open; only captura.sh does, to mount the image like a person.
    let build = read("tools/crear-dmg.sh");
    for line in code(&build) {
        let words: Vec<&str> = line.split_whitespace().collect();
        assert!(
            !words.contains(&"open") && !line.contains("/usr/bin/open"),
            "{line}"
        );
    }
    let capture = read("tools/dmg/captura.sh");
    let opens: Vec<&str> = code(&capture)
        .into_iter()
        .filter(|l| l.split_whitespace().next() == Some("open"))
        .collect();
    assert_eq!(opens, ["open \"$DMG\""], "captura.sh opens only the image");
}

#[test]
fn the_installer_script_removes_only_its_own_temp_paths() {
    for rel in [
        "tools/crear-dmg.sh",
        "tools/dmg/captura.sh",
        "tools/dmg/generar-fondo.sh",
    ] {
        let text = read(rel);
        for line in code(&text).into_iter().filter(|l| l.contains("rm -")) {
            assert!(line.contains("rm -rf \"$T\""), "{rel}: {line}");
            assert!(
                line.contains("*/altrune-"),
                "{rel} guards the pattern: {line}"
            );
        }
        assert!(
            !code(&text)
                .iter()
                .any(|l| l.split_whitespace().next() == Some("rm")),
            "{rel}: a bare rm"
        );
    }
}

#[test]
fn the_installer_script_names_the_image_after_the_cargo_version() {
    let script = read("tools/crear-dmg.sh");
    assert!(script.contains("Altrune-$FULL.dmg"));
    assert!(script.contains("Altrune-$FULL.dmg.sha256"));
    assert!(script.contains("shasum -a 256"));
    // The version is read from Cargo.toml, never typed in the script.
    assert!(script.contains("Cargo.toml"));
    assert!(!code(&script).iter().any(|l| l.contains("0.1.0")));
    // The app's version is the numeric core and its identity is checked before building.
    assert!(script.contains("CFBundleShortVersionString") && script.contains("\"$CORE\""));
    assert!(script.contains("com.altrune.app"));
    // Read-only, no auto-open, no Finder window, and the volume name is guarded.
    assert!(script.contains("hdiutil attach -readonly -nobrowse -noautoopen"));
    assert!(script.contains("/Volumes/Altrune"));
    assert!(script.contains("hdiutil verify"));
    assert!(script.contains("terceros.html") && script.contains("--prueba"));
}

#[test]
fn background_images_have_the_right_sizes() {
    assert_eq!(png_size("assets/dmg/fondo.png"), (660, 416));
    assert_eq!(png_size("assets/dmg/fondo@2x.png"), (1320, 832));
    assert_eq!(
        tiff_sizes("assets/dmg/fondo.tiff"),
        [(660, 416), (1320, 832)],
        "the HiDPI TIFF holds the 1x and the 2x picture"
    );
}

#[test]
fn background_source_has_no_logo_and_the_agreed_sentence() {
    let svg = read("assets/dmg/fondo.svg");
    assert!(svg.contains("Drag Altrune to Applications"));
    // No live text (fonts differ between Macs) and no picture: the logo is never drawn or
    // embedded here, the icons are the real ones.
    for banned in ["<text", "<image", "data:image", "href="] {
        assert!(!svg.contains(banned), "the SVG must not contain {banned}");
    }
    assert!(svg.contains("viewBox=\"0 0 660 416\""));
}

#[test]
fn the_sentence_is_not_traced_from_an_apple_font() {
    // THIRD_PARTY_NOTICES.md: no Apple font is copied anywhere, outlines included. The tracer
    // takes a font file by path; the committed sentence comes from the font egui ships.
    let tracer = read("tools/dmg/trazar-texto.swift");
    let lines = code_slash(&tracer);
    for banned in ["systemFont", "NSFont", "SFNS", "San Francisco"] {
        assert!(
            !lines.iter().any(|l| l.contains(banned)),
            "the tracer must not use {banned}"
        );
    }
    assert!(tracer.contains("CTFontManagerCreateFontDescriptorsFromURL"));
    let svg = read("assets/dmg/fondo.svg");
    assert!(svg.contains("Ubuntu-Light.ttf") && svg.contains("No Apple font is used"));
}

#[test]
fn the_installer_script_does_not_depend_on_the_locale_or_resume_after_a_signal() {
    for rel in [
        "tools/crear-dmg.sh",
        "tools/dmg/captura.sh",
        "tools/dmg/generar-fondo.sh",
    ] {
        let text = read(rel);
        let lines = code(&text);
        assert!(
            lines.iter().any(|l| l.contains("'exit 130' INT")),
            "{rel} must end on INT"
        );
        assert!(
            lines.iter().any(|l| l.contains("'exit 143' TERM")),
            "{rel} must end on TERM"
        );
        assert!(
            !lines.iter().any(|l| l.contains("EXIT INT TERM")),
            "{rel} must not run cleanup as the handler of INT and TERM"
        );
    }
    let text = read("tools/crear-dmg.sh");
    for line in code(&text) {
        if line.contains("| sort") {
            assert!(
                line.contains("LC_ALL=C sort"),
                "sort without C collation: {line}"
            );
        }
    }
    assert!(
        text.contains("utf-16-be"),
        ".DS_Store is scanned as UTF-16BE too"
    );
}
