//! The one state of a bottle, worked out from facts the window already holds. The sidebar row,
//! the overview card and the bottle's page all read this, so they never disagree.
//!
//! Nothing here touches the disk or the window: the facts are filled in by the app (some of
//! them were recorded once when the bottle was loaded) and the answer is a plain value.

use super::step::Step;

/// The label a bottle's job carries while it is being made. A bottle with this job shows as
/// being created, also before its folder looks like a bottle.
pub const CREATING: &str = "Creando…";

/// Why a bottle needs a look before anything else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attention {
    /// The folder lacks the files of a finished Windows (`drive_c` or `system.reg`).
    Incomplete,
    /// The engine chosen for the bottle is not installed.
    EngineMissing,
    /// Something failed in this bottle and the person has not seen it yet.
    Failed,
}

impl Attention {
    /// A short word for the badge and the list.
    pub fn label(self) -> &'static str {
        match self {
            Attention::Incomplete => "Incompleta",
            Attention::EngineMissing => "Faltan los gráficos",
            Attention::Failed => "Con un error",
        }
    }

    /// What is wrong and what to do about it, for a tooltip.
    pub fn reason(self) -> &'static str {
        match self {
            Attention::Incomplete => "No terminó de crearse: elimínala y crea otra.",
            Attention::EngineMissing => {
                "Los gráficos elegidos no están instalados: elige otros o vuelve a «Automático»."
            }
            Attention::Failed => "Algo falló en esta botella: lo explica el aviso de su página.",
        }
    }
}

impl Attention {
    /// The heading of the banner a bottle's page shows for this state, or `None` when the page
    /// says it some other way (a failure has its own banner, with the error's words).
    pub fn banner_title(self) -> Option<&'static str> {
        match self {
            Attention::Incomplete => Some("Esta botella no terminó de crearse"),
            Attention::EngineMissing => Some("Faltan los gráficos de esta botella"),
            Attention::Failed => None,
        }
    }

    /// What the banner explains: what is wrong and what the person can do, in plain words.
    pub fn banner_body(self) -> &'static str {
        match self {
            Attention::Incomplete => {
                "Se interrumpió antes de terminar y no se puede usar. Elimínala y crea otra: tus otras botellas no se tocan."
            }
            Attention::EngineMissing => {
                "En «Gráficos» elegiste una opción que no está instalada («Steam y launchers» o «Juegos DirectX 12»). «Steam y launchers» se instala al elegirlo otra vez; «Juegos DirectX 12» no se instala desde Altrune (es software de Apple). También puedes volver a «Automático» en la pestaña Configuración."
            }
            Attention::Failed => "",
        }
    }
}

/// What a state means in colour. This is a meaning, not a colour: the window keeps the palette
/// and picks the colour of each (see `ui::theme::status_tone`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusTone {
    /// Work is going on.
    Accent,
    /// Programs are running.
    Success,
    /// The bottle needs a look.
    Warning,
    /// Something failed.
    Danger,
    /// Nothing to point out.
    Neutral,
}

/// What a state means as a small picture (see `ui::theme::status_icon` for the drawing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusSymbol {
    /// Work is going on.
    Working,
    /// Wine runs in the bottle.
    Running,
    /// The bottle needs a look.
    Alert,
    /// Everything that can be checked is fine.
    Ready,
}

/// What a bottle is, in one value. The variants are listed from the strongest claim to the
/// weakest: see [`StatusFacts::status`] for the order they are chosen in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BottleStatus {
    /// The bottle is being made.
    Creating,
    /// A job of the app is working on it (copying, closing, saving…), named by its label.
    Busy(String),
    /// A program is being installed in it: the installer is open or its work is being looked at.
    /// Other programs may still be opened in it.
    Installing,
    /// A program is being opened in it.
    Starting(String),
    /// Programs opened from here are running in it.
    Running(Vec<String>),
    /// Wine runs in it, but nothing was opened from this window.
    InUse,
    /// It cannot be used as it is.
    NeedsAttention(Attention),
    /// Everything that can be checked is fine.
    Ready,
}

impl BottleStatus {
    /// The state in words, short enough for a list row.
    pub fn label(&self) -> String {
        match self {
            BottleStatus::Creating => CREATING.to_string(),
            BottleStatus::Busy(job) => job.clone(),
            BottleStatus::Installing => "Instalando…".to_string(),
            BottleStatus::Starting(program) => format!("Abriendo {program}…"),
            BottleStatus::Running(programs) => format!("Abierto: {}", programs.join(", ")),
            BottleStatus::InUse => "En uso".to_string(),
            BottleStatus::NeedsAttention(why) => why.label().to_string(),
            BottleStatus::Ready => "Lista".to_string(),
        }
    }

