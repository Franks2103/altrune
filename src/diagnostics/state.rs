//! What the app remembers between reads: the matches kept as lines arrive, and the result,
//! working state and start time of each bottle.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::findings::Diagnosis;

/// One line that matched a rule, kept apart from the log window so that a line Wine prints
/// once at the start is still there when the window has long scrolled past it.
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
    /// Index into `RULES`.
    pub rule: usize,
    /// The line, already redacted and trimmed.
    pub line: String,
    /// The name of the bottle whose start came last when the line arrived.
    pub bottle: String,
    /// The run (start marker) that was current then.
    pub run: u64,
    /// How many identical lines arrived.
    pub count: usize,
}

/// The most recent distinct matches (see [`Kept`]), bounded so a program that repeats one error
/// thousands of times a second cannot grow it.
#[derive(Default)]
pub struct KeptStore {
    items: VecDeque<Kept>,
}

/// How many distinct matches are remembered.
pub const KEPT_MAX: usize = 200;

impl KeptStore {
    pub const fn new() -> Self {
        Self {
            items: VecDeque::new(),
        }
    }

    pub fn add(&mut self, k: Kept) {
        if let Some(same) = self.items.iter_mut().rev().find(|o| {
            o.rule == k.rule && o.run == k.run && o.bottle == k.bottle && o.line == k.line
        }) {
            same.count += k.count;
            return;
        }
        if self.items.len() >= KEPT_MAX {
            self.items.pop_front();
        }
        self.items.push_back(k);
    }

    /// What was kept for one run of one bottle, oldest first.
    pub fn for_run(&self, bottle: &str, run: u64) -> Vec<Kept> {
        self.items
            .iter()
            .filter(|k| k.run == run && k.bottle == bottle)
            .cloned()
            .collect()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.items.len()
    }
}

/// What the window keeps about diagnostics: the result for each bottle, which bottles are being
/// read right now, and when each bottle last started something.
#[derive(Default)]
pub struct Shown {
    working: HashSet<PathBuf>,
    results: HashMap<PathBuf, Diagnosis>,
    started: HashMap<PathBuf, SystemTime>,
    /// The last ticket handed out.
    last_ticket: u64,
    /// The only ticket of each bottle whose result is still wanted.
    wanted: HashMap<PathBuf, u64>,
}

impl Shown {
    /// Notes that a job reads this bottle now and returns its ticket. The job puts the ticket in
    /// its result; an older job still running no longer counts.
    pub fn begin(&mut self, bottle: &Path) -> u64 {
        self.last_ticket += 1;
        self.working.insert(bottle.to_path_buf());
        self.wanted.insert(bottle.to_path_buf(), self.last_ticket);
        self.last_ticket
    }

    pub fn is_working(&self, bottle: &Path) -> bool {
        self.working.contains(bottle)
    }

    /// Stores a finished result. Returns false (and stores nothing) when the result is stale:
    /// the bottle started something or changed engine since the job began, or a newer job took
    /// its place.
    pub fn finish(&mut self, d: Diagnosis) -> bool {
        if self.wanted.get(&d.bottle) != Some(&d.ticket) {
            return false;
        }
        self.wanted.remove(&d.bottle);
        self.working.remove(&d.bottle);
        self.results.insert(d.bottle.clone(), d);
        true
    }

    /// The result that belongs to this bottle, if there is one.
    pub fn result(&self, bottle: &Path) -> Option<&Diagnosis> {
        self.results.get(bottle)
    }

    /// Findings of a bottle that no longer apply (its engine changed). A job still reading the
    /// bottle is dropped too.
    pub fn forget(&mut self, bottle: &Path) {
        self.results.remove(bottle);
        self.wanted.remove(bottle);
        self.working.remove(bottle);
    }

    /// Lets go of everything held for bottles that `exists` no longer accepts (deleted or
    /// renamed ones). Every table is keyed by the bottle's folder, so without this each bottle
    /// the person ever diagnosed or opened a program in would stay here until the app closes. A
    /// job still reading such a bottle is dropped too: its answer would be stale.
    pub fn retain_bottles(&mut self, exists: impl Fn(&Path) -> bool) {
        self.results.retain(|bottle, _| exists(bottle));
        self.started.retain(|bottle, _| exists(bottle));
        self.working.retain(|bottle| exists(bottle));
        self.wanted.retain(|bottle, _| exists(bottle));
    }

