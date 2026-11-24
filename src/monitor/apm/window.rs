//! Purpose:
//! The window: one route row per route over a stretch of a service's time
//! (or one script run), the arithmetic over windows — differences of
//! snapshots, merges, the time a set of windows measured — and the folds the
//! store and the views apply to them.
//!
//! Called from:
//! - The collector, the store, the history, the views and the dashboard.
//!
//! Key details:
//! - Service windows tile time; a script's runs (epoch 0) do not, so time
//!   is measured as the union of service windows, or the spread of the runs.

use super::*;

/// One route's counters over one window, histogram kept sparse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RouteWindow {
    /// The shaped route label.
    pub route: String,
    /// Requests completed in the window.
    pub n: u64,
    /// Requests per status class, 1xx..5xx.
    pub status: [u64; apm::STATUS_CLASSES],
    /// Summed wall time, nanoseconds.
    pub sum_ns: u64,
    /// `(bucket, requests)` for every non-empty latency bucket.
    pub hist: Vec<(u16, u64)>,
    /// Database and outgoing-network operations and waits, when known: a
    /// script's run carries them from its exact capture.
    #[serde(default, skip_serializing_if = "Io::is_empty")]
    pub io: Io,
    /// The peak memory of the requests measured in the window.
    #[serde(default, skip_serializing_if = "Mem::is_empty")]
    pub mem: Mem,
    /// How the runs that ended in the window exited, `(exit code, runs)`: a script's run
    /// reports its exact code, so the page says `exit 255`, not only that it failed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exits: Vec<(i32, u64)>,
}

/// The peak memory of a route's measured requests over a window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Mem {
    /// Requests measured.
    pub n: u64,
    /// Their summed peak memory, bytes.
    pub sum: u64,
    /// `(bucket, requests)` for every non-empty memory bucket (`apm::mem_bucket_of`).
    pub hist: Vec<(u8, u64)>,
}

impl Mem {
    /// Nothing measured: no request, no bytes, no bucket. Every field counts: a request's
    /// bytes and bucket are published before its count, and a window cut between the two
    /// holds them with `n` at 0, which the next window takes as its baseline (review).
    pub(crate) fn is_empty(&self) -> bool {
        self.n == 0 && self.sum == 0 && self.hist.is_empty()
    }

    /// The sparse form of a route row's memory words.
    fn from_words(words: &[u64; apm::MEM_WORDS]) -> Self {
        Self {
            n: words[0],
            sum: words[1],
            hist: words[2..].iter().enumerate().filter(|(_, n)| **n > 0).map(|(b, n)| (b as u8, *n)).collect(),
        }
    }

    /// Adds this window's memory into a route row's memory words, saturating.
    fn add_to(&self, words: &mut [u64; apm::MEM_WORDS]) {
        words[0] = words[0].saturating_add(self.n);
        words[1] = words[1].saturating_add(self.sum);
        for (bucket, n) in &self.hist {
            if let Some(slot) = words.get_mut(2 + usize::from(*bucket)) {
                *slot = slot.saturating_add(*n);
            }
        }
    }
}

/// Database and outgoing-network work a transaction did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Io {
    /// Database statements executed.
    pub db_ops: u64,
    /// Nanoseconds blocked in database drivers.
    pub db_wait_ns: u64,
    /// Outgoing network operations.
    pub net_ops: u64,
    /// Nanoseconds blocked on outgoing network operations.
    pub net_wait_ns: u64,
}

impl Io {
    /// No database or network work recorded.
    pub(crate) fn is_empty(&self) -> bool {
        *self == Io::default()
    }

    /// Adds `other` into `self`, saturating: stored lines can hold any number.
    pub(crate) fn absorb(&mut self, other: &Io) {
        self.db_ops = self.db_ops.saturating_add(other.db_ops);
        self.db_wait_ns = self.db_wait_ns.saturating_add(other.db_wait_ns);
        self.net_ops = self.net_ops.saturating_add(other.net_ops);
        self.net_wait_ns = self.net_wait_ns.saturating_add(other.net_wait_ns);
    }
}

