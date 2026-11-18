//! Checks on the files that make Altrune.app (macos/Info.plist, tools/empaquetar.sh, .gitignore).
//! They only read the repository and write to the system's temporary folder: the bundle itself is
//! built by `sh tools/empaquetar.sh`, which this never runs (it compiles and signs).

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The value written after `<key>name</key>` in the template, as text (`true` for `<true/>`).
fn plist_value(plist: &str, key: &str) -> Option<String> {
    let rest = plist.split(&format!("<key>{key}</key>")).nth(1)?;
    let rest = rest.trim_start();
    if rest.starts_with("<true/>") {
        return Some("true".into());
    }
    let inner = rest.strip_prefix("<string>")?;
    Some(inner.split("</string>").next()?.to_string())
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("altrune-paquete-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn template_carries_the_agreed_keys() {
    let plist = read("macos/Info.plist");
    let expect = [
        ("CFBundleName", "Altrune"),
        ("CFBundleDisplayName", "Altrune"),
        ("CFBundleExecutable", "Altrune"),
        ("CFBundleIconFile", "Altrune"),
        ("CFBundleIdentifier", "com.altrune.app"),
        ("CFBundlePackageType", "APPL"),
        ("CFBundleInfoDictionaryVersion", "6.0"),
        ("CFBundleDevelopmentRegion", "es"),
        ("CFBundleShortVersionString", "@VERSION@"),
        ("CFBundleVersion", "@VERSION@"),
        ("LSMinimumSystemVersion", "@MINOS@"),
        ("NSHighResolutionCapable", "true"),
        ("LSApplicationCategoryType", "public.app-category.utilities"),
        ("NSHumanReadableCopyright", "Altrune"),
    ];
    for (key, value) in expect {
        assert_eq!(plist_value(&plist, key).as_deref(), Some(value), "{key}");
    }
}

#[test]
fn bundle_identifier_does_not_reuse_the_existing_ones() {
    // The opener and the launchers already exist on disk with com.botellalibre.*; the app's own
    // bundle is new and must never collide with them.
    let plist = read("macos/Info.plist");
    let id = plist_value(&plist, "CFBundleIdentifier").unwrap();
    assert!(!id.starts_with("com.botellalibre"), "{id}");
    let launchers = read("src/backend/launchers.rs");
    assert!(launchers.contains("com.botellalibre.opener"));
    assert!(launchers.contains("com.botellalibre.app."));
}

#[test]
fn icon_named_in_the_template_exists_in_the_brand_assets() {
    let plist = read("macos/Info.plist");
    let icon = plist_value(&plist, "CFBundleIconFile").unwrap();
    let file = root().join("assets/branding").join(format!("{icon}.icns"));
    let bytes = std::fs::read(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    assert_eq!(&bytes[..4], b"icns", "not an icns file");
}

#[test]
fn filled_template_is_a_valid_plist() {
    // Same substitution the script makes; plutil is part of macOS.
    let filled = read("macos/Info.plist")
        .replace("@VERSION@", "1.2.3")
        .replace("@MINOS@", "11.0");
    assert!(!filled.contains("@VERSION@") && !filled.contains("@MINOS@"));
    assert!(
        !filled.contains('@'),
        "every marker is filled, comment included"
    );
    let dir = scratch("plist");
    let file = dir.join("Info.plist");
    std::fs::write(&file, filled).unwrap();
    let out = Command::new("/usr/bin/plutil")
        .arg("-lint")
        .arg(&file)
        .output()
        .expect("plutil runs on macOS");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn cargo_version_is_readable_the_way_the_script_reads_it() {
    // The script takes the first `version = "..."` line of Cargo.toml: x.y.z or x.y.z-label
    // (0.1.0-alpha). The plist gets the numeric core, which is all digits and dots.
    let cargo = read("Cargo.toml");
    let line = cargo
        .lines()
        .find(|l| l.trim_start().starts_with("version"))
        .expect("a version line");
    let v = line.split('"').nth(1).expect("a quoted version");
    let (core, label) = match v.split_once('-') {
        Some((core, label)) => (core, Some(label)),
        None => (v, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    assert_eq!(parts.len(), 3, "{v}");
    assert!(parts.iter().all(|p| p.parse::<u32>().is_ok()), "{v}");
    if let Some(label) = label {
        assert!(
            !label.is_empty()
                && label
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'),
            "the label of {v} may only hold letters, digits, dots and hyphens"
        );
    }
    assert!(
        cargo.contains("name = \"altrune\""),
        "the package name names the release binary the script packages"
    );
}

#[test]
fn script_keeps_build_paths_out_of_the_binary_and_ships_the_licences() {
    let script = read("tools/empaquetar.sh");
    let code: String = script
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(code.contains("cargo build --release --locked"));
    assert!(code.contains("--remap-path-prefix="));
    assert!(code.contains("CARGO_TARGET_DIR="));
    // Flags set by the caller would be discarded by CARGO_ENCODED_RUSTFLAGS: refuse them.
    assert!(code.contains("CARGO_BUILD_RUSTFLAGS") && code.contains("would discard yours"));
    assert!(code.contains("strings -a"));
    assert!(code.contains("/Users/"));
    // The licences are copied into the bundle before the signature seals it.
    let sign = code.find("codesign --force --sign -").unwrap();
    for file in [
        "LICENSE",
        "NOTICE",
        "THIRD_PARTY_NOTICES.md",
        "licencias/terceros.html",
    ] {
        let at = code
            .find(file)
            .unwrap_or_else(|| panic!("{file} is not copied"));
        assert!(at < sign, "{file} must be in place before signing");
    }
    assert!(code.contains("Contents/Resources/Licencias"));
}

#[test]
fn script_signs_last_installs_nothing_and_only_replaces_its_bundle() {
    let script = read("tools/empaquetar.sh");
    let sign = script
        .find("codesign --force --sign -")
        .expect("signs the bundle");
    let last_copy = script.rfind("cp \"$BIN\"").unwrap();
    let plist_fill = script.find("macos/Info.plist >").expect("fills the plist");
    assert!(
        sign > last_copy && sign > plist_fill,
        "sign after every file is in place"
    );
    // The only removal is the bundle itself (and the throwaway check folder).
    for line in script.lines().filter(|l| l.contains("rm -rf")) {
        assert!(
            line.contains("\"$APP\"") || line.contains("\"$T\""),
            "unexpected removal: {line}"
        );
    }
    // It never installs, never opens the app and never uses the real HOME.
    let code: String = script
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    for banned in [
        "/Applications/",
        "~/Applications",
        "open -",
        "pkill",
        "killall",
        "--open",
    ] {
        assert!(!code.contains(banned), "script must not contain {banned}");
    }
    assert!(
        code.contains("HOME=\"$T\""),
        "the one run uses the isolated HOME"
    );
}

#[test]
fn dist_is_ignored_by_git() {
    let ignore = read(".gitignore");
    assert!(ignore.lines().any(|l| l.trim() == "/dist/"));
}

#[test]
fn docs_say_ad_hoc_is_not_for_other_macs() {
    let readme = read("README.md");
    assert!(readme.contains("Developer ID") && readme.contains("notariz"));
    let dist = read("docs/distribucion.md");
    assert!(dist.contains("Developer ID") && dist.contains("notari"));
}

/// The docs say what was checked with the real app and what is still pending, and never
/// claim more than that.
#[test]
fn docs_say_what_the_real_app_showed_and_what_is_pending() {
    let readme = read("README.md");
    let roadmap = read("docs/roadmap.md");
    assert!(readme.contains("Dock") && readme.contains("Con la app real"));
    assert!(readme.contains("falta ver Launchpad"));
    assert!(roadmap.contains("Comprobado con la app real"));
    assert!(roadmap.contains("Pendiente: ver el Dock y Finder con el estilo de iconos normal"));
    assert!(roadmap.contains("Developer ID"));
}

/// Three tiny programs, compiled for the test with the system C compiler: one that ends at once,
/// one that prints its arguments and one that sleeps for the number of seconds it is given. They
/// stand in for the executable of a bundle. Copies of system tools cannot do that: a copy of
/// `/bin/sleep` re-signed ad hoc does not run on macOS 15 (platform binaries lose their platform
/// status when copied), and the installer's check for a running Altrune needs an executable that
/// really runs.
struct Tools {
    /// Ends at once with success.
    ok: PathBuf,
    /// Prints its arguments.
    say: PathBuf,
    /// Sleeps for as many seconds as its first argument says.
    nap: PathBuf,
}

/// What the three programs of [`Tools`] are written in.
const TOOL_SOURCES: [(&str, &str); 3] = [
    ("ok", "int main(void) { return 0; }\n"),
    (
        "say",
        "#include <stdio.h>\nint main(int c, char **v) {\n  for (int i = 1; i < c; i++) printf(\"%s%s\", v[i], i + 1 < c ? \" \" : \"\");\n  putchar('\\n');\n  return 0;\n}\n",
    ),
    (
        "nap",
        "#include <stdlib.h>\n#include <unistd.h>\nint main(int c, char **v) { sleep(c > 1 ? (unsigned)atoi(v[1]) : 0); return 0; }\n",
    ),
];

/// Compiles the three programs into `dir`. A machine without the compiler (the Xcode
/// command-line tools) fails the test and says what to install: nothing is skipped quietly.
fn tools(dir: &Path) -> Tools {
    const NEEDED: &str = "tests/paquete.rs compiles its helper programs with the system C \
        compiler (cc): the Xcode command-line tools are needed (run `xcode-select --install`)";
    let folder = dir.join("herramientas");
    std::fs::create_dir_all(&folder).unwrap();
    let mut built = vec![];
    for (name, source) in TOOL_SOURCES {
        let c = folder.join(format!("{name}.c"));
        let exe = folder.join(name);
        std::fs::write(&c, source).unwrap();
        let out = Command::new("cc")
            .arg("-o")
            .arg(&exe)
            .arg(&c)
            .output()
            .unwrap_or_else(|e| panic!("{NEEDED}: cc could not start ({e})"));
        assert!(
            out.status.success() && exe.is_file(),
            "{NEEDED}: cc failed for {name}.c\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        built.push(exe);
    }
    let mut built = built.into_iter();
    Tools {
        ok: built.next().unwrap(),
        say: built.next().unwrap(),
        nap: built.next().unwrap(),
    }
}

/// A tiny signed bundle that looks like Altrune to the installer (or like another app when
/// `id` says so), with `exe` (one of the [`Tools`]) as its executable, ad hoc signed like the
/// real one.
fn fake_bundle_with(dir: &Path, id: &str, exe: &Path) -> PathBuf {
    let app = dir.join("Altrune.app");
    let _ = std::fs::remove_dir_all(&app);
    std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    std::fs::copy(exe, app.join("Contents/MacOS/Altrune")).unwrap();
    std::fs::write(
        app.join("Contents/Info.plist"),
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>CFBundleIdentifier</key><string>{id}</string><key>CFBundleExecutable</key><string>Altrune</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>\n"
        ),
    )
    .unwrap();
    let signed = Command::new("/usr/bin/codesign")
        .args(["--force", "--sign", "-"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(signed.status.success(), "{signed:?}");
    app
}

/// Runs tools/instalar.sh with `source` into `dest`, never registering with Launch Services.
fn install(source: &Path, dest: &Path) -> std::process::Output {
    Command::new("/bin/sh")
        .arg(root().join("tools/instalar.sh"))
        .arg(dest)
        .env("ALTRUNE_APP", source)
        .env("ALTRUNE_NO_REGISTER", "1")
        .output()
        .unwrap()
}

fn installed_exe(dest: &Path) -> Vec<u8> {
    std::fs::read(dest.join("Altrune.app/Contents/MacOS/Altrune")).unwrap()
}

fn no_staging_left(dest: &Path) {
    assert!(
        !dest.join(".Altrune.app.nuevo").exists(),
        "staging copy left"
    );
    assert!(
        !dest.join(".Altrune.app.anterior").exists(),
        "old copy left"
    );
}

/// The installer puts Altrune in place, and a second run with a different build really replaces
/// it, leaving no staging copy behind.
#[test]
fn the_installer_puts_altrune_in_place_and_replaces_it() {
    let dir = scratch("instalar-bien");
    let dest = dir.join("Aplicaciones");
    std::fs::create_dir_all(&dest).unwrap();
    let t = tools(&dir);
    let first = fake_bundle_with(&dir.join("primera"), "com.altrune.app", &t.ok);
    let out = install(&first, &dest);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        installed_exe(&dest),
        std::fs::read(first.join("Contents/MacOS/Altrune")).unwrap()
    );
    no_staging_left(&dest);
    let second = fake_bundle_with(&dir.join("segunda"), "com.altrune.app", &t.say);
    let out = install(&second, &dest);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        installed_exe(&dest),
        std::fs::read(second.join("Contents/MacOS/Altrune")).unwrap(),
        "the second build replaced the first"
    );
    no_staging_left(&dest);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Another app called Altrune.app is never replaced, an unsigned source is refused, and a
/// leftover under the staging name that belongs to another app stops it: nothing changes.
#[test]
fn the_installer_never_replaces_another_app_or_installs_an_unsigned_copy() {
    let dir = scratch("instalar-no");
    let dest = dir.join("Aplicaciones");
    std::fs::create_dir_all(&dest).unwrap();
    let t = tools(&dir);
    let other = fake_bundle_with(&dest, "com.example.other", &t.ok);
    let before = std::fs::read(other.join("Contents/Info.plist")).unwrap();
    let source = fake_bundle_with(&dir.join("fuente"), "com.altrune.app", &t.ok);
    let out = install(&source, &dest);
    assert!(!out.status.success(), "it must refuse");
    assert_eq!(
        std::fs::read(other.join("Contents/Info.plist")).unwrap(),
        before
    );
    // A source whose signature no longer verifies is refused before anything is copied.
    let empty = dir.join("vacio");
    std::fs::create_dir_all(&empty).unwrap();
    let broken = fake_bundle_with(&dir.join("rota"), "com.altrune.app", &t.ok);
    std::fs::write(broken.join("Contents/MacOS/Altrune"), b"changed").unwrap();
    let out = install(&broken, &empty);
    assert!(!out.status.success(), "an unsigned copy must be refused");
    assert!(String::from_utf8_lossy(&out.stderr).contains("does not verify"));
    assert!(std::fs::read_dir(&empty).unwrap().next().is_none());
    // A leftover under the staging name that is another app stops it too.
    let staged = dir.join("con-resto");
    std::fs::create_dir_all(&staged).unwrap();
    let foreign = fake_bundle_with(&staged.join("tmp"), "com.example.other", &t.ok);
    std::fs::rename(&foreign, staged.join(".Altrune.app.nuevo")).unwrap();
    let out = install(&source, &staged);
    assert!(!out.status.success(), "a foreign leftover must stop it");
    assert!(staged
        .join(".Altrune.app.nuevo/Contents/Info.plist")
        .is_file());
    assert!(!staged.join("Altrune.app").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// What an interrupted run left is cleaned or put back: a half-made copy (no Info.plist yet) is
/// removed, and a working Altrune that was moved aside comes back before the new copy goes in.
#[test]
fn the_installer_recovers_from_an_interrupted_run() {
    let dir = scratch("instalar-resto");
    let t = tools(&dir);
    let source = fake_bundle_with(&dir.join("fuente"), "com.altrune.app", &t.say);
    // A half-made copy blocks nothing.
    let half = dir.join("a-medias");
    std::fs::create_dir_all(half.join(".Altrune.app.nuevo/Contents/MacOS")).unwrap();
    let out = install(&source, &half);
    assert!(out.status.success(), "{out:?}");
    no_staging_left(&half);
    // The old copy moved aside, and no Altrune.app: the run ends with the new one in place.
    let aside = dir.join("apartada");
    std::fs::create_dir_all(&aside).unwrap();
    let old = fake_bundle_with(&aside.join("tmp"), "com.altrune.app", &t.ok);
    std::fs::rename(&old, aside.join(".Altrune.app.anterior")).unwrap();
    let out = install(&source, &aside);
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("restored"));
    assert_eq!(
        installed_exe(&aside),
        std::fs::read(source.join("Contents/MacOS/Altrune")).unwrap()
    );
    no_staging_left(&aside);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The process ids `lsof` finds with `exe` open: the check the installer itself makes.
fn open_by(exe: &Path) -> Vec<u32> {
    let out = Command::new("/usr/sbin/lsof")
        .arg("-t")
        .arg("--")
        .arg(exe)
        .output()
        .expect("lsof runs on macOS");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

/// A process the test started, ended by its PID when the guard goes, also when an assertion
/// failed before the test came to its end.
struct Started(std::process::Child);

impl Drop for Started {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// While Altrune runs from the destination it is not replaced.
#[test]
fn the_installer_does_not_replace_a_running_altrune() {
    let dir = scratch("instalar-abierta");
    let dest = dir.join("Aplicaciones");
    std::fs::create_dir_all(&dest).unwrap();
    let t = tools(&dir);
    let sleeper = fake_bundle_with(&dir.join("dormida"), "com.altrune.app", &t.nap);
    assert!(install(&sleeper, &dest).status.success());
    let before = installed_exe(&dest);
    let exe = dest.join("Altrune.app/Contents/MacOS/Altrune");
    let mut running = Started(Command::new(&exe).arg("30").spawn().unwrap());
    // The copy really runs (it has not ended) and `lsof` sees it, as the installer will.
    let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !open_by(&exe).contains(&running.0.id()) {
        assert!(
            std::time::Instant::now() < end,
            "lsof never saw the running copy of {}",
            exe.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        running.0.try_wait().unwrap().is_none(),
        "the copy ended by itself, so it was not running"
    );
    let newer = fake_bundle_with(&dir.join("nueva"), "com.altrune.app", &t.say);
    let out = install(&newer, &dest);
    assert!(!out.status.success(), "it must refuse while Altrune runs");
    assert!(String::from_utf8_lossy(&out.stderr).contains("open from"));
    assert_eq!(
        installed_exe(&dest),
        before,
        "the running app was not touched"
    );
    no_staging_left(&dest);
    // Ended by its PID, and only then is the folder removed.
    drop(running);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The installer never opens the app or touches the person's data.
#[test]
fn the_installer_opens_nothing_and_reads_no_data() {
    let script = read("tools/instalar.sh");
    let code: String = script
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    for banned in [
        "/usr/bin/open",
        "open -",
        "open \"",
        "--open",
        "pkill",
        "killall",
        "Application Support",
        "Botella Libre",
    ] {
        assert!(
            !code.contains(banned),
            "installer must not contain {banned}"
        );
    }
    // Every removal is one of its own staging paths.
    for line in code.lines().filter(|l| l.contains("rm -rf")) {
        assert!(
            ["\"$NEW\"", "\"$OLD\"", "\"$leftover\""]
                .iter()
                .any(|p| line.contains(p)),
            "unexpected removal: {line}"
        );
    }
}
