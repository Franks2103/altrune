//! Buttons: primary (the one important action on screen), secondary, ghost (quiet, in toolbars)
//! and destructive. Each has hover, pressed, disabled, loading and keyboard-focus states.

use super::a11y;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{self, mix, motion, pal, radius, size, Weight};
use eframe::egui::{self, vec2, Color32, CursorIcon, Rect, Response, Sense, Ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

#[must_use = "call .show(ui)"]
pub struct Button<'a> {
    label: &'a str,
    kind: Kind,
    icon: Option<Icon>,
    large: bool,
    enabled: bool,
    loading: bool,
    full_width: bool,
    left_aligned: bool,
    tooltip: Option<&'a str>,
    disabled_reason: Option<&'a str>,
}

impl<'a> Button<'a> {
    fn new(label: &'a str, kind: Kind) -> Self {
        Self {
            label,
            kind,
            icon: None,
            large: false,
            enabled: true,
            loading: false,
            full_width: false,
            left_aligned: false,
            tooltip: None,
            disabled_reason: None,
        }
    }
    pub fn primary(label: &'a str) -> Self {
        Self::new(label, Kind::Primary)
    }
    pub fn secondary(label: &'a str) -> Self {
        Self::new(label, Kind::Secondary)
    }
    pub fn ghost(label: &'a str) -> Self {
        Self::new(label, Kind::Ghost)
    }
    pub fn danger(label: &'a str) -> Self {
        Self::new(label, Kind::Danger)
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn large(mut self) -> Self {
        self.large = true;
        self
    }
    /// Shows a spinner and takes no clicks while some work it started is running. The button
    /// keeps the keyboard focus meanwhile.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }
    /// Content at the left edge (for list-like buttons).
    pub fn left(mut self) -> Self {
        self.left_aligned = true;
        self
    }
    pub fn tooltip(mut self, text: &'a str) -> Self {
        self.tooltip = Some(text);
        self
    }
    /// With `Some(why)` the button is disabled and `why` is its tooltip, so a grey button always
    /// says what to do about it. The reason is also the button's accessible description, and the
    /// button can be reached with Tab (it shows the reason while focused) so that people who do
    /// not use a mouse learn it too. With `None` the button works.
    pub fn disabled_reason(mut self, why: Option<&'a str>) -> Self {
        self.disabled_reason = why;
        self
    }

    pub fn show(mut self, ui: &mut Ui) -> Response {
        if self.disabled_reason.is_some() {
            self.enabled = false;
        }
        let p = pal(ui);
        let height = if self.large { 34.0 } else { 28.0 };
        let pad = if self.large { 16.0 } else { 12.0 };
        let text_size = if self.large { size::BODY_L } else { size::BODY };
        let galley = ui.painter().layout_no_wrap(
            self.label.to_owned(),
            theme::font(text_size, Weight::Medium),
            Color32::PLACEHOLDER,
        );
        let glyph = if self.large { 16.0 } else { 14.0 };
        let has_glyph = self.icon.is_some() || self.loading;
        let gap = if has_glyph && !self.label.is_empty() {
            6.0
        } else {
            0.0
        };
        let content_w = galley.size().x + if has_glyph { glyph + gap } else { 0.0 };
        let width = if self.full_width {
            ui.available_width()
        } else {
            content_w + pad * 2.0
        };
        let active = self.enabled && !self.loading;
        // A button that cannot be pressed right now (greyed out, or busy with the work it started)
        // still takes the keyboard focus and does nothing when pressed. Dropping the focus the
        // moment the work starts would send the next Tab back to the top of the page, and a
        // disabled button's reason is only reachable without a mouse if Tab stops on it.
        let sense = if active {
            Sense::click()
        } else {
            Sense::focusable_noninteractive()
        };
        let (rect, resp) = ui.allocate_exact_size(vec2(width, height), sense);
        if ui.is_rect_visible(rect) {
            let hover = theme::anim_bool(
                ui.ctx(),
                resp.id.with("hover"),
                active && resp.hovered(),
                motion::FAST,
            );
            let down = active && resp.is_pointer_button_down_on();
            let (base, over, pressed, fg, border) = match self.kind {
                Kind::Primary => (
                    p.fill_accent,
                    p.fill_accent_hover,
                    p.fill_accent_pressed,
                    p.on_accent,
                    primary_hover_outline(p, hover),
                ),
                Kind::Secondary => (
                    p.surface,
                    p.surface_hover,
                    p.surface_pressed,
                    p.text,
                    p.border_strong,
                ),
                Kind::Ghost => (
                    Color32::TRANSPARENT,
                    p.hover_wash,
                    p.hover_wash.gamma_multiply(2.0),
                    mix(p.text_muted, p.text, hover),
                    Color32::TRANSPARENT,
                ),
                Kind::Danger => (
                    p.fill_danger,
                    p.fill_danger_hover,
                    p.fill_danger_pressed,
                    p.on_accent,
                    Color32::TRANSPARENT,
                ),
            };
            let fill = if down {
                pressed
            } else {
                mix(base, over, hover)
            };
            // Pressing sinks the button by half a point.
            let shape = if down { rect.shrink(0.5) } else { rect };
            let mut painter = ui.painter().clone();
            if !self.enabled {
                painter.multiply_opacity(0.45);
            }
            if matches!(self.kind, Kind::Primary | Kind::Secondary) && !p.dark {
                painter.rect_filled(
                    shape.translate(vec2(0.0, 1.0)),
                    radius::MD,
                    p.shadow.gamma_multiply(0.5),
                );
            }
            painter.rect(
                shape,
                radius::MD,
                fill,
                egui::Stroke::new(1.0, border),
                egui::StrokeKind::Inside,
            );
            if resp.has_focus() {
                // At full strength even when the button is greyed out.
                a11y::focus_ring(ui.painter(), rect.expand(2.5), radius::MD + 2, p);
            }
            let left = if self.left_aligned {
                rect.left() + pad
            } else {
                rect.center().x - content_w / 2.0
            };
            let mut x = left;
            if self.loading {
                let r = egui::Rect::from_center_size(
                    egui::pos2(x + glyph / 2.0, rect.center().y),
                    vec2(glyph, glyph),
                );
                super::feedback::paint_spinner(ui, &painter, r, fg);
                x += glyph + gap;
            } else if let Some(icon) = self.icon {
                let r = egui::Rect::from_center_size(
                    egui::pos2(x + glyph / 2.0, rect.center().y),
                    vec2(glyph, glyph),
                );
                icons::paint(&painter, r, icon, fg);
                x += glyph + gap;
            }
            let pos = egui::pos2(x, rect.center().y - galley.size().y / 2.0);
            painter.galley(pos, galley, fg);
        }
        let mut resp = if active {
            resp.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            resp
        };
        let tip = self.disabled_reason.or(self.tooltip);
        let (label, enabled) = (self.label, self.enabled);
        // The node first, then the tooltip (which adds to it).
        let described = tip.or(self.loading.then_some("Trabajando…"));
        a11y::button_info(&resp, label, enabled, None, described);
        // The button is "disabled" by its own look and senses, not by the `Ui`, so a plain hover
        // tooltip is the one that shows (`on_disabled_hover_text` only fires in a disabled `Ui`).
        if let Some(t) = tip {
            resp = if resp.enabled() {
                resp.on_hover_text(t)
            } else {
                resp.on_disabled_hover_text(t)
            };
            a11y::tip_on_focus(&resp, t);
        }
        resp
    }
}

/// A square, quiet button with only an icon; `tip` says what it does and is also its name.
pub fn icon_button(ui: &mut Ui, icon: Icon, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    finish_icon_button(ui, resp, rect, icon, tip, tip)
}

/// An icon button in a place the layout did not reserve for it (the corner of a tile):
/// `name` is what a screen reader calls it, `tip` the tooltip. It is a real tab stop.
pub fn icon_button_at(
    ui: &mut Ui,
    rect: Rect,
    id: egui::Id,
    icon: Icon,
    tip: &str,
    name: &str,
) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    finish_icon_button(ui, resp, rect, icon, tip, name)
}

