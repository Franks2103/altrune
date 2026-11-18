//! Wine's state card with the live installation, and the optional engines (Direct3D 12, video,
//! controllers, the two engines of the graphics list) with the texts about what is installing.
//! Apple's software (D3DMetal and the games engine) is shown when it is installed and explained
//! when it is not: Altrune never offers to download it.

use crate::domain::engines::LAUNCHERS;
use crate::ui::components::{self as c, Button, StepState};
use crate::ui::icons::Icon;
use crate::ui::theme::{pal, size, space, text, Tone, Weight};
use crate::wine_setup::{Engine, Step, WineState, APPLE_SOFTWARE_NOT_INSTALLED};
use crate::{Action, App};
use eframe::egui;

/// The name of the install that runs in `progress`: `None` when none runs, empty while the job
/// has no name yet.
fn running_job(progress: &crate::wine_setup::Progress) -> Option<String> {
    progress.running.then(|| progress.flavour.clone())
}

/// Whether the button of the job `own` shows its spinner while `running` is the job that runs.
fn spins_for(running: Option<&str>, own: &str) -> bool {
    running == Some(own)
}

/// What the Direct3D 12 row says when D3DMetal is not there: what it is for, and why Altrune does
/// not bring it.
const D3D12_NOT_INSTALLED: &str = "Los juegos de Direct3D 12, como Red Dead Redemption 2, necesitan D3DMetal de Apple. No se instala desde Altrune: es software de Apple y su licencia no permite que Altrune lo descargue.";

/// What the download step of an install is called: the optional additions are named by what they
/// are, Wine's own engines by the Wine they bring. The size is added once it is known.
fn download_label(flavour: &str, sizes: Option<(u64, u64)>) -> String {
    // The addition and the engine of the table are named by their name, not as a Wine version.
    let named = [App::VIDEO_JOB, LAUNCHERS];
    let what = if named.contains(&flavour) {
        flavour.to_string()
    } else if flavour.is_empty() {
        "Wine".to_string()
    } else {
        format!("Wine {flavour}")
    };
    match sizes {
        Some((done, total)) if total > 0 => {
            format!(
                "Descargar {what} ({} de {} MB)",
                done / 1_000_000,
                total / 1_000_000
            )
        }
        _ => format!("Descargar {what}"),
    }
}

