//! Purpose:
//! The history shared by the collector and the monitor page: the last hour of
//! windows in memory, the store behind it, and the plan that assembles a range
//! from both.
//!
//! Called from:
//! - The collector (writes), the dashboard and alerts (read).
//!
//! Key details:
//! - A range is planned under the history's lock and read from disk after it
//!   is released, so a week-long view never holds up the poll.

use super::*;

/// How far back memory reaches: the hour the longest `--alert-window`
/// measures, at one window per poll.
const MEMORY_SPAN_MS: u64 = MEMORY_WINDOWS as u64 * POLL_INTERVAL.as_millis() as u64;
/// The most windows memory holds: an hour of every program a listening
/// collector serves, each pushing one window per push interval. It bounds the
/// budgets' folded hour too, which stays far below it: a few windows a second.
const MEMORY_WINDOWS_CEILING: usize = collector::MAX_SOURCES
    * (MEMORY_SPAN_MS / elephc_probe::push::INTERVAL.as_millis() as u64) as usize;

/// State shared between the collector and the dashboard.
pub(crate) struct History {
    /// What is being watched, for the page title.
    pub target: String,
    /// The most recent windows, oldest first.
    pub windows: VecDeque<Window>,
    /// The latest cumulative snapshot, for `/metrics`.
    pub latest: Option<Snapshot>,
    /// Each pushing program's latest snapshot, by source (`--listen`): one
    /// `/metrics` series per program.
    pub latest_by_source: std::collections::BTreeMap<String, Snapshot>,
    /// Whether this history is fed by programs that push (`--listen`): its
    /// stored windows are theirs, under their own source names.
    pub pushed: bool,
    /// Why the last poll failed, while it keeps failing.
    pub last_error: Option<String>,
    /// The on-disk history, for spans longer than memory holds.
    pub store: Option<Store>,
    /// Whether a collector is feeding this history (false when replaying a store).
    pub live: bool,
    /// Profiles captured from the page, newest last, at most ten.
    pub profiles: VecDeque<(u128, crate::call_graph::CallGraph)>,
    /// Where and with which key a profile can be captured; `None` when the
    /// page browses a store with no service behind it.
    pub capture: Option<(String, [u8; 32])>,
    /// Exact captures of slow and failed requests (`--capture-every`).
    pub traces: traces::TraceBook,
    /// Seconds between automatic captures, 0 when they are off.
    pub capture_every: u64,
    /// What the budgets measure: the last hour's windows folded by the second
    /// they ended in, the second they began in and their kind (see
    /// `fold_for_alerts`).
    pub alert_ledger: std::collections::BTreeMap<(u64, u64, bool), Vec<Window>>,
    /// How many windows `alert_ledger` holds, for its ceiling.
    pub alert_windows_held: usize,
    /// Budgets checked after every poll (`--alert`).
    pub alerts: alerts::Alerts,
    /// The deploys seen, and the builds each service served (`deploys`).
    pub deploys: deploys::Deploys,
}

impl History {
    /// An empty history of `target`, backed by `store` for ranges memory does
    /// not hold; `live` when a collector feeds it, rather than a page replaying a
    /// store.
    pub(crate) fn new(target: &str, store: Option<Store>, live: bool) -> Self {
        // The builds the store saw served: a collector restarted on the same build marks no deploy.
        let deploys = deploys::Deploys::from_list(store.as_ref().map(|store| deploys::read(store.dir())).unwrap_or_default());
        let store_kept = store.is_some();
        Self {
            deploys,
            target: target.to_string(),
            windows: VecDeque::with_capacity(MEMORY_WINDOWS),
            latest: None,
            latest_by_source: std::collections::BTreeMap::new(),
            pushed: false,
            last_error: None,
            store,
            live,
            profiles: VecDeque::new(),
            capture: None,
            traces: traces::TraceBook::stored(store_kept),
            capture_every: 0,
            alert_ledger: std::collections::BTreeMap::new(),
            alert_windows_held: 0,
            alerts: alerts::Alerts::default(),
        }
    }

