//! The steps of installing a free Wine, in the order the person sees them, with the words for
//! each. Nothing here installs anything: the install itself lives in `wine_setup`, which
//! reports the step it is on, and the window and the cancel rule read these words.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Rosetta,
    Find,
    Download,
    Unpack,
    Check,
    Done,
}

impl Step {
    pub const LIST: [Step; 5] = [
        Step::Rosetta,
        Step::Find,
        Step::Download,
        Step::Unpack,
        Step::Check,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Step::Rosetta => "Preparar el Mac (macOS puede pedir tu contraseña)",
            Step::Find => "Buscar la versión más reciente",
            Step::Download => "Descargar Wine",
            Step::Unpack => "Instalarlo en la app",
            Step::Check => "Comprobar que funciona",
            Step::Done => "Listo",
        }
    }

    /// The step in a few words, for the sidebar chip, which has room for about 28 characters:
    /// the full labels (the Rosetta one tells about the password) would be cut mid-word there.
    pub fn short_label(self) -> &'static str {
        match self {
            Step::Rosetta => "Preparando el Mac…",
            Step::Find => "Buscando la versión…",
            Step::Download => "Descargando…",
            Step::Unpack => "Instalando en la app…",
            Step::Check => "Comprobando…",
            Step::Done => "Listo",
        }
    }
}
