//! What the person can ask for: the actions the screens send, the dialogs, the pages, and the
//! dispatcher that hands each action to its handler.

use super::{App, BottleAsker};
use crate::backend::{self, launchers, open_in_finder, Tool};
use crate::wine_setup;
use std::path::PathBuf;

#[derive(Clone)]
pub(crate) enum Modal {
    NewBottle,
    /// Where an installer goes (the file and the choices are in `App::install_draft`).
    Install,
    /// Open a program (the bottle it was asked for, the file).
    Run(PathBuf, PathBuf),
    /// Each of these carries the bottle it was opened for, so a later change of the selected
    /// bottle never makes the confirmation act on another one.
    Rename(PathBuf),
    Delete(PathBuf),
    WineHelp,
    /// Changing Wine closes the programs of these bottles (folder, name) first.
    CloseForWine(WineChange, Vec<(PathBuf, String)>),
}

/// A change of Wine that has to close the bottles that run on the old one first.
#[derive(Clone)]
pub(crate) enum WineChange {
    /// The engine's folder is deleted and installed again.
    Reinstall(wine_setup::Engine),
    /// Another Wine becomes the usual one.
    Switch(wine_setup::WineState),
}

#[derive(Clone)]
pub(crate) enum Action {
    Select(usize),
    OpenNewBottle,
    CreateBottle,
    Install,
    /// «Instalar» in the install dialog.
    ConfirmInstall,
    /// «Cerrar todo en esta botella» on the installation card: closes every program of the
    /// installation's bottle, the installer among them.
    CloseInstaller,
    /// «Sí, ya terminó» on the installation card.
    FinishInstall,
    /// «Listo» on the installation card.
    DismissInstall,
    PickExe,
    RunPicked,
    OpenC,
    RunProgram(usize),
    /// Close Steam politely and open it again (its tile's menu).
    RestartSteam(usize),
    /// Go on with a change of Wine after its dialog named the bottles to close.
    ConfirmWineChange,
    CancelWineChange,
    HideProgram(usize),
    /// Put a program the person removed from the list back.
    UnhideProgram(usize),
    /// Read the programs of the bottles again.
    RefreshPrograms,
    /// Close the error banner of a bottle (or the one that belongs to no bottle on screen).
    DismissError(Option<PathBuf>),
    Tool(Tool),
    Quit(bool),
    SetWindows(String),
    SetMsync(bool),
    SetHighRes(bool),
    OpenRename,
    DoRename,
    Duplicate,
    Export,
    Import,
    AskDelete,
    DoDelete,
    ShowFinder,
    OpenWineHelp,
    InstallWine,
    CancelWineInstall,
    ReinstallWine,
    PickWine,
    Rescan,
    CloseModal,
    OpenSettings,
    Drop(PathBuf),
    SetDefault,
    RunWithOptions(usize),
    ShowLaunchers,
    SetLaunchers(bool),
    /// Light, dark or like the Mac, from Ajustes.
    SetAppearance(backend::Appearance),
    UseEngine(wine_setup::Engine),
    AddVideo,
    Diagnose,
    ExportReport,
    GoHome,
    SetTab(BottleTab),
    /// Open a program of another bottle (bottle index, program index), from the overview.
    RunFrom(usize, usize),
}

/// The page shown in the main area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    Home,
    Bottle,
    Settings,
}

/// The tabs of a bottle's page: frequent things first, technical controls apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BottleTab {
    Programs,
    Settings,
    Diagnostics,
}

