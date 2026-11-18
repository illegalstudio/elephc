//! Purpose:
//! What the monitor page's server answers with: the page itself, the JSON the
//! Transactions view draws from (series, alerts, captures) and the Prometheus
//! `/metrics` text.
//!
//! Called from:
//! - `dashboard::handle`, one function per path.
//!
//! Key details:
//! - Each takes what it needs from the shared history under its lock and
//!   renders after releasing it, so a slow render never holds up the poll or
//!   the other requests (review).
//! - Ranges reaching past memory are loaded from the store outside the lock
//!   (`RangePlan`).

use elephc_probe::apm::{self, RouteStats};
use serde_json::json;

use super::dashboard::query_param;
use super::{merge_routes, summaries, summarize, total_of, Shared, Window};

/// Most points a chart is given; longer spans are grouped into wider steps.
const MAX_POINTS: u64 = 180;
/// Ranges the API accepts, seconds: from five minutes to the longest
/// retention anyone would browse in a chart.
const MAX_RANGE_SECS: u64 = 90 * 86_400;

/// The monitor page, rendered for this request: the profiles captured so far
/// (none until someone asks) and the Transactions view, fed live.
///
/// With `trace`, the Profile view shows that captured request's exact call
/// graph instead of the sampled captures.
///
/// What the page shows is copied under the history's lock and rendered after
/// it is released: rendering a large call graph under it held up the poll and
/// every other request for as long (review).
pub(crate) fn page_html(shared: &Shared, trace: Option<u64>) -> String {
    let history = shared.0.lock().unwrap_or_else(|e| e.into_inner());
    let mut data = json!({
        "mode": "live",
        "target": history.target,
        "capture": history.capture.is_some(),
        "captureEvery": history.capture_every,
    });
    let target = history.target.clone();
    if let Some(trace) = trace.and_then(|id| history.traces.get(id)) {
        data["trace"] = trace_summary(trace);
        let graph = trace.graph.clone();
        drop(history);
        // The service stays the title: the page shortens a title to its last
        // path segment, which would leave `{id}` of a route. The banner names
        // the request.
        let view = crate::call_graph::ApmView { data };
        return crate::call_graph::render_html_page(&[(0, &graph)], &target, false, 0, true, &[], Some(&view));
    }
    let profiles: Vec<(u128, crate::call_graph::CallGraph)> = history.profiles.iter().cloned().collect();
    drop(history);
    let frames: Vec<(u128, &crate::call_graph::CallGraph)> = profiles.iter().map(|(ts, graph)| (*ts, graph)).collect();
    let view = crate::call_graph::ApmView { data };
    crate::call_graph::render_html_page(&frames, &target, false, 0, false, &[], Some(&view))
}

/// What the page lists about one captured request.
fn trace_summary(trace: &super::traces::Trace) -> serde_json::Value {
    json!({
        "id": trace.id,
        "t": trace.t_ms,
        "route": trace.route,
        "status": trace.status,
        "failed": trace.failed(),
        "duration_ns": trace.duration_ns,
        "functions": trace.graph.nodes.len(),
        "queries": trace.graph.queries.iter().map(|(_, n)| n).sum::<u64>(),
    })
}

/// Every budget and where it stands.
pub(crate) fn alerts_json(shared: &Shared) -> serde_json::Value {
    let history = shared.0.lock().unwrap_or_else(|e| e.into_inner());
    json!({
        "alerts": history.alerts.alerts.iter().map(|alert| json!({
            "rule": alert.rule.spec,
            // The rule's parts, so the page lays them out in columns instead of
            // showing `p95:GET /products<=100` as one string.
            "metric": alert.rule.spec.split_once(':').map_or("", |(metric, _)| metric),
            "route": alert.rule.route,
            "op": alert.rule.op,
            "threshold": alert.rule.threshold,
            "firing": alert.firing,
            "value": alert.value,
            "unit": alert.rule.metric.unit(),
            "since": alert.since_ms,
        })).collect::<Vec<_>>(),
    })
}

