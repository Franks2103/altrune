//! Things drawn over the window: dialogs (they fade and rise in) and the drop target.

use super::a11y::{self, enter_confirms, DialogFocus};
use super::button::Button;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{self, motion, pal_ctx, radius, size, space, text, Palette, Tone, Weight};
use eframe::egui::{self, vec2, Align2, Margin, Rect, Shadow, Stroke, StrokeKind, Ui};

pub fn modal_frame(p: &Palette) -> egui::Frame {
    egui::Frame::NONE
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(radius::XL)
        .inner_margin(Margin::same(22))
        .shadow(Shadow {
            offset: [0, 16],
            blur: 44,
            spread: 0,
            color: p.shadow,
        })
}

/// A dialog's icon, title and a sentence under it.
pub fn modal_header(ui: &mut Ui, icon: Icon, tone: Tone, title: &str, subtitle: &str) {
    let p = theme::pal(ui);
    ui.horizontal_top(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(36.0, 36.0), egui::Sense::hover());
        ui.painter().rect_filled(r, radius::LG, p.tone_soft(tone));
        icons::paint(ui.painter(), r.shrink(9.0), icon, p.tone(tone));
        ui.add_space(space::S);
        ui.vertical(|ui| {
            ui.add_space(1.0);
            a11y::heading(
                &ui.label(text(title, size::H3, Weight::Semibold, p.text)),
                1,
            );
            if !subtitle.is_empty() {
                ui.add_space(space::XXS);
                ui.label(text(subtitle, size::BODY, Weight::Regular, p.text_muted));
            }
        });
    });
    ui.add_space(space::L);
}

/// What the buttons of a dialog answered.
#[derive(Default)]
pub struct Answer {
    pub confirmed: bool,
    pub cancelled: bool,
}

/// What has the keyboard focus, as far as this dialog is concerned: a control behind the dialog
/// does not count.
fn dialog_focus(ui: &Ui) -> DialogFocus {
    let ctx = ui.ctx();
    let Some(id) = ctx.memory(|m| m.focused()) else {
        return DialogFocus::Nowhere;
    };
    let here = ctx
        .read_response(id)
        .is_some_and(|r| r.layer_id == ui.layer_id());
    if !here {
        DialogFocus::Nowhere
    } else if ctx.text_edit_focused() {
        DialogFocus::TextField
    } else {
        DialogFocus::Control
    }
}

/// Cancel and the dialog's own action at the bottom right, Cancel first (Tab follows the eye). ↩ presses the button that has the
/// focus; when only a text field (or nothing) has it, ↩ confirms, unless the action is
/// destructive: then ↩ does nothing until its own button is focused.
/// `disabled_reason` is `None` when the action can be taken; otherwise it says why it cannot
/// (it is the button's tooltip and description).
pub fn modal_actions(
    ui: &mut Ui,
    label: &str,
    destructive: bool,
    disabled_reason: Option<&str>,
) -> Answer {
    let enabled = disabled_reason.is_none();
    let focus = dialog_focus(ui);
    let mut a = Answer::default();
    ui.add_space(space::L);
    // Created left to right, the order the eye reads and Tab visits: «Cancelar» first, the
    // action (which may be destructive) second. `trailing_row` keeps the pair at the right.
    ui.horizontal(|ui| {
        super::surface::trailing_row(ui, &format!("modal-actions-{label}"), |ui| {
            if Button::secondary("Cancelar").show(ui).clicked() {
                a.cancelled = true;
            }
            let main = if destructive {
                Button::danger(label)
            } else {
                Button::primary(label)
            };
            if main.disabled_reason(disabled_reason).show(ui).clicked() {
                a.confirmed = true;
            }
        });
    });
    if enter_confirms(focus, enabled, destructive) && ui.input(|i| i.key_pressed(egui::Key::Enter))
    {
        a.confirmed = true;
    }
    // Both in one frame can only be a focused «Cancelar» answering ↩: cancelling wins.
    if a.cancelled {
        a.confirmed = false;
    }
    a
}