impl App {
    pub(crate) fn apply(&mut self, a: Action) {
        match a {
            Action::Diagnose => {
                if let Some(path) = self.bottle().map(|b| b.path.clone()) {
                    self.run_diagnosis(path, std::time::Duration::ZERO);
                }
            }
            Action::ExportReport => self.export_report(),
            Action::Select(i) => {
                if i != self.selected {
                    self.search.clear();
                }
                self.selected = i;
                self.screen = Screen::Bottle;
            }
            Action::GoHome => self.screen = Screen::Home,
            Action::SetTab(tab) => self.tab = tab,
            Action::RunFrom(bottle, program) => {
                self.selected = bottle;
                self.apply(Action::RunProgram(program));
            }
            Action::OpenNewBottle => {
                // Every way in ends here (the sidebar, the header, ⌘N): without Wine the dialog
                // could not be finished, so it does not open and says why instead.
                if let Some(why) = self.wine_missing_reason() {
                    self.say(why);
                    return;
                }
                self.new_name = "Mi botella".into();
                self.new_windows = "win10".into();
                // The games-engine box starts unticked each time, not as the last dialog left it.
                self.new_games = false;
                self.modal = Some(Modal::NewBottle);
            }
            Action::CreateBottle => self.create_bottle(BottleAsker::Dialog),
            Action::Install => self.pick_installer(),
            Action::Drop(f) => self.dropped(f),
            Action::ConfirmInstall => self.confirm_install(),
            Action::CloseInstaller => self.close_installer(),
            Action::FinishInstall => self.finish_install(),
            Action::DismissInstall => self.dismiss_install(),
            Action::PickExe => self.pick_exe(),
            Action::RunWithOptions(i) => self.open_run_options(i),
            Action::SetDefault => self.set_default(),
            Action::ShowLaunchers => open_in_finder(&launchers::root()),
            Action::SetLaunchers(on) => self.set_launchers(on),
            Action::SetAppearance(a) => self.set_appearance(a),
            Action::RunPicked => self.run_picked(),
            Action::OpenC => {
                if let Some(b) = self.bottle() {
                    open_in_finder(&b.path.join("drive_c"));
                }
            }
            Action::RunProgram(i) => self.run_program(i),
            Action::RestartSteam(i) => self.restart_steam(i),
            Action::ConfirmWineChange => self.confirm_wine_change(),
            Action::CancelWineChange => self.cancel_wine_change(),
            Action::HideProgram(i) => self.hide_program(i),
            Action::UnhideProgram(i) => self.unhide_program(i),
            Action::RefreshPrograms => self.refresh_programs(),
            Action::DismissError(bottle) => self.dismiss_error(bottle.as_deref()),
            Action::Tool(t) => self.run_tool(t),
            Action::Quit(force) => self.quit_programs(force),
            Action::SetWindows(v) => self.set_windows(v),
            Action::SetMsync(on) => self.set_msync(on),
            Action::SetHighRes(on) => self.set_high_res(on),
            Action::OpenRename => {
                if let Some((path, name)) = self.bottle().map(|b| (b.path.clone(), b.name.clone()))
                {
                    self.rename_text = name;
                    self.modal = Some(Modal::Rename(path));
                }
            }
            Action::DoRename => self.rename_bottle(),
            Action::Duplicate => self.duplicate_bottle(),
            Action::Export => self.export_bottle(),
            Action::Import => self.import_bottle(),
            Action::AskDelete => {
                if let Some(b) = self.bottle() {
                    self.modal = Some(Modal::Delete(b.path.clone()));
                }
            }
            Action::DoDelete => self.delete_bottle(),
            Action::ShowFinder => {
                if let Some(b) = self.bottle() {
                    open_in_finder(&b.path);
                }
            }
            Action::OpenWineHelp => self.modal = Some(Modal::WineHelp),
            Action::InstallWine => {
                self.auto_install = true;
                self.start_wine_install();
            }
            Action::CancelWineInstall => {
                wine_setup::lock_progress(&self.setup).cancel = true;
                self.auto_install = false;
            }
            Action::ReinstallWine => self.reinstall_wine(),
            Action::UseEngine(engine) => self.use_engine(engine),
            Action::AddVideo => self.add_video(),
            Action::PickWine => self.pick_wine(),
            Action::Rescan => self.look_again(),
            // Esc or a click outside the dialog of a Wine change is the same as «Cancelar».
            Action::CloseModal if matches!(self.modal, Some(Modal::CloseForWine(..))) => {
                self.cancel_wine_change()
            }
            Action::CloseModal => self.modal = None,
            Action::OpenSettings => self.screen = Screen::Settings,
        }
    }
}
