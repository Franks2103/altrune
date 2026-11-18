//! The dialogs: new bottle, install a program (its body is in `install.rs`), open a program,
//! rename, delete, and Wine's state. They fade in; ↩ confirms
//! and Esc or a click outside cancels.

use super::shell::progress_since;
use crate::backend::{is_valid_new_bottle_name, sanitize_bottle_name, unique_bottle_name};
use crate::ui::components::{self as c};
use crate::ui::icons::Icon;
use crate::ui::theme::{motion, pal, pal_ctx, size, space, text, Tone, Weight};
use crate::{Action, App, Modal, NameCheck, WineChange};
use eframe::egui;
use std::path::{Path, PathBuf};

/// What the delete dialog adds when the bottle is in use: deleting closes everything in it.
fn delete_warning(open: bool, installing: bool) -> Option<&'static str> {
    match (open, installing) {
        (_, true) => Some(
            "Hay una instalación en curso: se cerrará sin terminar, y los programas abiertos, sin guardar.",
        ),
        (true, false) => Some("Tiene programas abiertos: se cerrarán sin guardar."),
        (false, false) => None,
    }
}

/// What is left of the window's height for a dialog's body: the dialog's own padding, the space
/// around it and its buttons are not part of it.
const DIALOG_CHROME: f32 = 190.0;

/// The least height the body of a dialog is given, however small the window is.
const DIALOG_MIN_BODY: f32 = 120.0;

/// The bottom of a dialog, drawn outside the scrolling body so it is always in view.
enum Footer {
    /// Cancel and the dialog's own action.
    Actions {
        label: &'static str,
        destructive: bool,
        /// Why the action cannot be taken yet, or `None`.
        why: Option<&'static str>,
        confirm: Action,
        cancel: Action,
    },
    /// Wine's help has its own two buttons.
    WineHelp,
    /// The dialog has no buttons here (the install dialog lays out its own).
    Nothing,
}

impl Footer {
    fn actions(
        label: &'static str,
        destructive: bool,
        why: Option<&'static str>,
        confirm: Action,
    ) -> Self {
        Footer::Actions {
            label,
            destructive,
            why,
            confirm,
            cancel: Action::CloseModal,
        }
    }
}

/// The height the scrolling body of a dialog may take in a window `window_h` points high.
fn body_height_cap(window_h: f32) -> f32 {
    (window_h - DIALOG_CHROME).max(DIALOG_MIN_BODY)
}

impl App {
    pub(crate) fn dialogs(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let Some(m) = self.modal.clone() else {
            self.name_check = None;
            self.install_draft = None;
            return;
        };
        let p = pal_ctx(ctx);
        let t = progress_since(ctx, self.modal_since, motion::BASE);
        let resp = egui::Modal::new(egui::Id::unique("dialog"))
            .backdrop_color(p.scrim.gamma_multiply(t))
            .frame(c::modal_frame(p))
            .show(ctx, |ui| {
                ui.multiply_opacity(t);
                ui.set_width(460.0);
                self.dialog_body(ui, &m, acts);
            });
        if resp.should_close() {
            acts.push(Action::CloseModal);
        }
    }

    /// Puts the cursor in a dialog's text field on the frame the dialog opens, and only then:
    /// asking on every frame would keep Tab from ever leaving the field. What is remembered is
    /// the moment of the dialog that was last focused, in one place that is overwritten: a key
    /// of its own for each dialog would leave one more value in the window's memory with every
    /// dialog the person ever opens.
    fn focus_once(ui: &egui::Ui, field: &egui::Response, opened_at: f64) {
        let key = egui::Id::NULL.with("dialog-focus");
        let opened = opened_at.to_bits();
        let first = ui.data_mut(|d| {
            let seen = d.get_temp::<u64>(key);
            d.insert_temp(key, opened);
            seen != Some(opened)
        });
        if first {
            field.request_focus();
        }
    }

    /// The answer for the typed name, read from the disk only when the text changed.
    pub(super) fn checked_name(&mut self, typed: &str) -> NameCheck {
        let input = typed.trim();
        if let Some(c) = self.name_check.as_ref().filter(|c| c.input == input) {
            return c.clone();
        }
        let clean = sanitize_bottle_name(input);
        let check = NameCheck {
            input: input.to_string(),
            unique: if clean.is_empty() {
                String::new()
            } else {
                unique_bottle_name(&clean)
            },
            valid: is_valid_new_bottle_name(input),
            clean,
        };
        self.name_check = Some(check.clone());
        check
    }

