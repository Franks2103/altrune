//! Apple's D3DMetal and Game Porting Toolkit are never downloaded by the app. Where they are
//! missing the screens explain why; where they are installed they are used as before.

use super::driver::Flow;
use super::fixtures;
use crate::wine_setup::APPLE_SOFTWARE_NOT_INSTALLED;
use crate::{Action, BottleTab};
use eframe::egui::accesskit::Role;
use std::path::PathBuf;
use std::time::Duration;

const LONG: Duration = Duration::from_secs(20);

/// Makes the window believe this Mac runs the Metal engines, with the DXMT engine installed and
/// the given state of Apple's two pieces (only the snapshot the screens read; nothing on disk).
fn engines_seen(f: &mut Flow, games: bool, d3d12: bool) {
    let e = &mut f.app.engines;
    e.metal_capable = true;
    e.metal = Some(PathBuf::from("/no/hay/motor/engine/bin/wine"));
    e.games = games.then(|| PathBuf::from("/no/hay/juegos/wine"));
    e.d3d12 = d3d12;
}

fn count_part(f: &Flow, part: &str) -> usize {
    f.names().iter().filter(|n| n.contains(part)).count()
}

flow_test!(
    the_settings_rows_of_apple_software_have_no_button_when_it_is_missing,
    |home| {
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::OpenSettings);
        engines_seen(&mut f, false, false);
        f.frame();
        f.frame();
        // Both rows say why, and neither has a button to download anything.
        assert!(f.shows("Juegos DirectX 12 (D3DMetal)"), "{}", f.tree());
        assert!(f.shows("Direct3D 12 (D3DMetal de Apple)"), "{}", f.tree());
        assert!(f.shows(APPLE_SOFTWARE_NOT_INSTALLED), "{}", f.tree());
        assert!(
            f.shows_part("software de Apple y su licencia"),
            "{}",
            f.tree()
        );
        assert_eq!(
            count_part(&f, "No se instala desde Altrune"),
            2,
            "{}",
            f.tree()
        );
        // (One of the words is written in pieces: the scan of the sources keeps that button's
        // label out of every file, this one included.)
        let add_d3d12 = ["Añadir Direct3D", " 12"].concat();
        for gone in [
            "Instalar (~240 MB)",
            add_d3d12.as_str(),
            "gratuito",
            "~240 MB",
        ] {
            assert!(
                !f.shows_part(gone),
                "«{gone}» is still offered\n{}",
                f.tree()
            );
        }
        // The only buttons that install anything on this page are the Wine one and the video
        // one (which has its own source): none of them is for Apple's software.
        assert!(!f.app.installing(), "{}", f.app.status);
        assert!(f.app.pending_engine_bottle.is_none());
        f.finish();
    }
);

flow_test!(
    the_settings_rows_of_apple_software_show_it_when_it_is_installed,
    |home| {
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::OpenSettings);
        engines_seen(&mut f, true, true);
        f.frame();
        f.frame();
        assert!(f.shows("Juegos DirectX 12 (D3DMetal)"), "{}", f.tree());
        assert!(f.shows("Direct3D 12 (D3DMetal de Apple)"), "{}", f.tree());
        assert!(f.shows("Instalado"), "{}", f.tree());
        assert!(f.shows("Listo"), "{}", f.tree());
        assert_eq!(
            count_part(&f, "No se instala desde Altrune"),
            0,
            "{}",
            f.tree()
        );
        f.finish();
    }
);

flow_test!(
    the_graphics_list_offers_the_games_engine_only_when_it_is_installed,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Select(0));
        f.act(Action::SetTab(BottleTab::Settings));
        engines_seen(&mut f, false, false);
        f.frame();
        f.frame();
        f.frame();
        f.click_role(Role::ComboBox, "Gráficos");
        f.frame();
        // Missing: listed but switched off, with the reason as its description.
        let (disabled, why) = f
            .node_state("Juegos DirectX 12")
            .unwrap_or_else(|| panic!("{}", f.tree()));
        assert!(disabled, "{}", f.tree());
        assert_eq!(why.as_deref(), Some(APPLE_SOFTWARE_NOT_INSTALLED));
        // The other choices are not blocked by it.
        let (dxmt_disabled, _) = f.node_state("Steam y launchers  ·").unwrap();
        assert!(!dxmt_disabled, "{}", f.tree());
        // Pressing it starts nothing and moves no bottle.
        f.click("Juegos DirectX 12  ·  D3DMetal");
        assert!(!f.app.installing(), "{}", f.app.status);
        assert!(f.app.pending_engine_bottle.is_none());
        assert!(f
            .app
            .cfg
            .bottle_ref(&f.app.bottles[0].path)
            .is_none_or(|s| s.engine.is_none()));
        f.finish();
    }
);