impl RouteWindow {
    /// The sparse form of a dense route row.
    pub(crate) fn from_stats(stats: &RouteStats) -> Self {
        Self {
            route: stats.name.clone(),
            n: stats.count,
            status: stats.status,
            sum_ns: stats.sum_ns,
            hist: stats
                .hist
                .iter()
                .enumerate()
                .filter(|(_, n)| **n > 0)
                .map(|(bucket, n)| (bucket as u16, *n))
                .collect(),
            io: Io {
                db_ops: stats.io[apm::IO_DB_OPS],
                db_wait_ns: stats.io[apm::IO_DB_WAIT],
                net_ops: stats.io[apm::IO_NET_OPS],
                net_wait_ns: stats.io[apm::IO_NET_WAIT],
            },
            mem: Mem::from_words(&stats.mem),
            exits: Vec::new(),
        }
    }

    /// Counts one more run that exited with `code`.
    pub(crate) fn add_exit(&mut self, code: i32, runs: u64) {
        match self.exits.iter_mut().find(|(seen, _)| *seen == code) {
            Some(row) => row.1 = row.1.saturating_add(runs),
            None => {
                self.exits.push((code, runs));
                self.exits.sort_unstable();
            }
        }
    }

    /// Adds this window's counters into a dense row, saturating: a damaged or
    /// edited store line can hold any number.
    pub(crate) fn add_to(&self, stats: &mut RouteStats) {
        stats.count = stats.count.saturating_add(self.n);
        for class in 0..apm::STATUS_CLASSES {
            stats.status[class] = stats.status[class].saturating_add(self.status[class]);
        }
        stats.sum_ns = stats.sum_ns.saturating_add(self.sum_ns);
        for (bucket, n) in &self.hist {
            if let Some(slot) = stats.hist.get_mut(*bucket as usize) {
                *slot = slot.saturating_add(*n);
            }
        }
        self.mem.add_to(&mut stats.mem);
    }
}

/// What a service did over one stretch of time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Window {
    /// End of the window, Unix milliseconds. By the collector's clock once
    /// collected (`collect` stamps it when the snapshot arrives): the store
    /// names its day files and prunes them, and the page picks its ranges, by
    /// that clock, and a service whose clock is days off would otherwise write
    /// into files retention deletes at once, or never (review).
    pub t: u64,
    /// Length of the window, milliseconds.
    pub span_ms: u64,
    /// The run of the service the window belongs to.
    pub epoch: u64,
    /// The service it was collected from (its address); empty for a script's
    /// run. A store several collectors write to tells their windows apart by it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// Routes that completed a request, or did database or network work.
    pub routes: Vec<RouteWindow>,
    /// The service's CPU and memory over the window, when it measured them
    /// (never for a script's run).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub res: Option<WindowRes>,
    /// The errors failed transactions ended with over the window, grouped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<WindowError>,
    /// The build the source was running, as its snapshots carry it (`<unix seconds> <label>`),
    /// when it said: what the page groups an application's figures by version with.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub build: String,
}

/// How many failed transactions of one transaction ended with one error over a window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WindowError {
    /// The transaction's name, as its row in `routes`.
    #[serde(rename = "r")]
    pub route: String,
    /// The exception's class.
    #[serde(rename = "c")]
    pub class: String,
    /// Where it was thrown, `file:line`.
    pub at: String,
    /// How many times.
    pub n: u64,
    /// The W3C trace id of its last occurrence; empty without one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub trace: String,
}

impl WindowError {
    /// Copies a snapshot's grouped error into the stored window format.
    fn from_stats(error: &apm::ErrorStats) -> Self {
        Self {
            route: error.route.clone(),
            class: error.class.clone(),
            at: error.location.clone(),
            n: error.count,
            trace: error.trace.clone(),
        }
    }
}

/// The errors of several windows added up, one row per transaction and error, most
/// frequent first.
pub(crate) fn merge_errors<'a>(windows: impl IntoIterator<Item = &'a Window>) -> Vec<WindowError> {
    let mut merged: Vec<WindowError> = Vec::new();
    for error in windows.into_iter().flat_map(|w| &w.errors) {
        match merged.iter_mut().find(|e| e.route == error.route && e.class == error.class && e.at == error.at) {
            Some(existing) => {
                existing.n = existing.n.saturating_add(error.n);
                // Windows come oldest first: the later one's trace is the last occurrence's.
                if !error.trace.is_empty() {
                    existing.trace = error.trace.clone();
                }
            }
            None => merged.push(error.clone()),
        }
    }
    merged.sort_by(|a, b| b.n.cmp(&a.n).then_with(|| a.route.cmp(&b.route)));
    merged
}

