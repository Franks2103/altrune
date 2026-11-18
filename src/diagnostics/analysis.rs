//! Turns what was read for a bottle into findings: runs the rules over the lines, merges what
//! was kept with the window, adds the Rockstar log check, and refines each finding with what
//! the app confirmed by itself.

use std::path::Path;

use super::findings::{Confidence, Diagnosis, Finding, Remedy, Source};
use super::redact::Redactor;
use super::rules::{rules_hit, Rule, ALREADY_DXMT, RULES};
use super::sources::{rockstar_ui_stalled, ProgramLogs};
use super::state::Kept;

/// Lines the app writes itself start with one of these marks; everything else came from a
/// program or Wine.
fn line_source(line: &str) -> Source {
    if line.starts_with("✗ ") || line.starts_with("▶ ") || line.starts_with("◆ ") {
        Source::App
    } else {
        Source::Wine
    }
}

/// The name of the DLL a Wine line talks about: the word that ends in ".dll", without its
/// folder. `err:module:import_dll Library MSVCP140.dll (which is needed by …) not found`.
fn dll_name(line: &str) -> Option<String> {
    let low = line.to_ascii_lowercase();
    let start = low.find("library ").map_or(0, |i| i + "library ".len());
    line[start..]
        .split(|c: char| c.is_whitespace() || "\"'(),;".contains(c))
        .find(|w| w.to_ascii_lowercase().ends_with(".dll"))
        .and_then(|w| w.rsplit(['\\', '/']).next())
        .filter(|n| {
            n.len() <= 64
                && n.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_.+-".contains(c))
        })
        .map(str::to_string)
}

/// Facts about the bottle that the app can confirm cheaply, read on a worker thread.
#[derive(Clone, Debug, Default)]
pub struct Context {
    /// Whether the Wine this bottle runs on is the DXMT engine (None when unknown).
    pub uses_dxmt: Option<bool>,
    /// Whether this Mac lacks Rosetta (None when unknown).
    pub rosetta_missing: Option<bool>,
    /// The name of this Mac's user, hidden in paths (the app looks it up; this module does not).
    pub mac_user: String,
    /// Names that must be hidden in paths, besides this Mac's user (the bottle's Wine users).
    pub names: Vec<String>,
}

/// Everything read for one bottle before the rules run.
#[derive(Clone, Debug, Default)]
pub struct Gathered {
    /// What Wine and the app printed since the bottle's latest start.
    pub ring: Vec<String>,
    /// Matches kept from the same run (they survive the log window scrolling).
    pub kept: Vec<Kept>,
    pub logs: ProgramLogs,
}

/// One finding being collected: the same rule (and, for DLLs, the same DLL) seen again adds to it.
struct Pending {
    rule: usize,
    dll: Option<String>,
    source: Source,
    ring_matches: usize,
    kept_matches: usize,
    evidence: Vec<String>,
}

impl Pending {
    fn see(&mut self, line: String) {
        if self.evidence.len() < 3 && !self.evidence.contains(&line) {
            self.evidence.push(line);
        }
    }
}

const MAX_DLL_FINDINGS: usize = 5;

fn pending_for<'a>(
    list: &'a mut Vec<Pending>,
    rule: usize,
    line: &str,
    source: Source,
) -> Option<&'a mut Pending> {
    let dll = RULES[rule].names_dll.then(|| dll_name(line)).flatten();
    let at = list.iter().position(|p| {
        p.rule == rule
            && p.dll.as_deref().map(str::to_lowercase) == dll.as_deref().map(str::to_lowercase)
    });
    match at {
        Some(i) => Some(&mut list[i]),
        None => {
            if RULES[rule].names_dll
                && list.iter().filter(|p| p.rule == rule).count() >= MAX_DLL_FINDINGS
            {
                return None;
            }
            list.push(Pending {
                rule,
                dll,
                source,
                ring_matches: 0,
                kept_matches: 0,
                evidence: vec![],
            });
            list.last_mut()
        }
    }
}

