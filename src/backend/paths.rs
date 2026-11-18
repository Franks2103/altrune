//! Where the app keeps its files and which folders Wine may be started in.

use std::path::{Component, Path, PathBuf};

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// The name the person knows the app by: the window title and the app name eframe gives the
/// system, and the name an item gets in the Trash when its own name is only dots. (The data
/// folder below, the launchers folder and the bundle identifiers keep the old name on purpose:
/// they are how existing bottles and launchers are found.) It lives here, beside the data folder
/// name it complements, because this layer cannot name the crate root; the crate root re-exports it.
pub const APP_NAME: &str = "Altrune";

pub fn data_dir() -> PathBuf {
    home().join("Library/Application Support/Botella Libre")
}

pub fn own_bottles_dir() -> PathBuf {
    data_dir().join("Bottles")
}

/// `p` with every link on the way followed, also for the part that does not exist yet: the
/// deepest folder that exists is resolved by the system and the rest is appended as it is.
fn resolved(p: &Path) -> PathBuf {
    let mut rest = vec![];
    let mut head = p;
    loop {
        if let Ok(real) = std::fs::canonicalize(head) {
            return rest.iter().rev().fold(real, |acc, part| acc.join(part));
        }
        match (head.parent(), head.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name);
                head = parent;
            }
            _ => return p.to_path_buf(),
        }
    }
}

/// True when `p` is inside `root`, with every link followed on both sides (a bottle that is a
/// link out of the folder is not inside it) and never through a `..`.
pub fn is_inside_folder(p: &Path, root: &Path) -> bool {
    !p.components().any(|c| c == Component::ParentDir) && resolved(p).starts_with(resolved(root))
}

/// Whether Wine may be started with `prefix` as its Windows folder: only inside Altrune's own
/// bottles folder. A prefix anywhere else belongs to something else, and a Wine of another
/// version must never write into it. This is the last line of defence: every command that
/// starts Wine asks it, whatever the window allowed.
pub fn is_own_prefix(prefix: &Path) -> bool {
    let root = own_bottles_dir();
    is_inside_folder(prefix, &root) && resolved(prefix) != resolved(&root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bl-paths-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn folder_membership_follows_paths_and_links() {
        let base = scratch("inside");
        let (root, outside) = (base.join("bottles"), base.join("mine"));
        fs::create_dir_all(root.join("Steam")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        assert!(is_inside_folder(&root.join("Steam"), &root));
        // A bottle still to be made is judged by where it will be.
        assert!(is_inside_folder(&root.join("not-there-yet/deeper"), &root));
        assert!(!is_inside_folder(&outside, &root));
        assert!(!is_inside_folder(&base, &root));
        // A folder that only starts with the same letters is another folder.
        assert!(!is_inside_folder(&base.join("bottles-2/x"), &root));
        // Climbing out with `..` does not count, even when the text starts inside.
        assert!(!is_inside_folder(&root.join("../mine"), &root));
        // A link from elsewhere into the root points inside; a link from the root to the outside
        // does not.
        let to_steam = outside.join("link-to-steam");
        std::os::unix::fs::symlink(root.join("Steam"), &to_steam).unwrap();
        assert!(is_inside_folder(&to_steam, &root));
        let out = root.join("link-out");
        std::os::unix::fs::symlink(&outside, &out).unwrap();
        assert!(!is_inside_folder(&out, &root));
        assert!(!is_inside_folder(&out.join("not-there-yet"), &root));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn only_the_apps_own_bottles_folder_is_a_prefix() {
        assert!(is_own_prefix(&own_bottles_dir().join("Steam")));
        assert!(is_own_prefix(&own_bottles_dir().join("Mi botella (copia)")));
        assert!(!is_own_prefix(&own_bottles_dir()));
        assert!(!is_own_prefix(
            &home().join("Library/Application Support/Otra app/Bottles/Steam")
        ));
        assert!(!is_own_prefix(&data_dir().join("Wine")));
        assert!(!is_own_prefix(&home().join(".wine")));
        assert!(!is_own_prefix(&own_bottles_dir().join("../Wine")));
    }
}
