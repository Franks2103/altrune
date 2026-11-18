//! Design tokens ("Graphite + Frost"): colours for the dark and light appearance, spacing,
//! corner radii, type sizes and weights, motion, and the fonts. Every component reads them from
//! here, so the whole window changes together.

use crate::backend::Appearance;
use crate::domain::status::{BottleStatus, StatusSymbol, StatusTone};
use crate::ui::icons::Icon;
use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Shadow, Stroke, Theme, Ui,
    Visuals,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The colours of one appearance.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    /// Behind the content.
    pub canvas: Color32,
    pub sidebar: Color32,
    /// Cards and grouped settings.
    pub surface: Color32,
    pub surface_hover: Color32,
    pub surface_pressed: Color32,
    /// Text fields, code and wells, set into the surface.
    pub sunken: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    /// Altrune's blue-violet, the one accent of the interface (the symbol's own hue, measured in
    /// `assets/branding/paleta.md`): accent text and icons, links, progress and spinners, the
    /// active-row marker. It reaches 4.5:1 as text on the canvas and the surfaces.
    pub accent: Color32,
    /// The accent for the keyboard-focus ring, the text cursor and the selection outline: marks
    /// that are not text, so they only need 3:1 against what is behind them.
    pub focus: Color32,
    /// Fills of the primary and the destructive button, in rest, hover and pressed states. They
    /// carry `on_accent` text, so each one reaches 4.5:1 with it in both appearances.
    pub fill_accent: Color32,
    pub fill_accent_hover: Color32,
    pub fill_accent_pressed: Color32,
    pub fill_danger: Color32,
    pub fill_danger_hover: Color32,
    pub fill_danger_pressed: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    /// The tones as small text on their own quiet wash (badges): darker in the light
    /// appearance and lighter in the dark one than the tone itself, which stays for dots and icons.
    pub accent_text: Color32,
    pub success_text: Color32,
    pub warning_text: Color32,
    pub danger_text: Color32,
    /// The track of a switch that is off: a part of the interface, so 3:1 against the surface.
    pub control_off: Color32,
    /// The selected row in the sidebar and the background of selected text: a pale (light) or deep
    /// (dark) wash of the brand blue. Text and the accent read on it at 4.5:1.
    pub selection: Color32,
    /// Hover wash on transparent controls.
    pub hover_wash: Color32,
    pub shadow: Color32,
    /// Behind dialogs.
    pub scrim: Color32,
}

pub const DARK: Palette = Palette {
    dark: true,
    canvas: Color32::from_rgb(29, 30, 34),
    sidebar: Color32::from_rgb(23, 24, 27),
    surface: Color32::from_rgb(37, 38, 43),
    surface_hover: Color32::from_rgb(43, 45, 51),
    surface_pressed: Color32::from_rgb(50, 52, 59),
    sunken: Color32::from_rgb(21, 22, 25),
    border: Color32::from_rgb(47, 49, 55),
    border_strong: Color32::from_rgb(62, 65, 73),
    text: Color32::from_rgb(234, 235, 238),
    text_muted: Color32::from_rgb(168, 172, 181),
    text_faint: Color32::from_rgb(148, 152, 161),
    accent: Color32::from_rgb(122, 165, 255),
    focus: Color32::from_rgb(91, 140, 255),
    fill_accent: Color32::from_rgb(61, 104, 245),
    fill_accent_hover: Color32::from_rgb(64, 107, 245),
    fill_accent_pressed: Color32::from_rgb(50, 90, 225),
    fill_danger: Color32::from_rgb(196, 60, 52),
    fill_danger_hover: Color32::from_rgb(202, 66, 58),
    fill_danger_pressed: Color32::from_rgb(176, 48, 42),
    on_accent: Color32::WHITE,
    success: Color32::from_rgb(70, 184, 132),
    warning: Color32::from_rgb(226, 166, 66),
    danger: Color32::from_rgb(238, 106, 96),
    accent_text: Color32::from_rgb(138, 177, 255),
    success_text: Color32::from_rgb(100, 208, 158),
    warning_text: Color32::from_rgb(236, 180, 90),
    danger_text: Color32::from_rgb(250, 142, 132),
    control_off: Color32::from_rgb(114, 118, 128),
    selection: Color32::from_rgb(38, 56, 112),
    hover_wash: Color32::from_rgba_premultiplied(18, 18, 20, 18),
    shadow: Color32::from_rgba_premultiplied(0, 0, 0, 110),
    scrim: Color32::from_rgba_premultiplied(0, 0, 0, 120),
};