/// The kept captures, slowest first.
pub(crate) fn traces_json(shared: &Shared) -> serde_json::Value {
    let history = shared.0.lock().unwrap_or_else(|e| e.into_inner());
    json!({
        "captureEvery": history.capture_every,
        "traces": history.traces.list().into_iter().map(trace_summary).collect::<Vec<_>>(),
    })
}
/// The chart data for `?range=<seconds>[&route=<label>]`.
pub(crate) fn series_json(shared: &Shared, query: &str) -> serde_json::Value {
    let range_secs = query_param(query, "range")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(900)
        .clamp(60, MAX_RANGE_SECS);
    let route = query_param(query, "route").filter(|route| !route.is_empty());
    let history = shared.0.lock().unwrap_or_else(|e| e.into_inner());
    let now = history.newest_ms().unwrap_or(0);
    let range_ms = range_secs * 1_000;
    let from = now.saturating_sub(range_ms);
    // Planned under the lock, read from disk after it: a week's range parses
    // seven day files, and the collector's poll must not wait on that.
    let plan = history.plan(from, now);
    let target = history.target.clone();
    let live = history.live;
    let error = history.last_error.clone();
    drop(history);
    let mut windows = plan.load();
    // A page over a store several services write to keeps their routes apart,
    // as the terminal and the exported page do: `GET /` of two services is two
    // rows, not one row adding both (review G2). A collector's page reads only
    // its own service, which this leaves as it is.
    super::label_sources(&mut windows);
    build_series(&windows, from, now, route.as_deref(), &target, live, error)
}

/// Groups windows into chart points and summarizes the span.
pub(crate) fn build_series(
    windows: &[Window],
    from: u64,
    to: u64,
    route: Option<&str>,
    target: &str,
    live: bool,
    error: Option<String>,
) -> serde_json::Value {
    let range_ms = to.saturating_sub(from).max(1);
    // Never finer than the coarsest window in the span: stored history comes at
    // the store's resolution, and steps narrower than a window would leave
    // every other step empty and draw the span as scattered dots.
    // A script's run is not a stretch of watched time, so only service windows
    // set the finest step.
    //
    // The TYPICAL window, not the longest: one long window — a stretch the
    // collector could not reach, folded into one — must not coarsen the chart
    // around it into a few points.
    let mut spans: Vec<u64> = windows.iter().filter(|w| !super::is_run(w)).map(|w| w.span_ms).collect();
    spans.sort_unstable();
    let coarsest_ms = spans.get(spans.len() / 2).copied().unwrap_or(0);
    let step_ms = range_ms.div_ceil(MAX_POINTS).max(coarsest_ms).div_ceil(1_000).max(1) * 1_000;
    let refs: Vec<&Window> = windows.iter().collect();
    let measured_ms = super::measured_ms(&refs);
    let (routes, total) = summaries(&refs, measured_ms);
    let buckets = range_ms.div_ceil(step_ms);
    let mut grouped: Vec<Option<(RouteStats, Vec<&Window>)>> = vec![None; buckets as usize];
    for window in windows {
        let index = ((window.t.saturating_sub(from).saturating_sub(1)) / step_ms).min(buckets - 1);
        let entry = grouped[index as usize].get_or_insert_with(|| (RouteStats::empty(""), Vec::new()));
        // Runs add no watched time: their step is their denominator.
        if !super::is_run(window) {
            entry.1.push(window);
        }
        let rows = merge_routes(std::iter::once(window));
        for row in rows.iter().filter(|row| route.is_none_or(|wanted| row.name == wanted)) {
            entry.0.absorb(row);
        }
    }
    let points: Vec<serde_json::Value> = grouped
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let t = from + (index as u64 + 1) * step_ms;
            // The time the point's service windows cover, counted once: two
            // services watched over the same second watched one second, as the
            // table counts it, so their requests add up to their rate (G3).
            let resources = entry.as_ref().and_then(|(_, services)| point_resources(services));
            let entry = entry.as_ref().map(|(row, services)| (row, if services.is_empty() { 0 } else { super::measured_ms(services) }));
            match entry {
                None => json!({ "t": t, "gap": true }),
                Some((row, span_ms)) => {
                    let summary = summarize(row, if span_ms == 0 { step_ms } else { span_ms }, row.sum_ns);
                    json!({
                        "t": t,
                        "n": row.count,
                        "rps": summary.rps,
                        "status": row.status,
                        "mean": row.mean_ns(),
                        "p50": row.percentile_ns(50.0),
                        "p95": row.percentile_ns(95.0),
                        "p99": row.percentile_ns(99.0),
                        "cpu": resources.map(|(cores, ..)| cores),
                        "cpus": resources.map(|(.., available)| available),
                        "mem": resources.map(|(_, memory, _)| memory),
                    })
                }
            }
        })
        .collect();
    let selected = match route {
        Some(wanted) => routes.iter().find(|row| row.route == wanted).cloned(),
        None => Some(total.clone()),
    };
    json!({
        "target": target,
        "live": live,
        "error": error,
        "now": to,
        "range_ms": range_ms,
        "step_ms": step_ms,
        "measured_ms": measured_ms,
        "resources": super::resources_of(&refs).map(|res| {
            let mut value = json!(res);
            value["cpu_pct"] = json!(res.cpu_pct());
            value
        }),
        "route": route,
        "selected": selected,
        "total": total,
        "routes": routes,
        "points": points,
    })
}

