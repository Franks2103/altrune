//! The programs inside a bottle: Start Menu shortcuts, their icons and path conversion.

mod face;
mod icons;
mod links;
mod names;
mod walk;

#[cfg(test)]
mod tests {
    mod scan;
}

pub use icons::*;
pub use links::*;
pub use names::*;
use walk::*;

use super::shell_link::{self, ShellLink};
use super::*;
use std::fs;
use std::path::{Path, PathBuf};

/// Why a program is in the list but not among the ones shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Listing {
    /// A program a person installed: shown as a tile.
    #[default]
    Shown,
    /// An uninstaller, a manual, a web link...: kept, but tucked away in "Ocultos".
    Noise,
    /// Taken off the list by the person (`hidden_files`, or the older `hidden` names, of config.json).
    Removed,
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub name: String,
    /// Unix path of a .lnk or .exe
    pub path: PathBuf,
    pub args: String,
    pub env: Vec<String>,
    pub saved: bool,
    /// The program's own picture (PNG), when one could be found.
    pub icon: Option<PathBuf>,
    /// What the shortcut runs, as a Unix path inside the bottle, when the shortcut says it and
    /// the file exists. For a program picked by hand this is the .exe itself.
    pub target: Option<PathBuf>,
    /// The arguments written inside the shortcut (not the options a person chose to run with).
    pub link_args: String,
    /// The Start Menu folder the shortcut lives in ("Juegos › Aventuras"), empty at the top.
    pub folder: String,
    /// Shown under the name when two different programs are called the same.
    pub subtitle: String,
    pub listing: Listing,
}

impl Program {
    pub fn is_shown(&self) -> bool {
        self.listing == Listing::Shown
    }
}

impl Bottle {
    /// The programs to show as tiles, with their place in `programs` (actions use that place).
    pub fn shown_programs(&self) -> impl Iterator<Item = (usize, &Program)> {
        self.programs
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_shown())
    }

    pub fn shown_count(&self) -> usize {
        self.programs.iter().filter(|p| p.is_shown()).count()
    }

    /// The programs kept out of the main list (noise and the ones a person removed).
    pub fn tucked_programs(&self) -> impl Iterator<Item = (usize, &Program)> {
        self.programs
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.is_shown())
    }
}

/// What stands for a program's file in config.json: its path inside the bottle ("drive_c/..."),
/// so the choice follows the bottle when its folder is renamed, copied or imported. A file kept
/// outside the bottle has no such path and is written whole.
pub fn file_identity(prefix: &Path, file: &Path) -> String {
    file.strip_prefix(prefix)
        .unwrap_or(file)
        .to_string_lossy()
        .to_string()
}

/// What a program added by hand runs: its target and shortcut arguments, and the identity those
/// make. A file that is not an `.exe` or a readable shortcut runs something unknown (no identity).
fn saved_identity(
    prefix: &Path,
    p: &crate::backend::SavedProgram,
) -> (Option<PathBuf>, String, Option<String>) {
    let path = PathBuf::from(&p.path);
    let is = |ext: &str| {
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    };
    let (target, link_args) = if is("exe") {
        (Some(path.clone()), String::new())
    } else if is("lnk") {
        match read_link(&path) {
            Some(l) => (link_target(prefix, &path, &l), l.arguments),
            None => (None, String::new()),
        }
    } else {
        (None, String::new())
    };
    let key = target.as_ref().map(|t| identity_of_target(t, &link_args));
    (target, link_args, key)
}

/// The other files saved by hand that `scan_programs` folds into this one: the same name running
/// the same thing, such as a shortcut saved with options and the `.exe` it opens. Removing the
/// listed program has to forget them too, or the next scan would list the hidden twin.
pub fn saved_twins(prefix: &Path, settings: &BottleSettings, file: &Path) -> Vec<String> {
    let file = file.to_string_lossy();
    let Some(this) = settings
        .programs
        .iter()
        .find(|p| p.path.eq_ignore_ascii_case(&file))
    else {
        return vec![];
    };
    let Some(key) = saved_identity(prefix, this).2 else {
        return vec![];
    };
    let folded = fold_name(&this.name);
    settings
        .programs
        .iter()
        .filter(|p| {
            !p.path.eq_ignore_ascii_case(&file)
                && fold_name(&p.name) == folded
                && saved_identity(prefix, p).2.as_ref() == Some(&key)
        })
        .map(|p| p.path.clone())
        .collect()
}