pub const LIGHT: Palette = Palette {
    dark: false,
    canvas: Color32::from_rgb(245, 246, 248),
    sidebar: Color32::from_rgb(235, 237, 241),
    surface: Color32::WHITE,
    surface_hover: Color32::from_rgb(248, 249, 251),
    surface_pressed: Color32::from_rgb(239, 241, 245),
    sunken: Color32::from_rgb(238, 240, 243),
    border: Color32::from_rgb(224, 227, 233),
    border_strong: Color32::from_rgb(204, 209, 218),
    text: Color32::from_rgb(29, 31, 36),
    text_muted: Color32::from_rgb(82, 88, 100),
    text_faint: Color32::from_rgb(100, 106, 118),
    accent: Color32::from_rgb(42, 85, 210),
    focus: Color32::from_rgb(47, 107, 255),
    fill_accent: Color32::from_rgb(47, 95, 227),
    fill_accent_hover: Color32::from_rgb(40, 84, 208),
    fill_accent_pressed: Color32::from_rgb(35, 76, 192),
    fill_danger: Color32::from_rgb(204, 56, 46),
    fill_danger_hover: Color32::from_rgb(188, 46, 36),
    fill_danger_pressed: Color32::from_rgb(168, 38, 30),
    on_accent: Color32::WHITE,
    success: Color32::from_rgb(28, 150, 96),
    warning: Color32::from_rgb(158, 97, 0),
    danger: Color32::from_rgb(204, 56, 46),
    accent_text: Color32::from_rgb(34, 76, 190),
    success_text: Color32::from_rgb(18, 108, 68),
    warning_text: Color32::from_rgb(136, 80, 0),
    danger_text: Color32::from_rgb(176, 36, 28),
    control_off: Color32::from_rgb(140, 146, 158),
    selection: Color32::from_rgb(214, 226, 255),
    hover_wash: Color32::from_rgba_premultiplied(0, 0, 0, 10),
    shadow: Color32::from_rgba_premultiplied(0, 0, 0, 26),
    scrim: Color32::from_rgba_premultiplied(0, 0, 0, 60),
};

/// The palette of the appearance this `Ui` is drawn in.
pub fn pal(ui: &Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}

/// The palette of the window's current appearance.
pub fn pal_ctx(ctx: &egui::Context) -> &'static Palette {
    if ctx.theme() == Theme::Dark {
        &DARK
    } else {
        &LIGHT
    }
}

/// The meaning a colour carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Accent,
    Success,
    Warning,
    Danger,
}

/// The colour a bottle's state is drawn in. The state only says what it means (see
/// [`StatusTone`]); the palette is the window's, so the translation lives here.
pub fn status_tone(status: &BottleStatus) -> Tone {
    match status.tone() {
        StatusTone::Accent => Tone::Accent,
        StatusTone::Success => Tone::Success,
        StatusTone::Warning => Tone::Warning,
        StatusTone::Danger => Tone::Danger,
        StatusTone::Neutral => Tone::Neutral,
    }
}

/// The small picture of a bottle's state, from what the state means (see [`StatusSymbol`]).
pub fn status_icon(status: &BottleStatus) -> Icon {
    match status.symbol() {
        StatusSymbol::Working => Icon::Refresh,
        StatusSymbol::Running => Icon::Play,
        StatusSymbol::Alert => Icon::Alert,
        StatusSymbol::Ready => Icon::Check,
    }
}

impl Palette {
    pub fn tone(&self, tone: Tone) -> Color32 {
        match tone {
            Tone::Neutral => self.text_muted,
            Tone::Accent => self.accent,
            Tone::Success => self.success,
            Tone::Warning => self.warning,
            Tone::Danger => self.danger,
        }
    }

    /// The tone as small text on its own wash (`tone_soft`): reaches 4.5:1 in both appearances.
    pub fn tone_text(&self, tone: Tone) -> Color32 {
        match tone {
            Tone::Neutral => self.text_muted,
            Tone::Accent => self.accent_text,
            Tone::Success => self.success_text,
            Tone::Warning => self.warning_text,
            Tone::Danger => self.danger_text,
        }
    }

    /// A quiet wash of a tone, for badges and banners.
    pub fn tone_soft(&self, tone: Tone) -> Color32 {
        let base = if tone == Tone::Neutral {
            self.text_muted
        } else {
            self.tone(tone)
        };
        base.gamma_multiply(if self.dark { 0.18 } else { 0.11 })
    }
}

/// `a` towards `b` by `t` (0..1).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}

