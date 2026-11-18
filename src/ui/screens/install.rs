//! The guided install: the dialog that asks where a file goes, and the card on the bottle's page
//! that follows the installation step by step.

use crate::app::InstallDraft;
use crate::backend::Bottle;
use crate::domain::engines::engine_label;
use crate::domain::install::{closes_others, FileKind, InstallSession, StepMark, Target};
use crate::ui::components::{self as c, Button, StepState};
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{pal, size, space, text, Tone, Weight};
use crate::{Action, App, BottleTab};
use eframe::egui;
use std::path::PathBuf;

/// How many «Abrir …» buttons the card shows; the rest are in the bottle's list.
const OPEN_BUTTONS: usize = 4;

impl App {
    /// The dialog's body: what the file is, where it goes and why, and the buttons. Nothing
    /// starts until «Instalar» (or «Abrir»).
    pub(crate) fn install_dialog(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        // What had the keyboard focus when the frame began: Enter confirms only when it was the
        // main button, or nothing.
        let focused_before = ui.ctx().memory(|m| m.focused());
        let Some(mut d) = self.install_draft.take() else {
            return;
        };
        let supported = d.detected.supported();
        install_dialog_head(ui, &d, supported);

        let mut why_not: Option<&'static str> = None;
        // The name field of a new bottle, when it is on screen.
        let mut name_id = None;
        // A second press must not start a second installer: while one runs, the main button
        // says why it does nothing (the app also refuses it).
        if supported && self.install.as_ref().is_some_and(|s| s.active()) {
            why_not = Some("Ya hay una instalación en curso: espera a que termine.");
        }
        if supported {
            install_dialog_kind(ui, &mut d);
            self.install_dialog_bottles(ui, &mut d);
            let engine = self.install_dialog_target(ui, &mut d, &mut why_not, &mut name_id);
            self.install_dialog_notes(ui, &d, engine);
        }
        install_dialog_buttons(ui, acts, &d, supported, why_not, name_id, focused_before);
        self.install_draft = Some(d);
    }

    /// The bottles the file can go to, as radio buttons in a short scrolling list, with
    /// «Nueva botella» last.
    fn install_dialog_bottles(&self, ui: &mut egui::Ui, d: &mut InstallDraft) {
        let p = pal(ui);
        ui.add_space(space::M);
        ui.label(text(
            if d.as_installer {
                "Dónde se instala"
            } else {
                "En qué botella se abre"
            },
            size::SMALL,
            Weight::Semibold,
            p.text_muted,
        ));
        ui.add_space(space::XS);
        let choices: Vec<(PathBuf, String)> = d
            .options
            .iter()
            .map(|o| {
                let engine = engine_label(
                    self.cfg
                        .bottle_ref(&o.path)
                        .and_then(|s| s.engine.as_deref()),
                );
                let mut label = format!("{}  ·  {engine}", o.name);
                if o.is_default {
                    label.push_str("  ·  Principal");
                }
                if Target::Existing(o.path.clone()) == d.recommendation.target {
                    label.push_str("  ·  Propuesta");
                }
                (o.path.clone(), label)
            })
            .collect();
        egui::ScrollArea::vertical()
            .id_salt("install-bottles")
            .max_height(120.0)
            .auto_shrink([true, true])
            .show(ui, |ui| {
                let _follow = c::a11y::follow_focus(ui);
                for (path, label) in &choices {
                    ui.radio_value(&mut d.choice, Some(path.clone()), label);
                }
                let new_label = if matches!(d.recommendation.target, Target::New(_)) {
                    "Nueva botella  ·  Propuesta"
                } else {
                    "Nueva botella"
                };
                ui.radio_value(&mut d.choice, None, new_label);
            });
    }