/// One chart point's CPU (cores over the time its service windows cover,
/// counted once), memory (each service's highest in the point, added up) and
/// the cores the services may use (added up the same way), or `None` when no
/// window in it measured them.
fn point_resources(services: &[&Window]) -> Option<(f64, u64, u32)> {
    let measured: Vec<&Window> = services.iter().copied().filter(|w| w.res.is_some()).collect();
    if measured.is_empty() {
        return None;
    }
    let cpu_ns: u64 = measured.iter().filter_map(|w| w.res).map(|res| res.cpu_ns).sum();
    let covered_ms = super::measured_ms(&measured);
    let mut peaks: Vec<(&str, u64, u32)> = Vec::new();
    for window in &measured {
        let (memory, available) = window.res.map_or((0, 0), |res| (res.memory_bytes, res.cores));
        match peaks.iter_mut().find(|(source, ..)| *source == window.source.as_str()) {
            Some(peak) => {
                peak.1 = peak.1.max(memory);
                peak.2 = peak.2.max(available);
            }
            None => peaks.push((window.source.as_str(), memory, available)),
        }
    }
    let cores = if covered_ms == 0 { 0.0 } else { cpu_ns as f64 / (covered_ms as f64 * 1e6) };
    Some((cores, peaks.iter().map(|(_, memory, _)| memory).sum(), peaks.iter().map(|(.., available)| available).sum()))
}

