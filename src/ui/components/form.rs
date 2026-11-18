//! Controls: the on/off switch, the search field, segmented tabs and settings rows.

use super::a11y;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{self, mix, motion, pal, radius, size, space, text, Weight};
use eframe::egui::{
    self, pos2, vec2, Color32, CursorIcon, Margin, Rect, Response, Sense, Stroke, StrokeKind, Ui,
};

/// An on/off switch, eased between its two positions. `label` is the setting's name, which
/// assistive technology reads out.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    toggle_held(ui, on, label, None)
}

/// [`toggle`] that may be unavailable right now: with `Some(why)` it is dimmed, does nothing
/// when pressed, and `why` is its tooltip and its accessible description. It still takes the
/// keyboard focus, as a disabled button does, so Tab and a screen reader reach the reason (a
/// control inside a disabled `Ui` is skipped by both).
pub fn toggle_held(ui: &mut Ui, on: &mut bool, label: &str, held: Option<&str>) -> Response {
    let p = pal(ui);
    let sense = if held.is_some() {
        Sense::focusable_noninteractive()
    } else {
        Sense::click()
    };
    let (rect, mut resp) = ui.allocate_exact_size(vec2(38.0, 22.0), sense);
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let enabled = ui.is_enabled() && held.is_none();
    let now = *on;
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Switch, enabled, now, label));
    if let Some(why) = held {
        a11y::describe(&resp, why);
    }
    if ui.is_rect_visible(rect) {
        let t = theme::anim_bool(ui.ctx(), resp.id, *on, motion::BASE);
        let mut painter = ui.painter().clone();
        if !enabled {
            painter.multiply_opacity(0.45);
        }
        painter.rect_filled(rect, 11, mix(p.control_off, p.fill_accent, t));
        let x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), t);
        painter.circle_filled(pos2(x, rect.center().y + 0.5), 9.0, p.shadow);
        painter.circle_filled(pos2(x, rect.center().y), 9.0, Color32::WHITE);
        if resp.has_focus() {
            a11y::focus_ring(ui.painter(), rect.expand(2.5), 13, p);
        }
    }
    match held {
        Some(why) => {
            a11y::tip_on_focus(&resp, why);
            resp.on_hover_text(why)
        }
        None => resp.on_hover_cursor(CursorIcon::PointingHand),
    }
}

/// The width a search field adds around the width it is given: its side margins and the stroke.
pub const SEARCH_FIELD_CHROME: f32 = 18.0;

/// A search field with its magnifier and a button to clear it. ⌘F puts the cursor in it.
pub fn search_field(ui: &mut Ui, value: &mut String, hint: &str, width: f32) -> Response {
    let p = pal(ui);
    let id = ui.scope_id().with("search");
    let mut out = None;
    egui::Frame::NONE
        .fill(p.sunken)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(radius::MD)
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                icons::paint(ui.painter(), r, Icon::Search, p.text_faint);
                let clear_w = if value.is_empty() { 0.0 } else { 22.0 };
                let edit = egui::TextEdit::singleline(value)
                    .id(id)
                    .hint_text(text(hint, size::BODY, Weight::Regular, p.text_faint))
                    .frame(egui::Frame::NONE)
                    .margin(Margin::symmetric(2, 2))
                    .align(egui::Align2::LEFT_CENTER)
                    .desired_width(ui.available_width() - clear_w);
                let r = ui.add(edit).accessible_name(hint);
                if !value.is_empty()
                    && super::button::icon_button(ui, Icon::Close, "Borrar la búsqueda").clicked()
                {
                    value.clear();
                }
                out = Some(r);
            });
        });
    if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
        ui.memory_mut(|m| m.request_focus(id));
    }
    out.expect("the field is always drawn")
}

