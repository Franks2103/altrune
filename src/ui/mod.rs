//! Everything the window shows: the design tokens, the icons, the reusable components and the
//! screens. Screens only read the app's state and emit `Action`s; the app applies them.

pub mod brand;
pub mod components;
pub mod icons;
pub mod screens;
pub mod theme;

/// The first `n` characters of `s`, with «…» after them when text was cut off: a name or a path
/// that must fit in a small place on screen.
pub fn short(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        format!("{}…", s.chars().take(n).collect::<String>())
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::short;

    /// Text that fits is kept as it is; longer text is cut to `n` characters plus «…», counting
    /// characters and not bytes.
    #[test]
    fn short_text_ends_in_an_ellipsis_only_when_it_was_cut() {
        assert_eq!(short("Steam", 5), "Steam");
        assert_eq!(short("Steam", 9), "Steam");
        assert_eq!(short("Rockstar", 4), "Rock…");
        assert_eq!(short("Configuración", 6), "Config…");
        assert_eq!(short("", 3), "");
    }
}
