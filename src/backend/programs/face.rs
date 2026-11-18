//! The picture a bottle wears: the one of the program it was made for, picked from what the
//! scan already found (no disk is read here).

use super::*;

/// The name of a bottle without what making a copy of it added: «Steam (copia)», «Steam (copia 2)»
/// and «Steam-2» (the number `unique_bottle_name` appends) are all «Steam». Folded for comparing.
fn name_without_copy_marks(name: &str) -> String {
    let mut s = fold_name(name.trim());
    loop {
        let before = s.len();
        let t = s.trim_end();
        let t = match t.strip_suffix(')') {
            Some(inner) => match inner.rsplit_once('(') {
                Some((head, mark)) if is_copy_mark(mark) => head,
                _ => t,
            },
            None => match t.rsplit_once('-') {
                Some((head, n)) if !head.trim().is_empty() && is_number(n) => head,
                _ => t,
            },
        };
        s = t.trim_end().to_string();
        if s.len() == before {
            return s;
        }
    }
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

/// «copia» or «copia 2» (already folded), what sits between the brackets.
fn is_copy_mark(mark: &str) -> bool {
    match mark.trim().strip_prefix("copia") {
        Some("") => true,
        Some(rest) => rest.starts_with(' ') && is_number(rest.trim()),
        None => false,
    }
}

/// The picture a bottle named `name` shows, among its `programs`: the picture of the shown
/// program that has the bottle's name (copy marks, case and accents aside); otherwise, when
/// exactly one shown program has a picture, that one; otherwise none, and the bottle keeps its
/// letter (a bottle with several unrelated games has no face of its own). Programs taken off
/// the list and programs without a picture never count.
fn bottle_face<'a>(name: &str, programs: &'a [Program]) -> Option<&'a Path> {
    let pictured: Vec<(&Program, &Path)> = programs
        .iter()
        .filter(|p| p.is_shown())
        .filter_map(|p| p.icon.as_deref().map(|i| (p, i)))
        .collect();
    let plain = fold_name(name.trim());
    let bare = name_without_copy_marks(name);
    let named = pictured.iter().find(|(p, _)| {
        let folded = fold_name(p.name.trim());
        !folded.is_empty() && (folded == bare || folded == plain)
    });
    match (named, pictured.as_slice()) {
        (Some((_, icon)), _) => Some(icon),
        (None, [(_, only)]) => Some(only),
        _ => None,
    }
}

impl Bottle {
    /// The picture of this bottle's program, when it has one (see [`bottle_face`]).
    pub fn face(&self) -> Option<&Path> {
        bottle_face(&self.name, &self.programs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(name: &str, icon: Option<&str>, listing: Listing) -> Program {
        Program {
            name: name.into(),
            icon: icon.map(PathBuf::from),
            listing,
            ..Program::default()
        }
    }

    fn shown(name: &str, icon: &str) -> Program {
        program(name, Some(icon), Listing::Shown)
    }

    fn face(name: &str, programs: &[Program]) -> Option<String> {
        bottle_face(name, programs).map(|p| p.to_string_lossy().to_string())
    }

    #[test]
    fn a_program_named_like_the_bottle_gives_its_picture_among_several() {
        let programs = [shown("Otro", "o.png"), shown("Steam", "s.png")];
        assert_eq!(face("Steam", &programs).as_deref(), Some("s.png"));
    }

    #[test]
    fn copy_marks_of_the_bottle_name_are_ignored() {
        let programs = [shown("Otro", "o.png"), shown("Steam", "s.png")];
        for name in [
            "Steam (copia)",
            "Steam (copia 2)",
            "Steam-2",
            "Steam (copia)-3",
            "Steam (copia 2) (copia)",
            "Steam  (Copia)",
        ] {
            assert_eq!(face(name, &programs).as_deref(), Some("s.png"), "{name}");
        }
        let launcher = [
            shown("Otro", "o.png"),
            shown("Rockstar Games Launcher", "r.png"),
        ];
        assert_eq!(
            face("Rockstar Games Launcher (copia)", &launcher).as_deref(),
            Some("r.png")
        );
    }

    #[test]
    fn case_and_accents_do_not_matter() {
        let programs = [shown("Otro", "o.png"), shown("Cámara Ñandú", "c.png")];
        assert_eq!(face("CAMARA NANDU", &programs).as_deref(), Some("c.png"));
        assert_eq!(
            face("camara ñandu (copia)", &programs).as_deref(),
            Some("c.png")
        );
    }

    #[test]
    fn a_name_that_really_ends_in_a_number_still_matches_itself() {
        let programs = [shown("Otro", "o.png"), shown("Fallout-4", "f.png")];
        assert_eq!(face("Fallout-4", &programs).as_deref(), Some("f.png"));
    }

    #[test]
    fn a_single_pictured_program_is_the_face_whatever_the_names() {
        let programs = [
            shown("Juego", "j.png"),
            program("Sin imagen", None, Listing::Shown),
        ];
        assert_eq!(face("Mi botella", &programs).as_deref(), Some("j.png"));
    }

    #[test]
    fn several_unrelated_programs_leave_the_letter() {
        let programs = [shown("Uno", "1.png"), shown("Dos", "2.png")];
        assert_eq!(face("Mi botella", &programs), None);
        assert_eq!(face("Mi botella", &[]), None);
    }

    #[test]
    fn a_removed_or_tucked_away_program_is_never_the_face() {
        let removed = [program("Steam", Some("s.png"), Listing::Removed)];
        assert_eq!(face("Steam", &removed), None);
        let noise = [program("Steam", Some("s.png"), Listing::Noise)];
        assert_eq!(face("Steam", &noise), None);
        // the name match is among the shown ones only: the removed twin does not win
        let both = [
            program("Steam", Some("old.png"), Listing::Removed),
            shown("Steam", "new.png"),
        ];
        assert_eq!(face("Steam", &both).as_deref(), Some("new.png"));
        // a removed program does not count as «the only one» either
        let one_of_two = [
            program("Uno", Some("1.png"), Listing::Removed),
            shown("Dos", "2.png"),
        ];
        assert_eq!(face("Mi botella", &one_of_two).as_deref(), Some("2.png"));
    }

    #[test]
    fn a_program_without_a_picture_gives_nothing() {
        let programs = [program("Steam", None, Listing::Shown)];
        assert_eq!(face("Steam", &programs), None);
        // the name match needs a picture; the others decide then
        let programs = [
            program("Steam", None, Listing::Shown),
            shown("Otro", "o.png"),
        ];
        assert_eq!(face("Steam", &programs).as_deref(), Some("o.png"));
    }

    #[test]
    fn a_named_match_beats_the_single_picture_rule_in_a_bottle() {
        let bottle = Bottle {
            name: "Steam".into(),
            path: PathBuf::from("/bottles/Steam"),
            windows: "win10".into(),
            msync: false,
            high_res: false,
            logpixels: 96,
            programs: vec![shown("Steam", "s.png"), shown("Juego", "j.png")],
            env: vec![],
            prefix_complete: true,
        };
        assert_eq!(bottle.face(), Some(Path::new("s.png")));
    }

    #[test]
    fn a_name_of_only_marks_does_not_match_an_empty_program_name() {
        let programs = [shown("", "e.png"), shown("Otro", "o.png")];
        assert_eq!(face("(copia)", &programs), None);
    }
}
