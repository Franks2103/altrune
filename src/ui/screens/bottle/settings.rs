//! The settings tab of a bottle: what most people need first, the technical settings folded
//! under "Avanzado", and the Windows tools that live inside the bottle.

use crate::backend::{sync_var, windows_label, Bottle, Tool, WINDOWS_VERSIONS};
use crate::ui::components::{self as c, Button};
use crate::ui::icons::Icon;
use crate::ui::theme::{pal, size, space, text, Weight};
use crate::{Action, App};
use eframe::egui;

impl App {
    /// A bottle's settings: what most people need first, and the technical ones folded under
    /// "Avanzado".
    pub(super) fn settings_tab(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>, b: &Bottle) {
        let p = pal(ui);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let _follow = c::a11y::follow_focus(ui);
                self.bottle_notices(ui, acts, b);
                c::settings_group(ui, "LO BÁSICO", |ui| {
                    self.graphics_row(ui, acts, b);
                    c::divider(ui);
                    c::setting_row(
                        ui,
                        "Alta resolución",
                        "Letras y ventanas nítidas en pantallas Retina.",
                        |ui| {
                            let mut on = b.high_res;
                            // Wine runs inside the bottle to change this: not while another job
                            // is working on it.
                            let held = self
                                .busy_reason(&b.path)
                                .or(self.unfinished_reason(b).map(str::to_string));
                            let r = c::toggle_held(ui, &mut on, "Alta resolución", held.as_deref());
                            if r.changed() {
                                acts.push(Action::SetHighRes(on));
                            }
                        },
                    );
                    c::divider(ui);
                    c::setting_row(
                        ui,
                        "Versión de Windows",
                        "La que ven los programas. Algunos antiguos funcionan mejor con otra.",
                        |ui| {
                            let mut v = b.windows.clone();
                            // winecfg runs inside the bottle: not while another job holds it.
                            let held = self
                                .busy_reason(&b.path)
                                .or(self.unfinished_reason(b).map(str::to_string));
                            if let Some(why) = held.as_deref() {
                                // A drop-down inside a disabled `Ui` is skipped by Tab and by
                                // VoiceOver; this stand-in keeps the value, and the reason,
                                // reachable.
                                let shown = windows_label(&v);
                                let r = Button::secondary(&shown)
                                    .disabled_reason(Some(why))
                                    .show(ui);
                                c::a11y::button_info(
                                    &r,
                                    &format!("Versión de Windows: {shown}"),
                                    false,
                                    None,
                                    Some(why),
                                );
                            } else {
                                egui::ComboBox::from_id_salt("winver")
                                    .selected_text(windows_label(&v))
                                    .width(170.0)
                                    .show_ui(ui, |ui| {
                                        for (code, label) in WINDOWS_VERSIONS {
                                            ui.selectable_value(&mut v, code.to_string(), *label);
                                        }
                                    })
                                    .response
                                    .accessible_name("Versión de Windows");
                            }
                            if v != b.windows {
                                acts.push(Action::SetWindows(v));
                            }
                        },
                    );
                });
                ui.label(text(
                    "Los cambios se aplican la próxima vez que abras un programa en esta botella.",
                    size::SMALL,
                    Weight::Regular,
                    p.text_faint,
                ));
                ui.add_space(space::L);

                egui::CollapsingHeader::new(text(
                    "Avanzado",
                    size::TITLE,
                    Weight::Semibold,
                    p.text,
                ))
                .id_salt("bottle-advanced")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label(text(
                        "Opciones técnicas. Si todo funciona, no hace falta tocarlas.",
                        size::SMALL,
                        Weight::Regular,
                        p.text_muted,
                    ));
                    ui.add_space(space::M);
                    self.advanced_settings(ui, acts, b);
                });
                ui.add_space(space::XXL);
            });
    }

    /// The technical half of a bottle's settings.
    fn advanced_settings(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>, b: &Bottle) {
        let p = pal(ui);
        c::settings_group(ui, "RENDIMIENTO", |ui| {
            // Only offered when this bottle's Wine really reads a faster-sync switch.
            let sync = self.bottle_wine_cached(b).and_then(|w| sync_var(&w));
            let (title, help) = match sync {
                Some("WINEMSYNC") => (
                    "Sincronización rápida (MSync)",
                    "Los programas y juegos esperan menos entre sus hilos.",
                ),
                Some(_) => (
                    "Sincronización rápida (ESync)",
                    "Los programas y juegos esperan menos entre sus hilos.",
                ),
                None => (
                    "Sincronización rápida",
                    "No disponible: el Wine de esta botella no tiene MSync ni ESync.",
                ),
            };
            c::setting_row(ui, title, help, |ui| {
                let mut on = b.msync && sync.is_some();
                let why = sync
                    .is_none()
                    .then_some("El Wine de esta botella no tiene MSync ni ESync.");
                let r = c::toggle_held(ui, &mut on, title, why);
                if r.changed() {
                    acts.push(Action::SetMsync(on));
                }
            });
        });

        self.engine_extras(ui, acts, b);

        c::settings_group(ui, "HERRAMIENTAS DE WINDOWS", |ui| {
            ui.add_space(space::M);
            ui.label(text(
                "Programas de Windows que viven dentro de esta botella.",
                size::SMALL,
                Weight::Regular,
                p.text_muted,
            ));
            ui.add_space(space::S);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(space::S, space::S);
                let held = self
                    .busy_reason(&b.path)
                    .or(self.unfinished_reason(b).map(str::to_string));
                for t in Tool::ALL {
                    // A tool that is being opened shows its spinner until its window is there.
                    let opening = !self.opening.is_empty()
                        && self
                            .opening
                            .contains_key(&(b.path.clone(), Self::tool_key(t.label())));
                    let why = self.wine_missing_reason().or(held.as_deref());
                    if Button::secondary(t.label())
                        .icon(tool_icon(t))
                        .loading(opening)
                        .tooltip(tool_hint(t))
                        .disabled_reason(why)
                        .show(ui)
                        .clicked()
                    {
                        acts.push(Action::Tool(t));
                    }
                }
            });
            ui.add_space(space::M);
        });

        c::settings_group(ui, "CARPETA", |ui| {
            c::setting_row(
                ui,
                "Ubicación de la botella",
                &crate::ui::short(&b.path.to_string_lossy(), 90),
                |ui| {
                    if Button::secondary("Mostrar en Finder")
                        .icon(Icon::Folder)
                        .tooltip("Abre la carpeta de esta botella en Finder")
                        .show(ui)
                        .clicked()
                    {
                        acts.push(Action::ShowFinder);
                    }
                },
            );
        });
    }
}

