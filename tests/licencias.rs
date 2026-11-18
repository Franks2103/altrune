//! Checks on the third-party licence bundle: licencias/terceros.html (made by `sh tools/licencias.sh`
//! with cargo-about), its template, about.toml and the script. They only read the repository files;
//! cargo-about itself is not needed to run them. That the committed page matches what the tool
//! writes today is checked by `sh tools/licencias.sh --comprobar`.

use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The text of a file without the comment lines (`#` for sh and toml).
fn code_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// The strings of `key = [ "a", "b" ]` in a TOML file, for the top-level array that starts at
/// the line `key = [` (or `key = ["a", ...]` on one line).
fn toml_array(text: &str, key: &str) -> Vec<String> {
    let start = text
        .lines()
        .position(|l| l.trim_start().starts_with(&format!("{key} = [")))
        .unwrap_or_else(|| panic!("no `{key} = [` in the file"));
    let mut out = Vec::new();
    for line in text.lines().skip(start) {
        let line = line.split('#').next().unwrap_or("");
        let mut rest = line;
        while let Some(a) = rest.find('"') {
            let Some(b) = rest[a + 1..].find('"') else {
                break;
            };
            out.push(rest[a + 1..a + 1 + b].to_string());
            rest = &rest[a + b + 2..];
        }
        if line.contains(']') {
            break;
        }
    }
    out
}

#[test]
fn the_page_exists_and_is_not_empty() {
    let html = read("licencias/terceros.html");
    assert!(
        html.len() > 50_000,
        "terceros.html is suspiciously small ({} bytes); regenerate it with sh tools/licencias.sh",
        html.len()
    );
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<html lang=\"es\">"));
    assert!(html.contains("</html>"));
    assert!(html.contains("Licencias de terceros de Altrune"));
}

#[test]
fn the_page_has_no_machine_paths() {
    let html = read("licencias/terceros.html");
    for bad in [
        // Split so that this file does not contain the pattern it forbids.
        concat!("/Us", "ers/"),
        "/home/",
        "/private/",
        "/var/folders",
        "/tmp/",
        ".cargo/registry",
        "index.crates.io",
        "C:\\Users",
    ] {
        assert!(
            !html.contains(bad),
            "terceros.html contains a machine path ({bad}); the template must never print source_path or manifest_path"
        );
    }
}

#[test]
fn the_page_names_the_key_crates() {
    let html = read("licencias/terceros.html");
    // Every row of the table of crates is `<td>name</td><td>version</td>`.
    for name in [
        "egui",
        "eframe",
        "winit",
        "accesskit",
        "objc2",
        "serde",
        "self_cell",
        "epaint_default_fonts",
    ] {
        assert!(
            html.contains(&format!("<tr><td>{name}</td><td>")),
            "{name} is not in the table of crates of terceros.html"
        );
    }
}