/// A program on its way to the list, with what is needed to compare it with the others.
struct Entry {
    program: Program,
    folded: String,
    /// Where to say it came from when two programs are called the same.
    label: String,
}

pub fn scan_programs(prefix: &Path, settings: Option<&BottleSettings>) -> Vec<Program> {
    use std::collections::{HashMap, HashSet};
    let mut entries: Vec<Entry> = vec![];
    // The identities already listed: the shortcut or file itself, what it runs, its name.
    let mut paths: HashSet<PathBuf> = HashSet::new();
    let mut targets: HashSet<String> = HashSet::new();
    // Name and folder of the shortcuts listed, apart by whether what they run is known.
    let mut known_spots: HashSet<(String, String)> = HashSet::new();
    let mut unknown_spots: HashSet<(String, String)> = HashSet::new();
    let mut saved_files: HashSet<String> = HashSet::new();
    // What each name saved by hand runs. A name can be saved for several files.
    let mut saved_names: HashMap<String, Vec<Option<String>>> = HashMap::new();
    if let Some(s) = settings {
        for p in &s.programs {
            let folded = fold_name(&p.name);
            let path = PathBuf::from(&p.path);
            // A program added by hand is its file: the same file twice is one program.
            if !saved_files.insert(p.path.to_lowercase()) {
                continue;
            }
            let (target, link_args, key) = saved_identity(prefix, p);
            // The same name running the same thing is the same program, whatever file opens it.
            if key.is_some() && saved_names.get(&folded).is_some_and(|k| k.contains(&key)) {
                continue;
            }
            paths.insert(path.clone());
            if let Some(k) = &key {
                targets.insert(k.clone());
            }
            saved_names.entry(folded.clone()).or_default().push(key);
            entries.push(Entry {
                program: Program {
                    name: p.name.clone(),
                    path,
                    args: p.args.clone(),
                    env: p.env.clone(),
                    saved: true,
                    target,
                    link_args,
                    ..Program::default()
                },
                folded,
                label: "Guardado".into(),
            });
        }
    }
    let cached = cached_icons(prefix);
    let mut links = vec![];
    for (d, source) in start_menu_dirs(prefix) {
        let shared = source == Source::Desktop && points_outside(prefix, &d);
        if source == Source::StartMenu && points_outside(prefix, &d) {
            continue;
        }
        collect_links(&d, source, "", shared, &mut links, 0);
    }
    // Start Menu first, then desktops; `.lnk` before `.url` of the same name.
    links.sort_by_key(|f| {
        (
            f.source == Source::Desktop,
            f.path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase() == "url")
                .unwrap_or(false),
            f.path.to_string_lossy().to_string(),
        )
    });
    let steam_here = steam_exe(prefix).is_some();
    for f in links {
        let name = f
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.is_empty() || paths.contains(&f.path) {
            continue;
        }
        let is_url = f
            .path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("url"))
            .unwrap_or(false);
        let folded = fold_name(&name);
        let mut noise = false;
        let mut target: Option<PathBuf> = None;
        let mut link_args = String::new();
        // What a `.url` file opens is its identity, since it runs no file of the bottle.
        let mut url_key: Option<String> = None;
        let mut icon = cached.get(&name.to_lowercase()).cloned();
        if is_url {
            let address = url_address(&f.path).unwrap_or_default();
            if !address.is_empty() {
                url_key = Some(format!("url|{}", address.to_lowercase()));
            }
            let steam = address.to_lowercase().starts_with("steam://");
            if f.source == Source::Desktop {
                // A desktop only holds Steam games; a desktop that is the Mac's own folder
                // shows them only to a bottle that has Steam.
                if !steam || (f.shared_desktop && !steam_here) {
                    continue;
                }
            } else if is_web_address(&address) {
                noise = true;
            }
            if icon.is_none() {
                icon = url_icon(prefix, &f.path);
            }
        } else {
            // Shortcuts on a desktop that is a Mac folder could belong to any bottle.
            if f.shared_desktop {
                continue;
            }
            let link = read_link(&f.path);
            let target_win = link.as_ref().and_then(|l| l.target.clone());
            let args = link
                .as_ref()
                .map(|l| l.arguments.clone())
                .unwrap_or_default();
            noise = is_noise(&name, target_win.as_deref().map(|t| (t, args.as_str())));
            if let Some(l) = &link {
                target = link_target(prefix, &f.path, l);
                if icon.is_none() {
                    icon = link_icon(prefix, l);
                }
            }
            link_args = args;
        }
        // The same program under two shortcuts: the same target, else the same name in the
        // same folder. A program saved under this name that runs something else is another one.
        // An address stands for the program like a target does, but a shortcut of the same name
        // in the same folder that runs a file is taken to be the same program too.
        let from_url = url_key.is_some();
        let key = url_key.or_else(|| target.as_ref().map(|t| identity_of_target(t, &link_args)));
        let spot = (folded.clone(), fold_name(&f.folder));
        let saved_same_name = saved_names.get(&folded).map(Vec::as_slice).unwrap_or(&[]);
        let duplicate = match &key {
            Some(k) => {
                targets.contains(k)
                    || unknown_spots.contains(&spot)
                    || (from_url && known_spots.contains(&spot))
                    || saved_same_name.iter().any(|saved| saved.is_none())
            }
            None => {
                !saved_same_name.is_empty()
                    || known_spots.contains(&spot)
                    || unknown_spots.contains(&spot)
            }
        };
        if duplicate {
            continue;
        }
        match key {
            Some(k) => {
                targets.insert(k);
                known_spots.insert(spot);
            }
            None => {
                unknown_spots.insert(spot);
            }
        }
        paths.insert(f.path.clone());
        // A program is removed by the file of the shortcut that is listed. Two shortcuts that are
        // the same program are listed once, so if the listed one later disappears the other one
        // is listed instead and is not marked: removing is tied to one file by design.
        // Removed by its own file, or by a name an older version wrote (every program with that
        // name, as it always meant).
        let removed = settings.is_some_and(|s| {
            s.hides_file(prefix, &f.path) || s.hidden.iter().any(|h| fold_name(h) == folded)
        });
        let label = match (f.source, f.folder.as_str()) {
            (_, folder) if !folder.is_empty() => folder.to_string(),
            (Source::Desktop, _) => "Escritorio".into(),
            _ => "Menú Inicio".into(),
        };
        entries.push(Entry {
            program: Program {
                name,
                path: f.path,
                saved: false,
                icon,
                target,
                link_args,
                folder: f.folder,
                listing: if removed {
                    Listing::Removed
                } else if noise {
                    Listing::Noise
                } else {
                    Listing::Shown
                },
                ..Program::default()
            },
            folded,
            label,
        });
    }
    // Programs opened by hand: use the picture the bottle already has for that name, if any.
    for e in entries.iter_mut().filter(|e| e.program.icon.is_none()) {
        if let Some(icon) = cached.get(&e.program.name.to_lowercase()) {
            e.program.icon = Some(icon.clone());
        }
    }
    // Two programs with the same name tell themselves apart by where they come from.
    let mut same_name: HashMap<(bool, String), usize> = HashMap::new();
    for e in &entries {
        *same_name
            .entry((e.program.is_shown(), e.folded.clone()))
            .or_default() += 1;
    }
    // When the place is the same too (two programs saved by hand), the file itself tells them
    // apart.
    let mut same_place: HashMap<(bool, String, String), usize> = HashMap::new();
    for e in &entries {
        *same_place
            .entry((e.program.is_shown(), e.folded.clone(), e.label.clone()))
            .or_default() += 1;
    }
    for e in entries.iter_mut() {
        let group = (e.program.is_shown(), e.folded.clone());
        if same_name[&group] > 1 {
            let place = (group.0, group.1, e.label.clone());
            e.program.subtitle = if same_place[&place] > 1 {
                let id = file_identity(prefix, &e.program.path);
                let id = id.strip_prefix("drive_c/").unwrap_or(&id);
                format!("{} · {id}", e.label)
            } else {
                e.label.clone()
            };
        }
    }
    // Shown programs first, each group by name without accents.
    entries.sort_by(|a, b| {
        (
            !a.program.is_shown(),
            &a.folded,
            &a.program.folder,
            &a.program.path,
        )
            .cmp(&(
                !b.program.is_shown(),
                &b.folded,
                &b.program.folder,
                &b.program.path,
            ))
    });
    entries.into_iter().map(|e| e.program).collect()
}
