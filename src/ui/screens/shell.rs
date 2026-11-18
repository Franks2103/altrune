//! The window's frame: sidebar, main area, status line, dialogs and the drop target, plus the
//! short transitions between pages.

use crate::app::status_visibility;
use crate::ui::components::{self as c, Button};
use crate::ui::icons::Icon;
use crate::ui::theme::{self, motion, pal, size, text, Tone, Weight};
use crate::{Action, App, BottleTab, NoticeKind, Screen};
use eframe::egui::{self, Margin};
use std::path::{Path, PathBuf};

/// The height of the transparent title bar the content runs under.
pub const TITLE_BAR: f32 = 38.0;

/// Ease-out over 0..1.
pub fn ease(t: f64) -> f32 {
    let t = t.clamp(0.0, 1.0) as f32;
    1.0 - (1.0 - t).powi(3)
}

/// How far a transition that started at `since` has come.
pub fn progress_since(ctx: &egui::Context, since: f64, secs: f32) -> f32 {
    if theme::reduced_motion() {
        return 1.0;
    }
    let now = ctx.input(|i| i.time);
    let t = ease((now - since) / f64::from(secs));
    if t < 1.0 {
        ctx.request_repaint();
    }
    t
}

impl App {
    pub(crate) fn shell(&mut self, ui: &mut egui::Ui) {
        let _frame_time = crate::measure::perf::FrameTimer::start();
        let ctx = ui.ctx().clone();
        let p = pal(ui);
        let mut acts = vec![];
        // Reduce Motion may have changed in System Settings while the window was in the back.
        theme::refresh_reduced_motion_on_focus(&ctx);
        self.load_icons();
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        acts.extend(dropped.into_iter().map(Action::Drop));

        // Remember when the page, the dialog or the status changed, for their transitions.
        let now = ctx.input(|i| i.time);
        // Only a bottle's page depends on which bottle and tab are chosen: Inicio and Ajustes
        // look the same whichever is selected, and a program tile pressed on Inicio selects its
        // bottle, which must not make the page fade in again.
        let key = if self.screen == Screen::Bottle {
            (self.screen, self.selected, self.tab)
        } else {
            (self.screen, 0, BottleTab::Programs)
        };
        if self.view_key != Some(key) {
            self.view_key = Some(key);
            self.view_since = now;
        }
        let modal_kind = self.modal.as_ref().map(std::mem::discriminant);
        if modal_kind != self.modal_kind {
            // The control that had the keyboard when a dialog opened gets it back when the
            // dialog closes, so Tab carries on from there (and a screen reader keeps its place)
            // instead of starting again at the top of the window.
            let before = egui::Id::NULL.with("focus-before-dialog");
            let back = egui::Id::NULL.with("focus-back-from-dialog");
            if self.modal_kind.is_none() {
                let focused = ctx.memory(|m| m.focused());
                ctx.data_mut(|d| d.insert_temp(before, focused));
            } else if modal_kind.is_none() {
                let saved = ctx.data(|d| d.get_temp::<Option<egui::Id>>(before));
                ctx.data_mut(|d| d.insert_temp(back, saved.flatten()));
            }
            self.modal_kind = modal_kind;
            self.modal_since = now;
        }
        // The dialog's layer lingers for one frame after it closes, and what is under it cannot
        // take the focus meanwhile: the focus goes back once the layer is gone. A control that
        // is no longer drawn drops the request by itself.
        let back = egui::Id::NULL.with("focus-back-from-dialog");
        if let Some(Some(id)) = ctx.data(|d| d.get_temp::<Option<egui::Id>>(back)) {
            if modal_kind.is_some() {
                ctx.data_mut(|d| d.insert_temp(back, None::<egui::Id>));
            } else if ctx.memory(|m| m.top_modal_layer()).is_none() {
                ctx.memory_mut(|m| m.request_focus(id));
                ctx.data_mut(|d| d.insert_temp(back, None::<egui::Id>));
            }
        }
        if self.status != self.status_shown {
            self.status_shown = self.status.clone();
            self.status_since = now;
        }

        egui::Panel::left("side")
            .exact_size(236.0)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(p.sidebar).inner_margin(Margin {
                left: 10,
                right: 10,
                top: 0,
                bottom: 10,
            }))
            .show(ui, |ui| self.sidebar(ui, &mut acts));

