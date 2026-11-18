//! Tests of the diagnosis pipeline: rules, findings, sorting, program logs and the report.

use std::collections::HashSet;
use std::path::Path;
use std::time::SystemTime;

use super::*;

fn lines(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn gathered(ring: &[&str]) -> Gathered {
    Gathered {
        ring: lines(ring),
        ..Default::default()
    }
}

fn run(ring: &[&str], ctx: &Context) -> Diagnosis {
    analyze_bottle(Path::new("/b"), &gathered(ring), ctx)
}

fn codes(d: &Diagnosis) -> Vec<&'static str> {
    d.findings.iter().map(|f| f.code).collect()
}

#[test]
fn recognises_known_symptoms_once() {
    let d = run(
            &[
                "0024:err:module:import_dll Library MSVCP140.dll (which is needed by L\"C:\\\\x.exe\") not found",
                "0024:err:module:import_dll Library MSVCP140.dll (which is needed by L\"C:\\\\x.exe\") not found",
                "[2026-11-14 22:34:14] Disabling GPU acceleration due to runtime detect",
                "Login: Failed to poll auth session. Result 2. Transport Error: 2",
            ],
            &Context::default(),
        );
    // Most certain first: the DLL line is stated by Wine, the others are probable.
    assert_eq!(codes(&d), vec!["missing-dll", "black-window", "network"]);
    assert_eq!(d.findings[0].matches, 2, "both lines are counted");
    assert_eq!(
        d.findings[0].evidence.len(),
        1,
        "the same line is shown once"
    );
}

#[test]
fn missing_dll_names_the_dll_and_each_dll_is_its_own_finding() {
    let d = run(
            &[
                "0024:err:module:import_dll Library MSVCP140.dll (which is needed by L\"C:\\\\x.exe\") not found",
                "0024:err:module:import_dll Library VCRUNTIME140_1.dll (which is needed by L\"C:\\\\x.exe\") not found",
                "0024:err:module:import_dll Library msvcp140.dll (which is needed by L\"C:\\\\y.exe\") not found",
            ],
            &Context::default(),
        );
    let titles: Vec<&str> = d.findings.iter().map(|f| f.title.as_str()).collect();
    assert_eq!(
        titles,
        vec![
            "Falta un componente que el programa necesita (MSVCP140.dll)",
            "Falta un componente que el programa necesita (VCRUNTIME140_1.dll)"
        ]
    );
    assert_eq!(d.findings[0].matches, 2, "the DLL's name ignores case");
    // The DLL's folder is not part of the name.
    let d = run(
        &["err:module:import_dll Library C:\\windows\\system32\\d3dx9_43.dll not found"],
        &Context::default(),
    );
    assert_eq!(
        d.findings[0].title,
        "Falta un componente que el programa necesita (d3dx9_43.dll)"
    );
    // No name on the line: the general title stays.
    let d = run(
        &["err:module:import_dll something not found"],
        &Context::default(),
    );
    assert_eq!(
        d.findings[0].title,
        "Falta un componente que el programa necesita"
    );
}

#[test]
fn rules_that_mean_different_things_have_different_codes() {
    let mut seen = HashSet::new();
    for r in RULES {
        assert!(seen.insert(r.code), "{} is used by two rules", r.code);
    }
    let d = run(
        &[
            "Login: Transport Error: 2",
            "wine: unknown error 10045",
            "err:module:load_dll Library x.dll failed to load",
        ],
        &Context::default(),
    );
    let c = codes(&d);
    assert!(c.contains(&"network") && c.contains(&"network-unsupported"));
    assert!(c.contains(&"dll-load-failed"));
}

/// The line Wine printed when Resident Evil Requiem opened its first video (2026-11-15).
#[test]
fn missing_video_is_named() {
    let d = run(
            &["07fc:err:winediag:h264_decoder_create GStreamer doesn't support H.264 decoding, please install appropriate plugins"],
            &Context::default(),
        );
    assert_eq!(codes(&d), vec!["no-video"]);
    assert_eq!(d.findings[0].remedy, Some(Remedy::AddVideo));
    let d = run(
            &["winegstreamer: failed to create deinterlace, are 64-bit GStreamer \"good\" plugins installed?"],
            &Context::default(),
        );
    assert_eq!(d.findings[0].code, "no-video-filter");
    assert_eq!(
        d.findings[0].title,
        "Falta una pieza para reproducir videos"
    );
}

