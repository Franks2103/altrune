//! What a diagnosis says: how sure the app is, where the lines came from, the findings
//! themselves and the outcome for one bottle with its status text.

use std::path::PathBuf;

/// How sure the app is that the finding is what is wrong. Ordered from most to least sure, so
/// sorting puts the best-supported finding first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// The line itself, or something the app checked, states the cause.
    Comprobado,
    /// The line is a known symptom of this cause, but other causes are possible.
    Probable,
    /// A loose hint: it often means nothing.
    Pista,
}

impl Confidence {
    pub fn label(self) -> &'static str {
        match self {
            Confidence::Comprobado => "Comprobado",
            Confidence::Probable => "Probable",
            Confidence::Pista => "Pista",
        }
    }
}

/// Where the lines that matched came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Wine,
    Steam,
    Rockstar,
    /// A line this app wrote itself ("▶ …", "✗ …").
    App,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Wine => "Wine",
            Source::Steam => "Steam",
            Source::Rockstar => "Rockstar Launcher",
            Source::App => "la app",
        }
    }
}

/// A fix the app can offer as a button, because it maps to something the app already does and
/// that can be undone. The window decides whether the button is shown right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Remedy {
    /// Run the bottle on the DXMT engine.
    UseDxmt,
    /// Add GStreamer (video) to the DXMT engine.
    AddVideo,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// Stable identifier, e.g. "rosetta-missing". Two rules never share one.
    pub code: &'static str,
    pub title: String,
    /// The probable cause, in plain words.
    pub explanation: &'static str,
    pub suggestion: String,
    pub confidence: Confidence,
    pub source: Source,
    /// How many lines matched.
    pub matches: usize,
    /// Up to three different lines that matched (already redacted).
    pub evidence: Vec<String>,
    /// What the app confirmed by itself, one short sentence each.
    pub checked: Vec<String>,
    pub remedy: Option<Remedy>,
}

/// The outcome for one bottle.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnosis {
    /// The bottle the findings belong to.
    pub bottle: PathBuf,
    /// Most certain first.
    pub findings: Vec<Finding>,
    /// How many lines were read.
    pub lines: usize,
    /// Which sources had lines.
    pub sources: Vec<Source>,
    /// The ticket [`Shown::begin`] gave the job that made this result; a result whose ticket is
    /// no longer the latest one for its bottle is dropped. Zero until a job sets it.
    pub ticket: u64,
}

impl Diagnosis {
    /// "Wine, la app y Steam": the sources that were read, for the screen and the report.
    pub fn sources_label(&self) -> String {
        let names: Vec<&str> = self.sources.iter().map(|s| s.label()).collect();
        match names.split_last() {
            None => String::new(),
            Some((last, [])) => last.to_string(),
            Some((last, rest)) => format!("{} y {}", rest.join(", "), last),
        }
    }

    /// The short sentence for the status line.
    pub fn status_line(&self) -> String {
        match self.findings.first() {
            Some(f) => format!("Diagnóstico: {} — {}", f.title, f.suggestion),
            None if self.lines == 0 => {
                "Diagnóstico: todavía no hay mensajes de esta botella.".into()
            }
            None => format!(
                "Diagnóstico: no se reconoció ningún problema conocido en {} líneas.",
                self.lines
            ),
        }
    }
}
