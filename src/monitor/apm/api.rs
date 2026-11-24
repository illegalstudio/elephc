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

/// The span a request asks for, `[from, to]` in Unix milliseconds: `from` and `to` when it
/// names dates (a period picked on the page), else the last `range` seconds up to `now`,
/// the newest window. An absolute span is kept within `MAX_RANGE_SECS`, and a reversed one
/// put in order.
pub(crate) fn requested_span(query: &str, now: u64) -> (u64, u64) {
    let param = |name: &str| query_param(query, name).and_then(|value| value.parse::<u64>().ok());
    if let (Some(from), Some(to)) = (param("from"), param("to")) {
        let (from, to) = if from <= to { (from, to) } else { (to, from) };
        return (from.max(to.saturating_sub(MAX_RANGE_SECS * 1_000)), to.max(from + 1));
    }
    let range_secs = param("range").unwrap_or(900).clamp(60, MAX_RANGE_SECS);
    (now.saturating_sub(range_secs * 1_000), now)
}

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
    if let Some(id) = trace {
        // In memory, or in the store once its hour went there: a capture found
        // by date can be older than anything the collector still holds.
        // Exact unless it is a sampled profile captured from the page: its weights are samples,
        // which the exact views would label as measured time (review).
        let in_memory = history.traces.get(id).map(|trace| (trace_summary(trace), trace.graph.clone(), true));
        let store = history.store.clone();
        let own = history.source_filter();
        drop(history);
        let found = in_memory.or_else(|| {
            store
                .and_then(|store| store.capture(id))
                .filter(|(record, _)| belongs(&own, record))
                .map(|(record, graph)| (record_summary(&record), graph, record.kind != "profile"))
        });
        if let Some((summary, graph, exact)) = found {
            data["trace"] = summary;
            // The service stays the title: the page shortens a title to its last
            // path segment, which would leave `{id}` of a route. The banner names
            // the request.
            let view = crate::call_graph::ApmView { data };
            return crate::call_graph::render_html_page(&[(0, &graph)], &target, false, 0, exact, &[], Some(&view));
        }
        return page_html(shared, None);
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

/// The captures of the requested span (`requested_span`), slowest first: those still in
/// memory and those the store keeps, each once.
pub(crate) fn traces_json(shared: &Shared, query: &str) -> serde_json::Value {
    let history = shared.0.lock().unwrap_or_else(|e| e.into_inner());
    let (from, to) = requested_span(query, history.newest_ms().unwrap_or_else(super::store::now_ms));
    let mut listed: Vec<serde_json::Value> = Vec::new();
    let mut ids = std::collections::HashSet::new();
    for trace in history.traces.list() {
        if trace.t_ms >= from && trace.t_ms <= to {
            ids.insert(trace.id);
            listed.push(trace_summary(trace));
        }
    }
    let capture_every = history.capture_every;
    let store = history.store.clone();
    let own = history.source_filter();
    // Read from disk after the lock is let go, as a range's windows are.
    drop(history);
    if let Some(store) = store {
        for record in store.captures_in(from, to).into_iter().filter(|record| !ids.contains(&record.id) && belongs(&own, record)) {
            listed.push(record_summary(&record));
        }
    }
    listed.sort_by(|a, b| b["duration_ns"].as_u64().cmp(&a["duration_ns"].as_u64()).then(a["id"].as_u64().cmp(&b["id"].as_u64())));
    json!({ "captureEvery": capture_every, "from": from, "to": to, "traces": listed })
}

/// Whether a stored capture belongs on this page: its service's, as its windows are. A
/// script's run belongs on every page, as its runs do.
fn belongs(own: &super::history::SourceFilter, record: &super::captures::CaptureRecord) -> bool {
    record.kind == "run" || own.keeps(&record.source)
}

/// What the page lists about one stored capture, as `trace_summary` does for one in memory.
fn record_summary(record: &super::captures::CaptureRecord) -> serde_json::Value {
    json!({
        "id": record.id,
        "t": record.t,
        "kind": record.kind,
        "route": record.route,
        "status": record.status,
        "failed": record.status.is_some_and(|status| !(100..500).contains(&status)),
        "duration_ns": record.duration_ns,
        "functions": record.functions,
        "queries": record.queries,
    })
}
/// The chart data for `?range=<seconds>[&route=<label>]`.
pub(crate) fn series_json(shared: &Shared, query: &str) -> serde_json::Value {
    let route = query_param(query, "route").filter(|route| !route.is_empty());
    let history = shared.0.lock().unwrap_or_else(|e| e.into_inner());
    let (from, now) = requested_span(query, history.newest_ms().unwrap_or(0));
    // Planned under the lock, read from disk after it: a week's range parses
    // seven day files, and the collector's poll must not wait on that.
    let plan = history.plan(from, now);
    let target = history.target.clone();
    let live = history.live;
    let error = history.last_error.clone();
    // A live collector knows every deploy it and the store saw; a page replaying a store
    // another collector writes reads them anew.
    let known = history.deploys.between(from, now);
    let replayed = history.store.clone().filter(|_| !live);
    drop(history);
    let deploys = match replayed {
        Some(store) => super::deploys::Deploys::from_list(super::deploys::read(store.dir())).between(from, now),
        None => known,
    };
    let mut windows = plan.load();
    // A page over a store several services write to keeps their routes apart,
    // as the terminal and the exported page do: `GET /` of two services is two
    // rows, not one row adding both. A collector's page reads only
    // its own service, which this leaves as it is.
    super::label_sources(&mut windows);
    // A script the Scripts section lists once, whichever instance ran it: picked, its
    // `<instance> · CLI ...` rows answer to its name, so the charts, tiles and errors cover
    // every run of it (review).
    if let Some(wanted) = route.as_deref().filter(|wanted| wanted.starts_with(super::runs::CLI_PREFIX)) {
        unlabel_script(&mut windows, wanted);
    }
    let mut series = build_series(&windows, from, now, route.as_deref(), &target, live, error);
    // A recent range ends at the newest window, which stops moving once every program
    // stops sending: an instance's state is then read against the clock (review).
    if live && query_param(query, "from").is_none() {
        series["apps"] = apps_json(&windows, from, now, super::store::now_ms().max(now));
    }
    series["deploys"] = deploys
        .iter()
        .map(|deploy| {
            serde_json::json!({
                "t": deploy.t,
                "service": deploy.service,
                "built": deploy.built_at(),
                "label": deploy.label(),
            })
        })
        .collect();
    series
}

/// The most errors listed for a span; the rest are counted in the total only.
const ERRORS_LISTED: usize = 50;

/// The errors of a span, most frequent first, each with when it last happened: of one
/// transaction when `route` names one.
pub(crate) fn errors_json(windows: &[Window], route: Option<&str>) -> serde_json::Value {
    let kept: Vec<&Window> = windows.iter().filter(|w| !w.errors.is_empty()).collect();
    let mut errors = super::merge_errors(kept.iter().copied());
    if let Some(route) = route {
        errors.retain(|error| error.route == route);
    }
    let total: u64 = errors.iter().map(|error| error.n).sum();
    let listed: Vec<serde_json::Value> = errors
        .iter()
        .take(ERRORS_LISTED)
        .map(|error| {
            let last = kept
                .iter()
                .filter(|w| w.errors.iter().any(|e| e.route == error.route && e.class == error.class && e.at == error.at))
                .map(|w| w.t)
                .max()
                .unwrap_or(0);
            json!({ "route": error.route, "class": error.class, "at": error.at, "n": error.n, "last": last, "trace": error.trace })
        })
        .collect();
    json!({ "total": total, "groups": errors.len(), "list": listed })
}

/// Names every `<source> · <script>` row and error of `windows` as `script` alone.
fn unlabel_script(windows: &mut [Window], script: &str) {
    let suffix = format!(" · {script}");
    for window in windows {
        for row in window.routes.iter_mut().filter(|row| row.route.ends_with(&suffix)) {
            row.route = script.to_string();
        }
        for error in window.errors.iter_mut().filter(|error| error.route.ends_with(&suffix)) {
            error.route = script.to_string();
        }
    }
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
            // table counts it, so their requests add up to their rate.
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
        "runs": runs_json(windows),
        "apps": apps_json(windows, from, to, to),
        // Here, not only in `series_json`: the exported page builds its views with this
        // function too, and lost its Errors section without them (review).
        "errors": errors_json(windows, route),
    })
}

