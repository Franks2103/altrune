//! Static checks of the release workflow (.github/workflows/release.yml) and checks of
//! tools/notas-version.sh. Nothing here runs the workflow, signs, notarises, uploads or talks to
//! GitHub: the workflow is read as text, and the notes script runs on a tiny fake image in a
//! temporary folder.

use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn workflow() -> String {
    read(".github/workflows/release.yml")
}

/// The workflow without its full-line comments, so a comment cannot hide or fake a match.
fn code(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect()
}

/// The lines of a top-level key (`on:`, `permissions:`...) up to the next top-level key.
fn top_section(text: &str, key: &str) -> Vec<String> {
    let header = format!("{key}:");
    let mut out = Vec::new();
    let mut inside = false;
    for line in code(text) {
        let top = !line.is_empty() && !line.starts_with(' ');
        if top {
            inside = line.starts_with(&header);
        }
        if inside {
            out.push(line.to_string());
        }
    }
    out
}

/// `(name, body)` of every job: a line with exactly two spaces of indent under `jobs:`.
fn jobs(text: &str) -> Vec<(String, String)> {
    let mut result: Vec<(String, String)> = Vec::new();
    let mut in_jobs = false;
    for line in code(text) {
        if !line.is_empty() && !line.starts_with(' ') {
            in_jobs = line.starts_with("jobs:");
            continue;
        }
        if !in_jobs {
            continue;
        }
        let is_header =
            line.starts_with("  ") && !line.starts_with("   ") && line.trim_end().ends_with(':');
        if is_header {
            result.push((line.trim().trim_end_matches(':').to_string(), String::new()));
        } else if let Some((_, body)) = result.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    result
}

fn job(text: &str, name: &str) -> String {
    jobs(text)
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("no job named {name}"))
        .1
}

/// Every `run:` value (one-line or block scalar) as one string per step.
fn run_blocks(text: &str) -> Vec<String> {
    let lines = code(text);
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        let key = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        if let Some(value) = key.strip_prefix("run:") {
            let indent = line.len() - trimmed.len() + if trimmed.starts_with("- ") { 2 } else { 0 };
            let value = value.trim();
            if value.starts_with('|') || value.starts_with('>') {
                let mut block = String::new();
                i += 1;
                while i < lines.len() {
                    let next = lines[i];
                    let next_indent = next.len() - next.trim_start().len();
                    if !next.trim().is_empty() && next_indent <= indent {
                        break;
                    }
                    block.push_str(next.trim_start());
                    block.push('\n');
                    i += 1;
                }
                blocks.push(block);
                continue;
            }
            blocks.push(value.to_string());
        }
        i += 1;
    }
    blocks
}

