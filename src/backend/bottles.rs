//! Bottles: finding them, reading their settings and the rules for naming new ones.

use super::*;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_LOGPIXELS: u32 = 96;

#[derive(Clone, Debug)]
pub struct Bottle {
    pub name: String,
    pub path: PathBuf,
    pub windows: String,
    pub msync: bool,
    pub high_res: bool,
    pub logpixels: u32,
    pub programs: Vec<Program>,
    /// The bottle's own environment variables (the `[EnvironmentVariables]` of its settings file).
    pub env: Vec<(String, String)>,
    /// The folder has the files of a finished Windows (`drive_c` and `system.reg`). Looked at
    /// once, when the bottle is loaded, so the window never reads the disk to show it.
    pub prefix_complete: bool,
}

pub const WINDOWS_VERSIONS: &[(&str, &str)] = &[
    ("win11", "Windows 11"),
    ("win10", "Windows 10"),
    ("win81", "Windows 8.1"),
    ("win7", "Windows 7"),
    ("winxp64", "Windows XP"),
];

pub fn windows_label(code: &str) -> String {
    WINDOWS_VERSIONS
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, l)| l.to_string())
        .unwrap_or_else(|| match code {
            "win8" => "Windows 8".into(),
            "vista" => "Windows Vista".into(),
            "winxp" => "Windows XP".into(),
            "win2000" => "Windows 2000".into(),
            "win98" => "Windows 98".into(),
            other => other.to_string(),
        })
}

pub fn is_bottle_dir(p: &Path) -> bool {
    p.join("drive_c").is_dir() || p.join("system.reg").is_file()
}

/// Whether the folder looks like a finished Windows: both `drive_c` and `system.reg` exist. A
/// folder with only one of them is listed, but its state says it is incomplete.
fn prefix_looks_complete(p: &Path) -> bool {
    p.join("drive_c").is_dir() && p.join("system.reg").is_file()
}

/// Reads one value out of the bottle's settings file (`cxbottle.conf`): `"Key" = "Value"`.
fn conf_value(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    text.lines()
        .filter(|l| !l.trim_start().starts_with(";;"))
        .find(|l| l.trim_start().starts_with(&needle))
        .and_then(|l| l.split('=').nth(1))
        .map(|v| v.trim().trim_matches('"').to_string())
}

/// Finds `"name"=value` inside a `[section]` of a Wine registry file.
fn reg_value(text: &str, section: &str, name: &str) -> Option<String> {
    let header = format!("[{}]", section.replace('\\', "\\\\"));
    let mut inside = false;
    let needle = format!("\"{}\"=", name).to_lowercase();
    for line in text.lines() {
        if line.starts_with('[') {
            inside = line.to_lowercase().starts_with(&header.to_lowercase());
            continue;
        }
        if inside && line.to_lowercase().starts_with(&needle) {
            return Some(line[needle.len()..].trim().to_string());
        }
    }
    None
}

pub fn read_high_res(prefix: &Path) -> (bool, u32) {
    let text = fs::read_to_string(prefix.join("user.reg")).unwrap_or_default();
    let retina = reg_value(&text, "Software\\Wine\\Mac Driver", "RetinaMode")
        .map(|v| {
            let v = v.trim_matches('"');
            v.chars()
                .next()
                .map(|c| "yYtT1".contains(c))
                .unwrap_or(false)
        })
        .unwrap_or(false);
    let lp = reg_value(&text, "Control Panel\\Desktop", "LogPixels")
        .and_then(|v| v.strip_prefix("dword:").map(|h| h.to_string()))
        .and_then(|h| u32::from_str_radix(&h, 16).ok())
        .unwrap_or(DEFAULT_LOGPIXELS);
    (retina, lp)
}

/// The bottle's [EnvironmentVariables] from its settings file (MSync is handled by its own switch).
fn bottle_env(conf: &str) -> Vec<(String, String)> {
    let mut inside = false;
    let mut v = vec![];
    for line in conf.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            inside = l == "[EnvironmentVariables]";
            continue;
        }
        if !inside || l.starts_with(";;") || !l.starts_with('"') {
            continue;
        }
        if let Some((k, val)) = l.split_once('=') {
            let k = k.trim().trim_matches('"').to_string();
            let val = val.trim().trim_matches('"').to_string();
            if k.is_empty() || k == "WINEMSYNC" || k.starts_with("CX_") {
                continue;
            }
            v.push((k, val));
        }
    }
    v
}

pub(super) fn load_bottle(path: PathBuf, cfg: &Config) -> Bottle {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let conf = fs::read_to_string(path.join("cxbottle.conf")).unwrap_or_default();
    let settings = cfg.bottle_ref(&path);
    let windows = settings
        .and_then(|s| s.windows.clone())
        .or_else(|| conf_value(&conf, "Template").map(|t| t.trim_end_matches("_64").to_string()))
        .unwrap_or_else(|| "win10".into());
    let msync = settings
        .and_then(|s| s.msync)
        .or_else(|| conf_value(&conf, "WINEMSYNC").map(|v| v == "1"))
        .unwrap_or(true);
    let (high_res, logpixels) = read_high_res(&path);
    let programs = scan_programs(&path, settings);
    let env = bottle_env(&conf);
    let prefix_complete = prefix_looks_complete(&path);
    Bottle {
        name,
        path,
        windows,
        msync,
        high_res,
        logpixels,
        programs,
        env,
        prefix_complete,
    }
}

