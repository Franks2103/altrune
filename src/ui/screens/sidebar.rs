//! The sidebar: the overview, every bottle with its state, a new bottle, Wine's state and the
//! settings.

use super::shell::TITLE_BAR;
use crate::domain::status::CREATING;
use crate::ui::brand::{self, Art};
use crate::ui::components::{self as c, Button, Leading, Trailing};
use crate::ui::icons::Icon;
use crate::ui::theme::{self, pal, radius, size, text, Tone, Weight};
use crate::{Action, App, Screen};
use eframe::egui::{self, pos2, vec2, Color32, Rect};

impl App {
    pub(crate) fn sidebar(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let p = pal(ui);
        ui.add_space(TITLE_BAR + 4.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            // The symbol, in the 22-point box the old mark had: it is part of the name beside it
            // (the name says it to VoiceOver), so it has no node of its own.
            let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), egui::Sense::hover());
            let mark = brand::place_centered(ui.ctx(), Art::Symbol, r.center(), r.width());
            brand::paint(ui.painter(), Art::Symbol, mark, Color32::WHITE);
            ui.add_space(2.0);
            ui.label(text("Altrune", size::TITLE, Weight::Semibold, p.text));
        });
        ui.add_space(14.0);

        if c::nav_item(
            ui,
            Leading::Icon(Icon::Home),
            "Inicio",
            None,
            self.screen == Screen::Home,
            Trailing::Nothing,
            "",
        )
        .clicked()
        {
            acts.push(Action::GoHome);
        }

        let bottom_h = 104.0;
        egui::ScrollArea::vertical()
            .max_height((ui.available_height() - bottom_h).max(80.0))
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let _follow = c::a11y::follow_focus(ui);
                // Bottles being made are not on disk yet: each has a row of its own until the
                // scan finds it.
                let creating = self.creating_bottles();
                let mut titled = false;
                if !creating.is_empty() {
                    titled = true;
                    c::nav_section(ui, "MIS BOTELLAS");
                    for path in creating {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let engine = crate::domain::engines::engine_badge(
                            self.cfg.bottle_ref(path).and_then(|s| s.engine.as_deref()),
                        )
                        .unwrap_or("");
                        let sub = c::sub_line(&[engine, CREATING]);
                        c::nav_item(
                            ui,
                            Leading::Avatar(&name, None),
                            &name,
                            Some(&sub),
                            false,
                            Trailing::Spinner,
                            "Se está creando: un momento.",
                        );
                    }
                }
                for (i, b) in self.bottles.iter().enumerate() {
                    if !titled {
                        titled = true;
                        c::nav_section(ui, "MIS BOTELLAS");
                    }
                    let status = self.bottle_status(b);
                    // «Lista» is what every bottle is when nothing is wrong: it says nothing.
                    let label = status.label();
                    let sub = c::sub_line(&[
                        self.engine_note(b),
                        if super::home::state_worth_showing(&status) {
                            &label
                        } else {
                            ""
                        },
                    ]);
                    // Work turns; a bottle in use or in need of a look has a dot; the rest
                    // are quiet.
                    let trailing = if status.is_working() {
                        Trailing::Spinner
                    } else if theme::status_tone(&status) != Tone::Neutral {
                        Trailing::Dot(p.tone(theme::status_tone(&status)))
                    } else {
                        Trailing::Nothing
                    };
                    let selected = self.screen == Screen::Bottle && i == self.selected;
                    let row = c::nav_item(
                        ui,
                        Leading::Avatar(&b.name, self.bottle_picture(b)),
                        &b.name,
                        Some(&sub),
                        selected,
                        trailing,
                        status.detail().unwrap_or(""),
                    );
                    if row.clicked() {
                        acts.push(Action::Select(i));
                    }
                }
                if self.bottles.is_empty() {
                    c::nav_section(ui, "MIS BOTELLAS");
                    ui.horizontal(|ui| {
                        ui.add_space(10.0);
                        ui.label(text(
                            "Aún no tienes botellas.",
                            size::SMALL,
                            Weight::Regular,
                            p.text_faint,
                        ));
                    });
                }
                ui.add_space(6.0);
                if Button::ghost("Nueva botella")
                    .icon(Icon::Plus)
                    .full_width()
                    .left()
                    .disabled_reason(self.wine_missing_reason())
                    .tooltip("Un pequeño Windows para tus programas (⌘N)")
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::OpenNewBottle);
                }
            });

        // Bottom: Wine's state, then the settings. They are drawn top to bottom, in the order
        // Tab visits them, in a block that sits at the bottom: its height is the one measured on
        // the last frame (a bottom-up layout would draw the settings first, so Tab would go
        // against what the eye sees).
        let block_id = ui.scope_id().with("bottom-block");
        let last_h = ui
            .data(|d| d.get_temp::<f32>(block_id))
            .unwrap_or(bottom_h - 10.0);
        let free = ui.available_rect_before_wrap();
        let top = (free.bottom() - last_h).max(free.top());
        let used = ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_min_max(pos2(free.left(), top), free.max))
                    .layout(egui::Layout::top_down(egui::Align::Min)),
                |ui| {
                    self.wine_chip(ui, acts);
                    ui.add_space(4.0);
                    // ⌘, opens the settings of the bottle on screen, so the app's own have no key
                    // there.
                    let tip = if self.screen == Screen::Bottle && self.bottle().is_some() {
                        "Ajustes de la app: Wine, motores y copias"
                    } else {
                        "Ajustes de la app: Wine, motores y copias (⌘,)"
                    };
                    if c::nav_item(
                        ui,
                        Leading::Icon(Icon::Gear),
                        "Ajustes",
                        None,
                        self.screen == Screen::Settings,
                        Trailing::Nothing,
                        tip,
                    )
                    .clicked()
                    {
                        acts.push(Action::OpenSettings);
                    }
                },
            )
            .response
            .rect
            .height();
        if (used - last_h).abs() > 0.5 {
            ui.data_mut(|d| d.insert_temp(block_id, used));
            ui.ctx().request_repaint();
        }
    }

    /// Wine's state in one line (with a bar while it installs); opens its details.
    fn wine_chip(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let p = pal(ui);
        let WineSummary {
            tone,
            title,
            sub,
            frac,
            working,
        } = self.wine_summary();
        let shown_sub = crate::ui::short(&sub, 28);
        let resp = egui::Frame::NONE
            .fill(p.surface)
            .stroke(egui::Stroke::new(1.0, p.border))
            .corner_radius(radius::LG)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    // Work whose end cannot be measured turns; a dot would say it is still.
                    if working {
                        c::spinner(ui, 12.0);
                    } else {
                        c::status_dot(ui, p.tone(tone));
                    }
                    ui.vertical(|ui| {
                        // The button over the chip says all of this; the texts are not read twice.
                        c::a11y::hide(&ui.label(text(
                            &title,
                            size::SMALL,
                            Weight::Semibold,
                            p.text,
                        )));
                        c::a11y::hide(&ui.label(text(
                            &shown_sub,
                            size::CAPTION,
                            Weight::Regular,
                            p.text_muted,
                        )));
                    });
                });
                if let Some(f) = frac {
                    ui.add_space(4.0);
                    c::progress(ui, ui.scope_id().with("wine-progress"), Some(f));
                }
            })
            .response;
        let resp = ui
            .interact(
                resp.rect,
                ui.scope_id().with("wine-chip"),
                egui::Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.has_focus() {
            c::a11y::focus_ring(ui.painter(), resp.rect.expand(2.0), radius::LG + 2, p);
        }
        // A button named after what it says («Wine listo. 11.0») with the full text of the
        // detail, which is cut on screen.
        let name = format!("{title}. {sub}");
        let help = if shown_sub == sub {
            "Ver el estado de Wine".to_string()
        } else {
            c::a11y::joined_tip(Some(&name), "Ver el estado de Wine")
        };
        c::a11y::button_info(&resp, &name, true, None, Some(&help));
        let resp = resp.on_hover_text(&help);
        c::a11y::tip_on_focus(&resp, &help);
        if resp.clicked() {
            acts.push(Action::OpenWineHelp);
        }
    }

    /// What the Wine setup shows in the sidebar and on the overview: see [`WineSummary`].
    pub(crate) fn wine_summary(&self) -> WineSummary {
        use crate::wine_setup::{Step, WineState};
        let g = self.setup.lock().unwrap_or_else(|e| e.into_inner());
        let state = |tone, title: &str, sub: String| WineSummary {
            tone,
            title: title.to_string(),
            sub,
            frac: None,
            working: false,
        };
        if g.running {
            let what = if g.flavour.is_empty() {
                "Wine".to_string()
            } else {
                g.flavour.clone()
            };
            let sub = g
                .step
                .map(|s| s.short_label().to_string())
                .unwrap_or_else(|| "Empezando…".into());
            // Only the download has an amount that is really known: the other steps take what
            // they take, so they get a spinner instead of a percentage that would stand still.
            let frac = (g.step == Some(Step::Download) && g.total > 0)
                .then(|| (g.downloaded as f32 / g.total as f32).clamp(0.0, 1.0));
            let title = match frac {
                Some(f) => format!("Instalando {what}… {}%", (f * 100.0) as u32),
                None => format!("Instalando {what}…"),
            };
            return WineSummary {
                tone: Tone::Accent,
                title,
                sub,
                frac,
                working: frac.is_none(),
            };
        }
        match &self.wine_state {
            WineState::Unknown => state(Tone::Neutral, "Buscando Wine…", "Un momento".into()),
            WineState::Ready { version, .. } => state(Tone::Success, "Wine listo", version.clone()),
            WineState::Broken { reason, .. } => {
                state(Tone::Danger, "Wine no arranca", reason.clone())
            }
            WineState::Missing => state(
                Tone::Warning,
                "Falta Wine",
                if g.error.is_some() {
                    "Pulsa para reintentar".into()
                } else {
                    "Gratis · pulsa aquí".into()
                },
            ),
        }
    }
}

/// The state of Wine in words: a tone, a title, a line of detail, how far the download is
/// (`Some` only while an amount is really known) and whether the app is working on it without
/// a measurable end (then a spinner is shown instead of a percentage).
pub(crate) struct WineSummary {
    pub tone: Tone,
    pub title: String,
    pub sub: String,
    pub frac: Option<f32>,
    pub working: bool,
}
