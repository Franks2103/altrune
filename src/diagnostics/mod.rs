//! Deterministic diagnostics: read what Wine and the program printed (and, for known programs,
//! their own log files), recognise known symptoms, and explain them in plain Spanish with what to
//! try. No guessing services: every rule below is a literal text match, listed with why it exists.
//!
//! A finding says how sure the app is (`Confidence`), where the evidence came from (`Source`),
//! how many lines matched and shows up to three of them. What was seen and what is guessed are
//! kept apart: the lines are facts, the explanation is the probable cause, and `checked` holds
//! the few things the app confirmed by itself (for example which engine the bottle runs).
//!
//! This is a leaf layer: it imports nothing else of the crate (a test of the layers enforces it for the code that ships;
//! tests may reach upward).
//! What it needs from the Mac, such as the name of the user to hide, is passed in by the caller.

mod analysis;
mod findings;
mod redact;
mod report;
mod rules;
mod sources;
mod state;

pub use analysis::*;
pub use findings::*;
pub use redact::*;
pub use report::*;
pub use rules::*;
pub use sources::*;
pub use state::*;

#[cfg(test)]
mod redact_tests;
#[cfg(test)]
mod tests;