        egui::Panel::bottom("status")
            .exact_size(28.0)
            .frame(
                egui::Frame::NONE
                    .fill(p.canvas)
                    .inner_margin(Margin::symmetric(24, 0)),
            )
            .show(ui, |ui| self.status_line(ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(p.canvas).inner_margin(Margin {
                left: 32,
                right: 32,
                top: 0,
                bottom: 0,
            }))
            .show(ui, |ui| {
                // The strip under the title bar moves the window, as a title bar does. It is not
                // focusable: it has no name and nothing to do from the keyboard, so it must not
                // be a stop for Tab.
                let strip = egui::Rect::from_min_size(
                    ui.max_rect().min,
                    egui::vec2(ui.max_rect().width(), TITLE_BAR - 8.0),
                );
                let drag = ui.interact(
                    strip,
                    ui.scope_id().with("drag"),
                    egui::Sense::CLICK | egui::Sense::DRAG,
                );
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                }
                // A new page fades in and settles a few points.
                let t = progress_since(&ctx, self.view_since, motion::BASE);
                ui.multiply_opacity(t);
                ui.add_space(TITLE_BAR + 4.0 * (1.0 - t));
                match self.screen {
                    Screen::Bottle if self.bottle().is_some() => self.bottle_screen(ui, &mut acts),
                    Screen::Settings => self.settings_screen(ui, &mut acts),
                    _ => self.home(ui, &mut acts),
                }
            });

        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            c::drop_overlay(&ctx);
        }
        self.shortcuts(&ctx, &mut acts);
        self.dialogs(&ctx, &mut acts);
        for a in acts {
            self.apply(a);
        }
    }

    /// The line at the bottom: what the app is doing, and what is running. Information fades a
    /// few seconds after it appears and the hint comes back; an error stays until the next
    /// message (its banner stays until it is closed).
    fn status_line(&self, ui: &mut egui::Ui) {
        let p = pal(ui);
        let app_working = self.app_working();
        let kind = self.status_kind();
        ui.horizontal_centered(|ui| {
            if app_working {
                c::spinner(ui, 12.0);
            }
            let now = ui.ctx().input(|i| i.time);
            let age = (now - self.status_since).max(0.0);
            let fade_in = progress_since(ui.ctx(), self.status_since, motion::SLOW);
            let seen = if self.status.is_empty() {
                0.0
            } else {
                status_visibility(kind, age, app_working)
            };
            if !self.status.is_empty() && seen > 0.0 && kind != NoticeKind::Error && !app_working {
                // Wake up in time to take the line away.
                let left = crate::app::STATUS_SECS - age;
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(
                        left.clamp(0.05, 1.0),
                    ));
            }
            let (s, alpha, color) = if seen > 0.0 {
                let color = match kind {
                    NoticeKind::Info => p.text_muted,
                    NoticeKind::Success => p.text,
                    NoticeKind::Error => p.danger,
                };
                (self.status.as_str(), fade_in * seen, color)
            } else {
                (self.idle_hint(), fade_in, p.text_muted)
            };
            if seen > 0.0 && kind == NoticeKind::Error {
                let (r, icon) =
                    ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                c::a11y::label_node(&icon, "Error");
                crate::ui::icons::paint(ui.painter(), r, Icon::Alert, p.danger);
            }
            let open = self.bottles_in_use();
            // The count keeps its place at the right; a long message is cut before it.
            let reserved = if open > 0 { 150.0 } else { 0.0 };
            ui.scope(|ui| {
                ui.set_max_width((ui.available_width() - reserved).max(120.0));
                let line = ui
                    .add(
                        egui::Label::new(text(
                            s,
                            size::SMALL,
                            Weight::Regular,
                            color.gamma_multiply(alpha),
                        ))
                        .truncate(),
                    )
                    .on_hover_text(s);
                // A screen reader announces the line when it changes (an error at once).
                if seen > 0.0 {
                    let live = if kind == NoticeKind::Error {
                        egui::accesskit::Live::Assertive
                    } else {
                        egui::accesskit::Live::Polite
                    };
                    ui.ctx().accesskit_node_builder(line.id, |node| {
                        node.set_live(live);
                    });
                }
            });
            if open > 0 {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if open == 1 {
                        "1 botella en uso".to_string()
                    } else {
                        format!("{open} botellas en uso")
                    };
                    ui.label(text(label, size::SMALL, Weight::Medium, p.text_muted));
                    c::status_dot(ui, p.tone(Tone::Success));
                });
            }
        });
    }

    /// What the status line says when there is nothing to report. It follows what can be done:
    /// without Wine, dragging an installer in leads nowhere, so the line says what comes first.
    pub(crate) fn idle_hint(&self) -> &'static str {
        match self.wine_missing_reason() {
            None => "Puedes arrastrar un instalador (.exe, .msi o .bat) a esta ventana.",
            Some(_) if self.installing() => "Se está instalando Wine: un momento.",
            Some(_) if self.wine_state == crate::wine_setup::WineState::Unknown => {
                "Buscando Wine en tu Mac…"
            }
            Some(_) if matches!(self.wine_state, crate::wine_setup::WineState::Broken { .. }) => {
                "Wine no arranca: pulsa su estado, a la izquierda."
            }
            Some(_) => "Consigue Wine para empezar: es gratis.",
        }
    }

    /// The app itself is working on something that is not one bottle's job: it keeps its
    /// spinner in the status bar.
    pub(crate) fn app_working(&self) -> bool {
        self.installing()
            || self.app_job.is_some()
            || self.wine_state == crate::wine_setup::WineState::Unknown
    }

    /// The errors of a bottle (or, with `None`, those that belong to no bottle on the list) as
    /// banners that stay until they are closed. A bottle's error offers its diagnosis.
    pub(crate) fn error_banners(
        &self,
        ui: &mut egui::Ui,
        acts: &mut Vec<Action>,
        bottle: Option<&Path>,
    ) {
        for e in self.errors_for(bottle) {
            // On the overview a listed bottle's error says whose it is and offers to go there.
            let listed = if bottle.is_none() {
                self.error_bottle_index(e)
            } else {
                None
            };
            let (title, body) = self.error_words(e, listed);
            c::banner(ui, Tone::Danger, Icon::Alert, &title, body, |ui| {
                if let Some(i) = listed {
                    if Button::primary("Ver").show(ui).clicked() {
                        acts.push(Action::Select(i));
                        acts.push(Action::SetTab(crate::BottleTab::Diagnostics));
                    }
                } else if e.bottle.is_some()
                    && bottle.is_some()
                    && self.tab != crate::BottleTab::Diagnostics
                    && Button::primary("Ver diagnóstico").show(ui).clicked()
                {
                    acts.push(Action::SetTab(crate::BottleTab::Diagnostics));
                }
                if Button::secondary("Cerrar").show(ui).clicked() {
                    acts.push(Action::DismissError(e.bottle.clone()));
                }
            });
            ui.add_space(theme::space::M);
        }
    }

    /// How many bottles have something open (see [`count_bottles_in_use`]). The status line and
    /// the overview both show this number.
    pub(crate) fn bottles_in_use(&self) -> usize {
        count_bottles_in_use(&self.live, self.running.iter().map(|r| r.bottle.as_path()))
    }

    /// Why nothing can be done with Wine yet, or `None` when Wine is ready. It is the tooltip of
    /// every button that needs Wine.
    pub(crate) fn wine_missing_reason(&self) -> Option<&'static str> {
        if self.wine.is_some() {
            None
        } else if self.installing() {
            Some("Espera a que termine de instalarse Wine.")
        } else if self.wine_state == crate::wine_setup::WineState::Unknown {
            Some("Estoy buscando Wine en tu Mac: un momento.")
        } else {
            Some("Primero consigue Wine (es gratis): se instala desde el aviso de arriba.")
        }
    }

    /// Why another program cannot be installed now: one installation goes at a time.
    pub(crate) fn install_running_reason(&self) -> Option<&'static str> {
        self.install
            .as_ref()
            .is_some_and(|s| s.active())
            .then_some("Ya hay una instalación en curso: espera a que termine.")
    }

    /// ⌘N, ⌘O and ⌘, : the same things as the buttons, with the same reasons when they cannot.
    fn shortcuts(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        // A dialog owns the keyboard while it is open.
        if self.modal.is_some() {
            return;
        }
        let (new, open, settings) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::N),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::O),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma),
            )
        });
        if new {
            // Without Wine the action itself says why it does nothing.
            acts.push(Action::OpenNewBottle);
        }
        if open {
            if let Some(why) = self.wine_missing_reason() {
                self.say(why);
            } else {
                // The dialog asks where it goes: no bottle has to be chosen first.
                acts.push(Action::Install);
            }
        }
        if settings {
            // The settings of the bottle on screen; the app's own from anywhere else.
            if self.screen == Screen::Bottle && self.bottle().is_some() {
                acts.push(Action::SetTab(crate::BottleTab::Settings));
            } else {
                acts.push(Action::OpenSettings);
            }
        }
    }

    /// The graphics engine of a bottle as a list names it: empty while it is the automatic one,
    /// which says nothing to someone who never chose anything (as with the chip on its page).
    pub(crate) fn engine_note(&self, b: &crate::backend::Bottle) -> &'static str {
        crate::domain::engines::engine_badge(
            self.cfg
                .bottle_ref(&b.path)
                .and_then(|s| s.engine.as_deref()),
        )
        .unwrap_or("")
    }

    /// A page title with a line under it and actions on its right. When the page is narrow the
    /// actions go under the title instead, left to right, so nothing runs into the title.
    pub(crate) fn page_header(
        ui: &mut egui::Ui,
        title: &str,
        subtitle: &str,
        actions: impl FnOnce(&mut egui::Ui),
    ) {
        if theme::layout::is_narrow(ui.available_width()) {
            Self::page_title(ui, title, subtitle);
            ui.add_space(theme::space::S);
            ui.horizontal_wrapped(actions);
        } else {
            ui.horizontal(|ui| {
                Self::page_title(ui, title, subtitle);
                Self::pushed_right(ui, "page-actions", actions);
            });
        }
        ui.add_space(theme::space::XL);
    }

    /// Puts a group of widgets at the right end of the row they are in, created from left to
    /// right. A right-to-left layout would create the last widget first, so Tab and a screen
    /// reader would visit the group against what the eye sees. Instead the space in front of the
    /// group is the room left minus the width the group had on the last frame (kept in the
    /// context, as the sidebar does for its bottom block), and the frame after a change of the
    /// group's width puts it right.
    pub(crate) fn pushed_right(ui: &mut egui::Ui, salt: &str, group: impl FnOnce(&mut egui::Ui)) {
        let id = ui.scope_id().with(("pushed-right", salt));
        let last = ui.data(|d| d.get_temp::<f32>(id));
        if let Some(w) = last {
            let spare = ui.available_width() - w;
            if spare > 0.0 {
                ui.add_space(spare);
            }
        }
        let used = ui.scope(group).response.rect.width();
        if last.is_none_or(|w| (w - used).abs() > 0.5) {
            ui.data_mut(|d| d.insert_temp(id, used));
            ui.ctx().request_repaint();
        }
    }

    /// A page title with a line under it, and nothing on its right.
    pub(crate) fn page_header_plain(ui: &mut egui::Ui, title: &str, subtitle: &str) {
        Self::page_title(ui, title, subtitle);
        ui.add_space(theme::space::XL);
    }

    fn page_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
        let p = pal(ui);
        ui.vertical(|ui| {
            c::a11y::heading(&ui.label(text(title, size::H1, Weight::Bold, p.text)), 1);
            if !subtitle.is_empty() {
                ui.label(text(subtitle, size::BODY_L, Weight::Regular, p.text_muted));
            }
        });
    }
}

