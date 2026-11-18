//! The Altrune identity in the interface: no old visible name comes back (a scan of the string
//! literals), Ajustes closes with «Acerca de Altrune», the sidebar and the About logo are cut and
//! uploaded once, and the names the app keeps on purpose are still the ones the folders carry.

use super::driver::Flow;
use crate::ui::brand::{self, Art};
use crate::ui::screens::about::{ABOUT_LICENCES, ABOUT_LOGO_NAME, ABOUT_TITLE, ABOUT_WHAT};
use crate::Action;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------- the scan of string literals

/// What a scan of a source file sees: a string literal and the line it starts on.
#[derive(Debug, PartialEq)]
struct Literal {
    text: String,
    line: usize,
}

/// The string literals of Rust source outside comments and outside what is only compiled for
/// tests (`#[cfg(test)]` items and `#[test]` functions). It understands line and block comments
/// (nested), escapes, raw strings (`r#"..."#`) and character literals such as `'"'`.
fn literals(source: &str) -> Vec<Literal> {
    #[derive(Debug)]
    enum Tok {
        Str(String, usize),
        Word(String),
        Punct(char),
    }
    let c: Vec<char> = source.chars().collect();
    let (mut i, mut line) = (0, 1);
    let mut toks: Vec<Tok> = vec![];
    while i < c.len() {
        let ch = c[i];
        let prev_is_word = i > 0 && (c[i - 1].is_alphanumeric() || c[i - 1] == '_');
        if ch == '\n' {
            line += 1;
            i += 1;
        } else if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            let mut depth = 1;
            i += 2;
            while i < c.len() && depth > 0 {
                if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    line += usize::from(c[i] == '\n');
                    i += 1;
                }
            }
        } else if ch == 'r' && !prev_is_word && raw_start(&c, i).is_some() {
            let (hashes, body) = raw_start(&c, i).unwrap();
            let start_line = line;
            let mut j = body;
            let mut text = String::new();
            while j < c.len() {
                if c[j] == '"' && (0..hashes).all(|k| c.get(j + 1 + k) == Some(&'#')) {
                    break;
                }
                line += usize::from(c[j] == '\n');
                text.push(c[j]);
                j += 1;
            }
            toks.push(Tok::Str(text, start_line));
            i = j + 1 + hashes;
        } else if ch == '"' {
            let start_line = line;
            let mut text = String::new();
            i += 1;
            while i < c.len() && c[i] != '"' {
                if c[i] == '\\' && matches!(c.get(i + 1), Some('\n' | '\r')) {
                    // A line continuation: rustc drops the backslash, the line break and the
                    // next line's indentation, so a name split across lines is one name.
                    i += 1;
                    while i < c.len() && c[i].is_whitespace() {
                        line += usize::from(c[i] == '\n');
                        i += 1;
                    }
                    continue;
                }
                if c[i] == '\\' && i + 1 < c.len() {
                    text.push(c[i]);
                    i += 1;
                }
                line += usize::from(c[i] == '\n');
                text.push(c[i]);
                i += 1;
            }
            i += 1;
            toks.push(Tok::Str(text, start_line));
        } else if ch == '\'' {
            // A character literal ('"', '\n', '\'') or a lifetime ('a).
            if c.get(i + 1) == Some(&'\\') {
                i += 2;
                while i < c.len() && c[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if c.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
            }
        } else if ch.is_alphanumeric() || ch == '_' {
            let mut w = String::new();
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                w.push(c[i]);
                i += 1;
            }
            toks.push(Tok::Word(w));
        } else {
            if !ch.is_whitespace() {
                toks.push(Tok::Punct(ch));
            }
            i += 1;
        }
    }

    // Drop the items that exist only for tests: after `#[cfg(test)]` or `#[test]`, everything up
    // to the end of the item (its `;` or its balanced braces).
    let is = |k: usize, t: &Tok, want: &str| {
        let _ = k;
        matches!(t, Tok::Word(w) if w == want)
            || matches!(t, Tok::Punct(p) if want.len() == 1 && want.starts_with(*p))
    };
    let mut out = vec![];
    let mut k = 0;
    while k < toks.len() {
        let test_attr = is(k, &toks[k], "#")
            && toks.get(k + 1).is_some_and(|t| is(k, t, "["))
            && ((toks.get(k + 2).is_some_and(|t| is(k, t, "cfg"))
                && toks.get(k + 3).is_some_and(|t| is(k, t, "("))
                && toks.get(k + 4).is_some_and(|t| is(k, t, "test"))
                && toks.get(k + 5).is_some_and(|t| is(k, t, ")")))
                || (toks.get(k + 2).is_some_and(|t| is(k, t, "test"))
                    && toks.get(k + 3).is_some_and(|t| is(k, t, "]"))));
        if test_attr {
            // Past the attribute's `]`, then to the end of the item.
            while k < toks.len() && !is(k, &toks[k], "]") {
                k += 1;
            }
            k += 1;
            let mut depth = 0;
            while k < toks.len() {
                let t = &toks[k];
                k += 1;
                if is(k, t, "{") {
                    depth += 1;
                } else if is(k, t, "}") {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                } else if depth == 0 && is(k, t, ";") {
                    break;
                }
            }
            continue;
        }
        if let Tok::Str(text, line) = &toks[k] {
            out.push(Literal {
                text: text.clone(),
                line: *line,
            });
        }
        k += 1;
    }
    out
}

