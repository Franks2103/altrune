//! Reading shortcut files: Windows paths turned into bottle files and what a shortcut runs.

use super::*;

/// Turns `C:\a\b.ico` into the file inside the bottle.
pub fn windows_to_unix(prefix: &Path, win: &str) -> Option<PathBuf> {
    let w = win.trim().trim_matches('"');
    let (drive, rest) = w.split_once(':')?;
    let rest = rest.replace('\\', "/");
    // Climbing out of the drive with `..` would leave the bottle.
    if rest.split('/').any(|part| part == "..") {
        return None;
    }
    let base = if drive.eq_ignore_ascii_case("c") {
        prefix.join("drive_c")
    } else {
        prefix
            .join("dosdevices")
            .join(format!("{}:", drive.to_lowercase()))
    };
    let p = base.join(rest.trim_start_matches('/'));
    if p.exists() {
        return Some(p);
    }
    // Windows paths ignore case; walk the folders case-insensitively.
    let mut cur = base;
    for part in rest.split('/').filter(|s| !s.is_empty()) {
        let next = fs::read_dir(&cur)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|e| {
                e.file_name()
                    .map(|f| f.to_string_lossy().eq_ignore_ascii_case(part))
                    .unwrap_or(false)
            })?;
        cur = next;
    }
    Some(cur)
}

/// The address a `.url` shortcut opens.
pub(super) fn url_address(url_file: &Path) -> Option<String> {
    let text = String::from_utf8_lossy(&fs::read(url_file).ok()?).to_string();
    text.lines()
        .find_map(|l| l.trim().strip_prefix("URL=").map(|u| u.trim().to_string()))
}

/// Shortcuts are tiny; anything bigger is not one.
const MAX_LINK_BYTES: u64 = 1 << 20;

pub(super) fn read_link(path: &Path) -> Option<ShellLink> {
    if fs::metadata(path).ok()?.len() > MAX_LINK_BYTES {
        return None;
    }
    shell_link::parse(&fs::read(path).ok()?)
}

/// The file a shortcut runs, inside the bottle: its local path, or else its relative path
/// from the shortcut's own folder when that stays inside the bottle.
pub(super) fn link_target(prefix: &Path, lnk: &Path, link: &ShellLink) -> Option<PathBuf> {
    if let Some(t) = link
        .target
        .as_deref()
        .filter(|t| drive_stays_in_bottle(prefix, t))
        .and_then(|t| windows_to_unix(prefix, t))
    {
        return Some(t);
    }
    if link.relative_path.is_empty() {
        return None;
    }
    let joined = lnk.parent()?.join(link.relative_path.replace('\\', "/"));
    let real = fs::canonicalize(joined).ok()?;
    real.starts_with(fs::canonicalize(prefix).ok()?)
        .then_some(real)
}

/// Whether a Windows path is on the bottle's own C: drive or on a drive that is a link to a
/// place inside the bottle. Drives that lead to the Mac's folders (Z: usually does) are left
/// alone, so reading a shortcut never looks into them.
pub(super) fn drive_stays_in_bottle(prefix: &Path, win: &str) -> bool {
    let w = win.trim().trim_matches('"');
    let Some((drive, _)) = w.split_once(':') else {
        return false;
    };
    if drive.eq_ignore_ascii_case("c") {
        return true;
    }
    let dev = prefix
        .join("dosdevices")
        .join(format!("{}:", drive.to_lowercase()));
    match (fs::canonicalize(dev), fs::canonicalize(prefix)) {
        (Ok(d), Ok(p)) => d.starts_with(p),
        _ => false,
    }
}