/// Shell commands with their `\` line continuations joined, one string per command line.
fn commands(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for block in run_blocks(text) {
        let mut current = String::new();
        for line in block.lines() {
            let line = line.trim();
            if let Some(stripped) = line.strip_suffix('\\') {
                current.push_str(stripped.trim_end());
                current.push(' ');
            } else {
                current.push_str(line);
                out.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            out.push(current);
        }
    }
    out
}

#[test]
fn only_a_manual_trigger() {
    let on = top_section(&workflow(), "on").join("\n");
    assert!(on.contains("workflow_dispatch:"), "{on}");
    for other in [
        "push:",
        "pull_request",
        "schedule:",
        "workflow_run",
        "release:",
        "workflow_call",
        "repository_dispatch",
    ] {
        assert!(
            !on.contains(other),
            "the workflow must only run by hand, found {other}"
        );
    }
    assert!(
        on.contains("version:") && on.contains("required: true"),
        "{on}"
    );
}

#[test]
fn permissions_are_read_only_except_the_release_job() {
    let text = workflow();
    let top = top_section(&text, "permissions").join("\n");
    let top: Vec<&str> = top.lines().map(str::trim).collect();
    assert_eq!(
        top,
        ["permissions:", "contents: read"],
        "top-level permissions"
    );

    let writes: Vec<&str> = code(&text)
        .into_iter()
        .filter(|l| l.contains(": write"))
        .collect();
    assert_eq!(writes.len(), 1, "exactly one write permission: {writes:?}");
    assert!(
        job(&text, "release").contains("contents: write"),
        "the write belongs to the release job"
    );
    for name in ["check", "package"] {
        assert!(
            !job(&text, name).contains("permissions:"),
            "{name} keeps the read-only token"
        );
    }
    for forbidden in [
        "write-all",
        "id-token",
        "actions: write",
        "packages:",
        "pull-requests:",
    ] {
        assert!(
            !code(&text).iter().any(|l| l.contains(forbidden)),
            "{forbidden}"
        );
    }
}

#[test]
fn it_only_ever_makes_a_draft_prerelease() {
    let text = workflow();
    let all = code(&text).join("\n");
    for forbidden in [
        "--draft=false",
        "--draft=0",
        "--prerelease=false",
        "--latest",
        "draft: false",
        "prerelease: false",
        "gh release delete",
        "gh api",
        "softprops/action-gh-release",
        "ncipollo/release-action",
    ] {
        assert!(!all.contains(forbidden), "found {forbidden}");
    }

    let release = job(&text, "release");
    assert!(
        release.contains("gh release create"),
        "the release job creates the release"
    );
    let mut seen = 0;
    for command in commands(&text) {
        if command.contains("gh release create") || command.contains("gh release edit") {
            assert!(command.contains("--draft"), "no --draft in: {command}");
            assert!(
                command.contains("--prerelease"),
                "no --prerelease in: {command}"
            );
            assert!(command.contains("--target"), "no --target in: {command}");
            seen += 1;
        }
    }
    assert!(
        seen >= 2,
        "both the create and the edit command are checked ({seen})"
    );

    // Anything that changes a release lives in the one job that may write.
    for name in ["check", "package"] {
        let body = job(&text, name);
        for verb in ["create", "edit", "upload", "delete"] {
            assert!(
                !body.contains(&format!("gh release {verb}")),
                "{name} must not run gh release {verb}"
            );
        }
    }
    // A published release of the same version stops the run.
    assert!(
        release.contains("exists"),
        "the published-release case is handled"
    );
}

#[test]
fn every_action_is_pinned_to_a_full_commit() {
    let text = workflow();
    let mut count = 0;
    for line in code(&text) {
        let trimmed = line.trim_start();
        let Some(value) = trimmed
            .strip_prefix("- uses:")
            .or_else(|| trimmed.strip_prefix("uses:"))
        else {
            continue;
        };
        count += 1;
        let (reference, comment) = match value.split_once(" #") {
            Some((r, c)) => (r.trim(), c.trim()),
            None => (value.trim(), ""),
        };
        let (name, sha) = reference
            .split_once('@')
            .unwrap_or_else(|| panic!("no @ in {line}"));
        assert!(!name.starts_with("./") && name.contains('/'), "{line}");
        assert!(
            sha.len() == 40
                && sha
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "not a 40-hex commit: {line}"
        );
        assert!(
            comment.starts_with('v'),
            "name the version after the SHA in a comment: {line}"
        );
    }
    assert!(count >= 6, "found only {count} uses: lines");
}

#[test]
fn run_lines_never_contain_expressions() {
    // Inputs, event data and secrets reach the shell through env: only. A ${{ }} inside a run:
    // block is pasted into the script before the shell sees it, which is how injection happens.
    let text = workflow();
    let blocks = run_blocks(&text);
    assert!(blocks.len() >= 20, "run_blocks found only {}", blocks.len());
    for block in blocks {
        assert!(
            !block.contains("${{"),
            "an expression inside a run block:\n{block}"
        );
        assert!(
            !block.contains("inputs."),
            "inputs used inside a run block:\n{block}"
        );
        assert!(
            !block.contains("github.event"),
            "event data inside a run block:\n{block}"
        );
    }
    // The version reaches the steps as an environment variable.
    assert!(text.contains("VERSION: ${{ inputs.version }}"));
}

#[test]
fn the_version_is_checked_against_cargo_toml_before_anything_else() {
    let text = workflow();
    let check = job(&text, "check");
    let validate = check.find("Validate the version").expect("validation step");
    let toolchain = check
        .find("Install the Rust toolchain")
        .expect("toolchain step");
    assert!(validate < toolchain, "validate first");
    assert!(
        check.contains("grep -Eq") && check.contains("Cargo.toml"),
        "regex and Cargo.toml"
    );
    for step in [
        "cargo fmt --all --check",
        "cargo clippy --locked --all-targets -- -D warnings",
        "cargo test --locked",
    ] {
        assert!(check.contains(step), "{step}");
    }
    assert!(check.contains("runs-on: macos-15"));
    assert!(check.contains("rustup toolchain install 1.100.0"));
    // The check job stays in step with ci.yml.
    let ci = read(".github/workflows/ci.yml");
    assert!(
        ci.contains("rustup toolchain install 1.100.0"),
        "ci.yml moved its toolchain: move this one too"
    );
}

#[test]
fn the_check_job_guards_the_branch_the_tag_and_the_token() {
    let text = workflow();
    let check = job(&text, "check");
    assert!(
        check.contains("refs/heads/main"),
        "only the main branch may start a release"
    );
    assert!(check.contains("fetch-tags: true"), "the tags are fetched");
    assert!(
        check.contains("git rev-parse -q --verify \"refs/tags/v$VERSION\""),
        "an existing tag is looked at"
    );
    // The token is only the env of the one step that calls gh, not of the whole job.
    let job_level = check.split("    steps:").next().unwrap();
    assert!(!job_level.contains("GH_TOKEN"), "the token is at job level");
    // In the release job a failing `gh release list` must stop the step, so it is never piped.
    let release = job(&text, "release");
    assert!(
        !release.contains("--json tagName,isDraft \\\n"),
        "gh release list is piped: a failure would look like an empty list"
    );
    assert!(release.contains("lista=$(gh release list"));
}

#[test]
fn jobs_run_in_order_and_secrets_stay_in_the_package_job() {
    let text = workflow();
    let names: Vec<String> = jobs(&text).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["check", "package", "release"]);
    assert!(job(&text, "package").contains("needs: check"));
    assert!(job(&text, "release").contains("needs: package"));

    let all = code(&text).join("\n");
    assert_eq!(
        all.matches("secrets.").count(),
        job(&text, "package").matches("secrets.").count(),
        "a secret is used outside the package job"
    );
    for name in ["check", "release"] {
        assert!(
            !job(&text, name).contains("secrets."),
            "{name} must not see a secret"
        );
    }
    let package = job(&text, "package");
    for secret in [
        "MACOS_CERTIFICADO_P12_BASE64",
        "MACOS_CERTIFICADO_CLAVE",
        "ALTRUNE_IDENTIDAD",
        "NOTARIO_CLAVE_P8_BASE64",
        "NOTARIO_CLAVE_ID",
        "NOTARIO_EMISOR",
    ] {
        assert!(package.contains(&format!("secrets.{secret}")), "{secret}");
    }
    // Secrets are only the env of a step, never a workflow-level or job-level variable.
    assert!(!top_section(&text, "env").join("\n").contains("secrets."));
    let job_level = package.split("    steps:").next().unwrap();
    assert!(!job_level.contains("secrets."), "a secret at job level");
    // The default token is the only credential the release job uses.
    assert!(job(&text, "release").contains("GH_TOKEN: ${{ github.token }}"));
}