/// A folder whose name starts with a dot: the app makes copies and imports under such a name and
/// gives them their real one only when they are complete, so no scan lists them.
pub fn is_work_in_progress(p: &Path) -> bool {
    p.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

pub fn scan_bottles(cfg: &Config) -> Vec<Bottle> {
    let mut found: Vec<PathBuf> = fs::read_dir(own_bottles_dir())
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                // A hidden folder is something the app is still making (a copy, an import).
                .filter(|p| !is_work_in_progress(p))
                .filter(|p| is_bottle_dir(p))
                .collect()
        })
        .unwrap_or_default();
    found.sort_by_key(|p| p.file_name().map(|s| s.to_string_lossy().to_lowercase()));
    found.into_iter().map(|p| load_bottle(p, cfg)).collect()
}

/// Characters a bottle name never keeps on macOS: each one is turned into `_`.
pub const PROHIBITED_CHARS: &str = "\\/*?:|<>\"'{}";

/// What the last character kept in a cleaned-up bottle name means for the next one.
#[derive(Clone, Copy, PartialEq)]
enum NameTail {
    /// Ordinary text, or nothing kept yet.
    Text,
    /// The name ends in a separator `_` that the next character may replace.
    Separator,
    /// That separator was just replaced by `=` or `-`; more of `_`, `=` and `-` are swallowed.
    Joined,
}

/// Turns what the person typed into a name that is safe as a bottle folder.
///
/// This is the specification, written from how names look to the person:
///
/// 1. Every prohibited character (see [`PROHIBITED_CHARS`]) counts as `_`.
/// 2. Dots and underscores before the first kept character are dropped (a leading dot would
///    hide the folder). Dots anywhere else are kept. Leading spaces and any other character
///    are kept as they are.
/// 3. Several underscores in a row give one.
/// 4. Several spaces in a row give one.
/// 5. An underscore followed by a space gives the space: the space wins. (A space that came
///    before the underscore is not merged with it: `"a _ b"` keeps both spaces.)
/// 6. An underscore followed by `=` or `-` gives that character: the underscore gives way.
///    Right after that, any further `_`, `=` or `-` are dropped until another character
///    comes (`"a_-=_b"` gives `"a-b"`; a dot or a space ends this).
/// 7. Underscores and spaces at the end are trimmed.
///
/// Accents, other alphabets and emoji are ordinary characters and pass through untouched.
/// The result may be empty; callers choose a fallback name.
pub fn sanitize_bottle_name(hint: &str) -> String {
    let mut name = String::with_capacity(hint.len());
    let mut tail = NameTail::Text;
    for ch in hint
        .chars()
        .map(|c| if PROHIBITED_CHARS.contains(c) { '_' } else { c })
    {
        let nothing_kept_yet = name.is_empty();
        match (tail, ch) {
            (NameTail::Text, '.' | '_') if nothing_kept_yet => {}
            (NameTail::Text, '_') => {
                name.push('_');
                tail = NameTail::Separator;
            }
            (NameTail::Text, ' ') if name.ends_with(' ') => {}
            (NameTail::Separator, '_') | (NameTail::Joined, '_' | '=' | '-') => {}
            (NameTail::Separator, '=' | '-') => {
                name.pop();
                name.push(ch);
                tail = NameTail::Joined;
            }
            (NameTail::Separator, ' ') => {
                name.pop();
                name.push(' ');
                tail = NameTail::Text;
            }
            _ => {
                name.push(ch);
                tail = NameTail::Text;
            }
        }
    }
    name.truncate(name.trim_end_matches(['_', ' ']).len());
    name
}

