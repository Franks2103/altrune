//! The layers of the code import one way, and the environment every Wine command gets does not
//! change with where the code that builds it lives.
//!
//! The first part pins the environment of a Direct3D 11/12 engine with every layer added (the
//! variables, their values, their order) and the text of the Terminal's command, on a stand-in
//! engine inside a throwaway HOME. The second part is the static scan that keeps the layers
//! pointing one way.

use crate::backend::runtime_env;
use crate::backend::{adapt_env_to_wine, open_command, terminal_command, Bottle, Tool};
use std::fs;
use std::path::{Path, PathBuf};

// ------------------------------------------------------ the environment of an engine, pinned

fn touch(file: &Path) {
    fs::create_dir_all(file.parent().expect("a folder")).expect("the folder");
    fs::write(file, "x").expect("the file");
}

/// What the app would find after installing everything: the standard Wine (the Mac libraries the
/// Metal engine borrows), the controller libraries, and a Metal engine with video and
/// Direct3D 12 added. Returns the Metal engine's `wine`.
fn complete_engine(home: &Path) -> PathBuf {
    let managed = home.join("Library/Application Support/Botella Libre/Wine");
    fs::create_dir_all(managed.join("Wine Stable.app/Contents/Resources/wine/lib")).unwrap();
    touch(&managed.join("Wine Stable.app/Contents/Resources/wine/bin/wine"));
    touch(&managed.join("Mandos/libSDL2-2.0.0.dylib"));
    touch(&managed.join("Mandos/libSDL3.dylib"));
    let engine = managed.join("Motor DXMT");
    touch(&engine.join("engine/bin/wine"));
    touch(&engine.join("engine/lib/wine/x86_64-unix/wine"));
    touch(&engine.join("engine/lib/wine/x86_64-unix/wine-motor"));
    touch(&engine.join("dxmt/x86_64-windows/d3d11.dll"));
    touch(&engine.join("dxmt/x86_64-unix/winemetal.so"));
    let gst = engine.join("gstreamer/lib");
    touch(&gst.join("libgstreamer-1.0.0.dylib"));
    for plugin in ["coreelements", "isomp4", "deinterlace", "libav"] {
        touch(&gst.join(format!("gstreamer-1.0/libgst{plugin}.dylib")));
    }
    touch(&engine.join("d3dmetal/wine/x86_64-windows/d3d12.dll"));
    touch(&engine.join("d3dmetal/external/libd3dshared.dylib"));
    engine.join("engine/bin/wine")
}

fn bottle(home: &Path) -> Bottle {
    Bottle {
        name: "Mi botella".into(),
        path: home.join("Library/Application Support/Botella Libre/Bottles/Mi botella"),
        windows: "win10".into(),
        msync: true,
        high_res: false,
        logpixels: 96,
        programs: vec![],
        env: vec![("MI_VARIABLE".into(), "su valor".into())],
        prefix_complete: true,
    }
}

/// `NAME=value` for each variable, in the order they are in.
fn lines(env: &[(String, String)]) -> Vec<String> {
    env.iter().map(|(k, v)| format!("{k}={v}")).collect()
}

/// A command's own variables (the ones it sets and the ones it removes), in the order the
/// standard library keeps them, with the program and the folder it starts in.
fn command_lines(c: &std::process::Command) -> Vec<String> {
    let mut out = vec![
        format!("program={}", c.get_program().to_string_lossy()),
        format!(
            "dir={}",
            c.get_current_dir()
                .map(|d| d.to_string_lossy().to_string())
                .unwrap_or_default()
        ),
    ];
    for (k, v) in c.get_envs() {
        match v {
            Some(v) => out.push(format!("{}={}", k.to_string_lossy(), v.to_string_lossy())),
            None => out.push(format!("-{}", k.to_string_lossy())),
        }
    }
    out
}