/// Tabs in a well: the marker slides to the chosen one. Returns the new choice when it changes.
/// `tips` holds a tooltip for each tab (an empty one shows nothing).
pub fn segmented(
    ui: &mut Ui,
    id: egui::Id,
    labels: &[String],
    tips: &[&str],
    current: usize,
) -> Option<usize> {
    let p = pal(ui);
    let font = theme::font(size::BODY, Weight::Medium);
    let widths: Vec<f32> = labels
        .iter()
        .map(|l| {
            ui.painter()
                .layout_no_wrap(l.clone(), font.clone(), Color32::PLACEHOLDER)
                .size()
                .x
                + 28.0
        })
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + 4.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, 30.0), Sense::hover());
    ui.painter().rect_filled(rect, radius::MD, p.sunken);
    let mut x = rect.left() + 2.0;
    let mut slots = vec![];
    for w in &widths {
        slots.push(Rect::from_min_size(
            pos2(x, rect.top() + 2.0),
            vec2(*w, 26.0),
        ));
        x += w;
    }
    // The marker eases to the chosen slot.
    let target = slots.get(current).copied().unwrap_or(slots[0]);
    // A row that measures its control first draws it once unseen, at the wrong place: the
    // marker must not start its slide from there, so that pass leaves the animation alone.
    let (left, width) = if ui.is_visible() {
        (
            theme::anim_value(ui.ctx(), id.with("x"), target.left(), motion::BASE),
            theme::anim_value(ui.ctx(), id.with("w"), target.width(), motion::BASE),
        )
    } else {
        (target.left(), target.width())
    };
    let marker = Rect::from_min_size(pos2(left, target.top()), vec2(width, target.height()));
    if !p.dark {
        ui.painter()
            .rect_filled(marker.translate(vec2(0.0, 1.0)), radius::SM, p.shadow);
    }
    ui.painter().rect(
        marker,
        radius::SM,
        if p.dark { p.surface_pressed } else { p.surface },
        Stroke::new(1.0, if p.dark { p.border_strong } else { p.border }),
        StrokeKind::Inside,
    );
    let mut chosen = None;
    // ← and → on a focused tab choose the one next to it and carry the focus there.
    let mut step = arrow_step(ui);
    for (i, (slot, label)) in slots.iter().zip(labels).enumerate() {
        let mut resp = ui
            .interact(*slot, id.with(i), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if let Some(tip) = tips.get(i).filter(|t| !t.is_empty()) {
            resp = resp.on_hover_text(*tip);
        }
        // The tabs are tabs, with the chosen one marked selected (not check boxes).
        a11y::node_info(
            &resp,
            egui::Role::Tab,
            label,
            true,
            Some(i == current),
            tips.get(i).copied(),
        );
        if let Some(tip) = tips.get(i) {
            a11y::tip_on_focus(&resp, tip);
        }
        let hover = theme::anim_bool(ui.ctx(), resp.id.with("h"), resp.hovered(), motion::FAST);
        let color = if i == current {
            p.text
        } else {
            mix(p.text_muted, p.text, hover)
        };
        ui.painter().text(
            slot.center(),
            egui::Align2::CENTER_CENTER,
            label,
            font.clone(),
            color,
        );
        if resp.has_focus() {
            a11y::focus_ring(ui.painter(), slot.expand(1.0), radius::SM, p);
            // The arrows belong to the tabs while one has the focus; without this egui would
            // also move the focus to whatever lies in that direction.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    resp.id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        ..Default::default()
                    },
                )
            });
            if step != 0 {
                let next = next_tab(i, step, slots.len());
                ui.memory_mut(|m| m.request_focus(id.with(next)));
                chosen = Some(next);
                // The focus has moved on to a tab later in this loop: one press, one step.
                step = 0;
            }
        }
        if resp.clicked() && i != current {
            chosen = Some(i);
        }
    }
    chosen
}

/// −1 when ← was pressed this frame, 1 for →, 0 for neither.
fn arrow_step(ui: &Ui) -> i32 {
    ui.input(|i| {
        if i.modifiers.any() {
            0
        } else {
            i32::from(i.key_pressed(egui::Key::ArrowRight))
                - i32::from(i.key_pressed(egui::Key::ArrowLeft))
        }
    })
}

/// The tab an arrow leads to from tab `from` of `count`: the row is a ring, so → on the last
/// tab goes to the first and ← on the first goes to the last.
fn next_tab(from: usize, step: i32, count: usize) -> usize {
    (from as i64 + i64::from(step)).rem_euclid(count as i64) as usize
}

/// A setting: its name and a short explanation on the left, its control on the right.
pub fn setting_row(ui: &mut Ui, title: &str, help: &str, control: impl FnOnce(&mut Ui)) {
    setting_row_wide(ui, title, help, 190.0, control);
}

/// A setting whose control needs `control_w` points: the text on the left leaves that much room.
pub fn setting_row_wide(
    ui: &mut Ui,
    title: &str,
    help: &str,
    control_w: f32,
    control: impl FnOnce(&mut Ui),
) {
    let p = pal(ui);
    ui.add_space(space::M);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width((ui.available_width() - control_w).max(160.0));
            ui.label(text(title, size::BODY, Weight::Medium, p.text));
            if !help.is_empty() {
                ui.add_space(1.0);
                ui.label(text(help, size::SMALL, Weight::Regular, p.text_muted));
            }
        });
        // Created left to right (pushed to the right edge), so Tab and VoiceOver visit two
        // controls of one row in the order the eye reads them.
        super::surface::trailing_row(ui, title, control);
    });
    ui.add_space(space::M);
}