/// What the service's process tree used over one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WindowRes {
    /// CPU time spent during the window, nanoseconds.
    pub cpu_ns: u64,
    /// Private memory at the window's end, bytes.
    #[serde(rename = "mem")]
    pub memory_bytes: u64,
    /// Live processes at the window's end.
    #[serde(rename = "procs")]
    pub processes: u32,
    /// Cores the service may use, what its CPU is a share of; 0 when the
    /// service did not say.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cores: u32,
}

/// Whether a count is zero, for `skip_serializing_if`.
fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl WindowRes {
    /// The figures of a window between two measurements: the CPU spent between
    /// them (all of `current`'s when `previous` is from another run, or
    /// missing), memory and processes as `current` saw them.
    fn between(previous: Option<&apm::Resources>, current: &apm::Resources) -> Self {
        Self {
            cpu_ns: current.cpu_ns.saturating_sub(previous.map_or(0, |earlier| earlier.cpu_ns)),
            memory_bytes: current.memory_bytes,
            processes: current.processes,
            cores: current.cores,
        }
    }
}

impl Window {
    /// The window between two snapshots, or since the restart when `current`
    /// belongs to a run `previous` did not see.
    pub(crate) fn between(previous: &Snapshot, current: &Snapshot) -> Self {
        let diff = current.since(previous);
        let res = current.resources.as_ref().map(|now| {
            WindowRes::between(previous.resources.as_ref().filter(|_| diff.is_some()), now)
        });
        let same_run = diff.is_some();
        let (routes, span_ms) = match diff {
            Some(routes) => (routes, current.time_ms.saturating_sub(previous.time_ms)),
            None => (
                current.routes.clone(),
                current.time_ms.saturating_sub(current.epoch / 1_000_000),
            ),
        };
        let mut window = Self {
            build: current.build.clone().unwrap_or_default(),
            t: current.time_ms,
            span_ms,
            epoch: current.epoch,
            source: String::new(),
            routes: routes
                .iter()
                .filter(|route| has_activity(route))
                .map(RouteWindow::from_stats)
                .collect(),
            res,
            errors: current.errors_since(previous).iter().map(WindowError::from_stats).collect(),
        };
        window.attach_exits(same_run.then_some(previous), current);
        window
    }

    /// Everything a snapshot counted since its service started.
    pub(crate) fn since_start(snapshot: &Snapshot) -> Self {
        let mut window = Self {
            build: snapshot.build.clone().unwrap_or_default(),
            t: snapshot.time_ms,
            span_ms: snapshot.time_ms.saturating_sub(snapshot.epoch / 1_000_000),
            epoch: snapshot.epoch,
            source: String::new(),
            routes: snapshot
                .routes
                .iter()
                .filter(|route| has_activity(route))
                .map(RouteWindow::from_stats)
                .collect(),
            res: snapshot.resources.as_ref().map(|now| WindowRes::between(None, now)),
            errors: snapshot.errors.iter().map(WindowError::from_stats).collect(),
        };
        window.attach_exits(None, snapshot);
        window
    }

    /// Counts the runs `current` reports ended that `previous` did not (all of them
    /// after a restart) against their transactions' rows, by exit code. A snapshot
    /// repeats every run its process ended, so only the new ones belong here.
    fn attach_exits(&mut self, previous: Option<&Snapshot>, current: &Snapshot) {
        let mut seen: std::collections::HashMap<(&str, i32), usize> = std::collections::HashMap::new();
        for (route, code) in previous.map_or(&[][..], |p| p.exits.as_slice()) {
            *seen.entry((route.as_str(), *code)).or_default() += 1;
        }
        for (route, code) in &current.exits {
            if let Some(left) = seen.get_mut(&(route.as_str(), *code)).filter(|left| **left > 0) {
                *left -= 1;
                continue;
            }
            if let Some(row) = self.routes.iter_mut().find(|row| row.route == *route) {
                row.add_exit(*code, 1);
            }
        }
    }
}

/// Whether any counter of a route moved in a window: a completed request, any
/// part of one, or database or network work for one still in flight.
///
/// Every part matters, because the next window is measured from the snapshot
/// that held it, so a dropped row loses what it held for good:
/// - a request's queries are published just before its count, so a poll can
///   see them before it sees the request end (and work outside any transaction
///   is counted as it happens);
/// - a request's count is written last and read first, so a poll can hold its
///   status, time and latency bucket but not its count (review).
fn has_activity(route: &RouteStats) -> bool {
    route.count > 0
        || route.sum_ns > 0
        || route.status.iter().any(|value| *value > 0)
        || route.hist.iter().any(|value| *value > 0)
        || route.io.iter().any(|value| *value > 0)
        // Memory too: it is published before the count, and a window that dropped a
        // memory-only change lost that measurement for good (review).
        || route.mem.iter().any(|value| *value > 0)
}

