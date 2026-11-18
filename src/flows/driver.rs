//! Drives the real `App` without a window: one egui frame at a time, with the accessibility
//! tree switched on so a test can find controls by name, press them and follow the keyboard
//! focus, the way VoiceOver or a keyboard user would.

use super::fake_wine::FakeWine;
use crate::{Action, App, StartOptions};
use eframe::egui::{self, accesskit, Event, Key, Modifiers, PointerButton, RawInput, Rect};
use std::path::Path;
use std::time::{Duration, Instant};

/// A window as small as the app allows, so the flows also prove the smallest layout works.
const WINDOW: (f32, f32) = (900.0, 640.0);

/// How much faster than the wall clock the frames' clock runs. The app looks at its bottles
/// every 3 s of that clock and an installation needs two idle looks, so at real speed each
/// install would cost six seconds of waiting; the app itself is unchanged.
const CLOCK_SPEED: f64 = 6.0;

pub struct Flow {
    pub app: App,
    pub fake: FakeWine,
    ctx: egui::Context,
    started: Instant,
    /// The size of the window the frames are run in; `resize` changes it.
    window: (f32, f32),
    /// Seconds the frames' clock was moved ahead by `advance`.
    skew: f64,
    last: Option<accesskit::TreeUpdate>,
}

impl Flow {
    /// The app on a fresh HOME that has the fake Wine (config.json points at it).
    pub fn start(home: &Path) -> Self {
        Self::with(FakeWine::write(home))
    }

    /// The app on a fresh HOME where no Wine is known yet: the first start.
    pub fn start_without_wine(home: &Path) -> Self {
        Self::with(FakeWine::write_without_config_path(home))
    }

    /// The app on a HOME the test already prepared: `FakeWine::write` ran, and the test may have
    /// changed config.json or put bottles on disk since. Nothing is written again.
    pub fn start_prepared(fake: FakeWine) -> Self {
        Self::with(fake)
    }

