//! Looking for known problems in what Wine printed, and saving a report.

use crate::backend::*;
use crate::*;
use std::time::Duration;

/// What a worker thread needs to read one bottle's messages and judge them: facts the window
/// already knows, so the worker never touches the window's state.
struct Job {
    bottle: Bottle,
    /// The Wine the bottle runs on, when there is one.
    wine: Option<PathBuf>,
    /// When something last started in the bottle (older program logs are skipped).
    started: Option<std::time::SystemTime>,
    /// How long to wait before reading, so the last lines of a program that just closed have
    /// time to arrive from its pipes.
    settle: Duration,
}

impl Job {
    /// Reads the lines of the bottle's latest attempt, its program logs, and what can be
    /// confirmed about the bottle. Runs on a worker thread: it reads files and starts the
    /// processes that check Rosetta.
    fn read(&self, log: &Log) -> (diagnostics::Gathered, diagnostics::Context) {
        if !self.settle.is_zero() {
            std::thread::sleep(self.settle);
        }
        self.read_with_log_file(log, &log_file())
    }

    /// [`Job::read`] with the log file given. A program opened from the Finder with no window
    /// printed into that file only, so when it is the bottle's latest start its lines are read
    /// from there, in place of the window's older ones.
    fn read_with_log_file(
        &self,
        log: &Log,
        file: &Path,
    ) -> (diagnostics::Gathered, diagnostics::Context) {
        let name = &self.bottle.name;
        let window = log.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let (ring, kept) = match diagnostics::read_windowless_run(
            file,
            name,
            marker_bottle,
            crate::backend::mac_user_once(),
        ) {
            Some(run) => (run.ring, run.kept),
            None => (lines_since_run(&window, name), kept_for_bottle(name)),
        };
        let gathered = diagnostics::Gathered {
            ring,
            kept,
            logs: diagnostics::read_program_logs(&self.bottle.path, self.started),
        };
        let ctx = diagnostics::Context {
            uses_dxmt: self
                .wine
                .as_deref()
                .map(|w| wine_setup::Engine::of(w) == wine_setup::Engine::Metal),
            rosetta_missing: wine_setup::rosetta_missing_checked(),
            mac_user: crate::backend::mac_user_once().to_string(),
            names: diagnostics::wine_user_names(&self.bottle.path),
        };
        (gathered, ctx)
    }
}

/// The file name the save dialog offers for the report.
pub(crate) const REPORT_FILE_NAME: &str = "informe-altrune.txt";

/// The «App» line of the report: the app's name and version.
pub(crate) fn app_line() -> String {
    format!("{} {}", crate::APP_NAME, env!("CARGO_PKG_VERSION"))
}

/// Why the report could not be saved, in plain Spanish (never the system's English text).
fn report_not_saved_text(e: &std::io::Error, log: &Log, dest: &Path) -> String {
    format!(
        "No se pudo guardar el informe. {}",
        io_error_logged(e, log, "Guardar el informe", dest)
    )
}

impl App {
    fn diagnosis_job(&self, bottle: &Path, settle: Duration) -> Option<Job> {
        let b = self.bottles.iter().find(|b| b.path == bottle)?.clone();
        Some(Job {
            wine: self.bottle_wine(&b),
            started: self.diagnosis.started(&b.path),
            bottle: b,
            settle,
        })
    }

    /// Notes that something starts in this bottle now: the diagnosis will look only at what is
    /// printed from here on, and at program logs written from here on.
    pub(crate) fn begin_run(&mut self, bottle: &Path) {
        if let Some(name) = self
            .bottles
            .iter()
            .find(|b| b.path == bottle)
            .map(|b| b.name.clone())
        {
            note_run_start(&self.log, &name);
            self.diagnosis.note_start(bottle);
        }
    }

    /// Reads what the given bottle printed in its latest attempt, plus its known program logs,
    /// and looks for known symptoms. The result is stored for that bottle only.
    pub(crate) fn run_diagnosis(&mut self, bottle: PathBuf, settle: Duration) {
        // A press while the bottle is already being read is answered by the one that runs (its
        // button shows the spinner). A start that waits for a program to settle (after a failed
        // exit) is newer than what is being read, so it takes the place of that.
        if settle.is_zero() && self.diagnosis.is_working(&bottle) {
            return;
        }
        let Some(job) = self.diagnosis_job(&bottle, settle) else {
            return;
        };
        let ticket = self.diagnosis.begin(&bottle);
        self.say("Analizando los mensajes…");
        self.task_freeing(OnPanic::diagnosing(&bottle), move |log| {
            let (gathered, ctx) = job.read(log);
            let mut d = diagnostics::analyze_bottle(&job.bottle.path, &gathered, &ctx);
            d.ticket = ticket;
            vec![Msg::Diagnosis(d)]
        });
    }