    /// Appends a window, dropping the oldest once memory holds both an hour
    /// and `MEMORY_WINDOWS` windows.
    ///
    /// By time, not only by count: a listening collector keeps every pushed
    /// program's windows in one memory, and a count shared by 256 programs
    /// held about a minute of each (review). The ceiling keeps that memory
    /// bounded however many windows arrive; what falls out of it, the page
    /// reads from the store, and the budgets from their own folded hour
    /// (`fold_for_alerts`).
    pub(crate) fn push(&mut self, window: Window) {
        self.fold_for_alerts(&window);
        let keep_from = window.t.saturating_sub(MEMORY_SPAN_MS);
        while let Some(oldest) = self.windows.front() {
            let full = self.windows.len() >= MEMORY_WINDOWS_CEILING;
            let aged = self.windows.len() >= MEMORY_WINDOWS && oldest.t < keep_from;
            if !(full || aged) {
                break;
            }
            self.windows.pop_front();
        }
        self.windows.push_back(window);
    }

    /// Folds a window into what the budgets measure.
    ///
    /// Memory may lose the start of the hour: past its ceiling it drops its
    /// oldest windows, which the page then reads from the store, but alerts
    /// read nothing else, and many short-lived jobs — each its own source,
    /// each sending a window as it exits — could fill that ceiling within the
    /// hour an `--alert-window` measures (review). Folded, the hour holds a
    /// few windows per second however many programs send.
    ///
    /// Windows fold when they ended and began in the same seconds, are of the
    /// same kind (a service's window or a script's run), AND overlap or
    /// touch: the folded window then covers exactly the time its windows
    /// covered, which is what `measured_ms` adds up. Folding two jobs of
    /// 100–200 ms and 800–900 ms into 100–900 ms counted 800 ms for 200, and
    /// read their throughput four times too low (review). Windows a gap
    /// separates stay apart in the same second's list.
    fn fold_for_alerts(&mut self, window: &Window) {
        let start = window.t.saturating_sub(window.span_ms);
        let key = (window.t / 1_000, start / 1_000, is_run(window));
        let held = self.alert_ledger.entry(key).or_default();
        // The incoming window, grown by every held one it overlaps; each
        // merge can reach one more, so the held ones are taken out until none
        // is left to join.
        let mut folded = window.clone();
        let (mut from, mut to) = (start, window.t);
        loop {
            let Some(index) = held.iter().position(|w| w.t.saturating_sub(w.span_ms) <= to && from <= w.t) else {
                break;
            };
            let joined = held.swap_remove(index);
            self.alert_windows_held -= 1;
            from = from.min(joined.t.saturating_sub(joined.span_ms));
            to = to.max(joined.t);
            if let Some(mut merged) = coalesce(&[joined, folded.clone()]) {
                merged.t = to;
                merged.span_ms = to - from;
                folded = merged;
            }
        }
        held.push(folded);
        self.alert_windows_held += 1;
        let keep_from = window.t.saturating_sub(MEMORY_SPAN_MS) / 1_000;
        loop {
            let full = self.alert_windows_held > MEMORY_WINDOWS_CEILING;
            let Some(oldest) = self.alert_ledger.first_entry() else {
                break;
            };
            if oldest.key().0 >= keep_from && !full {
                break;
            }
            self.alert_windows_held -= oldest.remove().len();
        }
    }

    /// Windows ending in `(from_ms, to_ms]`, from memory where it reaches back
    /// far enough and from the store before that.
    #[cfg(test)]
    pub(crate) fn windows_in(&self, from_ms: u64, to_ms: u64) -> Vec<Window> {
        self.plan(from_ms, to_ms).load()
    }

    /// Which stored windows and captures belong on this collector's page: its own service's,
    /// its programs' when it listens, or every one when the page replays a store.
    pub(crate) fn source_filter(&self) -> SourceFilter {
        match (self.live, self.pushed) {
            (false, _) => SourceFilter::Any,
            (true, true) => SourceFilter::Pushed,
            (true, false) => SourceFilter::Service(self.target.clone()),
        }
    }

