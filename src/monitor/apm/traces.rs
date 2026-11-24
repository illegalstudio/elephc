//! Purpose:
//! Exact profiles of the requests that matter, captured automatically while
//! `monitor --apm` collects (`--capture-every <seconds>`): the collector asks
//! the service for the exact slice of one request at a time and keeps the
//! slowest of each route and the ones that failed.
//!
//! Called from:
//! - `apm::collect`, which runs the capture loop on its own thread.
//! - The monitor page (`/api/traces`, `/?trace=<id>`) and the `--live` view.
//!
//! Key details:
//! - Each capture instruments exactly one request of the service — the next
//!   one to complete after the ask — so the cost to the service is one exact
//!   request per period, whatever its traffic.
//! - Requests are captured as they come, not chosen in advance: a request's
//!   duration and status are known only once it ends. Keeping the slowest few
//!   per route and every recent failure out of a steady stream is what turns
//!   that into "the slow and failed requests", as far as the stream reached.
//! - They are kept PER ROUTE AND PER HOUR: the slowest of a whole week would
//!   all come from one bad night, and a page looking at any other period
//!   would find no example. An hour that is over goes to the store, when there
//!   is one (`unwritten_hours`), where the page finds it by date.

use crate::call_graph::CallGraph;

/// An hour, in milliseconds: the period each route's captures are kept per.
const HOUR_MS: u64 = 3_600_000;

/// Slowest captures kept per route and hour.
const SLOWEST_PER_ROUTE: usize = 5;
/// Most recent failed (5xx) captures kept per route and hour.
const FAILED_PER_ROUTE: usize = 5;
/// Captures kept in all, oldest dropped first past it.
const MAX_TRACES: usize = 200;
/// Captures kept while not saved yet, at most: a store's hour of keep sets, with room.
const MAX_UNSAVED: usize = 1_000;

/// One captured request with its exact profile.
pub(crate) struct Trace {
    /// Identifier for the page's links, unique within this collector.
    pub id: u64,
    /// When it was captured, Unix milliseconds.
    pub t_ms: u64,
    /// The route the request was served under.
    pub route: String,
    /// The HTTP status, when the service recorded it.
    pub status: Option<u16>,
    /// The request's measured wall time, nanoseconds.
    pub duration_ns: u64,
    /// The exact call graph of the request.
    pub graph: CallGraph,
    /// Whether it went to the store already.
    pub written: bool,
}

impl Trace {
    /// Whether the request failed: a 5xx, or a status outside HTTP's range.
    pub(crate) fn failed(&self) -> bool {
        self.status.is_some_and(|status| !(100..500).contains(&status))
    }

    /// The hour it was captured in.
    fn hour(&self) -> u64 {
        self.t_ms / HOUR_MS
    }

    /// What the store keeps about it.
    pub(crate) fn record(&self, source: &str) -> super::captures::CaptureRecord {
        super::captures::CaptureRecord {
            source: source.to_string(),
            id: self.id,
            t: self.t_ms,
            kind: "trace".to_string(),
            route: self.route.clone(),
            status: self.status,
            duration_ns: self.duration_ns,
            functions: self.graph.nodes.len(),
            queries: self.graph.queries.iter().map(|(_, n)| n).sum(),
        }
    }
}

/// The captures kept.
#[derive(Default)]
pub(crate) struct TraceBook {
    traces: Vec<Trace>,
    /// Whether a store keeps the captures: the cap then never drops one it has not
    /// saved yet, which the hour's writer would otherwise never see (review).
    stored: bool,
}

impl TraceBook {
    /// A book whose captures a store keeps (`true`) or only memory.
    pub(crate) fn stored(stored: bool) -> Self {
        Self { stored, ..Self::default() }
    }

    /// Builds a trace from an exact slice, or `None` when the slice holds no
    /// profile (an empty wait, or a note from the service).
    pub(crate) fn trace_from_slice(text: &str) -> Option<(String, Option<u16>, u64, CallGraph)> {
        let graph = super::super::parse_instrument_dump(text);
        if graph.nodes.is_empty() {
            return None;
        }
        let duration_ns = graph.nodes.iter().map(|node| node.inclusive).max().unwrap_or(0);
        let (route, status) = match &graph.trace {
            Some(trace) if !trace.route.is_empty() => (trace.route.clone(), trace.status),
            Some(trace) => ("<untagged>".to_string(), trace.status),
            None => ("<untagged>".to_string(), None),
        };
        Some((route, status, duration_ns, graph))
    }

