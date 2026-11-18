//! Finding a free Wine on this Mac, checking that it really runs, and installing one by itself
//! when there is none: a ready-made Wine build is downloaded into the app's own folder, so no
//! administrator password, Homebrew or Terminal is needed (only Rosetta on Apple chips, which
//! macOS installs with its usual password window).
//!
//! This layer may use `backend`, `domain` and `diagnostics` and never the app or the window (a
//! test of the layers enforces it). Where the engines and their layers live on the disk, and the
//! environment each one needs, is read-only knowledge that the backend uses to build Wine
//! commands, so it lives in `backend::engine_layout` and is re-exported here under the same
//! names (`wine_setup::Engine`…). What stays here is what downloads, unpacks and installs
//! those folders, and `EngineStatus::read`, which looks at them for the window.

mod d3d12;
mod detect;
mod download;
mod engines;
mod install;
mod layers;
mod progress;
mod swap;
mod video;

pub use d3d12::*;
pub use detect::*;
pub use download::*;
pub use engines::*;
pub use install::*;
use layers::METAL_LAYER;
pub use progress::*;
pub use swap::*;
pub use video::*;

// Where the engines and their layers live, which the backend owns (see its `engine_layout`).
pub use crate::backend::{
    controllers_ready, d3d12_layer, engine_loader, managed_dir, video_layer, wine_in_app, Engine,
    REAL_LOADER,
};

#[cfg(test)]
mod tests {
    mod children;
    mod d3d12;
    mod layers;
    mod loader;
    mod pinned;
    // The helpers the tests inside the modules (downloads, swaps) share with these.
    pub(super) mod support;
    mod video;
}
