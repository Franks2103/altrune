//! The appearance setting end to end: choosing it in Ajustes shows it at once, keeps it in
//! config.json, and the next start comes up with it (unless the command line forces one).

use super::driver::Flow;
use crate::backend::Appearance;
use crate::{Action, App, StartOptions};
use eframe::egui::{self, ThemePreference};
use std::path::Path;

fn preference(ctx: &egui::Context) -> ThemePreference {
    ctx.options(|o| o.theme_preference)
}

fn saved_file(home: &Path) -> String {
    std::fs::read_to_string(home.join("Library/Application Support/Botella Libre/config.json"))
        .expect("config.json")
}

/// The app started again in the same home, as a new run of the program would be.
fn second_start(forced: Option<egui::Theme>) -> egui::Context {
    let ctx = egui::Context::default();
    let start = StartOptions {
        theme: forced,
        ..StartOptions::default()
    };
    drop(App::with_context(ctx.clone(), None, None, start));
    ctx
}

flow_test!(the_appearance_is_chosen_at_once_saved_and_kept, |home| {
    let mut f = Flow::start(home);
    f.act(Action::OpenSettings);
    // The row measures its control in one frame and shows it in the next.
    f.frame();
    f.frame();
    // By default the window follows the Mac, and the three choices are named and reachable.
    assert_eq!(preference(&f.app.ctx), ThemePreference::System);
    for name in ["Apariencia", "Automática", "Clara", "Oscura"] {
        assert!(f.shows(name), "{name}\n{}", f.dump());
    }

    // They are tabs: the chosen one is selected and each says what it sets.
    assert_eq!(
        f.tab("Automática"),
        Some((true, Some("Apariencia igual que tu Mac".into())))
    );
    assert_eq!(
        f.tab("Clara"),
        Some((false, Some("Apariencia siempre clara".into())))
    );
    assert_eq!(f.tab("Oscura").map(|t| t.0), Some(false));

    // Pressed like a person would, then through the action.
    f.click("Oscura");
    f.frame();
    assert_eq!(f.tab("Oscura").map(|t| t.0), Some(true));
    assert_eq!(f.tab("Automática").map(|t| t.0), Some(false));
    assert_eq!(f.app.cfg.appearance, Appearance::Dark);
    assert_eq!(preference(&f.app.ctx), ThemePreference::Dark);
    assert!(saved_file(home).contains("\"appearance\": \"dark\""));

    f.act(Action::SetAppearance(Appearance::Light));
    f.frame();
    assert_eq!(preference(&f.app.ctx), ThemePreference::Light);
    assert!(saved_file(home).contains("\"appearance\": \"light\""));
    // The rest of the file is as the flow wrote it.
    assert!(saved_file(home).contains("\"games_offered\": true"));

    // A second start in the same home comes up with the saved choice...
    assert_eq!(preference(&second_start(None)), ThemePreference::Light);
    // ...unless the command line forces one.
    assert_eq!(
        preference(&second_start(Some(egui::Theme::Dark))),
        ThemePreference::Dark
    );

    // Back to the Mac's own.
    f.click("Automática");
    assert_eq!(preference(&f.app.ctx), ThemePreference::System);
    assert!(saved_file(home).contains("\"appearance\": \"system\""));
    assert_eq!(preference(&second_start(None)), ThemePreference::System);
    f.finish();
});

flow_test!(
    a_misspelled_appearance_in_the_file_starts_like_the_mac,
    |home| {
        let f = Flow::start(home);
        let file = home.join("Library/Application Support/Botella Libre/config.json");
        let mut json: serde_json::Value = serde_json::from_str(&saved_file(home)).unwrap();
        json["appearance"] = serde_json::json!("oscuroo");
        std::fs::write(&file, json.to_string()).unwrap();
        assert_eq!(preference(&second_start(None)), ThemePreference::System);
        // The file was not set aside as unreadable.
        let aside = std::fs::read_dir(file.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("ilegible"))
            .count();
        assert_eq!(aside, 0);
        f.finish();
    }
);