/// A pushing program silent this long counts as silent: a send is due every five seconds.
const SILENT_AFTER_MS: u64 = 15_000;
/// And this long, as gone: the collector forgets it, its history stays.
const GONE_AFTER_MS: u64 = 600_000;

/// The programs that send to a listening collector, for the page's Apps section: each
/// instance (one process of one program on one host) with its build, its last send and
/// whether it is sending, silent or gone at `to`; and each program's figures per build,
/// so a new version that fails more than the one before shows. `None` when the span holds
/// no pushing program.
fn apps_json(windows: &[Window], from: u64, to: u64, clock: u64) -> serde_json::Value {
    let pushed: Vec<&Window> = windows.iter().filter(|w| super::collector::is_pushed_source(&w.source)).collect();
    if pushed.is_empty() {
        return serde_json::Value::Null;
    }
    // A version is its whole build identity, `<unix seconds> <label>`: two builds without a
    // label (no git, no `ELEPHC_BUILD_ID`) differ by when they were built, and must not
    // share a row (review). The label and the build time are what the page shows.
    let label = |build: &str| -> (String, u64) {
        let (secs, label) = build.split_once(' ').unwrap_or((build, ""));
        (label.trim().to_string(), secs.trim().parse().unwrap_or(0))
    };
    let mut by_source: std::collections::BTreeMap<&str, Vec<&Window>> = std::collections::BTreeMap::new();
    for window in &pushed {
        by_source.entry(window.source.as_str()).or_default().push(window);
    }
    let mut instances = Vec::new();
    // Each instance's current build: the version it counts under in the versions table.
    let mut current: std::collections::BTreeMap<(String, String), usize> = std::collections::BTreeMap::new();
    for (source, list) in &by_source {
        let newest = list.iter().max_by_key(|w| w.t).expect("a source has windows");
        let program = super::deploys::service_of(source).to_string();
        let build = newest.build.clone();
        let (_, total) = summaries(list, super::measured_ms(list).max(1));
        let quiet = clock.saturating_sub(newest.t);
        let state = if quiet >= GONE_AFTER_MS { "gone" } else if quiet >= SILENT_AFTER_MS { "silent" } else { "sending" };
        *current.entry((program.clone(), build.clone())).or_default() += 1;
        instances.push(json!({
            "source": source, "program": program, "build": label(&build).0, "id": build, "state": state, "last": newest.t,
            "n": total.n, "errors": total.server_error_pct, "mem": total.mem_mean_bytes,
        }));
    }
    let mut by_version: std::collections::BTreeMap<(String, String), Vec<&Window>> = std::collections::BTreeMap::new();
    for window in &pushed {
        by_version.entry((super::deploys::service_of(&window.source).to_string(), window.build.clone())).or_default().push(window);
    }
    let versions: Vec<serde_json::Value> = by_version
        .iter()
        .map(|((program, build), list)| {
            let (_, total) = summaries(list, super::measured_ms(list).max(1));
            let first = list.iter().map(|w| w.t.saturating_sub(w.span_ms)).min().unwrap_or(0);
            // Calls and failed calls per slice of the span, for the chart of each
            // version's error rate over time: `[[calls, failed], ...]`.
            let span = to.saturating_sub(from).max(1);
            let mut slices = vec![[0u64; 2]; APP_SLICES];
            for window in list {
                let at = (window.t.saturating_sub(from).min(span - 1) as u128 * APP_SLICES as u128 / span as u128) as usize;
                for route in &window.routes {
                    slices[at][0] = slices[at][0].saturating_add(route.n);
                    slices[at][1] = slices[at][1].saturating_add(route.status[4]);
                }
            }
            json!({
                "program": program, "id": build, "build": label(build).0, "built": label(build).1, "first": first,
                "instances": current.get(&(program.clone(), build.clone())).copied().unwrap_or(0),
                "n": total.n, "rps": total.rps, "errors": total.server_error_pct, "p95": total.p95_ns, "mem": total.mem_mean_bytes,
                "slices": slices,
            })
        })
        .collect();
    json!({ "instances": instances, "versions": versions })
}

