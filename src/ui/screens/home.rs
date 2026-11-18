//! The overview: what there is (bottles, programs, what is running, Wine) and the main actions.
//! Only real data: with nothing yet, it shows how to start.

use crate::domain::status::BottleStatus;
use crate::ui::components::{self as c, BottleCard, Button, StepState};
use crate::ui::icons::Icon;
use crate::ui::theme::{self, pal, size, space, text, Tone, Weight};
use crate::{Action, App, BottleTab};
use eframe::egui;

/// Most program tiles shown on the overview.
const PROGRAMS_SHOWN: usize = 18;

/// Whether a bottle's state is worth a chip or a word in a list. «Lista» is what every bottle is
/// when nothing is wrong, so it stays quiet: what is left (work in progress, a program open, a
/// problem) must always be seen.
pub(super) fn state_worth_showing(status: &BottleStatus) -> bool {
    !matches!(status, BottleStatus::Ready)
}

impl App {
    pub(crate) fn home(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let p = pal(ui);
        let wine_ok = self.wine.is_some();
        // With no bottle yet the card below owns the one main action, so the header offers
        // nothing: two primary buttons for the same first step would only make a newcomer
        // wonder which one is right. Otherwise the actions are created in the order they read.
        let first_run = self.bottles.is_empty();
        let title = "Inicio";
        let subtitle = "Tus programas de Windows, en tu Mac.";
        if first_run {
            Self::page_header_plain(ui, title, subtitle);
        } else {
            Self::page_header(ui, title, subtitle, |ui| {
                if wine_ok
                    && Button::secondary("Instalar un programa")
                        .icon(Icon::Download)
                        .disabled_reason(self.install_running_reason())
                        .tooltip("Elige el instalador y dónde se instala (⌘O)")
                        .show(ui)
                        .clicked()
                {
                    acts.push(Action::Install);
                }
                if Button::primary("Nueva botella")
                    .icon(Icon::Plus)
                    .disabled_reason(self.wine_missing_reason())
                    .tooltip("Un pequeño Windows para tus programas (⌘N)")
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::OpenNewBottle);
                }
            });
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let _follow = c::a11y::follow_focus(ui);
                // Errors that belong to no bottle on the list (a bottle that could not be made,
                // a file that is no Wine) stay here until they are closed.
                self.error_banners(ui, acts, None);
                self.install_card_orphan(ui, acts);
                let installing = self.installing();
                if !wine_ok || installing {
                    self.wine_card(ui, acts, false);
                    ui.add_space(space::XL);
                }
                if self.bottles.is_empty() {
                    self.onboarding(ui, acts);
                    return;
                }

                // Numbers at a glance.
                let programs: usize = self.bottles.iter().map(|b| b.shown_count()).sum();
                // Wine is not counted here: the sidebar says how it is, and the card above
                // appears when it needs a look.
                let open = self.bottles_in_use();
                let stats = [
                    (
                        Icon::Bottle,
                        self.bottles.len().to_string(),
                        if self.bottles.len() == 1 {
                            "botella"
                        } else {
                            "botellas"
                        },
                        Tone::Accent,
                    ),
                    (
                        Icon::Play,
                        programs.to_string(),
                        if programs == 1 {
                            "programa"
                        } else {
                            "programas"
                        },
                        Tone::Accent,
                    ),
                    (
                        Icon::Pulse,
                        open.to_string(),
                        "en uso ahora",
                        if open > 0 {
                            Tone::Success
                        } else {
                            Tone::Neutral
                        },
                    ),
                ];
                ui.columns(stats.len(), |cols| {
                    for (col, (icon, value, label, tone)) in cols.iter_mut().zip(&stats) {
                        c::stat_tile(col, *icon, value, label, *tone);
                    }
                });
                ui.add_space(space::S);
                ui.add_space(space::XL);

                c::section(ui, "Botellas", "", |_| {});
                let per_row = theme::layout::bottle_columns(ui.available_width());
                let order: Vec<usize> = (0..self.bottles.len()).collect();
                for row in order.chunks(per_row) {
                    ui.columns(per_row, |cols| {
                        for (j, &i) in row.iter().enumerate() {
                            let b = &self.bottles[i];
                            let status = self.bottle_status(b);
                            // «Lista» is left out: the card only names what is not usual.
                            let state = if state_worth_showing(&status) {
                                status.label()
                            } else {
                                String::new()
                            };
                            let windows = crate::backend::windows_label(&b.windows);
                            let card = BottleCard {
                                name: &b.name,
                                engine: self.engine_note(b),
                                windows: &windows,
                                state: &state,
                                state_tone: theme::status_tone(&status),
                                working: status.is_working(),
                                hint: status.detail().unwrap_or(""),
                                programs: b.shown_count(),
                                picture: self.bottle_picture(b),
                            };
                            if c::bottle_card(&mut cols[j], &card).clicked() {
                                acts.push(Action::Select(i));
                            }
                        }
                    });
                    ui.add_space(space::M);
                }
                ui.add_space(space::M);

