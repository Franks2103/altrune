//! The diagnostics tab: the problems the app found in a bottle, each as a card with what was
//! seen, the probable cause and the one-click fixes that can be offered.

use crate::backend::Bottle;
use crate::ui::components::{self as c, Button};
use crate::ui::icons::Icon;
use crate::ui::theme::{self, pal, radius, size, space, text, Tone, Weight};
use crate::{Action, App};
use eframe::egui::{self, Margin, RichText, Stroke};

impl App {
    pub(super) fn diagnostics_tab(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>, b: &Bottle) {
        let p = pal(ui);
        let working = self.diagnosis.is_working(&b.path);
        // Only what was found in this bottle is shown here, never another bottle's.
        let result = self.diagnosis.result(&b.path);
        let in_use = self.live.contains(&b.path) || !self.running_in(&b.path).is_empty();
        let buttons = RemedyButtons {
            dxmt: self.engines.metal.is_some(),
            in_use,
            video: self.engines.metal.is_some() && !self.engines.video,
            busy: self.installing(),
            adding_video: self.installing() && self.installing_job() == Self::VIDEO_JOB,
            // The engine is being installed for this bottle, or the bottle is being moved to it.
            engine_working: matches!(
                self.busy.get(&b.path).map(String::as_str),
                Some(Self::ENGINE_CHECK_JOB | Self::ENGINE_SWITCH_JOB)
            ) || self
                .pending_engine_bottle
                .as_ref()
                .is_some_and(|(path, _)| *path == b.path),
        };
        // The report is made from the same reading: it waits while that runs.
        let reading = working.then_some("Estoy analizando los mensajes: un momento.");
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let _follow = c::a11y::follow_focus(ui);
                self.bottle_notices(ui, acts, b);
                let intro = "Se leen los mensajes del último intento en esta botella y los registros del programa. Nada sale de tu Mac.";
                // The search first, then the report: the order they are seen and reached with Tab.
                let top_buttons = |ui: &mut egui::Ui, acts: &mut Vec<Action>| {
                    if Button::secondary("Buscar problemas")
                        .icon(Icon::Pulse)
                        .loading(working)
                        .tooltip("Lee los mensajes de esta botella y dice qué encontró")
                        .show(ui)
                        .clicked()
                    {
                        acts.push(Action::Diagnose);
                    }
                    if Button::secondary("Guardar informe…")
                        .icon(Icon::Upload)
                        .disabled_reason(reading)
                        .tooltip("Oculta los datos personales conocidos; léelo antes de compartirlo")
                        .show(ui)
                        .clicked()
                    {
                        acts.push(Action::ExportReport);
                    }
                };
                if theme::layout::is_narrow(ui.available_width()) {
                    // The sentence first, then its buttons under it.
                    ui.label(text(intro, size::BODY, Weight::Regular, p.text_muted));
                    ui.add_space(space::S);
                    ui.horizontal_wrapped(|ui| top_buttons(ui, acts));
                } else {
                    ui.horizontal(|ui| {
                        // The sentence wraps in the room the buttons leave, instead of running
                        // under them.
                        ui.vertical(|ui| {
                            ui.set_max_width((ui.available_width() - 360.0).max(240.0));
                            ui.label(text(intro, size::BODY, Weight::Regular, p.text_muted));
                        });
                        c::surface::trailing_row(ui, "diagnosis-buttons", |ui| {
                            top_buttons(ui, acts);
                        });
                    });
                }
                ui.add_space(space::L);

                match result {
                    None => c::empty_state(
                        ui,
                        Icon::Pulse,
                        "Busca la causa de un fallo",
                        "Si un programa no abre, se cierra o se ve mal, el diagnóstico dice qué pasó y qué probar.",
                        |_| {},
                    ),
                    // Nothing was read: that is not the same as "all fine".
                    Some(d) if d.lines == 0 && d.findings.is_empty() => c::banner(
                        ui,
                        Tone::Neutral,
                        Icon::Info,
                        "Todavía no hay mensajes de esta botella",
                        "Abre el programa desde aquí y vuelve a buscar. Lo que abres desde un acceso directo del Mac no deja mensajes en la app.",
                        |_| {},
                    ),
                    Some(d) if d.findings.is_empty() => c::banner(
                        ui,
                        Tone::Neutral,
                        Icon::Info,
                        "No se reconoció ningún problema conocido",
                        &format!(
                            "{} de {}. Eso no quiere decir que todo esté bien: solo que ninguna coincide con lo que la app sabe reconocer. Si algo sigue fallando, guarda el informe para pedir ayuda.",
                            crate::domain::status::lines_read(d.lines),
                            d.sources_label()
                        ),
                        |ui| {
                            if Button::secondary("Guardar informe…")
                                .icon(Icon::Upload)
                                .disabled_reason(reading)
                                .show(ui)
                                .clicked()
                            {
                                acts.push(Action::ExportReport);
                            }
                        },
                    ),
                    Some(d) => {
                        ui.label(text(
                            format!(
                                "{} de {}. Primero lo que está mejor comprobado.",
                                crate::domain::status::lines_read(d.lines),
                                d.sources_label()
                            ),
                            size::SMALL,
                            Weight::Regular,
                            p.text_faint,
                        ));
                        ui.add_space(space::S);
                        for f in &d.findings {
                            finding_card(ui, f, acts, &buttons);
                            ui.add_space(space::M);
                        }
                    }
                }

                ui.add_space(space::L);
                egui::CollapsingHeader::new(text("Detalles técnicos", size::BODY, Weight::Medium, p.text_muted))
                    .id_salt("log")
                    .show(ui, |ui| {
                        egui::Frame::NONE
                            .fill(p.sunken)
                            .stroke(Stroke::new(1.0, p.border))
                            .corner_radius(radius::LG)
                            .inner_margin(Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                egui::ScrollArea::vertical()
                                    .max_height(220.0)
                                    .stick_to_bottom(true)
                                    .show(ui, |ui| {
                                        // Read under the lock, without copying the whole log.
                                        let lines = self.log.lock().unwrap_or_else(|e| e.into_inner());
                                        if lines.is_empty() {
                                            ui.label(RichText::new("Nada todavía.").monospace().color(p.text_faint));
                                        }
                                        let skip = lines.len().saturating_sub(300);
                                        for l in lines.iter().skip(skip) {
                                            ui.label(RichText::new(l).monospace().size(size::CAPTION).color(p.text_muted));
                                        }
                                    });
                            });
                        ui.add_space(space::XS);
                        ui.label(text(
                            format!("Registro completo: {}", crate::backend::log_file().display()),
                            size::CAPTION,
                            Weight::Regular,
                            p.text_faint,
                        ));
                        ui.label(text(
                            format!("Carpeta: {}", b.path.display()),
                            size::CAPTION,
                            Weight::Regular,
                            p.text_faint,
                        ));
                    });
                ui.add_space(space::XXL);
            });
    }
}