#[test]
fn the_page_lists_the_direct_dependencies() {
    let html = read("licencias/terceros.html");
    let cargo = read("Cargo.toml");
    let deps = cargo
        .split("[dependencies]")
        .nth(1)
        .expect("Cargo.toml has [dependencies]")
        .split("\n[")
        .next()
        .unwrap();
    let mut seen = 0;
    for line in deps.lines() {
        let Some((name, _)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || name.starts_with('#') {
            continue;
        }
        seen += 1;
        assert!(
            html.contains(&format!("<tr><td>{name}</td><td>")),
            "the direct dependency {name} is missing from terceros.html; run sh tools/licencias.sh"
        );
    }
    assert!(seen >= 7, "expected the 7 direct dependencies, saw {seen}");
}

#[test]
fn no_unknown_licence_and_no_placeholder_text() {
    let html = read("licencias/terceros.html");
    for bad in [
        "NOASSERTION",
        "SIN CLASIFICAR",
        "<copyright holders>",
        "&lt;copyright holders&gt;",
        "&lt;year&gt;",
        "Unknown",
        "UNKNOWN",
    ] {
        assert!(
            !html.contains(bad),
            "terceros.html contains \"{bad}\": a crate has no real licence text or is not in a group of plantilla.hbs"
        );
    }
    // The licences of the summary table are only the expected ones.
    let allowed = [
        "MIT",
        "Apache-2.0",
        "Zlib",
        "ISC",
        "BSD-3-Clause",
        "BSD-2-Clause",
        "0BSD",
        "Unlicense",
        "Unicode-3.0",
        "OFL-1.1",
        "Ubuntu-font-1.0",
        "Bitstream-Vera",
    ];
    let summary = html
        .split("<h2>Resumen</h2>")
        .nth(1)
        .and_then(|s| s.split("</table>").next())
        .expect("summary table");
    let mut ids = 0;
    for part in summary.split("<code>").skip(1) {
        let id = part.split("</code>").next().unwrap();
        ids += 1;
        assert!(allowed.contains(&id), "licence {id} is not an expected one");
    }
    assert!(ids >= 6, "the summary lists only {ids} licences");
}

#[test]
fn the_font_licences_and_the_self_cell_choice_are_there() {
    let html = read("licencias/terceros.html");
    for needle in [
        "UBUNTU FONT LICENCE Version 1.0",
        "Copyright 2011 Canonical Ltd.",
        "BITSTREAM VERA LICENSE",
        "Copyright (c) 2018 Source Foundry Authors",
        "Copyright (c) 2014 John Slegers",
        "SIL OPEN FONT LICENSE Version 1.1",
        "Apache-2.0 OR GPL-2.0-only",
        "Altrune toma la opción <strong>Apache-2.0</strong>",
        "Copyright 2015 The Chromium Authors",
    ] {
        assert!(html.contains(needle), "terceros.html lacks: {needle}");
    }
}

#[test]
fn the_crates_without_a_licence_file_get_their_holder() {
    let html = read("licencias/terceros.html");
    for holder in [
        "Copyright (c) 2018-2021 Emil Ernerfeldt",
        "Copyright (c) The AccessKit contributors",
        "Copyright (c) Steven Sheldon",
        "Copyright (c) Mads Marquart",
        "Copyright (c) 2020 Philip Degarmo and other contributors",
    ] {
        assert!(html.contains(holder), "terceros.html lacks: {holder}");
    }
}

#[test]
fn the_page_is_self_contained() {
    let html = read("licencias/terceros.html");
    for bad in [
        "<script", "<link", "<img", "<iframe", "src=", "href=", "@import", "url(",
    ] {
        assert!(
            !html.contains(bad),
            "terceros.html is not self-contained: {bad}"
        );
    }
    assert!(html.contains("prefers-color-scheme: dark"));
    assert!(html.contains("<style>"));
}

#[test]
fn the_template_never_prints_machine_data_or_dates() {
    let tpl = read("licencias/plantilla.hbs");
    // `source_path` may be tested ({{#if source_path}}) but never printed.
    assert!(!tpl.contains("{{source_path}}"));
    assert!(!tpl.contains("{{{source_path}}}"));
    assert!(!tpl.contains("manifest_path"));
    assert!(!tpl.contains("{{path}}"));
    for bad in ["<script", "<link", "src=", "href=", "@import", "url("] {
        assert!(
            !tpl.contains(bad),
            "the template is not self-contained: {bad}"
        );
    }
}

#[test]
fn the_script_pins_one_cargo_about_version() {
    let sh = read("tools/licencias.sh");
    let pins: Vec<&str> = sh
        .lines()
        .filter(|l| l.starts_with("CARGO_ABOUT_VERSION="))
        .collect();
    assert_eq!(
        pins.len(),
        1,
        "exactly one CARGO_ABOUT_VERSION line expected"
    );
    let version = pins[0]
        .trim_start_matches("CARGO_ABOUT_VERSION=")
        .trim_matches('"');
    let parts: Vec<&str> = version.split('.').collect();
    assert_eq!(parts.len(), 3, "version must be x.y.z, got {version}");
    assert!(parts
        .iter()
        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())));
    // The install and the check both use the variable, never a second literal version.
    let code = code_lines(&sh).join("\n");
    assert!(code.contains("--version \"$CARGO_ABOUT_VERSION\""));
    assert!(code.contains("--locked --root \"$DIR\""));
    assert!(code.contains("--locked --offline --fail"));
    assert!(
        !code
            .lines()
            .filter(|l| !l.starts_with("CARGO_ABOUT_VERSION="))
            .any(|l| l.contains(version)),
        "the version number must appear only on the CARGO_ABOUT_VERSION line"
    );
}

#[test]
fn the_script_uses_the_network_only_to_install() {
    let sh = read("tools/licencias.sh");
    let code = code_lines(&sh);
    for line in &code {
        for bad in ["curl", "wget", "git clone", "pip ", "brew "] {
            assert!(!line.contains(bad), "network or install command in: {line}");
        }
    }
    let joined = code.join("\n");
    // `cargo install` appears once, inside the --instalar branch.
    assert_eq!(
        joined
            .lines()
            .filter(|l| l.starts_with("cargo install"))
            .count(),
        1
    );
    let install_at = joined.find("cargo install").unwrap();
    let branch_at = joined.find("if [ \"$MODE\" = instalar ]").unwrap();
    let exit_at = branch_at + joined[branch_at..].find("exit 0").unwrap();
    assert!(branch_at < install_at && install_at < exit_at);
    // Nothing outside licencias/ and the temporary folder is written, and nothing is installed
    // into the real cargo folder.
    assert!(!joined.contains("~/.cargo"));
    assert!(!joined.contains("$HOME"));
    assert!(!joined.contains("/Applications"));
    // The only `rm -rf` removes the temporary folder.
    for l in joined.lines().filter(|l| l.contains("rm -rf")) {
        assert!(l.contains("\"$T\""), "unexpected rm -rf: {l}");
    }
}

