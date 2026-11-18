//! The list of bottles and their programs: reading it again, choosing in it, its pictures.

use crate::backend::*;
use crate::*;

/// Lets one pass of a background job run at a time, and remembers that another was asked for
/// while it ran. Every reading of the lists asks for a new pass of the launchers' sync, and a
/// burst of them (a press on «Actualizar» again and again, a few bottles stopping together)
/// would otherwise start one worker thread for each, all writing the same folders at once.
///
/// The price is that a pass stuck in one of the Mac helper tools (`lsregister` is known to stall)
/// holds the gate until that tool answers, so later requests wait with it. Before the gate each
/// request started its own worker, and every one of them would have stalled on the same tool,
/// piling up a thread and a process per press; one stuck worker is the bounded outcome. The
/// launchers are a convenience, so a stale set until the helper answers (or the app restarts)
/// is accepted.
#[derive(Default)]
pub(crate) struct OnePass(std::sync::Mutex<PassState>);

#[derive(Default)]
struct PassState {
    running: bool,
    again: bool,
}

/// Marks the pass as over when it is dropped, also when the worker panicked.
pub(crate) struct PassDone(std::sync::Arc<OnePass>);

impl Drop for PassDone {
    fn drop(&mut self) {
        self.0.state().running = false;
    }
}

impl OnePass {
    fn state(&self) -> std::sync::MutexGuard<'_, PassState> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A pass is asked for. It starts (the guard is returned) when none is running; otherwise
    /// the request is remembered for [`Self::take_again`] and nothing starts.
    pub(crate) fn ask(self: &std::sync::Arc<Self>) -> Option<PassDone> {
        let mut state = self.state();
        if state.running {
            state.again = true;
            return None;
        }
        state.running = true;
        state.again = false;
        Some(PassDone(self.clone()))
    }

    /// Whether a pass was asked for while the last one ran, once it is over: then one more pass
    /// answers all those requests, with the lists as they are by then.
    pub(crate) fn take_again(&self) -> bool {
        let mut state = self.state();
        !state.running && std::mem::take(&mut state.again)
    }
}

impl App {
    /// The bottle used when none is chosen: the one marked as main, else the first one.
    pub(crate) fn default_index(&self) -> Option<usize> {
        if let Some(d) = &self.cfg.default_bottle {
            if let Some(i) = self
                .bottles
                .iter()
                .position(|b| &b.path.to_string_lossy() == d)
            {
                return Some(i);
            }
        }
        (!self.bottles.is_empty()).then_some(0)
    }

    pub(crate) fn is_default(&self, b: &Bottle) -> bool {
        self.default_index()
            .map(|i| self.bottles[i].path == b.path)
            .unwrap_or(false)
    }

    /// Keeps a Mac app for every Windows program, and the Finder double-click helper, up to date.
    pub(crate) fn sync_launchers(&self) {
        // Tests never build or register Mac launchers.
        if cfg!(test) || self.wine.is_none() || self.screenshot.is_some() || !self.cfg.mac_launchers
        {
            return;
        }
        let bottles: Vec<(Bottle, PathBuf)> = self
            .bottles
            .iter()
            .filter(|b| is_own_prefix(&b.path))
            .filter_map(|b| self.bottle_wine(b).map(|w| (b.clone(), w)))
            .collect();
        // One sync at a time: a request that arrives meanwhile is answered when it ends.
        let Some(pass) = self.launcher_pass.ask() else {
            return;
        };
        self.task(move |log| {
            let _pass = pass;
            let mut notes = vec![];
            let n = launchers::sync_noting(&launchers::root(), &bottles, &mut notes);
            for line in notes {
                push_tool_log(log, line);
            }
            if let Err(e) = launchers::install_opener() {
                push_log(
                    log,
                    format!("No se pudo preparar el doble clic en .exe: {e}"),
                );
            }
            vec![Msg::Launchers(n)]
        });
    }

    pub(crate) fn bottle(&self) -> Option<&Bottle> {
        self.bottles.get(self.selected)
    }

    pub(crate) fn rescan(&mut self) {
        let keep = self.bottle().map(|b| b.path.clone());
        self.bottles = scan_bottles(&self.cfg);
        if let Some(k) = keep {
            self.select_path(&k);
        }
        if self.selected >= self.bottles.len() {
            self.selected = 0;
        }
        self.prune_icons();
        // What the diagnosis holds is keyed by the bottle's folder: a bottle that is gone (deleted,
        // renamed) takes it along instead of leaving it for the rest of the session.
        let present: std::collections::HashSet<&Path> =
            self.bottles.iter().map(|b| b.path.as_path()).collect();
        self.diagnosis.retain_bottles(|p| present.contains(p));
        self.refresh_engines();
        self.sync_launchers();
    }

