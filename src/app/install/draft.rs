//! The draft of the install dialog: what a file is, which bottle it is proposed for and what the
//! person may change before confirming. Nothing here starts a worker, and the draft itself is
//! plain data and logic; the one helper that looks at the disk is `suggested_bottle_name`, which
//! asks for a free bottle name. The flow that follows the confirmation is in `flow`.

use crate::domain::install::{
    recommend, BottleOption, Detected, FileKind, Programs, Recommendation, Target,
};
use crate::*;

/// A bottle name from an installer's file name: "SteamSetup.exe" → "Steam".
fn name_from_installer(file: &Path) -> String {
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut s = stem.clone();
    for w in [
        "installer",
        "install",
        "setup",
        "x86_64",
        "x64",
        "win64",
        "win32",
        "_64",
        "-64",
    ] {
        // ASCII-only lowercasing keeps every byte offset the same as in `s`; full Unicode
        // lowercasing can change a letter's length (the Turkish dotted capital I, say).
        let low = s.to_ascii_lowercase();
        if let Some(i) = low.find(w) {
            s.replace_range(i..i + w.len(), " ");
        }
    }
    let s: String = s
        .split(|c: char| c == '_' || c == '-' || c == '.' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let s = s
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == ' ')
        .trim()
        .to_string();
    let clean = sanitize_bottle_name(&s);
    if clean.is_empty() {
        "Mi Windows".into()
    } else {
        clean
    }
}

/// What the install dialog is asking: the file, what it is, and where it could go. The dialog
/// edits `choice`, `name` and `as_installer`; the rest is read once when it opens.
#[derive(Clone, Debug)]
pub(crate) struct InstallDraft {
    pub(crate) file: PathBuf,
    pub(crate) detected: Detected,
    /// The program the app has fixes for, when those fixes write something into the bottle
    /// first (when the file is its installer).
    pub(crate) fixes: Option<String>,
    /// The id of that program's recipe, so that a report from the worker that wrote it can be
    /// told from the report of another one.
    pub(crate) recipe_id: Option<String>,
    /// Every bottle there is, to choose from.
    pub(crate) options: Vec<BottleOption>,
    pub(crate) recommendation: Recommendation,
    /// The bottle chosen, or `None` for a new one named `name`.
    pub(crate) choice: Option<PathBuf>,
    pub(crate) name: String,
    /// Look for new programs afterwards (an installer), or just open the file.
    pub(crate) as_installer: bool,
}

impl InstallDraft {
    /// What the choice stands for.
    pub(crate) fn target(&self) -> Target {
        match &self.choice {
            Some(path) => Target::Existing(path.clone()),
            None => Target::New(self.name.trim().to_string()),
        }
    }

    /// The choice is the one the app proposed.
    pub(crate) fn follows_recommendation(&self) -> bool {
        match (&self.recommendation.target, &self.choice) {
            (Target::Existing(a), Some(b)) => a == b,
            (Target::New(_), None) => true,
            _ => false,
        }
    }
}

/// The draft for a file, with the proposed bottle chosen. `page` is the bottle the request came
/// from, `suggested` the name a new bottle would get.
pub(super) fn make_draft(
    file: PathBuf,
    detected: Detected,
    known: Option<String>,
    fixes: Option<String>,
    options: Vec<BottleOption>,
    page: Option<&Path>,
    suggested: String,
) -> InstallDraft {
    let recommendation = recommend(&suggested, known.as_deref(), page, &options);
    let (choice, name) = match &recommendation.target {
        Target::Existing(path) => (Some(path.clone()), suggested),
        Target::New(name) => (None, name.clone()),
    };
    // A program with fixes of its own (the Rockstar launcher, say) is installed even when its
    // file name does not say so: its fixes must be written first.
    let as_installer =
        detected.installs() || (known.is_some() && matches!(detected.kind, FileKind::Exe { .. }));
    InstallDraft {
        file,
        detected,
        fixes,
        recipe_id: None,
        options,
        recommendation,
        choice,
        name,
        as_installer,
    }
}

/// This very file is already saved by hand: saving it again would replace that entry, with its
/// options. Another file with the same name is another program and does not count.
pub(super) fn in_list(programs: &[Program], file: &Path) -> bool {
    programs
        .iter()
        .any(|p| p.saved && p.path.as_os_str().eq_ignore_ascii_case(file.as_os_str()))
}