#[test]
fn the_package_job_uses_the_release_environment_and_cleans_up() {
    let package = job(&workflow(), "package");
    assert!(package.contains("environment: release"));
    assert!(package.contains("runs-on: macos-15"));
    for tool in [
        "tools/licencias.sh --instalar",
        "tools/licencias.sh --comprobar",
        "tools/empaquetar.sh",
        "tools/llavero-ci.sh crear",
        "tools/firmar.sh app",
        "tools/notarizar.sh dist/Altrune.app",
        "tools/crear-dmg.sh",
        "tools/firmar.sh dmg",
        "tools/llavero-ci.sh borrar",
        "tools/notas-version.sh",
        "ALTRUNE_PYTHON",
    ] {
        assert!(package.contains(tool), "{tool}");
    }
    // The keychain is removed even after a failure, and only when it was created.
    let cleanup = package.find("tools/llavero-ci.sh borrar").unwrap();
    let before = &package[..cleanup];
    let step_start = before.rfind("- name:").unwrap();
    assert!(
        package[step_start..cleanup].contains("if: always()"),
        "cleanup must run on failure too"
    );
    // The signing steps are conditional on the secrets being present.
    for name in [
        "Sign the app",
        "Notarise and staple the app",
        "Sign the disk image",
        "Notarise and staple the disk image",
    ] {
        let at = package.find(name).unwrap_or_else(|| panic!("step {name}"));
        let step = &package[at..];
        let step = &step[..step.find("\n      - name:").unwrap_or(step.len())];
        assert!(
            step.contains("steps.secretos.outputs.firmado == 'true'"),
            "{name} is not conditional"
        );
    }
    // The checksum is recomputed from the final image, after signing.
    assert!(
        package.find("Write the SHA-256").unwrap()
            > package.find("Notarise and staple the disk image").unwrap()
    );
}

