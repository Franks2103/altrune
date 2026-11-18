//! Surfaces: cards, section headings, badges, status dots, letter avatars, and the clickable
//! tiles for bottles and programs (they lift a little under the pointer).

use super::a11y;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{self, mix, motion, pal, radius, size, space, text, Palette, Tone, Weight};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CursorIcon, Margin, Painter, Rect, Response, Sense, Stroke,
    StrokeKind, TextureHandle, Ui,
};

/// The frame of a card.
pub fn card_frame(p: &Palette) -> egui::Frame {
    egui::Frame::NONE
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(radius::XL)
        .inner_margin(Margin::symmetric(16, 4))
}

/// A card holding `add`, as wide as the space it is in.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal(ui);
    card_frame(p)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A section title with an optional caption and controls at its right.
pub fn section(ui: &mut Ui, title: &str, caption: &str, trailing: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        a11y::heading(
            &ui.label(text(title, size::TITLE, Weight::Semibold, p.text)),
            2,
        );
        if !caption.is_empty() {
            ui.label(text(caption, size::SMALL, Weight::Regular, p.text_faint));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), trailing);
    });
    ui.add_space(space::S);
}

/// A group of widgets at the right end of a row, created left to right.
///
/// A right-to-left layout creates its widgets in the reverse of the order they appear in, and
/// Tab follows the order of creation, so keyboard focus would run against what the eye sees.
/// Here the group is laid out left to right, pushed to the right edge by the width it had in
/// the previous frame (the first frame draws nothing and asks for another). `salt` tells two
/// groups of the same row apart.
pub fn trailing_row(ui: &mut Ui, salt: &str, add: impl FnOnce(&mut Ui)) {
    let id = ui.scope_id().with(("trailing-row", salt));
    let known = ui.data(|d| d.get_temp::<f32>(id));
    // A group that drew nothing needs no room.
    if let Some(width) = known.filter(|w| *w > 0.0) {
        // The layout adds its own gap after the space, which is the one the group needs.
        ui.add_space((ui.available_width() - width).max(0.0));
    }
    let shown = ui.horizontal(|ui| {
        if known.is_none() {
            ui.set_invisible();
        }
        add(ui);
    });
    let width = shown.response.rect.width();
    if known.is_none_or(|w| (w - width).abs() > 0.5) {
        ui.data_mut(|d| d.insert_temp(id, width));
        ui.ctx().request_repaint();
    }
}

/// A small pill with a word or two.
pub fn badge(ui: &mut Ui, label: &str, tone: Tone) -> Response {
    let p = pal(ui);
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        theme::font(size::CAPTION, Weight::Medium),
        Color32::PLACEHOLDER,
    );
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 14.0, 20.0), Sense::hover());
    // Small text on a wash: the tone's own text colour, which reaches 4.5:1 there.
    let fg = p.tone_text(tone);
    a11y::label_node(&resp, label);
    ui.painter().rect_filled(rect, 10, p.tone_soft(tone));
    ui.painter().galley(
        pos2(rect.left() + 7.0, rect.center().y - galley.size().y / 2.0),
        galley,
        fg,
    );
    resp
}

/// A badge that starts with a small icon, so its meaning does not rest on colour alone.
pub fn badge_icon(ui: &mut Ui, icon: Icon, label: &str, tone: Tone) -> Response {
    let p = pal(ui);
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        theme::font(size::CAPTION, Weight::Medium),
        Color32::PLACEHOLDER,
    );
    let icon_w = 12.0;
    let (rect, resp) = ui.allocate_exact_size(
        vec2(galley.size().x + icon_w + 4.0 + 14.0, 20.0),
        Sense::hover(),
    );
    let fg = p.tone_text(tone);
    a11y::label_node(&resp, label);
    ui.painter().rect_filled(rect, 10, p.tone_soft(tone));
    let icon_rect = Rect::from_center_size(
        pos2(rect.left() + 7.0 + icon_w / 2.0, rect.center().y),
        vec2(icon_w, icon_w),
    );
    icons::paint(ui.painter(), icon_rect, icon, fg);
    ui.painter().galley(
        pos2(
            rect.left() + 7.0 + icon_w + 4.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        fg,
    );
    resp
}

