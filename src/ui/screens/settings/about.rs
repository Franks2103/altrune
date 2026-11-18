//! «Acerca de Altrune», the section that closes Ajustes: the logo, the version, what the app does,
//! where the old name's files stay and the licences of the parts it brings.

use super::shown_folder;
use crate::ui::brand::{self, Art};
use crate::ui::components as c;
use crate::ui::theme::{pal, size, space, text, Weight};
use crate::App;
use eframe::egui;

/// The title of the section that closes Ajustes (shown like the other groups' titles).
pub(crate) const ABOUT_TITLE: &str = "ACERCA DE ALTRUNE";

/// The accessible name of the logo, which a picture cannot say by itself.
pub(crate) const ABOUT_LOGO_NAME: &str = "Logotipo de Altrune";

/// How wide the logo is drawn in «Acerca de», in points.
const ABOUT_LOGO_WIDTH: f32 = 168.0;

/// One plain line about what the app does.
pub(crate) const ABOUT_WHAT: &str =
    "Abre programas y juegos de Windows en tu Mac con Wine, que es software libre.";

/// The licences of what the app brings, as already said in the engines' rows.
pub(crate) const ABOUT_LICENCES: &str =
    "Altrune es software libre con licencia Apache 2.0. Wine y GStreamer son software libre (LGPL). Altrune no incluye ni descarga software de Apple como D3DMetal.";

impl App {
    /// «Acerca de Altrune»: the full logo (the only place it is drawn), the version, what the
    /// app does, the folder that keeps its files and the licences of the parts it brings.
    pub(super) fn about_card(&self, ui: &mut egui::Ui) {
        let p = pal(ui);
        ui.add_space(space::L);
        // The logo is two layers of one canvas drawn in the same rectangle: the symbol in its own
        // colours and the wordmark as a mask tinted with the text colour, so it reads in both
        // appearances. Both were cut down once to the pixels they cover.
        let width = ABOUT_LOGO_WIDTH;
        let size = brand::size_pt(ui.ctx(), Art::LogoSymbol, width);
        let (slot, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
        if ui.is_rect_visible(slot) {
            let rect = brand::place(ui.ctx(), Art::LogoSymbol, slot.min, width);
            brand::paint(ui.painter(), Art::LogoSymbol, rect, egui::Color32::WHITE);
            brand::paint(ui.painter(), Art::LogoWord, rect, p.text);
        }
        c::a11y::node_info(&resp, egui::Role::Image, ABOUT_LOGO_NAME, true, None, None);
        ui.add_space(space::M);
        ui.label(text(
            format!("Versión {}", env!("CARGO_PKG_VERSION")),
            size::BODY,
            Weight::Medium,
            p.text,
        ));
        ui.add_space(space::XS);
        for (line, color) in [
            (ABOUT_WHAT.to_string(), p.text_muted),
            (shown_folder(&crate::backend::data_dir()), p.text_faint),
            (ABOUT_LICENCES.to_string(), p.text_faint),
        ] {
            ui.add_space(space::XS);
            ui.label(text(line, size::SMALL, Weight::Regular, color));
        }
        ui.add_space(space::L);
    }
}