/// Spacing scale, in points.
pub mod space {
    pub const XXS: f32 = 2.0;
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

/// Corner radii.
pub mod radius {
    pub const SM: u8 = 6;
    pub const MD: u8 = 8;
    pub const LG: u8 = 10;
    pub const XL: u8 = 14;
}

/// Type sizes, in points (macOS body text is 13).
pub mod size {
    pub const CAPTION: f32 = 11.0;
    pub const SMALL: f32 = 12.0;
    pub const BODY: f32 = 13.0;
    pub const BODY_L: f32 = 14.0;
    pub const TITLE: f32 = 15.0;
    pub const H3: f32 = 17.0;
    pub const H2: f32 = 21.0;
    pub const H1: f32 = 26.0;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Medium,
    Semibold,
    Bold,
}

fn family(w: Weight) -> FontFamily {
    match w {
        Weight::Regular => FontFamily::Proportional,
        Weight::Medium => FontFamily::Name("medium".into()),
        Weight::Semibold => FontFamily::Name("semibold".into()),
        Weight::Bold => FontFamily::Name("bold".into()),
    }
}

pub fn font(size: f32, w: Weight) -> FontId {
    FontId::new(size, family(w))
}

/// Text in the design's type: size, weight and colour.
pub fn text(s: impl Into<String>, size: f32, w: Weight, color: Color32) -> RichText {
    RichText::new(s).size(size).family(family(w)).color(color)
}

/// Motion: durations in seconds. Short enough to feel immediate, long enough to be seen.
pub mod motion {
    pub const FAST: f32 = 0.12;
    pub const BASE: f32 = 0.18;
    pub const SLOW: f32 = 0.24;
}

/// Layout rules that depend on how much room a page has. The smallest window (820 x 540) leaves
/// about 520 points for the content once the sidebar (236) and the margins (64) are taken out.
pub mod layout {
    /// Below this content width the pages use their compact layout: fewer columns, actions under
    /// the title instead of beside it.
    pub const NARROW: f32 = 700.0;

    /// Whether `width` (usually `ui.available_width()`) calls for the compact layout.
    pub fn is_narrow(width: f32) -> bool {
        width < NARROW
    }

    /// Columns of bottle cards: three on a wide page, two on most, one when very little room.
    pub fn bottle_columns(width: f32) -> usize {
        if width > 860.0 {
            3
        } else if width >= 420.0 {
            2
        } else {
            1
        }
    }

    /// The longest start of `text`, with an ellipsis when it was cut, that `measure` finds no
    /// wider than `max_w`. `measure` gives a text's width in points.
    pub fn fit_text(text: &str, max_w: f32, measure: impl Fn(&str) -> f32) -> String {
        if text.is_empty() || measure(text) <= max_w {
            return text.to_string();
        }
        let ends: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
        let cut = |n: usize| format!("{}…", text[..ends[n]].trim_end());
        // The widest prefix that fits (widths only grow with more characters). All the
        // characters plus the ellipsis are wider than the text, which did not fit.
        let (mut lo, mut hi) = (0, ends.len() - 1);
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if measure(&cut(mid)) <= max_w {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        cut(lo)
    }
}

static REDUCED_MOTION: AtomicBool = AtomicBool::new(false);
/// A read of the system setting is running on its worker (so two never overlap).
static MOTION_READ_RUNNING: AtomicBool = AtomicBool::new(false);
/// The window had the focus on the last frame (a read starts when it comes back).
static WAS_FOCUSED: AtomicBool = AtomicBool::new(true);

/// Whether macOS asks for less motion (System Settings › Accessibility › Display).
pub fn reduced_motion() -> bool {
    REDUCED_MOTION.load(Ordering::Relaxed)
}

/// The places where macOS keeps "Reduce motion": the classic one and the newer accessibility
/// domain. Any of them saying "1" means the person asked for less motion.
const MOTION_KEYS: [(&str, &str); 2] = [
    ("com.apple.universalaccess", "reduceMotion"),
    ("com.apple.Accessibility", "ReduceMotionEnabled"),
];

/// Whether the text `defaults read` printed means "on" (`1`, or `true` in some domains).
pub fn defaults_says_on(output: &str) -> bool {
    matches!(output.trim(), "1" | "true" | "YES" | "yes")
}

/// Reads "Reduce motion" off the interface thread (`defaults` takes a few milliseconds) and
/// applies it. Safe to call again at any time: a read already running is not doubled.
pub fn detect_reduced_motion(ctx: &egui::Context) {
    if MOTION_READ_RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let on = MOTION_KEYS.iter().any(|(domain, key)| {
            std::process::Command::new("/usr/bin/defaults")
                .args(["read", domain, key])
                .output()
                .is_ok_and(|o| defaults_says_on(&String::from_utf8_lossy(&o.stdout)))
        });
        let was = REDUCED_MOTION.swap(on, Ordering::Relaxed);
        if on != was {
            // egui's own transitions (menus, the "Avanzado" fold) follow the setting too.
            let secs = if on {
                0.0
            } else {
                egui::Style::default().animation_time
            };
            ctx.all_styles_mut(|s| s.animation_time = secs);
        }
        MOTION_READ_RUNNING.store(false, Ordering::Release);
        ctx.request_repaint();
    });
}

/// Whether a read of the setting should start: the window has just come back to the front,
/// which is when the person may have changed it in System Settings.
pub fn should_reread_motion(was_focused: bool, focused: bool) -> bool {
    focused && !was_focused
}