/// A coloured dot with a soft halo.
pub fn paint_dot(painter: &Painter, center: egui::Pos2, color: Color32) {
    painter.circle_filled(center, 6.0, color.gamma_multiply(0.22));
    painter.circle_filled(center, 3.5, color);
}

pub fn status_dot(ui: &mut Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
    paint_dot(ui.painter(), r.center(), color);
}

/// The colours of the letter tiles. The white letter on each is big and semibold, so 3:1.
pub const AVATAR_HUES: [Color32; 8] = [
    Color32::from_rgb(76, 116, 214),
    Color32::from_rgb(52, 142, 156),
    Color32::from_rgb(112, 98, 204),
    Color32::from_rgb(190, 104, 84),
    Color32::from_rgb(58, 144, 100),
    Color32::from_rgb(176, 88, 136),
    Color32::from_rgb(150, 124, 58),
    Color32::from_rgb(92, 108, 134),
];

/// A colour for a name, always the same for the same name: calm hues that read in both
/// appearances.
pub fn name_color(name: &str) -> Color32 {
    const HUES: [Color32; 8] = AVATAR_HUES;
    // FNV-1a, then the high bits mixed down: every letter of the name counts, not only the
    // last ones (names that end alike, "… (copia)", got the same colour).
    let mut h = name.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    HUES[((h >> 11) as usize) % HUES.len()]
}

/// A rounded square with the first letter of `name`.
pub fn paint_avatar(painter: &Painter, rect: Rect, name: &str) {
    let c = name_color(name);
    let r = (rect.width() * 0.26) as u8;
    painter.rect_filled(rect, r, c);
    // a faint light from above, so the tile is not flat
    let mut top = rect;
    top.set_height(rect.height() * 0.5);
    painter.rect_filled(
        top,
        egui::CornerRadius {
            nw: r,
            ne: r,
            sw: 0,
            se: 0,
        },
        Color32::from_white_alpha(18),
    );
    let letter: String = name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".into());
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        letter,
        theme::font(rect.height() * 0.46, Weight::Semibold),
        Color32::WHITE,
    );
}

/// A bottle's mark: the picture of its program when it has one and it is decoded, clipped to the
/// same rounded square as the letter; otherwise the letter square, exactly as ever.
pub fn paint_bottle_face(
    painter: &Painter,
    rect: Rect,
    name: &str,
    picture: Option<&TextureHandle>,
) {
    match picture {
        Some(t) => {
            let r = (rect.width() * 0.26) as u8;
            painter.add(
                egui::epaint::RectShape::filled(rect, r, Color32::WHITE)
                    .with_texture(t.id(), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))),
            );
        }
        None => paint_avatar(painter, rect, name),
    }
}