    /// A dialog: its content scrolls when the window is too short for it, and its buttons stay
    /// below, always in view. The install dialog lays itself out (it has a scroll area and
    /// buttons of its own).
    fn dialog_body(&mut self, ui: &mut egui::Ui, m: &Modal, acts: &mut Vec<Action>) {
        if matches!(m, Modal::Install) {
            self.install_dialog(ui, acts);
            return;
        }
        let cap = body_height_cap(ui.ctx().content_rect().height());
        let footer = egui::ScrollArea::vertical()
            .max_height(cap)
            .auto_shrink([false, true])
            .show(ui, |ui| self.dialog_content(ui, m, acts))
            .inner;
        match footer {
            Footer::Actions {
                label,
                destructive,
                why,
                confirm,
                cancel,
            } => {
                let a = c::modal_actions(ui, label, destructive, why);
                if a.confirmed {
                    // The bottle is made with the name as it was checked.
                    if matches!(m, Modal::NewBottle) {
                        self.new_name = self.new_name.trim().to_string();
                    }
                    acts.push(confirm);
                } else if a.cancelled {
                    acts.push(cancel);
                }
            }
            Footer::Nothing => {}
            Footer::WineHelp => {
                ui.add_space(space::L);
                // Created left to right, the order the eye reads and Tab visits.
                ui.horizontal(|ui| {
                    Self::pushed_right(ui, "wine-help-buttons", |ui| {
                        if c::Button::ghost("Motores y ajustes…").show(ui).clicked() {
                            acts.push(Action::CloseModal);
                            acts.push(Action::OpenSettings);
                        }
                        if c::Button::secondary("Cerrar").show(ui).clicked() {
                            acts.push(Action::CloseModal);
                        }
                    });
                });
            }
        }
    }

    /// What a dialog says and asks, and the buttons that go under it.
    fn dialog_content(&mut self, ui: &mut egui::Ui, m: &Modal, acts: &mut Vec<Action>) -> Footer {
        match m {
            Modal::NewBottle => self.new_bottle_dialog(ui, acts),
            // Laid out by `dialog_body`, which never asks for its content.
            Modal::Install => Footer::Nothing,
            Modal::Run(_, file) => self.run_dialog(ui, file),
            Modal::Rename(path) => self.rename_dialog(ui, path),
            Modal::Delete(path) => self.delete_dialog(ui, path),
            Modal::CloseForWine(change, bottles) => self.close_for_wine_dialog(ui, change, bottles),
            Modal::WineHelp => self.wine_help_dialog(ui, acts),
        }
    }