/// Called once per frame: starts a read of the setting when the window regains the focus. It
/// only compares two flags; the read itself runs on a worker.
pub fn refresh_reduced_motion_on_focus(ctx: &egui::Context) {
    let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
    let was = WAS_FOCUSED.swap(focused, Ordering::Relaxed);
    if should_reread_motion(was, focused) {
        detect_reduced_motion(ctx);
    }
}

/// 0..1 following `value`, eased over `secs` (at once when motion is reduced). egui repaints
/// only while the value is moving.
pub fn anim_bool(ctx: &egui::Context, id: egui::Id, value: bool, secs: f32) -> f32 {
    let secs = if reduced_motion() { 0.0 } else { secs };
    ctx.animate_bool_with_time(id, value, secs)
}

/// A number following `target`, eased over `secs`.
pub fn anim_value(ctx: &egui::Context, id: egui::Id, target: f32, secs: f32) -> f32 {
    let secs = if reduced_motion() { 0.0 } else { secs };
    ctx.animate_value_with_time(id, target, secs)
}

/// The Mac's own typeface, read from the system at run time (nothing is bundled): San
/// Francisco, a variable font, at four weights. Without it egui's own font stays.
fn install_fonts(ctx: &egui::Context) {
    use egui::epaint::text::{FontTweak, VariationCoords};
    use egui::FontData;
    let mut fonts = egui::FontDefinitions::default();
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut weights: Vec<(&str, Vec<String>)> = vec![
        ("medium", fallback.clone()),
        ("semibold", fallback.clone()),
        ("bold", fallback),
    ];
    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/SFNS.ttf") {
        // Read once and shared by the four weights for the life of the app.
        let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let face = |w: f32| {
            Arc::new(FontData::from_static(bytes).tweak(FontTweak {
                coords: VariationCoords::new([(b"wght", w)]),
                ..Default::default()
            }))
        };
        fonts.font_data.insert("sf-regular".into(), face(400.0));
        fonts.font_data.insert("sf-medium".into(), face(510.0));
        fonts.font_data.insert("sf-semibold".into(), face(590.0));
        fonts.font_data.insert("sf-bold".into(), face(700.0));
        if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
            list.insert(0, "sf-regular".into());
        }
        for (name, list) in &mut weights {
            list.insert(0, format!("sf-{name}"));
        }
    }
    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/SFNSMono.ttf") {
        fonts
            .font_data
            .insert("sf-mono".into(), Arc::new(FontData::from_owned(bytes)));
        if let Some(list) = fonts.families.get_mut(&FontFamily::Monospace) {
            list.insert(0, "sf-mono".into());
        }
    }
    for (name, list) in weights {
        fonts.families.insert(FontFamily::Name(name.into()), list);
    }
    ctx.set_fonts(fonts);
}

fn style_for(p: &Palette) -> egui::Style {
    let mut style = egui::Style::default();
    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = p.canvas;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.extreme_bg_color = p.sunken;
    v.faint_bg_color = p.surface_hover;
    v.code_bg_color = p.sunken;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v.window_corner_radius = CornerRadius::same(radius::XL);
    v.menu_corner_radius = CornerRadius::same(radius::LG);
    v.window_shadow = Shadow {
        offset: [0, 12],
        blur: 36,
        spread: 0,
        color: p.shadow,
    };
    v.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: p.shadow,
    };
    v.selection.bg_fill = p.selection;
    v.selection.stroke = Stroke::new(1.0, p.focus);
    v.text_cursor.stroke = Stroke::new(2.0, p.focus);
    v.widgets.noninteractive.bg_fill = p.surface;
    v.widgets.noninteractive.weak_bg_fill = p.surface;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.inactive.bg_fill = p.border_strong;
    v.widgets.inactive.weak_bg_fill = p.surface;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border_strong);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.hovered.bg_fill = p.border_strong;
    v.widgets.hovered.weak_bg_fill = p.surface_hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, mix(p.border_strong, p.focus, 0.35));
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.active.bg_fill = p.accent;
    v.widgets.active.weak_bg_fill = p.surface_pressed;
    // Pressed and keyboard-focused: a clear accent border (egui draws focus as "active").
    v.widgets.active.bg_stroke = Stroke::new(2.0, p.focus);
    v.widgets.active.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.open.weak_bg_fill = p.surface_hover;
    v.widgets.open.bg_stroke = Stroke::new(1.0, p.border_strong);
    v.widgets.open.fg_stroke = Stroke::new(1.0, p.text);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(radius::MD);
        w.expansion = 0.0;
    }
    style.visuals = v;
    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(space::S, 6.0);
    s.button_padding = egui::vec2(space::M, 5.0);
    s.interact_size.y = 28.0;
    s.menu_margin = Margin::same(6);
    s.window_margin = Margin::same(16);
    s.combo_width = 150.0;
    s.scroll = egui::style::ScrollStyle::floating();
    use egui::TextStyle::{Body, Button, Heading, Monospace, Small};
    style
        .text_styles
        .insert(Body, font(size::BODY, Weight::Regular));
    style
        .text_styles
        .insert(Button, font(size::BODY, Weight::Medium));
    style
        .text_styles
        .insert(Heading, font(size::H2, Weight::Semibold));
    style
        .text_styles
        .insert(Small, font(size::CAPTION, Weight::Regular));
    style
        .text_styles
        .insert(Monospace, FontId::monospace(size::SMALL));
    style.interaction.tooltip_delay = 0.35;
    style
}

