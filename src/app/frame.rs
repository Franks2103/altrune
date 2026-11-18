//! What happens on each frame: eframe's callbacks, the work between two frames and the
//! `--screenshot` countdown.

use super::Msg;
use crate::backend::is_running_at;
use crate::ui;
use crate::wine_setup;
use crate::{measure::perf, App};
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Marks a live check as finished when it is dropped, also if the worker stops early.
struct LiveCheckDone(Arc<AtomicBool>);

impl Drop for LiveCheckDone {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl App {
    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else {
            return;
        };
        self.frames += 1;
        ctx.request_repaint();
        if self.shot_done.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.frames >= 120 && !self.shot_requested {
            self.shot_requested = true;
            let done = self.shot_done.clone();
            let ctx2 = ctx.clone();
            ctx.request_screenshot(move |img| {
                let [w, h] = img.size;
                let mut buf = Vec::with_capacity(w * h * 4);
                for p in &img.pixels {
                    buf.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
                }
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                match image::save_buffer(&path, &buf, w as u32, h as u32, image::ColorType::Rgba8) {
                    Ok(()) => {
                        eprintln!("saved {}", path.display());
                        perf::report();
                    }
                    Err(e) => eprintln!("screenshot failed: {e}"),
                }
                done.store(true, Ordering::SeqCst);
                ctx2.request_repaint();
            });
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.shell(ui);
    }

    /// The window is closing in the middle of an install: the download or unpacking it started is
    /// ended with it, so nothing keeps running into the app's folder after the app is gone.
    /// Programs the person opened are not touched: they go on running after the app closes.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        wine_setup::lock_progress(&self.setup).cancel = true;
        wine_setup::stop_install_children();
    }

    fn clear_color(&self, v: &egui::Visuals) -> [f32; 4] {
        let p = if v.dark_mode {
            &ui::theme::DARK
        } else {
            &ui::theme::LIGHT
        };
        p.canvas.to_normalized_gamma_f32()
    }
}

impl App {
    /// What happens between two frames: the workers' messages are read, the live check runs and
    /// the next repaint is scheduled.
    pub(crate) fn tick(&mut self, ctx: &egui::Context) {
        self.poll();
        self.follow_requests(ctx);
        // Which bottles really have Wine running (the first process ending doesn't mean the
        // program closed): checked every 3 s on a worker thread.
        let now = ctx.input(|i| i.time);
        if self.screenshot.is_none()
            && now - self.last_live_check > 3.0
            && !self.live_check_busy.load(Ordering::SeqCst)
        {
            self.last_live_check = now;
            // Only the folder and the Wine of each bottle go to the worker, not the bottles.
            let pairs: Vec<(PathBuf, PathBuf)> = self
                .bottles
                .iter()
                .filter_map(|b| self.bottle_wine_cached(b).map(|w| (b.path.clone(), w)))
                .collect();
            self.live_check_busy.store(true, Ordering::SeqCst);
            let done = LiveCheckDone(self.live_check_busy.clone());
            self.task(move |_| {
                let _done = done;
                vec![Msg::RunningSet(
                    pairs
                        .iter()
                        .filter(|(prefix, w)| is_running_at(w, prefix))
                        .map(|(prefix, _)| prefix.clone())
                        .collect(),
                )]
            });
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(
            if self.running.is_empty() && self.busy.is_empty() {
                3000
            } else {
                500
            },
        ));
        if !self.running.is_empty()
            || !self.busy.is_empty()
            || self.installing()
            || self.wine_state == wine_setup::WineState::Unknown
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        self.screenshot_step(ctx);
        if let Some(probe) = &mut self.idle_probe {
            probe.step(ctx);
        }
    }
}
