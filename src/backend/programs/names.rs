//! Folding names, the rules that tell noise from programs, and the identity of a program.

use super::*;

/// Folds a name for comparing: lower case, without accents. "Cámara" and "CAMARA" are the same.
pub fn fold_name(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' | 'å' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Whether a program answers a search: the query is already folded; it is looked for in the
/// folded name and in the Start Menu folder.
pub fn matches_search(program: &Program, folded_query: &str) -> bool {
    folded_query.is_empty()
        || fold_name(&program.name).contains(folded_query)
        || fold_name(&program.folder).contains(folded_query)
}

/// The words of a folded name.
fn words(folded: &str) -> Vec<&str> {
    folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Words that make a shortcut an uninstaller wherever they are.
const REMOVAL_WORDS: [&str; 6] = [
    "uninstall",
    "uninstaller",
    "uninstalling",
    "desinstalar",
    "desinstalador",
    "desinstalacion",
];
/// Words that make a shortcut an uninstaller when the name starts with them ("Eliminar Foo").
const REMOVAL_LEAD: [&str; 3] = ["remove", "eliminar", "quitar"];
/// Words of documents, help and web pages.
const DOCUMENT_WORDS: [&str; 13] = [
    "readme", "leame", "leeme", "help", "ayuda", "manual", "manuals", "license", "licence",
    "licencia", "support", "soporte", "website",
];
const DOCUMENT_PHRASES: [[&str; 2]; 4] = [
    ["release", "notes"],
    ["web", "site"],
    ["sitio", "web"],
    ["pagina", "web"],
];
const DOCUMENT_EXTENSIONS: [&str; 11] = [
    "txt", "pdf", "chm", "hlp", "html", "htm", "rtf", "doc", "docx", "md", "mht",
];
/// Windows programs that only show a document or a page they are given.
const VIEWERS: [&str; 6] = [
    "hh.exe",
    "notepad.exe",
    "wordpad.exe",
    "write.exe",
    "iexplore.exe",
    "winhlp32.exe",
];

pub(super) fn is_web_address(s: &str) -> bool {
    let s = s.trim().trim_matches('"').to_lowercase();
    ["http://", "https://", "ftp://", "www."]
        .iter()
        .any(|p| s.starts_with(p))
}

pub(super) fn extension(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    file.rsplit_once('.')
        .map(|(_, e)| e.trim_matches('"').to_lowercase())
        .unwrap_or_default()
}

/// Whether what a shortcut runs is an uninstaller, a document or a web page.
fn runs_noise(target: &str, args: &str) -> bool {
    let file = target
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(target)
        .trim_matches('"')
        .to_lowercase();
    if file.starts_with("unins") {
        return true;
    }
    let tokens: Vec<String> = args.split_whitespace().map(|t| t.to_lowercase()).collect();
    if file == "msiexec.exe" || file == "msiexec" {
        return tokens.iter().any(|t| {
            t == "/x" || t.starts_with("/x{") || t == "/uninstall" || t.starts_with("/uninstall")
        });
    }
    if DOCUMENT_EXTENSIONS.contains(&extension(&file).as_str()) || is_web_address(target) {
        return true;
    }
    VIEWERS.contains(&file.as_str())
        && tokens
            .iter()
            .any(|t| DOCUMENT_EXTENSIONS.contains(&extension(t).as_str()) || is_web_address(t))
}

/// Whether a shortcut is something other than a program a person would open: an uninstaller, a
/// manual, a web link. The name is judged by whole words ("Helper" is a program, "Help" is not).
/// When the shortcut says what it runs (`runs`: target and arguments), that decides how
/// documents and help are told from programs; uninstallers are recognised by either.
pub fn is_noise(name: &str, runs: Option<(&str, &str)>) -> bool {
    let folded = fold_name(name);
    let w = words(&folded);
    if w.first().is_some_and(|f| REMOVAL_LEAD.contains(f))
        || w.iter().any(|x| REMOVAL_WORDS.contains(x))
    {
        return true;
    }
    match runs {
        Some((target, args)) => runs_noise(target, args),
        None => {
            w.iter().any(|x| DOCUMENT_WORDS.contains(x))
                || w.windows(2)
                    .any(|pair| DOCUMENT_PHRASES.iter().any(|ph| ph[..] == pair[..]))
        }
    }
}

pub(super) fn identity_of_target(target: &Path, args: &str) -> String {
    format!(
        "{}|{}",
        target.to_string_lossy().to_lowercase(),
        fold_name(args.trim())
    )
}
