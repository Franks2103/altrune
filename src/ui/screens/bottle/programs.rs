//! The programs tab: the tiles of a bottle's programs, the toolbar above them (search, look
//! for programs again, open a program, show the C: drive) and what it says when there are none.

use crate::backend::{fold_name, matches_search, opens_steam, Bottle, Listing, Program};
use crate::ui::components::{self as c, Button};
use crate::ui::icons::Icon;
use crate::ui::theme::{self, pal, size, space, text, Weight};
use crate::{Action, App};
use eframe::egui;

/// The content width from which the programs toolbar (search field, «Buscar programas de nuevo»,
/// «Abrir un programa», «Disco C:») fits in one row; it needs about 725 points.
const TOOLBAR_ONE_ROW: f32 = 740.0;

impl App {
    /// What a program's tile shows: open while it runs, «abriéndose» from the click until it
    /// runs, otherwise nothing. The bottle page and Inicio use the same answer.
    pub(crate) fn tile_state(
        &self,
        b: &Bottle,
        prog: &Program,
        running: bool,
    ) -> c::surface::TileState {
        if running {
            c::surface::TileState::Open
        } else if self
            .opening
            .keys()
            .any(|(bottle, file)| *bottle == b.path && *file == prog.path)
        {
            c::surface::TileState::Starting
        } else {
            c::surface::TileState::Idle
        }
    }

