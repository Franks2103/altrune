//! Installing a Windows program from a file the user picked or dropped. The draft of the install
//! dialog (what the file is and which bottle it is proposed for) is in `draft`; what the app
//! does once the person confirms, up to the end of the installation, is in `flow`.

mod draft;
mod flow;

pub(crate) use draft::InstallDraft;