/// Lines copied from the trace of the failing launcher (WineHQ 11.0, Apple M5).
#[test]
fn graphics_init_failure_is_named() {
    let l = [
            "0024:fixme:winediag:wined3d_select_feature_level None of the requested D3D feature levels is supported on this GPU with the current shader backend.",
            "0024:warn:d3d10core:D3D10CoreCreateDevice Failed to create device, hr 0x80004005.",
            "0024:warn:d2d:d2d_factory_get_device Failed to create device, hr 0x80004005.",
        ];
    let d = run(&l, &Context::default());
    assert_eq!(codes(&d), vec!["graphics-init", "direct2d-init"]);
    assert!(d.findings[0].evidence[0].contains("wined3d_select_feature_level"));
    assert_eq!(d.findings[0].confidence, Confidence::Comprobado);
    // Direct2D's own line is enough when Wine's diagnostic is hidden, but is less certain.
    let d = run(&l[2..], &Context::default());
    assert_eq!(d.findings[0].code, "direct2d-init");
    assert_eq!(
        d.findings[0].title,
        "No se pudo dibujar la ventana del programa"
    );
    assert_eq!(d.findings[0].confidence, Confidence::Probable);
}

/// The same symptom on a bottle that already uses DXMT must not tell the user to choose DXMT.
#[test]
fn dxmt_advice_is_dropped_when_the_bottle_already_uses_dxmt() {
    let l = ["0024:warn:d2d:d2d_factory_get_device Failed to create device, hr 0x80004005."];
    let on_dxmt = Context {
        uses_dxmt: Some(true),
        ..Default::default()
    };
    let f = &run(&l, &on_dxmt).findings[0];
    assert_eq!(f.confidence, Confidence::Pista);
    assert_eq!(f.remedy, None);
    assert!(f.suggestion.contains("ya usa Steam y launchers"));
    assert_eq!(
        f.checked,
        vec!["Esta botella ya usa Steam y launchers (DXMT)."]
    );
    let off = Context {
        uses_dxmt: Some(false),
        ..Default::default()
    };
    let f = &run(&l, &off).findings[0];
    assert_eq!(f.remedy, Some(Remedy::UseDxmt));
    assert!(f.checked[0].contains("no usa Steam y launchers"));
}

#[test]
fn rosetta_finding_is_confirmed_by_the_app() {
    let l = ["bad CPU type in executable"];
    let f = |rosetta_missing| {
        let ctx = Context {
            rosetta_missing,
            ..Default::default()
        };
        run(&l, &ctx).findings.remove(0)
    };
    assert_eq!(f(None).confidence, Confidence::Probable);
    assert_eq!(f(Some(true)).confidence, Confidence::Comprobado);
    let not = f(Some(false));
    assert_eq!(not.confidence, Confidence::Pista);
    assert!(not.suggestion.contains("Rosetta ya está instalada"));
}

#[test]
fn generic_failure_is_only_a_hint() {
    let l = ["Failed to initialize the thing"];
    let f = run(&l, &Context::default()).findings.remove(0);
    assert_eq!(f.code, "init-failed");
    assert_eq!(f.confidence, Confidence::Pista);
    assert!(!f.suggestion.contains("copia"));
}

#[test]
fn findings_are_sorted_by_confidence_and_say_where_they_were_seen() {
    let d = run(
        &[
            "wine: unknown error 10045",
            "winegstreamer: failed to create deinterlace, are plugins installed?",
            "✗ Steam: Permission denied (os error 13)",
        ],
        &Context::default(),
    );
    let order: Vec<Confidence> = d.findings.iter().map(|f| f.confidence).collect();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(order, sorted);
    assert_eq!(d.findings[0].code, "no-video-filter");
    let permission = d.findings.iter().find(|f| f.code == "permission").unwrap();
    assert_eq!(permission.source, Source::App);
    assert_eq!(permission.confidence, Confidence::Probable);
    assert_eq!(d.findings[0].source, Source::Wine);
}