/// The bottle's mark as a widget of `side` points (see [`paint_bottle_face`]).
pub fn bottle_mark(ui: &mut Ui, name: &str, side: f32, picture: Option<&TextureHandle>) {
    let (r, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    paint_bottle_face(ui.painter(), r, name, picture);
}

/// `text` laid out on one line and cut with an ellipsis to fit `max_w` points.
pub fn fit_galley(
    painter: &Painter,
    text: &str,
    font: egui::FontId,
    color: Color32,
    max_w: f32,
) -> std::sync::Arc<egui::Galley> {
    fit_galley_cut(painter, text, font, color, max_w).0
}

/// Like [`fit_galley`], and says whether the text had to be cut (so the full text should be
/// offered in a tooltip and in the accessible name).
pub fn fit_galley_cut(
    painter: &Painter,
    text: &str,
    font: egui::FontId,
    color: Color32,
    max_w: f32,
) -> (std::sync::Arc<egui::Galley>, bool) {
    // Most text fits: one layout is all it costs.
    let whole = painter.layout_no_wrap(text.to_owned(), font.clone(), color);
    if whole.size().x <= max_w {
        return (whole, false);
    }
    let measure = |s: &str| {
        painter
            .layout_no_wrap(s.to_owned(), font.clone(), color)
            .size()
            .x
    };
    let shown = theme::layout::fit_text(text, max_w.max(0.0), measure);
    (painter.layout_no_wrap(shown, font, color), true)
}

/// Hover and press state of a tile, eased.
fn tile_state(ui: &Ui, resp: &Response) -> (f32, bool) {
    let hover = theme::anim_bool(ui.ctx(), resp.id.with("lift"), resp.hovered(), motion::BASE);
    (hover, resp.is_pointer_button_down_on())
}

/// The background of a tile: lifts and gains a shadow under the pointer.
fn paint_tile(ui: &Ui, rect: Rect, hover: f32, down: bool, focused: bool) -> Rect {
    let p = pal(ui);
    let lift = if down { 0.0 } else { -1.5 * hover };
    let r = rect.translate(vec2(0.0, lift));
    if hover > 0.01 {
        ui.painter().add(
            egui::epaint::Shadow {
                offset: [0, 4],
                blur: 14,
                spread: 0,
                color: p.shadow.gamma_multiply(hover),
            }
            .as_shape(r, radius::XL),
        );
    }
    let border = mix(p.border, p.border_strong, hover);
    ui.painter().rect(
        r,
        radius::XL,
        mix(p.surface, p.surface_hover, hover),
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    if focused {
        a11y::focus_ring(ui.painter(), r.expand(2.0), radius::XL + 2, p);
    }
    r
}

/// What a program tile says about its program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileState {
    /// Not open.
    Idle,
    /// Being opened: a spinner and «Abriendo…» until it is open.
    Starting,
    /// Open: a green dot and «Abierto».
    Open,
}

impl TileState {
    /// What a screen reader says after the name, so the state is words and not only a mark.
    pub fn spoken(self) -> Option<&'static str> {
        match self {
            TileState::Idle => None,
            TileState::Starting => Some("abriéndose"),
            TileState::Open => Some("abierto"),
        }
    }
}