    /// Saves a report for asking for help. Known personal data is hidden; the report says so and
    /// asks the reader to check it before sharing.
    pub(crate) fn export_report(&mut self) {
        let Some(bottle) = self.bottle().map(|b| b.path.clone()) else {
            return;
        };
        let Some(job) = self.diagnosis_job(&bottle, Duration::ZERO) else {
            return;
        };
        // One reading at a time: the report waits for the one that runs.
        if self.diagnosis.is_working(&bottle) {
            self.say("Estoy analizando los mensajes: un momento.");
            return;
        }
        let Some(dest) = super::native_dialog(|| {
            rfd::FileDialog::new()
                .set_title("Guardar informe")
                .set_file_name(REPORT_FILE_NAME)
                .save_file()
        }) else {
            return;
        };
        let app_wine = self.wine_ver.clone();
        let bottle_wine = job
            .wine
            .as_ref()
            .map(|w| w.to_string_lossy().to_string())
            .unwrap_or_default();
        let ticket = self.diagnosis.begin(&bottle);
        self.say("Preparando el informe…");
        self.task_freeing(OnPanic::diagnosing(&bottle), move |log| {
            let (gathered, ctx) = job.read(log);
            let mut d = diagnostics::analyze_bottle(&job.bottle.path, &gathered, &ctx);
            d.ticket = ticket;
            let b = &job.bottle;
            let macos = std::process::Command::new("/usr/bin/sw_vers")
                .arg("-productVersion")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            let info: Vec<(&str, String)> = vec![
                ("App", app_line()),
                ("macOS", macos),
                ("Wine", app_wine),
                ("Botella", b.name.clone()),
                ("Windows", windows_label(&b.windows)),
                ("Wine de la botella", bottle_wine),
            ];
            let redactor = diagnostics::Redactor::for_this_mac(&ctx.mac_user, &ctx.names);
            let text = diagnostics::report(&info, &d, &gathered, &redactor);
            let saved = match std::fs::write(&dest, text) {
                Ok(()) => Msg::Done("Informe guardado. Ocultamos los datos personales que conocemos; léelo antes de compartirlo.".to_string()),
                Err(e) => Msg::Fail(None, report_not_saved_text(&e, log, &dest)),
            };
            vec![Msg::Diagnosis(d), saved]
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_that_cannot_be_saved_is_explained_in_spanish() {
        let e = std::io::Error::from_raw_os_error(13);
        let log = Log::default();
        let text = report_not_saved_text(&e, &log, Path::new("/x/informe.txt"));
        assert_eq!(
            text,
            "No se pudo guardar el informe. No tengo permiso para hacerlo."
        );
        assert!(!text.contains("os error"));
        // The detail the sentence leaves out went to the log.
        let lines = log.lock().unwrap().join("\n");
        assert!(lines.contains("Guardar el informe") && lines.contains("/x/informe.txt"));
        assert!(lines.contains("Código del sistema: 13"), "{lines}");
    }

    /// The crash of a program opened from the Finder with no window is explained by
    /// «Buscar problemas»: its lines are in the log file only, and the diagnosis reads them there.
    #[test]
    fn the_diagnosis_reads_the_run_of_a_windowless_launch() {
        let dir = std::env::temp_dir().join(format!("bl-diag-finder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("registro.txt");
        std::fs::write(
            &file,
            format!(
                "◆ Nuevo intento en la botella «Sin ventana»\n▶ juego.exe {}\n0024:err:module:import_dll Library MSVCP140.dll (which is needed by L\"C:\\\\x.exe\") not found\n",
                diagnostics::FROM_FINDER
            ),
        )
        .unwrap();
        let job = Job {
            bottle: Bottle {
                name: "Sin ventana".into(),
                path: dir.join("Sin ventana"),
                windows: "win10".into(),
                msync: false,
                high_res: false,
                logpixels: 96,
                programs: vec![],
                env: vec![],
                prefix_complete: true,
            },
            wine: None,
            started: None,
            settle: Duration::ZERO,
        };
        // The window of this run of the app holds nothing about that start.
        let log: Log = Default::default();
        let (gathered, ctx) = job.read_with_log_file(&log, &file);
        let d = diagnostics::analyze_bottle(&job.bottle.path, &gathered, &ctx);
        assert!(
            d.findings.iter().any(|f| f.code == "missing-dll"),
            "{:?}",
            d.findings.iter().map(|f| f.code).collect::<Vec<_>>()
        );
        // Without the file, the same job finds nothing.
        let (none, _) = job.read_with_log_file(&log, &dir.join("no-hay.txt"));
        assert!(none.ring.is_empty() && none.kept.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