/// A raw string starting at `i` (`r"`, `r#"`, `r##"` ...): the number of `#` and where its body
/// starts.
fn raw_start(c: &[char], i: usize) -> Option<(usize, usize)> {
    let mut j = i + 1;
    while c.get(j) == Some(&'#') {
        j += 1;
    }
    (c.get(j) == Some(&'"')).then_some((j - i - 1, j + 1))
}

/// The old name in any of the ways it is written.
const OLD_NAMES: [&str; 3] = ["botella libre", "botella-libre", "botellalibre"];

/// The folders whose strings the person reads.
const SCANNED: [&str; 4] = ["src/ui", "src/app", "src/domain", "src/diagnostics"];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Whether a file is only compiled for tests (`tests.rs`, `x_tests.rs`).
fn is_test_file(path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s == "tests" || s.ends_with("_tests"))
}

/// The literals that name the old app, as `file:line: text`, and how many files were read.
fn offenders(root: &Path) -> (Vec<String>, usize) {
    let mut files = vec![];
    for dir in SCANNED {
        rust_files(&root.join(dir), &mut files);
    }
    let mut bad = vec![];
    let mut scanned = 0;
    for file in files.iter().filter(|f| !is_test_file(f)) {
        scanned += 1;
        let rel = file.strip_prefix(root.join("src")).unwrap();
        let rel = rel.to_string_lossy().replace('\\', "/");
        let source = std::fs::read_to_string(file).unwrap();
        for lit in literals(&source) {
            let lower = lit.text.to_lowercase();
            if OLD_NAMES.iter().any(|n| lower.contains(n)) {
                bad.push(format!("{rel}:{}: {}", lit.line, lit.text));
            }
        }
    }
    (bad, scanned)
}

/// The old visible name cannot come back: no string literal of the interface, the actions, the
/// domain or the diagnostics names it.
#[test]
fn the_old_name_is_in_no_visible_string() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (bad, scanned) = offenders(root);
    assert!(scanned > 30, "the scan found only {scanned} files");
    assert!(bad.is_empty(), "the old name is back:\n{}", bad.join("\n"));
}