fn all_bottle_names() -> Vec<String> {
    fs::read_dir(own_bottles_dir())
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().to_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

/// A name that is free: on a clash (an existing bottle or folder) it tries `<name>-2`,
/// `<name>-3`…
pub fn unique_bottle_name(base: &str) -> String {
    let existing = all_bottle_names();
    let mut candidate = base.to_string();
    let mut i = 1;
    loop {
        let low = candidate.to_lowercase();
        let clash = existing.contains(&low) || own_bottles_dir().join(&candidate).exists();
        if !clash {
            return candidate;
        }
        i += 1;
        candidate = format!("{base}-{i}");
    }
}

/// A typed name is accepted as it is when it is not empty and already clean and free (what
/// `sanitize_bottle_name` and `unique_bottle_name` would give back unchanged).
pub fn is_valid_new_bottle_name(name: &str) -> bool {
    !name.is_empty() && name == unique_bottle_name(&sanitize_bottle_name(name))
}

pub fn unique_bottle_path(name: &str) -> PathBuf {
    let clean = sanitize_bottle_name(name);
    let clean = if clean.is_empty() {
        "Mi botella".to_string()
    } else {
        clean
    };
    own_bottles_dir().join(unique_bottle_name(&clean))
}

/// The usual file names of an installer on a disc or in a folder, in the order they are tried
/// (after autorun.inf, which names the installer itself).
pub const INSTALLER_NAMES: [&str; 7] = [
    "autorun.exe",
    "setup.exe",
    "setup.bat",
    "install.exe",
    "install.bat",
    "setuppls.exe",
    "setupse.exe",
];

/// Finds the installer inside a folder: autorun.inf's `open=` line first, then the known names.
pub fn find_installer_in(dir: &Path) -> Option<PathBuf> {
    let entries: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    let by_name = |n: &str| {
        entries
            .iter()
            .find(|p| {
                p.file_name()
                    .map(|f| f.to_string_lossy().eq_ignore_ascii_case(n))
                    .unwrap_or(false)
            })
            .cloned()
    };
    if let Some(inf) = by_name("autorun.inf") {
        let text = String::from_utf8_lossy(&fs::read(&inf).unwrap_or_default()).to_string();
        for line in text.lines() {
            let l = line.trim();
            if l.to_lowercase().starts_with("open=") {
                let target = l[5..]
                    .trim()
                    .trim_matches('"')
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .replace('\\', "/");
                let p = dir.join(&target);
                if p.is_file() {
                    return Some(p);
                }
                if let Some(found) = by_name(&target) {
                    return Some(found);
                }
            }
        }
    }
    INSTALLER_NAMES.iter().find_map(|n| by_name(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_ordinary_names_and_cleans_the_rest() {
        assert_eq!(sanitize_bottle_name("Mi/Juego"), "Mi_Juego");
        assert_eq!(sanitize_bottle_name(".oculto"), "oculto");
        assert_eq!(sanitize_bottle_name("a//b"), "a_b");
        assert_eq!(sanitize_bottle_name("a_ b"), "a b");
        assert_eq!(sanitize_bottle_name("a:-b"), "a-b");
        assert_eq!(sanitize_bottle_name("juego?? "), "juego");
        assert_eq!(sanitize_bottle_name("Steam 2"), "Steam 2");
    }

    /// Each rule of `sanitize_bottle_name`, one row per case: (typed, kept).
    #[test]
    fn sanitize_rules_one_by_one() {
        let table: &[(&str, &str)] = &[
            // 1. prohibited characters become `_`
            ("a\\b", "a_b"),
            ("a/b", "a_b"),
            ("a*b", "a_b"),
            ("a?b", "a_b"),
            ("a:b", "a_b"),
            ("a|b", "a_b"),
            ("a<b", "a_b"),
            ("a>b", "a_b"),
            ("a\"b", "a_b"),
            ("a'b", "a_b"),
            ("a{b", "a_b"),
            ("a}b", "a_b"),
            // 2. leading dots and underscores go, other dots stay, leading spaces stay
            ("...a", "a"),
            (".._a", "a"),
            ("?.a", "a"),
            ("a.b.", "a.b."),
            ("a._b", "a._b"),
            (" a", " a"),
            // 3. runs of underscores
            ("a___b", "a_b"),
            ("a/\\?b", "a_b"),
            // 4. runs of spaces
            ("a   b", "a b"),
            // 5. the space wins over the underscore, but not over a space before it
            ("a_ b", "a b"),
            ("a __ b", "a  b"),
            ("a _ b", "a  b"),
            // 6. the underscore gives way to `=` and `-`, which swallow what follows
            ("a_=b", "a=b"),
            ("a_-b", "a-b"),
            ("a:-b", "a-b"),
            ("a_-=_b", "a-b"),
            ("a_==b", "a=b"),
            ("a_-.-b", "a-.-b"),
            ("a_- -b", "a- -b"),
            ("a=_b", "a=_b"),
            ("a-b", "a-b"),
            // 7. trailing underscores and spaces are trimmed
            ("a_", "a"),
            ("a _ ", "a"),
            ("juego?? ", "juego"),
            // nothing left, and characters from outside ASCII pass through
            ("", ""),
            ("...", ""),
            ("___", ""),
            ("   ", ""),
            ("Ñandú 日本 \u{1F3AE}", "Ñandú 日本 \u{1F3AE}"),
        ];
        for (typed, kept) in table {
            assert_eq!(sanitize_bottle_name(typed), *kept, "typed {typed:?}");
        }
    }

    /// A folder with only half of the files of a Windows is not a finished bottle.
    #[test]
    fn a_bottle_folder_needs_both_marker_files_to_look_complete() {
        let tmp = std::env::temp_dir().join(format!("botella-prefix-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("drive_c")).unwrap();
        assert!(is_bottle_dir(&tmp));
        assert!(!prefix_looks_complete(&tmp));
        fs::write(tmp.join("system.reg"), "").unwrap();
        assert!(prefix_looks_complete(&tmp));
        fs::remove_dir_all(tmp.join("drive_c")).unwrap();
        assert!(!prefix_looks_complete(&tmp));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn unusable_names_are_refused() {
        assert!(!is_valid_new_bottle_name(""));
        assert!(!is_valid_new_bottle_name("a/b"));
    }
}
