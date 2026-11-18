//! Finding the shortcut files in a bottle: the folders to look in and the walk through them.

use super::*;

/// Where a shortcut was found.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Source {
    StartMenu,
    Desktop,
}

pub(super) struct Found {
    pub(super) path: PathBuf,
    pub(super) source: Source,
    /// Start Menu folder, empty at the top and on the desktop.
    pub(super) folder: String,
    /// The desktop is a link to a folder outside the bottle (a Mac folder).
    pub(super) shared_desktop: bool,
}

/// A folder that is a link: where it points, when that is outside `prefix`.
pub(super) fn points_outside(prefix: &Path, dir: &Path) -> bool {
    let is_link = fs::symlink_metadata(dir)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    if !is_link {
        return false;
    }
    match (fs::canonicalize(dir), fs::canonicalize(prefix)) {
        (Ok(real), Ok(base)) => !real.starts_with(base),
        _ => true,
    }
}

/// The folders shortcuts live in: the common Start Menu, and for each real user its Start Menu
/// and its desktop. A user that is only a link (the bridge to another profile folder) is skipped:
/// the folder it points at is listed on its own.
pub(super) fn start_menu_dirs(prefix: &Path) -> Vec<(PathBuf, Source)> {
    let mut v = vec![(
        prefix.join("drive_c/ProgramData/Microsoft/Windows/Start Menu/Programs"),
        Source::StartMenu,
    )];
    if let Ok(users) = fs::read_dir(prefix.join("drive_c/users")) {
        let mut users: Vec<_> = users
            .flatten()
            .filter(|u| u.file_type().map(|t| !t.is_symlink()).unwrap_or(false))
            .map(|u| u.path())
            .collect();
        users.sort();
        for u in users {
            v.push((
                u.join("AppData/Roaming/Microsoft/Windows/Start Menu/Programs"),
                Source::StartMenu,
            ));
            v.push((u.join("Desktop"), Source::Desktop));
        }
    }
    v
}

/// The Windows shortcuts this app turns into launchers: `.lnk` and `.url` (Steam games are
/// `steam://rungameid/…` `.url` files on the desktop).
///
/// Links are never followed: Wine usually makes a user's Desktop (and Documents...) a link to
/// the Mac's own folders, and this scan must not wander in there. A desktop is read at its top
/// level only.
pub(super) fn collect_links(
    dir: &Path,
    source: Source,
    folder: &str,
    shared_desktop: bool,
    out: &mut Vec<Found>,
    depth: usize,
) {
    if depth > 4 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let Ok(kind) = e.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        let p = e.path();
        if kind.is_dir() {
            if source == Source::Desktop {
                continue;
            }
            let sub = e.file_name().to_string_lossy().to_string();
            let inner = if folder.is_empty() {
                sub
            } else {
                format!("{folder} › {sub}")
            };
            collect_links(&p, source, &inner, shared_desktop, out, depth + 1);
        } else if kind.is_file()
            && p.extension()
                .map(|x| x.eq_ignore_ascii_case("lnk") || x.eq_ignore_ascii_case("url"))
                .unwrap_or(false)
        {
            out.push(Found {
                path: p,
                source,
                folder: folder.to_string(),
                shared_desktop,
            });
        }
    }
}