/// Whether a window is a script's run rather than a service's stretch of time.
///
/// A service window has the epoch of the run of the service it came from
/// (never 0); a script's run carries 0.
pub(crate) fn is_run(window: &Window) -> bool {
    window.epoch == 0
}

/// The time a set of windows measured, for rates.
///
/// A service's windows tile the time the collector watched, so their spans
/// add up to it. A script's runs do not tile anything — nine runs of 160 ms
/// spread over an hour did not happen in 1.5 s — so runs count the time they
/// were spread over, from the first one's start to the last one's end.
///
/// Service windows count as the union of the time they cover, not the sum:
/// two services collected into one store over the same minute watched one
/// minute, not two, and summing halved every rate.
pub(crate) fn measured_ms(windows: &[&Window]) -> u64 {
    let mut spans: Vec<(u64, u64)> = windows
        .iter()
        .filter(|w| !is_run(w) && w.span_ms > 0)
        .map(|w| (w.t.saturating_sub(w.span_ms), w.t))
        .collect();
    if !spans.is_empty() {
        spans.sort_unstable();
        let mut total = 0;
        let (mut start, mut end) = spans[0];
        for (from, to) in spans.into_iter().skip(1) {
            if from > end {
                total += end - start;
                start = from;
                end = to;
            } else {
                end = end.max(to);
            }
        }
        return total + (end - start);
    }
    let start = windows.iter().map(|w| w.t.saturating_sub(w.span_ms)).min().unwrap_or(0);
    let end = windows.iter().map(|w| w.t).max().unwrap_or(0);
    end.saturating_sub(start)
}

/// Merges windows into one dense row per route, sorted by name.
pub(crate) fn merge_routes<'a>(windows: impl IntoIterator<Item = &'a Window>) -> Vec<RouteStats> {
    let mut merged: Vec<RouteStats> = Vec::new();
    for window in windows {
        for route in &window.routes {
            let index = match merged.iter().position(|row| row.name == route.route) {
                Some(index) => index,
                None => {
                    merged.push(RouteStats::empty(&route.route));
                    merged.len() - 1
                }
            };
            route.add_to(&mut merged[index]);
        }
    }
    merged.sort_by(|a, b| a.name.cmp(&b.name));
    merged
}

/// Database and network work per route over `windows`.
pub(crate) fn merge_io<'a>(windows: impl IntoIterator<Item = &'a Window>) -> std::collections::HashMap<String, Io> {
    let mut merged: std::collections::HashMap<String, Io> = std::collections::HashMap::new();
    for window in windows {
        for route in window.routes.iter().filter(|route| !route.io.is_empty()) {
            merged.entry(route.route.clone()).or_default().absorb(&route.io);
        }
    }
    merged
}

/// All routes of `rows` folded into one.
pub(crate) fn total_of(rows: &[RouteStats]) -> RouteStats {
    let mut total = RouteStats::empty("all routes");
    for row in rows {
        total.absorb(row);
    }
    total
}

/// Folds consecutive windows into a single record for the store.
///
/// Windows on both sides of a restart are added together — each already holds
/// only what its own run did, so the sum is what the service did over the
/// stretch — and the record carries the newest run's epoch.
pub(crate) fn coalesce(windows: &[Window]) -> Option<Window> {
    let last = windows.last()?;
    // Saturating, like every fold of numbers a damaged store line can hold (review).
    let span_ms = windows.iter().fold(0u64, |total, w| total.saturating_add(w.span_ms));
    let io = merge_io(windows.iter());
    let routes = merge_routes(windows.iter())
        .iter()
        .filter(|row| has_activity(row) || io.contains_key(&row.name))
        .map(|row| {
            let mut window = RouteWindow { io: io.get(&row.name).copied().unwrap_or_default(), ..RouteWindow::from_stats(row) };
            // The runs' exit codes add up, code by code.
            for exit in windows.iter().flat_map(|w| &w.routes).filter(|r| r.route == row.name).flat_map(|r| &r.exits) {
                window.add_exit(exit.0, exit.1);
            }
            window
        })
        .collect();
    // CPU adds up; memory and processes keep the stretch's highest, so a
    // stored record does not hide a peak its one-second windows saw.
    let res = windows.iter().filter_map(|w| w.res).reduce(|a, b| WindowRes {
        cpu_ns: a.cpu_ns.saturating_add(b.cpu_ns),
        memory_bytes: a.memory_bytes.max(b.memory_bytes),
        processes: a.processes.max(b.processes),
        cores: a.cores.max(b.cores),
    });
    Some(Window {
        // The newest window's build: a stretch that spans a deploy is the new build's.
        build: last.build.clone(),
        t: last.t,
        span_ms,
        epoch: last.epoch,
        source: last.source.clone(),
        routes,
        res,
        errors: merge_errors(windows.iter()),
    })
}