    /// The "Actualizar" button: reads the programs again, for installs the app did not see end.
    pub(crate) fn refresh_programs(&mut self) {
        self.rescan();
        self.done("Programas al día.");
    }

    /// Puts a program the person removed from the list back: only that program, never the ones
    /// that happen to share its name. A program an older version removed by name is replaced by
    /// entries for the other programs listed under that name, so they stay removed as before.
    pub(crate) fn unhide_program(&mut self, i: usize) {
        let Some(b) = self.bottle().cloned() else {
            return;
        };
        if let Some(p) = b.programs.get(i).filter(|p| p.listing == Listing::Removed) {
            self.cfg
                .bottle(&b.path)
                .unhide_program(&b.path, p, &b.programs);
            self.cfg.save();
            self.rescan();
        }
    }

    pub(crate) fn select_path(&mut self, p: &Path) {
        if let Some(i) = self.bottles.iter().position(|b| b.path == p) {
            self.selected = i;
        }
    }

    /// Lets go of the pictures of programs that are not in any bottle any more (uninstalled, or
    /// in a bottle that was renamed or deleted). A picture is a texture in the graphics memory,
    /// and without this every picture ever shown would stay there for as long as the app runs.
    pub(crate) fn prune_icons(&mut self) {
        let wanted: std::collections::HashSet<&PathBuf> = self
            .bottles
            .iter()
            .flat_map(|b| b.programs.iter().filter_map(|p| p.icon.as_ref()))
            .collect();
        keep_wanted_icons(&mut self.textures, &mut self.icons_requested, &wanted);
    }

    /// The decoded picture a bottle wears instead of its letter (see `Bottle::face`): the
    /// texture `load_icons` made, so nothing is read here. `None` while it is not decoded yet.
    pub(crate) fn bottle_picture(&self, b: &Bottle) -> Option<&egui::TextureHandle> {
        b.face()
            .and_then(|i| self.textures.get(i))
            .and_then(|t| t.as_ref())
    }

    /// The programs' own pictures (of every bottle, for the overview), decoded once on a worker
    /// so the window never waits for them; they appear when ready.
    pub(crate) fn load_icons(&mut self) {
        // Called every frame: the choice is made on references, so when every picture is
        // already there (nearly always) nothing is allocated, and only what is wanted is cloned.
        let wanted: Vec<PathBuf> = icons_to_request(
            self.bottles
                .iter()
                .flat_map(|b| b.programs.iter().filter_map(|p| p.icon.as_ref())),
            &self.textures,
            &self.icons_requested,
        )
        .into_iter()
        .cloned()
        .collect();
        if wanted.is_empty() {
            return;
        }
        self.icons_requested.extend(wanted.iter().cloned());
        self.task(move |_| {
            let decoded = wanted
                .into_iter()
                .map(|path| {
                    let img = decode_icon(&path);
                    (path, img)
                })
                .collect();
            vec![Msg::IconsLoaded(decoded)]
        });
    }
}

/// A program's picture, read from its file and made small enough for a tile.
pub(crate) fn decode_icon(path: &Path) -> Option<egui::ColorImage> {
    let img = image::open(path)
        .ok()?
        .resize(96, 96, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

/// The pictures among `icons` that are neither decoded nor already asked for, as references.
/// Nothing is allocated when there are none.
fn icons_to_request<'a, T>(
    icons: impl Iterator<Item = &'a PathBuf>,
    pictures: &std::collections::HashMap<PathBuf, T>,
    requested: &std::collections::HashSet<PathBuf>,
) -> Vec<&'a PathBuf> {
    icons
        .filter(|i| !pictures.contains_key(*i) && !requested.contains(*i))
        .collect()
}

/// Drops the pictures (and the requests for pictures) of files that are not wanted any more.
/// Returns how many pictures went.
fn keep_wanted_icons<T>(
    pictures: &mut std::collections::HashMap<PathBuf, T>,
    requested: &mut std::collections::HashSet<PathBuf>,
    wanted: &std::collections::HashSet<&PathBuf>,
) -> usize {
    let before = pictures.len();
    pictures.retain(|path, _| wanted.contains(path));
    requested.retain(|path| wanted.contains(path));
    before - pictures.len()
}

