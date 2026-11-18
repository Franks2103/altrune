//! Tests that the downloaded packages are pinned to exact versions and read back by SHA-256.

use crate::backend::D3D12_PROGRAMS;
use crate::wine_setup::*;

/// The Metal engine is two downloads of one exact version each; a wrong or tampered file
/// must never be unpacked, so each carries its SHA-256.
#[test]
fn metal_packages_are_pinned() {
    for k in [&METAL_WINE, &METAL_LAYER] {
        assert!(k.url.starts_with(
            "https://github.com/gauthierpiarrette/highball-engine/releases/download/"
        ));
        assert!(k.url.ends_with(k.name), "{}", k.url);
        assert_eq!(k.sha256.len(), 64);
        assert!(k
            .sha256
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
        assert!(k.size > 1_000_000);
    }
}

/// The programs that get D3DMetal are plain lower-case names without an extension.
#[test]
fn d3d12_programs_are_plain_names() {
    assert!(D3D12_PROGRAMS
        .split(':')
        .all(|p| !p.is_empty() && p == p.to_lowercase() && !p.ends_with(".exe")));
}

/// The source knows no place to get Apple's Game Porting Toolkit or D3DMetal: no address of the
/// toolkit's repository on GitHub, no API path to look up its releases, and the only address
/// of that GitHub account that remains is the one of the usual Wine builds.
#[test]
fn no_source_file_knows_where_to_download_apples_toolkit() {
    fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let mut files = vec![];
    rust_files(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(files.len() > 50, "the scan found {} files", files.len());
    // The needles are written in pieces so that this file does not contain them either.
    let toolkit = ["game", "porting", "toolkit"].join("-");
    let account = ["gcenx", ""].join("/");
    let allowed = ["macos", "wine", "builds"].join("_");
    let releases = ["games", "api"].join("_");
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap().to_lowercase();
        let name = file.display();
        assert!(
            !text.contains(&toolkit),
            "{name} names the toolkit's repository"
        );
        assert!(
            !text.contains(&releases),
            "{name} looks up the games releases"
        );
        // No text offers to download Apple's software or says its licence allows it.
        for offered in [
            ["con la licencia", "de apple"].join(" "),
            ["descarga ", "«juegos"].concat(),
            ["añadir direct3d", " 12"].concat(),
        ] {
            assert!(!text.contains(&offered), "{name} says «{offered}»");
        }
        for (at, _) in text.match_indices(&account) {
            assert!(
                text[at + account.len()..].starts_with(&allowed),
                "{name} points at an address of that account that is not the usual Wine"
            );
        }
    }
}

#[test]
fn sha256_is_read_from_the_file() {
    let f = std::env::temp_dir().join(format!("bl-sha-{}", std::process::id()));
    std::fs::write(&f, b"abc").unwrap();
    assert_eq!(
        sha256_of(&f).as_deref(),
        Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    let _ = std::fs::remove_file(&f);
    assert_eq!(sha256_of(&f), None);
}