impl App {
    /// The installing job's flavour while one runs ("Direct3D 12", "video", …).
    pub(super) fn installing_what(&self) -> Option<String> {
        running_job(&self.setup.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// The optional engines, app-wide.
    pub(super) fn engine_rows(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let e = &self.engines;
        let busy = self.installing_what();
        if !e.metal_capable {
            c::setting_row(
                ui,
                "Motores por Metal",
                "«Juegos DirectX 12» y «Steam y launchers» necesitan un Mac con chip de Apple y macOS 14 o posterior.",
                |ui| {
                    c::badge(ui, "No disponible", Tone::Neutral);
                },
            );
            return;
        }
        c::setting_row(
            ui,
            &crate::domain::engines::engine_label_technical(Some("games")),
            if e.games.is_some() {
                "Wine 7.7 de Apple con DirectX 11/12 por Metal. Se elige en cada botella."
            } else {
                APPLE_SOFTWARE_NOT_INSTALLED
            },
            |ui| {
                if e.games.is_some() {
                    c::badge(ui, "Instalado", Tone::Success);
                } else {
                    c::badge(ui, "No instalado", Tone::Neutral);
                }
            },
        );
        c::divider(ui);
        c::setting_row(
            ui,
            &crate::domain::engines::engine_label_technical(Some("dxmt")),
            "Wine 11 con DirectX 10/11 por Metal (software libre). Lo necesitan Steam y el launcher de Rockstar.",
            |ui| {
                if e.metal.is_some() {
                    c::badge(ui, "Instalado", Tone::Success);
                } else {
                    c::badge(ui, "Se instala al elegirlo en una botella", Tone::Neutral);
                }
            },
        );
        if e.metal.is_some() {
            c::divider(ui);
            self.d3d12_row(ui);
            c::divider(ui);
            self.video_row(ui, acts, busy.as_deref());
            // Nothing can be done about controllers from here, so the row waits until they are
            // ready instead of reporting a problem with no way to act on it.
            if self.engines.controllers {
                c::divider(ui);
                self.controllers_row(ui);
            }
        }
    }

    /// Why an install cannot start while another one runs: what is installing now (`busy`), except
    /// the very thing this button installs (`own`), which shows its own spinner.
    pub(crate) fn wait_reason(busy: Option<&str>, own: Option<&str>) -> Option<String> {
        // The job's name is empty until its download starts.
        busy.filter(|w| Some(*w) != own).map(|w| {
            if w.is_empty() {
                "Ahora se está instalando algo: espera a que termine.".to_string()
            } else {
                format!("Ahora se está instalando algo ({w}): espera a que termine.")
            }
        })
    }

    /// Apple's D3DMetal beside the DXMT engine: shown when it is installed, explained when it is
    /// not. Altrune has no button for it.
    pub(super) fn d3d12_row(&self, ui: &mut egui::Ui) {
        let ready = self.engines.d3d12;
        c::setting_row(
            ui,
            "Direct3D 12 (D3DMetal de Apple)",
            if ready {
                "Lo usan los juegos de Direct3D 12 abiertos con «Steam y launchers»; sus launchers siguen con DXMT."
            } else {
                D3D12_NOT_INSTALLED
            },
            |ui| {
                if ready {
                    c::badge(ui, "Listo", Tone::Success);
                } else {
                    c::badge(ui, "No instalado", Tone::Neutral);
                }
            },
        );
    }

    pub(super) fn video_row(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>, busy: Option<&str>) {
        let ready = self.engines.video;
        let wait = Self::wait_reason(busy, Some(Self::VIDEO_JOB));
        c::setting_row(
            ui,
            "Video para juegos (GStreamer)",
            if ready {
                "Los juegos pueden reproducir sus videos (MP4, H.264, WMV)."
            } else {
                "Sin esto, los juegos que reproducen videos (Resident Evil Requiem) se cierran."
            },
            |ui| {
                if ready {
                    c::badge(ui, "Listo", Tone::Success);
                } else if Button::secondary("Añadir video")
                    .icon(Icon::Film)
                    .disabled_reason(wait.as_deref())
                    .loading(spins_for(busy, Self::VIDEO_JOB))
                    .tooltip("Descarga GStreamer 1.28.7 oficial (~150 MB, LGPL) y deja junto al motor solo lo que Wine usa (~56 MB).")
                    .show(ui)
                    .clicked()
                {
                    acts.push(Action::AddVideo);
                }
            },
        );
    }

    pub(super) fn controllers_row(&self, ui: &mut egui::Ui) {
        let ready = self.engines.controllers;
        c::setting_row(
            ui,
            "Mandos por cable tipo Xbox",
            if ready {
                "Wine los ve en los programas abiertos con «Steam y launchers»."
            } else {
                "macOS no trae controlador para ellos; sin las bibliotecas de mandos, Wine no los ve."
            },
            |ui| {
                if ready {
                    c::badge(ui, "Listos", Tone::Success);
                } else {
                    c::badge(ui, "No instalados", Tone::Neutral);
                }
            },
        );
    }

    /// Whether the engine can be used at once (nothing to download first).
    pub(super) fn engine_ready(&self, engine: Engine) -> bool {
        match engine {
            Engine::Standard => true,
            Engine::Metal => self.engines.metal.is_some(),
            Engine::Games => self.engines.games.is_some(),
        }
    }

    /// Wine's state: a card on its own (`in_place` false) or rows inside another card.
    pub(crate) fn wine_card(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>, in_place: bool) {
        let p = pal(ui);
        let super::super::sidebar::WineSummary {
            tone,
            title,
            sub,
            frac,
            working,
        } = self.wine_summary();
        let (running, step, downloaded, total, error, flavour, rosetta_needed, cancelling) = {
            let g = self.setup.lock().unwrap_or_else(|e| e.into_inner());
            (
                g.running,
                g.step,
                g.downloaded,
                g.total,
                g.error.clone(),
                g.flavour.clone(),
                g.rosetta_needed,
                g.cancel,
            )
        };
        // What the app's own job line says it is doing: the buttons that start another job wait,
        // and the one that started this one shows its spinner.
        let app_job = self.app_job.as_deref();
        let other_job = app_job.map(|_| "Espera a que termine lo que se está haciendo.");
        let body = |ui: &mut egui::Ui, acts: &mut Vec<Action>| {
            ui.horizontal(|ui| {
                if working {
                    c::spinner(ui, 14.0);
                } else {
                    c::status_dot(ui, p.tone(tone));
                }
                ui.label(text(&title, size::TITLE, Weight::Semibold, p.text));
                if !running && !in_place {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.wine.is_none()
                            && !matches!(self.wine_state, WineState::Unknown)
                            && Button::primary("Instalar Wine gratis")
                                .icon(Icon::Download)
                                .show(ui)
                                .clicked()
                        {
                            acts.push(Action::InstallWine);
                        }
                    });
                }
            });
            if running {
                ui.add_space(space::S);
                // A bar only while the download says how far it is; the step rows turn otherwise.
                if frac.is_some() {
                    c::progress(ui, ui.scope_id().with("wine-install"), frac);
                    ui.add_space(space::S);
                }
                for s in Step::LIST {
                    // The answer comes from the install worker; nothing is started from here.
                    if s == Step::Rosetta
                        && step != Some(Step::Rosetta)
                        && rosetta_needed != Some(true)
                    {
                        continue;
                    }
                    let state = match step {
                        Some(cur) if cur == s => StepState::Now,
                        Some(cur) if cur > s => StepState::Done,
                        _ => StepState::Later,
                    };
                    let label = if s == Step::Download {
                        let sizes = (step == Some(Step::Download)).then_some((downloaded, total));
                        download_label(&flavour, sizes)
                    } else {
                        s.label().to_string()
                    };
                    c::step_row(ui, state, &label);
                }
                ui.add_space(space::S);
                ui.horizontal_wrapped(|ui| {
                    ui.label(text(
                        "Se instala dentro de esta app. Solo si hace falta, macOS pedirá tu contraseña para preparar el Mac (Rosetta).",
                        size::SMALL,
                        Weight::Regular,
                        p.text_muted,
                    ));
                    // Pressed once, it says so and cannot be pressed again: the download stops at
                    // its next chunk.
                    if Button::secondary(if cancelling {
                        "Cancelando…"
                    } else {
                        "Cancelar"
                    })
                    .loading(cancelling)
                    .disabled_reason(crate::domain::status::install_cancel_reason(step))
                    .show(ui)
                    .clicked()
                    {
                        acts.push(Action::CancelWineInstall);
                    }
                });
                return;
            }
            match &self.wine_state {
                WineState::Ready { path, .. } => {
                    ui.label(text(&sub, size::BODY, Weight::Regular, p.text_muted));
                    if in_place {
                        ui.label(text(
                            path.to_string_lossy(),
                            size::CAPTION,
                            Weight::Regular,
                            p.text_faint,
                        ));
                        ui.add_space(space::S);
                        ui.horizontal_wrapped(|ui| {
                            if Button::secondary("Usar otro Wine…")
                                .loading(app_job == Some(Self::PICK_JOB))
                                .disabled_reason(
                                    other_job.filter(|_| app_job != Some(Self::PICK_JOB)),
                                )
                                .show(ui)
                                .clicked()
                            {
                                acts.push(Action::PickWine);
                            }
                            // The games engine is never downloaded again: pressing would only
                            // explain that, so the button says it up front.
                            let reinstall_why = if Engine::of(path) == Engine::Games {
                                Some(APPLE_SOFTWARE_NOT_INSTALLED)
                            } else {
                                other_job.filter(|_| app_job != Some(Self::REINSTALL_JOB))
                            };
                            if Button::secondary("Reinstalar")
                                .icon(Icon::Refresh)
                                .loading(app_job == Some(Self::REINSTALL_JOB))
                                .disabled_reason(reinstall_why)
                                .show(ui)
                                .clicked()
                            {
                                acts.push(Action::ReinstallWine);
                            }
                            if Button::ghost("Buscar de nuevo").show(ui).clicked() {
                                acts.push(Action::Rescan);
                            }
                        });
                    }
                }
                WineState::Unknown => {
                    ui.horizontal(|ui| {
                        c::spinner(ui, 14.0);
                        ui.label(text(
                            "Revisando si ya tienes Wine en este Mac…",
                            size::BODY,
                            Weight::Regular,
                            p.text_muted,
                        ));
                    });
                }
                WineState::Broken { path, reason } => {
                    ui.label(text(
                        format!(
                            "Encontré Wine en {}, pero no se abre: {}.",
                            path.to_string_lossy(),
                            reason.trim_end_matches('.')
                        ),
                        size::BODY,
                        Weight::Regular,
                        p.text,
                    ));
                    ui.add_space(space::S);
                    ui.horizontal_wrapped(|ui| {
                        if Button::primary("Instalar uno que funcione")
                            .show(ui)
                            .clicked()
                        {
                            acts.push(Action::InstallWine);
                        }
                        if Button::secondary("Buscar de nuevo").show(ui).clicked() {
                            acts.push(Action::Rescan);
                        }
                    });
                }
                WineState::Missing => {
                    match &error {
                        Some(e) => ui.label(text(e, size::BODY, Weight::Regular, p.danger)),
                        None => ui.label(text(
                            "Wine es el motor libre y gratuito que abre los programas de Windows. Se descarga e instala solo (unos 225 MB).",
                            size::BODY,
                            Weight::Regular,
                            p.text,
                        )),
                    };
                    ui.add_space(space::S);
                    ui.horizontal_wrapped(|ui| {
                        if in_place
                            && Button::primary(if error.is_some() {
                                "Intentar otra vez"
                            } else {
                                "Instalar Wine gratis"
                            })
                            .icon(Icon::Download)
                            .show(ui)
                            .clicked()
                        {
                            acts.push(Action::InstallWine);
                        }
                        if Button::secondary("Ya lo tengo: buscar de nuevo")
                            .show(ui)
                            .clicked()
                        {
                            acts.push(Action::Rescan);
                        }
                        if Button::ghost("Elegirlo a mano…").show(ui).clicked() {
                            acts.push(Action::PickWine);
                        }
                    });
                }
            }
        };
        if in_place {
            body(ui, acts);
        } else {
            c::card(ui, |ui| {
                ui.add_space(space::M);
                body(ui, acts);
                ui.add_space(space::M);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_wait_text_has_no_empty_parentheses() {
        assert_eq!(App::wait_reason(None, None), None);
        assert_eq!(App::wait_reason(Some("video"), Some("video")), None);
        let t = App::wait_reason(Some(""), Some("video")).unwrap();
        assert!(!t.contains("()") && t.contains("algo:"));
        let t = App::wait_reason(Some("Steam y launchers"), Some("video")).unwrap();
        assert!(t.contains("(Steam y launchers)"));
    }

    /// The download step names what is downloaded: an addition by its own name, an engine as
    /// Wine, and the size only once it is known.
    #[test]
    fn the_download_step_names_what_it_downloads() {
        assert_eq!(download_label("video", None), "Descargar video");
        assert_eq!(download_label("", None), "Descargar Wine");
        assert_eq!(download_label("11.0", None), "Descargar Wine 11.0");
        // The engine of the list is named as the list names it, not as a Wine.
        assert_eq!(
            download_label(LAUNCHERS, None),
            "Descargar Steam y launchers"
        );
        assert_eq!(
            download_label("video", Some((5_000_000, 150_000_000))),
            "Descargar video (5 de 150 MB)"
        );
        // Size not known yet: no «0 de 0 MB».
        assert_eq!(download_label("video", Some((0, 0))), "Descargar video");
    }

    /// An install is running and named on the frame it is asked for. The buttons of the
    /// settings compare that name with their own job, so the one that was pressed spins at once
    /// and the others say they wait (the name used to stay empty until the download began).
    #[test]
    fn an_install_is_named_on_the_frame_it_is_asked_for() {
        let video = App::VIDEO_JOB;
        let progress = App::started_progress(video);
        assert!(progress.running);
        let running = running_job(&progress);
        assert_eq!(running.as_deref(), Some(video));
        assert!(
            spins_for(running.as_deref(), video),
            "video must spin at once"
        );
        assert!(!spins_for(running.as_deref(), LAUNCHERS));
        // The engine is named too, so the install of «Steam y launchers» spins at once.
        let metal = App::engine_job_name(Engine::Metal);
        assert_eq!(metal, LAUNCHERS);
        let running = running_job(&App::started_progress(metal));
        assert!(spins_for(running.as_deref(), metal));
        // The games engine is never installed from here: its install has no name.
        assert_eq!(App::engine_job_name(Engine::Games), "");
        // A reset install (nothing runs) names nothing.
        assert_eq!(running_job(&crate::wine_setup::Progress::default()), None);
    }
}