/// Whether a bottle was running and no longer is: its programs may have changed meanwhile.
pub(crate) fn some_bottle_stopped(before: &[PathBuf], now: &[PathBuf]) -> bool {
    before.iter().any(|b| !now.contains(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        /// Allocations made by this thread, counted by [`Counting`].
        static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    }

    /// The system allocator, counting the allocations of each thread, so a test can tell that
    /// a piece of code allocated nothing without the other tests' threads disturbing the count.
    struct Counting;

    // SAFETY: every call is passed on unchanged to the system allocator.
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
            System.alloc(l)
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            System.dealloc(p, l)
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
            System.realloc(p, l, n)
        }
    }

    #[global_allocator]
    static COUNTING: Counting = Counting;

    /// `load_icons` runs on every frame: with every picture decoded or asked for, choosing what
    /// to request allocates nothing and picks nothing; and it clones only what is wanted.
    #[test]
    fn choosing_pictures_allocates_nothing_when_all_are_loaded() {
        let icons: Vec<PathBuf> = (0..40)
            .map(|i| PathBuf::from(format!("/bottles/Steam/icons/program-number-{i}.png")))
            .collect();
        let mut pictures: std::collections::HashMap<PathBuf, u8> = Default::default();
        let mut requested: std::collections::HashSet<PathBuf> = Default::default();
        for (i, p) in icons.iter().enumerate() {
            if i % 2 == 0 {
                pictures.insert(p.clone(), 0);
            } else {
                requested.insert(p.clone());
            }
        }
        let before = ALLOCATIONS.with(Cell::get);
        let wanted = icons_to_request(icons.iter(), &pictures, &requested);
        let allocated = ALLOCATIONS.with(Cell::get) - before;
        assert!(wanted.is_empty());
        assert_eq!(allocated, 0, "nothing is wanted, so nothing is allocated");

        // A new picture is the only one chosen.
        let mut more = icons.clone();
        more.push(PathBuf::from("/bottles/Steam/icons/new.png"));
        let wanted = icons_to_request(more.iter(), &pictures, &requested);
        assert_eq!(wanted, [&PathBuf::from("/bottles/Steam/icons/new.png")]);
    }

    /// A program that left (uninstalled, or its bottle renamed) takes its picture with it; the
    /// pictures still in use, and the ones still being decoded for them, stay.
    #[test]
    fn pictures_of_programs_that_left_are_dropped() {
        let (a, b, c) = (
            PathBuf::from("/b/a.png"),
            PathBuf::from("/b/b.png"),
            PathBuf::from("/b/c.png"),
        );
        let mut pictures: std::collections::HashMap<PathBuf, u8> =
            [(a.clone(), 1), (b.clone(), 2)].into();
        let mut requested: std::collections::HashSet<PathBuf> =
            [a.clone(), b.clone(), c.clone()].into();
        let wanted: std::collections::HashSet<&PathBuf> = [&a, &c].into();
        assert_eq!(keep_wanted_icons(&mut pictures, &mut requested, &wanted), 1);
        assert_eq!(pictures.keys().collect::<Vec<_>>(), [&a]);
        assert!(requested.contains(&a) && requested.contains(&c) && !requested.contains(&b));
        // Nothing is wanted: nothing is kept.
        let none = std::collections::HashSet::new();
        assert_eq!(keep_wanted_icons(&mut pictures, &mut requested, &none), 1);
        assert!(pictures.is_empty() && requested.is_empty());
    }

    /// Requests that arrive while a pass runs start nothing; once it ends, one more pass
    /// answers all of them. A pass that panics still ends.
    #[test]
    fn requests_during_a_pass_make_one_more_pass() {
        let gate: std::sync::Arc<OnePass> = Default::default();
        assert!(!gate.take_again(), "nothing was asked");
        let first = gate.ask().expect("the first request starts a pass");
        let started: Vec<bool> = (0..50).map(|_| gate.ask().is_some()).collect();
        assert!(started.iter().all(|s| !s), "a pass is running: none starts");
        assert!(!gate.take_again(), "the pass has not ended yet");
        drop(first);
        assert!(gate.take_again(), "fifty requests wait for one more pass");
        assert!(!gate.take_again(), "and only one");
        let second = gate.ask().expect("a new pass may start");
        let panicked = std::thread::spawn(move || {
            let _second = second;
            panic!("the worker failed");
        })
        .join();
        assert!(panicked.is_err());
        assert!(gate.ask().is_some(), "a pass that panicked still ended");
    }

    #[test]
    fn a_bottle_leaving_the_running_set_is_noticed() {
        let a = [PathBuf::from("/b/a")];
        let b = [PathBuf::from("/b/b")];
        let both = [a[0].clone(), b[0].clone()];
        assert!(!some_bottle_stopped(&[], &a));
        assert!(!some_bottle_stopped(&a, &both));
        assert!(some_bottle_stopped(&both, &b));
        assert!(some_bottle_stopped(&a, &[]));
    }
}