/// A program: its own picture (or a letter), its name, and its whole state, so a tile answers
/// the press at once: from the frame of the click it shows a spinner and «Abriendo…», then a
/// dot and the word «Abierto» while it runs. A «Más opciones» button in its corner opens `menu`,
/// the same menu a secondary click opens, so the options are reachable with Tab and ↩ as well.
/// Clicking the tile itself is the returned response's `clicked()`.
pub fn program_tile_in(
    ui: &mut Ui,
    name: &str,
    subtitle: &str,
    state: TileState,
    picture: Option<&TextureHandle>,
    mut menu: impl FnMut(&mut Ui),
) -> Response {
    let running = state == TileState::Open;
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(124.0, 128.0), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let spoken = match state.spoken() {
        Some("abierto") => a11y::tile_name(name, true),
        Some(word) => format!("{name}, {word}"),
        None => a11y::tile_name(name, false),
    };
    a11y::button_info(&resp, &spoken, true, None, None);
    if ui.is_rect_visible(rect) {
        let (hover, down) = tile_state(ui, &resp);
        let r = paint_tile(ui, rect, hover, down, resp.has_focus());
        let pic = Rect::from_center_size(pos2(r.center().x, r.top() + 38.0), vec2(48.0, 48.0));
        match picture {
            Some(t) => {
                ui.painter().image(
                    t.id(),
                    pic,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => paint_avatar(ui.painter(), pic.shrink(2.0), name),
        }
        if running {
            let c = pic.right_top() + vec2(-1.0, 1.0);
            ui.painter().circle_filled(c, 6.5, p.surface);
            ui.painter().circle_filled(c, 4.5, p.success);
            // The state in words too, not only a coloured dot.
            ui.painter().text(
                pos2(r.center().x, r.bottom() - 20.0),
                Align2::CENTER_TOP,
                "Abierto",
                theme::font(size::CAPTION, Weight::Medium),
                p.success_text,
            );
        }
        if state == TileState::Starting {
            let c = pic.right_top() + vec2(-1.0, 1.0);
            ui.painter().circle_filled(c, 8.5, p.surface);
            super::feedback::paint_spinner(
                ui,
                ui.painter(),
                Rect::from_center_size(c, vec2(14.0, 14.0)),
                p.accent,
            );
            ui.painter().text(
                pos2(r.center().x, r.bottom() - 20.0),
                Align2::CENTER_TOP,
                "Abriendo…",
                theme::font(size::CAPTION, Weight::Medium),
                p.tone_text(Tone::Accent),
            );
        }
        let galley = ui.painter().layout(
            name.to_owned(),
            theme::font(size::BODY, Weight::Medium),
            p.text,
            r.width() - 16.0,
        );
        let mut y = r.top() + 72.0;
        let shown_rows = galley.rows.len().min(2);
        let galley = if galley.rows.len() > 2 {
            // keep two lines at most
            ui.painter().layout(
                format!("{}…", crate::ui::short(name, 24).trim_end_matches('…')),
                theme::font(size::BODY, Weight::Medium),
                p.text,
                r.width() - 16.0,
            )
        } else {
            galley
        };
        ui.painter().galley(
            pos2(r.center().x - galley.size().x / 2.0, y),
            galley,
            p.text,
        );
        y += 17.0 * shown_rows as f32 + 1.0;
        if !subtitle.is_empty() && shown_rows < 2 {
            ui.painter().text(
                pos2(r.center().x, y),
                Align2::CENTER_TOP,
                crate::ui::short(subtitle, 18),
                theme::font(size::CAPTION, Weight::Regular),
                p.text_faint,
            );
        }
    }
    // The corner button comes after the tile, so it is the next stop for Tab and, where the two
    // overlap, the one under the pointer.
    let more_rect = Rect::from_min_size(
        pos2(rect.right() - 28.0, rect.top() + 4.0),
        vec2(24.0, 24.0),
    );
    let more = super::button::icon_button_at(
        ui,
        more_rect,
        resp.id.with("more"),
        Icon::More,
        "Más opciones",
        &a11y::more_options_name(name),
    );
    a11y::opens_menu(&more);
    // The same menu from a secondary click on the tile and from the button.
    egui::Popup::context_menu(&resp)
        .style(a11y::menu_style(p))
        .show(&mut menu);
    egui::Popup::menu(&more)
        .style(a11y::menu_style(p))
        .show(&mut menu);
    resp
}

/// One entry of a tile's menu: a button that closes the menu when it is pressed, also with the
/// keyboard (a pointer click closes it by itself, ↩ and Space do not).
pub fn menu_item(ui: &mut Ui, label: &str) -> Response {
    let r = ui.button(label);
    if r.clicked() {
        ui.close();
    }
    r
}

/// What a bottle card shows.
pub struct BottleCard<'a> {
    pub name: &'a str,
    /// The engine's name, or empty while it is the automatic one.
    pub engine: &'a str,
    pub windows: &'a str,
    pub state: &'a str,
    pub state_tone: Tone,
    /// The bottle is being worked on: a turning arc takes the place of the dot.
    pub working: bool,
    /// A sentence for the tooltip when the state needs one (what is wrong, what to do).
    pub hint: &'a str,
    pub programs: usize,
    /// The picture of the bottle's program, once decoded; the letter shows without it.
    pub picture: Option<&'a TextureHandle>,
}

/// A bottle in the overview: letter, name, engine, Windows version, state.
pub fn bottle_card(ui: &mut Ui, c: &BottleCard<'_>) -> Response {
    let p = pal(ui);
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 92.0), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    // The parts that are cut to fit, so the full text can be offered.
    let mut cut = false;
    let mut full_meta = String::new();
    if ui.is_rect_visible(rect) {
        let (hover, down) = tile_state(ui, &resp);
        let r = paint_tile(ui, rect, hover, down, resp.has_focus());
        let av = Rect::from_min_size(r.min + vec2(14.0, 14.0), vec2(36.0, 36.0));
        paint_bottle_face(ui.painter(), av, c.name, c.picture);
        let x = av.right() + 12.0;
        // Everything is cut to the room it has, so a long name or state never runs into the
        // next thing on its line.
        let text_w = r.right() - x - 30.0;
        let (name, name_cut) = fit_galley_cut(
            ui.painter(),
            c.name,
            theme::font(size::BODY_L, Weight::Semibold),
            p.text,
            text_w,
        );
        cut |= name_cut;
        ui.painter().galley(pos2(x, av.top() + 1.0), name, p.text);
        // An engine nobody chose is not named: the line starts with the Windows version.
        let meta = super::nav::sub_line(&[c.engine, c.windows]);
        let (meta_galley, meta_cut) = fit_galley_cut(
            ui.painter(),
            &meta,
            theme::font(size::SMALL, Weight::Regular),
            p.text_muted,
            text_w,
        );
        cut |= meta_cut;
        full_meta = meta;
        ui.painter()
            .galley(pos2(x, av.top() + 21.0), meta_galley, p.text_muted);
        let n = match c.programs {
            0 => "sin programas".to_string(),
            1 => "1 programa".to_string(),
            n => format!("{n} programas"),
        };
        let count =
            ui.painter()
                .layout_no_wrap(n, theme::font(size::SMALL, Weight::Regular), p.text_faint);
        let dot_c = p.tone(c.state_tone);
        let base = pos2(r.left() + 14.0, r.bottom() - 18.0);
        // A quiet state («Lista») comes with no dot and no word: the count then starts the line.
        let count_x = if c.state.is_empty() {
            base.x
        } else {
            if c.working {
                let arc = Rect::from_center_size(base + vec2(5.0, 0.0), vec2(12.0, 12.0));
                super::feedback::paint_spinner(ui, ui.painter(), arc, dot_c);
            } else {
                paint_dot(ui.painter(), base + vec2(5.0, 0.0), dot_c);
            }
            // The state gets what the program count leaves on the line.
            let state_w = r.right() - 14.0 - count.size().x - 8.0 - (base.x + 16.0);
            let (state, state_cut) = fit_galley_cut(
                ui.painter(),
                c.state,
                theme::font(size::SMALL, Weight::Medium),
                p.text,
                state_w,
            );
            cut |= state_cut;
            ui.painter().galley(
                pos2(base.x + 16.0, base.y - state.size().y / 2.0),
                state,
                p.text,
            );
            r.right() - 14.0 - count.size().x
        };
        ui.painter().galley(
            pos2(count_x, base.y - count.size().y / 2.0),
            count,
            p.text_faint,
        );
        let chev = Rect::from_center_size(pos2(r.right() - 18.0, av.center().y), vec2(14.0, 14.0));
        icons::paint(
            ui.painter(),
            chev,
            Icon::ChevronRight,
            mix(p.text_faint, p.text_muted, hover),
        );
    }
    // The card is a button named after the bottle and its state, in words. When something was
    // cut, the whole text is in the tooltip, which also shows while the card has the focus.
    let name = a11y::name_with_state(c.name, Some(c.state));
    let full = format!("{name}\n{full_meta}");
    let help = a11y::joined_tip(cut.then_some(full.as_str()), c.hint);
    a11y::button_info(&resp, &name, true, None, Some(&help));
    if help.is_empty() {
        return resp;
    }
    let resp = resp.on_hover_text(&help);
    a11y::tip_on_focus(&resp, &help);
    resp
}