#[test]
fn evidence_is_limited_to_three_distinct_redacted_lines() {
    let many: Vec<String> = (0..10)
        .map(|i| format!("0024:err:module:import_dll Library A.dll (needed by x{i}) not found"))
        .collect();
    let refs: Vec<&str> = many.iter().map(|s| s.as_str()).collect();
    let f = &run(&refs, &Context::default()).findings[0];
    assert_eq!(f.matches, 10);
    assert_eq!(f.evidence.len(), 3);
}

/// The service's error 183 is written while it stops; it must never be read as the cause.
#[test]
fn service_error_183_is_not_a_diagnosis() {
    let d = run(
            &[
                "[2026-11-14 23:14:13.372] [   476] [  1E4] Setting state to SERVICE_STOPPED (1)",
                "[2026-11-14 23:14:13.379] [   476] [  1E0] Last error: 183 (0xb7): File already exists.",
            ],
            &Context::default(),
        );
    assert!(d.findings.is_empty());
}

#[test]
fn nothing_found_says_how_much_was_read() {
    let d = run(
        &["wine: configuration has been updated."],
        &Context::default(),
    );
    assert!(d.findings.is_empty());
    assert_eq!(d.lines, 1);
    assert_eq!(d.sources, vec![Source::Wine]);
    assert!(d.status_line().contains("1 líneas"));
    let none = run(&[], &Context::default());
    assert_eq!(none.lines, 0);
    assert!(none.sources.is_empty());
    assert!(none.status_line().contains("todavía no hay mensajes"));
}

#[test]
fn sources_are_named_in_plain_words() {
    let mut d = run(&["wine: x", "▶ Steam"], &Context::default());
    assert_eq!(d.sources_label(), "Wine y la app");
    d.sources.push(Source::Steam);
    assert_eq!(d.sources_label(), "Wine, la app y Steam");
}

/// A line Wine printed once must still count after the log window scrolled past it.
#[test]
fn kept_matches_are_merged_with_the_window() {
    let kept = vec![Kept {
            rule: RULES.iter().position(|r| r.code == "graphics-init").unwrap(),
            line: "0024:fixme:winediag:wined3d_select_feature_level None of the requested D3D feature levels is supported".into(),
            bottle: "Juegos".into(),
            run: 3,
            count: 4,
        }];
    let g = Gathered {
        ring: lines(&["0024:fixme:d3d:wined3d_swapchain_present nothing special"]),
        kept,
        ..Default::default()
    };
    let d = analyze_bottle(Path::new("/b"), &g, &Context::default());
    assert_eq!(codes(&d), vec!["graphics-init"]);
    assert_eq!(d.findings[0].matches, 4);
    assert!(d.findings[0].evidence[0].contains("wined3d_select_feature_level"));
}

const STALLED: &str = "\
[2026-11-14 23:13:51.460] [DISPLAY] [Main ] [subsystem] Initializing engine...
[2026-11-14 23:13:51.462] [DISPLAY] [Main ] [subsystem] Initializing group 1...
[2026-11-14 23:13:51.485] [DISPLAY] [Workr] [selfupdate] Up-to-date.
[2026-11-14 23:13:51.623] [DISPLAY] [Main ] [localisation] Language updated to: 11
[2026-11-14 23:14:12.822] [DISPLAY] [Main ] [svc] Stopping the background service.
";

#[test]
fn rockstar_stall_is_read_from_its_own_log() {
    let last = rockstar_ui_stalled(STALLED).expect("stalled");
    assert!(last.contains("Stopping the background service"));
    // Still starting: nothing to report yet.
    let running = STALLED.rsplit_once("[2026-11-14 23:14:12").unwrap().0;
    assert!(rockstar_ui_stalled(running).is_none());
    // Went past the interface start and was closed later: not this failure.
    let went_on = STALLED.replace(
        "[Workr] [selfupdate] Up-to-date.",
        "[Main ] [subsystem] Initializing group 2...",
    );
    assert!(rockstar_ui_stalled(&went_on).is_none());
    assert!(rockstar_ui_stalled("").is_none());
}

