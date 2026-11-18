//! Telling the person what happened: information that fades, and errors that stay.
//!
//! Information and success lines go to the status bar and leave after a few seconds. An
//! error is kept in `App::errors` until the person closes it or something new works in the
//! same bottle, and the page of that bottle shows it as a banner.

use crate::backend::*;
use crate::domain::status::{BottleStatus, StatusFacts};
use crate::*;

/// How a message to the person is meant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoticeKind {
    /// What the app is doing or just did: shown in the status bar, then gone.
    Info,
    /// Something finished well: like `Info`, in a firmer colour.
    Success,
    /// Something failed: kept in a banner until the person closes it.
    Error,
}

/// A message to the person: its words, how it is meant, and the bottle it concerns. An error
/// of a bottle is a banner at the top of that bottle's page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Notice {
    pub(crate) text: String,
    pub(crate) kind: NoticeKind,
    pub(crate) bottle: Option<PathBuf>,
}

impl Notice {
    pub(crate) fn new(kind: NoticeKind, bottle: Option<&Path>, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind,
            bottle: bottle.map(Path::to_path_buf),
        }
    }
}

/// How long an information or success line stays in the status bar.
pub(crate) const STATUS_SECS: f64 = 9.0;
/// How long the end of that time takes to fade out.
const STATUS_FADE_SECS: f64 = 1.5;

/// How visible the status line is `age` seconds after it was set: 1 while it is read, falling
/// to 0 as it leaves. An error never leaves by itself, and nothing leaves while the app works
/// on something of its own (the line says what it is doing).
pub(crate) fn status_visibility(kind: NoticeKind, age: f64, app_working: bool) -> f32 {
    if kind == NoticeKind::Error || app_working || age < STATUS_SECS - STATUS_FADE_SECS {
        return 1.0;
    }
    (((STATUS_SECS - age) / STATUS_FADE_SECS).clamp(0.0, 1.0)) as f32
}

/// Adds an error. A bottle keeps only its latest error, and so do the errors that belong to
/// no bottle: a newer one replaces the older.
pub(crate) fn put_error(list: &mut Vec<Notice>, notice: Notice) {
    list.retain(|e| e.bottle != notice.bottle);
    list.push(notice);
}

/// Removes the error of a bottle (or the one that belongs to none).
pub(crate) fn drop_error(list: &mut Vec<Notice>, bottle: Option<&Path>) {
    list.retain(|e| e.bottle.as_deref() != bottle);
}

/// The words for a settings file that could not be read.
pub(crate) fn unreadable_settings_text(saved_as: Option<&Path>) -> String {
    match saved_as {
        Some(p) => format!(
            "No pude leer tu archivo de ajustes y empecé con los de fábrica. Guardé una copia del archivo en «{}».",
            p.display()
        ),
        None => "No pude leer tu archivo de ajustes y empecé con los de fábrica. Tampoco pude guardar una copia del archivo.".into(),
    }
}

impl App {
    /// A line of information for the status bar.
    pub(crate) fn say(&mut self, text: impl Into<String>) {
        self.notify(Notice::new(NoticeKind::Info, None, text));
    }

    /// A line for the status bar about something that finished well.
    pub(crate) fn done(&mut self, text: impl Into<String>) {
        self.notify(Notice::new(NoticeKind::Success, None, text));
    }

    /// An error. With a bottle that is in the list it becomes a banner on that bottle's page
    /// (the status bar only points to it); without one the banner is on the overview.
    pub(crate) fn fail(&mut self, bottle: Option<&Path>, text: impl Into<String>) {
        self.notify(Notice::new(NoticeKind::Error, bottle, text));
    }

