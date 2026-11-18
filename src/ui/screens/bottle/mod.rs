//! A bottle's page: its name, engine and state, the main action (install), and three tabs:
//! the programs (what is used every day), its configuration, and the diagnosis. This file has
//! the page itself (header, notices, the menu); each tab lives in its own file.

mod diagnostics;
mod programs;
mod settings;

use crate::backend::{windows_label, Bottle};
use crate::domain::status::{Attention, BottleStatus};
use crate::ui::components::{self as c, Button};
use crate::ui::icons::Icon;
use crate::ui::theme::{self, pal, size, space, text, Tone, Weight};
use crate::{Action, App, BottleTab};
use eframe::egui::{self, RichText};

/// The label of a tab: its word and, when there is something to count, one space and the count.
fn tab_label(word: &str, count: Option<usize>) -> String {
    match count {
        Some(n) => format!("{word} {n}"),
        None => word.to_string(),
    }
}

impl App {
    pub(crate) fn bottle_screen(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let mut search = std::mem::take(&mut self.search);
        if let Some(b) = self.bottle() {
            self.bottle_page(ui, acts, b, &mut search);
        }
        self.search = search;
    }

    fn bottle_page(
        &self,
        ui: &mut egui::Ui,
        acts: &mut Vec<Action>,
        b: &Bottle,
        search: &mut String,
    ) {
        let p = pal(ui);
        let status = self.bottle_status(b);
        // While a program opens or installs, the status says so, but Wine may already run here:
        // the close controls stay for as long as something is open.
        let in_use = status.is_in_use()
            || (matches!(status, BottleStatus::Starting(_) | BottleStatus::Installing)
                && (self.live.contains(&b.path) || !self.running_in(&b.path).is_empty()));

        // Header: who it is, how it runs, and what to do with it. On a narrow page the actions
        // go under the name instead of beside it, where they would not fit.
        let narrow = theme::layout::is_narrow(ui.available_width());
        ui.horizontal(|ui| {
            c::bottle_mark(ui, &b.name, 50.0, self.bottle_picture(b));
            ui.add_space(space::S);
            // Room for the actions on the right; a long name ends in "…" instead of under them.
            let actions_w = match (narrow, in_use) {
                (true, _) => 0.0,
                (false, true) => 440.0,
                (false, false) => 270.0,
            };
            ui.vertical(|ui| {
                ui.set_max_width((ui.available_width() - actions_w).max(160.0));
                let title = ui.add(
                    egui::Label::new(text(&b.name, size::H2, Weight::Bold, p.text)).truncate(),
                );
                c::a11y::heading(&title, 1);
                title.on_hover_text(&b.name);
                ui.add_space(space::XXS);
                // The badges wrap onto a second line instead of running past the edge.
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(space::XS + 2.0, space::XS);
                    // The automatic engine says nothing to someone who chose nothing: no badge.
                    if let Some(engine) = crate::domain::engines::engine_badge(
                        self.cfg
                            .bottle_ref(&b.path)
                            .and_then(|s| s.engine.as_deref()),
                    ) {
                        c::badge(ui, engine, Tone::Accent)
                            .on_hover_text("Gráficos de esta botella");
                    }
                    c::badge(ui, &windows_label(&b.windows), Tone::Neutral);
                    // Work in progress turns a spinner beside its label, at the top of the page;
                    // any other state carries its icon inside the badge.
                    let label = crate::ui::short(&status.label(), 40);
                    let state = if status.is_working() {
                        c::spinner(ui, 14.0);
                        c::badge(ui, &label, theme::status_tone(&status))
                    } else {
                        c::badge_icon(
                            ui,
                            theme::status_icon(&status),
                            &label,
                            theme::status_tone(&status),
                        )
                    };
                    if let Some(detail) = status.detail() {
                        state.on_hover_text(detail);
                    }
                    if self.is_default(b) {
                        c::badge(ui, "Principal", Tone::Neutral)
                            .on_hover_text("Los .exe abiertos desde Finder van aquí");
                    }
                });
            });
            if !narrow {
                c::surface::trailing_row(ui, "header-actions", |ui| {
                    self.header_actions(ui, acts, b, in_use);
                });
            }
        });
        if narrow {
            ui.add_space(space::S);
            ui.horizontal_wrapped(|ui| self.header_actions(ui, acts, b, in_use));
        }
        ui.add_space(space::L);
        // What failed in this bottle stays here, above whichever tab is open, until it is
        // closed or the bottle works again.
        self.error_banners(ui, acts, Some(&b.path));
        // The installation going on here: what it is doing, and what to do next.
        self.install_card_for(ui, acts, b);

        let found = self
            .diagnosis
            .result(&b.path)
            .map_or(0, |d| d.findings.len());
        let labels = [
            tab_label("Programas", Some(b.shown_count())),
            "Configuración".to_string(),
            tab_label("Diagnóstico", (found > 0).then_some(found)),
        ];
        let current = match self.tab {
            BottleTab::Programs => 0,
            BottleTab::Settings => 1,
            BottleTab::Diagnostics => 2,
        };
        let tips = [
            "Los programas instalados en esta botella",
            "Cómo se ve y se dibuja todo aquí (⌘,)",
            "Busca la causa de un programa que no abre o se cierra",
        ];
        if let Some(i) = c::segmented(ui, ui.scope_id().with("tabs"), &labels, &tips, current) {
            acts.push(Action::SetTab(match i {
                0 => BottleTab::Programs,
                1 => BottleTab::Settings,
                _ => BottleTab::Diagnostics,
            }));
        }
        ui.add_space(space::L);

        match self.tab {
            BottleTab::Programs => self.programs_tab(ui, acts, b, search),
            BottleTab::Settings => self.settings_tab(ui, acts, b),
            BottleTab::Diagnostics => self.diagnostics_tab(ui, acts, b),
        }
    }

    /// The main actions of the header, left to right in the order they are read and reached
    /// with Tab: the main action first, the menu last.
    fn header_actions(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>, b: &Bottle, in_use: bool) {
        #[derive(Clone, Copy)]
        enum Part {
            Install,
            Close,
            More,
        }
        for part in [Part::Install, Part::Close, Part::More] {
            match part {
                Part::More => self.more_menu(ui, acts, b, in_use),
                Part::Close => {
                    // Closing shows its spinner; any other job that holds the bottle says so.
                    let closing = self.busy.get(&b.path).map(String::as_str) == Some(Self::CLOSING);
                    let held = self.busy_reason(&b.path).filter(|_| !closing);
                    if in_use
                        && Button::secondary(if closing {
                            "Cerrando…"
                        } else {
                            "Cerrar programas"
                        })
                        .icon(Icon::Stop)
                        .loading(closing)
                        .disabled_reason(held.as_deref())
                        .tooltip("Cierra con calma los programas abiertos de esta botella")
                        .show(ui)
                        .clicked()
                    {
                        acts.push(Action::Quit(false));
                    }
                }
                Part::Install => {
                    let why = self.install_reason(b);
                    if Button::primary("Instalar un programa")
                        .icon(Icon::Download)
                        .disabled_reason(why)
                        .tooltip("Elige su instalador .exe o .msi (⌘O)")
                        .show(ui)
                        .clicked()
                    {
                        acts.push(Action::Install);
                    }
                }
            }
        }
    }

    /// Why a program cannot be installed in this bottle, or `None` when it can.
    pub(crate) fn install_reason(&self, b: &Bottle) -> Option<&'static str> {
        self.unfinished_reason(b)
            .or(self.wine_missing_reason())
            .or(self.install_running_reason())
    }

    /// Why nothing can be installed or opened here because the bottle never finished being
    /// made, or `None`.
    pub(crate) fn unfinished_reason(&self, b: &Bottle) -> Option<&'static str> {
        self.bottle_status(b).blocks_use()
    }

    /// What needs attention in this bottle, above whichever tab is open (inside its scroll area,
    /// so a small window can still reach the tab under it): the state of the bottle, and Wine
    /// when it is missing or installing.
    fn bottle_notices(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>, b: &Bottle) {
        // A state that needs attention says what is wrong and what to do, in words: the chip
        // and the dot alone leave the person with nothing to act on.
        if let Some(why) = self.bottle_status(b).to_explain() {
            c::banner(
                ui,
                Tone::Warning,
                Icon::Alert,
                why.banner_title().unwrap_or_default(),
                why.banner_body(),
                |ui| match why {
                    // No repair: Wine would have to start over in a folder it left half made.
                    Attention::Incomplete => {
                        if Button::secondary("Eliminar…")
                            .icon(Icon::Trash)
                            .disabled_reason(self.busy_reason(&b.path).as_deref())
                            .tooltip("Pasa la botella a la Papelera")
                            .show(ui)
                            .clicked()
                        {
                            acts.push(Action::AskDelete);
                        }
                        if Button::primary("Crear otra botella")
                            .icon(Icon::Plus)
                            .show(ui)
                            .clicked()
                        {
                            acts.push(Action::OpenNewBottle);
                        }
                    }
                    Attention::EngineMissing | Attention::Failed => {
                        if self.tab != BottleTab::Settings
                            && Button::secondary("Ir a Configuración").show(ui).clicked()
                        {
                            acts.push(Action::SetTab(BottleTab::Settings));
                        }
                    }
                },
            );
            ui.add_space(space::L);
        }
        if self.wine.is_none() || self.installing() {
            self.wine_card(ui, acts, false);
            ui.add_space(space::L);
        }
    }

    /// Everything that is not done every day, in one menu.
    fn more_menu(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>, b: &Bottle, in_use: bool) {
        let p = pal(ui);
        // What a job that holds the bottle blocks: each of these says why it is grey.
        let held = self.busy_reason(&b.path);
        let unfinished = self.unfinished_reason(b);
        let menu = egui::containers::menu::MenuButton::new(text(
            "Más",
            size::BODY,
            Weight::Medium,
            p.text,
        ))
        .config(egui::containers::menu::MenuConfig::new().style(c::a11y::menu_style(p)));
        let (button, _) = menu.ui(ui, |ui| {
            ui.set_min_width(210.0);
            let held = held.as_deref();
            // An entry that cannot be used now is grey and says why.
            let mut item = |ui: &mut egui::Ui, label: &str, action: Action, why: Option<&str>| {
                let r = ui.add_enabled(why.is_none(), egui::Button::new(label));
                if let Some(why) = why {
                    c::a11y::describe(&r, why);
                    r.on_disabled_hover_text(why);
                } else if r.clicked() {
                    acts.push(action);
                    ui.close();
                }
            };
            item(
                ui,
                "Abrir un programa…",
                Action::PickExe,
                held.or(unfinished),
            );
            item(ui, "Abrir el disco C:", Action::OpenC, None);
            item(ui, "Mostrar en Finder", Action::ShowFinder, None);
            ui.separator();
            if !self.is_default(b) {
                item(ui, "Usar como botella principal", Action::SetDefault, None);
            }
            item(ui, "Cambiar nombre…", Action::OpenRename, held);
            item(ui, "Duplicar", Action::Duplicate, held);
            item(ui, "Guardar una copia…", Action::Export, held);
            ui.separator();
            if in_use {
                item(ui, "Cerrar programas", Action::Quit(false), held);
            }
            if ui
                .button("Forzar cierre")
                .on_hover_text("Cierra todo de golpe si algo se quedó colgado")
                .clicked()
            {
                acts.push(Action::Quit(true));
                ui.close();
            }
            ui.separator();
            let delete = ui.add_enabled(
                held.is_none(),
                egui::Button::new(RichText::new("Eliminar botella…").color(p.danger)),
            );
            if let Some(why) = held {
                c::a11y::describe(&delete, why);
                delete.on_disabled_hover_text(why);
            } else if delete.clicked() {
                acts.push(Action::AskDelete);
                ui.close();
            }
        });
        // The menu button gets the same keyboard outline as the tiles' menus, says it opens a
        // menu, and says whose actions they are.
        c::a11y::opens_menu(&button);
        c::a11y::describe(&button, "Más acciones de esta botella");
    }
}

#[cfg(test)]
mod tests {
    use super::tab_label;

    /// A counted tab has one space between its word and its count, and an uncounted one is
    /// just its word.
    #[test]
    fn a_tab_label_has_a_single_space_before_its_count() {
        assert_eq!(tab_label("Programas", Some(2)), "Programas 2");
        assert_eq!(tab_label("Programas", Some(0)), "Programas 0");
        assert_eq!(tab_label("Diagnóstico", None), "Diagnóstico");
        assert!(!tab_label("Diagnóstico", Some(3)).contains("  "));
    }
}