/// The scan itself: it reads strings and ignores comments, tests, lifetimes and quote characters,
/// and it does find the old name where it is written.
#[test]
fn the_scan_reads_literals_and_skips_comments_and_tests() {
    let source = r##"
// "Botella Libre" in a comment
/// "Botella Libre" in a doc comment
/* "Botella Libre" in a block /* nested */ comment */
fn a<'x>(q: &'x str) -> &'x str {
    let quote = '"';
    let _ = ("Hola", r#"con "comillas" y Botella Libre"#, '\'');
    let _ = "línea 1
línea 2 Botella Libre";
    "Altrune \" Botella Libre"
}
#[cfg(test)]
mod tests {
    fn b() -> &'static str { "Botella Libre en una prueba { llaves" }
}
#[test]
fn c() { let _ = "Botella Libre en una prueba"; }
fn after() -> &'static str { "después" }
"##;
    let found = literals(source);
    let texts: Vec<_> = found.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "Hola",
            "con \"comillas\" y Botella Libre",
            "línea 1\nlínea 2 Botella Libre",
            "Altrune \\\" Botella Libre",
            "después"
        ]
    );
    // The line a literal starts on.
    assert_eq!(found[2].line, 8);
    assert_eq!(found[3].line, 10);
}

/// A name split by a line continuation is still the name: rustc joins the lines, so the scan
/// does too (a dialog text written over two lines could otherwise hide the old name).
#[test]
fn the_scan_joins_line_continuations() {
    let source = "fn t() -> &'static str {\n    \"Abre con Botella \\\n        Libre ya\"\n}\nfn u() -> &'static str { \"tras\" }\n";
    let found = literals(source);
    let texts: Vec<_> = found.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["Abre con Botella Libre ya", "tras"]);
    assert!(texts[0].contains("Botella Libre"));
    // The line count survives the joined line break.
    assert_eq!(found[1].line, 5);
}