/// Over the whole window while a file is dragged over it. Nothing is drawn while a dialog is
/// open: a file dropped then is refused, so the sign must not promise anything.
pub fn drop_overlay(ctx: &egui::Context) {
    if ctx.memory(|m| m.top_modal_layer()).is_some() {
        return;
    }
    let p = pal_ctx(ctx);
    let t = theme::anim_bool(ctx, egui::Id::unique("drop-overlay"), true, motion::FAST);
    let screen = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::unique("drop"),
    ));
    painter.rect_filled(screen, 0, p.scrim.gamma_multiply(t));
    let r = Rect::from_center_size(screen.center(), vec2(400.0, 120.0))
        .translate(vec2(0.0, 8.0 * (1.0 - t)));
    painter.rect(
        r,
        radius::XL,
        p.surface.gamma_multiply(t),
        Stroke::new(2.0, p.accent.gamma_multiply(t)),
        StrokeKind::Inside,
    );
    let ic = Rect::from_center_size(r.center() - vec2(0.0, 20.0), vec2(28.0, 28.0));
    icons::paint(&painter, ic, Icon::Download, p.accent.gamma_multiply(t));
    painter.text(
        r.center() + vec2(0.0, 20.0),
        Align2::CENTER_CENTER,
        "Suelta para elegir dónde se instala",
        theme::font(size::H3, Weight::Semibold),
        p.text.gamma_multiply(t),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Event, Key, Modifiers, RawInput};

    fn frame(ctx: &egui::Context, time: f64, keys: &[Key], destructive: bool) -> Answer {
        let mut raw = RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                vec2(800.0, 600.0),
            )),
            time: Some(time),
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
        let mut answer = Answer::default();
        let mut out = ctx.run_ui(raw, |ui| {
            answer = modal_actions(ui, "Hacer algo", destructive, None);
        });
        out.textures_delta.clear();
        answer
    }

    fn ctx() -> egui::Context {
        let ctx = egui::Context::default();
        theme::install_for_tests(&ctx);
        ctx
    }

    /// ↩ with nothing focused confirms; ↩ on a focused «Cancelar» only cancels; a destructive
    /// dialog is never confirmed by ↩ alone, only by pressing its own focused button.
    #[test]
    fn enter_presses_the_focused_button() {
        // Nothing focused: the shortcut confirms.
        let c = ctx();
        frame(&c, 0.0, &[], false);
        let a = frame(&c, 0.1, &[Key::Enter], false);
        assert!(a.confirmed && !a.cancelled);

        // Tab goes the way the eye reads: «Cancelar» on the left is the first stop, so ↩ after
        // one Tab only cancels.
        let c = ctx();
        frame(&c, 0.0, &[], false);
        frame(&c, 0.1, &[Key::Tab], false);
        let a = frame(&c, 0.2, &[Key::Enter], false);
        assert!(a.cancelled && !a.confirmed, "Enter on Cancelar must cancel");

        // The second stop is the action itself: ↩ presses it, once.
        let c = ctx();
        frame(&c, 0.0, &[], false);
        frame(&c, 0.1, &[Key::Tab], false);
        frame(&c, 0.2, &[Key::Tab], false);
        let a = frame(&c, 0.3, &[Key::Enter], false);
        assert!(a.confirmed && !a.cancelled);

        // Destructive: ↩ alone does nothing, ↩ on its own focused button does it.
        let c = ctx();
        frame(&c, 0.0, &[], true);
        let a = frame(&c, 0.1, &[Key::Enter], true);
        assert!(!a.confirmed && !a.cancelled);
        frame(&c, 0.2, &[Key::Tab], true);
        frame(&c, 0.3, &[Key::Tab], true);
        let a = frame(&c, 0.4, &[Key::Enter], true);
        assert!(a.confirmed && !a.cancelled);
    }

    /// The text the drop sign draws in a frame, with or without a dialog open.
    fn drop_sign_text(dialog_open: bool) -> String {
        let ctx = ctx();
        let mut said = String::new();
        for step in 0..3 {
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    vec2(800.0, 600.0),
                )),
                time: Some(f64::from(step) * 0.1),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                if dialog_open {
                    egui::Modal::new(egui::Id::unique("dialog")).show(ui.ctx(), |ui| {
                        ui.label("Un diálogo");
                    });
                }
                drop_overlay(ui.ctx());
            });
            out.textures_delta.clear();
            said.clear();
            for clipped in &out.shapes {
                if let egui::epaint::Shape::Text(t) = &clipped.shape {
                    said.push_str(t.galley.text());
                    said.push('\n');
                }
            }
        }
        said
    }

    /// The sign says what a drop really does (it asks where), and is gone while a dialog is
    /// open because a drop is refused then.
    #[test]
    fn the_drop_sign_promises_only_what_happens() {
        let said = drop_sign_text(false);
        assert!(
            said.contains("Suelta para elegir dónde se instala"),
            "{said:?}"
        );
        assert!(!said.contains("botella elegida"), "{said:?}");
        let said = drop_sign_text(true);
        assert!(!said.contains("Suelta"), "no sign over a dialog: {said:?}");
    }
}