#[test]
fn the_unsigned_build_is_named_in_the_title_and_the_notes() {
    let text = workflow();
    assert!(job(&text, "release").contains("(versión de prueba sin firmar)"));
    assert!(job(&text, "package").contains("sin-firmar"));
    assert!(job(&text, "package").contains("::warning::"));
}

// ---- tools/notas-version.sh -------------------------------------------------------------

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("altrune-release-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn notes(args: &[&str], skip_ticket_check: bool) -> std::process::Output {
    let mut cmd = Command::new("sh");
    cmd.current_dir(root())
        .arg("tools/notas-version.sh")
        .args(args);
    if skip_ticket_check {
        cmd.env("ALTRUNE_NOTAS_NO_VERIFICAR", "1");
    }
    cmd.output().expect("run sh")
}

fn sha256_of(path: &std::path::Path) -> String {
    let out = Command::new("shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .expect("shasum");
    assert!(out.status.success());
    String::from_utf8(out.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

fn fake_image(dir: &std::path::Path, version: &str) -> PathBuf {
    let dmg = dir.join(format!("Altrune-{version}.dmg"));
    std::fs::write(&dmg, b"this is not a disk image, only bytes to hash\n").unwrap();
    dmg
}

#[test]
fn notes_for_an_unsigned_image_start_with_the_warning_and_end_with_the_checksum() {
    let dir = scratch("unsigned");
    let dmg = fake_image(&dir, "0.1.0-alpha");
    let out = notes(&["0.1.0-alpha", dmg.to_str().unwrap(), "sin-firmar"], false);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        stdout.starts_with("> ## Versión de prueba sin firmar"),
        "{}",
        &stdout[..80.min(stdout.len())]
    );
    for needle in [
        "Developer ID",
        "no está notarizada",
        "Privacidad y seguridad",
        "Abrir igualmente",
        "documenta abrir una app no notarizada",
        "solo para probar",
    ] {
        assert!(stdout.contains(needle), "the warning lacks: {needle}");
    }
    // The text of the version follows the warning.
    let doc = read("docs/releases/0.1.0-alpha.md");
    assert!(
        stdout.contains(doc.trim_end()),
        "the notes of the version are included as they are"
    );
    assert!(stdout.find("Versión de prueba").unwrap() < stdout.find("## Qué es Altrune").unwrap());

    let sum = sha256_of(&dmg);
    assert_eq!(sum.len(), 64);
    assert!(
        stdout.contains(&format!("{sum}  Altrune-0.1.0-alpha.dmg")),
        "the SHA-256 line"
    );
    assert!(stdout.contains("shasum -a 256 -c Altrune-0.1.0-alpha.dmg.sha256"));
    assert!(
        stdout.find("## Qué es Altrune").unwrap() < stdout.find(&sum).unwrap(),
        "the checksum comes last"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn notes_for_a_signed_image_have_no_warning() {
    let dir = scratch("signed");
    let dmg = fake_image(&dir, "0.1.0-alpha");
    let out = notes(&["0.1.0-alpha", dmg.to_str().unwrap(), "firmado"], true);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!stdout.contains("Versión de prueba sin firmar"));
    assert!(!stdout.contains("Abrir igualmente"));
    assert!(stdout.contains("notarizada por Apple"));
    assert!(stdout.contains(&sha256_of(&dmg)));

    // Without the skip, a fake image has no notarisation ticket, so the script refuses to
    // describe it as signed.
    let refused = notes(&["0.1.0-alpha", dmg.to_str().unwrap(), "firmado"], false);
    assert!(
        !refused.status.success(),
        "a fake image must not be described as firmado"
    );
    assert!(
        refused.stdout.is_empty(),
        "nothing is printed when it refuses"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn notes_script_refuses_bad_input() {
    let dir = scratch("bad");
    let dmg = fake_image(&dir, "0.1.0-alpha");
    let other = dir.join("Other.dmg");
    std::fs::write(&other, b"x").unwrap();
    let dmg = dmg.to_str().unwrap();
    let cases: [(Vec<&str>, &str); 6] = [
        (
            vec!["0.1.0-alpha", dmg, "publicado"],
            "firmado or sin-firmar",
        ),
        (vec!["0.1.0-alpha;id", dmg, "sin-firmar"], "is not like"),
        (vec!["../0.1.0-alpha", dmg, "sin-firmar"], "is not like"),
        (
            vec!["9.9.9", dmg, "sin-firmar"],
            "no notes for this version",
        ),
        (
            vec!["0.1.0-alpha", other.to_str().unwrap(), "sin-firmar"],
            "expected Altrune-0.1.0-alpha.dmg",
        ),
        (vec!["0.1.0-alpha"], "usage"),
    ];
    for (args, message) in cases {
        let out = notes(&args, false);
        assert!(!out.status.success(), "{args:?} must fail");
        assert!(out.stdout.is_empty(), "{args:?} printed notes");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains(message), "{args:?}: {stderr}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn notes_script_only_reads_and_prints() {
    let script = read("tools/notas-version.sh");
    let code: Vec<&str> = script
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();
    let code = code.join("\n");
    for forbidden in [
        "curl",
        "gh ",
        "codesign",
        "notarytool",
        "hdiutil",
        "/usr/bin/open",
        "rm -",
        "> \"$",
        ">> \"$",
    ] {
        assert!(!code.contains(forbidden), "{forbidden}");
    }
    assert!(script.starts_with("#!/bin/sh"));
    assert!(code.contains("set -eu"));
}

#[test]
fn the_notes_of_the_first_version_say_the_honest_things() {
    let doc = read("docs/releases/0.1.0-alpha.md");
    for needle in [
        "primera versión de prueba pública (alpha)",
        "Apple silicon",
        "macOS 27.0.1",
        "único Mac",
        "Arrastra Altrune a Aplicaciones",
        "No lleva Wine dentro",
        "No incluye software de Apple",
        "Apache-2.0",
        "Contents/Resources/Licencias/",
        "SHA-256",
        "shasum -a 256 -c Altrune-0.1.0-alpha.dmg.sha256",
        "Limitaciones conocidas",
    ] {
        assert!(doc.contains(needle), "the notes lack: {needle}");
    }
    // The script adds the checksum and the warning; the text itself never claims a signature.
    assert!(!doc.contains("notarizada por Apple"));
    assert!(!doc.contains("/Users/"));
}