    /// The tiles of some of a bottle's programs (by their place in its list).
    fn program_tiles(
        &self,
        ui: &mut egui::Ui,
        acts: &mut Vec<Action>,
        b: &Bottle,
        which: &[usize],
    ) {
        let running = self.running_in(&b.path);
        // A bottle that never finished being made opens nothing: its banner says why.
        let blocked = self.unfinished_reason(b);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(space::M, space::M);
            for &i in which {
                let prog = &b.programs[i];
                let pic = prog
                    .icon
                    .as_ref()
                    .and_then(|ic| self.textures.get(ic))
                    .and_then(|t| t.as_ref());
                let mut hover = if prog.subtitle.is_empty() {
                    format!("Abrir {}", prog.name)
                } else {
                    format!("Abrir {} ({})", prog.name, prog.subtitle)
                };
                if let Some(why) = blocked {
                    hover = format!("{why}\n{hover}");
                }
                // What it runs, to tell apart programs that share a name.
                if let Some(file) = prog.target.as_ref().and_then(|t| t.file_name()) {
                    hover.push_str(&format!("\nEjecuta: {}", file.to_string_lossy()));
                    if !prog.link_args.is_empty() {
                        hover.push_str(&format!(" {}", crate::ui::short(&prog.link_args, 40)));
                    }
                }
                // The same menu opens with a secondary click and with the «Más opciones» button
                // in the tile's corner (Tab, then ↩).
                // From the click on, the tile says it is opening; once it runs, that it is open.
                let state = self.tile_state(b, prog, running.contains(&prog.name.as_str()));
                let r =
                    c::surface::program_tile_in(ui, &prog.name, &prog.subtitle, state, pic, |ui| {
                        if blocked.is_none() && c::menu_item(ui, "Abrir").clicked() {
                            acts.push(Action::RunProgram(i));
                        }
                        if blocked.is_none() && c::menu_item(ui, "Abrir con opciones…").clicked()
                        {
                            acts.push(Action::RunWithOptions(i));
                        }
                        // Only Steam's own tile: it asks Steam to close by itself and opens it
                        // again.
                        if opens_steam(&b.path, &prog.path)
                            && c::menu_item(ui, "Reiniciar Steam")
                                .on_hover_text("Cierra Steam con calma y lo abre otra vez")
                                .clicked()
                        {
                            acts.push(Action::RestartSteam(i));
                        }
                        match prog.listing {
                            Listing::Shown => {
                                if c::menu_item(ui, "Quitar de la lista").clicked() {
                                    acts.push(Action::HideProgram(i));
                                }
                            }
                            Listing::Removed => {
                                if c::menu_item(ui, "Volver a mostrar").clicked() {
                                    acts.push(Action::UnhideProgram(i));
                                }
                            }
                            Listing::Noise => {}
                        }
                    })
                    .on_hover_text(&hover);
                c::a11y::tip_on_focus(&r, &hover);
                if r.clicked() && blocked.is_none() {
                    acts.push(Action::RunProgram(i));
                }
            }
        });
    }

    /// One button of the programs toolbar.
    fn toolbar_button(
        &self,
        ui: &mut egui::Ui,
        acts: &mut Vec<Action>,
        b: &Bottle,
        which: Toolbar,
    ) {
        match which {
            Toolbar::Refresh => {
                if Button::secondary("Buscar programas de nuevo")
                    .icon(Icon::Refresh)
                    .tooltip("Vuelve a mirar qué programas hay instalados en esta botella")
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::RefreshPrograms);
                }
            }
            Toolbar::OpenExe => {
                if Button::secondary("Abrir un programa")
                    .icon(Icon::Play)
                    .tooltip("Para programas que no tienen instalador (un archivo .exe)")
                    .disabled_reason(self.unfinished_reason(b).or(self.wine_missing_reason()))
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::PickExe);
                }
            }
            Toolbar::DriveC => {
                if Button::secondary("Disco C:")
                    .icon(Icon::Drive)
                    .tooltip("Ver los archivos de la botella en Finder")
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::OpenC);
                }
            }
        }
    }

    pub(super) fn programs_tab(
        &self,
        ui: &mut egui::Ui,
        acts: &mut Vec<Action>,
        b: &Bottle,
        search: &mut String,
    ) {
        let p = pal(ui);
        // The toolbar: the search, then the buttons. They share a row on a wide page; on a
        // narrow one the search takes its own row and the buttons wrap under it.
        // The search field and the three buttons share a row only when all of them fit: just
        // above the narrow width they do not, so the page switches to the wrapped layout earlier.
        let width = ui.available_width();
        let one_row =
            !theme::layout::is_narrow(width) && (b.programs.is_empty() || width >= TOOLBAR_ONE_ROW);
        if !one_row {
            if !b.programs.is_empty() {
                // The field adds its own margins and stroke around the width it is given.
                let inner = ui.available_width() - c::SEARCH_FIELD_CHROME;
                c::search_field(ui, search, "Buscar programas", inner);
                ui.add_space(space::S);
            }
            ui.horizontal_wrapped(|ui| {
                for which in [Toolbar::OpenExe, Toolbar::Refresh, Toolbar::DriveC] {
                    self.toolbar_button(ui, acts, b, which);
                }
            });
        } else {
            ui.horizontal(|ui| {
                if !b.programs.is_empty() {
                    c::search_field(ui, search, "Buscar programas", 240.0);
                }
                self.toolbar_button(ui, acts, b, Toolbar::Refresh);
                // Created in the order they are seen, so Tab follows the eye.
                c::surface::trailing_row(ui, "toolbar", |ui| {
                    self.toolbar_button(ui, acts, b, Toolbar::OpenExe);
                    self.toolbar_button(ui, acts, b, Toolbar::DriveC);
                });
            });
        }
        ui.add_space(space::M);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let _follow = c::a11y::follow_focus(ui);
                self.bottle_notices(ui, acts, b);
                // Names are compared without accents and capitals, and the Start Menu folder counts.
                let needle = fold_name(search.trim());
                let matches = |prog: &Program| matches_search(prog, &needle);
                let shown: Vec<usize> = b
                    .shown_programs()
                    .filter(|(_, prog)| matches(prog))
                    .map(|(i, _)| i)
                    .collect();
                let tucked: Vec<usize> = b
                    .tucked_programs()
                    .filter(|(_, prog)| matches(prog))
                    .map(|(i, _)| i)
                    .collect();
                if b.shown_count() == 0 && self.unfinished_reason(b).is_some() {
                    // Nothing to install into: the banner above says what to do instead.
                } else if b.shown_count() == 0 {
                    c::empty_state(
                        ui,
                        Icon::Download,
                        "Todavía no hay programas",
                        "Instala uno con su instalador .exe o .msi, o arrástralo a esta ventana.",
                        |ui| {
                            if Button::primary("Instalar un programa")
                                .icon(Icon::Download)
                                .disabled_reason(self.install_reason(b))
                                .tooltip("Elige su instalador .exe o .msi (⌘O)")
                                .show(ui)
                                .clicked()
                            {
                                acts.push(Action::Install);
                            }
                        },
                    );
                } else if shown.is_empty() {
                    c::empty_state(
                        ui,
                        Icon::Search,
                        "Sin resultados",
                        &format!("Ningún programa de esta botella se llama «{}».", search.trim()),
                        |ui| {
                            if Button::secondary("Borrar la búsqueda").show(ui).clicked() {
                                search.clear();
                            }
                        },
                    );
                } else {
                    self.program_tiles(ui, acts, b, &shown);
                    ui.add_space(space::S);
                    let hint = if self.cfg.mac_launchers && self.launcher_count > 0 {
                        "Clic para abrir · «Más opciones» o clic derecho para ver más · también están en Launchpad"
                    } else {
                        "Clic para abrir · «Más opciones» o clic derecho para ver más"
                    };
                    ui.label(text(hint, size::SMALL, Weight::Regular, p.text_faint));
                }
                if !tucked.is_empty() {
                    ui.add_space(space::L);
                    egui::CollapsingHeader::new(text(
                        format!("Ocultos ({})", tucked.len()),
                        size::BODY,
                        Weight::Medium,
                        p.text_muted,
                    ))
                    .id_salt(("tucked-programs", &b.path))
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label(text(
                            "Desinstaladores, manuales y lo que quitaste de la lista. Siguen aquí por si los necesitas.",
                            size::SMALL,
                            Weight::Regular,
                            p.text_faint,
                        ));
                        ui.add_space(space::S);
                        self.program_tiles(ui, acts, b, &tucked);
                    });
                }

                ui.add_space(space::XXL);
            });
    }
}

/// The buttons of the programs toolbar.
#[derive(Clone, Copy)]
enum Toolbar {
    Refresh,
    OpenExe,
    DriveC,
}