/// One sentence on what a Windows tool is for.
fn tool_hint(t: Tool) -> &'static str {
    match t {
        Tool::WineConfig => {
            "Ajustes del Windows de esta botella: versión, unidades, audio y pantalla."
        }
        Tool::ControlPanel => "El panel de control de Windows de esta botella.",
        Tool::Uninstaller => "Quita programas instalados en esta botella.",
        Tool::GameControllers => "Prueba y ajusta los mandos que Windows ve en esta botella.",
        Tool::TaskManager => "Mira y cierra lo que se está ejecutando dentro de esta botella.",
        Tool::Registry => "Para quien ya sabe: cambiar algo mal puede estropear la botella.",
        Tool::CommandPrompt => "Una ventana de comandos de Windows dentro de la botella.",
        Tool::Terminal => "Una terminal del Mac ya preparada para esta botella.",
        Tool::Reboot => "Reinicia el Windows de esta botella, como apagar y encender el PC.",
    }
}

fn tool_icon(t: Tool) -> Icon {
    match t {
        Tool::WineConfig => Icon::Gear,
        Tool::ControlPanel => Icon::Monitor,
        Tool::Uninstaller => Icon::Trash,
        Tool::GameControllers => Icon::Gamepad,
        Tool::TaskManager => Icon::Pulse,
        Tool::Registry => Icon::Layers,
        Tool::CommandPrompt | Tool::Terminal => Icon::Terminal,
        Tool::Reboot => Icon::Refresh,
    }
}