/// A titled group of settings in one card, rows separated by hairlines (call `divider`).
pub fn settings_group(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    a11y::heading(
        &ui.label(text(title, size::SMALL, Weight::Semibold, p.text_muted)),
        2,
    );
    ui.add_space(space::XS + 2.0);
    super::surface::card(ui, add);
    ui.add_space(space::XL);
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Event, Key, Modifiers, RawInput};

    #[test]
    fn arrows_walk_a_ring_of_tabs() {
        assert_eq!(next_tab(0, 1, 3), 1);
        assert_eq!(next_tab(2, 1, 3), 0);
        assert_eq!(next_tab(0, -1, 3), 2);
        assert_eq!(next_tab(1, -1, 3), 0);
    }

    struct Page {
        ctx: egui::Context,
        current: usize,
        time: f64,
    }

    impl Page {
        fn new() -> Self {
            let ctx = egui::Context::default();
            theme::install_for_tests(&ctx);
            Self {
                ctx,
                current: 0,
                time: 0.0,
            }
        }

        /// One frame with `keys` pressed; the choice the tabs report is applied, as the page does.
        fn frame(&mut self, keys: &[Key]) -> Option<usize> {
            self.time += 0.1;
            let mut raw = RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    vec2(600.0, 300.0),
                )),
                time: Some(self.time),
                ..Default::default()
            };
            for key in keys {
                raw.events.push(Event::Key {
                    key: *key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                });
            }
            let labels = [
                "Programas".to_string(),
                "Configuración".into(),
                "Diagnóstico".into(),
            ];
            let current = self.current;
            let mut chosen = None;
            let mut out = self.ctx.run_ui(raw, |ui| {
                chosen = segmented(ui, egui::Id::NULL.with("tabs"), &labels, &[], current);
            });
            out.textures_delta.clear();
            if let Some(i) = chosen {
                self.current = i;
            }
            chosen
        }

        fn focused_tab(&self) -> Option<usize> {
            let focused = self.ctx.memory(|m| m.focused())?;
            (0..3).find(|i| egui::Id::NULL.with("tabs").with(i) == focused)
        }
    }

    /// With a tab focused, → and ← choose the tab beside it and the focus goes with the choice.
    #[test]
    fn arrows_choose_the_next_tab_and_take_the_focus() {
        let mut page = Page::new();
        page.frame(&[]);
        page.frame(&[Key::Tab]);
        page.frame(&[]);
        page.frame(&[]);
        assert_eq!(page.focused_tab(), Some(0));
        assert_eq!(page.frame(&[Key::ArrowRight]), Some(1));
        page.frame(&[]);
        assert_eq!(page.focused_tab(), Some(1));
        assert_eq!(page.frame(&[Key::ArrowRight]), Some(2));
        page.frame(&[]);
        assert_eq!(page.focused_tab(), Some(2));
        // The ring closes.
        assert_eq!(page.frame(&[Key::ArrowRight]), Some(0));
        page.frame(&[]);
        assert_eq!(page.focused_tab(), Some(0));
        assert_eq!(page.frame(&[Key::ArrowLeft]), Some(2));
        page.frame(&[]);
        assert_eq!(page.focused_tab(), Some(2));
        assert_eq!(page.current, 2);
    }

    /// The arrows do nothing when no tab has the focus.
    #[test]
    fn arrows_leave_the_tabs_alone_without_the_focus() {
        let mut page = Page::new();
        page.frame(&[]);
        assert_eq!(page.frame(&[Key::ArrowRight]), None);
        assert_eq!(page.current, 0);
    }

    /// A row that first draws its control unseen and somewhere else (to measure it) does not
    /// make the marker slide in from there: it is already at the chosen tab in the first
    /// frame that shows it.
    #[test]
    fn the_marker_does_not_slide_in_from_the_unseen_measuring_pass() {
        fn marker_left(shapes: &[egui::epaint::ClippedShape]) -> Option<f32> {
            fn find(shape: &egui::Shape) -> Option<f32> {
                match shape {
                    egui::Shape::Rect(r) if r.fill == theme::DARK.surface_pressed => {
                        Some(r.rect.left())
                    }
                    egui::Shape::Vec(v) => v.iter().find_map(find),
                    _ => None,
                }
            }
            shapes.iter().find_map(|c| find(&c.shape))
        }
        let ctx = egui::Context::default();
        theme::install_for_tests(&ctx);
        let labels = ["Uno".to_string(), "Dos".into(), "Tres".into()];
        let mut time = 0.0;
        let mut run = |unseen_offset: Option<f32>| {
            time += 0.05;
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    vec2(600.0, 300.0),
                )),
                time: Some(time),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                ui.horizontal(|ui| {
                    if let Some(x) = unseen_offset {
                        ui.add_space(x);
                        ui.set_invisible();
                    }
                    segmented(ui, egui::Id::NULL.with("tabs"), &labels, &[], 1);
                });
            });
            out.textures_delta.clear();
            marker_left(&out.shapes)
        };
        // The unseen pass sits far to the right; the next frame shows it at the left edge.
        run(Some(300.0));
        let first = run(None).expect("the marker is drawn");
        let mut settled = first;
        for _ in 0..20 {
            settled = run(None).unwrap();
        }
        assert!(
            (first - settled).abs() < 0.5,
            "the marker started at {first} and settled at {settled}"
        );
    }
}
