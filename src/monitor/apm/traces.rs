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

use crate::call_graph::CallGraph;

/// Slowest captures kept per route.
const SLOWEST_PER_ROUTE: usize = 5;
/// Most recent failed (5xx) captures kept per route.
const FAILED_PER_ROUTE: usize = 5;
/// Captures kept in all, oldest dropped first past it.
const MAX_TRACES: usize = 200;

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
}

impl Trace {
    /// Whether the request failed: a 5xx, or a status outside HTTP's range.
    pub(crate) fn failed(&self) -> bool {
        self.status.is_some_and(|status| !(100..500).contains(&status))
    }
}

/// The captures kept.
#[derive(Default)]
pub(crate) struct TraceBook {
    next_id: u64,
    traces: Vec<Trace>,
}

impl TraceBook {
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
    /// failures must not push out the slowest request the route served (review
    /// G5).
    pub(crate) fn offer(&mut self, route: String, status: Option<u16>, duration_ns: u64, graph: CallGraph, t_ms: u64) -> bool {
        self.next_id += 1;
        let trace = Trace { id: self.next_id, t_ms, route, status, duration_ns, graph };
        let route = trace.route.clone();
        self.traces.push(trace);
        // Re-derive the route's keep set with the newcomer in it.
        let mut slow: Vec<(u64, u64)> = self
            .traces
            .iter()
            .filter(|t| t.route == route)
            .map(|t| (t.duration_ns, t.id))
            .collect();
        slow.sort_by(|a, b| b.cmp(a));
        let mut failures: Vec<(u64, u64)> = self
            .traces
            .iter()
            .filter(|t| t.route == route && t.failed())
            .map(|t| (t.t_ms, t.id))
            .collect();
        failures.sort_by(|a, b| b.cmp(a));
        let keep: Vec<u64> = slow
            .iter()
            .take(SLOWEST_PER_ROUTE)
            .chain(failures.iter().take(FAILED_PER_ROUTE))
            .map(|(_, id)| *id)
            .collect();
        let newcomer = self.next_id;
        self.traces.retain(|t| t.route != route || keep.contains(&t.id));
        if self.traces.len() > MAX_TRACES {
            let excess = self.traces.len() - MAX_TRACES;
            self.traces.drain(..excess);
        }
        let kept = self.traces.iter().any(|t| t.id == newcomer);
        kept
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
}