flow_test!(the_engine_environment_is_pinned, |home| {
    std::env::remove_var("WINEDEBUG");
    let wine = complete_engine(home);
    let b = bottle(home);
    let managed = home.join("Library/Application Support/Botella Libre/Wine");
    let m = managed.display().to_string();

    // What the engine adds, in this order.
    let libraries = format!(
        "{m}/Mandos:{m}/Motor DXMT/gstreamer/lib:{m}/Wine Stable.app/Contents/Resources/wine/lib:/usr/lib"
    );
    let layer = format!("{m}/Motor DXMT/dxmt");
    let plugins = format!("{m}/Motor DXMT/gstreamer/lib/gstreamer-1.0");
    let expected_runtime = vec![
        format!("WINEDLLPATH_PREPEND={layer}"),
        "DXMT_ALLOW_CROSS_PROCESS_SWAPCHAIN=1".to_string(),
        format!("DYLD_FALLBACK_LIBRARY_PATH={libraries}"),
        format!("BOTELLA_BIBLIOTECAS={libraries}"),
        format!("GST_PLUGIN_SYSTEM_PATH_1_0={plugins}"),
        format!("GST_PLUGIN_PATH_1_0={plugins}"),
        format!("GST_REGISTRY_1_0={m}/Motor DXMT/gstreamer/registro-x86_64.bin"),
        "GST_REGISTRY_FORK=no".to_string(),
        format!("BOTELLA_CAPA={layer}"),
        format!("BOTELLA_D3DMETAL={m}/Motor DXMT/d3dmetal"),
        "BOTELLA_D3DMETAL_PROGRAMAS=rdr2:tlou-ii:tlou-ii-l".to_string(),
    ];
    assert_eq!(lines(&runtime_env(&wine)), expected_runtime);

    // A program's environment: the bottle's own variables first, then the engine's (a variable
    // the bottle sets itself wins), and the sync switch last.
    let (mut env, _) = crate::backend::program_env(&b, &["WINEDLLPATH_PREPEND=/suyo".to_string()]);
    adapt_env_to_wine(&wine, &mut env);
    let mut expected_program = vec![
        format!("WINEPREFIX={}", b.path.display()),
        "WINEDEBUG=fixme-all,fixme+winediag,warn+d2d".to_string(),
        "DOTNET_EnableWriteXorExecute=0".to_string(),
        "ROSETTA_ADVERTISE_AVX=1".to_string(),
        "MI_VARIABLE=su valor".to_string(),
        "WINEDLLPATH_PREPEND=/suyo".to_string(),
    ];
    expected_program.extend(expected_runtime[1..].iter().cloned());
    // This engine reads no sync switch of its own, so the bottle's choice is dropped.
    assert_eq!(lines(&env), expected_program);

    // The commands that carry it to Wine.
    let exe = b.path.join("drive_c/programa.exe");
    let open = command_lines(&open_command(&wine, &b, &exe, "", &[]));
    let tool = command_lines(&Tool::WineConfig.command(&wine, &b));
    // The program starts in its own folder; a tool starts in the bottle.
    assert_eq!(open[1], format!("dir={}", exe.parent().unwrap().display()));
    assert_eq!(tool[1], format!("dir={}", b.path.display()));
    for command in [&open, &tool] {
        assert_eq!(command[0], format!("program={}", wine.display()));
        for line in &expected_runtime {
            assert!(
                command.contains(line),
                "{line} is missing from {command:#?}"
            );
        }
        assert!(command.contains(&"MI_VARIABLE=su valor".to_string()));
    }
    assert_eq!(open.len(), tool.len());
    // The Terminal's command: the engine's variables as `export` lines, in the same order,
    // between the bottle's and the PATH.
    let terminal = terminal_command(&wine, &b);
    let args: Vec<String> = terminal
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    let exports: String = expected_runtime
        .iter()
        .map(|l| {
            let (k, v) = l.split_once('=').unwrap();
            format!("export {k}='{v}'; ")
        })
        .collect();
    let setup = format!(
        "export WINEPREFIX='{path}'; export WINEMSYNC=1; {exports}export PATH='{bin}'\":$PATH\"; cd '{path}'; clear",
        path = b.path.display(),
        bin = wine.parent().unwrap().display(),
    );
    let script = format!(
        "tell app \"Terminal\"\nactivate\ndelay 0.1\ndo script \"{}\"\nset the custom title of the front window to \"Mi botella\"\nend tell",
        setup.replace('"', "\\\"")
    );
    assert_eq!(args, ["-e".to_string(), script]);
    assert!(crate::backend::terminal_dropped_names(&wine).is_empty());
});

