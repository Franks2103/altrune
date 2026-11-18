//! A bottle's graphics choice (the list and what it says about each engine) and what the DXMT
//! engine has added, with the texts of why a choice has to wait.

use crate::backend::Bottle;
use crate::ui::components as c;
use crate::ui::theme::{pal, size, space, text, Weight};
use crate::wine_setup::{Engine, APPLE_SOFTWARE_NOT_INSTALLED};
use crate::{Action, App};
use eframe::egui;

/// The graphics choices a bottle offers. Each is named by what it is for, with the engine's name
/// as small print, and has its own stored value in the bottle's `engine` setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GraphicsChoice {
    /// No engine saved: the plain Wine, and Steam moves the bottle to DXMT by itself.
    Auto,
    /// The DXMT engine (DirectX 10/11 drawn with Metal).
    Dxmt,
    /// The games engine (Apple's D3DMetal, DirectX 11/12): offered only when it is installed.
    Games,
}

impl GraphicsChoice {
    /// In the order they are listed.
    const ALL: [Self; 3] = [Self::Auto, Self::Dxmt, Self::Games];

    /// What the bottle's `engine` setting holds for this choice.
    fn stored(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Dxmt => Some("dxmt"),
            Self::Games => Some("games"),
        }
    }

    /// The choice a stored value stands for (a value nobody offers any more reads as "Automático", as
    /// the bottle's label does).
    fn from_stored(stored: Option<&str>) -> Option<Self> {
        match stored {
            Some("dxmt") => Some(Self::Dxmt),
            Some("games") => Some(Self::Games),
            Some("standard") => None,
            _ => Some(Self::Auto),
        }
    }

    /// The choice that asks for this engine.
    fn from_engine(engine: Engine) -> Self {
        match engine {
            Engine::Standard => Self::Auto,
            Engine::Metal => Self::Dxmt,
            Engine::Games => Self::Games,
        }
    }

    /// The engine the app is asked to move the bottle to (the plain Wine clears the setting).
    fn engine(self) -> Engine {
        match self {
            Self::Auto => Engine::Standard,
            Self::Dxmt => Engine::Metal,
            Self::Games => Engine::Games,
        }
    }

    /// What it is called, in the list and everywhere else (one table: `domain::engines`).
    fn name(self) -> &'static str {
        crate::domain::engines::engine_label(self.stored())
    }

    /// The engine's own name, as small print (the plain Wine needs none). This is a technical
    /// screen: the name may follow the label here and nowhere else.
    fn engine_name(self) -> Option<&'static str> {
        crate::domain::engines::engine_technical(self.stored())
    }

    /// One sentence under the setting while this choice is on.
    fn help(self) -> &'static str {
        match self {
            Self::Auto => "La app usa el Wine normal y, al abrir Steam, pasa la botella sola a «Steam y launchers», que sí sabe dibujarlo.",
            Self::Dxmt => "Dibuja con Metal los programas de DirectX 10 y 11. Lo necesitan Steam y launchers como el de Rockstar.",
            Self::Games => "Dibuja con Metal los juegos de DirectX 11 y 12, con D3DMetal de Apple.",
        }
    }
}

/// What the closed graphics list shows. While the engine of a choice is being installed for the
/// bottle (or the bottle is moving to it) it says so instead of keeping the old choice, so the
/// press has an answer on screen for the minutes a download takes.
fn graphics_selected_text(
    stored: Option<&str>,
    current: Option<GraphicsChoice>,
    working_on: Option<GraphicsChoice>,
) -> String {
    if let Some(choice) = working_on {
        return format!("Preparando {}…", choice.name());
    }
    match (stored, current) {
        (Some("standard"), _) => crate::domain::engines::engine_label(stored).to_string(),
        (_, Some(c)) => c.name().to_string(),
        _ => GraphicsChoice::Auto.name().to_string(),
    }
}

/// A choice's line in the list: what it is for, then its engine in smaller, quieter type.
fn choice_text(ui: &egui::Ui, choice: GraphicsChoice) -> egui::text::LayoutJob {
    let p = pal(ui);
    let mut job = egui::text::LayoutJob::default();
    // The first part takes the colour of the row (so it reads on hover and when disabled).
    job.append(
        choice.name(),
        0.0,
        egui::TextFormat {
            font_id: crate::ui::theme::font(size::BODY, Weight::Medium),
            color: egui::Color32::PLACEHOLDER,
            ..Default::default()
        },
    );
    // The engine's own name, or, for the automatic choice, the advice to leave it alone.
    let small = choice
        .engine_name()
        .or((choice == GraphicsChoice::Auto).then_some("recomendado"));
    if let Some(small) = small {
        job.append(
            &format!("  ·  {small}"),
            0.0,
            egui::TextFormat {
                font_id: crate::ui::theme::font(size::CAPTION, Weight::Regular),
                color: p.text_faint,
                ..Default::default()
            },
        );
    }
    job
}