/// A number with its label, for the overview.
pub fn stat_tile(ui: &mut Ui, icon: Icon, value: &str, label: &str, tone: Tone) {
    let p = pal(ui);
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 64.0), Sense::hover());
    // The tile is painted, so VoiceOver gets its words from here: «2 botellas propias».
    a11y::label_node(&resp, &a11y::stat_name(value, label));
    ui.painter().rect(
        rect,
        radius::XL,
        p.surface,
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    let ic = Rect::from_min_size(rect.min + vec2(14.0, 14.0), vec2(36.0, 36.0));
    ui.painter().rect_filled(ic, radius::LG, p.tone_soft(tone));
    icons::paint(ui.painter(), ic.shrink(9.0), icon, p.tone(tone));
    let x = ic.right() + 12.0;
    let v = ui.painter().layout_no_wrap(
        value.to_owned(),
        theme::font(size::H3, Weight::Semibold),
        p.text,
    );
    let v = if v.size().x > rect.right() - x - 8.0 {
        ui.painter().layout_no_wrap(
            crate::ui::short(value, ((rect.right() - x) / 9.0) as usize),
            theme::font(size::BODY_L, Weight::Semibold),
            p.text,
        )
    } else {
        v
    };
    ui.painter().galley(pos2(x, ic.top() - 1.0), v, p.text);
    let label = fit_galley(
        ui.painter(),
        label,
        theme::font(size::SMALL, Weight::Regular),
        p.text_muted,
        rect.right() - x - 8.0,
    );
    let at = pos2(x, ic.bottom() + 1.0 - label.size().y);
    ui.painter().galley(at, label, p.text_muted);
}