    /// What the state means in colour: whether it is work, something in use, a warning, a
    /// failure or nothing to say. The window turns this into a colour of its palette.
    pub fn tone(&self) -> StatusTone {
        match self {
            BottleStatus::Creating
            | BottleStatus::Busy(_)
            | BottleStatus::Installing
            | BottleStatus::Starting(_) => StatusTone::Accent,
            BottleStatus::Running(_) | BottleStatus::InUse => StatusTone::Success,
            BottleStatus::NeedsAttention(Attention::Failed) => StatusTone::Danger,
            BottleStatus::NeedsAttention(_) => StatusTone::Warning,
            BottleStatus::Ready => StatusTone::Neutral,
        }
    }

    /// What the state means as a small picture, for places that draw one.
    pub fn symbol(&self) -> StatusSymbol {
        match self {
            BottleStatus::Creating
            | BottleStatus::Busy(_)
            | BottleStatus::Installing
            | BottleStatus::Starting(_) => StatusSymbol::Working,
            BottleStatus::Running(_) | BottleStatus::InUse => StatusSymbol::Running,
            BottleStatus::NeedsAttention(_) => StatusSymbol::Alert,
            BottleStatus::Ready => StatusSymbol::Ready,
        }
    }

    /// Something is being done right now: show a spinner instead of a still dot.
    pub fn is_working(&self) -> bool {
        matches!(
            self,
            BottleStatus::Creating
                | BottleStatus::Busy(_)
                | BottleStatus::Installing
                | BottleStatus::Starting(_)
        )
    }

    /// Wine runs in the bottle: its programs can be closed.
    pub fn is_in_use(&self) -> bool {
        matches!(self, BottleStatus::Running(_) | BottleStatus::InUse)
    }

    /// The reason to explain on the page, when the state needs attention and the page has no
    /// other place that says it.
    pub fn to_explain(&self) -> Option<Attention> {
        match self {
            BottleStatus::NeedsAttention(why) if why.banner_title().is_some() => Some(*why),
            _ => None,
        }
    }

    /// Why nothing can be installed or opened in the bottle, or `None` when it can be used. A
    /// bottle that never finished being made has no Windows to run anything in.
    pub fn blocks_use(&self) -> Option<&'static str> {
        matches!(self, BottleStatus::NeedsAttention(Attention::Incomplete))
            .then_some("Esta botella no terminó de crearse: elimínala y crea otra.")
    }

    /// A sentence for a tooltip when the label alone does not say enough.
    pub fn detail(&self) -> Option<&'static str> {
        match self {
            BottleStatus::NeedsAttention(why) => Some(why.reason()),
            BottleStatus::InUse => Some("Wine está funcionando en esta botella."),
            _ => None,
        }
    }
}

/// Why «Cancelar» does nothing at this step of an install, or `None` when it stops it. The
/// install looks for the cancel request only while it downloads; the other steps cannot be
/// interrupted, so the button says so instead of seeming to work.
pub fn install_cancel_reason(step: Option<Step>) -> Option<&'static str> {
    match step {
        Some(Step::Download) => None,
        Some(Step::Unpack | Step::Check) => Some(
            "Ya se descargó; solo falta terminar de instalar, y este paso no se puede cancelar.",
        ),
        _ => Some("Se podrá cancelar cuando empiece la descarga."),
    }
}

/// «Se leyó 1 línea» or «Se leyeron 12 líneas»: the verb and the noun agree with the number.
pub fn lines_read(n: usize) -> String {
    if n == 1 {
        "Se leyó 1 línea".to_string()
    } else {
        format!("Se leyeron {n} líneas")
    }
}

/// What the app knows about one bottle. Every field is already in memory: none is read from
/// the disk when the status is asked for.
#[derive(Clone, Copy, Debug, Default)]
pub struct StatusFacts<'a> {
    /// The label of the job the app is running on the bottle, if any.
    pub busy: Option<&'a str>,
    /// A program is being installed in it (see [`BottleStatus::Installing`]).
    pub installing: bool,
    /// Programs being opened in it.
    pub starting: &'a [&'a str],
    /// Programs opened from here that are open in it (also those that handed over).
    pub running: &'a [&'a str],
    /// Wine was found running in it at the last check.
    pub live: bool,
    /// Its folder has `drive_c` and `system.reg` (recorded when the bottle was loaded).
    pub prefix_complete: bool,
    /// The engine chosen for it is not installed.
    pub engine_missing: bool,
    /// A failure in it is waiting to be seen.
    pub failed: bool,
}

