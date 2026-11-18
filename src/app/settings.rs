//! What the user sets for a bottle or for the app.

use crate::backend::*;
use crate::*;

/// The error for a Windows version that winecfg could not apply. `before` is the label of the
/// version the page showed until the change.
fn windows_failed_text(before: &str) -> String {
    format!("No se pudo cambiar la versión de Windows: la botella sigue con {before}.")
}

impl App {
    pub(crate) fn set_launchers(&mut self, on: bool) {
        self.cfg.mac_launchers = on;
        self.cfg.save();
        if on {
            self.sync_launchers();
        } else {
            launchers::remove_all(&launchers::root());
            self.launcher_count = 0;
        }
    }

    /// Shows the chosen appearance at once and keeps it for the next start.
    pub(crate) fn set_appearance(&mut self, appearance: Appearance) {
        self.cfg.appearance = appearance;
        ui::theme::apply_appearance(&self.ctx, appearance);
        self.cfg.save();
    }

    pub(crate) fn set_windows(&mut self, v: String) {
        let Some(wine) = self.need_wine() else { return };
        let Some(b) = self.bottle().cloned() else {
            return;
        };
        if self.refuse_if_held(&b.path, &b.name) || self.refuse_if_unfinished(&b) {
            return;
        }
        // The page shows what the config says, so the new version is saved only once winecfg
        // has applied it (Msg::WindowsSet); a failure leaves both as they were.
        let before = windows_label(&b.windows);
        self.busy
            .insert(b.path.clone(), "Cambiando la versión de Windows…".into());
        self.say("Cambiando la versión de Windows…");
        self.task_freeing(OnPanic::busy([b.path.clone()]), move |log| {
            let mut c = bottle_cmd(&wine, &b);
            c.args(["winecfg", "/v", &v]);
            let ok = run_logged(c, log, "Cambiar versión de Windows");
            vec![
                Msg::Busy(b.path.clone(), None),
                if ok {
                    Msg::WindowsSet(b.path.clone(), v)
                } else {
                    Msg::Fail(Some(b.path.clone()), windows_failed_text(&before))
                },
            ]
        });
    }

    pub(crate) fn set_msync(&mut self, on: bool) {
        let Some(b) = self.bottle().cloned() else {
            return;
        };
        self.cfg.bottle(&b.path).msync = Some(on);
        self.cfg.save();
        self.rescan();
        self.say("Se aplicará la próxima vez que abras un programa.");
    }

    pub(crate) fn set_high_res(&mut self, on: bool) {
        let Some(wine) = self.need_wine() else { return };
        let Some(b) = self.bottle().cloned() else {
            return;
        };
        // Wine must not touch a prefix that is being copied, saved, moved or switched, and the
        // end of this job would clear the mark of that other job.
        if self.refuse_if_held(&b.path, &b.name) || self.refuse_if_unfinished(&b) {
            return;
        }
        self.busy.insert(b.path.clone(), "Aplicando…".into());
        self.say("Aplicando la resolución…");
        self.task_freeing(OnPanic::busy([b.path.clone()]), move |log| {
            let ok = set_high_res(&wine, &b, on, log);
            vec![
                Msg::Busy(b.path.clone(), None),
                Msg::Rescan,
                if ok {
                    Msg::Done("Listo. Cierra y vuelve a abrir tus programas para verlo.".into())
                } else {
                    Msg::Fail(
                        Some(b.path.clone()),
                        "No se pudo cambiar la resolución.".into(),
                    )
                },
            ]
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_windows_change_names_the_version_kept() {
        let t = windows_failed_text("Windows 10");
        assert!(t.starts_with("No se pudo cambiar la versión de Windows"));
        assert!(t.ends_with("sigue con Windows 10."));
    }
}
