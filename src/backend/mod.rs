//! Bottles, free Wine detection and the commands that run inside a bottle.
//!
//! This layer may use `domain` and `diagnostics` and nothing else of the crate: it never names
//! `wine_setup`, the app or the window (a test of the layers enforces it for the code that ships;
//! tests may reach upward, for example to the installer). What it needs to know
//! about engines (where they live on the disk, which layers sit beside them and what environment
//! each one needs) is read-only knowledge about the disk and lives here, in `engine_layout`;
//! `wine_setup` adds what downloads and installs them and re-exports this.

mod bottles;
mod disk;
mod engine_layout;
mod environment;
mod file_kind;
pub mod launchers;
mod logging;
mod paths;
mod processes;
mod programs;
pub mod recipes;
mod settings;
mod shell_link;
mod steam;
mod tools;

pub use bottles::*;
pub use disk::*;
pub use engine_layout::*;
pub use environment::*;
pub use file_kind::*;
pub use logging::*;
pub use paths::*;
pub use processes::*;
pub use programs::*;
pub use settings::*;
pub use steam::*;
pub use tools::*;

#[cfg(test)]
mod tests;