/// Latency boundaries exported to Prometheus, seconds.
const PROM_BOUNDS: [f64; 13] = [
    0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// The latest cumulative counters in the Prometheus text format.
///
/// The histogram's `le` buckets count the fine buckets whose upper edge is at
/// or below the boundary — never a request that might have been slower than
/// it. A boundary falling inside a fine bucket therefore undercounts by at most
/// that bucket, which is the conservative direction for a latency objective.
///
/// A collector receiving several programs (`--listen`) exports each one's
/// counters as a series of its own, labelled `source`: one shared series
/// would switch between unrelated counters at every send (review).
pub(crate) fn metrics_text(shared: &Shared) -> String {
    let history = shared.0.lock().unwrap_or_else(|e| e.into_inner());
    // A listening collector exports its programs, and only them: once the
    // last one is forgotten there is nothing, not its last counters again
    // without a label (review).
    let sources: Vec<(Option<String>, apm::Snapshot)> = if history.pushed {
        history.latest_by_source.iter().map(|(source, snapshot)| (Some(source.clone()), snapshot.clone())).collect()
    } else {
        history.latest.clone().map(|snapshot| (None, snapshot)).into_iter().collect()
    };
    drop(history);
    if sources.is_empty() {
        return "# no snapshot yet\n".to_string();
    }
    // `source="…",` before a series' own labels, or nothing.
    let label = |source: &Option<String>| {
        source.as_ref().map_or(String::new(), |source| format!("source=\"{}\",", super::super::escape_label(source)))
    };
    // A series with no labels of its own: `{source="…"}`, or nothing.
    let only = |source: &Option<String>| {
        source.as_ref().map_or(String::new(), |source| format!("{{source=\"{}\"}}", super::super::escape_label(source)))
    };
    let rows_of = |snapshot: &apm::Snapshot| {
        let mut rows = snapshot.routes.clone();
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        rows
    };
    let mut out = String::new();
    out.push_str("# HELP elephc_apm_requests_total Requests completed, per route and status class.\n");
    out.push_str("# TYPE elephc_apm_requests_total counter\n");
    for (source, snapshot) in &sources {
        for row in &rows_of(snapshot) {
            let route = super::super::escape_label(&row.name);
            for (class, n) in row.status.iter().enumerate() {
                out.push_str(&format!(
                    "elephc_apm_requests_total{{{}route=\"{route}\",class=\"{}xx\"}} {n}\n",
                    label(source),
                    class + 1
                ));
            }
        }
    }
    out.push_str("# HELP elephc_apm_request_duration_seconds Wall time from request head to handler return.\n");
    out.push_str("# TYPE elephc_apm_request_duration_seconds histogram\n");
    for (source, snapshot) in &sources {
        let prefix = label(source);
        for row in &rows_of(snapshot) {
            let route = super::super::escape_label(&row.name);
            for bound in PROM_BOUNDS {
                let bound_ns = (bound * 1e9) as u64;
                let n: u64 = row
                    .hist
                    .iter()
                    .enumerate()
                    .filter(|(bucket, _)| apm::bucket_upper_ns(*bucket) <= bound_ns)
                    .map(|(_, n)| n)
                    .sum();
                out.push_str(&format!(
                    "elephc_apm_request_duration_seconds_bucket{{{prefix}route=\"{route}\",le=\"{bound}\"}} {n}\n"
                ));
            }
            let all: u64 = row.hist.iter().sum();
            out.push_str(&format!(
                "elephc_apm_request_duration_seconds_bucket{{{prefix}route=\"{route}\",le=\"+Inf\"}} {all}\n"
            ));
            out.push_str(&format!(
                "elephc_apm_request_duration_seconds_sum{{{prefix}route=\"{route}\"}} {}\n",
                row.sum_ns as f64 / 1e9
            ));
            out.push_str(&format!(
                "elephc_apm_request_duration_seconds_count{{{prefix}route=\"{route}\"}} {}\n",
                row.count
            ));
        }
    }
    let with_resources: Vec<(&Option<String>, &apm::Resources)> = sources
        .iter()
        .filter_map(|(source, snapshot)| snapshot.resources.as_ref().map(|res| (source, res)))
        .collect();
    if !with_resources.is_empty() {
        out.push_str("# HELP elephc_apm_cpu_seconds_total CPU time of the service's processes, user plus system.\n");
        out.push_str("# TYPE elephc_apm_cpu_seconds_total counter\n");
        for (source, res) in &with_resources {
            out.push_str(&format!("elephc_apm_cpu_seconds_total{} {}\n", only(source), res.cpu_ns as f64 / 1e9));
        }
        out.push_str("# HELP elephc_apm_memory_bytes Private memory of the service's live processes.\n");
        out.push_str("# TYPE elephc_apm_memory_bytes gauge\n");
        for (source, res) in &with_resources {
            out.push_str(&format!("elephc_apm_memory_bytes{} {}\n", only(source), res.memory_bytes));
        }
        if with_resources.iter().any(|(_, res)| res.cores > 0) {
            out.push_str("# HELP elephc_apm_cpu_cores Cores the service may use: what its CPU time is a share of.\n");
            out.push_str("# TYPE elephc_apm_cpu_cores gauge\n");
            for (source, res) in with_resources.iter().filter(|(_, res)| res.cores > 0) {
                out.push_str(&format!("elephc_apm_cpu_cores{} {}\n", only(source), res.cores));
            }
        }
        out.push_str("# HELP elephc_apm_processes Live processes of the service, its master included.\n");
        out.push_str("# TYPE elephc_apm_processes gauge\n");
        for (source, res) in &with_resources {
            out.push_str(&format!("elephc_apm_processes{} {}\n", only(source), res.processes));
        }
    }
    out.push_str("# HELP elephc_apm_service_start_seconds When the service's counters started.\n");
    out.push_str("# TYPE elephc_apm_service_start_seconds gauge\n");
    for (source, snapshot) in &sources {
        out.push_str(&format!("elephc_apm_service_start_seconds{} {}\n", only(source), snapshot.epoch as f64 / 1e9));
    }
    let count: u64 = sources.iter().map(|(_, snapshot)| total_of(&rows_of(snapshot)).count).sum();
    out.push_str(&format!("# requests since start: {count}\n"));
    out
}