/// Turns what was read into the findings, the number of lines and the sources.
pub fn analyze_bottle(bottle: &Path, g: &Gathered, ctx: &Context) -> Diagnosis {
    let red = Redactor::for_this_mac(&ctx.mac_user, &ctx.names);
    let mut list: Vec<Pending> = vec![];

    // What was kept as the lines arrived comes first: it is older than the window and includes
    // lines the window has already dropped.
    for k in &g.kept {
        let source = line_source(&k.line);
        if let Some(p) = pending_for(&mut list, k.rule, &k.line, source) {
            p.kept_matches += k.count;
            p.see(red.apply(&k.line));
        }
    }
    let mut wine_lines = 0;
    let mut app_lines = 0;
    let mut scan = |lines: &[String], force: Option<Source>, list: &mut Vec<Pending>| {
        for line in lines {
            let source = force.unwrap_or_else(|| line_source(line));
            if force.is_none() {
                match source {
                    Source::App => app_lines += 1,
                    _ => wine_lines += 1,
                }
            }
            for rule in rules_hit(line) {
                if let Some(p) = pending_for(list, rule, line, source) {
                    p.ring_matches += 1;
                    p.see(red.apply(line.trim()));
                }
            }
        }
    };
    scan(&g.ring, None, &mut list);
    scan(&g.logs.steam, Some(Source::Steam), &mut list);

    let mut findings: Vec<Finding> = list
        .into_iter()
        .map(|p| {
            let rule = &RULES[p.rule];
            let title = match (&p.dll, rule.code) {
                (Some(d), "missing-dll") => {
                    format!("Falta un componente que el programa necesita ({d})")
                }
                (Some(d), _) => format!("No se pudo cargar un componente del programa ({d})"),
                (None, _) => rule.title.to_string(),
            };
            let mut f = Finding {
                code: rule.code,
                title,
                explanation: rule.explanation,
                suggestion: rule.suggestion.to_string(),
                confidence: rule.confidence,
                source: p.source,
                matches: p.ring_matches.max(p.kept_matches),
                evidence: p.evidence,
                checked: vec![],
                remedy: rule.remedy,
            };
            refine(&mut f, rule, ctx);
            f
        })
        .collect();

    let mut rockstar_lines = 0;
    for text in &g.logs.rockstar {
        rockstar_lines += text.lines().filter(|l| !l.trim().is_empty()).count();
        if let Some(evidence) = rockstar_ui_stalled(text) {
            if findings.iter().any(|f| f.code == "rockstar-ui-init") {
                break;
            }
            let mut f = Finding {
                code: "rockstar-ui-init",
                title: "Rockstar Games Launcher no pudo crear su ventana".into(),
                explanation: "Su registro se detiene al iniciar la interfaz («Initializing group 1») y enseguida apaga su servicio. Es probable que la interfaz, que se dibuja con Direct2D sobre Direct3D 10, no pudiera iniciar; el «Error 183» del servicio aparece al cerrarse y no es la causa.",
                suggestion: "En los ajustes de esta botella elige Gráficos: Steam y launchers (DXMT). DXMT es un motor libre que dibuja DirectX 10/11 con Metal, que el Wine normal no trae en el Mac. Cambiar la versión de Windows o reinstalar el launcher no lo arregla.".into(),
                confidence: Confidence::Probable,
                source: Source::Rockstar,
                matches: 1,
                evidence: vec![red.apply(&evidence)],
                checked: vec![],
                remedy: Some(Remedy::UseDxmt),
            };
            let rule = Rule {
                code: "rockstar-ui-init",
                all: &[],
                title: "",
                explanation: "",
                suggestion: "",
                confidence: Confidence::Probable,
                names_dll: false,
                dxmt_advice: true,
                remedy: Some(Remedy::UseDxmt),
            };
            refine(&mut f, &rule, ctx);
            findings.push(f);
            break;
        }
    }
    // Stable: findings of the same confidence keep the order of the rules.
    findings.sort_by_key(|f| f.confidence);

    let mut sources = vec![];
    for (n, s) in [
        (wine_lines, Source::Wine),
        (app_lines, Source::App),
        (g.logs.steam.len(), Source::Steam),
        (rockstar_lines, Source::Rockstar),
    ] {
        if n > 0 {
            sources.push(s);
        }
    }
    Diagnosis {
        bottle: bottle.to_path_buf(),
        findings,
        lines: wine_lines + app_lines + g.logs.steam.len() + rockstar_lines,
        sources,
        ticket: 0,
    }
}

/// Uses what the app confirmed about the bottle to raise or lower the confidence of a finding
/// and to drop advice that was already followed.
fn refine(f: &mut Finding, rule: &Rule, ctx: &Context) {
    match f.code {
        "rosetta-missing" => match ctx.rosetta_missing {
            Some(true) => {
                f.confidence = Confidence::Comprobado;
                f.checked
                    .push("Este Mac no tiene Rosetta instalada.".into());
            }
            Some(false) => {
                f.confidence = Confidence::Pista;
                f.checked
                    .push("Este Mac sí tiene Rosetta instalada.".into());
                f.suggestion = "Rosetta ya está instalada, así que el mensaje apunta a otro archivo. Comprueba que el programa sea de Windows (no de Mac) y que la descarga terminó.".into();
            }
            None => {}
        },
        "permission" if f.source == Source::App => f.confidence = Confidence::Probable,
        _ => {}
    }
    if rule.dxmt_advice {
        match ctx.uses_dxmt {
            Some(true) => {
                f.confidence = Confidence::Pista;
                f.checked
                    .push("Esta botella ya usa Steam y launchers (DXMT).".into());
                f.suggestion = ALREADY_DXMT.into();
                f.remedy = None;
            }
            Some(false) => f
                .checked
                .push("Esta botella no usa Steam y launchers (DXMT).".into()),
            None => {}
        }
    } else if f.code == "black-window" && ctx.uses_dxmt == Some(true) {
        f.checked
            .push("Esta botella ya usa Steam y launchers (DXMT).".into());
        f.remedy = None;
    }
}