    /// The new bottle's name field when «Nueva botella» is chosen, or the engine of the chosen
    /// bottle. Returns the engine's name for the note under the choices.
    fn install_dialog_target(
        &mut self,
        ui: &mut egui::Ui,
        d: &mut InstallDraft,
        why_not: &mut Option<&'static str>,
        name_id: &mut Option<egui::Id>,
    ) -> &'static str {
        let p = pal(ui);
        let mut engine = engine_label(None);
        // The field takes the keyboard on the frame it appears (the dialog opening on a new
        // bottle, or the person choosing «Nueva botella»), and only then: asking on every
        // frame would keep Tab from ever leaving it.
        // A queued file can reopen this dialog in the very frame the last one closed, so the
        // file is part of the key as well as the opening time.
        // One entry for the whole app says which opening of the dialog (and for which file)
        // already gave the name field the focus. A key per opening would leave one entry
        // behind in egui's memory for every dialog ever opened.
        let shown_key = egui::Id::NULL.with("install-name-shown");
        let opening = egui::Id::NULL
            .with((self.modal_since.to_bits(), &d.file))
            .value();
        match d.choice.clone() {
            None => {
                ui.add_space(space::XS);
                let r = ui
                    .add(
                        egui::TextEdit::singleline(&mut d.name)
                            .hint_text("Nombre de la botella")
                            .desired_width(f32::INFINITY)
                            .margin(egui::Margin::symmetric(10, 7))
                            .font(crate::ui::theme::font(size::BODY_L, Weight::Regular)),
                    )
                    .accessible_name("Nombre de la botella");
                let was_shown = ui.data_mut(|data| {
                    let was = data.get_temp::<(u64, bool)>(shown_key) == Some((opening, true));
                    data.insert_temp(shown_key, (opening, true));
                    was
                });
                if !was_shown {
                    r.request_focus();
                }
                *name_id = Some(r.id);
                let checked = self.checked_name(&d.name.clone());
                if checked.clean.is_empty() {
                    *why_not = Some("Escribe un nombre para la botella.");
                } else if checked.unique != d.name.trim() {
                    ui.add_space(space::XS);
                    ui.label(text(
                        format!("Se guardará como «{}»", checked.unique),
                        size::SMALL,
                        Weight::Regular,
                        p.text_muted,
                    ));
                }
            }
            Some(path) => {
                ui.data_mut(|data| data.insert_temp(shown_key, (opening, false)));
                engine = engine_label(self.cfg.bottle_ref(&path).and_then(|s| s.engine.as_deref()));
            }
        }
        engine
    }

    /// The small print under the choices: why this one is proposed, the engine, the settings
    /// written first and the wait for Wine.
    fn install_dialog_notes(&self, ui: &mut egui::Ui, d: &InstallDraft, engine: &str) {
        let p = pal(ui);
        if d.follows_recommendation() {
            ui.add_space(space::S);
            ui.label(text(
                d.recommendation.reason.clone(),
                size::SMALL,
                Weight::Regular,
                p.text_muted,
            ));
        }
        ui.add_space(space::S);
        ui.label(text(
            format!("Motor de gráficos de esta botella: {engine}. Puedes cambiarlo en su página."),
            size::SMALL,
            Weight::Regular,
            p.text_muted,
        ));
        if let Some(name) = d.fixes.as_ref().filter(|_| d.as_installer) {
            ui.add_space(space::S);
            ui.label(text(
                format!(
                    "Antes de abrir el instalador se escriben en la botella los ajustes que {name} necesita."
                ),
                size::SMALL,
                Weight::Regular,
                p.text_muted,
            ));
        }
        if self.wine.is_none() {
            ui.add_space(space::M);
            let line = if self.wine_state == crate::wine_setup::WineState::Unknown {
                "Estoy buscando Wine en tu Mac: un momento."
            } else {
                "Todavía no hay Wine: se instala primero y el archivo se abre solo cuando termine."
            };
            ui.label(text(line, size::BODY, Weight::Medium, p.warning));
        }
    }

    /// The installation going on in this bottle (or just over, until the person says «Listo»).
    pub(crate) fn install_card_for(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>, b: &Bottle) {
        if let Some(s) = self.install.as_ref().filter(|s| s.bottle == b.path) {
            self.install_card(ui, acts, s, Some(b));
        }
    }

    /// The same card on the overview while the bottle is not on the list yet (Wine being
    /// installed, the bottle being made, or a failure before it existed).
    pub(crate) fn install_card_orphan(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        if let Some(s) = self
            .install
            .as_ref()
            .filter(|s| self.bottle_at(&s.bottle).is_none())
        {
            self.install_card(ui, acts, s, None);
        }
    }

    fn install_card(
        &self,
        ui: &mut egui::Ui,
        acts: &mut Vec<Action>,
        s: &InstallSession,
        bottle: Option<&Bottle>,
    ) {
        let p = pal(ui);
        let buttons = s.card_actions();
        let failed = matches!(
            s.stage,
            crate::domain::install::Stage::Done(crate::domain::install::Outcome::Failed(_))
        );
        c::card(ui, |ui| {
            ui.add_space(space::M);
            ui.label(text(s.title(), size::TITLE, Weight::Semibold, p.text));
            ui.add_space(space::XXS);
            ui.label(text(
                s.headline(),
                size::BODY,
                Weight::Regular,
                if failed { p.danger } else { p.text_muted },
            ));
            let steps = s.steps();
            if !steps.is_empty() {
                ui.add_space(space::M);
                for (mark, label) in steps {
                    match mark {
                        StepMark::Done => c::step_row(ui, StepState::Done, &label),
                        StepMark::Now => c::step_row(ui, StepState::Now, &label),
                        StepMark::Later => c::step_row(ui, StepState::Later, &label),
                        StepMark::NotApplied => c::step_row(ui, StepState::NotApplied, &label),
                        StepMark::Failed => failed_row(ui, &label),
                    }
                    ui.add_space(space::XXS);
                }
            }
            if let Some(advice) = s.advice() {
                ui.add_space(space::S);
                ui.label(text(advice, size::BODY, Weight::Regular, p.text));
            }
            if let Some(note) = s.fixes_note() {
                ui.add_space(space::S);
                ui.label(text(note, size::BODY, Weight::Regular, p.text));
            }
            ui.add_space(space::M);
            ui.horizontal_wrapped(|ui| {
                if buttons.ask_done {
                    ui.label(text(
                        "¿Ya terminó el instalador?",
                        size::BODY,
                        Weight::Medium,
                        p.text,
                    ));
                    if Button::primary("Sí, ya terminó")
                        .tooltip("Busca los programas nuevos ahora y deja de esperar")
                        .show(ui)
                        .clicked()
                    {
                        acts.push(Action::FinishInstall);
                    }
                }
                if buttons.close
                    && Button::secondary("Cerrar todo en esta botella")
                        .icon(Icon::Stop)
                        .tooltip(
                            "Cierra el instalador y los demás programas abiertos de esta botella",
                        )
                        .show(ui)
                        .clicked()
                {
                    acts.push(Action::CloseInstaller);
                }
                if let Some(b) = bottle {
                    for f in s.found.iter().take(OPEN_BUTTONS) {
                        let place = b
                            .programs
                            .iter()
                            .position(|p| p.path.to_string_lossy() == f.id.as_str());
                        let label = format!("Abrir {}", crate::ui::short(&f.name, 28));
                        if let (Some(i), false) = (place, s.active()) {
                            if Button::primary(&label).show(ui).clicked() {
                                acts.push(Action::RunProgram(i));
                            }
                        }
                    }
                    if buttons.find_by_hand
                        && Button::secondary("Buscar el programa en la botella")
                            .tooltip("Elige el .exe del programa dentro de esta botella")
                            .show(ui)
                            .clicked()
                    {
                        acts.push(Action::PickExe);
                    }
                    if buttons.diagnose
                        && Button::secondary("Ver diagnóstico")
                            .tooltip("Lee los mensajes del último intento en esta botella")
                            .show(ui)
                            .clicked()
                    {
                        acts.push(Action::SetTab(BottleTab::Diagnostics));
                        acts.push(Action::Diagnose);
                    }
                }
                // The label says what the button does: cancel a start that has not reached the
                // installer yet (it never answers, or it waits for Wine), or close a finished
                // installation (the installer is never stopped).
                let leave = s.dismiss_label();
                if buttons.dismiss && Button::secondary(leave).show(ui).clicked() {
                    acts.push(Action::DismissInstall);
                }
            });
            if buttons.close
                && closes_others(
                    &s.process_label(),
                    &self.running_in(&s.bottle),
                    self.live.contains(&s.bottle),
                )
            {
                ui.add_space(space::XS);
                ui.label(text(
                    "También se cierran los demás programas de esta botella.",
                    size::SMALL,
                    Weight::Regular,
                    p.text_muted,
                ));
            }
            if !s.found.is_empty() && !s.active() && s.found.len() > OPEN_BUTTONS {
                ui.add_space(space::XS);
                ui.label(text(
                    format!(
                        "Y {} más en la lista de programas.",
                        s.found.len() - OPEN_BUTTONS
                    ),
                    size::SMALL,
                    Weight::Regular,
                    p.text_muted,
                ));
            }
            ui.add_space(space::M);
        });
        ui.add_space(space::L);
    }
}