/// What a test needs to draw the components: the style, and the named weights as plain
/// families (the system fonts are not read and no process is started).
#[cfg(test)]
pub fn install_for_tests(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for name in ["medium", "semibold", "bold"] {
        fonts
            .families
            .insert(FontFamily::Name(name.into()), fallback.clone());
    }
    ctx.set_fonts(fonts);
    ctx.set_style_of(Theme::Dark, style_for(&DARK));
    ctx.set_style_of(Theme::Light, style_for(&LIGHT));
    ctx.set_theme(egui::ThemePreference::Dark);
}

/// What egui is told to show for a saved appearance.
pub fn preference_of(appearance: Appearance) -> egui::ThemePreference {
    match appearance {
        Appearance::System => egui::ThemePreference::System,
        Appearance::Light => egui::ThemePreference::Light,
        Appearance::Dark => egui::ThemePreference::Dark,
    }
}

/// What the window starts with: the command line's `--tema` (used for screenshots) beats the
/// saved appearance.
pub fn start_preference(forced: Option<Theme>, saved: Appearance) -> egui::ThemePreference {
    match forced {
        Some(t) => egui::ThemePreference::from(t),
        None => preference_of(saved),
    }
}

/// Shows `appearance` at once (egui eases the colours over by itself).
pub fn apply_appearance(ctx: &egui::Context, appearance: Appearance) {
    ctx.set_theme(preference_of(appearance));
}