flow_test!(
    an_installed_games_engine_is_offered_in_the_graphics_list,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Select(0));
        f.act(Action::SetTab(BottleTab::Settings));
        engines_seen(&mut f, true, true);
        for _ in 0..3 {
            f.frame();
        }
        f.click_role(Role::ComboBox, "Gráficos");
        f.frame();
        let (disabled, why) = f
            .node_state("Juegos DirectX 12")
            .unwrap_or_else(|| panic!("{}", f.tree()));
        assert!(!disabled, "{}", f.tree());
        assert_ne!(why.as_deref(), Some(APPLE_SOFTWARE_NOT_INSTALLED));
        f.finish();
    }
);

flow_test!(the_games_engine_is_never_started_as_a_download, |home| {
    fixtures::bottle_with("Mi botella", &[]);
    let mut f = Flow::start(home);
    f.wait_until("Wine", LONG, |a| a.wine.is_some());
    f.act(Action::Select(0));
    engines_seen(&mut f, false, false);
    // Choosing it for a bottle, installing it, reinstalling it: each only says why.
    f.act(Action::UseEngine(crate::wine_setup::Engine::Games));
    let said = f.app.status.clone();
    assert!(
        said.contains("«Juegos DirectX 12» no se instala desde Altrune"),
        "{said}"
    );
    assert!(!f.app.installing());
    assert!(f.app.pending_engine_bottle.is_none());
    f.app.start_engine_install(crate::wine_setup::Engine::Games);
    assert!(
        f.app.status.contains("software de Apple"),
        "{}",
        f.app.status
    );
    assert!(!f.app.installing());
    assert_eq!(f.app.installing_job(), "");
    f.finish();
});

flow_test!(
    the_reinstall_button_of_an_installed_games_wine_says_why_it_is_off,
    |home| {
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::OpenSettings);
        // The Wine in use is the games engine: reinstalling it would download it again.
        let games = PathBuf::from("/no/hay/Game Porting Toolkit/bin/wine");
        f.app.wine = Some(games.clone());
        f.app.wine_state = crate::wine_setup::WineState::Ready {
            path: games,
            version: "wine-7.7".into(),
        };
        f.frame();
        f.frame();
        let (disabled, why) = f
            .node_state("Reinstalar")
            .unwrap_or_else(|| panic!("{}", f.tree()));
        assert!(disabled, "{}", f.tree());
        assert_eq!(why.as_deref(), Some(APPLE_SOFTWARE_NOT_INSTALLED));
        f.finish();
    }
);

flow_test!(
    a_mac_that_cannot_run_the_metal_engines_is_told_so_before_the_licence,
    |home| {
        fixtures::bottle_with("Mi botella", &[]);
        let mut f = Flow::start(home);
        f.wait_until("Wine", LONG, |a| a.wine.is_some());
        f.act(Action::Select(0));
        f.act(Action::SetTab(BottleTab::Settings));
        engines_seen(&mut f, false, false);
        f.app.engines.metal_capable = false;
        for _ in 0..3 {
            f.frame();
        }
        f.click_role(Role::ComboBox, "Gráficos");
        f.frame();
        let (disabled, why) = f
            .node_state("Juegos DirectX 12")
            .unwrap_or_else(|| panic!("{}", f.tree()));
        assert!(disabled, "{}", f.tree());
        assert_eq!(
            why.as_deref(),
            Some("Necesita un Mac con chip de Apple y macOS 14 o posterior.")
        );
        f.finish();
    }
);