#[test]
fn rockstar_stall_is_found_in_a_bottle_once_and_only_when_fresh() {
    let dir = std::env::temp_dir().join(format!("bl-rgl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let docs = dir.join("drive_c/users/legacy/Documents/Rockstar Games/Launcher");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("launcher.log"), STALLED).unwrap();
    // The Mac user's folder is a link to the same profile: reported once.
    #[cfg(unix)]
    std::os::unix::fs::symlink("legacy", dir.join("drive_c/users/yo")).unwrap();
    let logs = read_program_logs(&dir, None);
    assert_eq!(logs.rockstar.len(), 1);
    let g = Gathered {
        logs,
        ..Default::default()
    };
    let d = analyze_bottle(&dir, &g, &Context::default());
    assert_eq!(codes(&d), vec!["rockstar-ui-init"]);
    assert_eq!(d.findings[0].source, Source::Rockstar);
    assert_eq!(d.sources, vec![Source::Rockstar]);
    // Wine's own line comes first when both are known and it is surer.
    let g = Gathered {
            ring: lines(&["0024:fixme:winediag:wined3d_select_feature_level None of the requested D3D feature levels is supported"]),
            logs: read_program_logs(&dir, None),
            ..Default::default()
        };
    let d = analyze_bottle(&dir, &g, &Context::default());
    assert_eq!(codes(&d), vec!["graphics-init", "rockstar-ui-init"]);
    // A log written before the bottle's latest start belongs to an older run.
    let later = SystemTime::now() + std::time::Duration::from_secs(3600);
    assert!(read_program_logs(&dir, Some(later)).rockstar.is_empty());
    let earlier = SystemTime::now() - std::time::Duration::from_secs(3600);
    assert_eq!(read_program_logs(&dir, Some(earlier)).rockstar.len(), 1);
    // The Wine user folders (not the generic ones) are the names to hide: a profile folder that
    // came from another Mac may carry a person's name too.
    let mut names = wine_user_names(&dir);
    names.sort();
    assert_eq!(names, ["legacy".to_string(), "yo".to_string()]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn steam_logs_older_than_the_start_are_skipped() {
    let dir = std::env::temp_dir().join(format!("bl-steam-{}-logs", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let logs = dir.join("drive_c/Program Files (x86)/Steam/logs");
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(logs.join("webhelper.txt"), "Disabling GPU acceleration\n").unwrap();
    assert_eq!(read_program_logs(&dir, None).steam.len(), 1);
    let later = SystemTime::now() + std::time::Duration::from_secs(3600);
    assert!(read_program_logs(&dir, Some(later)).steam.is_empty());
    let g = Gathered {
        logs: read_program_logs(&dir, None),
        ..Default::default()
    };
    let d = analyze_bottle(&dir, &g, &Context::default());
    assert_eq!(d.findings[0].source, Source::Steam);
    assert_eq!(d.sources, vec![Source::Steam]);
    let _ = std::fs::remove_dir_all(&dir);
}

pub(super) fn lucia() -> Redactor {
    Redactor::new("/Users/lucia", &["lucia".to_string()])
}

#[test]
fn evidence_of_a_finding_is_redacted() {
    let ctx = Context {
        names: vec!["lucia".into()],
        ..Default::default()
    };
    let d = run(
            &["0024:err:module:import_dll Library X.dll (needed by L\"C:\\\\users\\\\lucia\\\\g.exe\") not found"],
            &ctx,
        );
    assert!(!d.findings[0].evidence[0].contains("lucia"));
}

#[test]
fn report_has_findings_sources_and_hidden_data() {
    let g = gathered(&["Bad CPU type in executable", "mail lucia@example.com"]);
    let ctx = Context::default();
    let d = analyze_bottle(Path::new("/b"), &g, &ctx);
    let r = report(&[("Botella", "Juegos".into())], &d, &g, &lucia());
    assert!(r.contains("Falta instalar Rosetta") && r.contains("[rosetta-missing]"));
    assert!(r.contains("Probable") && r.contains("fuente: Wine"));
    assert!(r.contains("Se vio: Bad CPU type in executable"));
    assert!(r.contains("Botella: Juegos"));
    assert!(r.contains("2 líneas de Wine"));
    assert!(r.contains("Léelo antes de compartirlo"));
    assert!(!r.contains("sin datos personales"));
    assert!(!r.contains("example.com"));
    // Nothing read: it says so rather than claiming all is well.
    let empty = Gathered::default();
    let d = analyze_bottle(Path::new("/b"), &empty, &ctx);
    let r = report(&[], &d, &empty, &lucia());
    assert!(r.contains("Ningún mensaje"));
    assert!(r.contains("no quiere decir que todo esté bien"));
}

/// A person who follows a diagnosis finds the option under the name the list gives it, and no
/// card speaks of an engine by a name that is not in the engine table.
#[test]
fn the_diagnosis_names_the_engines_as_the_table_does() {
    use crate::domain::engines::{AUTOMATIC, LAUNCHERS};
    let old_names = [
        "Gráficos (DirectX)",
        "Motor para juegos",
        "motor para juegos",
        "motor DXMT",
        "Motor DXMT",
        "motor normal",
        "Juegos con DirectX 12",
    ];
    let all_text =
        |r: &super::rules::Rule| format!("{} {} {}", r.title, r.explanation, r.suggestion);
    for r in RULES {
        let text = all_text(r);
        for old in old_names {
            assert!(!text.contains(old), "{}: «{old}» in «{text}»", r.code);
        }
        if r.dxmt_advice {
            assert!(
                r.suggestion.contains(LAUNCHERS),
                "{} sends to the DXMT choice by its name",
                r.code
            );
        }
    }
    assert!(super::rules::ALREADY_DXMT.contains(LAUNCHERS));
    // The wording in the rule that sends to plain Wine uses the names of the table.
    let newer = RULES.iter().find(|r| r.code == "prefix-newer").unwrap();
    assert!(newer.suggestion.contains(AUTOMATIC));
}

/// No card sends a person to something Altrune cannot give: the games engine and D3DMetal are
/// Apple's and are never installed from the app, so no rule asks to choose, add or download them.
#[test]
fn no_diagnosis_sends_to_apple_software_the_app_does_not_install() {
    for r in RULES {
        let text = format!("{} {} {}", r.title, r.explanation, r.suggestion);
        for asked in [
            "Juegos DirectX 12",
            "D3DMetal",
            "Añadir Direct3D",
            "Game Porting",
        ] {
            assert!(!text.contains(asked), "{}: «{asked}» in «{text}»", r.code);
        }
    }
    let newer = RULES.iter().find(|r| r.code == "prefix-newer").unwrap();
    assert_eq!(
        newer.suggestion,
        "Ábrela con «Automático» (Wine 11) o crea una botella nueva."
    );
}

/// A card's title says what a person sees, not the Wine part that failed.
#[test]
fn the_titles_have_no_wine_jargon() {
    let jargon = [
        "Direct3D",
        "Direct2D",
        "wined3d",
        "DLL",
        "prefijo",
        "Redistributable",
        "GStreamer",
        "Chromium",
        "deinterlace",
        "OpenGL",
        "Vulkan",
    ];
    for r in RULES {
        for word in jargon {
            assert!(
                !r.title.contains(word),
                "{}: the title «{}» says «{word}»",
                r.code,
                r.title
            );
        }
    }
}

/// A log file written for one test, removed with its folder when the test ends (also when it
/// fails), so test runs leave nothing behind in the temp dir.
struct ScratchLog(std::path::PathBuf);

impl std::ops::Deref for ScratchLog {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for ScratchLog {
    fn drop(&mut self) {
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// A log file as the app and the windowless launches write it.
fn log_file_with(name: &str, text: &str) -> ScratchLog {
    let dir = std::env::temp_dir().join(format!("bl-run-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("registro.txt");
    std::fs::write(&file, text).unwrap();
    ScratchLog(file)
}

fn marker(bottle: &str) -> String {
    format!("◆ Nuevo intento en la botella «{bottle}»")
}

fn from_finder(file: &str) -> String {
    format!("▶ {file} {FROM_FINDER}")
}

const MISSING_DLL: &str = "0024:err:module:import_dll Library MSVCP140.dll (which is needed by L\"C:\\\\x.exe\") not found";

/// A program opened from the Finder printed only into the log file: its lines are read back,
/// the decisive line is kept even when hundreds of lines follow it, and the window is bounded.
#[test]
fn a_windowless_run_is_read_back_from_the_log_file() {
    let mut text = format!(
        "{}\nlinea de otra botella\n{}\n{}\n{MISSING_DLL}\n",
        marker("Otra"),
        marker("Juegos"),
        from_finder("juego.exe")
    );
    for i in 0..900 {
        text.push_str(&format!("ruido {i}\n"));
    }
    let file = log_file_with("back", &text);
    let run = read_windowless_run(&file, "Juegos", crate::backend::marker_bottle, "")
        .expect("the latest start of the bottle was windowless");
    assert_eq!(run.ring.len(), 400, "bounded like the window");
    assert_eq!(run.ring.last().unwrap(), "ruido 899");
    assert!(
        !run.ring.iter().any(|l| l.contains("import_dll")),
        "the decisive line scrolled out of the last lines"
    );
    assert_eq!(run.kept.len(), 1, "but it is kept");
    assert!(run.kept[0].line.contains("MSVCP140.dll"));
    // Another bottle's start ends the run: nothing of it belongs to the first one.
    assert!(read_windowless_run(&file, "Otra", crate::backend::marker_bottle, "").is_none());
}

/// The lines of the run have the usual redaction, and they stop at the next start.
#[test]
fn a_windowless_run_is_redacted_and_ends_at_the_next_start() {
    let me = std::env::var("USER").unwrap_or_else(|_| "persona".into());
    let user = crate::diagnostics::Redactor::for_this_mac(&me, &[]);
    let leaked = format!("0024:err:module:import_dll Library /Users/{me}/x/A.dll not found");
    assert_ne!(user.apply(&leaked), leaked, "the user's name is hidden");
    let text = format!(
        "{}\n{}\n{leaked}\n{}\nlinea posterior\n",
        marker("Juegos"),
        from_finder("juego.exe"),
        marker("Otra")
    );
    let file = log_file_with("redact", &text);
    let run = read_windowless_run(&file, "Juegos", crate::backend::marker_bottle, &me).unwrap();
    assert!(!run.ring.iter().any(|l| l.contains("posterior")));
    assert_eq!(run.kept.len(), 1);
    assert!(
        !run.kept[0].line.contains(&format!("/Users/{me}/")),
        "{}",
        run.kept[0].line
    );
}

/// The line that explains a disk failure (written by the app, with the operating system's own
/// English text) is not something the program printed: no rule keeps it, while the lines the
/// program printed around it are still kept.
#[test]
fn a_windowless_run_ignores_the_disk_detail_lines() {
    let detail = format!(
        "{DISK_DETAIL_PREFIX}Guardar el informe (/x/y) falló. Tipo: PermissionDenied. \
         Código del sistema: 13. Texto: Permission denied (os error 13)"
    );
    let text = format!(
        "{}\n{}\n{detail}\n{MISSING_DLL}\n",
        marker("Juegos"),
        from_finder("juego.exe")
    );
    let file = log_file_with("disk-detail", &text);
    let run = read_windowless_run(&file, "Juegos", crate::backend::marker_bottle, "").unwrap();
    assert_eq!(run.kept.len(), 1, "{:?}", run.kept);
    assert!(run.kept[0].line.contains("MSVCP140.dll"));
}

/// A later start from the window owns the window's lines; a start the tail cannot reach, a
/// missing file and a start without the Finder's line give nothing.
#[test]
fn only_a_windowless_latest_start_is_read_from_the_file() {
    let later_from_window = format!(
        "{}\n{}\n{MISSING_DLL}\n{}\n▶ juego.exe\n",
        marker("Juegos"),
        from_finder("juego.exe"),
        marker("Juegos")
    );
    let file = log_file_with("later", &later_from_window);
    assert!(read_windowless_run(&file, "Juegos", crate::backend::marker_bottle, "").is_none());

    let only_window = format!("{}\n▶ juego.exe\n{MISSING_DLL}\n", marker("Juegos"));
    let file = log_file_with("window", &only_window);
    assert!(read_windowless_run(&file, "Juegos", crate::backend::marker_bottle, "").is_none());

    assert!(read_windowless_run(
        Path::new("/no/hay/registro.txt"),
        "Juegos",
        crate::backend::marker_bottle,
        ""
    )
    .is_none());

    // The marker is further back than the tail that is read: it is not guessed.
    let mut far = format!("{}\n{}\n", marker("Juegos"), from_finder("juego.exe"));
    let filler = "x".repeat(1000);
    for _ in 0..5000 {
        far.push_str(&filler);
        far.push('\n');
    }
    let file = log_file_with("far", &far);
    assert!(read_windowless_run(&file, "Juegos", crate::backend::marker_bottle, "").is_none());
}
