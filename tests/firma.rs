//! Checks on the signing and notarisation files: tools/firmar.sh, tools/notarizar.sh,
//! tools/llavero-ci.sh, macos/Altrune.entitlements, the Apple events text of macos/Info.plist and
//! docs/firma-y-notarizacion.md.
//!
//! Most of it is static (the scripts are read as text). The few runs use a copy of the script in a
//! temporary folder and, for notarisation, fake `codesign`, `xcrun`, `spctl` and `stapler` tools
//! put first in the PATH, so nothing reaches Apple, no keychain is read or changed, and nothing
//! outside the system's temporary folder is written. tools/llavero-ci.sh is only ever run outside
//! CI, where it refuses before it touches anything.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SCRIPTS: [&str; 3] = [
    "tools/firmar.sh",
    "tools/notarizar.sh",
    "tools/llavero-ci.sh",
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("altrune-firma-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The lines of a script that are code: no blank lines, no comments.
fn code_lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// A copy of a script (and of the entitlements) in a throwaway repository-like folder, so the
/// script's own `ROOT` (and its `dist/`) is that folder and not this repository.
fn script_in_scratch(dir: &Path, script: &str) -> PathBuf {
    std::fs::create_dir_all(dir.join("tools")).unwrap();
    std::fs::create_dir_all(dir.join("macos")).unwrap();
    std::fs::copy(root().join(script), dir.join(script)).unwrap();
    std::fs::copy(
        root().join("macos/Altrune.entitlements"),
        dir.join("macos/Altrune.entitlements"),
    )
    .unwrap();
    dir.join(script)
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

// ---- Static checks ------------------------------------------------------------------------

#[test]
fn scripts_never_trace_and_never_print_a_secret() {
    // Every variable that holds a secret or a credential, under the name the scripts use.
    let secrets = [
        "ALTRUNE_IDENTIDAD",
        "IDENT",
        "ALTRUNE_NOTARIO_PERFIL",
        "PERFIL",
        "ALTRUNE_NOTARIO_CLAVE",
        "CLAVE",
        "ALTRUNE_NOTARIO_CLAVE_ID",
        "CLAVE_ID",
        "ALTRUNE_NOTARIO_EMISOR",
        "EMISOR",
        "MACOS_CERTIFICADO_CLAVE",
        "MACOS_CERTIFICADO_P12_BASE64",
        "PW",
    ];
    let printers = ["echo ", "printf ", "say ", "fail ", "ok ", "usage"];
    for script in SCRIPTS {
        let src = read(script);
        for line in code_lines(&src) {
            // `set -x`, `set -eux`, `sh -x`: shell tracing prints every command with its arguments.
            let words: Vec<&str> = line.split_whitespace().collect();
            for pair in words.windows(2) {
                if (pair[0] == "set" || pair[0] == "sh") && pair[1].starts_with('-') {
                    assert!(!pair[1].contains('x'), "{script} turns tracing on: {line}");
                }
            }
            assert!(!line.contains("xtrace"), "{script}: {line}");
            if !printers.iter().any(|p| line.contains(p)) {
                continue;
            }
            for name in secrets {
                assert!(
                    !uses_variable(line, name),
                    "{script} prints the secret {name}: {line}"
                );
            }
        }
    }
}

/// True when the line expands `$NAME` or `${NAME...}` (and not a longer name that starts the same).
fn uses_variable(line: &str, name: &str) -> bool {
    for form in [format!("${name}"), format!("${{{name}")] {
        let mut rest = line;
        while let Some(at) = rest.find(&form) {
            let after = rest[at + form.len()..].chars().next();
            if !after.is_some_and(is_name_char) {
                return true;
            }
            rest = &rest[at + form.len()..];
        }
    }
    false
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[test]
fn scripts_touch_no_settings_and_no_applications_folder() {
    for script in SCRIPTS {
        let src = read(script);
        assert!(src.starts_with("#!/bin/sh\n"), "{script}: shebang");
        assert!(code_lines(&src).contains(&"set -eu"), "{script}: set -eu");
        for line in code_lines(&src) {
            assert!(
                !line.starts_with("open "),
                "{script} opens something: {line}"
            );
            for banned in [
                "/Applications",
                "~/Applications",
                "defaults write",
                "defaults delete",
                "killall",
                "pkill",
                "/usr/bin/open",
                "default-keychain",
                "Library/Keychains",
            ] {
                assert!(!line.contains(banned), "{script} has `{banned}`: {line}");
            }
        }
    }
}

#[test]
fn firmar_refuses_ad_hoc_outside_the_local_check_and_uses_the_hardened_runtime() {
    let src = read("tools/firmar.sh");
    assert!(
        src.contains(r#"[ "$IDENT" = "-" ]"#)
            && src.contains(r#"the ad hoc identity \"-\" is refused"#),
        "the identity \"-\" must be refused when --prueba is not given"
    );
    assert!(
        src.contains(r#"IDENT="-""#),
        "--prueba is the only place that sets \"-\""
    );
    assert!(
        src.contains("Developer ID Application: "),
        "only Developer ID identities"
    );
    for needle in [
        "--options runtime",
        "--timestamp",
        "--entitlements \"$ENTITLEMENTS\"",
        "--force",
        "--verify --deep --strict --verbose=2",
        "codesign -dv --verbose=4",
        "codesign -d --entitlements :-",
        "TeamIdentifier",
        "Authority=Developer ID Application",
        "--keychain",
    ] {
        assert!(src.contains(needle), "firmar.sh lacks `{needle}`");
    }
    // The timestamp is secure (Apple's server) everywhere but in the local check.
    assert!(src.contains("--timestamp=none"));
    // It signs and verifies; it never notarises or builds.
    for line in code_lines(&src) {
        for banned in ["notarytool", "stapler", "cargo ", "hdiutil", "dmgbuild"] {
            assert!(!line.contains(banned), "firmar.sh has `{banned}`: {line}");
        }
    }
}

#[test]
fn notarizar_waits_staples_validates_and_keeps_the_log_in_dist() {
    let src = read("tools/notarizar.sh");
    for needle in [
        "notarytool \"$_sub\"",
        "--wait",
        "--timeout",
        "--output-format json",
        "--keychain-profile",
        "--key-id",
        "--issuer",
        "xcrun stapler staple",
        "xcrun stapler validate",
        "ditto -c -k --keepParent",
        "$DIST/notary-log-$ID.json",
        "spctl -a -vv -t exec",
        "spctl -a -vv -t open --context context:primary-signature",
        "exit 2",
    ] {
        assert!(src.contains(needle), "notarizar.sh lacks `{needle}`");
    }
    let ignore = read(".gitignore");
    assert!(
        ignore.lines().any(|l| l == "notary-log*.json") && ignore.lines().any(|l| l == "/dist/"),
        "the notary log must be ignored by git"
    );
}

#[test]
fn llavero_ci_runs_only_on_a_ci_runner_and_cleans_up() {
    let src = read("tools/llavero-ci.sh");
    assert!(
        src.contains(r#"[ "${CI:-}" != "true" ] || [ -z "${RUNNER_TEMP:-}" ]"#),
        "the CI guard is missing"
    );
    // The guard comes before anything touches a keychain.
    let guard = src.find("refusing to run outside CI").unwrap();
    for call in [
        "security create-keychain",
        "security import",
        "security list-keychains -d user -s",
        "security delete-keychain",
        "security unlock-keychain",
    ] {
        let at = src
            .find(call)
            .unwrap_or_else(|| panic!("llavero-ci.sh lacks `{call}`"));
        assert!(guard < at, "`{call}` comes before the CI guard");
    }
    for needle in [
        "set-key-partition-list",
        "openssl rand",
        "base64 -D",
        "$RUNNER_TEMP",
        "rm -f \"$P12\"",
        "crear",
        "borrar",
    ] {
        assert!(src.contains(needle), "llavero-ci.sh lacks `{needle}`");
    }
}

#[test]
fn entitlements_hold_exactly_the_agreed_keys() {
    let file = root().join("macos/Altrune.entitlements");
    let lint = Command::new("plutil")
        .arg("-lint")
        .arg(&file)
        .output()
        .unwrap();
    assert!(lint.status.success(), "plutil -lint: {}", text(&lint));
    let json = Command::new("plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(json.status.success(), "{}", text(&json));
    // Only the Apple events key. No JIT, no unsigned memory, no DYLD variables, no disabled
    // library validation: Altrune's own process needs none of them.
    assert_eq!(
        String::from_utf8_lossy(&json.stdout).trim(),
        r#"{"com.apple.security.automation.apple-events":true}"#
    );
    let src = read("macos/Altrune.entitlements");
    assert!(
        src.contains("osascript") && src.contains("Terminal"),
        "the file must say why the key is there"
    );
}

#[test]
fn info_plist_explains_the_apple_events_request_without_at_signs() {
    let plist = read("macos/Info.plist");
    let rest = plist
        .split("<key>NSAppleEventsUsageDescription</key>")
        .nth(1)
        .expect("NSAppleEventsUsageDescription is missing");
    let value = rest
        .trim_start()
        .strip_prefix("<string>")
        .expect("it must be a string")
        .split("</string>")
        .next()
        .unwrap();
    assert!(
        value.len() > 20,
        "the text is too short to explain anything"
    );
    assert!(
        !value.contains('@'),
        "the text must not contain an at-sign: {value}"
    );
    assert!(
        value.contains("Terminal"),
        "it must say what is controlled: {value}"
    );
}

#[test]
fn the_signing_doc_says_plainly_that_nothing_is_signed_yet() {
    let doc = read("docs/firma-y-notarizacion.md");
    for needle in [
        "ad hoc",
        "Developer ID Application",
        "notarytool store-credentials",
        "MACOS_CERTIFICADO_P12_BASE64",
        "MACOS_CERTIFICADO_CLAVE",
        "ALTRUNE_IDENTIDAD",
        "NOTARIO_CLAVE_P8_BASE64",
        "NOTARIO_CLAVE_ID",
        "NOTARIO_EMISOR",
        "sh tools/firmar.sh",
        "sh tools/notarizar.sh",
        "Abrir igualmente",
        "environment",
        "notary-log",
    ] {
        assert!(
            doc.contains(needle),
            "docs/firma-y-notarizacion.md lacks `{needle}`"
        );
    }
    assert!(
        doc.contains("ninguna versión de Altrune está firmada con Developer ID ni notarizada"),
        "the doc must say that nothing has been signed or notarised yet"
    );
}

// ---- Runs ---------------------------------------------------------------------------------

#[test]
fn llavero_ci_refuses_outside_ci_and_touches_nothing() {
    let dir = scratch("llavero");
    let script = script_in_scratch(&dir, "tools/llavero-ci.sh");
    // No CI variable at all; then CI=true without RUNNER_TEMP. RUNNER_TEMP is never given together
    // with CI=true here, so the script can never get past its guard in this test.
    for vars in [vec![], vec![("CI", "true")]] {
        for action in ["crear", "borrar"] {
            let out = Command::new("/bin/sh")
                .arg(&script)
                .arg(action)
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .envs(vars.clone())
                .output()
                .unwrap();
            assert_eq!(
                out.status.code(),
                Some(2),
                "{action} {vars:?}: {}",
                text(&out)
            );
            assert!(text(&out).contains("refusing to run outside CI"));
        }
    }
    let out = Command::new("/bin/sh")
        .arg(&script)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "no argument: {}", text(&out));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn firmar_refuses_the_ad_hoc_identity_and_non_developer_id_identities() {
    let dir = scratch("firmar-rechazos");
    let script = script_in_scratch(&dir, "tools/firmar.sh");
    let run = |identity: Option<&str>, args: &[&str]| {
        let mut c = Command::new("/bin/sh");
        c.arg(&script)
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin");
        if let Some(i) = identity {
            c.env("ALTRUNE_IDENTIDAD", i);
        }
        c.output().unwrap()
    };
    let app = dir.join("Altrune.app");
    let args = ["app", app.to_str().unwrap()];

    let none = run(None, &args);
    assert_eq!(none.status.code(), Some(2), "{}", text(&none));
    let dash = run(Some("-"), &args);
    assert_eq!(dash.status.code(), Some(1), "{}", text(&dash));
    assert!(text(&dash).contains("ad hoc identity"), "{}", text(&dash));
    let other = run(Some("Apple Development: Someone (ABCDE12345)"), &args);
    assert_eq!(other.status.code(), Some(1), "{}", text(&other));
    assert!(
        text(&other).contains("Developer ID Application"),
        "{}",
        text(&other)
    );
    let usage = run(Some("-"), &["wrong", "x"]);
    assert_eq!(usage.status.code(), Some(2), "{}", text(&usage));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A tiny bundle that looks like Altrune: the real Info.plist template filled in, and an
/// executable made by the C compiler of the Xcode command-line tools.
fn mini_app(dir: &Path) -> PathBuf {
    let app = dir.join("Altrune.app");
    std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
    std::fs::create_dir_all(app.join("Contents/Resources")).unwrap();
    let plist = read("macos/Info.plist")
        .replace("@VERSION@", "0.1.0")
        .replace("@MINOS@", "11.0");
    std::fs::write(app.join("Contents/Info.plist"), plist).unwrap();
    let c = dir.join("main.c");
    std::fs::write(&c, "int main(void) { return 0; }\n").unwrap();
    let out = Command::new("cc")
        .arg("-o")
        .arg(app.join("Contents/MacOS/Altrune"))
        .arg(&c)
        .output()
        .expect("cc is needed: install the Xcode command-line tools (xcode-select --install)");
    assert!(out.status.success(), "{}", text(&out));
    app
}

#[test]
fn the_local_check_signs_a_copy_with_the_hardened_runtime_and_the_entitlements() {
    let dir = scratch("firmar-prueba");
    let script = script_in_scratch(&dir, "tools/firmar.sh");
    let app = mini_app(&dir);
    let out = Command::new("/bin/sh")
        .arg(&script)
        .args(["--prueba", "app"])
        .arg(&app)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    let said = text(&out);
    assert!(out.status.success(), "{said}");
    assert!(said.contains("hardened runtime flag present"), "{said}");
    assert!(
        said.contains("com.apple.security.automation.apple-events"),
        "{said}"
    );
    // Checked again from outside the script.
    let info = Command::new("codesign")
        .args(["-dv", "--verbose=4"])
        .arg(&app)
        .output()
        .unwrap();
    let info = text(&info);
    assert!(info.contains("runtime"), "{info}");
    assert!(info.contains("Signature=adhoc"), "{info}");
    // An ad hoc signature is not accepted by Gatekeeper: the check is for this Mac only. Whether
    // spctl rejects anything depends on the machine's setting (a runner image may have the
    // assessments off), so this one assertion runs only when they are on.
    let status = Command::new("spctl").arg("--status").output().unwrap();
    if text(&status).contains("assessments enabled") {
        let spctl = Command::new("spctl")
            .args(["-a", "-t", "exec"])
            .arg(&app)
            .output()
            .unwrap();
        assert!(
            !spctl.status.success(),
            "an ad hoc app must be rejected by Gatekeeper"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Fake `codesign`, `xcrun` and `spctl`. They record every call in `calls.log` and answer like
/// Apple's tools would for a Developer ID signed file; `FAKE_STATUS` is the notary's verdict.
fn fake_tools(dir: &Path) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let write = |name: &str, body: &str| {
        let path = bin.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        Command::new("chmod")
            .arg("755")
            .arg(&path)
            .status()
            .unwrap();
    };
    write(
        "codesign",
        r#"echo "codesign $*" >> "$FAKE_DIR/calls.log"
echo "Identifier=com.altrune.app" >&2
echo "CodeDirectory v=20500 size=1 flags=0x10000(runtime) hashes=1+1 location=embedded" >&2
echo "Authority=Developer ID Application: Test (ABCDE12345)" >&2
echo "TeamIdentifier=ABCDE12345" >&2
"#,
    );
    write(
        "spctl",
        r#"echo "spctl $*" >> "$FAKE_DIR/calls.log"
echo "accepted" >&2
echo "source=Notarized Developer ID" >&2
"#,
    );
    write(
        "xcrun",
        r#"echo "xcrun $*" >> "$FAKE_DIR/calls.log"
case "$1 $2" in
  "notarytool submit")
    case "$FAKE_STATUS" in
      NOID) echo "Conducting pre-submission checks..."; echo "Error: network down" >&2; exit 1 ;;
      PREFIX) echo "Conducting pre-submission checks..." ;;
    esac
    ST="$FAKE_STATUS"; [ "$ST" = PREFIX ] && ST=Accepted
    printf '{"id":"2efe2717-52ef-43a5-96dc-0797e3c1c3b3","message":"x","status":"%s"}\n' "$ST"
    case "$FAKE_STATUS" in "In Progress") exit 1 ;; esac ;;
  "notarytool log")
    printf '{"issues":[{"message":"not signed with a valid Developer ID"}]}\n' > "$4" ;;
  "stapler staple")
    N=$(cat "$FAKE_DIR/staple.count" 2>/dev/null || echo 0); N=$((N + 1)); echo "$N" > "$FAKE_DIR/staple.count"
    if [ "$N" -le "${FAKE_STAPLE_FAILS:-0}" ]; then echo "Record not found" >&2; exit 65; fi
    echo "The staple and validate action worked!" ;;
  "stapler validate") echo "The validate action worked!" ;;
  *) echo "unexpected: $*" >&2; exit 9 ;;
esac
"#,
    );
    bin
}

fn run_notarizar(
    dir: &Path,
    script: &Path,
    target: &Path,
    status: &str,
    extra: &[(&str, &str)],
) -> Output {
    let bin = fake_tools(dir);
    let key = dir.join("AuthKey_FAKE.p8");
    std::fs::write(&key, "not a key\n").unwrap();
    Command::new("/bin/sh")
        .arg(script)
        .arg(target)
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("FAKE_DIR", dir)
        .env("FAKE_STATUS", status)
        .env("ALTRUNE_NOTARIO_CLAVE", &key)
        .env("ALTRUNE_NOTARIO_CLAVE_ID", "SECRETKEYID99")
        .env("ALTRUNE_NOTARIO_EMISOR", "secret-issuer-uuid")
        .envs(extra.iter().copied())
        .output()
        .unwrap()
}

#[test]
fn notarizar_accepted_app_is_zipped_stapled_validated_and_assessed() {
    let dir = scratch("notarizar-ok");
    let script = script_in_scratch(&dir, "tools/notarizar.sh");
    let app = mini_app(&dir);
    let out = run_notarizar(&dir, &script, &app, "Accepted", &[]);
    let said = text(&out);
    assert!(out.status.success(), "{said}");
    let calls = std::fs::read_to_string(dir.join("calls.log")).unwrap();
    assert!(calls.contains("notarytool submit"), "{calls}");
    assert!(
        calls.contains("Altrune.zip"),
        "an app is sent zipped: {calls}"
    );
    assert!(
        calls.contains("--wait") && calls.contains("--timeout 30m"),
        "{calls}"
    );
    let staple = calls.find("stapler staple").expect("staple");
    let validate = calls.find("stapler validate").expect("validate");
    let spctl = calls.find("spctl -a -vv -t exec").expect("spctl");
    assert!(staple < validate && validate < spctl, "{calls}");
    // The credentials are only ever arguments: the script's own output never shows them.
    for secret in ["SECRETKEYID99", "secret-issuer-uuid", "AuthKey_FAKE"] {
        assert!(!said.contains(secret), "the output shows {secret}: {said}");
    }
    assert!(
        !dir.join("dist").exists(),
        "an accepted submission leaves no log"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn notarizar_dmg_is_sent_as_it_is_and_assessed_as_an_open_document() {
    let dir = scratch("notarizar-dmg");
    let script = script_in_scratch(&dir, "tools/notarizar.sh");
    let dmg = dir.join("Altrune-0.1.0-alpha.dmg");
    std::fs::write(&dmg, "not really an image\n").unwrap();
    let out = run_notarizar(&dir, &script, &dmg, "Accepted", &[]);
    assert!(out.status.success(), "{}", text(&out));
    let calls = std::fs::read_to_string(dir.join("calls.log")).unwrap();
    assert!(
        calls.contains("notarytool submit") && calls.contains("Altrune-0.1.0-alpha.dmg"),
        "{calls}"
    );
    assert!(!calls.contains(".zip"), "{calls}");
    assert!(
        calls.contains("spctl -a -vv -t open --context context:primary-signature"),
        "{calls}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn notarizar_rejected_keeps_the_log_and_never_staples() {
    let dir = scratch("notarizar-no");
    let script = script_in_scratch(&dir, "tools/notarizar.sh");
    let app = mini_app(&dir);
    let out = run_notarizar(&dir, &script, &app, "Invalid", &[]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains("dist/notary-log-2efe2717-52ef-43a5-96dc-0797e3c1c3b3.json"),
        "{}",
        text(&out)
    );
    let log = dir.join("dist/notary-log-2efe2717-52ef-43a5-96dc-0797e3c1c3b3.json");
    assert!(std::fs::read_to_string(&log).unwrap().contains("issues"));
    let calls = std::fs::read_to_string(dir.join("calls.log")).unwrap();
    assert!(
        !calls.contains("stapler"),
        "a rejected submission is never stapled: {calls}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn notarizar_still_processing_is_not_a_rejection_and_names_the_submission() {
    let dir = scratch("notarizar-pending");
    let script = script_in_scratch(&dir, "tools/notarizar.sh");
    let app = mini_app(&dir);
    let out = run_notarizar(&dir, &script, &app, "In Progress", &[]);
    let said = text(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(said.contains("no verdict yet"), "{said}");
    assert!(
        said.contains("xcrun notarytool info 2efe2717-52ef-43a5-96dc-0797e3c1c3b3"),
        "{said}"
    );
    assert!(!said.contains("not accepted"), "{said}");
    let calls = std::fs::read_to_string(dir.join("calls.log")).unwrap();
    assert!(
        !calls.contains("notarytool log") && !calls.contains("stapler"),
        "a pending submission has no log and nothing to staple: {calls}"
    );
    assert!(!dir.join("dist").exists(), "{calls}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn notarizar_without_an_id_shows_what_notarytool_said_and_points_to_the_history() {
    let dir = scratch("notarizar-noid");
    let script = script_in_scratch(&dir, "tools/notarizar.sh");
    let app = mini_app(&dir);
    let out = run_notarizar(&dir, &script, &app, "NOID", &[]);
    let said = text(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(said.contains("Conducting pre-submission checks"), "{said}");
    assert!(said.contains("xcrun notarytool history"), "{said}");
    for secret in ["SECRETKEYID99", "secret-issuer-uuid", "AuthKey_FAKE"] {
        assert!(!said.contains(secret), "the output shows {secret}: {said}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn notarizar_reads_the_json_even_when_text_comes_before_it() {
    let dir = scratch("notarizar-prefix");
    let script = script_in_scratch(&dir, "tools/notarizar.sh");
    let app = mini_app(&dir);
    let out = run_notarizar(&dir, &script, &app, "PREFIX", &[]);
    assert!(out.status.success(), "{}", text(&out));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn notarizar_retries_the_staple_and_then_gives_up_saying_the_submission_was_accepted() {
    let fast = [("ALTRUNE_NOTARIO_GRAPAR_ESPERA", "0")];
    // Two transient failures, then it works.
    let dir = scratch("notarizar-retry");
    let script = script_in_scratch(&dir, "tools/notarizar.sh");
    let app = mini_app(&dir);
    let mut extra = fast.to_vec();
    extra.push(("FAKE_STAPLE_FAILS", "2"));
    let out = run_notarizar(&dir, &script, &app, "Accepted", &extra);
    assert!(out.status.success(), "{}", text(&out));
    let count = std::fs::read_to_string(dir.join("staple.count")).unwrap();
    assert_eq!(count.trim(), "3");
    let _ = std::fs::remove_dir_all(&dir);

    // It never works: five attempts, then a clear failure.
    let dir = scratch("notarizar-retry-fail");
    let script = script_in_scratch(&dir, "tools/notarizar.sh");
    let app = mini_app(&dir);
    let mut extra = fast.to_vec();
    extra.push(("FAKE_STAPLE_FAILS", "99"));
    let out = run_notarizar(&dir, &script, &app, "Accepted", &extra);
    let said = text(&out);
    assert_eq!(out.status.code(), Some(1), "{said}");
    assert!(said.contains("did accept submission"), "{said}");
    let count = std::fs::read_to_string(dir.join("staple.count")).unwrap();
    assert_eq!(count.trim(), "5");
    let calls = std::fs::read_to_string(dir.join("calls.log")).unwrap();
    assert!(!calls.contains("stapler validate"), "{calls}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_signing_doc_rewrites_the_checksum_after_the_last_step() {
    let doc = read("docs/firma-y-notarizacion.md");
    let steps = doc
        .find("sh tools/notarizar.sh dist/Altrune-0.1.0-alpha.dmg")
        .unwrap();
    let rewrite = doc
        .find("shasum -a 256 Altrune-0.1.0-alpha.dmg > Altrune-0.1.0-alpha.dmg.sha256")
        .expect("the guide must regenerate the checksum");
    assert!(
        steps < rewrite,
        "the checksum is rewritten after notarising the image"
    );
    assert!(
        doc.contains("desfasado"),
        "the guide must say the first checksum goes stale"
    );
}

#[test]
fn notarizar_without_credentials_or_with_wrong_arguments_does_nothing() {
    let dir = scratch("notarizar-sin");
    let script = script_in_scratch(&dir, "tools/notarizar.sh");
    let bin = fake_tools(&dir);
    let app = mini_app(&dir);
    let run = |args: &[&Path], vars: &[(&str, &str)]| {
        Command::new("/bin/sh")
            .arg(&script)
            .args(args)
            .env_clear()
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env("FAKE_DIR", &dir)
            .envs(vars.iter().copied())
            .output()
            .unwrap()
    };
    let none = run(&[&app], &[]);
    assert_eq!(none.status.code(), Some(2), "{}", text(&none));
    assert!(
        text(&none).contains("no notarisation credentials"),
        "{}",
        text(&none)
    );
    let partial = run(&[&app], &[("ALTRUNE_NOTARIO_CLAVE_ID", "x")]);
    assert_eq!(partial.status.code(), Some(2), "{}", text(&partial));
    assert!(
        text(&partial).contains("ALTRUNE_NOTARIO_EMISOR"),
        "{}",
        text(&partial)
    );
    let no_args = run(&[], &[("ALTRUNE_NOTARIO_PERFIL", "p")]);
    assert_eq!(no_args.status.code(), Some(2), "{}", text(&no_args));
    let zip = dir.join("Altrune.zip");
    let wrong = run(&[&zip], &[("ALTRUNE_NOTARIO_PERFIL", "p")]);
    assert_eq!(wrong.status.code(), Some(2), "{}", text(&wrong));
    assert!(
        !dir.join("calls.log").exists(),
        "nothing may have been called"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