#[test]
fn about_toml_targets_both_macs_and_matches_deny_toml() {
    let about = read("about.toml");
    let targets = toml_array(&about, "targets");
    assert_eq!(
        targets,
        ["aarch64-apple-darwin", "x86_64-apple-darwin"],
        "about.toml must use the two macOS targets"
    );
    assert!(code_lines(&about).contains(&"ignore-dev-dependencies = true"));
    assert!(code_lines(&about).contains(&"private = { ignore = true }"));

    let deny = read("deny.toml");
    let deny_targets = toml_array(&deny, "targets");
    assert_eq!(
        targets, deny_targets,
        "about.toml and deny.toml use different targets"
    );

    // Every licence about.toml accepts for all crates is also on the allow list of deny.toml.
    let deny_allow = toml_array(&deny, "allow");
    for lic in toml_array(&about, "accepted") {
        assert!(
            deny_allow.contains(&lic),
            "{lic} is accepted by about.toml but not allowed by deny.toml"
        );
    }
    // The extra licences are only for epaint_default_fonts: its own section, never the global list.
    let global = about.split("\n[").next().unwrap();
    for font in ["OFL-1.1", "Ubuntu-font-1.0", "Bitstream-Vera"] {
        assert!(
            !global.contains(&format!("\"{font}\"")),
            "{font} must not be accepted globally"
        );
    }
    assert!(about.contains("[epaint_default_fonts]"));
    assert!(about.contains("[epaint_default_fonts.clarify]"));
    // GPL is never accepted.
    assert!(!about
        .lines()
        .any(|l| !l.trim_start().starts_with('#') && l.contains("GPL")));
}

#[test]
fn the_licence_files_of_the_bundle_exist_in_the_repository() {
    // empaquetar.sh copies these four next to the app; they must exist.
    for f in [
        "LICENSE",
        "NOTICE",
        "THIRD_PARTY_NOTICES.md",
        "licencias/terceros.html",
    ] {
        let p = root().join(f);
        assert!(p.is_file(), "{f} is missing");
        assert!(p.metadata().unwrap().len() > 0, "{f} is empty");
    }
}

#[test]
fn the_notices_document_says_where_the_bundle_ships_and_how_to_make_it() {
    let notices = read("THIRD_PARTY_NOTICES.md");
    for needle in [
        "Contents/Resources/Licencias",
        "sh tools/licencias.sh",
        "sh tools/licencias.sh --comprobar",
        "licencias/terceros.html",
        "about.toml",
    ] {
        assert!(
            notices.contains(needle),
            "THIRD_PARTY_NOTICES.md lacks: {needle}"
        );
    }
}

#[test]
fn the_prose_of_the_template_matches_the_data() {
    // These statements are written by hand in the template, so nothing regenerates them when
    // about.toml or Cargo.lock change. Check that they still agree with the data.
    let template = read("licencias/plantilla.hbs");
    let html = read("licencias/terceros.html");

    // The order of preference printed on the page is the `accepted` array of about.toml.
    let accepted = toml_array(&read("about.toml"), "accepted").join(", ");
    assert!(
        template.contains(&format!("se toma la primera de esta lista: {accepted}.")),
        "the list of accepted licences in plantilla.hbs differs from `accepted` in about.toml: {accepted}"
    );

    // The crates the template names as Apache-2.0 only really are, in the generated table.
    for name in [
        "winit",
        "glutin",
        "accesskit_winit",
        "unicode-general-category",
    ] {
        assert!(
            template.contains(&format!("<strong>{name}</strong>")),
            "{name} is no longer named in plantilla.hbs"
        );
        let row = format!("<tr><td>{name}</td><td>");
        let line = html
            .lines()
            .find(|l| l.starts_with(&row))
            .unwrap_or_else(|| panic!("{name} is not in the table of terceros.html"));
        assert!(
            line.ends_with("</td><td>Apache-2.0</td></tr>"),
            "plantilla.hbs says {name} is Apache-2.0 only, but its row is: {line}"
        );
    }

    // The choice for self_cell: it offers Apache-2.0 or the GPL, and the page says Apache-2.0.
    assert!(
        html.lines()
            .any(|l| l.starts_with("<tr><td>self_cell</td><td>")
                && l.ends_with("<td>Apache-2.0 OR GPL-2.0-only</td></tr>")),
        "self_cell no longer offers Apache-2.0 OR GPL-2.0-only: update the note of plantilla.hbs"
    );
}