/// Two windows of one source and kind folded into one, as the store's read
/// cache keeps them: counters added, ending at the later one.
///
/// Service windows tile time, so their spans add up (as `coalesce` does). A
/// script's runs do not, so a folded run spans from the earlier start to the
/// later end, which is what `measured_ms` measures runs over.
pub(crate) fn merge_pair(a: &Window, b: &Window) -> Window {
    let (older, newer) = if a.t <= b.t { (a, b) } else { (b, a) };
    let mut merged = coalesce(&[older.clone(), newer.clone()]).expect("two windows coalesce");
    if is_run(newer) {
        let start = older.t.saturating_sub(older.span_ms).min(newer.t.saturating_sub(newer.span_ms));
        merged.span_ms = newer.t.saturating_sub(start);
    }
    merged
}

/// The service's CPU and memory over a set of windows.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub(crate) struct ResourceSummary {
    /// CPU as cores: CPU time over the time the windows cover, counted once.
    pub cores: f64,
    /// Memory: each service's figure in its newest window, added up.
    pub memory_bytes: u64,
    /// Processes, the same way.
    pub processes: u32,
    /// Cores the services may use, the same way: what `cores` is a share of.
    /// 0 when no service said.
    pub cores_available: u32,
}

impl ResourceSummary {
    /// CPU as a share of every core the services may use, percent; `None`
    /// when they did not say how many they have.
    pub(crate) fn cpu_pct(&self) -> Option<f64> {
        (self.cores_available > 0).then(|| self.cores * 100.0 / f64::from(self.cores_available))
    }
}

/// The CPU and memory the services of `windows` used, or `None` when none of
/// them measured any (a script's runs, a service built before it could).
pub(crate) fn resources_of(windows: &[&Window]) -> Option<ResourceSummary> {
    let measured: Vec<&Window> = windows.iter().copied().filter(|w| !is_run(w) && w.res.is_some()).collect();
    if measured.is_empty() {
        return None;
    }
    let cpu_ns = measured.iter().filter_map(|w| w.res).fold(0u64, |sum, res| sum.saturating_add(res.cpu_ns));
    let covered_ms = measured_ms(&measured);
    let mut newest: Vec<(&str, u64, WindowRes)> = Vec::new();
    for window in &measured {
        let res = window.res.expect("filtered on res");
        match newest.iter_mut().find(|(source, ..)| *source == window.source.as_str()) {
            Some(entry) if entry.1 < window.t => *entry = (window.source.as_str(), window.t, res),
            Some(_) => {}
            None => newest.push((window.source.as_str(), window.t, res)),
        }
    }
    Some(ResourceSummary {
        cores: if covered_ms == 0 { 0.0 } else { cpu_ns as f64 / (covered_ms as f64 * 1e6) },
        memory_bytes: newest.iter().map(|(.., res)| res.memory_bytes).sum(),
        processes: newest.iter().map(|(.., res)| res.processes).sum(),
        cores_available: newest.iter().map(|(.., res)| res.cores).sum(),
    })
}

/// Prefixes route names with their service when windows from more than one
/// service are shown together, so `GET /` of two services stays two rows.
pub(crate) fn label_sources(windows: &mut [Window]) {
    let mut sources: Vec<&str> = windows
        .iter()
        .filter(|w| !is_run(w) && !w.source.is_empty())
        .map(|w| w.source.as_str())
        .collect();
    sources.sort_unstable();
    sources.dedup();
    if sources.len() < 2 {
        return;
    }
    for window in windows.iter_mut().filter(|w| !is_run(w) && !w.source.is_empty()) {
        let source = window.source.clone();
        for route in &mut window.routes {
            route.route = format!("{source} · {}", route.route);
        }
        for error in &mut window.errors {
            error.route = format!("{source} · {}", error.route);
        }
    }
}