                // Programs of every bottle, to open them from here.
                let tiles: Vec<(usize, usize)> = self
                    .bottles
                    .iter()
                    .enumerate()
                    .flat_map(|(bi, b)| b.shown_programs().map(move |(pi, _)| (bi, pi)))
                    .collect();
                if !tiles.is_empty() {
                    let caption = if tiles.len() > PROGRAMS_SHOWN {
                        format!("{PROGRAMS_SHOWN} de {}", tiles.len())
                    } else {
                        String::new()
                    };
                    c::section(ui, "Programas", &caption, |_| {});
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(space::M, space::M);
                        for &(bi, pi) in tiles.iter().take(PROGRAMS_SHOWN) {
                            let b = &self.bottles[bi];
                            let prog = &b.programs[pi];
                            let pic = prog
                                .icon
                                .as_ref()
                                .and_then(|i| self.textures.get(i))
                                .and_then(|t| t.as_ref());
                            let running = self.running_in(&b.path).contains(&prog.name.as_str());
                            let hover = if prog.subtitle.is_empty() {
                                format!("Abrir {} ({})", prog.name, b.name)
                            } else {
                                format!("Abrir {} · {} ({})", prog.name, prog.subtitle, b.name)
                            };
                            // The menu opens with a secondary click and with the tile's
                            // «Más opciones» button.
                            // From the click on, the tile says it is opening, as on the
                            // bottle's page.
                            let state = self.tile_state(b, prog, running);
                            let r = c::surface::program_tile_in(
                                ui,
                                &prog.name,
                                &b.name,
                                state,
                                pic,
                                |ui| {
                                    if c::menu_item(ui, "Abrir").clicked() {
                                        acts.push(Action::RunFrom(bi, pi));
                                    }
                                    if c::menu_item(ui, &format!("Ir a «{}»", b.name)).clicked() {
                                        acts.push(Action::Select(bi));
                                    }
                                },
                            )
                            .on_hover_text(&hover);
                            c::a11y::tip_on_focus(&r, &hover);
                            if r.clicked() {
                                acts.push(Action::RunFrom(bi, pi));
                            }
                        }
                    });
                    ui.add_space(space::XS);
                    ui.label(text(
                        "Clic para abrir · «Más opciones» o clic derecho para ver más",
                        size::SMALL,
                        Weight::Regular,
                        p.text_faint,
                    ));
                    // The rest are one click away: in the page of the bottle that holds them.
                    if let Some(&(bi, _)) = tiles.get(PROGRAMS_SHOWN) {
                        ui.add_space(space::S);
                        if Button::secondary(&format!("Ver todos ({})", tiles.len()))
                            .tooltip("Todos los programas, con búsqueda")
                            .show(ui)
                            .clicked()
                        {
                            acts.push(Action::Select(bi));
                            acts.push(Action::SetTab(BottleTab::Programs));
                        }
                    }
                }
                ui.add_space(space::XXL);
            });
    }

    /// First run: the three steps, each marked when done.
    fn onboarding(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let p = pal(ui);
        let wine_ok = self.wine.is_some();
        // While Wine's card is above, the card and the button are what matter, and in the
        // smallest window they must both be in view: the intro and the third line go.
        let compact = !wine_ok || self.installing();
        c::card(ui, |ui| {
            ui.add_space(space::L);
            ui.label(text("Empieza así", size::H3, Weight::Semibold, p.text));
            if !compact {
                ui.add_space(space::XS);
                ui.label(text(
                    "Todo es gratis y se instala dentro de esta app.",
                    size::BODY,
                    Weight::Regular,
                    p.text_muted,
                ));
            }
            ui.add_space(space::M);
            let state = |done: bool, now: bool| {
                if done {
                    StepState::Done
                } else if now {
                    StepState::Now
                } else {
                    StepState::Later
                }
            };
            c::step_row(
                ui,
                state(wine_ok, self.installing()),
                "Consigue Wine: el motor libre que entiende los programas de Windows",
            );
            c::step_row(
                ui,
                state(false, false),
                "La app crea la botella por ti: un pequeño Windows para tus programas",
            );
            if !compact {
                c::step_row(
                    ui,
                    state(false, false),
                    "Instala tu programa: elige su instalador o arrástralo a la ventana",
                );
            }
            ui.add_space(space::L);
            // The guided installer makes the bottle by itself, so this is the way in. An empty
            // bottle stays one click away in the sidebar.
            if Button::primary("Instalar mi primer programa")
                .icon(Icon::Download)
                .large()
                .disabled_reason(self.wine_missing_reason().or(self.install_running_reason()))
                .tooltip("Elige el instalador de tu programa; la botella se crea sola (⌘O)")
                .show(ui)
                .clicked()
            {
                acts.push(Action::Install);
            }
            ui.add_space(space::L);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::status::Attention;

    /// «Lista» says nothing; work, use and problems always do.
    #[test]
    fn only_the_usual_state_is_left_unsaid() {
        assert!(!state_worth_showing(&BottleStatus::Ready));
        for s in [
            BottleStatus::Creating,
            BottleStatus::Installing,
            BottleStatus::InUse,
            BottleStatus::Running(vec!["Bloc".into()]),
            BottleStatus::NeedsAttention(Attention::Incomplete),
            BottleStatus::NeedsAttention(Attention::Failed),
        ] {
            assert!(state_worth_showing(&s), "{s:?}");
        }
    }
}
