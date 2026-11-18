//! The app itself: its state (`state`), the messages workers send back (`msg`), the actions the
//! screens send (`action`), the background jobs (`worker`) and each frame (`frame`), and what
//! the app does when the user acts or a worker reports back: `impl App` blocks, one file per
//! kind of work.

mod action;
mod bottles;
mod diagnose;
mod engines;
mod frame;
mod install;
mod instance;
mod library;
mod messages;
mod msg;
mod notices;
mod programs;
mod settings;
mod state;
mod worker;

/// Asks the person with a native file dialog. Under test nobody answers: the dialog is never
/// built, so a flow that presses a button that asks for a file cannot open a window or wait
/// for a click that will not come.
pub(crate) fn native_dialog<T>(ask: impl FnOnce() -> Option<T>) -> Option<T> {
    if cfg!(test) {
        None
    } else {
        ask()
    }
}

pub(crate) use action::{Action, BottleTab, Modal, Screen, WineChange};
#[cfg(test)]
pub(crate) use bottles::export_job;
pub(crate) use bottles::BottleAsker;
#[cfg(test)]
pub(crate) use diagnose::{app_line, REPORT_FILE_NAME};
pub(crate) use engines::bottle_engine_path;
pub(crate) use install::InstallDraft;
pub(crate) use instance::{
    ask_running_app, claim_settled, hand_over, open_without_window, requests_dir, takes_lock, Claim,
};
pub(crate) use library::{decode_icon, OnePass};
pub(crate) use msg::Msg;
pub(crate) use notices::{status_visibility, Notice, NoticeKind, STATUS_SECS};
pub(crate) use state::{
    App, Exit, ExitVerdict, HandedOver, NameCheck, Opened, Running, StartOptions,
};
pub(crate) use worker::{OnPanic, WORKER_FAILED_TEXT};

#[cfg(test)]
mod tests {
    use super::native_dialog;

    /// Under test the dialog is never built, so no window can open.
    #[test]
    fn a_native_dialog_is_never_built_under_test() {
        let answer: Option<u8> = native_dialog(|| panic!("a native dialog was opened"));
        assert_eq!(answer, None);
    }
}
