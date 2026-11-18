//! The sidebar's rows: a place (icon) or a bottle (letter), with the selection easing in and
//! out as it moves.

use super::a11y;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{self, mix, motion, pal, radius, size, text, Weight};
use eframe::egui::{pos2, vec2, Color32, CursorIcon, Rect, Response, Sense, TextureHandle, Ui};

pub enum Leading<'a> {
    Icon(Icon),
    /// A bottle: its letter, or the picture of its program when there is one.
    Avatar(&'a str, Option<&'a TextureHandle>),
}

/// What a row shows at its right end.
#[derive(Clone, Copy)]
pub enum Trailing {
    Nothing,
    /// A dot: something is running, or needs a look.
    Dot(Color32),
    /// A turning arc: the row's bottle is being worked on.
    Spinner,
}

/// One row. `sub` adds a second line (state of a bottle); `trailing` marks what is going on;
/// `tip` is a longer help (empty for none). When the name or the second line is cut to fit, the
/// tooltip (and the focus tooltip, and the accessible description) carries the full text.
pub fn nav_item(
    ui: &mut Ui,
    leading: Leading<'_>,
    label: &str,
    sub: Option<&str>,
    selected: bool,
    trailing: Trailing,
    tip: &str,
) -> Response {
    let p = pal(ui);
    let h = if sub.is_some() { 42.0 } else { 30.0 };
    // A row whose second line has nothing to say keeps the two-line height (the rows of a list
    // all match) with its name in the middle.
    let sub_text = sub.filter(|s| !s.is_empty());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let mut cut = false;
    if ui.is_rect_visible(rect) {
        let sel = theme::anim_bool(ui.ctx(), resp.id.with("sel"), selected, motion::BASE);
        let hover = theme::anim_bool(
            ui.ctx(),
            resp.id.with("hover"),
            resp.hovered() && !selected,
            motion::FAST,
        );
        let painter = ui.painter();
        if hover > 0.0 {
            painter.rect_filled(rect, radius::MD, p.hover_wash.gamma_multiply(hover * 1.5));
        }
        if sel > 0.0 {
            painter.rect_filled(rect, radius::MD, p.selection.gamma_multiply(sel));
            // The marker grows from the middle of the row.
            let bar_h = (h - 16.0) * sel;
            let bar =
                Rect::from_center_size(pos2(rect.left() + 1.5, rect.center().y), vec2(3.0, bar_h));
            painter.rect_filled(bar, 2, p.accent.gamma_multiply(sel));
        }
        if resp.has_focus() {
            a11y::focus_ring(painter, rect.expand(1.0), radius::MD + 1, p);
        }
        let lead = if sub.is_some() { 26.0 } else { 16.0 };
        let lead_rect = Rect::from_center_size(
            pos2(rect.left() + 12.0 + lead / 2.0, rect.center().y),
            vec2(lead, lead),
        );
        match leading {
            Leading::Icon(icon) => {
                icons::paint(painter, lead_rect, icon, mix(p.text_muted, p.accent, sel))
            }
            Leading::Avatar(name, picture) => {
                super::surface::paint_bottle_face(painter, lead_rect, name, picture)
            }
        }
        let x = lead_rect.right() + 10.0;
        let right = rect.right()
            - if matches!(trailing, Trailing::Nothing) {
                8.0
            } else {
                24.0
            };
        let weight = if selected {
            Weight::Semibold
        } else {
            Weight::Medium
        };
        let label_y = if sub_text.is_some() {
            rect.top() + 7.0
        } else {
            rect.center().y - 8.0
        };
        // Cut to the measured room, not to a guess of how many letters fit.
        let (name, name_cut) = super::surface::fit_galley_cut(
            painter,
            label,
            theme::font(size::BODY, weight),
            p.text,
            right - x,
        );
        cut |= name_cut;
        painter.galley(pos2(x, label_y), name, p.text);
        if let Some(sub) = sub_text {
            let (sub, sub_cut) = super::surface::fit_galley_cut(
                painter,
                sub,
                theme::font(size::CAPTION, Weight::Regular),
                p.text_muted,
                right - x,
            );
            cut |= sub_cut;
            painter.galley(pos2(x, rect.top() + 24.0), sub, p.text_muted);
        }
        let end = pos2(rect.right() - 13.0, rect.center().y);
        match trailing {
            Trailing::Nothing => {}
            Trailing::Dot(c) => super::surface::paint_dot(painter, end, c),
            Trailing::Spinner => {
                super::feedback::paint_spinner(
                    ui,
                    painter,
                    Rect::from_center_size(end, vec2(12.0, 12.0)),
                    p.accent,
                );
            }
        }
    }
    // A row is a button that is selected when it marks the page the person is on (not a check
    // box). Its name carries the second line, the state, in words.
    let (name, help) = name_and_help(label, sub, selected, cut, tip);
    a11y::button_info(&resp, &name, true, Some(selected), Some(&help));
    if !help.is_empty() {
        let resp = resp.on_hover_text(&help);
        a11y::tip_on_focus(&resp, &help);
        return resp;
    }
    resp
}

/// The accessible name of a row and its help text. The name says «página actual» for the
/// selected row; the help (tooltip and description) carries the full text of a cut row without
/// that marker, because sighted people read the tooltip and the name is already spoken.
fn name_and_help(
    label: &str,
    sub: Option<&str>,
    selected: bool,
    cut: bool,
    tip: &str,
) -> (String, String) {
    let state_name = a11y::name_with_state(label, sub);
    let help = a11y::joined_tip(cut.then_some(state_name.as_str()), tip);
    (a11y::current_page_name(&state_name, selected), help)
}

/// The words of a second line, joined with « · »; the parts with nothing to say are left out.
pub fn sub_line(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|p| !p.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A small heading between groups of rows.
pub fn nav_section(ui: &mut Ui, title: &str) {
    let p = pal(ui);
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        a11y::heading(
            &ui.label(text(title, size::CAPTION, Weight::Semibold, p.text_faint)),
            2,
        );
    });
    ui.add_space(2.0);
}

#[cfg(test)]
mod tests {
    use super::{name_and_help, sub_line};

    /// Empty parts leave no stray separator: «Windows 10» alone, not « · Windows 10».
    #[test]
    fn a_sub_line_skips_the_parts_with_nothing_to_say() {
        assert_eq!(sub_line(&["", "Windows 10"]), "Windows 10");
        assert_eq!(
            sub_line(&["Steam y launchers", "En uso"]),
            "Steam y launchers · En uso"
        );
        assert_eq!(sub_line(&["", ""]), "");
    }

    /// The tooltip of a cut, selected row repeats its words, never the screen-reader marker.
    #[test]
    fn the_tooltip_of_the_current_row_has_no_page_marker() {
        let (name, help) = name_and_help("Nombre largo", Some("Auto · Lista"), true, true, "");
        assert_eq!(name, "Nombre largo, Auto · Lista, página actual");
        assert_eq!(help, "Nombre largo, Auto · Lista");
        let (name, help) = name_and_help("Inicio", None, true, false, "Ayuda");
        assert_eq!(name, "Inicio, página actual");
        assert_eq!(help, "Ayuda");
    }
}
