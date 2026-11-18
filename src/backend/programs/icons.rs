//! Icons: the pictures a bottle already extracted, a shortcut's own picture and the PNG cache.

use super::*;

/// Icons already extracted for a bottle (a bottle copied from elsewhere may carry them):
/// cxmenu.conf gives each shortcut an `"Icon"` name, stored as PNGs under
/// windata/cxmenu/icons/hicolor/<size>/apps.
pub(super) fn cached_icons(prefix: &Path) -> std::collections::HashMap<String, PathBuf> {
    let mut map = std::collections::HashMap::new();
    let conf = fs::read_to_string(prefix.join("cxmenu.conf")).unwrap_or_default();
    let icons = prefix.join("windata/cxmenu/icons/hicolor");
    let mut sizes: Vec<(u32, PathBuf)> = fs::read_dir(&icons)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    n.split('x')
                        .next()?
                        .parse::<u32>()
                        .ok()
                        .map(|s| (s, e.path().join("apps")))
                })
                .collect()
        })
        .unwrap_or_default();
    sizes.sort_by_key(|s| std::cmp::Reverse(s.0));
    let mut current: Option<String> = None;
    for line in conf.lines() {
        let l = line.trim();
        if l.starts_with('[') && l.ends_with(']') {
            let section = &l[1..l.len() - 1];
            current = Path::new(section)
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase());
        } else if let (Some(stem), Some(icon)) = (&current, conf_line_value(l, "Icon")) {
            if let Some(png) = sizes
                .iter()
                .map(|(_, d)| d.join(format!("{icon}.png")))
                .find(|p| p.is_file())
            {
                map.entry(stem.clone()).or_insert(png);
            }
        }
    }
    map
}

fn conf_line_value(line: &str, key: &str) -> Option<String> {
    let rest = line.strip_prefix(&format!("\"{key}\""))?;
    Some(
        rest.trim()
            .trim_start_matches('=')
            .trim()
            .trim_matches('"')
            .to_string(),
    )
}

/// A `.url` shortcut's own icon (`ICONFILE=` an .ico inside the bottle), saved as PNG once.
pub(super) fn url_icon(prefix: &Path, url_file: &Path) -> Option<PathBuf> {
    let text = String::from_utf8_lossy(&fs::read(url_file).ok()?).to_string();
    let ico = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("ICONFILE=").map(|s| s.to_string()))?;
    if !drive_stays_in_bottle(prefix, &ico) {
        return None;
    }
    ico_to_png(&windows_to_unix(prefix, &ico)?)
}

/// Saves the largest picture of an .ico/.png as a PNG in the app's icon cache.
pub fn ico_to_png(src: &Path) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    fs::metadata(src).ok()?.len().hash(&mut h);
    let out = data_dir()
        .join("icons")
        .join(format!("{:016x}.png", h.finish()));
    if out.is_file() {
        return Some(out);
    }
    let img = image::open(src).ok()?;
    let _ = fs::create_dir_all(out.parent()?);
    img.save(&out).ok()?;
    Some(out)
}

/// The picture a shortcut names, when it is an .ico or .png inside the bottle.
pub(super) fn link_icon(prefix: &Path, link: &ShellLink) -> Option<PathBuf> {
    let loc = link.icon_location.as_deref()?;
    if !matches!(extension(loc).as_str(), "ico" | "png") {
        return None;
    }
    if !drive_stays_in_bottle(prefix, loc) {
        return None;
    }
    ico_to_png(&windows_to_unix(prefix, loc)?)
}