/// The programs a bottle shows as tiles, by identity and name.
pub(super) fn shown_programs(programs: &[Program]) -> Programs {
    programs
        .iter()
        .filter(|p| p.is_shown())
        .map(|p| (p.path.to_string_lossy().to_string(), p.name.clone()))
        .collect()
}

/// A name for the bottle a file would make: its own name, free on the disk.
pub(super) fn suggested_bottle_name(file: &Path) -> String {
    unique_bottle_name(&name_from_installer(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::install::{classify_bytes, FileKind};

    #[test]
    fn bottle_name_from_installer() {
        assert_eq!(name_from_installer(Path::new("/x/SteamSetup.exe")), "Steam");
        assert_eq!(
            name_from_installer(Path::new("/x/Rockstar-Games-Launcher.exe")),
            "Rockstar Games Launcher"
        );
        assert_eq!(name_from_installer(Path::new("/x/setup.exe")), "Mi Windows");
        assert_eq!(
            name_from_installer(Path::new("/x/Notepad++_Installer_x64.exe")),
            "Notepad++"
        );
    }

    /// A letter whose lowercase is longer (İ, the Kelvin sign) must not shift the cut.
    #[test]
    fn bottle_name_survives_letters_that_change_length() {
        assert_eq!(
            name_from_installer(Path::new("/x/İndir_Setup.exe")),
            "İndir"
        );
        // U+212A, the Kelvin sign: three bytes, and one byte when lowercased.
        assert_eq!(
            name_from_installer(Path::new("/x/\u{212A}Setup.exe")),
            "\u{212A}"
        );
        assert_eq!(
            name_from_installer(Path::new("/x/İstanbulSetup.exe")),
            "İstanbul"
        );
    }

    /// The name typed in the dialog is trimmed where the bottle is made, as it is where it is
    /// checked.
    #[test]
    fn a_new_bottle_name_is_trimmed() {
        let installer = classify_bytes("NotepadSetup.exe", &exe_bytes());
        let mut d = make_draft(
            PathBuf::from("/d/NotepadSetup.exe"),
            installer,
            None,
            None,
            vec![],
            None,
            "Notepad".into(),
        );
        d.name = "  Juegos ".into();
        assert!(matches!(d.target(), Target::New(n) if n == "Juegos"));
    }

    fn exe_bytes() -> Vec<u8> {
        let mut b = vec![0u8; 0x100];
        b[..2].copy_from_slice(b"MZ");
        b
    }

    fn option(name: &str, prepared: bool, is_default: bool) -> BottleOption {
        BottleOption {
            path: PathBuf::from(format!("/b/{name}")),
            name: name.into(),
            prepared,
            is_default,
        }
    }

    /// The dialog opens with the proposed bottle chosen, and says so.
    #[test]
    fn the_draft_starts_on_the_proposed_bottle() {
        let installer = classify_bytes("NotepadSetup.exe", &exe_bytes());
        let options = vec![option("Oficina", false, true)];
        let d = make_draft(
            PathBuf::from("/d/NotepadSetup.exe"),
            installer.clone(),
            None,
            None,
            options.clone(),
            None,
            "Notepad".into(),
        );
        assert_eq!(d.choice, Some(PathBuf::from("/b/Oficina")));
        assert!(d.follows_recommendation() && d.as_installer);

        // Coming from a bottle's page proposes that bottle; from a page that is no bottle, not.
        let mut options = options;
        options.push(option("Juegos", false, false));
        let d = make_draft(
            PathBuf::from("/d/NotepadSetup.exe"),
            installer.clone(),
            None,
            None,
            options.clone(),
            Some(Path::new("/b/Juegos")),
            "Notepad".into(),
        );
        assert_eq!(d.choice, Some(PathBuf::from("/b/Juegos")));
        let d = make_draft(
            PathBuf::from("/d/NotepadSetup.exe"),
            installer.clone(),
            None,
            None,
            options,
            Some(Path::new("/b/Otra")),
            "Notepad".into(),
        );
        assert_eq!(d.choice, Some(PathBuf::from("/b/Oficina")));

        // No bottle yet: a new bottle with the suggested name.
        let d = make_draft(
            PathBuf::from("/d/NotepadSetup.exe"),
            installer,
            None,
            None,
            vec![],
            None,
            "Notepad".into(),
        );
        assert_eq!(d.choice, None);
        assert_eq!(d.target(), Target::New("Notepad".into()));
        assert!(d.follows_recommendation());
    }

    /// A program with its own fixes goes to a bottle of its own, and the person can still pick
    /// another one: the draft then no longer follows the proposal.
    #[test]
    fn a_known_program_proposes_its_own_bottle() {
        let installer = classify_bytes("SteamSetup.exe", &exe_bytes());
        let mut d = make_draft(
            PathBuf::from("/d/SteamSetup.exe"),
            installer,
            Some("Steam".into()),
            Some("Steam".into()),
            vec![option("Oficina", false, true)],
            None,
            "Steam".into(),
        );
        assert_eq!(d.target(), Target::New("Steam".into()));
        assert!(d.recommendation.reason.contains("Steam"));
        d.choice = Some(PathBuf::from("/b/Oficina"));
        assert!(!d.follows_recommendation());
        assert_eq!(d.target(), Target::Existing("/b/Oficina".into()));
    }

    /// A program that needs no installing is opened, not searched for; a plain .exe can be
    /// marked as an installer by the person.
    #[test]
    fn a_plain_program_is_opened_unless_marked_as_installer() {
        let program = classify_bytes("notepad++.exe", &exe_bytes());
        assert_eq!(
            program.kind,
            FileKind::Exe {
                installer_like: false
            }
        );
        let mut d = make_draft(
            PathBuf::from("/d/notepad++.exe"),
            program,
            None,
            None,
            vec![],
            None,
            "Notepad++".into(),
        );
        assert!(!d.as_installer);
        d.as_installer = true;
        assert!(d.detected.supported());
    }

    /// A program with fixes of its own is installed even when its name does not say «setup», so
    /// the fixes are written; a plain one with no fixes is not.
    #[test]
    fn a_program_with_fixes_is_installed_whatever_its_name() {
        let launcher = classify_bytes("Rockstar-Games-Launcher.exe", &exe_bytes());
        assert!(!launcher.installs());
        let make = |known: Option<String>, detected| {
            make_draft(
                PathBuf::from("/d/Rockstar-Games-Launcher.exe"),
                detected,
                known.clone(),
                known,
                vec![],
                None,
                "Rockstar".into(),
            )
        };
        assert!(make(Some("Rockstar".into()), launcher.clone()).as_installer);
        assert!(!make(None, launcher).as_installer);
        // A file that cannot be opened is never an installer, whatever it is called.
        let bad = classify_bytes("Rockstar-Games-Launcher.exe", b"nope");
        assert!(!make(Some("Rockstar".into()), bad).as_installer);
    }

    /// Opening a plain program does not replace the saved entry of that same file, but another
    /// file with the same name is another program.
    #[test]
    fn a_saved_program_is_not_replaced_by_opening_its_file() {
        let saved = |name: &str, saved| Program {
            name: name.into(),
            path: PathBuf::from(format!("/old/{name}.exe")),
            saved,
            ..Program::default()
        };
        let list = [saved("Game", true), saved("Menu", false)];
        assert!(in_list(&list, Path::new("/old/Game.exe")));
        assert!(in_list(&list, Path::new("/old/game.EXE")));
        // The same name in another folder is another program, so it is saved as well.
        assert!(!in_list(&list, Path::new("/new/game.EXE")));
        // A Start Menu entry is not replaced by saving, so it does not count.
        assert!(!in_list(&list, Path::new("/old/Menu.exe")));
        assert!(!in_list(&list, Path::new("/new/other.exe")));
    }

    /// Programs are counted by the file of their shortcut; tucked-away ones are not programs.
    #[test]
    fn only_shown_programs_count_as_installed() {
        use crate::backend::Listing;
        let program = |name: &str, listing| Program {
            name: name.into(),
            path: PathBuf::from(format!("/b/Menu/{name}.lnk")),
            listing,
            ..Program::default()
        };
        let list = [
            program("Steam", Listing::Shown),
            program("Desinstalar Steam", Listing::Noise),
            program("Viejo", Listing::Removed),
        ];
        let seen = shown_programs(&list);
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen.get("/b/Menu/Steam.lnk").map(String::as_str),
            Some("Steam")
        );
    }
}