    /// Notes that something starts in the bottle now. A job still reading what the bottle
    /// printed before is dropped.
    pub fn note_start(&mut self, bottle: &Path) {
        self.started.insert(bottle.to_path_buf(), SystemTime::now());
        self.wanted.remove(bottle);
        self.working.remove(bottle);
    }

    pub fn started(&self, bottle: &Path) -> Option<SystemTime> {
        self.started.get(bottle).copied()
    }

    /// How many bottles each of the four tables holds something for (results, starts, jobs
    /// reading, tickets wanted), so a long-session test can see that none keeps growing.
    #[cfg(test)]
    pub fn sizes(&self) -> [usize; 4] {
        [
            self.results.len(),
            self.started.len(),
            self.working.len(),
            self.wanted.len(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_store_is_bounded_and_counts_repeats() {
        let mut s = KeptStore::new();
        let k = |line: &str, run| Kept {
            rule: 0,
            line: line.to_string(),
            bottle: "B".into(),
            run,
            count: 1,
        };
        for _ in 0..1000 {
            s.add(k("same", 1));
        }
        assert_eq!(s.len(), 1);
        assert_eq!(s.for_run("B", 1)[0].count, 1000);
        for i in 0..(KEPT_MAX + 50) {
            s.add(k(&format!("line {i}"), 2));
        }
        assert_eq!(s.len(), KEPT_MAX);
        assert!(s.for_run("B", 1).is_empty(), "the oldest ones went first");
        assert!(s.for_run("Other", 2).is_empty());
        assert_eq!(s.for_run("B", 2).len(), KEPT_MAX);
    }

    #[test]
    fn shown_keeps_results_per_bottle_and_forgets_on_demand() {
        let (a, b) = (Path::new("/a"), Path::new("/b"));
        let mut s = Shown::default();
        let ticket = s.begin(a);
        assert!(s.is_working(a) && !s.is_working(b));
        assert!(s.finish(Diagnosis {
            bottle: a.to_path_buf(),
            findings: vec![],
            lines: 3,
            sources: vec![],
            ticket,
        }));
        assert!(!s.is_working(a));
        assert!(s.result(a).is_some(), "the result belongs to bottle a");
        assert!(s.result(b).is_none(), "and is never shown for bottle b");
        s.forget(a);
        assert!(s.result(a).is_none());
        assert!(s.started(a).is_none());
        s.note_start(a);
        assert!(s.started(a).is_some());
    }

    /// A bottle that is gone (deleted, renamed) takes its diagnosis, its start time and its
    /// pending job with it; the bottles that remain keep theirs.
    #[test]
    fn what_is_held_for_a_gone_bottle_is_let_go() {
        let (gone, kept) = (Path::new("/gone"), Path::new("/kept"));
        let mut s = Shown::default();
        for b in [gone, kept] {
            let ticket = s.begin(b);
            assert!(s.finish(result_with(b, ticket, 1)));
            s.note_start(b);
            s.begin(b);
        }
        assert_eq!(s.sizes(), [2, 2, 2, 2]);
        s.retain_bottles(|b| b == kept);
        assert_eq!(s.sizes(), [1, 1, 1, 1]);
        assert!(s.result(kept).is_some() && s.started(kept).is_some());
        assert!(s.result(gone).is_none() && s.started(gone).is_none());
        assert!(!s.is_working(gone) && s.is_working(kept));
    }

    fn result_with(bottle: &Path, ticket: u64, lines: usize) -> Diagnosis {
        Diagnosis {
            bottle: bottle.to_path_buf(),
            findings: vec![],
            lines,
            sources: vec![],
            ticket,
        }
    }

    #[test]
    fn stale_results_are_dropped() {
        let a = Path::new("/a");
        let mut s = Shown::default();
        // Two jobs overlap: only the latest one counts, and it keeps the bottle "working".
        let (first, second) = (s.begin(a), s.begin(a));
        assert!(!s.finish(result_with(a, first, 1)));
        assert!(s.is_working(a) && s.result(a).is_none());
        assert!(s.finish(result_with(a, second, 2)));
        assert_eq!(s.result(a).map(|d| d.lines), Some(2));
        // The engine changes while a job reads: its result must not come back.
        let t = s.begin(a);
        s.forget(a);
        assert!(!s.is_working(a));
        assert!(!s.finish(result_with(a, t, 3)));
        assert!(s.result(a).is_none());
        // A new start in the bottle drops the job that read the older attempt.
        let t = s.begin(a);
        s.note_start(a);
        assert!(!s.finish(result_with(a, t, 4)));
        assert!(s.result(a).is_none());
    }
}