    /// The new bottle dialog: its name, the options folded away, and the Wine banner when Wine is missing.
    fn new_bottle_dialog(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) -> Footer {
        let p = pal(ui);
        c::modal_header(
            ui,
            Icon::Bottle,
            Tone::Accent,
            "Nueva botella",
            "Un pequeño Windows para tus programas. Ponle el nombre que quieras.",
        );
        ui.label(text("Nombre", size::SMALL, Weight::Semibold, p.text_muted));
        ui.add_space(space::XS);
        let r = ui
            .add(
                egui::TextEdit::singleline(&mut self.new_name)
                    .desired_width(f32::INFINITY)
                    .margin(egui::Margin::symmetric(10, 7))
                    .font(crate::ui::theme::font(size::BODY_L, Weight::Regular)),
            )
            .accessible_name("Nombre de la botella");
        Self::focus_once(ui, &r, self.modal_since);
        let checked = self.checked_name(&self.new_name.clone());
        let clean = checked.clean;
        if !clean.is_empty() {
            let final_name = checked.unique;
            if final_name != self.new_name.trim() {
                ui.add_space(space::XS);
                ui.label(text(
                    format!("Se guardará como «{final_name}»"),
                    size::SMALL,
                    Weight::Regular,
                    p.text_muted,
                ));
            }
        }
        ui.add_space(space::M);
        // A newcomer needs neither choice: the defaults are Windows 10 and Wine's own
        // engine, so they wait under «Opciones».
        egui::CollapsingHeader::new(text("Opciones", size::BODY, Weight::Medium, p.text_muted))
            .id_salt("newopts")
            .show(ui, |ui| {
                ui.label(text(
                    "Versión de Windows",
                    size::SMALL,
                    Weight::Semibold,
                    p.text_muted,
                ));
                ui.add_space(space::XS);
                for (code, label) in [
                    ("win10", "Windows 10 (recomendado)"),
                    ("win11", "Windows 11"),
                    ("win7", "Windows 7 (programas antiguos)"),
                ] {
                    ui.radio_value(&mut self.new_windows, code.to_string(), label);
                }
                if self.engines.games.is_some() {
                    ui.add_space(space::M);
                    ui.checkbox(
                        &mut self.new_games,
                        format!(
                            "Usar «{}» (DirectX 11/12 por Metal)",
                            crate::domain::engines::GAMES
                        ),
                    );
                    ui.label(text(
                        format!(
                        "Para juegos modernos. Steam no lo necesita: al abrirlo pasa solo a «{}».",
                        crate::domain::engines::LAUNCHERS
                    ),
                        size::SMALL,
                        Weight::Regular,
                        p.text_muted,
                    ));
                }
            });
        if self.wine.is_none() {
            ui.add_space(space::M);
            c::banner(
                ui,
                Tone::Warning,
                Icon::Alert,
                "Primero consigue Wine",
                "Es gratis y se instala dentro de esta app.",
                |ui| {
                    // The installation shows its progress on the overview and in the
                    // sidebar, so the dialog makes way for it.
                    if c::Button::primary("Instalar Wine gratis")
                        .icon(Icon::Download)
                        .show(ui)
                        .clicked()
                    {
                        acts.push(Action::CloseModal);
                        acts.push(Action::InstallWine);
                    }
                },
            );
        }
        let why = if self.wine.is_none() {
            Some("Primero consigue Wine: es gratis.")
        } else if clean.is_empty() {
            Some("Escribe un nombre para la botella.")
        } else {
            None
        };
        Footer::actions("Crear botella", false, why, Action::CreateBottle)
    }

    /// The dialog that opens a program: its options and whether to keep it in the list.
    fn run_dialog(&mut self, ui: &mut egui::Ui, file: &Path) -> Footer {
        let p = pal(ui);
        let name = file
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        c::modal_header(ui, Icon::Play, Tone::Accent, "Abrir programa", &name);
        ui.label(text(
            "Opciones del programa (opcional)",
            size::SMALL,
            Weight::Semibold,
            p.text_muted,
        ));
        ui.add_space(space::XS);
        ui.add(
            egui::TextEdit::singleline(&mut self.run_args)
                .hint_text("Ej.: -windowed")
                .margin(egui::Margin::symmetric(10, 7))
                .desired_width(f32::INFINITY),
        )
        .accessible_name("Opciones del programa");
        ui.add_space(space::M);
        ui.checkbox(&mut self.run_save, "Guardar en la lista de programas");
        egui::CollapsingHeader::new(text(
            "Opciones avanzadas",
            size::BODY,
            Weight::Medium,
            p.text_muted,
        ))
        .id_salt("runadv")
        .show(ui, |ui| {
            ui.checkbox(
                &mut self.run_hud,
                "Mostrar el rendimiento en pantalla (Metal HUD)",
            );
            ui.checkbox(
                &mut self.run_avx,
                "Ocultar AVX al programa (si se cierra solo al abrir)",
            );
        });
        Footer::actions("Abrir", false, None, Action::RunPicked)
    }

    /// The dialog that renames a bottle, which warns when the name cannot be used.
    fn rename_dialog(&mut self, ui: &mut egui::Ui, path: &Path) -> Footer {
        let p = pal(ui);
        c::modal_header(ui, Icon::Edit, Tone::Accent, "Cambiar nombre", "");
        let r = ui
            .add(
                egui::TextEdit::singleline(&mut self.rename_text)
                    .desired_width(f32::INFINITY)
                    .margin(egui::Margin::symmetric(10, 7))
                    .font(crate::ui::theme::font(size::BODY_L, Weight::Regular)),
            )
            .accessible_name("Nuevo nombre de la botella");
        Self::focus_once(ui, &r, self.modal_since);
        let current = self
            .bottle_at(path)
            .map(|b| b.name.clone())
            .unwrap_or_default();
        let checked = self.checked_name(&self.rename_text.clone());
        let ok = checked.input == current || checked.valid;
        if !ok {
            ui.add_space(space::S);
            let hint = if checked.unique.is_empty() {
                String::new()
            } else {
                format!(" Prueba «{}».", checked.unique)
            };
            ui.label(text(
                format!("Ese nombre ya existe o tiene signos como / : * ? \" < > |.{hint}"),
                size::SMALL,
                Weight::Regular,
                p.warning,
            ));
        }
        let why =
            (!ok).then_some("Ese nombre no sirve: ya existe o tiene signos que no se pueden usar.");
        Footer::actions("Guardar", false, why, Action::DoRename)
    }