    /// What `windows_in` needs, taken without reading the disk, so a caller
    /// holding the history's lock can release it before the store is read.
    pub(crate) fn plan(&self, from_ms: u64, to_ms: u64) -> RangePlan {
        let memory_start = self.windows.front().map(|w| w.t.saturating_sub(w.span_ms));
        let reaches = matches!(memory_start, Some(start) if start <= from_ms);
        RangePlan {
            from_ms,
            to_ms,
            store: self.store.clone(),
            services_before: (!reaches).then(|| memory_start.unwrap_or(to_ms).min(to_ms)),
            // A live collector's page is about its own service: another
            // service's windows in a shared store would add its traffic. A
            // listening collector's are its programs', each under its own
            // source name (review).
            own: self.source_filter(),
            memory: self
                .windows
                .iter()
                .filter(|w| w.t > from_ms && w.t <= to_ms)
                .cloned()
                .collect(),
        }
    }

    /// The end of the newest window, or the newest stored one when replaying.
    pub(crate) fn newest_ms(&self) -> Option<u64> {
        self.windows
            .back()
            .map(|w| w.t)
            .or_else(|| self.store.as_ref().and_then(Store::newest_ms))
    }
}

/// A range of windows to assemble: from the store up to where memory begins,
/// then from memory. Built under the history's lock, loaded after it.
pub(crate) struct RangePlan {
    from_ms: u64,
    to_ms: u64,
    store: Option<Store>,
    /// Where memory begins, when the range reaches back past it: stored
    /// service windows that started before it are read. `None` when memory
    /// covers the range's services.
    services_before: Option<u64>,
    own: SourceFilter,
    memory: Vec<Window>,
}

/// Which stored windows a history's page shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SourceFilter {
    /// Every one: a page replaying a store.
    Any,
    /// The collector's own service's.
    Service(String),
    /// Those of programs that pushed (`--listen`).
    Pushed,
}

impl SourceFilter {
    /// Whether a stored window from `source` belongs on the page.
    pub(crate) fn keeps(&self, source: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Service(target) => source.is_empty() || source == target,
            Self::Pushed => super::collector::is_pushed_source(source),
        }
    }
}

impl RangePlan {
    /// Reads the stored part and joins the memory part after it, then the
    /// script runs of the whole range.
    ///
    /// A stored service window that began before memory does is read whole,
    /// even when it ends after: its earlier requests have left memory. Stored
    /// windows are coarser than memory's one-second ones, so the oldest memory
    /// window usually starts inside one, and cutting at the end time dropped it.
    /// Memory then joins only after the last stored SERVICE window.
    ///
    /// Script runs never set that cut: memory holds only the collector's own
    /// windows, so a long run ending inside it would have hidden every memory
    /// window before its end. They come from the store over the
    /// whole range instead, which also shows a run that ended in the last hour.
    pub(crate) fn load(self) -> Vec<Window> {
        let mut out = Vec::new();
        if let (Some(store), Some(memory_start)) = (&self.store, self.services_before) {
            let own = |w: &Window| self.own.keeps(&w.source);
            out.extend(store.load(self.from_ms, self.to_ms).into_iter().filter(|w| {
                !is_run(w) && w.t > self.from_ms && w.t.saturating_sub(w.span_ms) < memory_start && own(w)
            }));
        }
        // Memory joins after the last stored window OF ITS SOURCE: with
        // several programs pushing, one program's stored window crossing the
        // memory boundary must not hide another's memory windows (review).
        let mut stored_until: std::collections::HashMap<&str, u64> = std::collections::HashMap::new();
        for window in &out {
            let until = stored_until.entry(window.source.as_str()).or_insert(0);
            *until = (*until).max(window.t);
        }
        let stored_until: std::collections::HashMap<String, u64> =
            stored_until.into_iter().map(|(source, t)| (source.to_string(), t)).collect();
        // A service's own windows keep one cutoff: older stored ones may
        // carry no source at all.
        let from_ms = self.from_ms;
        let single = out.last().map_or(from_ms, |w| w.t.max(from_ms));
        let per_source = self.own == SourceFilter::Pushed;
        out.extend(self.memory.into_iter().filter(|w| {
            let after = if per_source {
                stored_until.get(&w.source).map_or(from_ms, |t| (*t).max(from_ms))
            } else {
                single
            };
            w.t > after
        }));
        if let Some(store) = &self.store {
            out.extend(store.runs_in(self.from_ms, self.to_ms));
        }
        out.sort_by_key(|w| w.t);
        out
    }
}

/// The history plus the signal that a window arrived.
pub(crate) type Shared = Arc<(Mutex<History>, Condvar)>;
