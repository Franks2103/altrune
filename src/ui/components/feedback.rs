//! Telling the person what is going on: spinners, progress bars, banners, empty states and the
//! steps of a long installation.

use super::a11y;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{self, motion, pal, radius, size, space, text, Tone, Weight};
use eframe::egui::{self, vec2, Color32, Margin, Painter, Pos2, Rect, Sense, Shape, Stroke, Ui};

/// How long a moving indicator waits before the next frame: about 30 frames a second is smooth
/// enough for a spinner and costs half of drawing every frame.
const INDICATOR_FRAME: std::time::Duration = std::time::Duration::from_millis(33);

/// Asks for the next frame of a moving indicator. With Reduce Motion on the indicator is drawn
/// still, so nothing is asked for.
fn request_indicator_frame(ctx: &egui::Context) {
    if !theme::reduced_motion() {
        ctx.request_repaint_after(INDICATOR_FRAME);
    }
}

/// A turning arc in `rect` (a still arc with Reduce Motion). Asks for the next frame, at about 30
/// a second, only while it is on screen.
pub fn paint_spinner(ui: &Ui, painter: &Painter, rect: Rect, color: Color32) {
    let t = ui.input(|i| i.time) as f32;
    let start = if theme::reduced_motion() {
        0.0
    } else {
        t * 300.0
    };
    let r = rect.width().min(rect.height()) / 2.0 - 1.0;
    let pts: Vec<Pos2> = (0..=20)
        .map(|i| {
            let a = (start + 270.0 * i as f32 / 20.0).to_radians();
            rect.center() + vec2(r * a.cos(), r * a.sin())
        })
        .collect();
    painter.add(Shape::line(pts, Stroke::new(1.6, color)));
    request_indicator_frame(ui.ctx());
}

pub fn spinner(ui: &mut Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    let color = pal(ui).text_muted;
    paint_spinner(ui, ui.painter(), rect, color);
}

/// A thin bar: filled to `frac` (eased as it grows), or a sliding segment when the amount of work
/// is not known.
pub fn progress(ui: &mut Ui, id: egui::Id, frac: Option<f32>) {
    let p = pal(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    // The bar is painted: tell the accessibility tree what it is and, when known, how far.
    resp.widget_info(|| {
        let mut info = egui::WidgetInfo::labeled(
            egui::Role::ProgressIndicator,
            true,
            if frac.is_some() {
                "Progreso"
            } else {
                "Trabajando…"
            },
        );
        info.value = frac.map(|f| f64::from(f.clamp(0.0, 1.0)) * 100.0);
        info
    });
    let painter = ui.painter();
    painter.rect_filled(rect, 3, p.sunken);
    match frac {
        Some(f) => {
            let shown = theme::anim_value(ui.ctx(), id, f.clamp(0.0, 1.0), motion::SLOW);
            let mut fill = rect;
            fill.set_width((rect.width() * shown).max(6.0));
            painter.rect_filled(fill, 3, p.accent);
        }
        None => {
            if theme::reduced_motion() {
                // Nothing moves: the whole bar, softly, says "working" and cannot be read as a
                // half-done job.
                painter.rect_filled(rect, 3, p.accent.gamma_multiply(0.45));
                return;
            }
            let t = ui.input(|i| i.time) as f32;
            let w = rect.width() * 0.28;
            let x = rect.left() + (rect.width() + w) * (t * 0.6).fract() - w;
            let seg = Rect::from_min_max(
                egui::pos2(x.max(rect.left()), rect.top()),
                egui::pos2((x + w).min(rect.right()), rect.bottom()),
            );
            painter.rect_filled(seg, 3, p.accent);
            request_indicator_frame(ui.ctx());
        }
    }
}

/// A message that stays in place: an icon, a title, a sentence and its actions.
pub fn banner(
    ui: &mut Ui,
    tone: Tone,
    icon: Icon,
    title: &str,
    body: &str,
    actions: impl FnOnce(&mut Ui),
) {
    let p = pal(ui);
    egui::Frame::NONE
        .fill(p.tone_soft(tone))
        .stroke(Stroke::new(1.0, p.tone(tone).gamma_multiply(0.35)))
        .corner_radius(radius::LG)
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                icons::paint(ui.painter(), r, icon, p.tone(tone));
                ui.add_space(space::XS);
                ui.vertical(|ui| {
                    ui.label(text(title, size::BODY_L, Weight::Semibold, p.text));
                    if !body.is_empty() {
                        ui.add_space(space::XXS);
                        ui.label(text(body, size::BODY, Weight::Regular, p.text_muted));
                    }
                    ui.add_space(space::S);
                    ui.horizontal_wrapped(actions);
                });
            });
        });
}

/// What a place shows when it has nothing yet, and what to do about it.
pub fn empty_state(ui: &mut Ui, icon: Icon, title: &str, body: &str, action: impl FnOnce(&mut Ui)) {
    let p = pal(ui);
    egui::Frame::NONE
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(radius::XL)
        .inner_margin(Margin::symmetric(24, 28))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.vertical_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
                ui.painter().rect_filled(r, radius::LG, p.sunken);
                icons::paint(ui.painter(), r.shrink(10.0), icon, p.text_muted);
                ui.add_space(space::M);
                ui.label(text(title, size::TITLE, Weight::Semibold, p.text));
                ui.add_space(space::XS);
                ui.label(text(body, size::BODY, Weight::Regular, p.text_muted));
                ui.add_space(space::M);
                action(ui);
            });
        });
}

/// One step of a long job: done, in progress or still to come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    Done,
    Now,
    Later,
    /// It ran, but not all of it worked, and the job went on.
    NotApplied,
}

pub fn step_row(ui: &mut Ui, state: StepState, label: &str) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        let (r, icon) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
        a11y::label_node(
            &icon,
            match state {
                StepState::Done => "Hecho",
                StepState::Now => "En curso",
                StepState::Later => "Pendiente",
                StepState::NotApplied => "No aplicado del todo",
            },
        );
        match state {
            StepState::Done => {
                ui.painter()
                    .circle_filled(r.center(), 7.0, p.tone_soft(Tone::Success));
                icons::paint(ui.painter(), r.shrink(3.0), Icon::Check, p.success);
            }
            StepState::Now => paint_spinner(ui, ui.painter(), r.shrink(1.0), p.accent),
            StepState::Later => {
                ui.painter()
                    .circle_stroke(r.center(), 5.5, Stroke::new(1.3, p.border_strong));
            }
            StepState::NotApplied => {
                icons::paint(ui.painter(), r.shrink(1.0), Icon::Alert, p.warning)
            }
        }
        let color = match state {
            StepState::Now => p.text,
            StepState::Done => p.text_muted,
            StepState::Later => p.text_faint,
            StepState::NotApplied => p.text,
        };
        let w = if state == StepState::Now {
            Weight::Medium
        } else {
            Weight::Regular
        };
        ui.label(text(label, size::BODY, w, color));
    });
}
