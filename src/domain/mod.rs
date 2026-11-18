//! What the app knows about its world, independent of how it is drawn: here, what the
//! installed engines can do, how an installation is classified, placed and followed (its steps
//! included), and the one state of a bottle.
//!
//! This is a leaf layer: it imports nothing else of the crate, so it holds only decisions that
//! can be made from the facts it is given (a test of the layers enforces it for the code that
//! ships; tests may reach upward). Reading the disk or
//! picking a colour is done by the layers above, which hand it the facts and translate its answer.

pub mod engines;
pub mod install;
pub mod status;
pub mod step;