/// Slices of the span the Apps chart draws each version's error rate over.
const APP_SLICES: usize = 60;

/// The most runs listed per script; older ones still count in its row.
const RUNS_LISTED: usize = 240;

/// Each script's runs in the span, oldest first, for the page's Scripts section: when
/// one ended, how long it took, whether it failed and its statements. A stored window
/// can fold a minute of runs into one record; it then reads as that minute's mean run
/// with its count, as the terminal's trend reads it.
fn runs_json(windows: &[Window]) -> serde_json::Value {
    let mut by_script: std::collections::BTreeMap<&str, Vec<serde_json::Value>> = std::collections::BTreeMap::new();
    // Every run of each script, counted before the list is cut to what is drawn: the
    // row's runs, failures and p95 cover them all, the p95 from their durations.
    let mut totals: std::collections::BTreeMap<&str, apm::RouteStats> = std::collections::BTreeMap::new();
    let mut ordered: Vec<&Window> = windows.iter().collect();
    ordered.sort_by_key(|w| w.t);
    for window in ordered {
        // A run a program sent reads `<instance> · CLI <script>` once several sources are
        // shown: its script is the same whichever instance ran it.
        let prefix = format!("{} · ", window.source);
        for row in &window.routes {
            let name = row.route.strip_prefix(prefix.as_str()).unwrap_or(&row.route);
            if row.n == 0 || !name.starts_with(super::runs::CLI_PREFIX) {
                continue;
            }
            row.add_to(totals.entry(name).or_insert_with(|| apm::RouteStats::empty(name)));
            by_script.entry(name).or_default().push(json!({
                "t": window.t,
                "n": row.n,
                "mean": row.sum_ns / row.n,
                "failed": row.status[4],
                // The exact codes the failed runs exited with, `[[code, runs], ...]`.
                "exits": row.exits.iter().filter(|(code, _)| *code != 0).map(|(code, runs)| json!([code, runs])).collect::<Vec<_>>(),
                "q": row.io.db_ops,
            }));
        }
    }
    let scripts: Vec<serde_json::Value> = by_script
        .into_iter()
        .map(|(name, mut runs)| {
            let total = runs.len();
            if total > RUNS_LISTED {
                runs.drain(..total - RUNS_LISTED);
            }
            let all = &totals[name];
            json!({
                "name": name,
                "points": total,
                "runs": runs,
                "total": all.count,
                "failed": all.status[4],
                "p95": summarize(all, 0, all.sum_ns).p95_ns,
            })
        })
        .collect();
    json!(scripts)
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