// -------------------------------------------------------------- the layers import one way

/// A layer of the code and the layers its production code may name. A layer may always name
/// itself. Anything else after `crate::` is a violation: another layer, the crate's own root
/// (`crate::App`, `crate::Msg`, `crate::APP_NAME`) or everything at once (`use crate::*`).
///
/// The rule, from the lowest layer up: `domain` and `diagnostics` are leaves; `backend` may use
/// those two; `wine_setup` may use those and `backend`. The app, the window, start-up and the
/// measure tool may use anything. A new layer or a new allowed direction is one row here.
struct Layer {
    name: &'static str,
    may_use: &'static [&'static str],
}

const LAYERS: [Layer; 4] = [
    Layer {
        name: "domain",
        may_use: &[],
    },
    Layer {
        name: "diagnostics",
        may_use: &[],
    },
    Layer {
        name: "backend",
        may_use: &["domain", "diagnostics"],
    },
    Layer {
        name: "wine_setup",
        may_use: &["domain", "diagnostics", "backend"],
    },
];

/// The source with everything that is not code taken out: comments, the inside of strings and
/// characters, and the items only compiled for tests (`#[cfg(test)]` and `#[test]`). Line
/// breaks stay, so a position in the result is on the same line as in the file.
fn production_code(source: &str) -> String {
    let c: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    let blank = |ch: char| if ch == '\n' { '\n' } else { ' ' };
    while i < c.len() {
        let ch = c[i];
        let prev_is_word = i > 0 && (c[i - 1].is_alphanumeric() || c[i - 1] == '_');
        if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            let mut depth = 0;
            while i < c.len() {
                if c[i] == '/' && c.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if c[i] == '*' && c.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(c[i]));
                    i += 1;
                }
            }
        } else if ch == 'r' && !prev_is_word && raw_string(&c, i).is_some() {
            let (hashes, body) = raw_string(&c, i).unwrap();
            while i < body {
                out.push(' ');
                i += 1;
            }
            while i < c.len()
                && !(c[i] == '"' && (0..hashes).all(|k| c.get(i + 1 + k) == Some(&'#')))
            {
                out.push(blank(c[i]));
                i += 1;
            }
            for _ in 0..=hashes {
                out.push(' ');
                i += 1;
            }
        } else if ch == '"' {
            out.push(' ');
            i += 1;
            while i < c.len() && c[i] != '"' {
                if c[i] == '\\' && i + 1 < c.len() {
                    out.push(' ');
                    i += 1;
                }
                out.push(blank(c[i]));
                i += 1;
            }
            out.push(' ');
            i += 1;
        } else if ch == '\'' {
            // A character literal ('"', '\n', '\'') or a lifetime ('a).
            let end = if c.get(i + 1) == Some(&'\\') {
                (i + 2..c.len()).find(|&j| c[j] == '\'')
            } else if c.get(i + 2) == Some(&'\'') {
                Some(i + 2)
            } else {
                None
            };
            match end {
                Some(end) => {
                    while i <= end {
                        out.push(' ');
                        i += 1;
                    }
                }
                None => {
                    out.push(ch);
                    i += 1;
                }
            }
        } else {
            out.push(ch);
            i += 1;
        }
    }
    without_test_items(&out)
}