/// A full-width hairline between rows of a card.
pub fn divider(ui: &mut Ui) {
    let p = pal(ui);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0, p.border);
}

#[cfg(test)]
mod tests {
    use super::{name_color, trailing_row};
    use eframe::egui::{self, Event, Key, Modifiers, RawInput};

    /// A row with a name on the left and `trailing_row` with two buttons on the right, run for
    /// a few frames (the first draws the group invisibly to learn its width). Returns where
    /// each button ended up and which one the first Tab press focused.
    fn run_row(
        with_trailing_row: bool,
    ) -> (egui::Rect, egui::Rect, Option<egui::Id>, [egui::Id; 2]) {
        let ctx = egui::Context::default();
        let ids = std::cell::Cell::new([egui::Id::NULL; 2]);
        let rects = std::cell::Cell::new([egui::Rect::NOTHING; 2]);
        let frame = |events: Vec<Event>| {
            let raw = RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(600.0, 200.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                ui.horizontal(|ui| {
                    ui.label("name");
                    let buttons = |ui: &mut egui::Ui| {
                        let first = ui.add(egui::Button::new("first"));
                        let second = ui.add(egui::Button::new("second"));
                        ids.set([first.id, second.id]);
                        rects.set([first.rect, second.rect]);
                    };
                    if with_trailing_row {
                        trailing_row(ui, "test", buttons);
                    } else {
                        // The old way: right to left, so the first created is the rightmost.
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            buttons(ui)
                        });
                    }
                });
            });
            out.textures_delta.clear();
        };
        frame(vec![]);
        frame(vec![]);
        frame(vec![]);
        let tab = |pressed| Event::Key {
            key: Key::Tab,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        frame(vec![tab(true)]);
        frame(vec![tab(false)]);
        let [first, second] = rects.get();
        (first, second, ctx.memory(|m| m.focused()), ids.get())
    }

    /// The group at the right end of a row reads left to right, is at the right edge, and Tab
    /// reaches its buttons in the order they appear.
    #[test]
    fn a_trailing_row_is_in_visual_order_for_the_eye_and_for_tab() {
        let (first, second, focused, ids) = run_row(true);
        assert!(first.right() <= second.left() + 0.5, "{first:?} {second:?}");
        assert!(
            second.right() > 560.0,
            "pushed to the right edge: {second:?}"
        );
        assert_eq!(focused, Some(ids[0]), "Tab reaches the left button first");
        // The right-to-left layout it replaces put the first created button at the right.
        let (a, b, focused, ids) = run_row(false);
        assert!(b.right() <= a.left() + 0.5, "{a:?} {b:?}");
        assert_eq!(focused, Some(ids[0]));
    }

    /// Bottles whose names end alike still get different colours, and a name always gets the
    /// same one.
    #[test]
    fn similar_names_get_different_colours() {
        let names = [
            "Rockstar Games Launcher (copia)",
            "Steam (copia)",
            "Rockstar Games Launcher",
            "Steam",
        ];
        let colours: std::collections::HashSet<_> = names.iter().map(|n| name_color(n)).collect();
        assert_eq!(colours.len(), names.len(), "{colours:?}");
        assert_eq!(name_color("Steam"), name_color("Steam"));
    }
}