/// How many different bottles have something open: the ones Wine is running in (`live`) joined
/// with the ones a program was started in (`started`), each counted once. A bottle with two
/// programs open is one bottle in use, not two.
pub fn count_bottles_in_use<'a>(
    live: &'a [PathBuf],
    started: impl Iterator<Item = &'a Path>,
) -> usize {
    let mut set: std::collections::BTreeSet<&Path> = live.iter().map(PathBuf::as_path).collect();
    set.extend(started);
    set.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A group pushed to the right end of a row is created left to right: what is created
    /// first is also what is seen first, so Tab and a screen reader follow the eye. It ends at
    /// the right edge once the width of the group is known (the second frame).
    #[test]
    fn a_right_group_is_created_in_the_order_it_reads_and_ends_at_the_edge() {
        let ctx = egui::Context::default();
        theme::install_for_tests(&ctx);
        let mut rects = vec![];
        for n in 0..3 {
            rects.clear();
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(800.0, 400.0),
                )),
                time: Some(f64::from(n) * 0.1),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Título");
                    App::pushed_right(ui, "test", |ui| {
                        rects.push(ui.add(egui::Button::new("Primero")).rect);
                        rects.push(ui.add(egui::Button::new("Segundo")).rect);
                    });
                });
            });
            out.textures_delta.clear();
        }
        let [first, second] = rects[..] else {
            panic!("two widgets expected: {rects:?}");
        };
        assert!(
            first.right() <= second.left() + 0.5,
            "the first created is the left one: {first:?} {second:?}"
        );
        // The row's right edge is the screen's, less the panel margin egui leaves.
        assert!(
            second.right() > 760.0,
            "the group is at the right: {second:?}"
        );
    }

    /// Programs are not bottles: two programs in one bottle, and a bottle that is both running
    /// and started from here, count once.
    #[test]
    fn bottles_in_use_counts_bottles_not_programs() {
        let a = PathBuf::from("/b/a");
        let b = PathBuf::from("/b/b");
        let c = PathBuf::from("/b/c");
        let live = vec![a.clone(), b.clone()];
        assert_eq!(count_bottles_in_use(&[], std::iter::empty()), 0);
        assert_eq!(count_bottles_in_use(&live, std::iter::empty()), 2);
        // Two programs started in bottle `a`, one more in `c`.
        let started = [a.as_path(), a.as_path(), c.as_path()];
        assert_eq!(count_bottles_in_use(&live, started.into_iter()), 3);
        // Only started from here (the live check has not answered yet).
        let started = [c.as_path(), c.as_path()];
        assert_eq!(count_bottles_in_use(&[], started.into_iter()), 1);
    }
}