/// A raw string starting at `i` (`r"`, `r#"`, ...): the number of `#` and where its body starts.
fn raw_string(c: &[char], i: usize) -> Option<(usize, usize)> {
    let mut j = i + 1;
    while c.get(j) == Some(&'#') {
        j += 1;
    }
    (c.get(j) == Some(&'"')).then_some((j - i - 1, j + 1))
}

/// `code` with every item under `#[cfg(test)]` or `#[test]` blanked out: from the attribute to
/// the `;` or the closing brace that ends the item.
fn without_test_items(code: &str) -> String {
    let c: Vec<char> = code.chars().collect();
    let mut out: Vec<char> = c.clone();
    let mut i = 0;
    while i < c.len() {
        let rest: String = c[i..c.len().min(i + 16)].iter().collect();
        if !(rest.starts_with("#[cfg(test)]") || rest.starts_with("#[test]")) {
            i += 1;
            continue;
        }
        let start = i;
        let (mut round, mut brace) = (0i32, 0i32);
        let mut seen_brace = false;
        while i < c.len() {
            match c[i] {
                '(' | '[' => round += 1,
                ')' | ']' => round -= 1,
                '{' if round == 0 => {
                    brace += 1;
                    seen_brace = true;
                }
                '}' if round == 0 => brace -= 1,
                _ => {}
            }
            i += 1;
            // The attribute's own brackets are round-bracketed: they close before the item.
            if (c[i - 1] == ';' && round == 0 && brace == 0) || (seen_brace && brace == 0) {
                break;
            }
        }
        for slot in &mut out[start..i.min(c.len())] {
            if *slot != '\n' {
                *slot = ' ';
            }
        }
    }
    out.into_iter().collect()
}

/// The first-level names a `crate::` path reaches, with the line each is on: `crate::ui::x` is
/// `ui`, `crate::{backend, wine_setup::y}` is `backend` and `wine_setup`, `crate::*` is `*`.
fn crate_roots(code: &str) -> Vec<(usize, String)> {
    let c: Vec<char> = code.chars().collect();
    let mut found = vec![];
    let mut line = 1;
    let mut i = 0;
    let word = |ch: char| ch.is_alphanumeric() || ch == '_';
    while i < c.len() {
        if c[i] == '\n' {
            line += 1;
        }
        let here: String = c[i..c.len().min(i + 7)].iter().collect();
        let starts_word = i == 0 || !word(c[i - 1]);
        if here != "crate::" || !starts_word {
            i += 1;
            continue;
        }
        i += 7;
        match c.get(i) {
            Some('{') => {
                // A group: the first name of each member at the top level of the braces.
                let mut depth = 0;
                let mut at_start = false;
                let mut j = i;
                let mut group_line = line;
                while j < c.len() {
                    match c[j] {
                        '{' => {
                            depth += 1;
                            at_start = depth == 1;
                        }
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        ',' if depth == 1 => at_start = true,
                        '\n' => group_line += 1,
                        ch if ch.is_whitespace() => {}
                        _ if at_start => {
                            at_start = false;
                            let name: String = if c[j] == '*' {
                                "*".into()
                            } else {
                                c[j..].iter().take_while(|&&k| word(k)).collect()
                            };
                            found.push((group_line, name));
                        }
                        _ => {}
                    }
                    j += 1;
                }
            }
            Some('*') => found.push((line, "*".to_string())),
            _ => {
                let name: String = c[i..].iter().take_while(|&&k| word(k)).collect();
                found.push((line, name));
            }
        }
    }
    found
}