/// Which one-click fixes can be offered right now.
struct RemedyButtons {
    /// The DXMT engine is installed.
    dxmt: bool,
    /// Something is running in the bottle (changing its engine would close it).
    in_use: bool,
    /// The DXMT engine is installed and still lacks video.
    video: bool,
    /// An install is in progress.
    busy: bool,
    /// The install in progress is the video one (its button shows the spinner).
    adding_video: bool,
    /// The bottle's engine is being installed or changed (the engine button shows the spinner).
    engine_working: bool,
}

/// One problem found: how sure the app is, what was seen, the probable cause and what to try.
fn finding_card(
    ui: &mut egui::Ui,
    f: &crate::diagnostics::Finding,
    acts: &mut Vec<Action>,
    buttons: &RemedyButtons,
) {
    use crate::diagnostics::{Confidence, Remedy};
    let p = pal(ui);
    let (tone, icon_color) = match f.confidence {
        Confidence::Comprobado => (Tone::Warning, p.warning),
        Confidence::Probable => (Tone::Accent, p.warning),
        Confidence::Pista => (Tone::Neutral, p.text_faint),
    };
    c::card(ui, |ui| {
        ui.add_space(space::M);
        ui.horizontal_wrapped(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
            crate::ui::icons::paint(ui.painter(), r, Icon::Alert, icon_color);
            ui.label(text(&f.title, size::TITLE, Weight::Semibold, p.text));
            c::badge(ui, f.confidence.label(), tone);
        });
        ui.add_space(space::XXS);
        ui.label(text(
            format!(
                "Fuente: {} · {} {}",
                f.source.label(),
                f.matches,
                if f.matches == 1 { "línea" } else { "líneas" }
            ),
            size::CAPTION,
            Weight::Regular,
            p.text_faint,
        ));
        ui.add_space(space::S);

        // What was seen: facts, apart from the guess below.
        ui.label(text("Se vio", size::SMALL, Weight::Semibold, p.text_muted));
        for line in &f.evidence {
            ui.label(
                RichText::new(crate::ui::short(line, 160))
                    .monospace()
                    .size(size::CAPTION)
                    .color(p.text_muted),
            );
        }
        for done in &f.checked {
            ui.label(text(
                format!("La app comprobó: {done}"),
                size::BODY,
                Weight::Regular,
                p.text_muted,
            ));
        }
        ui.add_space(space::S);

        let cause = if f.confidence == Confidence::Comprobado {
            "Qué significa"
        } else {
            "Causa probable"
        };
        ui.label(text(cause, size::SMALL, Weight::Semibold, p.text_muted));
        ui.label(text(f.explanation, size::BODY, Weight::Regular, p.text));
        ui.add_space(space::S);
        egui::Frame::NONE
            .fill(p.tone_soft(Tone::Accent))
            .corner_radius(radius::MD)
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(text(
                    "Qué probar",
                    size::SMALL,
                    Weight::Semibold,
                    p.tone_text(Tone::Accent),
                ));
                ui.label(text(&f.suggestion, size::BODY, Weight::Regular, p.text));
            });
        match f.remedy {
            Some(Remedy::AddVideo) if buttons.video => {
                ui.add_space(space::S);
                if Button::primary("Añadir video para juegos")
                    .icon(Icon::Download)
                    .loading(buttons.adding_video)
                    .disabled_reason(
                        (buttons.busy && !buttons.adding_video)
                            .then_some("Espera a que termine la instalación en curso."),
                    )
                    .tooltip("Descarga el video para juegos y lo deja junto a «Steam y launchers» (~150 MB)")
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::AddVideo);
                }
            }
            Some(Remedy::UseDxmt) if buttons.dxmt => {
                ui.add_space(space::S);
                let why = if buttons.engine_working {
                    None
                } else if buttons.in_use {
                    Some("Cierra primero los programas de esta botella.")
                } else if buttons.busy {
                    Some("Espera a que termine la instalación en curso.")
                } else {
                    None
                };
                if Button::primary(&format!(
                    "Usar «{}» en esta botella",
                    crate::domain::engines::LAUNCHERS
                ))
                    .loading(buttons.engine_working)
                    .disabled_reason(why)
                    .tooltip("La botella pasa a «Steam y launchers». Puedes volver al anterior en su configuración")
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::UseEngine(crate::wine_setup::Engine::Metal));
                }
            }
            _ => {}
        }
        ui.add_space(space::M);
    });
}