/// The dialog's header and «Qué es»: what the file is, and the warning about it if there is one.
fn install_dialog_head(ui: &mut egui::Ui, d: &InstallDraft, supported: bool) {
    let p = pal(ui);
    let file_name = d
        .file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let title = if !supported {
        "No puedo abrir este archivo"
    } else if d.as_installer {
        "Instalar un programa"
    } else {
        "Abrir un programa"
    };
    c::modal_header(
        ui,
        if supported {
            Icon::Download
        } else {
            Icon::Alert
        },
        if supported {
            Tone::Accent
        } else {
            Tone::Warning
        },
        title,
        &crate::ui::short(&file_name, 52),
    );

    ui.label(text("Qué es", size::SMALL, Weight::Semibold, p.text_muted));
    ui.add_space(space::XXS);
    ui.label(text(
        d.detected.label(),
        size::BODY_L,
        Weight::Medium,
        p.text,
    ));
    if let Some(warning) = d.detected.warning() {
        ui.add_space(space::XS);
        ui.label(text(warning, size::BODY, Weight::Regular, p.warning));
    }
}

/// A program that is not named like an installer may still be one: the checkbox that says so.
fn install_dialog_kind(ui: &mut egui::Ui, d: &mut InstallDraft) {
    let p = pal(ui);
    // A program that is not named like an installer may still be one.
    if matches!(
        d.detected.kind,
        FileKind::Exe {
            installer_like: false
        }
    ) {
        ui.add_space(space::M);
        ui.checkbox(&mut d.as_installer, "Este archivo instala un programa");
        if !d.as_installer {
            ui.label(text(
                "Se abrirá tal cual y quedará en la lista de programas de la botella.",
                size::SMALL,
                Weight::Regular,
                p.text_muted,
            ));
        }
    }
}