fn finish_icon_button(
    ui: &mut Ui,
    resp: Response,
    rect: Rect,
    icon: Icon,
    tip: &str,
    name: &str,
) -> Response {
    let p = pal(ui);
    if ui.is_rect_visible(rect) {
        let hover = theme::anim_bool(
            ui.ctx(),
            resp.id.with("hover"),
            resp.hovered(),
            motion::FAST,
        );
        let fill = if resp.is_pointer_button_down_on() {
            p.hover_wash.gamma_multiply(2.0)
        } else {
            p.hover_wash.gamma_multiply(hover)
        };
        ui.painter().rect_filled(rect, radius::MD, fill);
        if resp.has_focus() {
            a11y::focus_ring(ui.painter(), rect.expand(2.0), radius::MD + 2, p);
        }
        icons::paint(
            ui.painter(),
            rect.shrink(7.0),
            icon,
            mix(p.text_muted, p.text, hover.max(f32::from(resp.has_focus()))),
        );
    }
    a11y::button_info(&resp, name, true, None, Some(tip).filter(|t| *t != name));
    a11y::tip_on_focus(&resp, tip);
    resp.on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(tip)
}

/// The outline a primary button grows while the pointer is over it. In the dark theme the fill
/// cannot get lighter on hover (the white label needs 4.5:1 and the brand fill already sits at
/// 4.67:1), so the hover would be almost invisible; a light accent outline carries it instead.
/// The light theme's fill gets darker on hover, which is clear on its own.
fn primary_hover_outline(p: &theme::Palette, hover: f32) -> Color32 {
    if p.dark {
        p.accent.gamma_multiply(hover.clamp(0.0, 1.0))
    } else {
        Color32::TRANSPARENT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Event, Key, Modifiers, RawInput};

    fn frame(ctx: &egui::Context, time: f64, tab: bool, loading: bool) -> Option<egui::Id> {
        let mut raw = RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                vec2(400.0, 200.0),
            )),
            time: Some(time),
            ..Default::default()
        };
        if tab {
            raw.events.push(Event::Key {
                key: Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            });
        }
        let mut out = ctx.run_ui(raw, |ui| {
            Button::primary("Crear botella").loading(loading).show(ui);
        });
        out.textures_delta.clear();
        ctx.memory(|m| m.focused())
    }

    /// A focused button that starts its work keeps the keyboard focus while it spins, and keeps
    /// it when the work ends: the next Tab goes on from there, not from the top of the page.
    #[test]
    fn a_button_keeps_the_focus_while_it_works() {
        let ctx = egui::Context::default();
        theme::install_for_tests(&ctx);
        frame(&ctx, 0.0, false, false);
        let focused = frame(&ctx, 0.1, true, false).expect("Tab reaches the button");
        assert_eq!(frame(&ctx, 0.2, false, false), Some(focused));
        // The work starts: still focused, frame after frame.
        for step in 0..5 {
            let now = frame(&ctx, 0.3 + f64::from(step) * 0.1, false, true);
            assert_eq!(now, Some(focused), "frame {step} of the work");
        }
        // And when it ends.
        assert_eq!(frame(&ctx, 1.0, false, false), Some(focused));
    }

    /// The primary button must look different under the pointer in both themes: the dark fill has
    /// no room to get lighter (label contrast), so an outline appears; the light one darkens.
    #[test]
    fn the_primary_button_shows_hover_in_both_themes() {
        use crate::ui::theme::{DARK, LIGHT};
        let apart = |a: Color32, b: Color32| {
            a.r().abs_diff(b.r()) as u32
                + a.g().abs_diff(b.g()) as u32
                + a.b().abs_diff(b.b()) as u32
        };
        // Dark: no outline at rest, the accent on hover, fading in with the animation.
        assert_eq!(primary_hover_outline(&DARK, 0.0), Color32::TRANSPARENT);
        assert_eq!(primary_hover_outline(&DARK, 1.0), DARK.accent);
        let half = primary_hover_outline(&DARK, 0.5);
        assert!(half.a() > 0 && half.a() < 255);
        // The outline stands out from the hovered fill (the fill itself moves by a few units).
        assert!(apart(DARK.accent, DARK.fill_accent_hover) >= 100);
        // Light: the fill itself changes clearly, no outline needed.
        assert_eq!(primary_hover_outline(&LIGHT, 1.0), Color32::TRANSPARENT);
        assert!(apart(LIGHT.fill_accent, LIGHT.fill_accent_hover) >= 20);
    }
}