impl StatusFacts<'_> {
    /// Picks the state. What is happening beats what is true of the folder: a job in
    /// progress hides everything else, an installation beats the programs opened beside it,
    /// running programs beat a bare "Wine runs", and a bottle
    /// is "Lista" only when no check found a problem.
    pub fn status(&self) -> BottleStatus {
        if let Some(job) = self.busy {
            return if job == CREATING {
                BottleStatus::Creating
            } else {
                BottleStatus::Busy(job.to_string())
            };
        }
        if self.installing {
            return BottleStatus::Installing;
        }
        if let Some(program) = self.starting.first() {
            return BottleStatus::Starting((*program).to_string());
        }
        if !self.running.is_empty() {
            return BottleStatus::Running(self.running.iter().map(|s| s.to_string()).collect());
        }
        if self.live {
            return BottleStatus::InUse;
        }
        if !self.prefix_complete {
            return BottleStatus::NeedsAttention(Attention::Incomplete);
        }
        if self.engine_missing {
            return BottleStatus::NeedsAttention(Attention::EngineMissing);
        }
        if self.failed {
            return BottleStatus::NeedsAttention(Attention::Failed);
        }
        BottleStatus::Ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bottle with nothing wrong and nothing going on.
    fn calm() -> StatusFacts<'static> {
        StatusFacts {
            prefix_complete: true,
            ..Default::default()
        }
    }

    #[test]
    fn a_checked_bottle_is_ready() {
        assert_eq!(calm().status(), BottleStatus::Ready);
        assert_eq!(BottleStatus::Ready.label(), "Lista");
    }

    #[test]
    fn a_job_beats_everything_else() {
        let facts = StatusFacts {
            busy: Some("Copiando…"),
            starting: &["Steam"],
            running: &["Steam"],
            live: true,
            engine_missing: true,
            failed: true,
            ..calm()
        };
        assert_eq!(facts.status(), BottleStatus::Busy("Copiando…".into()));
        let facts = StatusFacts {
            busy: Some(CREATING),
            prefix_complete: false,
            ..calm()
        };
        assert_eq!(facts.status(), BottleStatus::Creating);
    }

    #[test]
    fn running_beats_in_use_and_in_use_beats_the_checks() {
        let facts = StatusFacts {
            running: &["Steam"],
            live: true,
            ..calm()
        };
        assert_eq!(facts.status(), BottleStatus::Running(vec!["Steam".into()]));
        let facts = StatusFacts {
            live: true,
            prefix_complete: false,
            engine_missing: true,
            ..calm()
        };
        assert_eq!(facts.status(), BottleStatus::InUse);
        // Opening a program is shown before the ones already open.
        let facts = StatusFacts {
            starting: &["Notepad"],
            running: &["Steam"],
            ..calm()
        };
        assert_eq!(facts.status(), BottleStatus::Starting("Notepad".into()));
    }

    /// An installation shows as such, below a job and above the programs beside it, and it
    /// does not make the bottle look closed to them: the close controls still see it in use.
    #[test]
    fn an_installation_shows_as_installing() {
        let facts = StatusFacts {
            installing: true,
            starting: &["Notepad"],
            running: &["Steam"],
            live: true,
            ..calm()
        };
        assert_eq!(facts.status(), BottleStatus::Installing);
        assert_eq!(BottleStatus::Installing.label(), "Instalando…");
        assert!(BottleStatus::Installing.is_working());
        assert_eq!(BottleStatus::Installing.tone(), StatusTone::Accent);
        // A job of the app on the bottle (closing, copying) still comes first.
        let facts = StatusFacts {
            installing: true,
            busy: Some("Cerrando…"),
            ..calm()
        };
        assert_eq!(facts.status(), BottleStatus::Busy("Cerrando…".into()));
    }

    #[test]
    fn a_failed_check_beats_ready() {
        let facts = StatusFacts {
            prefix_complete: false,
            ..calm()
        };
        assert_eq!(
            facts.status(),
            BottleStatus::NeedsAttention(Attention::Incomplete)
        );
        let facts = StatusFacts {
            engine_missing: true,
            ..calm()
        };
        assert_eq!(
            facts.status(),
            BottleStatus::NeedsAttention(Attention::EngineMissing)
        );
        let facts = StatusFacts {
            failed: true,
            ..calm()
        };
        assert_eq!(
            facts.status(),
            BottleStatus::NeedsAttention(Attention::Failed)
        );
        // An incomplete folder is reported before a missing engine.
        let facts = StatusFacts {
            prefix_complete: false,
            engine_missing: true,
            ..calm()
        };
        assert_eq!(
            facts.status(),
            BottleStatus::NeedsAttention(Attention::Incomplete)
        );
    }

    #[test]
    fn counts_agree_with_their_nouns() {
        assert_eq!(lines_read(1), "Se leyó 1 línea");
        assert_eq!(lines_read(0), "Se leyeron 0 líneas");
        assert_eq!(lines_read(12), "Se leyeron 12 líneas");
    }

    /// Only the download can be stopped; the other steps say why they cannot.
    #[test]
    fn an_install_can_be_cancelled_only_while_it_downloads() {
        assert_eq!(install_cancel_reason(Some(Step::Download)), None);
        for step in [
            None,
            Some(Step::Rosetta),
            Some(Step::Find),
            Some(Step::Unpack),
            Some(Step::Check),
        ] {
            assert!(install_cancel_reason(step).is_some());
        }
        // After the download the reason must not promise a later chance.
        let late = install_cancel_reason(Some(Step::Unpack)).unwrap();
        assert!(late.contains("Ya se descargó"));
        assert_eq!(install_cancel_reason(Some(Step::Check)), Some(late));
        assert!(!install_cancel_reason(Some(Step::Find))
            .unwrap()
            .contains("Ya se"));
    }

    /// A bottle whose chosen graphics are not installed says so with the words of the list.
    #[test]
    fn the_missing_engine_is_told_with_the_names_of_the_list() {
        use crate::domain::engines::{GAMES, LAUNCHERS};
        let why = Attention::EngineMissing;
        assert!(why.banner_body().contains(&format!("«{LAUNCHERS}»")));
        assert!(why.banner_body().contains(&format!("«{GAMES}»")));
        assert!(why.banner_body().contains("no se instala desde Altrune"));
        assert!(why.banner_body().contains("«Gráficos»"));
        for text in [why.label(), why.reason(), why.banner_title().unwrap()] {
            assert!(text.to_lowercase().contains("gráficos"), "{text}");
        }
        // The tooltip never sends to an install that the app does not offer.
        assert!(!why.reason().contains("instálalos"), "{}", why.reason());
    }

    #[test]
    fn words_tones_and_work_come_from_the_variant() {
        let running = BottleStatus::Running(vec!["Steam".into(), "Notepad".into()]);
        assert_eq!(running.label(), "Abierto: Steam, Notepad");
        assert!(running.is_in_use() && !running.is_working());
        assert_eq!(running.tone(), StatusTone::Success);
        assert_eq!(
            BottleStatus::Starting("Steam".into()).label(),
            "Abriendo Steam…"
        );
        assert!(BottleStatus::Starting("Steam".into()).is_working());
        assert!(BottleStatus::Creating.is_working());
        assert!(BottleStatus::Busy("Cerrando…".into()).is_working());
        assert!(!BottleStatus::InUse.is_working() && BottleStatus::InUse.is_in_use());
        let warn = BottleStatus::NeedsAttention(Attention::EngineMissing);
        assert_eq!(warn.tone(), StatusTone::Warning);
        assert_eq!(
            warn.detail(),
            Some("Los gráficos elegidos no están instalados: elige otros o vuelve a «Automático».")
        );
        assert_eq!(
            BottleStatus::NeedsAttention(Attention::Failed).tone(),
            StatusTone::Danger
        );
        assert_eq!(
            BottleStatus::NeedsAttention(Attention::Incomplete).detail(),
            Some("No terminó de crearse: elimínala y crea otra.")
        );
    }

    /// An incomplete bottle explains itself on its page and cannot be used; a missing engine is
    /// explained but the bottle still works; a failure has its own banner, so it adds none.
    #[test]
    fn states_that_need_attention_say_why_and_only_an_incomplete_bottle_is_blocked() {
        let incomplete = BottleStatus::NeedsAttention(Attention::Incomplete);
        assert_eq!(incomplete.to_explain(), Some(Attention::Incomplete));
        assert!(incomplete.blocks_use().is_some());
        let engine = BottleStatus::NeedsAttention(Attention::EngineMissing);
        assert_eq!(engine.to_explain(), Some(Attention::EngineMissing));
        assert!(engine.blocks_use().is_none());
        let failed = BottleStatus::NeedsAttention(Attention::Failed);
        assert!(failed.to_explain().is_none() && failed.blocks_use().is_none());
        for calm in [BottleStatus::Ready, BottleStatus::InUse] {
            assert!(calm.to_explain().is_none() && calm.blocks_use().is_none());
        }
        for why in [Attention::Incomplete, Attention::EngineMissing] {
            assert!(why.banner_title().is_some() && !why.banner_body().is_empty());
        }
    }
}
