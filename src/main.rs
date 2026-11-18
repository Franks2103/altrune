//! Altrune: run Windows programs on a Mac with free Wine, in a simple window.
//!
//! This file only names the layers and what the whole crate shares. The state and what the app
//! does are in `app`, what the window shows in `ui`, the command line and start-up in `startup`.

mod app;
mod backend;
mod diagnostics;
mod domain;
#[cfg(test)]
mod flows;
mod measure;
mod startup;
mod ui;
mod wine_setup;

// What the layers say to one another and the code that uses them without naming where it lives:
// the files of `app` and `ui` take these through `use crate::*`.
use app::{
    Action, App, BottleTab, Exit, ExitVerdict, HandedOver, Modal, Msg, NameCheck, NoticeKind,
    OnPanic, Opened, Running, Screen, StartOptions, WineChange, WORKER_FAILED_TEXT,
};
use backend::*;
use eframe::egui;
use std::path::{Path, PathBuf};

// The name the person knows the app by is defined once, in `backend::paths`, because the backend
// uses it too and cannot name this file.
pub(crate) use backend::APP_NAME;

fn main() -> eframe::Result {
    startup::run()
}