impl App {
    /// A bottle's graphics choice, in words about what it is for.
    pub(crate) fn graphics_row(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>, b: &Bottle) {
        let p = pal(ui);
        let stored = self
            .cfg
            .bottle_ref(&b.path)
            .and_then(|s| s.engine.as_deref());
        let current = GraphicsChoice::from_stored(stored);
        let capable = self.engines.metal_capable;
        let in_use = self.bottle_in_use(&b.path);
        let downgrade = self
            .bottle_last_wine(b)
            .is_some_and(|last| crate::is_downgrade(&last, "wine-7.7"));
        let help = match (stored, current) {
            // Older versions of the app could save the plain Wine as a choice of its own.
            (Some("standard"), _) => "Esta botella usa el Wine normal. «Automático» hace lo mismo y además deja que Steam pase solo a «Steam y launchers».",
            (_, Some(c)) => c.help(),
            _ => GraphicsChoice::Auto.help(),
        };
        // The job that holds the bottle, and the engine that is being installed or moved to.
        let held = self.busy_reason(&b.path);
        let installing = self.installing();
        let working_on = self
            .pending_engine_bottle
            .as_ref()
            .filter(|(path, _)| *path == b.path)
            .map(|(_, engine)| GraphicsChoice::from_engine(*engine));
        c::setting_row(ui, "Gráficos", help, |ui| {
            let selected = graphics_selected_text(stored, current, working_on);
            egui::ComboBox::from_id_salt("gfx")
                .selected_text(selected)
                .width(170.0)
                .show_ui(ui, |ui| {
                    for choice in GraphicsChoice::ALL {
                        // Why a choice is not offered here, when it is not.
                        let why = match choice {
                            // Apple's engine is never downloaded by Altrune: it is offered only
                            // when it is already installed, and the reason does not change
                            // with what else runs. (A Mac that cannot run it at all says so
                            // further down: that reason matters more than its licence.)
                            GraphicsChoice::Games
                                if current != Some(choice)
                                    && self.engines.games.is_none()
                                    && capable =>
                            {
                                Some(APPLE_SOFTWARE_NOT_INSTALLED)
                            }
                            // A job holds the bottle, or the engine is on its way already.
                            _ if current != Some(choice)
                                && (held.is_some() || working_on.is_some()) =>
                            {
                                held.as_deref().or(Some(ENGINE_ON_ITS_WAY))
                            }
                            // Another install runs: this one would not start (it is not queued).
                            _ if current != Some(choice)
                                && installing
                                && !self.engine_ready(choice.engine()) =>
                            {
                                Some(INSTALL_RUNNING)
                            }
                            // Changing the engine closes whatever is open in the bottle.
                            _ if in_use && current != Some(choice) => {
                                Some(ENGINE_NEEDS_CLOSED)
                            }
                            GraphicsChoice::Auto => None,
                            _ if !capable => {
                                Some("Necesita un Mac con chip de Apple y macOS 14 o posterior.")
                            }
                            GraphicsChoice::Games if downgrade => {
                                Some("Esta botella ya se usó con Wine 11 y «Juegos DirectX 12» usa Wine 7.7: podría dañarla. Úsalo en una botella nueva.")
                            }
                            // The DXMT engine is Wine 11.0: only a bottle already updated by a
                            // newer Wine would go backwards.
                            GraphicsChoice::Dxmt
                                if self
                                    .bottle_last_wine(b)
                                    .is_some_and(|last| crate::is_downgrade(&last, "wine-11.0")) =>
                            {
                                Some("Esta botella ya se usó con un Wine más nuevo que el de «Steam y launchers» (Wine 11.0): podría dañarla.")
                            }
                            _ => None,
                        };
                        let r = ui.add_enabled(
                            why.is_none(),
                            egui::Button::selectable(
                                current == Some(choice),
                                choice_text(ui, choice),
                            ),
                        );
                        let r = match why {
                            Some(why) => {
                                // The reason is also the accessible description.
                                c::a11y::describe(&r, why);
                                r.on_disabled_hover_text(why)
                            }
                            None => r.on_hover_text(self.choice_tip(choice)),
                        };
                        if r.clicked() && stored != choice.stored() {
                            acts.push(Action::UseEngine(choice.engine()));
                        }
                    }
                })
                .response
                .accessible_name("Gráficos");
            // The row is laid out left to right and pushed to the edge, so the spinner that
            // sat at the right end now comes last.
            if working_on.is_some() {
                c::spinner(ui, 14.0);
            }
        });
        if downgrade && capable {
            ui.label(text(
                "«Juegos DirectX 12» no se ofrece en esta botella: ya la actualizó un Wine más nuevo.",
                size::SMALL,
                Weight::Regular,
                p.text_faint,
            ));
            ui.add_space(space::S);
        }
    }

