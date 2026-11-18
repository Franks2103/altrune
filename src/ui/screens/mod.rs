//! The pages and dialogs, as `impl App` blocks: each reads the app's state and pushes
//! `Action`s, which `App::apply` carries out after the frame.

mod bottle;
mod dialogs;
mod home;
mod install;
mod settings;
pub mod shell;
mod sidebar;

/// The About section's texts, for the flow tests that check what the page says.
#[cfg(test)]
pub(crate) mod about {
    pub(crate) use super::settings::about::{
        ABOUT_LICENCES, ABOUT_LOGO_NAME, ABOUT_TITLE, ABOUT_WHAT,
    };
}