/// The scan really catches a file that brings the old name back.
#[test]
fn the_scan_catches_a_file_with_the_old_name() {
    let root = std::env::temp_dir().join(format!("altrune-scan-{}", std::process::id()));
    let ui = root.join("src/ui");
    for dir in SCANNED {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(
        ui.join("hola.rs"),
        "fn t() -> &'static str {\n    \"Abre con Botella Libre\"\n}\n",
    )
    .unwrap();
    // A test file is not scanned.
    std::fs::write(
        ui.join("tests.rs"),
        "fn t() { let _ = \"Botella Libre\"; }\n",
    )
    .unwrap();
    let (bad, scanned) = offenders(&root);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(scanned, 1);
    assert_eq!(bad, ["ui/hola.rs:2: Abre con Botella Libre"]);
}

// ---------------------------------------------------------------- the names the app shows

#[test]
fn the_window_and_the_report_use_the_present_name() {
    assert_eq!(crate::APP_NAME, "Altrune");
    assert_eq!(crate::app::REPORT_FILE_NAME, "informe-altrune.txt");
    assert_eq!(
        crate::app::app_line(),
        format!("Altrune {}", env!("CARGO_PKG_VERSION"))
    );
}

/// What stays on purpose: the folders and the launchers' marks keep their names, because that
/// is how everything already made is found.
#[test]
fn the_names_that_find_what_exists_are_unchanged() {
    use crate::backend::data_dir;
    assert!(data_dir().ends_with("Library/Application Support/Botella Libre"));
    assert!(crate::backend::launchers::opener_app()
        .ends_with("Applications/Botella Libre/Abrir con Botella Libre.app"));
}

// ---------------------------------------------------------------- the screens

/// The names on screen with the one place where the old name may appear taken out: the data
/// folder (in the log line and in «Acerca de»).
fn old_name_left(f: &Flow) -> Vec<String> {
    f.names()
        .into_iter()
        .map(|n| n.replace("Application Support/Botella Libre", ""))
        .filter(|n| n.to_lowercase().contains("botella libre"))
        .collect()
}

flow_test!(the_sidebar_says_altrune_and_draws_its_symbol_once, |home| {
    let mut f = Flow::start(home);
    f.frame();
    assert!(f.shows("Altrune"), "{}", f.dump());
    assert!(old_name_left(&f).is_empty(), "{:?}", old_name_left(&f));

    // The symbol was cut to its pixels and uploaded once; more frames do not remake it.
    let first = brand::textures_made(&f.app.ctx);
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(first[0].0, Art::Symbol);
    for _ in 0..10 {
        f.frame();
    }
    assert_eq!(brand::textures_made(&f.app.ctx), first);
    // The logo is not drawn in the sidebar nor on the home page.
    assert!(!f.shows(ABOUT_LOGO_NAME));
    f.finish();
});

flow_test!(ajustes_closes_with_acerca_de_altrune, |home| {
    let mut f = Flow::start(home);
    f.act(Action::OpenSettings);
    f.frame();
    f.frame();

    assert!(f.shows("Altrune, Wine y sus motores."), "{}", f.tree());
    assert!(f.shows_part("«Abrir con › Altrune»"), "{}", f.tree());
    assert!(old_name_left(&f).is_empty(), "{:?}", old_name_left(&f));

    // The section is a heading of its own, level 2 like the other groups.
    let headings = f.headings();
    assert!(
        headings.contains(&(ABOUT_TITLE.to_string(), 2)),
        "{headings:?}"
    );
    assert!(f.shows(ABOUT_LOGO_NAME), "{}", f.tree());
    assert!(
        f.shows(&format!("Versión {}", env!("CARGO_PKG_VERSION"))),
        "{}",
        f.tree()
    );
    assert!(f.shows(ABOUT_WHAT));
    assert!(f.shows(ABOUT_LICENCES));
    assert_eq!(
        ABOUT_LICENCES,
        "Altrune es software libre con licencia Apache 2.0. Wine y GStreamer son software libre (LGPL). Altrune no incluye ni descarga software de Apple como D3DMetal."
    );
    for part in [
        "Apache 2.0",
        "Wine",
        "GStreamer",
        "LGPL",
        "no incluye ni descarga",
        "D3DMetal",
    ] {
        assert!(ABOUT_LICENCES.contains(part), "{part}");
    }
    assert!(
        !ABOUT_LICENCES.contains("licencia de Apple"),
        "{ABOUT_LICENCES}"
    );
    // The folder that keeps everything, written with «~».
    assert!(
        f.shows("~/Library/Application Support/Botella Libre"),
        "{}",
        f.tree()
    );

    // Out of view at the foot of a long page, the logo has cost nothing yet (only the sidebar's
    // symbol was cut).
    assert_eq!(brand::textures_made(&f.app.ctx).len(), 1);
    // Tall enough to show it, it is drawn as two layers, each cut once to the same size.
    f.resize(900.0, 2400.0);
    f.frame();
    f.frame();
    let made = brand::textures_made(&f.app.ctx);
    let of = |art| made.iter().find(|m| m.0 == art).map(|m| m.1);
    assert!(
        of(Art::LogoSymbol).is_some() && of(Art::LogoWord).is_some(),
        "{made:?}"
    );
    assert_eq!(of(Art::LogoSymbol), of(Art::LogoWord));
    for _ in 0..10 {
        f.frame();
    }
    assert_eq!(brand::textures_made(&f.app.ctx), made);
    f.finish();
});

// At the smallest window the About logo stays inside the page, and Tab never stops on it (it is
// a picture, not a control).
flow_test!(the_about_logo_fits_the_smallest_window, |home| {
    let mut f = Flow::start(home);
    f.resize(820.0, 540.0);
    f.act(Action::OpenSettings);
    f.frame();
    f.frame();
    let (logo, window) = f
        .bounds_in_window(ABOUT_LOGO_NAME)
        .expect("the logo's node");
    assert!(
        logo.left() >= window.left() && logo.right() <= window.right(),
        "{logo:?} in {window:?}"
    );
    assert!(logo.width() > 100.0 && logo.width() < 300.0, "{logo:?}");
    for _ in 0..40 {
        f.key(eframe::egui::Key::Tab);
        assert_ne!(f.focused().as_deref(), Some(ABOUT_LOGO_NAME));
    }
    f.finish();
});