    pub(crate) fn notify(&mut self, notice: Notice) {
        match notice.kind {
            NoticeKind::Info | NoticeKind::Success => {
                self.status_kind = (notice.text.clone(), notice.kind);
                self.status = notice.text;
            }
            NoticeKind::Error => {
                let shown = notice
                    .bottle
                    .as_deref()
                    .and_then(|p| self.bottle_at(p))
                    .map(|b| format!("Hubo un problema en «{}»: míralo en su página.", b.name))
                    .unwrap_or_else(|| notice.text.clone());
                self.status_kind = (shown.clone(), NoticeKind::Error);
                self.status = shown;
                put_error(&mut self.errors, notice);
            }
        }
        // The same words twice are a new message: its time starts again.
        self.status_shown.clear();
    }

    /// The kind of the status line. A line written straight into `status` is information.
    pub(crate) fn status_kind(&self) -> NoticeKind {
        if self.status_kind.0 == self.status {
            self.status_kind.1
        } else {
            NoticeKind::Info
        }
    }

    /// The person closed an error banner.
    pub(crate) fn dismiss_error(&mut self, bottle: Option<&Path>) {
        drop_error(&mut self.errors, bottle);
        // The red line in the bar pointed to this banner; it goes with it.
        if self.status_kind() == NoticeKind::Error {
            self.status.clear();
            self.status_kind.0.clear();
        }
    }

    /// Something worked in this bottle: an old error of it no longer describes it.
    pub(crate) fn clear_errors(&mut self, bottle: &Path) {
        drop_error(&mut self.errors, Some(bottle));
    }