/// The buttons. Enter acts on the main one when it, or nothing, has the focus.
fn install_dialog_buttons(
    ui: &mut egui::Ui,
    acts: &mut Vec<Action>,
    d: &InstallDraft,
    supported: bool,
    why_not: Option<&'static str>,
    name_id: Option<egui::Id>,
    focused_before: Option<egui::Id>,
) {
    ui.add_space(space::L);
    let main_label = if !supported {
        "Entendido"
    } else if d.as_installer {
        "Instalar"
    } else {
        "Abrir"
    };
    let mut confirmed = false;
    let mut cancelled = false;
    let mut main_id = None;
    // Created left to right, the order the eye reads and Tab visits: «Cancelar» first.
    ui.horizontal(|ui| {
        c::surface::trailing_row(ui, "install-actions", |ui| {
            if supported && Button::secondary("Cancelar").show(ui).clicked() {
                cancelled = true;
            }
            let r = Button::primary(main_label)
                .disabled_reason(why_not)
                .show(ui);
            main_id = Some(r.id);
            confirmed |= r.clicked();
        });
    });
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    if enter
        && why_not.is_none()
        && focused_before.is_none_or(|f| Some(f) == main_id || Some(f) == name_id)
    {
        confirmed = true;
    }
    if cancelled || (confirmed && !supported) {
        acts.push(Action::CloseModal);
    } else if confirmed {
        acts.push(Action::ConfirmInstall);
    }
}

/// A step that did not work: a red mark and its words.
fn failed_row(ui: &mut egui::Ui, label: &str) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
        icons::paint(ui.painter(), r.shrink(1.0), Icon::Alert, p.danger);
        // The state is in the words, not only in the red mark, so a screen reader says it too.
        ui.label(text(
            format!("Falló: {label}"),
            size::BODY,
            Weight::Medium,
            p.danger,
        ));
    });
}