    /// One sentence on what a graphics choice does, and what choosing it will cost the first time.
    fn choice_tip(&self, choice: GraphicsChoice) -> &'static str {
        match choice {
            GraphicsChoice::Auto => {
                "Lo más seguro: usa el Wine normal de esta botella y cambia solo de motor cuando abres Steam."
            }
            GraphicsChoice::Dxmt if self.engines.metal.is_some() => {
                "Para programas que el Wine normal no sabe dibujar, como Steam o el launcher de Rockstar."
            }
            GraphicsChoice::Dxmt => {
                "Descarga «Steam y launchers» (DXMT, ~340 MB, software libre) y lo usa en esta botella."
            }
            GraphicsChoice::Games if self.engines.games.is_some() => {
                "Para juegos modernos de DirectX 11 y 12."
            }
            GraphicsChoice::Games => APPLE_SOFTWARE_NOT_INSTALLED,
        }
    }

    /// What the DXMT engine has added (shown under "Avanzado" while a bottle uses that engine).
    pub(crate) fn engine_extras(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>, b: &Bottle) {
        let stored = self
            .cfg
            .bottle_ref(&b.path)
            .and_then(|s| s.engine.as_deref());
        if GraphicsChoice::from_stored(stored) != Some(GraphicsChoice::Dxmt)
            || self.engines.metal.is_none()
        {
            return;
        }
        let busy = self.installing_what();
        let title = crate::domain::engines::engine_label_technical(Some("dxmt")).to_uppercase();
        c::settings_group(ui, &title, |ui| {
            self.d3d12_row(ui);
            c::divider(ui);
            self.video_row(ui, acts, busy.as_deref());
            if self.engines.controllers {
                c::divider(ui);
                self.controllers_row(ui);
            }
        });
    }
}

/// Why a graphics choice waits while the engine of an earlier choice is being installed.
const ENGINE_ON_ITS_WAY: &str = "Espera a que termine de instalarse el motor que elegiste.";

/// Why an engine that is not installed yet cannot be chosen while another install runs.
const INSTALL_RUNNING: &str = "Ahora se está instalando algo: espera a que termine.";

/// Why the graphics list is locked while programs are open in the bottle.
const ENGINE_NEEDS_CLOSED: &str =
    "Cierra primero los programas de esta botella: cambiar los gráficos los cerraría.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::engines::{engine_label, engine_technical};
    use std::collections::HashSet;

    /// Every entry of the graphics list has its own stored value, its own name, and is read back
    /// as itself: choosing it and reopening the bottle shows the same entry.
    #[test]
    fn every_graphics_entry_maps_to_its_own_value_and_label() {
        let stored: HashSet<_> = GraphicsChoice::ALL.iter().map(|c| c.stored()).collect();
        let names: HashSet<_> = GraphicsChoice::ALL.iter().map(|c| c.name()).collect();
        let engines: Vec<Engine> = GraphicsChoice::ALL.iter().map(|c| c.engine()).collect();
        assert_eq!(stored.len(), GraphicsChoice::ALL.len());
        assert_eq!(names.len(), GraphicsChoice::ALL.len());
        assert!(engines
            .iter()
            .enumerate()
            .all(|(i, e)| !engines[..i].contains(e)));
        for c in GraphicsChoice::ALL {
            assert_eq!(GraphicsChoice::from_stored(c.stored()), Some(c), "{c:?}");
            // The list names the choice with the same words as the bottle's badge and the rest.
            assert_eq!(c.name(), engine_label(c.stored()));
            assert_eq!(c.engine_name(), engine_technical(c.stored()));
            assert!(!c.help().is_empty() && !c.name().is_empty());
        }
    }

    /// The old "WineD3D" entry saved the same thing as "Auto" (it cleared the setting), so it is
    /// not listed twice; a value an older version saved is still understood.
    #[test]
    fn plain_wine_is_not_a_second_entry() {
        assert_eq!(GraphicsChoice::Auto.stored(), None);
        assert_eq!(GraphicsChoice::from_stored(Some("standard")), None);
        assert_eq!(
            GraphicsChoice::from_stored(Some("otro")),
            Some(GraphicsChoice::Auto)
        );
        assert_eq!(GraphicsChoice::Auto.engine(), Engine::Standard);
    }

    /// While a choice's engine is on its way, the closed list says so; otherwise it shows the
    /// choice the bottle has.
    #[test]
    fn the_graphics_list_says_when_its_engine_is_on_its_way() {
        let dxmt = Some(GraphicsChoice::Dxmt);
        assert_eq!(
            graphics_selected_text(None, Some(GraphicsChoice::Auto), dxmt),
            "Preparando Steam y launchers…"
        );
        assert_eq!(
            graphics_selected_text(None, Some(GraphicsChoice::Auto), None),
            "Automático"
        );
        assert_eq!(
            graphics_selected_text(Some("standard"), None, None),
            "Wine normal"
        );
        assert_eq!(
            graphics_selected_text(Some("games"), Some(GraphicsChoice::Games), None),
            "Juegos DirectX 12"
        );
    }

    /// An engine maps to the list entry that asks for it, and back.
    #[test]
    fn every_engine_has_its_graphics_entry() {
        for choice in GraphicsChoice::ALL {
            assert_eq!(GraphicsChoice::from_engine(choice.engine()), choice);
        }
    }
}