    /// The dialog that confirms moving a bottle to the Trash.
    fn delete_dialog(&self, ui: &mut egui::Ui, path: &PathBuf) -> Footer {
        let p = pal(ui);
        let name = self
            .bottle_at(path)
            .map(|b| b.name.clone())
            .unwrap_or_default();
        c::modal_header(ui, Icon::Trash, Tone::Danger, "¿Eliminar esta botella?", "");
        ui.label(text(
            format!(
                "«{name}» y todos los programas instalados en ella irán a la Papelera. Podrás recuperarla desde allí mientras no la vacíes."
            ),
            size::BODY,
            Weight::Regular,
            p.text,
        ));
        let installing = self
            .install
            .as_ref()
            .is_some_and(|s| s.active() && s.bottle == *path);
        let open = self.live.contains(path) || !self.running_in(path).is_empty();
        if let Some(warning) = delete_warning(open, installing) {
            ui.add_space(space::S);
            ui.label(text(warning, size::SMALL, Weight::Medium, p.warning));
        }
        Footer::actions("Mover a la Papelera", true, None, Action::DoDelete)
    }

    /// The dialog that says which bottles close before Wine changes.
    fn close_for_wine_dialog(
        &self,
        ui: &mut egui::Ui,
        change: &WineChange,
        bottles: &[(PathBuf, String)],
    ) -> Footer {
        let p = pal(ui);
        let reinstall = matches!(change, WineChange::Reinstall(_));
        c::modal_header(
            ui,
            Icon::Alert,
            Tone::Warning,
            if reinstall {
                "¿Reinstalar Wine?"
            } else {
                "¿Cambiar de Wine?"
            },
            "",
        );
        let names = bottles
            .iter()
            .map(|(_, n)| format!("«{n}»"))
            .collect::<Vec<_>>()
            .join(", ");
        ui.label(text(
            format!(
                "Para cambiar de Wine se cerrarán los programas de: {names}. Guarda lo que tengas abierto antes de seguir."
            ),
            size::BODY,
            Weight::Regular,
            p.text,
        ));
        Footer::Actions {
            label: "Cerrar y cambiar",
            destructive: true,
            why: None,
            confirm: Action::ConfirmWineChange,
            cancel: Action::CancelWineChange,
        }
    }

    /// Wine's state, as the dialog the sidebar opens.
    fn wine_help_dialog(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>) -> Footer {
        c::modal_header(
            ui,
            Icon::Layers,
            Tone::Accent,
            "Wine, el motor gratuito",
            "Un programa libre que abre programas de Windows en el Mac. Esta app lo usa por dentro: no pagas nada.",
        );
        self.wine_card(ui, acts, true);
        Footer::WineHelp
    }
}

#[cfg(test)]
mod tests {
    use super::{body_height_cap, delete_warning, DIALOG_MIN_BODY};

    /// A dialog's body never takes the whole window (its buttons and padding need room), and
    /// never shrinks to nothing in a very small one.
    #[test]
    fn a_dialog_body_leaves_room_for_the_buttons() {
        assert!(body_height_cap(540.0) < 540.0 - 100.0);
        assert!(body_height_cap(1000.0) > body_height_cap(540.0));
        assert_eq!(body_height_cap(100.0), DIALOG_MIN_BODY);
    }

    #[test]
    fn deleting_a_bottle_in_use_says_what_will_close() {
        assert_eq!(delete_warning(false, false), None);
        assert!(delete_warning(true, false).is_some_and(|w| w.contains("sin guardar")));
        assert!(delete_warning(true, true).is_some_and(|w| w.contains("instalación")));
        assert!(delete_warning(false, true).is_some());
    }
}
