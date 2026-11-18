//! The app's settings page (Ajustes): the appearance row and, in order, the sections it is
//! made of. Each section has its own file: «Acerca de», Wine's state and the optional engines,
//! and a bottle's graphics choice. Every row says what it does; nothing is offered that does
//! not exist.

pub(super) mod about;
mod engines;
mod graphics;

use crate::backend::Appearance;
use crate::ui::components::{self as c, Button};
use crate::ui::icons::Icon;
use crate::ui::theme::{pal, size, space, text, Weight};
use crate::{Action, App};
use eframe::egui;

use about::ABOUT_TITLE;

/// A folder as the person reads it: their home folder written as «~».
fn shown_folder(path: &std::path::Path) -> String {
    let home = crate::backend::home();
    match path.strip_prefix(&home) {
        Ok(rest) if home.as_os_str() != "/" => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

/// What each appearance is called, and the tooltip that says what it does.
fn appearance_words(appearance: Appearance) -> (&'static str, &'static str) {
    match appearance {
        Appearance::System => ("Automática", "Apariencia igual que tu Mac"),
        Appearance::Light => ("Clara", "Apariencia siempre clara"),
        Appearance::Dark => ("Oscura", "Apariencia siempre oscura"),
    }
}

impl App {
    pub(crate) fn settings_screen(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let p = pal(ui);
        Self::page_header_plain(ui, "Ajustes", "Altrune, Wine y sus motores.");
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let _follow = c::a11y::follow_focus(ui);
                // Errors of the app's own jobs (Wine, engines) stay here until closed.
                self.error_banners(ui, acts, None);
                c::settings_group(ui, "GENERAL", |ui| {
                    self.appearance_row(ui, acts);
                    c::divider(ui);
                    c::setting_row(
                        ui,
                        "Programas en Launchpad y Aplicaciones",
                        "Cada programa de Windows se abre como una app más del Mac, sin pasar por aquí.",
                        |ui| {
                            let mut on = self.cfg.mac_launchers;
                            // «Ver» is drawn to the left of the switch, so it is created
                            // first: Tab goes the way the eye reads.
                            if on && Button::ghost("Ver").show(ui).clicked() {
                                acts.push(Action::ShowLaunchers);
                            }
                            if c::toggle(ui, &mut on, "Programas en Launchpad y Aplicaciones").changed() {
                                acts.push(Action::SetLaunchers(on));
                            }
                        },
                    );
                    c::divider(ui);
                    c::setting_row(
                        ui,
                        "Abrir .exe desde Finder",
                        "En un .exe elige «Abrir con › Altrune» (o «Cambiar todo») una vez: se abrirá en tu botella principal.",
                        |_| {},
                    );
                });

                c::settings_group(ui, "WINE", |ui| {
                    ui.add_space(space::M);
                    self.wine_card(ui, acts, true);
                    ui.add_space(space::M);
                });

                c::settings_group(ui, "MOTORES Y GRÁFICOS", |ui| self.engine_rows(ui, acts));

                c::settings_group(ui, "COPIAS", |ui| {
                    c::setting_row(
                        ui,
                        "Importar una botella",
                        "Desde una copia guardada antes con «Guardar una copia…».",
                        |ui| {
                            let importing = self.app_job.as_deref() == Some(Self::IMPORT_JOB);
                            let other_job = (!importing && self.app_job.is_some())
                                .then_some("Espera a que termine lo que se está haciendo.");
                            if Button::secondary("Importar…")
                                .icon(Icon::Download)
                                .loading(importing)
                                .disabled_reason(other_job)
                                .show(ui)
                                .clicked()
                            {
                                acts.push(Action::Import);
                            }
                        },
                    );
                });
                c::settings_group(ui, ABOUT_TITLE, |ui| self.about_card(ui));
                ui.label(text(
                    format!("Registro de actividad: {}", crate::backend::log_file().display()),
                    size::CAPTION,
                    Weight::Regular,
                    p.text_faint,
                ));
                ui.add_space(space::XXL);
                if self.settings_at_end {
                    ui.scroll_to_cursor_animation(
                        Some(egui::Align::BOTTOM),
                        egui::style::ScrollAnimation::none(),
                    );
                }
            });
    }

    /// «Apariencia»: three choices in one well, Tab and the arrows reach them.
    fn appearance_row(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        c::setting_row_wide(
            ui,
            "Apariencia",
            "Automática sigue a tu Mac: cambia cuando cambia el Mac. Clara y Oscura se quedan fijas.",
            // Three words in one well need more room than a switch.
            250.0,
            |ui| {
                let labels: Vec<String> = Appearance::ALL
                    .iter()
                    .map(|a| appearance_words(*a).0.to_string())
                    .collect();
                let tips: Vec<&str> = Appearance::ALL
                    .iter()
                    .map(|a| appearance_words(*a).1)
                    .collect();
                let current = Appearance::ALL
                    .iter()
                    .position(|a| *a == self.cfg.appearance)
                    .unwrap_or(0);
                if let Some(i) = c::segmented(
                    ui,
                    ui.scope_id().with("appearance"),
                    &labels,
                    &tips,
                    current,
                ) {
                    acts.push(Action::SetAppearance(Appearance::ALL[i]));
                }
            },
        );
    }
}