    /// The errors a page shows: those of this bottle, or (with `None`) the overview's: those
    /// that belong to no bottle and those of every bottle in the list too. A bottle's error is
    /// shown on its own page, but a failure nobody opened that page for must not wait unseen.
    pub(crate) fn errors_for<'a>(
        &'a self,
        bottle: Option<&'a Path>,
    ) -> impl Iterator<Item = &'a Notice> {
        self.errors
            .iter()
            .filter(move |e| match (bottle, &e.bottle) {
                (Some(p), Some(b)) => b == p,
                (None, _) => true,
                (Some(_), None) => false,
            })
    }

    /// The place in the list of the bottle an error belongs to, when that bottle is listed:
    /// where the banner's «Ver» goes.
    pub(crate) fn error_bottle_index(&self, e: &Notice) -> Option<usize> {
        let path = e.bottle.as_deref()?;
        self.bottles.iter().position(|b| b.path == path)
    }

    /// The title and the body of an error banner. Away from the bottle's page (`listed` is the
    /// bottle's place in the list) the banner names the bottle, since its words alone do not.
    pub(crate) fn error_words<'a>(
        &self,
        e: &'a Notice,
        listed: Option<usize>,
    ) -> (String, &'a str) {
        match listed.and_then(|i| self.bottles.get(i)) {
            Some(b) => (format!("Algo falló en «{}»", b.name), e.text.as_str()),
            None => (e.text.clone(), ""),
        }
    }

    /// The settings file could not be read: said once, with where the copy of it is.
    pub(crate) fn tell_unreadable_settings(&mut self, saved_as: Option<&Path>) {
        self.fail(None, unreadable_settings_text(saved_as));
    }

    /// An error of the app's own install (Wine, an engine, an addition). A cancelled install is
    /// something the person asked for, so it is only said.
    pub(crate) fn setup_failed(&mut self, text: String) {
        if wine_setup::lock_progress(&self.setup).cancel {
            self.say(text);
        } else {
            self.fail(None, text);
        }
    }

    /// Programs being opened in a bottle, by name (the list's name when the file is one of
    /// them). Asked on every frame, so it reads only what is in memory.
    pub(crate) fn starting_in(&self, bottle: &Path) -> Vec<String> {
        self.opening
            .keys()
            .filter(|(b, _)| b == bottle)
            .map(|(_, file)| {
                self.bottle_at(bottle)
                    .and_then(|b| b.programs.iter().find(|p| &p.path == file))
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| {
                        file.file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default()
                    })
            })
            .collect()
    }

    /// Whether the engine chosen for the bottle is missing. Read from the engines noted at the
    /// last scan, so asking is free.
    pub(crate) fn engine_missing(&self, b: &Bottle) -> bool {
        match self
            .cfg
            .bottle_ref(&b.path)
            .and_then(|s| s.engine.as_deref())
        {
            Some("games") => self.engines.games.is_none(),
            Some("dxmt") => self.engines.metal.is_none(),
            _ => false,
        }
    }

    /// The one state of a bottle, from what the window already knows.
    pub(crate) fn bottle_status(&self, b: &Bottle) -> BottleStatus {
        let running = self.running_in(&b.path);
        let starting = self.starting_in(&b.path);
        let starting: Vec<&str> = starting.iter().map(String::as_str).collect();
        StatusFacts {
            busy: self.busy.get(&b.path).map(String::as_str),
            installing: self.install.as_ref().is_some_and(|s| s.shows_on(&b.path)),
            starting: &starting,
            running: &running,
            live: self.live.contains(&b.path),
            prefix_complete: b.prefix_complete,
            engine_missing: self.engine_missing(b),
            failed: self
                .errors
                .iter()
                .any(|e| e.bottle.as_deref() == Some(&b.path)),
        }
        .status()
    }

    /// Refuses, with a message, to start Wine in a bottle that never finished being made: there
    /// is no Windows in it to run anything, and starting Wine would quietly repair it, which the
    /// page promised not to do. True when it was refused.
    pub(crate) fn refuse_if_unfinished(&mut self, b: &Bottle) -> bool {
        match self.bottle_status(b).blocks_use() {
            Some(why) => {
                self.say(why);
                true
            }
            None => false,
        }
    }

    /// Bottles being made that the scan does not list yet: they show as a row of their own.
    pub(crate) fn creating_bottles(&self) -> Vec<&Path> {
        self.busy
            .iter()
            .filter(|(path, job)| {
                job.as_str() == crate::domain::status::CREATING && self.bottle_at(path).is_none()
            })
            .map(|(path, _)| path.as_path())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(bottle: Option<&str>, text: &str) -> Notice {
        Notice::new(NoticeKind::Error, bottle.map(Path::new), text)
    }

    /// A newer error of the same bottle replaces the older; another bottle's stays.
    #[test]
    fn a_bottle_keeps_only_its_latest_error() {
        let mut list = vec![];
        put_error(&mut list, error(Some("/b/a"), "uno"));
        put_error(&mut list, error(Some("/b/b"), "dos"));
        put_error(&mut list, error(Some("/b/a"), "tres"));
        put_error(&mut list, error(None, "cuatro"));
        put_error(&mut list, error(None, "cinco"));
        let texts: Vec<&str> = list.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["dos", "tres", "cinco"]);
    }

    /// Closing, or a new success in the bottle, removes that bottle's error and no other.
    #[test]
    fn closing_an_error_leaves_the_others() {
        let mut list = vec![];
        put_error(&mut list, error(Some("/b/a"), "uno"));
        put_error(&mut list, error(Some("/b/b"), "dos"));
        put_error(&mut list, error(None, "tres"));
        drop_error(&mut list, Some(Path::new("/b/a")));
        assert_eq!(list.len(), 2);
        drop_error(&mut list, None);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].text, "dos");
        // Closing what is not there changes nothing.
        drop_error(&mut list, Some(Path::new("/b/zzz")));
        assert_eq!(list.len(), 1);
    }

    /// Information leaves after a few seconds; errors and the app's own work do not.
    #[test]
    fn information_fades_and_errors_stay() {
        use NoticeKind::*;
        assert_eq!(status_visibility(Info, 0.0, false), 1.0);
        assert_eq!(status_visibility(Success, STATUS_SECS - 2.0, false), 1.0);
        let fading = status_visibility(Info, STATUS_SECS - 0.5, false);
        assert!(fading > 0.0 && fading < 1.0);
        assert_eq!(status_visibility(Info, STATUS_SECS, false), 0.0);
        assert_eq!(status_visibility(Info, 600.0, false), 0.0);
        assert_eq!(status_visibility(Error, 600.0, false), 1.0);
        assert_eq!(status_visibility(Info, 600.0, true), 1.0);
    }
}