/// Fonts, both appearances, and which one to show: a forced one from the command line, else the
/// saved appearance (by default the Mac's own).
pub fn install(ctx: &egui::Context, forced: Option<Theme>, saved: Appearance) {
    install_fonts(ctx);
    ctx.set_style_of(Theme::Dark, style_for(&DARK));
    ctx.set_style_of(Theme::Light, style_for(&LIGHT));
    ctx.set_theme(start_preference(forced, saved));
    // egui would also give the native window its own light or dark look when a fixed theme is
    // chosen, and macOS then stops telling winit when the Mac changes. Going back to
    // «Automática» would show the theme the Mac had when the choice was made. The window draws
    // everything itself, so it never needs a look of its own.
    ctx.options_mut(|o| o.sync_window_theme = false);
    detect_reduced_motion(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_beats_the_saved_appearance_and_the_default_follows_the_mac() {
        use egui::ThemePreference as P;
        for saved in Appearance::ALL {
            assert_eq!(start_preference(Some(Theme::Light), saved), P::Light);
            assert_eq!(start_preference(Some(Theme::Dark), saved), P::Dark);
        }
        assert_eq!(start_preference(None, Appearance::System), P::System);
        assert_eq!(start_preference(None, Appearance::Light), P::Light);
        assert_eq!(start_preference(None, Appearance::Dark), P::Dark);
        assert_eq!(preference_of(Appearance::default()), P::System);
    }

    #[test]
    fn the_native_window_never_gets_a_look_of_its_own() {
        let ctx = egui::Context::default();
        assert!(ctx.options(|o| o.sync_window_theme), "egui's default");
        for saved in Appearance::ALL {
            install(&ctx, None, saved);
            assert!(!ctx.options(|o| o.sync_window_theme), "{saved:?}");
            // Choosing a fixed theme later sends no command that would set the window's look.
            ctx.set_theme(egui::ThemePreference::Dark);
            let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
            out.textures_delta.clear();
            let set_theme = out.viewport_output.values().any(|v| {
                v.commands
                    .iter()
                    .any(|c| matches!(c, egui::ViewportCommand::SetTheme(_)))
            });
            assert!(!set_theme, "{saved:?}");
        }
    }

    fn lum(c: Color32) -> f32 {
        let ch = |v: u8| {
            let v = f32::from(v) / 255.0;
            if v <= 0.040_45 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
    }

    /// WCAG contrast ratio of two opaque colours.
    fn ratio(a: Color32, b: Color32) -> f32 {
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// A translucent (premultiplied) colour laid over an opaque background.
    fn over(bg: Color32, wash: Color32) -> Color32 {
        let k = 1.0 - f32::from(wash.a()) / 255.0;
        let ch = |w: u8, b: u8| (f32::from(w) + f32::from(b) * k).round().min(255.0) as u8;
        Color32::from_rgb(
            ch(wash.r(), bg.r()),
            ch(wash.g(), bg.g()),
            ch(wash.b(), bg.b()),
        )
    }

    fn named(p: &Palette) -> Vec<(&'static str, Color32)> {
        vec![
            ("canvas", p.canvas),
            ("surface", p.surface),
            ("sidebar", p.sidebar),
            ("sunken", p.sunken),
            ("selection", p.selection),
            ("surface_hover", p.surface_hover),
        ]
    }

    const TONES: [Tone; 5] = [
        Tone::Neutral,
        Tone::Accent,
        Tone::Success,
        Tone::Warning,
        Tone::Danger,
    ];

    /// Text must stay readable on every background it is drawn on, in both appearances (WCAG
    /// contrast ratio): 4.5 for small text, 7 for the main text on the big surfaces.
    #[test]
    fn text_is_readable_in_both_appearances() {
        for p in [&DARK, &LIGHT] {
            for (name, bg) in named(p) {
                let big = matches!(name, "canvas" | "surface" | "sidebar");
                let need = if big { 7.0 } else { 4.5 };
                assert!(
                    ratio(p.text, bg) >= need,
                    "text on {name} (dark: {})",
                    p.dark
                );
                assert!(
                    ratio(p.text_muted, bg) >= 4.5,
                    "muted text on {name} (dark: {})",
                    p.dark
                );
            }
            // The faint text (captions, hints) is drawn on these, never on the selection.
            for (name, bg) in named(p).into_iter().filter(|(n, _)| *n != "selection") {
                assert!(
                    ratio(p.text_faint, bg) >= 4.5,
                    "faint text on {name} (dark: {})",
                    p.dark
                );
            }
        }
    }

    /// Button labels (13 and 14 pt, not large text) need 4.5 on the fill in every state.
    #[test]
    fn button_text_is_readable_in_every_state() {
        for p in [&DARK, &LIGHT] {
            for (name, fill) in [
                ("primary", p.fill_accent),
                ("primary hover", p.fill_accent_hover),
                ("primary pressed", p.fill_accent_pressed),
                ("danger", p.fill_danger),
                ("danger hover", p.fill_danger_hover),
                ("danger pressed", p.fill_danger_pressed),
            ] {
                assert!(ratio(p.on_accent, fill) >= 4.5, "{name} (dark: {})", p.dark);
            }
            for (name, fill) in [
                ("rest", p.surface),
                ("hover", p.surface_hover),
                ("pressed", p.surface_pressed),
            ] {
                assert!(
                    ratio(p.text, fill) >= 4.5,
                    "secondary {name} (dark: {})",
                    p.dark
                );
            }
            // Quiet buttons: muted text at rest and text under the pointer, over the washes.
            for bg in [p.canvas, p.surface, p.sidebar] {
                assert!(ratio(p.text_muted, bg) >= 4.5, "ghost rest");
                let hover = over(bg, p.hover_wash);
                let pressed = over(bg, p.hover_wash.gamma_multiply(2.0));
                assert!(ratio(p.text, hover) >= 4.5, "ghost hover");
                assert!(ratio(p.text, pressed) >= 4.5, "ghost pressed");
            }
        }
    }

    /// Badges: small text in the tone's own text colour on its wash, over the two backgrounds
    /// they sit on. Banners put the normal text colours on the same washes.
    #[test]
    fn badges_and_banners_are_readable() {
        for p in [&DARK, &LIGHT] {
            for bg in [p.canvas, p.surface] {
                for tone in TONES {
                    let wash = over(bg, p.tone_soft(tone));
                    assert!(
                        ratio(p.tone_text(tone), wash) >= 4.5,
                        "badge {tone:?} (dark: {})",
                        p.dark
                    );
                    assert!(ratio(p.text, wash) >= 4.5, "banner title {tone:?}");
                    assert!(ratio(p.text_muted, wash) >= 4.5, "banner text {tone:?}");
                }
            }
        }
    }

    /// The tones drawn as plain text (the error line, a warning under a field) are readable on
    /// the canvas and the surface; as dots and icons they reach 3:1.
    #[test]
    fn tones_are_readable_as_text_and_as_icons() {
        for p in [&DARK, &LIGHT] {
            for bg in [p.canvas, p.surface] {
                for tone in [Tone::Accent, Tone::Warning, Tone::Danger] {
                    assert!(
                        ratio(p.tone(tone), bg) >= 4.5,
                        "{tone:?} text (dark: {})",
                        p.dark
                    );
                }
                for tone in TONES {
                    assert!(
                        ratio(p.tone(tone), bg) >= 3.0,
                        "{tone:?} icon (dark: {})",
                        p.dark
                    );
                    let wash = over(bg, p.tone_soft(tone));
                    assert!(
                        ratio(p.tone(tone), wash) >= 3.0,
                        "{tone:?} icon on its wash (dark: {})",
                        p.dark
                    );
                }
            }
        }
    }

    /// The focus ring and the parts of controls need 3:1 against what is behind them.
    #[test]
    fn focus_ring_and_controls_are_visible() {
        for p in [&DARK, &LIGHT] {
            for (name, bg) in named(p).into_iter().filter(|(n, _)| *n != "selection") {
                assert!(
                    ratio(p.focus, bg) >= 3.0,
                    "focus ring on {name} (dark: {})",
                    p.dark
                );
            }
            assert!(
                ratio(p.control_off, p.surface) >= 3.0,
                "switch track (dark: {})",
                p.dark
            );
        }
    }

    /// Every place the brand accent is drawn, against what is behind it. Text (links, accent
    /// labels) needs 4.5:1; marks that are not text (focus ring, cursor, progress, spinner,
    /// active-row marker, switch track, control outline) need 3:1.
    #[test]
    fn the_accent_is_legible_wherever_it_is_drawn() {
        for p in [&DARK, &LIGHT] {
            let who = if p.dark { "dark" } else { "light" };
            // Accent text and links, on the page backgrounds and on the selected row.
            for (name, bg) in [
                ("canvas", p.canvas),
                ("surface", p.surface),
                ("sidebar", p.sidebar),
                ("sunken", p.sunken),
                ("selection", p.selection),
            ] {
                assert!(ratio(p.accent, bg) >= 4.5, "accent text on {name} ({who})");
            }
            // Marks that are not text, on every background (progress track is `sunken`, the
            // active-row marker and its icon sit on the sidebar and the selected row).
            for (name, bg) in [
                ("canvas", p.canvas),
                ("surface", p.surface),
                ("sidebar", p.sidebar),
                ("sunken", p.sunken),
                ("selection", p.selection),
                ("surface_hover", p.surface_hover),
                ("surface_pressed", p.surface_pressed),
            ] {
                assert!(ratio(p.focus, bg) >= 3.0, "focus ring on {name} ({who})");
                assert!(ratio(p.accent, bg) >= 3.0, "accent mark on {name} ({who})");
            }
            // Badges: the accent's text colour on its own wash, over both backgrounds.
            for bg in [p.canvas, p.surface, p.sidebar] {
                let wash = over(bg, p.tone_soft(Tone::Accent));
                assert!(ratio(p.accent_text, wash) >= 4.5, "accent badge ({who})");
            }
            // The selected row's name and second line.
            assert!(ratio(p.text, p.selection) >= 7.0, "row name ({who})");
            assert!(
                ratio(p.text_muted, p.selection) >= 4.5,
                "row sub line ({who})"
            );
            // The primary button: its fill is a control, so it stands out from the page (3:1),
            // in rest and hover, and the white label reads on every state.
            for (name, fill) in [
                ("rest", p.fill_accent),
                ("hover", p.fill_accent_hover),
                ("pressed", p.fill_accent_pressed),
            ] {
                assert!(ratio(p.on_accent, fill) >= 4.5, "label on {name} ({who})");
                // The pressed shade lasts while the pointer is down: the label is what matters.
                if name != "pressed" {
                    for bg in [p.canvas, p.surface, p.sidebar] {
                        assert!(ratio(fill, bg) >= 3.0, "button {name} on a page ({who})");
                    }
                }
            }
            // The switch's "on" track mixes towards the primary fill.
            assert!(ratio(p.fill_accent, p.surface) >= 3.0, "switch on ({who})");
        }
    }

    /// The accent reaches egui's own widgets through the style: links, the text cursor, the
    /// selection (its fill, its outline and the text on it), the hovered outline of a control
    /// (not less visible than the neutral one) and the outline of the pressed or focused one.
    #[test]
    fn the_accent_reaches_the_style_legibly() {
        for p in [&DARK, &LIGHT] {
            let who = if p.dark { "dark" } else { "light" };
            let v = style_for(p).visuals;
            assert_eq!(v.selection.bg_fill, p.selection, "selection fill ({who})");
            assert!(ratio(v.hyperlink_color, p.canvas) >= 4.5, "link ({who})");
            assert!(ratio(v.hyperlink_color, p.surface) >= 4.5, "link ({who})");
            for bg in [p.sunken, p.surface, p.canvas] {
                assert!(
                    ratio(v.text_cursor.stroke.color, bg) >= 3.0,
                    "cursor ({who})"
                );
                assert!(
                    ratio(v.widgets.active.bg_stroke.color, bg) >= 3.0,
                    "focus outline ({who})"
                );
            }
            assert!(ratio(v.selection.stroke.color, v.selection.bg_fill) >= 3.0);
            assert!(
                ratio(p.text, v.selection.bg_fill) >= 7.0,
                "selected text ({who})"
            );
            for bg in [p.surface, p.canvas, p.sunken] {
                assert!(
                    ratio(v.widgets.hovered.bg_stroke.color, bg)
                        >= ratio(p.border_strong, bg) - 0.01,
                    "hovered outline keeps its contrast ({who})"
                );
            }
        }
    }

    /// Blue-violet is the accent only: the surfaces and the text stay Graphite + Frost (nearly
    /// neutral greys with a faint cool cast), never tinted by the brand colour.
    #[test]
    fn surfaces_and_text_stay_neutral() {
        for p in [&DARK, &LIGHT] {
            for (name, c) in [
                ("canvas", p.canvas),
                ("surface", p.surface),
                ("sidebar", p.sidebar),
                ("text", p.text),
            ] {
                let spread = c.r().max(c.g()).max(c.b()) - c.r().min(c.g()).min(c.b());
                assert!(spread <= 16, "{name} is a neutral grey (dark: {})", p.dark);
            }
        }
    }

    /// The white letter of an avatar is big (16 pt and up, semibold): 3:1 on every hue.
    #[test]
    fn avatar_letters_are_readable() {
        for c in crate::ui::components::surface::AVATAR_HUES {
            assert!(ratio(Color32::WHITE, c) >= 3.0, "{c:?}");
        }
    }

    #[test]
    fn defaults_output_is_read_as_on_or_off() {
        assert!(defaults_says_on("1\n"));
        assert!(defaults_says_on(" true "));
        assert!(!defaults_says_on("0\n"));
        assert!(!defaults_says_on(""));
        assert!(!defaults_says_on("The domain/default pair does not exist"));
    }

    /// The setting is read again when the window comes back to the front, not on every frame.
    #[test]
    fn motion_is_reread_only_when_the_focus_returns() {
        assert!(should_reread_motion(false, true));
        assert!(!should_reread_motion(true, true));
        assert!(!should_reread_motion(true, false));
        assert!(!should_reread_motion(false, false));
    }

    /// Columns follow the room: two by two at the smallest window, four across on a wide one.
    #[test]
    fn columns_follow_the_width() {
        // The content area of the smallest window.
        assert_eq!(layout::bottle_columns(520.0), 2);
        assert_eq!(layout::bottle_columns(860.0), 2);
        assert_eq!(layout::bottle_columns(861.0), 3);
        assert_eq!(layout::bottle_columns(300.0), 1);
    }

    /// Cut text always fits, keeps its start and says it was cut; short text is left alone.
    #[test]
    fn fitting_text_cuts_to_the_measured_width() {
        let measure = |s: &str| s.chars().count() as f32 * 10.0;
        assert_eq!(layout::fit_text("Steam", 100.0, measure), "Steam");
        let cut = layout::fit_text("Rockstar Games Launcher", 100.0, measure);
        assert!(measure(&cut) <= 100.0, "{cut}");
        assert!(cut.ends_with('…') && cut.starts_with("Rockstar"), "{cut}");
        // Accents are not split in the middle.
        let cut = layout::fit_text("Configuración avanzada", 80.0, measure);
        assert!(measure(&cut) <= 80.0, "{cut}");
        // No room at all: only the ellipsis, never a panic.
        assert_eq!(layout::fit_text("Steam", 0.0, measure), "…");
        assert_eq!(layout::fit_text("", 0.0, measure), "");
    }

    #[test]
    fn mixing_reaches_both_ends() {
        let (a, b) = (Color32::BLACK, Color32::WHITE);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        assert_eq!(mix(a, b, 2.0), b);
    }

    /// What each state of a bottle looks like: the colour and the symbol. This table was written
    /// against the code that gave each state its look directly, and it stays the same after the
    /// state only says what it means and the window picks the look. The symbol is pinned apart
    /// from the colour.
    #[test]
    fn every_state_of_a_bottle_keeps_its_colour_and_symbol() {
        use crate::domain::status::{Attention, BottleStatus as S};
        let table = [
            (S::Creating, Tone::Accent, Icon::Refresh),
            (S::Busy("Copiando…".into()), Tone::Accent, Icon::Refresh),
            (S::Installing, Tone::Accent, Icon::Refresh),
            (S::Starting("Steam".into()), Tone::Accent, Icon::Refresh),
            (S::Running(vec!["Steam".into()]), Tone::Success, Icon::Play),
            (S::Running(vec![]), Tone::Success, Icon::Play),
            (S::InUse, Tone::Success, Icon::Play),
            (
                S::NeedsAttention(Attention::Incomplete),
                Tone::Warning,
                Icon::Alert,
            ),
            (
                S::NeedsAttention(Attention::EngineMissing),
                Tone::Warning,
                Icon::Alert,
            ),
            (
                S::NeedsAttention(Attention::Failed),
                Tone::Danger,
                Icon::Alert,
            ),
            (S::Ready, Tone::Neutral, Icon::Check),
        ];
        for (status, tone, icon) in table {
            assert_eq!(status_tone(&status), tone, "{status:?}");
            assert_eq!(status_icon(&status), icon, "{status:?}");
        }
    }
}