    /// Offers a capture; returns whether it was kept.
    ///
    /// Kept when it is one of the `SLOWEST_PER_ROUTE` slowest of its route,
    /// failed or not, or one of its route's `FAILED_PER_ROUTE` most recent
    /// failures. Anything the newcomer pushes out of its route's set is
    /// dropped. A slow failure ranks among the slowest too: newer, faster
    /// failures must not push out the slowest request the route served.
    pub(crate) fn offer(&mut self, route: String, status: Option<u16>, duration_ns: u64, graph: CallGraph, t_ms: u64) -> bool {
        // An id the store can keep: it names the capture's time, so its day.
        let id = super::captures::next_capture_id(t_ms);
        let trace = Trace { id, t_ms, route, status, duration_ns, graph, written: false };
        let route = trace.route.clone();
        let hour = trace.hour();
        self.traces.push(trace);
        // Re-derive the route's keep set with the newcomer in it.
        let mut slow: Vec<(u64, u64)> = self
            .traces
            .iter()
            .filter(|t| t.route == route && t.hour() == hour)
            .map(|t| (t.duration_ns, t.id))
            .collect();
        slow.sort_by(|a, b| b.cmp(a));
        let mut failures: Vec<(u64, u64)> = self
            .traces
            .iter()
            .filter(|t| t.route == route && t.hour() == hour && t.failed())
            .map(|t| (t.t_ms, t.id))
            .collect();
        failures.sort_by(|a, b| b.cmp(a));
        let keep: Vec<u64> = slow
            .iter()
            .take(SLOWEST_PER_ROUTE)
            .chain(failures.iter().take(FAILED_PER_ROUTE))
            .map(|(_, id)| *id)
            .collect();
        let newcomer = id;
        self.traces.retain(|t| t.route != route || t.hour() != hour || keep.contains(&t.id));
        // Past the cap the oldest go, but with a store those it saved first: an hour's
        // captures are saved once it is over (review). The unsaved are capped too, at
        // `MAX_UNSAVED`: a store that keeps failing (a full disk) must not grow memory
        // without limit, so past it the oldest unsaved go as well (review).
        let mut excess = self.traces.len().saturating_sub(MAX_TRACES);
        let stored = self.stored;
        self.traces.retain(|t| {
            if excess > 0 && (t.written || !stored) {
                excess -= 1;
                return false;
            }
            true
        });
        let mut unsaved_excess = self.traces.iter().filter(|t| !t.written).count().saturating_sub(MAX_UNSAVED);
        self.traces.retain(|t| {
            if unsaved_excess > 0 && !t.written {
                unsaved_excess -= 1;
                return false;
            }
            true
        });
        let kept = self.traces.iter().any(|t| t.id == newcomer);
        kept
    }

    /// Marks captures the store failed to save as unsaved again, so the next poll, or the
    /// stop, retries them while their graphs are in memory (review).
    pub(crate) fn unsaved(&mut self, ids: &[u64]) {
        for trace in self.traces.iter_mut().filter(|t| ids.contains(&t.id)) {
            trace.written = false;
        }
    }

    /// The kept captures, slowest first.
    pub(crate) fn list(&self) -> Vec<&Trace> {
        let mut out: Vec<&Trace> = self.traces.iter().collect();
        out.sort_by(|a, b| b.duration_ns.cmp(&a.duration_ns).then(a.id.cmp(&b.id)));
        out
    }

    /// One kept capture by id.
    pub(crate) fn get(&self, id: u64) -> Option<&Trace> {
        self.traces.iter().find(|t| t.id == id)
    }

    /// The kept captures of the hours that ended before `now_ms` and are not in the store
    /// yet, marked as written: an hour's keep set is final once the hour is over. With
    /// `u64::MAX`, every capture not in the store, the current hour's included (a stop).
    pub(crate) fn unwritten_hours(&mut self, now_ms: u64) -> Vec<&Trace> {
        let current = now_ms / HOUR_MS;
        let mut due = Vec::new();
        for trace in &mut self.traces {
            if !trace.written && (now_ms == u64::MAX || trace.hour() < current) {
                trace.written = true;
                due.push(&*trace);
            }
        }
        due
    }
}