/// Every Rust file under `dir` that is production code: not a test file (`tests.rs`,
/// `x_tests.rs`) and not under a `tests` folder.
fn production_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    entries.sort();
    for path in entries {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if path.is_dir() {
            if stem != "tests" {
                production_files(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs")
            && stem != "tests"
            && !stem.ends_with("_tests")
        {
            out.push(path);
        }
    }
}

/// The lines of the production code under `root/src` that name a layer they may not, as
/// `file:line: crate::name`, with the number of files looked at.
fn layer_violations(root: &Path, layers: &[Layer]) -> (Vec<String>, usize) {
    let mut bad = vec![];
    let mut scanned = 0;
    for layer in layers {
        let mut files = vec![];
        production_files(&root.join("src").join(layer.name), &mut files);
        for file in files {
            scanned += 1;
            let code = production_code(&fs::read_to_string(&file).unwrap());
            let rel = file
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .to_string();
            for (line, name) in crate_roots(&code) {
                if name != layer.name && !layer.may_use.contains(&name.as_str()) {
                    bad.push(format!("{rel}:{line}: crate::{name}"));
                }
            }
        }
    }
    (bad, scanned)
}

/// The rule itself: no production code of a layer names a layer above it, the crate's root or
/// everything at once. A violation reads as `file:line: crate::what`.
#[test]
fn the_layers_import_one_way() {
    let (bad, scanned) = layer_violations(Path::new(env!("CARGO_MANIFEST_DIR")), &LAYERS);
    assert!(scanned > 30, "the scan found only {scanned} files");
    assert!(
        bad.is_empty(),
        "a layer names one above it:\n{}",
        bad.join("\n")
    );
}

/// The scan catches what it is there for, and only that: planted violations of each kind in a
/// temporary tree are found, while the allowed direction, comments, strings and the code that
/// is only compiled for tests are not.
#[test]
fn the_scan_catches_a_planted_violation() {
    let root = std::env::temp_dir().join(format!("altrune-layers-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let write = |file: &str, text: &str| {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    };
    for layer in &LAYERS {
        fs::create_dir_all(root.join("src").join(layer.name)).unwrap();
    }
    write(
        "src/domain/a.rs",
        "use crate::ui::theme::Tone;\nfn f() {}\n",
    );
    write(
        "src/domain/b.rs",
        "fn f() -> crate::wine_setup::Step { todo!() }\n",
    );
    write(
        "src/diagnostics/a.rs",
        "fn f() { crate::backend::mac_user(); }\n",
    );
    write(
        "src/backend/a.rs",
        "use crate::*;\nuse crate::domain::status::X;\n",
    );
    write(
        "src/backend/b.rs",
        "fn f() -> &'static str {\n    crate::APP_NAME\n}\nuse crate::{diagnostics, app::x, domain};\n",
    );
    write(
        "src/wine_setup/a.rs",
        "use crate::App;\nuse crate::backend::Log;\n",
    );
    write("src/wine_setup/b.rs", "pub(in crate::ui) fn f() {}\n");
    // None of this is a violation.
    write(
        "src/domain/clean.rs",
        r#"// use crate::ui::x;
/// crate::app::Y in a doc comment
/* crate::ui::z in a /* nested */ block comment */
fn f() -> &'static str { "crate::ui::w in a string" }
fn g() -> char { '"' }
#[cfg(test)]
mod tests {
    use crate::ui::q;
    fn h() { let _ = ("}", 1); }
}
#[test]
fn t() { crate::app::run(); }
use crate::domain::engines::GAMES;
"#,
    );
    write("src/domain/tests.rs", "use crate::ui::x;\n");
    write("src/domain/status_tests.rs", "use crate::ui::x;\n");
    write("src/backend/tests/support.rs", "use crate::app::x;\n");
    let (bad, scanned) = layer_violations(&root, &LAYERS);
    let _ = fs::remove_dir_all(&root);
    assert_eq!(scanned, 8);
    assert_eq!(
        bad,
        [
            "src/domain/a.rs:1: crate::ui",
            "src/domain/b.rs:1: crate::wine_setup",
            "src/diagnostics/a.rs:1: crate::backend",
            "src/backend/a.rs:1: crate::*",
            "src/backend/b.rs:2: crate::APP_NAME",
            "src/backend/b.rs:4: crate::app",
            "src/wine_setup/a.rs:1: crate::App",
            "src/wine_setup/b.rs:1: crate::ui",
        ]
    );
}