    fn with(fake: FakeWine) -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let app = App::with_context(ctx.clone(), None, None, StartOptions::default());
        let mut flow = Self {
            app,
            fake,
            ctx,
            started: Instant::now(),
            window: WINDOW,
            skew: 0.0,
            last: None,
        };
        flow.frame();
        flow
    }

    fn raw(&self, events: Vec<Event>) -> RawInput {
        RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(self.window.0, self.window.1),
            )),
            time: Some(self.started.elapsed().as_secs_f64() * CLOCK_SPEED + self.skew),
            events,
            ..Default::default()
        }
    }

    /// Makes the window `width` by `height` points from the next frame on.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.window = (width, height);
    }

    /// Moves the frames' clock ahead by `secs`, so an animation (a page that scrolls to the
    /// focus, a dialog that fades in) is over by the next frame without waiting for it.
    pub fn advance(&mut self, secs: f64) {
        self.skew += secs;
    }

    /// One frame, as eframe runs it: the between-frames work, then the window.
    pub fn frame(&mut self) {
        self.frame_with(vec![]);
    }

    fn frame_with(&mut self, events: Vec<Event>) {
        self.app.tick(&self.ctx);
        let raw = self.raw(events);
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(raw, |ui| app.shell(ui));
        out.textures_delta.clear();
        if let Some(update) = out.platform_output.accesskit_update.take() {
            self.last = Some(update);
        }
    }

    /// Runs frames about every 20 ms until `done` is true. On timeout it panics with what the
    /// window was saying and doing, which is what the failure is about.
    pub fn wait_until(&mut self, what: &str, timeout: Duration, done: impl Fn(&App) -> bool) {
        let end = Instant::now() + timeout;
        loop {
            self.frame();
            if done(&self.app) {
                return;
            }
            if Instant::now() > end {
                panic!("timed out waiting for {what}\n{}", self.dump());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Runs frames about every 20 ms until the number of live fake programs is one `want`
    /// accepts, and panics on timeout with what the window holds. The app follows a program from
    /// the moment its process starts, but the fake Wine records it a little later, and it ends a
    /// little after the app saw the close through: a single look in between races with both.
    pub fn wait_for_fake(&mut self, what: &str, timeout: Duration, want: impl Fn(usize) -> bool) {
        let end = Instant::now() + timeout;
        loop {
            self.frame();
            let alive = self.fake.alive().len();
            if want(alive) {
                return;
            }
            if Instant::now() > end {
                panic!(
                    "timed out waiting for {what}: {alive} fake programs alive\n{}",
                    self.dump()
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Runs frames about every 20 ms until `done` is true of the whole flow (what the window
    /// shows, what the fake Wine recorded, what the system holds), and panics on timeout with
    /// what the window holds. For what must happen but that no field of the app tells by itself.
    pub fn wait_for(&mut self, what: &str, timeout: Duration, done: impl Fn(&Flow) -> bool) {
        let end = Instant::now() + timeout;
        loop {
            self.frame();
            if done(self) {
                return;
            }
            if Instant::now() > end {
                panic!("timed out waiting for {what}\n{}", self.dump());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Runs frames for `span`, for what must not happen: a wait for something that has to
    /// happen is `wait_until`, `wait_for_fake` or `wait_for`, which end when it did and fail
    /// when it never does, however slow the machine is.
    pub fn settle(&mut self, span: Duration) {
        let end = Instant::now() + span;
        while Instant::now() < end {
            self.frame();
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// What the window shows and holds, for a failure message.
    pub fn dump(&self) -> String {
        let a = &self.app;
        format!(
            "status: {:?}\nerrors: {:?}\nbusy: {:?}\nrunning: {}\nwine: {:?}\ninstall: {:?}\nbottles: {:?}\nnames: {:?}\nfake wine calls:\n{}",
            a.status,
            a.errors,
            a.busy,
            a.running.len(),
            a.wine_state,
            a.install,
            a.bottles.iter().map(|b| &b.name).collect::<Vec<_>>(),
            self.names(),
            self.fake.calls().join("\n"),
        )
    }

    /// Moves the frames' clock forward by `secs` at once, for what the app does only every few
    /// seconds (its look at the bottles): the next frame sees that time as gone by.
    pub fn skip_clock(&mut self, secs: f64) {
        self.advance(secs);
    }

    /// A button of the interface, by its action, as the window would send it.
    pub fn act(&mut self, action: Action) {
        self.app.apply(action);
    }

    /// A key pressed and released, in two frames.
    pub fn key(&mut self, key: Key) {
        self.key_with(key, Modifiers::NONE);
    }

    /// A key pressed and released with modifiers held (Shift-Tab, for one), in two frames.
    pub fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        for pressed in [true, false] {
            self.frame_with(vec![Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            }]);
        }
    }

    /// Text typed into the focused field.
    pub fn type_text(&mut self, text: &str) {
        self.frame_with(vec![Event::Text(text.to_string())]);
        self.frame();
    }

    fn nodes(&self) -> Vec<&accesskit::Node> {
        self.last
            .as_ref()
            .map(|u| u.nodes.iter().map(|(_, n)| n).collect())
            .unwrap_or_default()
    }

    /// The tab called `name` as a screen reader gets it: whether it is the chosen one and what
    /// it says besides its name. None when no tab has that name.
    pub fn tab(&self, name: &str) -> Option<(bool, Option<String>)> {
        self.nodes()
            .iter()
            .find(|n| n.role() == accesskit::Role::Tab && n.label() == Some(name))
            .map(|n| {
                (
                    n.is_selected() == Some(true),
                    n.description().map(str::to_string),
                )
            })
    }

    /// The names of the controls and texts on screen, in tree order.
    pub fn names(&self) -> Vec<String> {
        self.nodes()
            .iter()
            .filter_map(|n| n.label().or_else(|| n.value()))
            .map(str::to_string)
            .collect()
    }

    /// Whether a node with exactly this name is on screen.
    pub fn shows(&self, name: &str) -> bool {
        self.names().iter().any(|n| n == name)
    }

    /// Whether some name on screen contains `part`.
    pub fn shows_part(&self, part: &str) -> bool {
        self.names().iter().any(|n| n.contains(part))
    }

    /// Where the control called `name` is, and the window it is in: a control is in view when
    /// its rectangle is inside the window's.
    pub fn bounds_in_window(&self, name: &str) -> Option<(Rect, Rect)> {
        let b = self
            .nodes()
            .iter()
            .find(|n| n.label() == Some(name) && n.bounds().is_some())
            .and_then(|n| n.bounds())?;
        let rect = Rect::from_min_max(
            egui::pos2(b.x0 as f32, b.y0 as f32),
            egui::pos2(b.x1 as f32, b.y1 as f32),
        );
        Some((
            rect,
            Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(self.window.0, self.window.1)),
        ))
    }

    /// The name of the node that has the keyboard focus, when it has one with a name.
    pub fn focused(&self) -> Option<String> {
        let update = self.last.as_ref()?;
        update
            .nodes
            .iter()
            .find(|(id, _)| *id == update.focus)
            .and_then(|(_, n)| n.label().or_else(|| n.value()))
            .map(str::to_string)
    }

    /// The place of the control that has the keyboard focus on screen, as (left, top, right,
    /// bottom) in points, when it has one.
    pub fn focused_bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let update = self.last.as_ref()?;
        let (_, node) = update.nodes.iter().find(|(id, _)| *id == update.focus)?;
        let b = node.bounds()?;
        Some((b.x0, b.y0, b.x1, b.y1))
    }

    /// The titles on screen as (text, level), in tree order.
    pub fn headings(&self) -> Vec<(String, usize)> {
        self.nodes()
            .iter()
            .filter(|n| n.role() == accesskit::Role::Heading)
            .filter_map(|n| Some((n.label()?.to_string(), n.level()?)))
            .collect()
    }

    /// What a screen reader would walk through: like `names`, without the texts that are marked
    /// hidden and without the text runs that repeat the words of their label.
    pub fn spoken(&self) -> Vec<String> {
        self.nodes()
            .iter()
            .filter(|n| !n.is_hidden() && n.role() != accesskit::Role::TextRun)
            .filter_map(|n| n.label().or_else(|| n.value()))
            .map(str::to_string)
            .collect()
    }

    /// The accessibility tree as indented text (role, name, value), to read when a check fails.
    pub fn tree(&self) -> String {
        let Some(update) = self.last.as_ref() else {
            return String::new();
        };
        let by_id: std::collections::HashMap<_, _> =
            update.nodes.iter().map(|(id, n)| (*id, n)).collect();
        let root = update.tree.as_ref().map(|t| t.root);
        let mut out = String::new();
        let mut stack: Vec<_> = root.into_iter().map(|id| (id, 0usize)).collect();
        while let Some((id, depth)) = stack.pop() {
            let Some(node) = by_id.get(&id) else { continue };
            out.push_str(&format!(
                "{}{:?} label={:?} value={:?}\n",
                "  ".repeat(depth),
                node.role(),
                node.label(),
                node.value()
            ));
            for child in node.children().iter().rev() {
                stack.push((*child, depth + 1));
            }
        }
        out
    }

    /// The first node on screen whose name contains `part`: whether it is switched off and what
    /// it says besides its name. None when no node has such a name.
    pub fn node_state(&self, part: &str) -> Option<(bool, Option<String>)> {
        self.nodes()
            .iter()
            .find(|n| n.label().is_some_and(|l| l.contains(part)) && n.bounds().is_some())
            .map(|n| (n.is_disabled(), n.description().map(str::to_string)))
    }

    /// Presses the node of this `role` called `name` (a name can also belong to a label of the
    /// same words), with the pointer. Panics when there is none.
    pub fn click_role(&mut self, role: accesskit::Role, name: &str) {
        let bounds = self
            .nodes()
            .iter()
            .find(|n| n.role() == role && n.label() == Some(name) && n.bounds().is_some())
            .and_then(|n| n.bounds())
            .unwrap_or_else(|| panic!("no {role:?} called {name:?}\n{}", self.dump()));
        self.press_at(bounds);
    }

    /// Presses the control called `name` with the pointer: moves to its centre, presses, lets
    /// go. Panics when no such control is on screen.
    pub fn click(&mut self, name: &str) {
        let bounds = self
            .nodes()
            .iter()
            .find(|n| n.label() == Some(name) && n.bounds().is_some())
            .and_then(|n| n.bounds())
            .unwrap_or_else(|| panic!("no control called {name:?}\n{}", self.dump()));
        self.press_at(bounds);
    }

    /// Moves to the centre of `bounds`, presses and lets go.
    fn press_at(&mut self, bounds: accesskit::Rect) {
        let at = egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.0) as f32,
            ((bounds.y0 + bounds.y1) / 2.0) as f32,
        );
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame_with(vec![Event::PointerMoved(at)]);
        self.frame_with(vec![button(true)]);
        self.frame_with(vec![button(false)]);
        self.frame();
    }

    /// The end of every flow: no fake program may be left running (they are ended here, and the
    /// test fails if one would not end).
    pub fn finish(self) {
        self.fake.end_all();
        let left = self.fake.alive();
        assert!(left.is_empty(), "fake programs still alive: {left:?}");
    }
}

/// How many file descriptors this process holds open now: the entries of `/dev/fd`. Reading
/// the folder holds one descriptor itself, the same one every time, so two counts compare.
pub fn open_descriptors() -> usize {
    std::fs::read_dir("/dev/fd")
        .expect("/dev/fd lists the open descriptors")
        .count()
}

/// How many threads this process runs now, from `ps -M` (one line for each thread under a
/// header line).
pub fn thread_count() -> usize {
    let out = std::process::Command::new("/bin/ps")
        .args(["-M", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps must run to count the threads");
    assert!(out.status.success(), "ps must succeed to count the threads");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .count()
}

impl Drop for Flow {
    /// A flow that failed half way must not leave fake programs behind either.
    fn drop(&mut self) {
        self.fake.end_all();
    }
}
