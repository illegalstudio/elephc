//! Purpose:
//! Renders an aggregated PHP-level call graph — the Blackfire-style view where
//! each function is one node with inclusive/exclusive cost and its callers/callees
//! as edges — to Graphviz DOT and to a self-contained interactive HTML page.
//!
//! Called from:
//! - `crate::monitor` when `--dot` / `--html` export a capture.
//!
//! Key details:
//! - The graph is built from folded sample stacks, so cost is a SAMPLE SHARE
//!   (statistical), not instrumented time. Nodes carry the runtime-cause
//!   breakdown (heap allocation, Mixed cell boxing, …) elephc can attribute and
//!   an interpreter profiler cannot.
//! - The HTML is self-contained (inline CSS/JS, no network), theme-aware, with a
//!   hand-rolled layered (Sugiyama-lite) layout — no external graph library.
//! - `render_html_frames` embeds up to N capture frames; with more than one it
//!   lays out the UNION of every frame once (so nodes hold their position while
//!   you scrub) and shows a timeline navigator plus a diff-vs-previous mode. A
//!   live capture rewrites the file each window; the page auto-reloads when
//!   opened as a file, or updates in place (no flicker) when served over http.

use std::fmt::Write as _;

/// One function node of the call graph.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct GraphNode {
    /// PHP-level name (`{main}`, `Class::method`, `name (inlined)`).
    pub name: String,
    /// Samples in which this function appears anywhere on the stack.
    pub inclusive: u64,
    /// Samples in which this function is the leaf (its own work).
    pub exclusive: u64,
    /// Exact call count from `--counters`, when the capture carries one.
    pub call_count: Option<u64>,
    /// Exact inclusive/exclusive allocation counts (`--instrument`); 0 for
    /// sampled captures, which have no per-function allocation counts.
    pub alloc_inclusive: u64,
    pub alloc_exclusive: u64,
    /// Exact inclusive/exclusive I/O operation counts — DB queries (`--instrument`).
    #[serde(default)]
    pub io_inclusive: u64,
    #[serde(default)]
    pub io_exclusive: u64,
    /// Exact retained objects — allocated minus freed (`--instrument`). Signed:
    /// a function that releases more than it takes reports negative.
    #[serde(default)]
    pub retained_inclusive: i64,
    #[serde(default)]
    pub retained_exclusive: i64,
    /// Exact nanoseconds blocked in DB driver calls (`--instrument`). Self time
    /// minus this wait is an unclassified non-DB remainder, not OS CPU time.
    #[serde(default)]
    pub wait_inclusive: u64,
    #[serde(default)]
    pub wait_exclusive: u64,
    /// Exact inclusive/exclusive outgoing network operation counts.
    #[serde(default)]
    pub network_inclusive: u64,
    #[serde(default, alias = "network_ops")]
    pub network_exclusive: u64,
    /// Exact inclusive/exclusive nanoseconds blocked in outgoing network work.
    #[serde(default)]
    pub network_wait_inclusive: u64,
    #[serde(default, alias = "network_wait")]
    pub network_wait_exclusive: u64,
    /// Runtime-cause breakdown of this function's exclusive time, most first.
    pub causes: Vec<(String, u64)>,
}

/// A caller → callee relation, weighted by samples that took it.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct GraphEdge {
    pub from: usize,
    pub to: usize,
    pub weight: u64,
    /// Exact number of times this edge was taken (`--instrument`); `None` for
    /// sampled captures. Used to flag high fan-out (possible N+1) calls.
    #[serde(default)]
    pub count: Option<u64>,
}

/// The W3C Trace Context identity of one profiled request slice — what links
/// captures taken in different services into a single distributed trace.
#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
pub(crate) struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    /// Empty at the root of the trace.
    pub parent_span_id: String,
    /// Wall-clock microseconds at which this slice opened, for placing it on a
    /// shared axis. `None` for a capture taken before slices were timestamped —
    /// the view falls back to comparing durations.
    #[serde(default)]
    pub start_us: Option<u64>,
    /// The request's HTTP status, when the slice recorded one.
    #[serde(default)]
    pub status: Option<u16>,
    /// `METHOD /path` the request was routed to. Empty outside `--web`, where
    /// there is no route, and for captures taken before slices recorded one.
    #[serde(default)]
    pub route: String,
}

/// Per-line self cost over one PHP source file, recovered from sampled
/// addresses via the dSYM. Sampled, not exact — see `monitor::line_profile`.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct SourceLines {
    pub file: String,
    pub source: Vec<String>,
    /// (1-based line, samples charged to it). Empty for an exact capture, which
    /// has no per-line data — see `funcs`.
    pub hits: Vec<(u32, u64)>,
    pub total: u64,
    /// Declared functions with their measured cost, so the file can be read as
    /// a map even when no per-line sampling exists. Present for exact captures.
    #[serde(default)]
    pub funcs: Vec<SourceFunc>,
}

/// One declared function located in the source, with what the run measured.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct SourceFunc {
    pub name: String,
    /// 1-based inclusive line range of the declaration.
    pub start: u32,
    pub end: u32,
    pub self_pct: f64,
    pub incl_pct: f64,
    pub calls: u64,
}

/// One function that ran a query, as the exact profiler charged it: the first
/// frame above the database driver's own.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct QueryCaller {
    pub function: String,
    /// `path:line` of its declaration, or empty when unknown.
    pub location: String,
    pub executions: u64,
}

/// An aggregated call graph ready to render.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct CallGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub total: u64,
    /// Distinct DB queries and their execution counts (exact `--instrument`
    /// runs only), hottest first — the SQL panel / N+1 view. Empty otherwise.
    #[serde(default)]
    pub queries: Vec<(String, u64)>,
    /// For each query of `queries`, by its text: the functions that ran it, where
    /// they are declared and how many times, most first — what the Queries
    /// panel lists when a statement is opened.
    #[serde(default)]
    pub query_callers: std::collections::BTreeMap<String, Vec<QueryCaller>>,
    /// Per-line self cost over the PHP source, when a dSYM made it recoverable.
    #[serde(default)]
    pub lines: Option<SourceLines>,
    /// The distributed-trace identity of this slice (`--web --instrument`).
    #[serde(default)]
    pub trace: Option<TraceContext>,
}

impl CallGraph {
    /// This many samples as a percentage of the run, guarding the empty graph
    /// so an export of nothing renders zeroes rather than dividing by zero.
    fn share(&self, samples: u64) -> f64 {
        100.0 * samples as f64 / self.total.max(1) as f64
    }
}

/// Renders the call graph as Graphviz DOT. `dot -Tsvg`/`-Tpng` lays it out, and
/// `go tool pprof`-style tooling reads the same shape.
/// How one assertion came out.
#[derive(PartialEq, Clone, Copy)]
pub(crate) enum AssertStatus {
    Pass,
    Fail,
    /// The assertion could not be evaluated (bad syntax, unknown metric, or a
    /// function the run never reached). Reported separately from a failure
    /// because the code under test is not what is wrong.
    Error,
}

/// One evaluated assertion, ready to render in the report and the page.
pub(crate) struct AssertOutcome {
    pub spec: String,
    /// Free-text note from the budget file (`# ...` after the assertion).
    pub label: Option<String>,
    pub metric: String,
    /// Function the assertion is about, or `*` for the whole run.
    pub target: String,
    pub op: String,
    pub budget: f64,
    /// Measured value, absent when the assertion could not be evaluated.
    pub actual: Option<f64>,
    pub status: AssertStatus,
    /// Why it could not be evaluated, for `Error` outcomes.
    pub note: Option<String>,
}

/// One span of a distributed trace, ready to render: the slice's identity, its
/// service, and the cost figures the waterfall shows.
pub(crate) struct TraceSpan {
    pub service: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: String,
    pub total_ns: u64,
    pub functions: usize,
    pub queries: u64,
    pub wait_ns: u64,
    pub network_ops: u64,
    pub network_wait_ns: u64,
    /// Wall-clock microseconds at which the span opened, when the capture
    /// recorded it. Present spans are laid out on a real time axis; without it
    /// the chart can only compare durations.
    pub start_us: Option<u64>,
    /// The heaviest functions of this slice, `(name, inclusive %, self %)`.
    pub top: Vec<(String, f64, f64)>,
}

/// Renders correlated spans as a self-contained distributed-trace chart: one
/// row per span, nested under its caller, bar width proportional to the trace's
/// slowest span. Clicking a row reveals that service's hottest functions, so a
/// slow hop is diagnosable without opening its own profile.
///
/// A **waterfall** when every span in the trace carries a start timestamp: bars
/// are offset by when the slice opened, so sequential hops step rightwards and
/// concurrent ones overlap — the distinction the chart exists to show.
///
/// Falls back to duration-only bars, all flush left, when any span lacks a
/// timestamp (a capture taken before slices were dated). Mixing the two would be
/// worse than either: a span pinned at the left edge among placed ones reads as
/// "started first" rather than "unknown".
///
/// The axis is wall clock, so it inherits every distributed tracer's caveat:
/// two hosts' clocks can disagree, and a hop may appear to start slightly before
/// its parent.
pub(crate) fn render_trace_html(spans: &[TraceSpan], title: &str) -> String {
    use std::collections::BTreeMap;
    let mut by_trace: BTreeMap<&str, Vec<&TraceSpan>> = BTreeMap::new();
    for span in spans {
        by_trace.entry(span.trace_id.as_str()).or_default().push(span);
    }
    let mut body = String::new();
    for (trace_id, members) in &by_trace {
        let present: std::collections::HashSet<&str> =
            members.iter().map(|s| s.span_id.as_str()).collect();
        let scale = members.iter().map(|s| s.total_ns).max().unwrap_or(1).max(1);
        // Placed layout needs every member dated: one undated span would sit at the
        // left edge and read as the earliest, which is exactly the wrong answer.
        let window = members
            .iter()
            .map(|s| s.start_us)
            .collect::<Option<Vec<u64>>>()
            .and_then(|starts| {
                let t0 = *starts.iter().min()?;
                let end = members
                    .iter()
                    .map(|s| s.start_us.unwrap_or(t0) + s.total_ns / 1_000)
                    .max()?;
                (end > t0).then_some((t0, end - t0))
            });
        body.push_str(&format!(
            "<section class=\"trace\"><h2>trace <code>{}</code> <span class=\"n\">{} span{}</span></h2>",
            html_escape(trace_id),
            members.len(),
            if members.len() == 1 { "" } else { "s" }
        ));
        // Children by parent, then walk depth-first from the roots. A span whose
        // parent was not collected is a root, so a partial capture still renders.
        let mut children: BTreeMap<&str, Vec<&TraceSpan>> = BTreeMap::new();
        let mut roots: Vec<&TraceSpan> = Vec::new();
        for span in members {
            if span.parent_span_id.is_empty() || !present.contains(span.parent_span_id.as_str()) {
                roots.push(span);
            } else {
                children
                    .entry(span.parent_span_id.as_str())
                    .or_default()
                    .push(span);
            }
        }
        let mut stack: Vec<(&TraceSpan, usize)> =
            roots.into_iter().rev().map(|s| (s, 0)).collect();
        let mut guard = 0;
        while let Some((span, depth)) = stack.pop() {
            if guard > 4096 {
                break;
            }
            guard += 1;
            // Placed: offset and width both read off the trace's wall-clock window.
            // Unplaced: flush left, width proportional to the slowest span.
            let (offset, share) = match window {
                Some((t0, span_us)) => {
                    let start = span.start_us.unwrap_or(t0).saturating_sub(t0);
                    (
                        100.0 * start as f64 / span_us as f64,
                        100.0 * (span.total_ns / 1_000) as f64 / span_us as f64,
                    )
                }
                None => (0.0, 100.0 * span.total_ns as f64 / scale as f64),
            };
            let heat = heat_color((share / 100.0).clamp(0.0, 1.0));
            let ink = ink_for((share / 100.0).clamp(0.0, 1.0));
            let mut facts = Vec::new();
            facts.push(format!("{} fn", span.functions));
            if span.queries > 0 {
                facts.push(format!("{} queries", span.queries));
            }
            if span.wait_ns > 0 {
                facts.push(format!("{} waiting", fmt_ns_short(span.wait_ns)));
            }
            if span.network_ops > 0 {
                facts.push(format!("{} network", span.network_ops));
            }
            if span.network_wait_ns > 0 {
                facts.push(format!("{} network wait", fmt_ns_short(span.network_wait_ns)));
            }
            let top: String = span
                .top
                .iter()
                .map(|(name, incl, excl)| {
                    format!(
                        "<tr><td class=\"fn\">{}</td><td class=\"pc\">{:.1}%</td><td class=\"pc\">{:.1}%</td></tr>",
                        html_escape(name),
                        incl,
                        excl
                    )
                })
                .collect();
            body.push_str(&format!(
                "<details class=\"span\" style=\"--d:{depth}\"><summary>\
                 <span class=\"svc\">{svc}</span>\
                 <span class=\"bar\"><i style=\"margin-left:{off:.2}%;width:{w:.2}%;background:{heat};color:{ink}\">{dur}</i></span>\
                 <span class=\"facts\">{facts}</span></summary>\
                 <table class=\"top\"><thead><tr><th>function</th><th>incl</th><th>self</th></tr></thead>\
                 <tbody>{top}</tbody></table></details>",
                depth = depth,
                svc = html_escape(&span.service),
                off = offset.clamp(0.0, 98.0),
                w = share.max(2.0),
                heat = heat,
                ink = ink,
                dur = fmt_ns_short(span.total_ns),
                facts = html_escape(&facts.join(" · ")),
                top = top,
            ));
            if let Some(kids) = children.get(span.span_id.as_str()) {
                for kid in kids.iter().rev() {
                    stack.push((kid, depth + 1));
                }
            }
        }
        body.push_str("</section>");
    }
    if by_trace.is_empty() {
        body.push_str("<p class=\"none\">No correlated traces in these logs.</p>");
    }
    let head = format!(
        "<span class=\"meta\">{} trace{} · {} span{}</span>",
        by_trace.len(),
        if by_trace.len() == 1 { "" } else { "s" },
        spans.len(),
        if spans.len() == 1 { "" } else { "s" }
    );
    TRACE_TEMPLATE_HTML
        .replace("__LOGO__", crate::brand::mark_data_uri())
        .replace("__TITLE__", &html_escape(title))
        .replace("__HEAD__", &head)
        .replace("__BODY__", &body)
}

/// Compact duration for the trace view (the call-graph page formats its own).
fn fmt_ns_short(ns: u64) -> String {
    if ns >= 1_000_000_000 {
        format!("{:.2} s", ns as f64 / 1e9)
    } else if ns >= 1_000_000 {
        format!("{:.1} ms", ns as f64 / 1e6)
    } else if ns >= 1_000 {
        format!("{:.1} µs", ns as f64 / 1e3)
    } else {
        format!("{ns} ns")
    }
}

/// Renders the graph as Graphviz DOT: one node per function, heat-coloured by
/// self share, one edge per observed caller→callee pair.
pub(crate) fn render_dot(graph: &CallGraph) -> String {
    let mut out = String::from("digraph elephc_callgraph {\n");
    out.push_str("  rankdir=TB;\n");
    out.push_str("  node [shape=box, style=filled, fontname=\"sans-serif\"];\n");
    for (index, node) in graph.nodes.iter().enumerate() {
        let incl = graph.share(node.inclusive);
        let excl = graph.share(node.exclusive);
        // Hot nodes (high self share) climb the elephc heat ramp; amplified ×3
        // to match the HTML view, since most functions sit near zero self.
        let heat = (excl / 100.0 * 3.0).clamp(0.0, 1.0);
        let color = heat_color(heat);
        let fontcolor = ink_for(heat);
        let mut label = format!("{}\\nincl {:.1}% · self {:.1}%", dot_escape(&node.name), incl, excl);
        if let Some(count) = node.call_count {
            let _ = write!(label, "\\ncalls {count}");
        }
        if node.alloc_inclusive > 0 {
            let _ = write!(label, "\\n{} allocs self", node.alloc_exclusive);
        }
        if node.network_inclusive > 0 {
            let _ = write!(
                label,
                "\\n{} network ops self, {} incl",
                node.network_exclusive, node.network_inclusive
            );
        }
        if node.network_wait_inclusive > 0 {
            let _ = write!(
                label,
                "\\n{} network wait self, {} incl",
                fmt_ns_short(node.network_wait_exclusive),
                fmt_ns_short(node.network_wait_inclusive)
            );
        }
        if let Some((cause, samples)) = node.causes.first() {
            let _ = write!(label, "\\n{}: {:.0}%", cause, graph.share(*samples));
        }
        let _ = writeln!(
            out,
            "  n{index} [label=\"{label}\", fillcolor=\"{color}\", fontcolor=\"{fontcolor}\"];"
        );
    }
    for edge in &graph.edges {
        let label = match edge.count {
            Some(count) => format!("{:.0}% (×{count})", graph.share(edge.weight)),
            None => format!("{:.0}%", graph.share(edge.weight)),
        };
        let _ = writeln!(out, "  n{} -> n{} [label=\"{label}\"];", edge.from, edge.to);
    }
    out.push_str("}\n");
    out
}

/// Escapes a label for a DOT double-quoted string.
fn dot_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Renders a single (sampled) capture as a self-contained interactive HTML page.
pub(crate) fn render_html(graph: &CallGraph, title: &str) -> String {
    render_html_frames(&[(0, graph)], title, false, 0, false, &[])
}

/// Renders one or more capture frames as a self-contained interactive page.
/// With ≥2 frames (or `live`), a bottom timeline navigator scrubs the frames and
/// a diff mode highlights functions that grew hotter since the previous frame.
/// `window_secs` sets the live auto-reload cadence.
pub(crate) fn render_html_frames(
    frames: &[(u128, &CallGraph)],
    title: &str,
    live: bool,
    window_secs: u32,
    exact: bool,
    asserts: &[AssertOutcome],
) -> String {
    render_html_page(frames, title, live, window_secs, exact, asserts, None)
}

/// The transactions view `monitor --apm` adds to the page: its styles, its
/// panel and its script, with the data (`DATA.apm`) it draws from.
pub(crate) struct ApmView {
    /// `{ "mode": "live", ... }` for a page a collector serves, or
    /// `{ "mode": "static", "all": ..., "routes": ... }` for an export.
    pub data: serde_json::Value,
}

/// Styles of the transactions view.
const APM_STYLE: &str = include_str!("monitor/apm/view/style.css");
/// Markup of the transactions view and of the profile-capture prompt.
const APM_PANEL: &str = include_str!("monitor/apm/view/panel.html");
/// Script of the transactions view; it runs after the call-graph script.
const APM_SCRIPT: &str = include_str!("monitor/apm/view/script.js");

/// Renders the page, with the transactions view when `apm` is given.
///
/// One page for both, so profiling and monitoring share an entry point:
/// without `--apm` the placeholders are removed and the page is the call graph
/// it always was; with it, a Transactions/Profile switch joins the header.
pub(crate) fn render_html_page(
    frames: &[(u128, &CallGraph)],
    title: &str,
    live: bool,
    window_secs: u32,
    exact: bool,
    asserts: &[AssertOutcome],
    apm: Option<&ApmView>,
) -> String {
    render_html_page_with_timeline(frames, title, live, window_secs, exact, asserts, apm, None)
}

/// Renders the page with the run's timeline (`monitor::timeline`): every call
/// at its time and the heap in bytes, which the Flame view draws in its
/// Timeline mode. Only a launched exact run has one.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_html_page_with_timeline(
    frames: &[(u128, &CallGraph)],
    title: &str,
    live: bool,
    window_secs: u32,
    exact: bool,
    asserts: &[AssertOutcome],
    apm: Option<&ApmView>,
    timeline: Option<&serde_json::Value>,
) -> String {
    let frames_json: Vec<serde_json::Value> = frames
        .iter()
        .map(|(ts, g)| {
            // Allocation shares are relative to the root's inclusive allocs.
            let alloc_total = g.nodes.iter().map(|n| n.alloc_inclusive).max().unwrap_or(0);
            let alloc_share = |a: u64| if alloc_total > 0 {
                100.0 * a as f64 / alloc_total as f64
            } else {
                0.0
            };
            let nodes: Vec<serde_json::Value> = g
                .nodes
                .iter()
                .map(|node| {
                    serde_json::json!({
                        "name": node.name,
                        "incl": g.share(node.inclusive),
                        "excl": g.share(node.exclusive),
                        "calls": node.call_count,
                        "allocIncl": alloc_share(node.alloc_inclusive),
                        "allocExcl": alloc_share(node.alloc_exclusive),
                        "allocInclN": node.alloc_inclusive,
                        "allocExclN": node.alloc_exclusive,
                        "ioInclN": node.io_inclusive,
                        "ioExclN": node.io_exclusive,
                        "retInclN": node.retained_inclusive,
                        "retExclN": node.retained_exclusive,
                        "waitInclN": node.wait_inclusive,
                        "waitExclN": node.wait_exclusive,
                        "networkInclN": node.network_inclusive,
                        "networkExclN": node.network_exclusive,
                        "networkWaitInclN": node.network_wait_inclusive,
                        "networkWaitExclN": node.network_wait_exclusive,
                        "causes": node.causes.iter()
                            .map(|(c, s)| serde_json::json!({"name": c, "pct": g.share(*s)}))
                            .collect::<Vec<_>>(),
                    })
                })
                .collect();
            // Edges reference endpoints by NAME so the page can lay out the union
            // of all frames on one stable coordinate system.
            let edges: Vec<serde_json::Value> = g
                .edges
                .iter()
                .map(|edge| {
                    serde_json::json!({
                        "from": g.nodes[edge.from].name,
                        "to": g.nodes[edge.to].name,
                        "pct": g.share(edge.weight),
                        "count": edge.count,
                    })
                })
                .collect();
            let queries: Vec<serde_json::Value> = g
                .queries
                .iter()
                .map(|(sql, count)| {
                    let callers: Vec<serde_json::Value> = g
                        .query_callers
                        .get(sql)
                        .map(|rows| {
                            rows.iter()
                                .map(|c| serde_json::json!({"fn": c.function, "loc": c.location, "n": c.executions}))
                                .collect()
                        })
                        .unwrap_or_default();
                    serde_json::json!({"sql": sql, "count": count, "callers": callers})
                })
                .collect();
            serde_json::json!({
                "ts": (*ts as u64),
                "total": g.total,
                "totalAllocs": alloc_total,
                "nodes": nodes,
                "edges": edges,
                "queries": queries,
            })
        })
        .collect();
    let reload_ms = window_secs as u64 * 1000 + 500;
    // Per-line attribution belongs to the capture, not to a frame: it comes
    // from one dSYM resolution over the whole run. The latest frame that has it
    // wins, so a live page keeps showing lines as windows roll by.
    let lines_json = frames
        .iter()
        .rev()
        .find_map(|(_, g)| g.lines.as_ref())
        .map(|l| {
            serde_json::json!({
                "file": l.file,
                "source": l.source,
                "hits": l.hits.iter().map(|(n, s)| serde_json::json!([n, s])).collect::<Vec<_>>(),
                "total": l.total,
                "funcs": l.funcs.iter().map(|f| serde_json::json!({
                    "name": f.name, "start": f.start, "end": f.end,
                    "selfPct": f.self_pct, "inclPct": f.incl_pct, "calls": f.calls,
                })).collect::<Vec<_>>(),
            })
        });
    let asserts_json: Vec<serde_json::Value> = asserts
        .iter()
        .map(|a| {
            serde_json::json!({
                "spec": a.spec,
                "label": a.label,
                "metric": a.metric,
                "target": a.target,
                "op": a.op,
                "budget": a.budget,
                "actual": a.actual,
                "status": match a.status {
                    AssertStatus::Pass => "pass",
                    AssertStatus::Fail => "fail",
                    AssertStatus::Error => "error",
                },
                "note": a.note,
            })
        })
        .collect();
    let mut data = serde_json::json!({
        "title": title,
        "live": live,
        "exact": exact,
        "reloadMs": reload_ms,
        "frames": frames_json,
        "lines": lines_json,
        "asserts": asserts_json,
    });
    if let Some(view) = apm {
        data["apm"] = view.data.clone();
    }
    if let Some(timeline) = timeline {
        data["timeline"] = timeline.clone();
    }
    // Neutralize `</script>` (and any `</…`) in embedded names: `\/` is a valid
    // JSON escape for `/`, so this stays parseable while it cannot close the tag.
    let data_json = serde_json::to_string(&data)
        .unwrap_or_else(|_| "{}".to_string())
        .replace("</", "<\\/");
    // Title is developer-supplied (the target path); insert it first. The frame
    // data — which carries attacker-influenceable PHP names — is inserted last so
    // no later replace can touch it.
    let (style, panel, script) = match apm {
        Some(_) => (APM_STYLE, APM_PANEL, format!("<script>\n{APM_SCRIPT}</script>\n")),
        None => ("", "", String::new()),
    };
    TEMPLATE_HTML
        .replace("__APM_STYLE__", style)
        .replace("__APM_PANEL__", panel)
        .replace("__APM_SCRIPT__", &script)
        .replace("__LOGO__", crate::brand::mark_data_uri())
        .replace("__TITLE__", &html_escape(title))
        .replace("__DATA_JSON__", &data_json)
}

/// Escapes text for HTML element content / the `<title>`.
fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The elephc brand heat ramp, sampled from the logo gradient: warm-pale (cold)
/// → gold → orange → red → magenta (hottest). `heat` is clamped to 0..1.
const HEAT_STOPS: [(f64, u8, u8, u8); 5] = [
    (0.0, 0xf2, 0xe9, 0xe4),
    (0.08, 0xff, 0xd9, 0x00),
    (0.25, 0xff, 0x8b, 0x1b),
    (0.55, 0xff, 0x52, 0x2c),
    (1.0, 0xff, 0x00, 0x70),
];

/// Interpolates the heat gradient at `heat` (clamped to 0..=1), returning the
/// RGB triple between the two surrounding stops.
fn heat_rgb(heat: f64) -> (u8, u8, u8) {
    let t = heat.clamp(0.0, 1.0);
    let mut i = 0;
    while i < HEAT_STOPS.len() - 1 && t > HEAT_STOPS[i + 1].0 {
        i += 1;
    }
    let a = HEAT_STOPS[i];
    let b = HEAT_STOPS[(i + 1).min(HEAT_STOPS.len() - 1)];
    let f = ((t - a.0) / (b.0 - a.0).max(1e-9)).clamp(0.0, 1.0);
    let lerp = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * f).round() as u8;
    (lerp(a.1, b.1), lerp(a.2, b.2), lerp(a.3, b.3))
}

/// Hex fill for a 0..1 heat on the elephc ramp.
fn heat_color(heat: f64) -> String {
    let (r, g, b) = heat_rgb(heat);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Readable label color over a heat fill: light ink over the hot (dark) end,
/// dark ink over the pale end.
fn ink_for(heat: f64) -> &'static str {
    let (r, g, b) = heat_rgb(heat);
    let lum = 0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64;
    if lum < 150.0 {
        "#fff8f4"
    } else {
        "#201a17"
    }
}

/// The interactive page. `__TITLE__` and `__DATA_JSON__` are substituted at
/// render time. Plain const (not `format!`), so braces need no escaping.
/// The distributed-trace page. Same tokens and heat ramp as the call graph, so
/// the two read as one tool; static markup, no scripting beyond `<details>`.
const TRACE_TEMPLATE_HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>__TITLE__</title>
<style>
:root {
  --bg: #faf9f7; --panel: #ffffff; --ink: #1c1b1a; --muted: #5f5a54;
  --border: #e7e3de; --accent: #d6005e; --hover: rgba(28,27,26,.05);
  --brand: linear-gradient(90deg, #ff0070 0%, #ff5252 38%, #ff8f2c 68%, #ffb800 100%);
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --bg: #151413; --panel: #1f1e1c; --ink: #ece9e5; --muted: #b9b3ab;
    --border: #33302c; --accent: #ff5c99; --hover: rgba(255,255,255,.06);
  }
}
:root[data-theme="dark"] {
  --bg: #151413; --panel: #1f1e1c; --ink: #ece9e5; --muted: #b9b3ab;
  --border: #33302c; --accent: #ff5c99; --hover: rgba(255,255,255,.06);
}
* { box-sizing: border-box; }
html, body { margin: 0; background: var(--bg); color: var(--ink);
  font-family: Roboto, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
header { position: relative; display: flex; align-items: center; gap: .75rem; height: 3.1rem; padding: 0 1rem 2px;
  background: var(--panel); }
header::after { content: ""; position: absolute; left: 0; right: 0; bottom: 0; height: 2px; background: var(--brand); }
header .logo { padding: 4px; box-sizing: content-box; background: #ffffff; border: 1px solid var(--border);
  border-radius: 10px; }
header .logo { flex: none; display: block; }
header h1 { margin: 0; font-size: 1rem; font-weight: 650; white-space: nowrap;
  overflow: hidden; text-overflow: ellipsis; }
header .meta { color: var(--muted); font-size: .82rem; white-space: nowrap; margin-left: auto; }
main { padding: 1rem; max-width: 78rem; }
.trace { margin-bottom: 1.6rem; }
.trace h2 { font-size: .8rem; font-weight: 600; color: var(--muted); margin: 0 0 .5rem;
  text-transform: uppercase; letter-spacing: .05em; }
.trace h2 code { font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  text-transform: none; letter-spacing: 0; color: var(--ink); }
.trace h2 .n { color: var(--muted); font-weight: 400; }
.span { margin-left: calc(var(--d) * 1.4rem); border-left: 2px solid var(--border);
  padding-left: .5rem; }
.span > summary { display: grid; grid-template-columns: minmax(6rem, 12rem) 1fr auto;
  gap: .75rem; align-items: center; padding: .3rem .4rem; border-radius: 7px;
  cursor: pointer; list-style: none; }
.span > summary::-webkit-details-marker { display: none; }
.span > summary:hover { background: var(--hover); }
.svc { font-weight: 620; font-size: .88rem; white-space: nowrap; overflow: hidden;
  text-overflow: ellipsis; }
.bar { display: block; background: var(--hover); border-radius: 5px; overflow: hidden; }
.bar i { display: block; font-style: normal; font-size: .74rem; line-height: 1.5rem;
  padding: 0 .45rem; white-space: nowrap; font-variant-numeric: tabular-nums;
  border-radius: 5px; min-width: 3.5rem; }
.facts { color: var(--muted); font-size: .76rem; white-space: nowrap;
  font-variant-numeric: tabular-nums; }
table.top { width: 100%; border-collapse: collapse; margin: .2rem 0 .6rem 1rem;
  max-width: 44rem; }
table.top th { text-align: left; font-size: .66rem; text-transform: uppercase;
  letter-spacing: .05em; color: var(--muted); font-weight: 600;
  padding: .2rem .5rem; border-bottom: 1px solid var(--border); }
table.top th:not(:first-child), table.top td.pc { text-align: right; }
table.top td { padding: .16rem .5rem; font-size: .8rem; border-bottom: 1px solid var(--border); }
table.top td.fn { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
table.top td.pc { font-variant-numeric: tabular-nums; color: var(--muted); white-space: nowrap; }
.none { color: var(--muted); font-size: .85rem; }
footer { color: var(--muted); font-size: .74rem; padding: 0 1rem 1.5rem; max-width: 78rem; }
</style>
</head>
<body>
<header><img class="logo" src="__LOGO__" alt="" width="20" height="21"><h1>__TITLE__</h1>__HEAD__</header>
<main>__BODY__</main>
<footer>Each row is one profiled request slice, nested under the service that called it.
Bars sit on a shared time axis: a span starts where it opened and is as wide as it ran,
so hops that followed one another step rightwards and concurrent ones overlap. The axis
is wall clock, so two hosts' clocks can drift apart. A trace whose spans are not all
timestamped falls back to durations, flush left. Open a row for that service's hottest
functions. Spans correlate by W3C Trace Context.</footer>
</body>
</html>
"##;

const TEMPLATE_HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>__TITLE__ — call graph</title>
<style>
/* Material 3 roles over elephc's colours: a pearl-white surface, the logo's
   magenta deepened to #d6005e as the primary (white text on it reads at 5:1),
   the logo's gradient kept for one hairline under the app bar. A selected
   segment, chip or row takes the primary container; a filled button the
   primary itself. Pass and fail tags use the M3 container pairs, so their text
   reads in both themes. Dark is chosen, not flipped: its primary is the light
   #ff5c99 with dark text on it. */
:root {
  --bg: #faf9f7; --panel: #ffffff; --ink: #1c1b1a; --muted: #5f5a54;
  --line: #e5e2de; --edge: #a8a29b; --border: #e7e3de; --accent: #d6005e;
  --on-accent: #ffffff; --accent-container: #ffe3ee; --on-accent-container: #5c0026;
  --pass: #1b6d3a; --pass-container: #c8f2d2; --on-pass-container: #00210c;
  --fail: #ba1a1a; --fail-container: #ffdad6; --on-fail-container: #410002;
  --live: #1f9d55; --idle: #a8a29b;
  --mem: #c9d4de; --mem-line: #8a9bab; --tl-dim: rgba(250,249,247,.72);
  --shadow: 0 1px 3px rgba(28,27,26,.12), 0 4px 12px rgba(28,27,26,.08);
  --brand: linear-gradient(90deg, #ff0070 0%, #ff5252 38%, #ff8f2c 68%, #ffb800 100%);
  --sidebar-w: 20rem; --sel: rgba(214,0,94,.08); --hover: rgba(28,27,26,.05);
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --bg: #151413; --panel: #1f1e1c; --ink: #ece9e5; --muted: #b9b3ab;
    --line: #3a3733; --edge: #6e6862; --border: #33302c; --accent: #ff5c99;
    --on-accent: #3e0018; --accent-container: #5a1530; --on-accent-container: #ffd9e5;
    --pass: #8bd69b; --pass-container: #00522a; --on-pass-container: #c8f2d2;
    --fail: #ffb4ab; --fail-container: #93000a; --on-fail-container: #ffdad6;
    --live: #4cc77a; --idle: #6e6862;
  --mem: #2c3946; --mem-line: #6f8396; --tl-dim: rgba(21,20,19,.7);
    --mem: #2c3946; --mem-line: #6f8396; --tl-dim: rgba(21,20,19,.7);
    --shadow: 0 1px 3px rgba(0,0,0,.4), 0 4px 12px rgba(0,0,0,.3);
    --sel: rgba(255,92,153,.16); --hover: rgba(255,255,255,.06);
  }
}
:root[data-theme="dark"] {
  --bg: #151413; --panel: #1f1e1c; --ink: #ece9e5; --muted: #b9b3ab;
  --line: #3a3733; --edge: #6e6862; --border: #33302c; --accent: #ff5c99;
  --on-accent: #3e0018; --accent-container: #5a1530; --on-accent-container: #ffd9e5;
  --pass: #8bd69b; --pass-container: #00522a; --on-pass-container: #c8f2d2;
  --fail: #ffb4ab; --fail-container: #93000a; --on-fail-container: #ffdad6;
  --live: #4cc77a; --idle: #6e6862;
  --shadow: 0 1px 3px rgba(0,0,0,.4), 0 4px 12px rgba(0,0,0,.3);
  --sel: rgba(255,92,153,.16); --hover: rgba(255,255,255,.06);
}
* { box-sizing: border-box; }
html, body { margin: 0; height: 100%; background: var(--bg); color: var(--ink);
  font-family: Roboto, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
/* Fixed-height app bar. The panes below are pinned at 3.1rem, so the header
   must never grow past it: a long title (monitor titles are file paths) would
   otherwise wrap and sit on top of the sidebar and the graph. Nothing wraps;
   the title is the only thing allowed to shrink, and it ellipsizes. */
header { position: relative; display: flex; gap: .75rem; align-items: center; padding: 0 1rem 2px;
  height: 3.1rem; box-sizing: border-box; flex-wrap: nowrap; overflow: hidden;
  background: var(--panel); }
/* The brand gradient, once: a hairline under the app bar. */
header::after { content: ""; position: absolute; left: 0; right: 0; bottom: 0; height: 2px; background: var(--brand); }
header > * { flex: none; }
/* The logo sits in a white tile in both themes: its gradient is drawn for white. */
header .logo { flex: none; display: block; padding: 4px; box-sizing: content-box; background: #ffffff;
  border: 1px solid var(--border); border-radius: 10px; }
header h1 { font-size: 1.05rem; margin: 0; font-weight: 500; flex: 0 1 auto; min-width: 3rem;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
header .meta { color: var(--muted); font-size: .82rem; white-space: nowrap;
  flex: 0 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
header .spacer { flex: 1 1 auto; min-width: .25rem; }
header input { padding: .35rem .6rem; border: 1px solid var(--border);
  border-radius: 999px; background: var(--bg); color: var(--ink); font-size: .85rem; width: 13rem; }
/* Segmented controls: the view switcher and the dimension selector. Both are
   single-choice, so both read as one control rather than a row of toggles. */
#metricbar, #viewbar { display: none; align-items: center; gap: 2px; padding: 2px;
  background: var(--bg); border: 1px solid var(--border); border-radius: 999px; }
#metricbar.show, #viewbar.show { display: inline-flex; }
#metricbar button, #viewbar button { border: 0; background: transparent; color: var(--muted); cursor: pointer;
  padding: .26rem .55rem; border-radius: 999px; font-size: .78rem; display: inline-flex; align-items: center; gap: .3rem;
  white-space: nowrap; }
#metricbar button .g, #viewbar button .g { font-size: .8rem; line-height: 1; }
/* The three view buttons are now driven by #viewbar; they stay in the DOM so
   the keyboard shortcuts and state handlers keep one code path. */
#flamebtn, #sqlbtn, #srcbtn, #chkbtn { display: none !important; }
/* Narrow windows: drop the segment labels and keep the icons, so the bar keeps
   fitting on one line instead of pushing the header past its fixed height. */
#metricbar button span:not(.g) { display: none; }
#metricbar button { padding: .26rem .42rem; }
@media (max-width: 1000px) {
  #viewbar button span:not(.g) { display: none; }
  #viewbar button.on span:not(.g) { display: inline; }
  #viewbar button { padding: .26rem .4rem; }
}
/* Narrower still: the run summary moves out (it is also in the legend) and the
   search box gives up width, so nothing gets clipped by the fixed height. */
@media (max-width: 880px) {
  header .meta { display: none; }
  header input { width: 8rem; }
}
/* The active segment is filled, not merely lifted: with two bars side by side a
   subtle panel-coloured pill was not enough to say what is being shown. The
   chosen one also keeps its label while the others stay icons, so the bar reads
   as a sentence — "Graph, by Memory" — at a glance and for one label's width. */
#metricbar button.on, #viewbar button.on {
  background: var(--accent-container); color: var(--on-accent-container); font-weight: 600; }
#metricbar button.on .g, #viewbar button.on .g { filter: none; }
#metricbar button.on span:not(.g) { display: inline; }
#metricbar button:hover:not(.on), #viewbar button:hover:not(.on) {
  background: var(--hover); color: var(--ink); }
#metricbar button:hover:not(.on) { color: var(--ink); }
#stage { position: absolute; inset: 3.1rem 0 0 var(--sidebar-w); overflow: hidden; cursor: grab; }
body.hasnav #stage { bottom: 3.1rem; }
#stage.grabbing { cursor: grabbing; }
/* Flame graph (icicle): an alternative to the node-link view. Rows of cells,
   width = inclusive time, stacked callee-below-caller. Overlays the stage. */
/* The four reading panels (Flame, Queries, Source, Checks) share one frame:
   a head (what is shown, as a title, chips and a quiet note) over a card on
   the surface. The legend card is the graph's: elsewhere it only covered
   the panels' heads and figures. */
#flame, #sqlpanel, #srcpanel, #chkpanel { padding: 1rem 1.25rem 1.5rem; }
body.flame #legend, body.sql #legend, body.src #legend, body.chk #legend { display: none; }
.phead { display: flex; flex-wrap: wrap; align-items: center; gap: .5rem .6rem; margin: 0 0 .75rem; max-width: 1100px; }
.phead .pt { font-size: 1rem; font-weight: 500; color: var(--ink); }
.phead .pnote { font-size: .78rem; color: var(--muted); }
.phead .pnote b { color: var(--ink); font-weight: 600; }
.phead .pchip { display: inline-flex; align-items: center; gap: .3rem; height: 1.6rem; padding: 0 .65rem;
  border: 1px solid var(--border); border-radius: 8px; font-size: .76rem; color: var(--muted); background: var(--panel); }
.phead .pchip b { color: var(--ink); font-weight: 600; }
.phead .pchip.pass { border-color: transparent; background: var(--pass-container); color: var(--on-pass-container); }
.phead .pchip.fail { border-color: transparent; background: var(--fail-container); color: var(--on-fail-container); }
.phead .pchip.pass b, .phead .pchip.fail b { color: inherit; }
.phead .pbtn { height: 1.6rem; display: inline-flex; align-items: center; padding: 0 .75rem; border-radius: 999px;
  background: var(--accent-container); color: var(--on-accent-container); font-size: .76rem; font-weight: 600; text-decoration: none; }
.pcard { background: var(--panel); border: 1px solid var(--border); border-radius: 16px; overflow: hidden; max-width: 1100px; }
.pcard.pempty { padding: 1rem 1.25rem; color: var(--muted); font-size: .85rem; }
/* The Flame view's two modes, and its Timeline: an overview of the whole
   run with the picked range, then every call at its time over the heap's
   bytes. The memory is a cool slate so it never reads as heat. */
.phead .pseg { display: inline-flex; gap: 2px; padding: 2px; border: 1px solid var(--border); border-radius: 999px; }
.phead .pseg button { border: 0; background: transparent; color: var(--muted); cursor: pointer; border-radius: 999px; padding: .15rem .7rem; font-size: .76rem; }
.phead .pseg button:hover:not(.on) { background: var(--hover); color: var(--ink); }
.phead .pseg button.on { background: var(--accent-container); color: var(--on-accent-container); font-weight: 600; }
.phead .tl-key { margin-left: auto; display: inline-flex; align-items: center; gap: .4rem; font-size: .74rem; color: var(--muted); }
.phead .tl-key i { display: inline-block; width: 14px; height: 9px; border-radius: 2px; margin-left: .5rem; }
.phead .tl-key i.mem { background: var(--mem); border-top: 2px solid var(--mem-line); }
.phead .tl-key i.own { width: 40px; height: 8px; border-radius: 4px; background: linear-gradient(90deg, #f2e9e4, #ffd900 20%, #ff8b1b 45%, #ff522c 70%, #ff0070); }
.pcard.tl { max-width: none; padding: .75rem 1rem 1rem; }
.tl-overax { position: relative; height: 18px; margin-left: 64px; font-size: .7rem; color: var(--muted); font-variant-numeric: tabular-nums; }
.tl-overax span { position: absolute; top: 2px; transform: translateX(-50%); white-space: nowrap; }
.tl-overax span:first-child { transform: none; }
.tl-over { position: relative; margin-left: 64px; height: 96px; border-radius: 8px; overflow: hidden; background: var(--bg); cursor: crosshair; user-select: none; }
.tl-over canvas { display: block; width: 100%; height: 96px; }
.tl-dim { position: absolute; top: 0; bottom: 0; background: var(--tl-dim); pointer-events: none; }
.tl-dim.l { left: 0; } .tl-dim.r { right: 0; }
.tl-brush { position: absolute; top: 0; bottom: 0; box-sizing: border-box; border: 2px solid var(--accent); border-radius: 4px; pointer-events: none; }
.tl-main { display: block; width: 100%; margin-top: .75rem; }
.tl-note { margin: .5rem 0 0 64px; font-size: .74rem; color: var(--muted); }
.pcard.tl-detail { margin-top: .75rem; max-width: none; display: flex; align-items: center; gap: 1.25rem; padding: .75rem 1rem; }
.tl-detail .sw { width: 5px; align-self: stretch; border-radius: 3px; }
.tl-detail .tl-dn { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.tl-detail .tl-dn b { font-weight: 500; font-size: .95rem; }
.tl-detail .tl-dn span { font-size: .78rem; color: var(--muted); }
.tl-detail .tl-fig { display: flex; flex-direction: column; align-items: flex-end; min-width: 6rem; }
.tl-detail .tl-fig span { font-size: .72rem; color: var(--muted); }
.tl-detail .tl-fig b { font-size: 1rem; font-weight: 600; font-variant-numeric: tabular-nums; }
#flame { display: none; position: absolute; inset: 0; overflow: auto; cursor: default; background: var(--bg); }
body.flame #stage { cursor: default; }
body.flame #svg { display: none; }
body.flame #flame { display: block; }
#flame .fcard { max-width: none; padding: .75rem; }
#flame .fcanvas { position: relative; }
/* Cells keep a 2px surface gap to their neighbours, so a row of same-heat
   frames still reads as separate calls. */
#flame .fcell { position: absolute; height: 20px; border-radius: 4px; border: 0;
  border-right: 2px solid var(--panel); box-sizing: border-box; overflow: hidden;
  font-size: .74rem; font-weight: 500; line-height: 20px; padding: 0 6px; white-space: nowrap; cursor: pointer;
  text-overflow: ellipsis; font-variant-numeric: tabular-nums; }
#flame .fcell:hover { outline: 2px solid var(--ink); outline-offset: -2px; z-index: 3; }
#flame .fcell.sel { outline: 2px solid var(--accent); outline-offset: -2px; z-index: 2; }
/* SQL panel: distinct DB statements and their execution counts (the N+1 view). */
#sqlpanel { display: none; position: absolute; inset: 0; overflow: auto; cursor: default; background: var(--bg); }
body.sql #stage { cursor: default; }
body.sql #svg, body.sql #flame { display: none; }
body.sql #sqlpanel { display: block; }
#sqlpanel table { border-collapse: collapse; width: 100%; }
#sqlpanel th { text-align: left; font-size: .75rem; color: var(--muted); font-weight: 500;
  padding: .65rem 1rem; border-bottom: 1px solid var(--border); }
#sqlpanel th:first-child { text-align: right; }
#sqlpanel td { padding: .75rem 1rem; border-bottom: 1px solid var(--border); vertical-align: middle; }
#sqlpanel tbody tr:last-child td { border-bottom: 0; }
#sqlpanel tbody tr:hover { background: var(--hover); }
#sqlpanel td.qc { font-variant-numeric: tabular-nums; font-weight: 650; white-space: nowrap; text-align: right; }
#sqlpanel td.qbar { width: 24%; min-width: 8rem; }
#sqlpanel .qtrack { display: block; height: 8px; border-radius: 4px; background: var(--line); overflow: hidden; }
#sqlpanel .qtrack i { display: block; height: 100%; border-radius: 4px; min-width: 2px; }
#sqlpanel td.qsql { width: 100%; }
#sqlpanel code { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .8rem;
  color: var(--ink); white-space: pre-wrap; word-break: break-word; }
#sqlpanel tr.warn td.qc { color: var(--accent); }
/* A statement with callers opens on click: a chevron that turns, then the
   functions that ran it with their file and their share of its runs. */
#sqlpanel tr.qopen { cursor: pointer; }
#sqlpanel tr.qopen:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
#sqlpanel .qchev { display: inline-block; width: 15px; height: 15px; vertical-align: -3px; margin-right: .3rem; color: var(--muted); transition: transform .15s; }
#sqlpanel tr.open .qchev { transform: rotate(90deg); color: var(--accent); }
#sqlpanel tr.open td { border-bottom-color: transparent; }
#sqlpanel .qfromn { margin-left: .6rem; font-size: .72rem; color: var(--muted); }
#sqlpanel tr.qfrom td { padding-top: 0; background: var(--panel); }
#sqlpanel tr.qfrom:hover { background: transparent; }
#sqlpanel .qlist { border: 1px solid var(--border); border-radius: 12px; overflow: hidden; background: var(--bg); }
#sqlpanel .qlh, #sqlpanel .qcaller { display: grid; grid-template-columns: minmax(10rem, 1.4fr) minmax(8rem, 1fr) 4rem 8rem; gap: 1rem; align-items: center; padding: .5rem .85rem; }
#sqlpanel .qlh { font-size: .72rem; color: var(--muted); border-bottom: 1px solid var(--border); }
#sqlpanel .qcaller { width: 100%; border: 0; border-bottom: 1px solid var(--border); background: transparent; text-align: left; font: inherit; color: var(--ink); cursor: pointer; }
#sqlpanel .qcaller:last-child { border-bottom: 0; }
#sqlpanel .qcaller:hover { background: var(--hover); }
#sqlpanel .qfn { font-weight: 500; font-size: .85rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
#sqlpanel .qloc { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .78rem; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
#sqlpanel .qn { font-variant-numeric: tabular-nums; font-weight: 600; text-align: right; font-size: .85rem; }
#sqlpanel .qcaller .qtrack i { background: var(--accent); }
#sqlpanel .qn1 { display: inline-block; margin-left: .6rem; font-size: .68rem; font-weight: 600;
  color: var(--on-accent-container); background: var(--accent-container); border-radius: 999px; padding: .08rem .5rem;
  vertical-align: middle; }
/* Checks panel: the performance budget and how this run measured against it. */
#chkpanel { display: none; position: absolute; inset: 0; overflow: auto; cursor: default; background: var(--bg); }
body.chk #stage { cursor: default; }
body.chk #svg, body.chk #flame, body.chk #sqlpanel, body.chk #srcpanel { display: none; }
/* Only the graph pans, so its whole tool section is irrelevant elsewhere. */
body.flame #mh-graph, body.sql #mh-graph, body.src #mh-graph, body.chk #mh-graph,
body.flame #fitbtn, body.sql #fitbtn, body.src #fitbtn, body.chk #fitbtn,
body.flame #critbtn, body.sql #critbtn, body.src #critbtn, body.chk #critbtn,
body.flame #prunerow, body.sql #prunerow, body.src #prunerow, body.chk #prunerow { display: none; }
body.chk #chkpanel { display: block; }
#chkpanel .krow { display: grid; grid-template-columns: 4.2rem 1fr auto auto; gap: .9rem;
  align-items: baseline; padding: .75rem 1rem; border-bottom: 1px solid var(--border); }
#chkpanel .krow:last-child { border-bottom: 0; }
#chkpanel .kt { font-size: .68rem; font-weight: 700; letter-spacing: .05em; border-radius: 999px;
  padding: .1rem .45rem; text-align: center; }
#chkpanel .kt.pass { background: var(--pass-container); color: var(--on-pass-container); }
#chkpanel .kt.fail { background: var(--fail-container); color: var(--on-fail-container); }
#chkpanel .kt.error { background: var(--line); color: var(--ink); }
#chkpanel .kspec { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .82rem;
  color: var(--ink); min-width: 0; overflow-wrap: anywhere; }
#chkpanel .klabel { display: block; margin-top: .15rem; font-family: Roboto, system-ui, -apple-system, sans-serif; font-size: .78rem; color: var(--muted); }
#chkpanel .kval { font-variant-numeric: tabular-nums; font-size: .82rem; white-space: nowrap; }
#chkpanel .kval b { font-weight: 700; }
#chkpanel .kbudget { font-variant-numeric: tabular-nums; font-size: .78rem; color: var(--muted);
  white-space: nowrap; }

/* Source panel: the PHP file annotated with per-line sampled cost. */
#srcpanel { display: none; position: absolute; inset: 0; overflow: auto; cursor: default; background: var(--bg); }
#srcpanel .scode { max-width: none; padding: .5rem 0; }
body.src #stage { cursor: default; }
body.src #svg, body.src #flame, body.src #sqlpanel { display: none; }
body.src #srcpanel { display: block; }
#srcpanel .sline { display: grid; grid-template-columns: 4.6rem 3.2rem 1fr; align-items: baseline;
  gap: .5rem; padding: .02rem .8rem; font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: .8rem; line-height: 1.5; }
#srcpanel .sline.hot { font-weight: 600; }
#srcpanel .sn { color: var(--muted); text-align: right; font-variant-numeric: tabular-nums;
  font-size: .72rem; }
#srcpanel .sp { font-variant-numeric: tabular-nums; text-align: right; border-radius: 999px;
  padding: 0 .45rem; font-family: Roboto, system-ui, sans-serif; font-size: .72rem; font-weight: 600; }
#srcpanel .sc { white-space: pre; overflow-x: auto; color: var(--ink); }
#srcpanel .sfn { font-family: Roboto, system-ui, -apple-system, sans-serif; font-size: .7rem;
  color: var(--muted); margin-left: .9rem; }
/* Left sidebar: the sortable function list (Blackfire-style). */
#sidebar { position: absolute; left: 0; top: 3.1rem; bottom: 0; width: var(--sidebar-w);
  background: var(--panel); border-right: 1px solid var(--border); display: flex; flex-direction: column; }
body.hasnav #sidebar { bottom: 3.1rem; }
/* Drag handle on the sidebar/graph boundary to resize the list width. */
#dragger { position: absolute; top: 3.1rem; bottom: 0; left: var(--sidebar-w); width: 7px;
  margin-left: -3px; cursor: col-resize; z-index: 6; }
body.hasnav #dragger { bottom: 3.1rem; }
#dragger::after { content: ''; position: absolute; top: 0; bottom: 0; left: 3px; width: 1px; background: transparent; }
#dragger:hover::after, body.resizing #dragger::after { background: var(--accent); width: 2px; left: 2.5px; }
body.resizing { cursor: col-resize; user-select: none; }
body.resizing #stage, body.resizing #fnlist { pointer-events: none; }
.side-head { display: flex; align-items: baseline; justify-content: space-between; gap: .5rem;
  padding: .6rem .85rem; border-bottom: 1px solid var(--border); }
.side-head .t { font-size: .92rem; font-weight: 500; color: var(--ink); }
/* Outlined icons (the page's ICONS): 24 grid, 2px stroke, the text colour. */
.ic { display: block; width: 18px; height: 18px; fill: none; stroke: currentColor; stroke-width: 2;
  stroke-linecap: round; stroke-linejoin: round; flex: none; }
.side-head #groupbtn { display: none; place-items: center; width: 28px; height: 28px; padding: 0; background: transparent;
  border: 1px solid var(--border); border-radius: 999px; color: var(--muted); cursor: pointer; }
.side-head #groupbtn .ic { width: 15px; height: 15px; }
.side-head #groupbtn:hover { color: var(--ink); background: var(--hover); }
.side-head #groupbtn.on { background: var(--accent-container); color: var(--on-accent-container); border-color: transparent; }
/* Self / Inclusive: which cost the list ranks by and the boxes print. */
.side-head .scope { margin-left: auto; display: inline-flex; gap: 2px; padding: 2px; border: 1px solid var(--border);
  border-radius: 999px; }
.side-head .scope button { border: 0; background: transparent; color: var(--muted); cursor: pointer; border-radius: 999px;
  padding: .2rem .6rem; font-size: .75rem; }
.side-head .scope button:hover:not(.on) { background: var(--hover); color: var(--ink); }
.side-head .scope button.on { background: var(--accent-container); color: var(--on-accent-container); font-weight: 600; }
.side-head .sort { font-size: .7rem; color: var(--muted); text-transform: uppercase; letter-spacing: .05em;
  font-variant-numeric: tabular-nums; }
#sidebar > .sort { padding: .45rem 1rem 0; font-size: .75rem; color: var(--muted); }
/* Header buttons that hold one icon. */
#menubtn { display: grid; place-items: center; width: 34px; height: 34px; padding: 0; }
#nav #prev, #nav #next { display: grid; place-items: center; width: 30px; height: 30px; padding: 0; }
#menu .mi .ic { width: 18px; height: 18px; margin: 0 auto; }
/* Grouped list: collapsible class / namespace headers. */
.grouphdr { display: flex; align-items: center; gap: .4rem; padding: .34rem .55rem; cursor: pointer;
  font-size: .74rem; color: var(--muted); border-top: 1px solid var(--border); }
.grouphdr:first-child { border-top: none; }
.grouphdr:hover { background: var(--hover); }
.grouphdr .gtw { font-size: .58rem; transition: transform .15s; }
.grouphdr.coll .gtw { transform: rotate(-90deg); }
.grouphdr .gname { font-weight: 700; color: var(--ink); flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.grouphdr .gcount { color: var(--muted); font-variant-numeric: tabular-nums; }
.grouphdr.hide { display: none; }
/* A grouped row is indented under its class; its open detail follows it. */
.fnrow.ingroup, .fndetail.ingroup { margin-left: .7rem; }
#fnlist { list-style: none; margin: 0; padding: .3rem; overflow-y: auto; flex: 1; }
.fnrow { display: grid; grid-template-columns: 4px 1fr auto; gap: .65rem; align-items: center;
  padding: .45rem .6rem; border-radius: 12px; cursor: pointer; border: 1px solid transparent; }
.fnrow:hover { background: var(--hover); }
.fnrow.sel { background: var(--accent-container); }
.fnrow.sel .nm .n, .fnrow.sel .val .pv { color: var(--on-accent-container); }
.fnrow.hide { display: none; }
.fnrow .sw { display: flex; align-items: flex-end; width: 4px; height: 30px; border-radius: 2px; background: var(--line); overflow: hidden; }
.fnrow .sw > i { display: block; width: 100%; min-height: 3px; border-radius: 2px; }
.fnrow .nm { min-width: 0; }
/* The name shortens, its query badge does not: the badge sits beside the
   ellipsized name instead of inside it. */
.fnrow .nm .n { display: flex; align-items: center; min-width: 0; font-size: .82rem; color: var(--ink); }
.fnrow .nm .n .nt { min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.fnrow .nm .n .qbadge { flex: none; }
.fnrow .nm .m2 { display: block; font-size: .68rem; color: var(--muted); font-variant-numeric: tabular-nums;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 1px; }
.fnrow .val { display: flex; flex-direction: column; align-items: flex-end; }
.fnrow .val .pv { display: block; font-size: .82rem; font-weight: 650; color: var(--ink); font-variant-numeric: tabular-nums; }
.fnrow .val .mini { display: block; height: 4px; width: 4rem; border-radius: 2px; background: var(--line); margin-top: 3px; overflow: hidden; }
.fnrow .val .mini > i { display: block; height: 100%; background: var(--accent); }
.qbadge { display: inline-block; font-size: .62rem; line-height: 1.4; padding: 0 .32rem; border-radius: 4px;
  margin-left: .35rem; background: var(--accent-container); color: var(--on-accent-container); font-variant-numeric: tabular-nums;
  vertical-align: middle; }
#empty { color: var(--muted); font-size: .78rem; padding: 1rem .85rem; }
/* The selected function opens in place, as in Blackfire: its row and a card
   attached under it with its figures and its callers / callees. */
.fnrow.sel.open { border-bottom-left-radius: 0; border-bottom-right-radius: 0; }
.fndetail { list-style: none; margin: -1px 0 .35rem; padding: .55rem .55rem .45rem; background: var(--bg);
  border: 1px solid var(--accent-container); border-top: 0; border-radius: 0 0 12px 12px; cursor: default; }
.fndetail.hide { display: none; }
.fndetail .fig { display: grid; grid-template-columns: 1fr auto auto; gap: .18rem .8rem; align-items: baseline;
  font-size: .76rem; font-variant-numeric: tabular-nums; margin-bottom: .55rem; }
.fndetail .fig .fh { font-size: .62rem; font-weight: 600; color: var(--muted); text-transform: uppercase;
  letter-spacing: .05em; text-align: right; }
.fndetail .fig .fh:first-child { text-align: left; }
.fndetail .fig .fl { color: var(--muted); white-space: nowrap; }
.fndetail .fig .fv { color: var(--ink); text-align: right; white-space: nowrap; }
.fndetail .fig .fv.x { font-weight: 650; }
.fndetail .dtabs { display: flex; align-items: center; gap: .4rem; margin-bottom: .35rem; }
.fndetail .dseg { display: inline-flex; gap: 2px; padding: 2px; background: var(--bg); border: 1px solid var(--border);
  border-radius: 7px; }
.fndetail .dseg button { border: 0; background: transparent; color: var(--muted); cursor: pointer; border-radius: 5px;
  padding: .18rem .5rem; font-size: .74rem; white-space: nowrap; font-variant-numeric: tabular-nums; }
.fndetail .dseg button:hover:not(.on):not(:disabled) { background: var(--hover); color: var(--ink); }
.fndetail .dseg button.on { background: var(--accent-container); color: var(--on-accent-container); font-weight: 600; }
.fndetail .dseg button:disabled { opacity: .45; cursor: default; }
.fndetail .dgo { margin-left: auto; border: 1px solid var(--border); background: var(--bg); color: var(--ink);
  border-radius: 6px; cursor: pointer; padding: .16rem .45rem; font-size: .72rem; white-space: nowrap; }
.fndetail .dgo:hover { border-color: var(--accent); }
.fndetail .drow { display: grid; grid-template-columns: 1fr auto; gap: .5rem; align-items: baseline;
  padding: .26rem .4rem; border-radius: 6px; cursor: pointer; font-size: .78rem; }
.fndetail .drow:hover { background: var(--hover); }
.fndetail .drow .dn { min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--ink); }
.fndetail .drow .dc { color: var(--muted); font-variant-numeric: tabular-nums; }
.fndetail .dnone { color: var(--muted); font-size: .74rem; padding: .2rem .4rem; }
svg { width: 100%; height: 100%; display: block; }
.edge { fill: none; stroke: var(--edge); stroke-width: 1; opacity: .5; }
.edge.hi { stroke: var(--accent); stroke-width: 2; opacity: 1; }
.elabel { fill: var(--muted); font-size: 9.5px; text-anchor: middle; pointer-events: none;
  paint-order: stroke; stroke: var(--bg); stroke-width: 3px; stroke-linejoin: round;
  font-variant-numeric: tabular-nums; }
.edge.fresh { stroke: var(--accent); opacity: .95; }
.node { cursor: pointer; }
.node .card { fill: var(--panel); stroke: var(--border); stroke-width: 1; }
.node:hover .card { stroke: var(--edge); }
.node .track, .node .gauge { fill: var(--line); }
.node .bar { fill: var(--accent); }
.node.dim { opacity: .25; }
.node.search-dim { opacity: .12; }
.node.absent { opacity: .16; }
.node.hotter .card { stroke: var(--accent); stroke-width: 2.5; }
/* A/B diff (baseline vs current): regression red, improvement green. */
.node.grew .card { stroke: var(--fail); stroke-width: 3; }
.node.shrank .card { stroke: var(--pass); stroke-width: 3; }
.node.newfn .card { stroke: var(--accent); stroke-width: 2.5; stroke-dasharray: 5 3; }
.node.removed .card { stroke: var(--pass); stroke-width: 2; stroke-dasharray: 4 4; }
.node.removed { opacity: .4; }
.edge.gone { stroke-dasharray: 5 4; opacity: .4; }
.edge.critical { stroke: var(--accent); stroke-width: 3.5; opacity: 1; }
.node.critical .card { stroke: var(--accent); stroke-width: 3; }
.node.sel .card { stroke: var(--accent); stroke-width: 2.5; }
.node.pruned, .edge.pruned, .elabel.pruned { display: none; }
/* The overflow menu. Everything that is set once and left alone lives here, so
   the bar keeps only what a reading loop touches: which view, which dimension,
   and search. */
#menu { position: fixed; z-index: 25; background: var(--panel); border: 1px solid var(--border);
  border-radius: 11px; padding: .4rem; min-width: 19rem; max-width: min(23rem, 94vw);
  box-shadow: 0 14px 44px rgba(20,14,10,.26); }
#menu[hidden] { display: none; }
#menu .mfirst { border-bottom: 1px solid var(--border); border-radius: 7px 7px 0 0;
  margin-bottom: .1rem; }
#menu .mh { font-size: .66rem; text-transform: uppercase; letter-spacing: .07em;
  color: var(--muted); font-weight: 700; padding: .5rem .55rem .25rem; }
#menu .mh.off { display: none; }
#menu .mrow { display: flex; align-items: center; gap: .6rem; width: 100%; text-align: left;
  background: transparent; border: 0; border-radius: 7px; padding: .42rem .55rem;
  color: var(--ink); font: inherit; font-size: .85rem; cursor: pointer; }
#menu button.mrow:hover, #menu label.mrow:hover { background: var(--hover); }
#menu .mrow.mstatic { cursor: default; }
#menu .mrow.mstatic:hover { background: transparent; }
#menu .mrow.on { background: var(--sel); }
#menu .mrow.off { display: none; }
#menu .mi { width: 1.15rem; text-align: center; flex: none; color: var(--muted); }
#menu .mrow.on .mi { color: var(--accent); }
#menu .ml { flex: 1; min-width: 0; display: flex; flex-direction: column; line-height: 1.25; }
#menu .ml em { font-style: normal; font-size: .73rem; color: var(--muted); margin-top: .05rem; }
#menu kbd { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .7rem;
  background: var(--bg); border: 1px solid var(--border); border-radius: 4px;
  padding: .04rem .3rem; color: var(--muted); flex: none; }
#menu select { background: var(--bg); color: var(--ink); border: 1px solid var(--border);
  border-radius: 6px; padding: .16rem .3rem; font-size: .78rem; flex: none; }
#menu .mseg { display: inline-flex; gap: 2px; padding: 2px; background: var(--bg);
  border: 1px solid var(--border); border-radius: 7px; flex: none; }
#menu .mseg button { border: 0; background: transparent; color: var(--muted); cursor: pointer;
  border-radius: 5px; padding: .16rem .42rem; font-size: .76rem; }
#menu .mseg button.on { background: var(--accent-container); color: var(--on-accent-container); }

/* Hover explanations for the header controls. Positioned by script from a body
   level element: the header clips its own overflow, so a bubble anchored inside
   it would be cut off. */
#htip { position: fixed; z-index: 30; background: var(--ink); color: var(--bg);
  padding: .3rem .55rem; border-radius: 6px; font-size: .74rem; line-height: 1.35;
  max-width: 17rem; pointer-events: none; box-shadow: 0 6px 20px rgba(20,14,10,.22); }
#htip[hidden] { display: none; }
#htip::before { content: ""; position: absolute; top: -4px; left: var(--ax, 50%);
  margin-left: -4px; border-left: 4px solid transparent; border-right: 4px solid transparent;
  border-bottom: 4px solid var(--ink); }

/* Keyboard help. The page has nine shortcuts; without this they are folded
   into tooltips nobody opens. */
#help { position: fixed; inset: 0; z-index: 20; background: rgba(20,14,10,.45);
  display: flex; align-items: center; justify-content: center; }
#help[hidden] { display: none; }
#help .card { background: var(--panel); border: 1px solid var(--border); border-radius: 12px;
  padding: 1rem 1.2rem; min-width: 20rem; max-width: 90vw; max-height: 80vh; overflow: auto;
  box-shadow: 0 12px 40px rgba(20,14,10,.28); }
#help .hh { font-size: .78rem; text-transform: uppercase; letter-spacing: .06em;
  color: var(--muted); margin-bottom: .6rem; }
#help dl { display: grid; grid-template-columns: auto 1fr; gap: .38rem .9rem; margin: 0; }
#help dt { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .78rem;
  background: var(--bg); border: 1px solid var(--border); border-radius: 5px;
  padding: .05rem .4rem; justify-self: start; color: var(--ink); }
#help dd { margin: 0; font-size: .84rem; color: var(--ink); }
#help .hf { margin-top: .9rem; font-size: .74rem; color: var(--muted); }
.htool { padding: .26rem .55rem; border: 1px solid var(--border); border-radius: 999px;
  background: var(--bg); color: var(--ink); font-size: .78rem; cursor: pointer; }
.htool.on { background: var(--accent); color: var(--on-accent); border-color: var(--accent); }
select.htool { padding: .2rem .35rem; }
.node text { fill: var(--ink); font-size: 12px; font-weight: 500; pointer-events: none; }
.node .sub { font-size: 10px; font-weight: 400; fill: var(--muted); }
.node .pv { font-size: 12px; font-weight: 600; font-variant-numeric: tabular-nums; }
#tip { position: absolute; pointer-events: none; background: var(--panel);
  border: 1px solid var(--border); border-radius: 12px; padding: .55rem .7rem;
  font-size: .82rem; box-shadow: var(--shadow); max-width: 22rem; display: none; }
#tip h3 { margin: 0 0 .3rem; font-size: .85rem; }
#tip .row { display: flex; justify-content: space-between; gap: 1rem; color: var(--muted); }
#tip .bar { height: 6px; border-radius: 3px; background: var(--accent); margin-top: 2px; }
#tip .cause { margin-top: .35rem; }
.legend { position: absolute; top: 3.4rem; right: .8rem; font-size: .74rem;
  color: var(--muted); background: var(--panel); border: 1px solid var(--border);
  border-radius: 12px; padding: .4rem .6rem; max-width: 20rem; }
#nav { position: absolute; left: 0; right: 0; bottom: 0; display: none; gap: .55rem;
  align-items: center; padding: .45rem .8rem; background: var(--panel); border-top: 1px solid var(--border); }
#nav.show { display: flex; }
#nav button { border: 1px solid var(--border); background: var(--bg); color: var(--ink);
  border-radius: 999px; cursor: pointer; padding: .25rem .5rem; font-size: .8rem; line-height: 1; }
#nav button:hover { border-color: var(--accent); }
#nav .pills { display: flex; gap: 3px; flex: 1; align-items: center; }
#nav .pill { flex: 1; min-width: 4px; max-width: 44px; height: 22px; border-radius: 3px;
  cursor: pointer; opacity: .5; border: 2px solid transparent; }
#nav .pill:hover { opacity: .85; }
#nav .pill.active { opacity: 1; border-color: var(--ink); }
#nav .tog { font-size: .78rem; color: var(--muted); display: inline-flex; align-items: center;
  gap: .3rem; cursor: pointer; user-select: none; }
#nav .live { display: inline-flex; align-items: center; gap: .35rem; }
#nav .live .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--idle); }
#nav .live.on .dot { background: var(--live); box-shadow: 0 0 0 3px rgba(46,194,106,.22); }
#nav .label { font-size: .76rem; color: var(--muted); font-variant-numeric: tabular-nums; white-space: nowrap; }
@media (prefers-reduced-motion: no-preference) {
  .node.hotter .card { animation: hotpulse 1.2s ease-in-out infinite; }
}
@keyframes hotpulse { 0%, 100% { stroke-opacity: 1; } 50% { stroke-opacity: .4; } }
__APM_STYLE__</style>
</head>
<body>
<header>
  <img class="logo" src="__LOGO__" alt="" width="20" height="21">
  <h1>__TITLE__</h1>
  <span class="meta" id="meta"></span>
  <span class="spacer"></span>
  <div id="viewbar"></div>
  <div id="metricbar"></div>
  <button id="flamebtn" hidden></button>
  <button id="sqlbtn" hidden></button>
  <button id="srcbtn" hidden></button>
  <button id="chkbtn" hidden></button>
  <input id="search" type="search" placeholder="Find a function…" autocomplete="off">
  <button id="menubtn" class="htool" aria-label="Menu" data-tip="Graph tools, appearance and shortcuts"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M4 12h16M4 17h16"/></svg></button>
</header>
<aside id="sidebar">
  <div class="side-head"><span class="t">Functions</span><button id="groupbtn" aria-label="Group" data-tip="Group the list under its classes and namespaces"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="4" width="7" height="7" rx="1.5"/><rect x="13" y="4" width="7" height="7" rx="1.5"/><rect x="4" y="13" width="7" height="7" rx="1.5"/><rect x="13" y="13" width="7" height="7" rx="1.5"/></svg></button>
    <span class="scope" id="scopeseg" role="group" aria-label="Cost"><button data-scope="self" data-tip="A function's own cost, without its callees (exclusive)">Self</button><button data-scope="incl" data-tip="A function's cost with everything it calls (inclusive)">Inclusive</button></span></div>
  <div class="sort" id="sortlabel"></div>
  <ul id="fnlist"></ul>
</aside>
<div id="dragger" title="Drag to resize the list"></div>
<div id="stage"><svg id="svg"><g id="viewport"></g></svg><div id="flame"></div><div id="sqlpanel"></div><div id="srcpanel"></div><div id="chkpanel"></div></div>
<div id="tip"></div>
<div id="htip" hidden></div>
__APM_PANEL__<div id="menu" hidden>
  <button id="resetbtn" class="mrow mfirst"><span class="mi"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 12a8 8 0 1 0 2.4-5.7M4 4v4h4"/></svg></span><span class="ml">Start over<em>Call graph, time, nothing selected or filtered</em></span><kbd>r</kbd></button>
  <div class="mh" id="mh-graph">Graph</div>
  <button id="fitbtn" class="mrow"><span class="mi"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"/></svg></span><span class="ml">Fit in view<em>Frame it again if zoom or pan lost it</em></span><kbd>0</kbd></button>
  <button id="critbtn" class="mrow"><span class="mi"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M13 3L5 14h6l-1 7 8-11h-6z"/></svg></span><span class="ml">Critical path<em>The heaviest chain, entry point down</em></span><kbd>p</kbd></button>
  <label class="mrow" id="prunerow"><span class="mi"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 6h16M7 12h10M10 18h4"/></svg></span><span class="ml">Hide the small<em>Drop functions under a share of the run</em></span>
    <select id="prune">
      <option value="0">all</option>
      <option value="1">≥1%</option>
      <option value="5">≥5%</option>
      <option value="10">≥10%</option>
    </select>
  </label>
  <div class="mh">Appearance</div>
  <div class="mrow mstatic"><span class="mi"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="8"/><path d="M12 4a8 8 0 0 1 0 16z" fill="currentColor"/></svg></span><span class="ml">Theme</span>
    <span class="mseg" id="themeseg"></span>
  </div>
  <div class="mh" id="mh-help">Help</div>
  <button id="helpbtn" class="mrow"><span class="mi"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.7.3-1 .9-1 1.7M12 17h.01"/></svg></span><span class="ml">Keyboard shortcuts</span><kbd>?</kbd></button>
</div>
<div id="help" hidden><div class="card">
  <div class="hh">Keyboard</div>
  <dl id="helplist"></dl>
  <div class="hf">Esc or ? to close</div>
</div></div>
<div class="legend" id="legend">Sampled — cost is a share of samples, not instrumented time. Hotter = more self time.</div>
<div id="nav">
  <button id="prev" title="Previous frame (←)" aria-label="Previous frame"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M15 6l-6 6 6 6"/></svg></button>
  <button id="next" title="Next frame (→)" aria-label="Next frame"><svg class="ic" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6"/></svg></button>
  <div class="pills" id="pills"></div>
  <label class="tog"><input type="checkbox" id="diff"> Δ vs previous</label>
  <button class="live" id="live" title="Follow latest / pause (l)"><span class="dot"></span><span id="livetext">live</span></button>
  <span class="label" id="navlabel"></span>
</div>
<script>
const DATA = __DATA_JSON__;
// Outlined icons (24 grid, 2px stroke, currentColor) in place of emoji, so they
// follow the theme and read the same on every system; the page stays offline.
const ICONS = {
  graph: '<circle cx="6" cy="6" r="2.5"/><circle cx="18" cy="6" r="2.5"/><circle cx="12" cy="18" r="2.5"/><path d="M7.6 8.1l3 7.6M16.4 8.1l-3 7.6"/>',
  flame: '<path d="M12 3c.8 3 5 5.2 5 10a5 5 0 0 1-10 0c0-2.6 1.4-4.2 2.6-5.2.2 1.6.9 2.6 1.9 3.2.6-3-.4-5.6.5-8z"/>',
  db: '<ellipse cx="12" cy="6" rx="7" ry="3"/><path d="M5 6v12c0 1.7 3.1 3 7 3s7-1.3 7-3V6M5 12c0 1.7 3.1 3 7 3s7-1.3 7-3"/>',
  code: '<path d="M9 7l-5 5 5 5M15 7l5 5-5 5"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  timer: '<circle cx="12" cy="13.5" r="7"/><path d="M12 10v3.5l2.5 2M10 3h4"/>',
  memory: '<rect x="6" y="6" width="12" height="12" rx="2"/><path d="M9.5 3v3M14.5 3v3M9.5 18v3M14.5 18v3M3 9.5h3M3 14.5h3M18 9.5h3M18 14.5h3"/>',
  drop: '<path d="M12 3.5c3 4 6 7.2 6 10.5a6 6 0 0 1-12 0c0-3.3 3-6.5 6-10.5z"/>',
  hourglass: '<path d="M7 3h10M7 21h10M8 3v3.5l4 5.5-4 5.5V21M16 3v3.5L12 12l4 5.5V21"/>',
  outbound: '<path d="M7 17L17 7M9 7h8v8"/>',
  globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c2.4 2.5 3.5 5.5 3.5 9s-1.1 6.5-3.5 9c-2.4-2.5-3.5-5.5-3.5-9s1.1-6.5 3.5-9z"/>',
  hash: '<path d="M5 9h14M5 15h14M10.5 4l-2 16M15.5 4l-2 16"/>',
};
function icon(name) { return '<svg class="ic" viewBox="0 0 24 24" aria-hidden="true">' + ICONS[name] + '</svg>'; }
// The page's chrome — theme, header hints, the ☰ menu — set up apart from the
// call graph and before it. The graph script stops early on a page with no
// profile yet (a monitor page before its first capture), and the menu and the
// theme have to work there too.
const PAGE = (function() {
  // Theme is a reader preference, not a property of one capture, so it is
  // stored per site rather than per page like the rest of the view state.
  const THEME_KEY = 'elephc-callgraph:theme';
  const hooks = { theme: [], menu: [] };
  let graph = false;

  // --- theme: follow the system, or force light / dark ---
  // The palette already answers to `prefers-color-scheme`; this is the missing
  // half, an explicit choice. `data-theme` on the root is what the token blocks
  // key off, and it beats the media query in both directions.
  const THEMES = [
    { key: 'system', label: 'Auto',  tip: 'Follow the system setting' },
    { key: 'light',  label: 'Light', tip: 'Always light' },
    { key: 'dark',   label: 'Dark',  tip: 'Always dark' },
  ];
  const themeseg = document.getElementById('themeseg');
  let themeIdx = 0;
  const themeEls = THEMES.map((t, i) => {
    const b = document.createElement('button');
    b.textContent = t.label;
    b.title = t.tip;
    b.addEventListener('click', () => { themeIdx = i; applyTheme(); });
    themeseg.appendChild(b);
    return b;
  });
  function applyTheme() {
    const t = THEMES[themeIdx];
    if (t.key === 'system') document.documentElement.removeAttribute('data-theme');
    else document.documentElement.setAttribute('data-theme', t.key);
    themeEls.forEach((b, i) => b.classList.toggle('on', i === themeIdx));
    // The ramp's cold end moved, so everything painted from it is restated.
    hooks.theme.forEach(fn => fn());
    try { localStorage.setItem(THEME_KEY, t.key); } catch (e) {}
  }

  // --- hover explanations for the header controls ---
  const htip = document.getElementById('htip');
  let htipFor = null;
  // Named apart from the graph's own node tooltip (showTip/hideTip): two
  // functions of the same name in one scope means the later one silently wins,
  // which is how the node tooltips stopped working.
  function showHint(el) {
    const text = el.dataset.tip;
    if (!text) return;
    htipFor = el;
    htip.textContent = text;
    htip.hidden = false;
    const r = el.getBoundingClientRect(), t = htip.getBoundingClientRect();
    // Keep the bubble on screen, and point the arrow at the control even when
    // the bubble had to slide sideways to fit.
    let left = r.left + r.width / 2 - t.width / 2;
    left = Math.max(6, Math.min(left, window.innerWidth - t.width - 6));
    htip.style.left = left + 'px';
    htip.style.top = (r.bottom + 7) + 'px';
    htip.style.setProperty('--ax', (r.left + r.width / 2 - left) + 'px');
  }
  function hideHint() { htip.hidden = true; htipFor = null; }
  // Delegated, so controls built later (the two segmented bars) are covered.
  document.querySelector('header').addEventListener('mouseover', ev => {
    const el = ev.target.closest('[data-tip]');
    if (el) showHint(el); else hideHint();
  });
  document.querySelector('header').addEventListener('mouseleave', hideHint);
  window.addEventListener('blur', hideHint);

  // --- the overflow menu ---
  const menu = document.getElementById('menu'), menubtn = document.getElementById('menubtn');
  function menuOpen() { return !menu.hasAttribute('hidden'); }
  function setMenu(on) {
    menu.toggleAttribute('hidden', !on);
    menubtn.classList.toggle('on', on);
    if (!on) return;
    hideHint();
    // Anchored under the button, pulled left to stay on screen.
    const r = menubtn.getBoundingClientRect(), m = menu.getBoundingClientRect();
    menu.style.left = Math.max(6, Math.min(r.right - m.width, window.innerWidth - m.width - 6)) + 'px';
    menu.style.top = (r.bottom + 6) + 'px';
    hooks.menu.forEach(fn => fn());
  }
  menubtn.addEventListener('click', ev => { ev.stopPropagation(); setMenu(!menuOpen()); });
  // A click inside acts and keeps the menu up, except for the one-shot actions.
  menu.addEventListener('click', ev => {
    if (ev.target.closest('#fitbtn, #helpbtn, #resetbtn')) setMenu(false);
  });
  document.addEventListener('click', ev => {
    if (menuOpen() && !menu.contains(ev.target) && ev.target !== menubtn) setMenu(false);
  });
  // Following the system means following it as it changes, not only at load.
  if (window.matchMedia) {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const onSystemTheme = () => { if (THEMES[themeIdx].key === 'system') applyTheme(); };
    if (mq.addEventListener) mq.addEventListener('change', onSystemTheme);
  }

  // applyTheme reads THEMES and htip, so its first call sits after BOTH blocks
  // have declared them — a `const` is unreachable before its declaration runs,
  // and one such call throwing here would take the whole page's init with it.
  try {
    const savedTheme = localStorage.getItem(THEME_KEY);
    const found = THEMES.findIndex(t => t.key === savedTheme);
    if (found >= 0) themeIdx = found;
  } catch (e) {}
  applyTheme();

  // Esc closes the menu; the graph's own key handler does it when it runs.
  window.addEventListener('keydown', ev => {
    if (!graph && ev.key === 'Escape' && menuOpen()) setMenu(false);
  });
  return {
    menuOpen, setMenu,
    graphReady() { graph = true; },
    onTheme(fn) { hooks.theme.push(fn); },
    onMenuOpen(fn) { hooks.menu.push(fn); },
  };
})();
(function() {
  const FRAMES = DATA.frames || [];
  const metaEl = document.getElementById('meta');
  if (!FRAMES.length || !FRAMES.some(f => f.nodes.length)) {
    metaEl.textContent = 'no samples';
    // Nothing to start over, frame or explain yet: the menu keeps the theme.
    document.querySelectorAll('#resetbtn, #mh-graph, #fitbtn, #critbtn, #prunerow, #mh-help, #helpbtn')
      .forEach(el => el.classList.add('off'));
    return;
  }
  if (DATA.exact) {
    const legend = document.getElementById('legend');
    if (legend) legend.textContent = 'Exact (--instrument) — pick a dimension above (or press m to cycle). Hotter = more of it.';
  }

  const NS = 'http://www.w3.org/2000/svg';
  const W = 236, H = 52, GAPX = 32, GAPY = 74;
  const vp = document.getElementById('viewport'), tip = document.getElementById('tip');
  const nav = document.getElementById('nav'), pills = document.getElementById('pills');
  const navlabel = document.getElementById('navlabel'), liveBtn = document.getElementById('live');
  const diffBox = document.getElementById('diff'), metricbar = document.getElementById('metricbar');
  const stage = document.getElementById('stage'), svg = document.getElementById('svg');
  const fnlist = document.getElementById('fnlist'), sortlabel = document.getElementById('sortlabel');
  const scopeEls = [...document.querySelectorAll('#scopeseg button')];
  const searchInput = document.getElementById('search'), dragger = document.getElementById('dragger');
  const critbtn = document.getElementById('critbtn'), pruneSel = document.getElementById('prune');
  const flame = document.getElementById('flame'), flamebtn = document.getElementById('flamebtn');
  const sqlpanel = document.getElementById('sqlpanel'), sqlbtn = document.getElementById('sqlbtn');
  const groupbtn = document.getElementById('groupbtn');
  const srcpanel = document.getElementById('srcpanel'), srcbtn = document.getElementById('srcbtn');
  const chkpanel = document.getElementById('chkpanel'), chkbtn = document.getElementById('chkbtn');

  // Rebuildable graph state: recomputed when a live update grows the union.
  let NAMES, nameId, uEdge, EDGES, xy, nodeEls, edgeEls, frameNode, frameEdge;
  let cur = FRAMES.length - 1, diffOn = false, follow = DATA.live, reloadTimer = null, curNodes = null;
  let selected = -1, sbw = 0, critOn = false, pruneT = 0;
  // Which list the open function shows, kept from one function to the next.
  let detailTab = 'callees';
  let flameOn = false, flameRoot = -1, sqlOn = false, grouped = false, srcOn = false, chkOn = false;
  const collapsed = new Set();
  const LINES = DATA.lines || null;
  const ASSERTS = DATA.asserts || [];
  const hasQueries = FRAMES.some(f => (f.queries || []).length > 0);
  const hasGroups = FRAMES.some(f => f.nodes.some(n => n.name.indexOf('::') > 0 || n.name.indexOf('\\') > 0));
  let scale = 1, tx = 20, ty = 20, panning = false, psx = 0, psy = 0;
  // Whether the pointer travelled far enough between press and release for it
  // to be a drag rather than a click.
  let dragMoved = false, dragX = 0, dragY = 0;
  // Which dimension colors the graph: 'time' (self time share) or 'mem' (self
  // allocation share). Memory is only available under --instrument.
  let metric = 'time';
  // Which cost the list ranks and the boxes show: a function's own ('self',
  // exclusive) or its own plus its callees' ('incl', inclusive), as Blackfire
  // splits them. The heat stays on the own cost whatever the choice: ranked by
  // inclusive cost the entry point is always first, and colour would stop
  // pointing at the function that does the work.
  let scope = 'self';
  const hasMem = !!DATA.exact && FRAMES.some(f => f.totalAllocs > 0);
  const hasIo = !!DATA.exact && FRAMES.some(f => f.nodes.some(n => (n.ioInclN || 0) > 0));
  const hasRet = !!DATA.exact && FRAMES.some(f => f.nodes.some(n => (n.retInclN || 0) !== 0));
  const hasWait = !!DATA.exact && FRAMES.some(f => f.nodes.some(n => (n.waitInclN || 0) > 0));
  const hasNetwork = !!DATA.exact && FRAMES.some(f => f.nodes.some(n => (n.networkInclN || 0) > 0));
  const hasNetworkWait = !!DATA.exact && FRAMES.some(f => f.nodes.some(n => (n.networkWaitInclN || 0) > 0));
  // Selectable cost dimensions (Blackfire-style). Availability depends on data.
  const METRICS = [
    { key: 'time',  label: 'Time',     icon: 'timer', on: !!DATA.exact },
    { key: 'mem',   label: 'Memory',   icon: 'memory', on: hasMem },
    { key: 'ret',   label: 'Retained', icon: 'drop', on: hasRet },
    { key: 'wait',  label: 'Wait',     icon: 'hourglass', on: hasWait },
    { key: 'io',    label: 'SQL',      icon: 'db', on: hasIo },
    { key: 'network', label: 'Network', icon: 'outbound', on: hasNetwork },
    { key: 'networkWait', label: 'Net wait', icon: 'globe', on: hasNetworkWait },
    { key: 'calls', label: 'Calls',    icon: 'hash', on: !!DATA.exact },
  ];
  // Per-frame scales used to color the io / calls / retained dimensions.
  let frameIoTotal = 0, frameNetworkTotal = 0, frameCallMax = 1, frameRetMax = 1;
  function computeScales(fN) {
    frameIoTotal = 0; frameNetworkTotal = 0; frameCallMax = 1; frameRetMax = 1;
    fN.forEach(n => { frameIoTotal += (n.ioExclN || 0); frameNetworkTotal += (n.networkExclN || 0); if ((n.calls || 0) > frameCallMax) frameCallMax = n.calls;
      const r = Math.abs(n.retInclN || 0); if (r > frameRetMax) frameRetMax = r; });
  }
  function nodeShare(n) {
    if (metric === 'mem') return n.allocExcl || 0;
    if (metric === 'io') return frameIoTotal ? 100 * (n.ioExclN || 0) / frameIoTotal : 0;
    if (metric === 'network') return frameNetworkTotal ? 100 * (n.networkExclN || 0) / frameNetworkTotal : 0;
    if (metric === 'networkWait') return waitShare(n.networkWaitExclN);
    if (metric === 'calls') return frameCallMax ? 100 * (n.calls || 0) / frameCallMax : 0;
    // Retained is signed; only net GROWTH is "hot" (a net release is cold).
    if (metric === 'ret') return frameRetMax ? 100 * Math.max(0, n.retExclN || 0) / frameRetMax : 0;
    // Wait shares the TIME denominator, so a wait bar and a time bar compare
    // directly: "of the whole run, this much was spent blocked here".
    if (metric === 'wait') return waitShare(n.waitExclN);
    return n.excl;
  }
  // Nanoseconds as a share of the frame's total run time.
  function waitShare(ns) {
    const total = (FRAMES[cur] && FRAMES[cur].total) || 0;
    return total ? 100 * (ns || 0) / total : 0;
  }
  // A function's own CPU time: self time minus the part it spent blocked.
  function nonDbNs(n) {
    const total = (FRAMES[cur] && FRAMES[cur].total) || 0;
    return Math.max(0, Math.round(n.excl / 100 * total) - (n.waitExclN || 0));
  }
  // Inclusive share of the current metric — used to weight edges and to find
  // the critical path (which follows the heaviest inclusive child).
  function nodeInclShare(n) {
    if (metric === 'mem') return n.allocIncl || 0;
    if (metric === 'io') return frameIoTotal ? 100 * (n.ioInclN || 0) / frameIoTotal : 0;
    if (metric === 'network') return frameNetworkTotal ? 100 * (n.networkInclN || 0) / frameNetworkTotal : 0;
    if (metric === 'networkWait') return waitShare(n.networkWaitInclN);
    if (metric === 'calls') return frameCallMax ? 100 * (n.calls || 0) / frameCallMax : 0;
    if (metric === 'ret') return frameRetMax ? 100 * Math.max(0, n.retInclN || 0) / frameRetMax : 0;
    if (metric === 'wait') return waitShare(n.waitInclN);
    return n.incl;
  }
  // Signed retained count, formatted with an explicit sign so a net release reads.
  function fmtSigned(v) { return (v > 0 ? '+' : '') + fmtK(Math.abs(v)).replace(/^/, v < 0 ? '-' : ''); }
  // The cost the list ranks by and the boxes print: own or inclusive, per the
  // Self / Inclusive switch. Calls have no inclusive form; they read the same.
  function scopedShare(n) { return scope === 'incl' ? nodeInclShare(n) : nodeShare(n); }
  function scopedValue(n) { return scope === 'incl' ? metricInclValue(n) : metricValue(n); }
  function scopedSub(n) { return scope === 'incl' ? metricSelfSub(n) : metricSub(n); }
  function metricInclValue(n) {
    if (metric === 'mem') return fmtK(n.allocInclN || 0);
    if (metric === 'io') return (n.ioInclN || 0) + ' q';
    if (metric === 'network') return (n.networkInclN || 0) + ' ops';
    if (metric === 'networkWait') return fmtNs(n.networkWaitInclN || 0);
    if (metric === 'calls') return fmtK(n.calls || 0);
    if (metric === 'ret') return fmtSigned(n.retInclN || 0);
    if (metric === 'wait') return fmtNs(n.waitInclN || 0);
    return n.incl.toFixed(1) + '%';
  }
  // Under the inclusive figure, the own one, so the two stay side by side.
  function metricSelfSub(n) {
    if (metric === 'mem') return 'self ' + fmtK(n.allocExclN || 0) + ' allocs';
    if (metric === 'io') return 'self ' + (n.ioExclN || 0) + ' q';
    if (metric === 'network') return 'self ' + (n.networkExclN || 0) + ' network ops';
    if (metric === 'networkWait') return 'self network wait ' + fmtNs(n.networkWaitExclN || 0);
    if (metric === 'calls') return 'incl ' + n.incl.toFixed(0) + '% time';
    if (metric === 'ret') return 'self ' + fmtSigned(n.retExclN || 0) + ' retained';
    if (metric === 'wait') return 'self wait ' + fmtNs(n.waitExclN || 0);
    return 'self ' + n.excl.toFixed(1) + '%';
  }
  function metricValue(n) {
    if (metric === 'mem') return fmtK(n.allocExclN || 0);
    if (metric === 'io') return (n.ioExclN || 0) + ' q';
    if (metric === 'network') return (n.networkExclN || 0) + ' ops';
    if (metric === 'networkWait') return fmtNs(n.networkWaitExclN || 0);
    if (metric === 'calls') return fmtK(n.calls || 0);
    if (metric === 'ret') return fmtSigned(n.retExclN || 0);
    if (metric === 'wait') return fmtNs(n.waitExclN || 0);
    return n.excl.toFixed(1) + '%';
  }
  function metricSub(n) {
    if (metric === 'mem') return 'incl ' + fmtK(n.allocInclN || 0) + ' allocs';
    if (metric === 'io') return 'incl ' + (n.ioInclN || 0) + ' q';
    if (metric === 'network') return 'incl ' + (n.networkInclN || 0) + ' network ops';
    if (metric === 'networkWait') return 'incl network wait ' + fmtNs(n.networkWaitInclN || 0);
    if (metric === 'calls') return 'self ' + n.excl.toFixed(1) + '% time';
    if (metric === 'ret') return 'incl ' + fmtSigned(n.retInclN || 0) + ' retained';
    if (metric === 'wait') return 'non-DB ' + fmtNs(nonDbNs(n)) + ' · incl wait ' + fmtNs(n.waitInclN || 0);
    return 'incl ' + n.incl.toFixed(0) + '%';
  }
  function nodeSub(n) {
    if (metric === 'mem') return fmtK(n.allocExclN || 0) + ' allocs · incl ' + n.allocIncl.toFixed(0) + '%';
    if (metric === 'io') return (n.ioExclN || 0) + ' queries';
    if (metric === 'network') return (n.networkExclN || 0) + ' network ops · incl ' + (n.networkInclN || 0);
    if (metric === 'networkWait') return 'network wait ' + fmtNs(n.networkWaitExclN || 0) + ' · incl ' + fmtNs(n.networkWaitInclN || 0);
    if (metric === 'calls') return (n.calls || 0) + ' calls';
    if (metric === 'ret') return fmtSigned(n.retExclN || 0) + ' retained · incl ' + fmtSigned(n.retInclN || 0);
    if (metric === 'wait') return 'wait ' + fmtNs(n.waitExclN || 0) + ' · non-DB ' + fmtNs(nonDbNs(n));
    return 'incl ' + n.incl.toFixed(1) + '% · self ' + n.excl.toFixed(1) + '%';
  }

  // --- elephc brand heat ramp ---
  // Only the COLD end follows the theme. On a dark ground the light ramp's pale
  // start made every quiet function a glaring near-white box — measured at
  // luminance 234 against a background of 20, nine of them at once — which reads
  // as alarm rather than calm. The hot stops stay exactly as they are: gold
  // through magenta is the signal, and it carries on either ground.
  const HEAT_COLD = { light: [0xf2,0xe9,0xe4], dark: [0x2e,0x26,0x20] };
  function isDark() {
    const forced = document.documentElement.getAttribute('data-theme');
    if (forced === 'dark') return true;
    if (forced === 'light') return false;
    return !!(window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches);
  }
  function heatStops() {
    const c = HEAT_COLD[isDark() ? 'dark' : 'light'];
    return [[0,c[0],c[1],c[2]],[0.08,0xff,0xd9,0x00],[0.25,0xff,0x8b,0x1b],[0.55,0xff,0x52,0x2c],[1,0xff,0x00,0x70]];
  }
  function heatRgb(excl) {
    const HEAT = heatStops();
    const t = Math.min(1, excl / 100 * 3);
    let i = 0; while (i < HEAT.length - 1 && t > HEAT[i+1][0]) i++;
    const a = HEAT[i], b = HEAT[Math.min(i+1, HEAT.length-1)];
    const f = (t - a[0]) / ((b[0] - a[0]) || 1);
    const ch = k => Math.round(a[k] + (b[k] - a[k]) * f);
    return [ch(1), ch(2), ch(3)];
  }
  function heat(excl) { const c = heatRgb(excl); return 'rgb(' + c[0] + ',' + c[1] + ',' + c[2] + ')'; }
  function heatA(excl, a) { const c = heatRgb(excl); return 'rgba(' + c[0] + ',' + c[1] + ',' + c[2] + ',' + a + ')'; }
  // A/B diff fill: pale→red as a function grew, pale→green as it shrank. `d` is
  // the change in the current metric's share (percentage points).
  function diffRgb(d) {
    const f = 0.2 + 0.8 * Math.min(1, Math.abs(d) / 20);
    const pale = [244, 240, 235], tgt = d > 0 ? [229, 72, 77] : [48, 163, 108];
    return pale.map((p, i) => Math.round(p + (tgt[i] - p) * f));
  }
  function diffFill(d) { const c = diffRgb(d); return 'rgb(' + c[0] + ',' + c[1] + ',' + c[2] + ')'; }
  function ink(excl) { const c = heatRgb(excl); return (0.299*c[0] + 0.587*c[1] + 0.114*c[2]) < 150 ? '#fff8f4' : '#201a17'; }
  function clip(s, n) { return s.length > n ? s.slice(0, n-1) + '…' : s; }
  // A function's name for a card: the method is what a reader looks for, so it
  // is kept whole. A long name first loses its namespace, then the middle of
  // its class; only a name that still does not fit is cut at the end. The full
  // name is the card's tooltip.
  function shortName(name, n) {
    if (name.length <= n) return name;
    const ci = name.indexOf('::');
    const owner = ci > 0 ? name.slice(0, ci) : name, member = ci > 0 ? name.slice(ci) : '';
    const s = owner.slice(owner.lastIndexOf('\\') + 1) + member;
    if (s.length <= n) return s;
    const keep = n - member.length - 1;
    if (member && keep >= 5) { const cls = s.slice(0, s.length - member.length);
      const head = Math.ceil(keep / 2), tail = keep - head;
      return cls.slice(0, head) + '…' + (tail ? cls.slice(-tail) : '') + member; }
    return clip(s, n);
  }
  // The heat as a gauge: how far up its track a function's own cost reaches,
  // on the same scale as its colour (a third of the run is full).
  // A card name is fitted to the room its value leaves, measured in the page's
  // own font: counting characters let a wide name (capitals, `::`) run into
  // the figure beside it.
  const measureCtx = document.createElement('canvas').getContext('2d');
  const cardFont = getComputedStyle(document.body).fontFamily;
  function textW(s, weight) { if (!measureCtx) return s.length * 7; measureCtx.font = weight + ' 12px ' + cardFont; return measureCtx.measureText(s).width; }
  function fitName(name, px) {
    for (let n = name.length; n > 8; n--) { const s = shortName(name, n); if (textW(s, 500) <= px) return s; }
    return shortName(name, 8);
  }
  function gaugePct(share) { return Math.max(0, Math.min(100, share * 3)); }
  function setGauge(rect, share, fill) {
    const track = H - 18, h = Math.max(3, track * gaugePct(share) / 100);
    rect.setAttribute('y', 9 + track - h); rect.setAttribute('height', h); rect.setAttribute('fill', fill);
  }
  function esc(s) { const d = document.createElement('div'); d.textContent = s; return d.innerHTML; }
  // esc() is for element content and leaves quotes alone; an attribute value
  // needs them escaped too or a name containing one would break out of it.
  function escAttr(s) { return esc(s).replace(/"/g, '&quot;'); }
  function fmtK(n) { return n >= 1e6 ? (n/1e6).toFixed(1) + 'M' : n >= 1e3 ? (n/1e3).toFixed(1) + 'k' : '' + n; }
  function fmtNs(ns) {
    if (ns >= 1e9) return (ns / 1e9).toFixed(2) + ' s';
    if (ns >= 1e6) return (ns / 1e6).toFixed(1) + ' ms';
    if (ns >= 1e3) return (ns / 1e3).toFixed(1) + ' µs';
    return ns + ' ns';
  }
  function totalLabel(total) { return DATA.exact ? fmtNs(total) : (total + ' samples'); }

  // --- (re)build the union, layout, and SVG skeleton from FRAMES ---
  // An edge's identity. Plain concatenation is not injective — `a -> bc` and
  // `ab -> c` both spell "abc" — so one edge could overwrite another in these
  // maps. The length prefix can only be split one way, and needs no escaping for
  // names that contain backslashes or colons, which PHP names routinely do.
  const edgeKey = (from, to) => from.length + '\u0000' + from + to;
  function computeFrameMaps() {
    frameNode = FRAMES.map(f => { const m = new Map(); f.nodes.forEach(n => m.set(n.name, n)); return m; });
    frameEdge = FRAMES.map(f => { const m = new Map(); f.edges.forEach(e => m.set(edgeKey(e.from, e.to), e.pct)); return m; });
  }
  function computeUnion() {
    nameId = new Map(); NAMES = [];
    const id = n => { if (!nameId.has(n)) { nameId.set(n, NAMES.length); NAMES.push(n); } return nameId.get(n); };
    uEdge = new Map();
    FRAMES.forEach(f => {
      f.nodes.forEach(n => id(n.name));
      f.edges.forEach(e => { const k = edgeKey(e.from, e.to); id(e.from); id(e.to);
        if (!uEdge.has(k)) uEdge.set(k, {from: id(e.from), to: id(e.to), key: k, count: e.count});
        else if (uEdge.get(k).count == null && e.count != null) uEdge.get(k).count = e.count; });
    });
    EDGES = [...uEdge.values()];
  }
  function computeLayout() {
    const incoming = NAMES.map(() => []), outgoing = NAMES.map(() => []);
    EDGES.forEach(e => { outgoing[e.from].push(e.to); incoming[e.to].push(e.from); });
    const layer = NAMES.map(() => 0), seen = new Array(NAMES.length).fill(0);
    function dfs(v, stack) { seen[v] = 1; outgoing[v].forEach(w => { if (stack.has(w)) return; if (layer[w] < layer[v] + 1) layer[w] = layer[v] + 1; stack.add(w); dfs(w, stack); stack.delete(w); }); }
    NAMES.forEach((_, v) => { if (incoming[v].length === 0) dfs(v, new Set([v])); });
    NAMES.forEach((_, v) => { if (!seen[v]) dfs(v, new Set([v])); });
    const maxLayer = Math.max(0, ...layer);
    const layers = Array.from({length: maxLayer + 1}, () => []);
    NAMES.forEach((_, v) => layers[layer[v]].push(v));
    const pos = NAMES.map(() => 0);
    layers.forEach(L => L.forEach((v, i) => pos[v] = i));
    function bary(v, adj) { const ns = adj[v]; if (!ns.length) return pos[v]; return ns.reduce((s, n) => s + pos[n], 0) / ns.length; }
    for (let p = 0; p < 4; p++) {
      for (let l = 1; l <= maxLayer; l++) { layers[l].sort((a, b) => bary(a, incoming) - bary(b, incoming)); layers[l].forEach((v, i) => pos[v] = i); }
      for (let l = maxLayer - 1; l >= 0; l--) { layers[l].sort((a, b) => bary(a, outgoing) - bary(b, outgoing)); layers[l].forEach((v, i) => pos[v] = i); }
    }
    const widest = Math.max(1, ...layers.map(L => L.length));
    xy = NAMES.map(() => ({x: 0, y: 0}));
    layers.forEach((L, l) => { const rowW = L.length * (W + GAPX); L.forEach((v, i) => { xy[v] = { x: i * (W + GAPX) + (widest * (W+GAPX) - rowW)/2, y: l * (H + GAPY) }; }); });
  }
  function buildSkeleton() {
    while (vp.firstChild) vp.removeChild(vp.firstChild);
    edgeEls = EDGES.map(e => {
      const a = xy[e.from], b = xy[e.to];
      const p = document.createElementNS(NS, 'path');
      const x1 = a.x + W/2, y1 = a.y + H, x2 = b.x + W/2, y2 = b.y;
      p.setAttribute('d', 'M' + x1 + ',' + y1 + ' C' + x1 + ',' + ((y1+y2)/2) + ' ' + x2 + ',' + ((y1+y2)/2) + ' ' + x2 + ',' + y2);
      p.setAttribute('class', 'edge');
      if (e.count != null) { const ti = document.createElementNS(NS, 'title'); ti.textContent = NAMES[e.from] + ' → ' + NAMES[e.to] + ' ×' + e.count; p.appendChild(ti); }
      vp.appendChild(p);
      // The exact call count, printed on the edge (midpoint of the bezier).
      if (e.count != null) {
        const lbl = document.createElementNS(NS, 'text');
        lbl.setAttribute('class', 'elabel');
        lbl.setAttribute('x', (x1 + x2) / 2);
        lbl.setAttribute('y', (y1 + y2) / 2 + 3);
        lbl.textContent = '×' + e.count;
        vp.appendChild(lbl);
        p._lbl = lbl;
      }
      return p;
    });
    nodeEls = NAMES.map((name, v) => {
      const g = document.createElementNS(NS, 'g');
      g.setAttribute('class', 'node');
      g.setAttribute('transform', 'translate(' + xy[v].x + ',' + xy[v].y + ')');
      // A card on the surface, as in the Functions list: the heat as a stripe
      // on its left, the name and its other figures, the chosen cost (self or
      // inclusive) on the right over a bar.
      const el = (tag, attrs) => { const e = document.createElementNS(NS, tag);
        Object.keys(attrs).forEach(k => e.setAttribute(k, attrs[k])); g.appendChild(e); return e; };
      el('rect', { class: 'card', width: W, height: H, rx: 10 });
      // The heat is a gauge: a grey track the full height of the card, filled
      // from the bottom as far as the heat reaches and in its colour, so a quiet
      // function still shows a visible (short, pale) mark on the white card.
      el('rect', { class: 'gauge', x: 7, y: 9, width: 4, height: H - 18, rx: 2 });
      el('rect', { class: 'stripe', x: 7, y: H - 9, width: 4, height: 0, rx: 2 });
      const t1 = el('text', { class: 'nm', x: 19, y: 22 });
      t1.textContent = shortName(name, 26);
      const ti = document.createElementNS(NS, 'title'); ti.textContent = name; g.appendChild(ti);
      el('text', { class: 'sub', x: 19, y: 39 });
      el('text', { class: 'pv', x: W - 11, y: 22, 'text-anchor': 'end' });
      el('rect', { class: 'track', x: W - 51, y: 33, width: 40, height: 4, rx: 2 });
      el('rect', { class: 'bar', x: W - 51, y: 33, width: 0, height: 4, rx: 2 });
      g.addEventListener('mousemove', ev => showTip(ev, name));
      g.addEventListener('mouseleave', hideTip);
      g.addEventListener('click', () => selectNode(v, false));
      vp.appendChild(g); return g;
    });
  }
  function rebuild() { selected = -1; computeUnion(); computeLayout(); buildSkeleton(); }

  // --- tooltip / highlight / search ---
  function showTip(ev, name) {
    if (!curNodes) return;
    const n = curNodes.get(name);
    let html = '<h3>' + esc(name) + '</h3>';
    if (!n) { html += '<div class="row"><span>not sampled in this frame</span></div>'; }
    else {
      html += '<div class="row"><span>time incl</span><b>' + n.incl.toFixed(1) + '%</b></div>';
      html += '<div class="row"><span>time self</span><b>' + n.excl.toFixed(1) + '%</b></div>';
      if (n.calls != null) html += '<div class="row"><span>calls (exact)</span><b>' + n.calls + '</b></div>';
      if (hasMem) {
        html += '<div class="row"><span>allocs incl</span><b>' + (n.allocInclN || 0) + '</b></div>';
        html += '<div class="row"><span>allocs self</span><b>' + (n.allocExclN || 0) + '</b></div>';
      }
      if (hasIo) {
        html += '<div class="row"><span>queries incl</span><b>' + (n.ioInclN || 0) + '</b></div>';
        html += '<div class="row"><span>queries self</span><b>' + (n.ioExclN || 0) + '</b></div>';
      }
      if (hasNetwork) {
        html += '<div class="row"><span>network operations incl</span><b>' + (n.networkInclN || 0) + '</b></div>';
        html += '<div class="row"><span>network operations self</span><b>' + (n.networkExclN || 0) + '</b></div>';
      }
      if (hasNetworkWait) {
        html += '<div class="row"><span>network wait incl</span><b>' + fmtNs(n.networkWaitInclN || 0) + '</b></div>';
        html += '<div class="row"><span>network wait self</span><b>' + fmtNs(n.networkWaitExclN || 0) + '</b></div>';
      }
      n.causes.forEach(c => { html += '<div class="cause">' + esc(c.name) + ' — ' + c.pct.toFixed(1) + '%<div class="bar" style="width:' + Math.min(100, c.pct*2) + '%"></div></div>'; });
    }
    tip.innerHTML = html; tip.style.display = 'block';
    tip.style.left = (ev.clientX + 14) + 'px'; tip.style.top = (ev.clientY + 12) + 'px';
  }
  function hideTip() { tip.style.display = 'none'; }
  function highlight(v) {
    const near = new Set([v]);
    EDGES.forEach(e => { if (e.from === v) near.add(e.to); if (e.to === v) near.add(e.from); });
    nodeEls.forEach((g, i) => { g.classList.toggle('dim', !near.has(i)); g.classList.toggle('sel', i === v); });
    edgeEls.forEach((p, k) => p.classList.toggle('hi', EDGES[k].from === v || EDGES[k].to === v));
  }
  // Clicking the background clears the selection — but the end of a pan lands
  // there too, with mousedown and mouseup on the same element, so the browser
  // reports a click. Dropping the selection every time the reader moved the
  // view to look at a selected node's callers made the panel useless.
  stage.addEventListener('click', ev => {
    if (dragMoved) { dragMoved = false; return; }
    if (ev.target.id === 'svg' || ev.target.id === 'viewport') clearSelection();
  });
  searchInput.addEventListener('input', ev => {
    const q = ev.target.value.toLowerCase();
    nodeEls.forEach((g, i) => g.classList.toggle('search-dim', q && !NAMES[i].toLowerCase().includes(q)));
    buildList();
  });

  // --- paint one frame ---
  function paint(i) {
    cur = i;
    const fN = frameNode[i], fE = frameEdge[i];
    curNodes = fN;
    computeScales(fN);
    const prev = (diffOn && i > 0) ? frameNode[i-1] : null;
    const prevE = (diffOn && i > 0) ? frameEdge[i-1] : null;
    // The card bars compare with the frame's largest figure in the chosen scope.
    let topShare = 0; fN.forEach(n => { topShare = Math.max(topShare, scopedShare(n)); });
    topShare = topShare || 1;
    NAMES.forEach((name, v) => {
      const g = nodeEls[v], n = fN.get(name);
      const stripe = g.querySelector('.stripe'), sub = g.querySelector('.sub'), pv = g.querySelector('.pv'), bar = g.querySelector('.bar'), nm = g.querySelector('.nm');
      if (!n) {
        g.classList.add('absent'); g.classList.remove('hotter', 'grew', 'shrank', 'newfn');
        // In diff mode, a node present in the baseline but gone now is "removed".
        g.classList.toggle('removed', !!(prev && prev.get(name)));
        setGauge(stripe, 0, heat(0)); sub.textContent = ''; pv.textContent = ''; bar.setAttribute('width', 0); return;
      }
      g.classList.remove('absent', 'removed');
      const share = nodeShare(n);
      let fill = heat(share);
      g.classList.remove('hotter', 'grew', 'shrank', 'newfn');
      if (prev) {
        const p = prev.get(name);
        if (!p) { g.classList.add('newfn'); }             // appeared since the baseline
        else {
          const d = share - nodeShare(p);
          if (d > 1.5) { g.classList.add('grew'); fill = diffFill(d); }
          else if (d < -1.5) { g.classList.add('shrank'); fill = diffFill(d); }
        }
      }
      setGauge(stripe, share, fill);
      sub.textContent = nodeSub(n);
      pv.textContent = scopedValue(n);
      const label = fitName(name, W - 19 - 24 - textW(pv.textContent, 600));
      if (nm.textContent !== label) nm.textContent = label;
      bar.setAttribute('width', Math.max(1, 40 * Math.min(1, scopedShare(n) / topShare)).toFixed(1));
    });
    edgeEls.forEach((p, k) => {
      const e = EDGES[k], pct = fE.get(e.key);
      if (pct == null) {
        // In diff mode, an edge that existed in the baseline but not now is "gone".
        if (prevE && prevE.has(e.key)) { p.style.display = ''; p.classList.add('gone'); p.classList.remove('fresh'); if (p._lbl) p._lbl.style.display = 'none'; return; }
        p.style.display = 'none'; if (p._lbl) p._lbl.style.display = 'none'; p.classList.remove('fresh', 'gone'); return;
      }
      p.style.display = ''; p.classList.remove('gone'); if (p._lbl) p._lbl.style.display = '';
      // Edge thickness = the callee's inclusive share of the CURRENT metric.
      const callee = fN.get(NAMES[e.to]);
      const w = callee ? nodeInclShare(callee) : 0;
      p.setAttribute('stroke-width', Math.max(1, Math.min(7, w / 12)));
      if (prevE) p.classList.toggle('fresh', !prevE.has(e.key));
      else p.classList.remove('fresh');
    });
    applyCritical();
    applyPrune();
    const rootRet = (() => { let m = 0; fN.forEach(n => { if ((n.retInclN || 0) > m) m = n.retInclN; }); return m; })();
    const head = metric === 'mem' ? (FRAMES[i].totalAllocs + ' allocations')
      : metric === 'io' ? (frameIoTotal + ' queries')
      : metric === 'network' ? (frameNetworkTotal + ' network operations')
      : metric === 'networkWait' ? (() => { let w = 0; fN.forEach(n => { w += (n.networkWaitExclN || 0); });
          return fmtNs(w) + ' network wait of ' + fmtNs(FRAMES[i].total); })()
      : metric === 'ret' ? (fmtSigned(rootRet) + ' retained')
      : metric === 'wait' ? (() => { let w = 0; fN.forEach(n => { w += (n.waitExclN || 0); });
          return fmtNs(w) + ' waiting of ' + fmtNs(FRAMES[i].total); })()
      : totalLabel(FRAMES[i].total);
    metaEl.textContent = head + ' · ' + fN.size + ' functions' + (DATA.live ? ' · live' : '');
    updateNav(); buildList(); if (flameOn) buildFlame(); if (sqlOn) buildSql(); if (srcOn) buildSrc(); if (chkOn) buildChecks(); save();
  }

  // --- sidebar function list (sorted by the current metric) ---
  function buildList() {
    const q = (searchInput.value || '').toLowerCase();
    const fN = frameNode[cur];
    const mdef = METRICS.find(m => m.key === metric);
    sortlabel.textContent = 'by ' + (scope === 'incl' ? 'inclusive ' : 'self ') + (mdef ? mdef.label.toLowerCase() : 'time');
    scopeEls.forEach(b => b.classList.toggle('on', b.dataset.scope === scope));
    const rows = [];
    NAMES.forEach((name, v) => { const n = fN.get(name); if (n) rows.push({ name, v, n }); });
    rows.sort((a, b) => scopedShare(b.n) - scopedShare(a.n));
    // Bars are relative to the list's largest figure, so the switch does not
    // leave every inclusive bar pinned full.
    const topShare = rows.reduce((m, r) => Math.max(m, scopedShare(r.n)), 0) || 1;
    fnlist.textContent = '';
    if (!rows.length) { const d = document.createElement('li'); d.id = 'empty'; d.textContent = 'no functions'; fnlist.appendChild(d); return; }
    const makeRow = r => {
      const n = r.n, share = nodeShare(n);
      const li = document.createElement('li');
      li.className = 'fnrow' + (r.v === selected ? ' sel' : '') + (q && !r.name.toLowerCase().includes(q) ? ' hide' : '');
      const val = scopedValue(n);
      let sub = scopedSub(n);
      if (n.calls != null && metric !== 'calls') sub += ' · ' + n.calls + (n.calls === 1 ? ' call' : ' calls');
      const qb = (n.ioExclN > 0 && metric !== 'io') ? '<span class="qbadge">' + n.ioExclN + ' q</span>' : '';
      li.innerHTML =
        '<span class="sw"><i style="height:' + gaugePct(share) + '%;background:' + heat(share) + '"></i></span>' +
        '<span class="nm"><span class="n"><span class="nt" title="' + escAttr(r.name) + '">' + esc(r.name) + '</span>' + qb + '</span><span class="m2">' + sub + '</span></span>' +
        '<span class="val"><span class="pv">' + val + '</span><span class="mini"><i style="width:' + Math.min(100, 100 * scopedShare(n) / topShare).toFixed(1) + '%"></i></span></span>';
      if (r.v === selected) li.classList.add('open');
      // A second click on the open function folds it.
      li.addEventListener('click', () => { if (r.v === selected) clearSelection(); else selectNode(r.v, true); });
      return li;
    };
    if (grouped) {
      // Cluster by class (before `::`) or namespace (before the last `\`); the
      // rest stay under "· global". Groups rank by aggregate share; rows within
      // keep the metric order. Headers collapse the group.
      const groups = new Map();
      rows.forEach(r => { const k = groupKey(r.name); let g = groups.get(k);
        if (!g) { g = { key: k, rows: [], agg: 0 }; groups.set(k, g); } g.rows.push(r);
        g.agg = scope === 'incl' ? Math.max(g.agg, scopedShare(r.n)) : g.agg + nodeShare(r.n); });
      [...groups.values()].sort((a, b) => b.agg - a.agg).forEach(g => {
        const isColl = collapsed.has(g.key);
        const anyMatch = !q || g.rows.some(r => r.name.toLowerCase().includes(q));
        const h = document.createElement('li');
        h.className = 'grouphdr' + (isColl ? ' coll' : '') + (anyMatch ? '' : ' hide');
        h.innerHTML = '<span class="gtw">▾</span><span class="gname">' + esc(g.key || '· global') +
          '</span><span class="gcount">' + g.rows.length + '</span>';
        h.addEventListener('click', () => { if (collapsed.has(g.key)) collapsed.delete(g.key); else collapsed.add(g.key); buildList(); });
        fnlist.appendChild(h);
        if (!isColl) g.rows.forEach(r => { const li = makeRow(r); li.classList.add('ingroup'); fnlist.appendChild(li);
          if (r.v === selected) { const d = detailItem(li); d.classList.add('ingroup'); fnlist.appendChild(d); } });
      });
    } else {
      rows.forEach(r => { const li = makeRow(r); fnlist.appendChild(li); if (r.v === selected) fnlist.appendChild(detailItem(li)); });
    }
    // A function picked in the graph opens where it sits in the list.
    const open = fnlist.querySelector('.fnrow.open');
    if (open) open.scrollIntoView({ block: 'nearest' });
  }
  // Class/namespace a function belongs to, for grouping ('' = global scope).
  function groupKey(name) {
    const ci = name.lastIndexOf('::');
    if (ci > 0) return name.slice(0, ci);
    const ni = name.lastIndexOf('\\');
    if (ni > 0) return name.slice(0, ni);
    return '';
  }

  // --- the open function: its figures and its callers / callees ---
  // As in Blackfire, the selected row opens in place: inclusive and exclusive
  // cost in every dimension the capture has, then the callers or the callees,
  // one button each. `pct` on an edge is the callee's inclusive time under that
  // caller (a % of the whole run), shown beside the exact call count; clicking
  // a neighbour opens it in turn.
  function detailItem(row) {
    const li = document.createElement('li');
    li.className = 'fndetail' + (row.classList.contains('hide') ? ' hide' : '');
    const n = frameNode[cur].get(NAMES[selected]);
    if (!n) return li;
    const fE = frameEdge[cur];
    const callers = [], callees = [];
    EDGES.forEach(e => {
      const pct = fE.get(e.key);
      if (pct == null) return;
      if (e.to === selected) callers.push({ v: e.from, c: e.count, p: pct });
      if (e.from === selected) callees.push({ v: e.to, c: e.count, p: pct });
    });
    callers.sort((a, b) => (b.p || 0) - (a.p || 0));
    callees.sort((a, b) => (b.p || 0) - (a.p || 0));
    const total = (FRAMES[cur] && FRAMES[cur].total) || 0;
    const timeOf = pct => DATA.exact && total ? fmtNs(Math.round(pct / 100 * total)) + ' · ' + pct.toFixed(1) + '%' : pct.toFixed(1) + '%';
    const figures = [['Time', timeOf(n.incl), timeOf(n.excl)]];
    if (hasMem) figures.push(['Allocations', fmtK(n.allocInclN || 0), fmtK(n.allocExclN || 0)]);
    if (hasRet) figures.push(['Retained', fmtSigned(n.retInclN || 0), fmtSigned(n.retExclN || 0)]);
    if (hasWait) figures.push(['Wait', fmtNs(n.waitInclN || 0), fmtNs(n.waitExclN || 0)]);
    if (hasIo) figures.push(['SQL queries', '' + (n.ioInclN || 0), '' + (n.ioExclN || 0)]);
    if (hasNetwork) figures.push(['Network ops', '' + (n.networkInclN || 0), '' + (n.networkExclN || 0)]);
    if (hasNetworkWait) figures.push(['Network wait', fmtNs(n.networkWaitInclN || 0), fmtNs(n.networkWaitExclN || 0)]);
    if (n.calls != null) figures.push(['Calls', fmtK(n.calls), '']);
    let html = '<div class="fig"><span class="fh"></span><span class="fh">Incl.</span><span class="fh">Excl.</span>' +
      figures.map(([label, incl, excl]) => '<span class="fl">' + label + '</span><span class="fv">' + incl +
        '</span><span class="fv x">' + excl + '</span>').join('') + '</div>';
    // Callees first when there are any: the usual question is "where does it go".
    const tab = detailTab === 'callers' ? (callers.length || !callees.length ? 'callers' : 'callees')
      : (callees.length || !callers.length ? 'callees' : 'callers');
    html += '<div class="dtabs"><span class="dseg">' +
      '<button type="button" data-tab="callers" class="' + (tab === 'callers' ? 'on' : '') + '"' + (callers.length ? '' : ' disabled') +
      '>Callers · ' + callers.length + '</button>' +
      '<button type="button" data-tab="callees" class="' + (tab === 'callees' ? 'on' : '') + '"' + (callees.length ? '' : ' disabled') +
      '>Callees · ' + callees.length + '</button></span>' +
      '<button type="button" class="dgo" title="Center the graph on this function">⌖ In the graph</button></div>';
    const list = tab === 'callers' ? callers : callees;
    html += list.length ? list.map(r => {
      const bits = [];
      if (r.c != null) bits.push('×' + fmtK(r.c));
      if (DATA.exact && r.p != null) bits.push(r.p.toFixed(1) + '%');
      return '<div class="drow" data-v="' + r.v + '" title="' + escAttr(NAMES[r.v]) + '"><span class="dn">' + esc(NAMES[r.v]) +
        '</span><span class="dc">' + bits.join(' · ') + '</span></div>';
    }).join('') : '<div class="dnone">' + (tab === 'callers' ? 'Called by nothing: an entry point.' : 'Calls nothing.') + '</div>';
    li.innerHTML = html;
    // Inside the card, clicks act on the card, not on the list behind it.
    li.addEventListener('click', ev => ev.stopPropagation());
    li.querySelectorAll('.dseg button').forEach(b => b.addEventListener('click', () => { detailTab = b.dataset.tab; buildList(); }));
    li.querySelector('.dgo').addEventListener('click', () => {
      // Back to the graph from whichever view is up, centred on the function.
      if (flameOn) setFlame(false);
      if (sqlOn) setSql(false);
      if (srcOn) setSrc(false);
      if (chkOn) setChk(false);
      // Close enough to read it: a whole wide graph fitted on screen leaves
      // every box a few pixels across. A view chosen by hand stays chosen.
      if (scale < 1) scale = 1;
      viewIsAuto = false;
      selectNode(selected, true);
    });
    li.querySelectorAll('.drow').forEach(r => r.addEventListener('click', () => selectNode(+r.dataset.v, true)));
    return li;
  }

  // --- timeline: the Flame view's other mode, as Blackfire draws it ---
  // Every call where and when it ran (rows by depth, on a time axis), the heap
  // in bytes behind them, and an overview of the whole run to pick a range on.
  // Only a launched exact run carries one (`DATA.timeline`, written by
  // monitor::timeline). Drawn on canvases: a request makes tens of thousands of
  // calls, more than the DOM draws smoothly.
  const TL = DATA.timeline || null;
  let flameMode = TL ? 'timeline' : 'merged';
  let tlT0 = 0, tlT1 = TL ? TL.totalNs : 1, tlPick = -1, tlDrag = null, tlDragEnd = 0;
  const TL_SPANS = [];
  let TL_DEPTH = 1, TL_PEAK = 1;
  const TL_ROW = 22, TL_GUTTER = 64, TL_AXIS = 22;
  if (TL) {
    const a = TL.spans || [];
    // Each call's own time comes measured over every recorded call, before the cut to
    // the longest: a callee the page does not draw is not its caller's work (review).
    for (let i = 0; i + 6 < a.length; i += 7) TL_SPANS.push({ n: a[i], d: a[i + 1], s: a[i + 2], len: a[i + 3], own: a[i + 4], hs: a[i + 5], he: a[i + 6] });
    TL_SPANS.forEach(sp => { if (sp.d + 1 > TL_DEPTH) TL_DEPTH = sp.d + 1; });
    const m = TL.memory || [];
    for (let i = 1; i < m.length; i += 2) TL_PEAK = Math.max(TL_PEAK, m[i]);
  }
  function fmtBytes(b) { return b >= 1048576 ? (b / 1048576).toFixed(1) + ' MiB' : b >= 1024 ? (b / 1024).toFixed(0) + ' KiB' : b + ' B'; }
  // The heap at `t`: the last reading at or before it.
  function memAt(t) {
    const m = TL.memory; let lo = 0, hi = m.length / 2 - 1;
    if (hi < 0) return 0;
    while (lo < hi) { const mid = (lo + hi + 1) >> 1; if (m[mid * 2] <= t) lo = mid; else hi = mid - 1; }
    return m[lo * 2 + 1];
  }
  // A call wears its function's heat: the function's own time over the run,
  // as in the graph and the list, so repeated short calls of a hot function
  // read hot together.
  function tlHeat(n) { const node = frameNode[cur] && frameNode[cur].get(TL.names[n]); return node ? node.excl : 0; }
  function tlColors() {
    const cs = getComputedStyle(document.documentElement), v = k => cs.getPropertyValue(k).trim();
    return { ink: v('--ink'), muted: v('--muted'), border: v('--border'), accent: v('--accent'), mem: v('--mem'), memLine: v('--mem-line'), panel: v('--panel') };
  }
  function niceStep(range, n) { const raw = range / n, p = Math.pow(10, Math.floor(Math.log10(raw))), f = raw / p; return (f < 1.5 ? 1 : f < 3.5 ? 2 : f < 7.5 ? 5 : 10) * p; }
  function tlTicks(a, b, n) { const st = niceStep(Math.max(1, b - a), n), out = []; for (let v = Math.ceil(a / st) * st; v <= b; v += st) out.push(v); return out; }
  function tlFmt(ns) { return ns === 0 ? '0' : fmtNs(Math.round(ns)); }
  function fitText(g, s, w) {
    if (g.measureText(s).width <= w) return s;
    let lo = 0, hi = s.length;
    while (lo < hi) { const mid = (lo + hi + 1) >> 1; if (g.measureText(s.slice(0, mid) + '…').width <= w) lo = mid; else hi = mid - 1; }
    return lo > 1 ? s.slice(0, lo) + '…' : '';
  }
  function tlCanvas(id, h) {
    const cv = document.getElementById(id); if (!cv) return null;
    const dpr = window.devicePixelRatio || 1, w = cv.clientWidth;
    cv.style.height = h + 'px'; cv.width = Math.round(w * dpr); cv.height = Math.round(h * dpr);
    const g = cv.getContext('2d'); g.setTransform(dpr, 0, 0, dpr, 0, 0); g.clearRect(0, 0, w, h);
    return { cv, g, w, h };
  }
  function tlMemArea(g, x0, w, top, h, a, b, C) {
    const steps = Math.max(2, Math.min(Math.round(w), 1600));
    g.beginPath(); g.moveTo(x0, top + h);
    for (let i = 0; i <= steps; i++) { const t = a + (b - a) * i / steps; g.lineTo(x0 + w * i / steps, top + h - memAt(t) / TL_PEAK * h * 0.92); }
    g.lineTo(x0 + w, top + h); g.closePath(); g.fillStyle = C.mem; g.fill();
    g.beginPath();
    for (let i = 0; i <= steps; i++) { const t = a + (b - a) * i / steps, y = top + h - memAt(t) / TL_PEAK * h * 0.92; if (i) g.lineTo(x0 + w * i / steps, y); else g.moveTo(x0, y); }
    g.strokeStyle = C.memLine; g.lineWidth = 1; g.stroke();
  }
  function drawTlOverview() {
    const c = tlCanvas('tlover', 96); if (!c) return;
    const { g, w, h } = c, C = tlColors(), total = TL.totalNs;
    tlMemArea(g, 0, w, 0, h, 0, total, C);
    const rowH = Math.max(1.5, Math.min(5, (h - 8) / TL_DEPTH));
    const last = [];
    TL_SPANS.forEach(sp => {
      const x0 = sp.s / total * w, x1 = Math.max(x0 + 1, (sp.s + sp.len) / total * w);
      if (last[sp.d] !== undefined && x1 <= last[sp.d]) return;
      last[sp.d] = x1;
      g.fillStyle = heat(tlHeat(sp.n)); g.fillRect(x0, 4 + sp.d * rowH, Math.max(1, x1 - x0 - 0.5), Math.max(1, rowH - 1));
    });
    const brush = document.getElementById('tlbrush'), dl = document.getElementById('tldiml'), dr = document.getElementById('tldimr');
    const pct = t => (t / total * 100).toFixed(3) + '%';
    if (brush) { brush.style.left = pct(tlT0); brush.style.width = pct(tlT1 - tlT0); }
    if (dl) dl.style.width = pct(tlT0);
    if (dr) dr.style.width = pct(total - tlT1);
    const ax = document.getElementById('tloverax');
    if (ax) ax.innerHTML = tlTicks(0, total, 9).map(v => '<span style="left:' + pct(v) + '">' + tlFmt(v) + '</span>').join('');
  }
  function drawTlMain() {
    const height = TL_AXIS + 8 + TL_DEPTH * TL_ROW;
    const c = tlCanvas('tlmain', Math.max(320, height)); if (!c) return;
    const { g, w, h } = c, C = tlColors();
    const plotW = w - TL_GUTTER, plotH = h - TL_AXIS, span = Math.max(1, tlT1 - tlT0);
    const X = t => TL_GUTTER + (t - tlT0) / span * plotW;
    tlMemArea(g, TL_GUTTER, plotW, TL_AXIS, plotH, tlT0, tlT1, C);
    g.font = '11px ' + cardFont; g.textBaseline = 'middle';
    tlTicks(tlT0, tlT1, 10).forEach(v => {
      const x = X(v); g.fillStyle = C.border; g.fillRect(Math.round(x), TL_AXIS, 1, plotH);
      g.fillStyle = C.muted; g.textAlign = 'center'; g.fillText(tlFmt(v), Math.min(w - 24, Math.max(TL_GUTTER + 18, x)), TL_AXIS / 2);
    });
    g.fillStyle = C.border; g.fillRect(TL_GUTTER, TL_AXIS - 1, plotW, 1);
    g.textAlign = 'right'; g.fillStyle = C.muted;
    [0.25, 0.5, 0.75, 1].forEach(f => g.fillText(fmtBytes(Math.round(TL_PEAK / 0.92 * f)), TL_GUTTER - 8, TL_AXIS + plotH * (1 - f) + (f === 1 ? 7 : 0)));
    g.textAlign = 'left'; g.font = '500 12px ' + cardFont;
    const last = [];
    let picked = null;
    for (let i = 0; i < TL_SPANS.length; i++) {
      const sp = TL_SPANS[i];
      if (sp.s > tlT1) break;
      if (sp.s + sp.len < tlT0) continue;
      let x0 = Math.max(X(sp.s), TL_GUTTER), x1 = Math.min(X(sp.s + sp.len), w);
      if (x1 - x0 < 1) { if (last[sp.d] !== undefined && x0 < last[sp.d]) continue; x1 = x0 + 1; }
      last[sp.d] = x1;
      const y = TL_AXIS + 4 + sp.d * TL_ROW, excl = tlHeat(sp.n);
      g.fillStyle = heat(excl);
      const bw = Math.max(1, x1 - x0 - 1.5);
      if (bw > 6 && g.roundRect) { g.beginPath(); g.roundRect(x0, y, bw, TL_ROW - 3, 4); g.fill(); } else g.fillRect(x0, y, bw, TL_ROW - 3);
      if (bw > 30) { const label = fitText(g, TL.names[sp.n], bw - 12); if (label) { g.fillStyle = ink(excl); g.fillText(label, x0 + 6, y + (TL_ROW - 3) / 2 + 0.5); } }
      if (i === tlPick) picked = { x0, y, bw };
    }
    if (picked) { g.strokeStyle = C.accent; g.lineWidth = 2.5; g.strokeRect(picked.x0 + 1, picked.y + 1, picked.bw - 2, TL_ROW - 5); }
  }
  // The call under a point of the main canvas, or -1.
  function tlHit(ev) {
    const cv = document.getElementById('tlmain'); if (!cv) return -1;
    const r = cv.getBoundingClientRect(), x = ev.clientX - r.left, y = ev.clientY - r.top;
    const d = Math.floor((y - TL_AXIS - 4) / TL_ROW);
    if (x < TL_GUTTER || d < 0 || d >= TL_DEPTH) return -1;
    const t = tlT0 + (x - TL_GUTTER) / (r.width - TL_GUTTER) * (tlT1 - tlT0);
    for (let i = TL_SPANS.length - 1; i >= 0; i--) { const sp = TL_SPANS[i]; if (sp.d === d && sp.s <= t && sp.s + sp.len >= t) return i; }
    return -1;
  }
  function tlZoom(a, b) { const total = TL.totalNs; const min = Math.max(1, total / 1e6); tlT0 = Math.max(0, a); tlT1 = Math.min(total, Math.max(b, tlT0 + min)); }
  function buildTimeline() {
    const picked = TL_SPANS[tlPick];
    const zoomed = tlT0 > 0 || tlT1 < TL.totalNs;
    let head = '<div class="phead"><span class="pt">Flame</span>' + flameModeSwitch() +
      '<span class="pchip">range <b>' + tlFmt(tlT0) + ' – ' + fmtNs(Math.round(tlT1)) + '</b></span>' +
      (picked ? '<span class="pchip"><b>' + esc(TL.names[picked.n]) + '</b></span>' : '') +
      (zoomed ? '<a href="#" id="tlhome" class="pbtn">Whole run</a>' : '') +
      '<span class="pnote">drag on the overview to pick a range &middot; click a call to zoom on it &middot; wheel to zoom</span>' +
      '<span class="tl-key"><i class="mem"></i>memory in use<i class="own"></i>own time</span></div>';
    // What the page leaves out: the longest calls kept of those recorded, and the calls
    // past the recording limit, which were not recorded at all (review).
    const notes = [];
    if (TL.shown < TL.recorded) notes.push(TL.shown + ' longest of ' + TL.recorded + ' calls shown');
    if (TL.dropped > 0) notes.push(TL.dropped + ' later calls past the recording limit not recorded');
    const note = notes.length ? '<div class="tl-note">' + notes.join(' &middot; ') + '</div>' : '';
    let detail = '';
    if (picked) {
      // The call's own heap readings, at its start and its end: the chart's curve keeps each
      // slice's peak only, and read from it a call's change could show zero (review).
      const before = picked.hs, after = picked.he, delta = after - before;
      detail = '<div class="pcard tl-detail"><span class="sw" style="background:' + heat(tlHeat(picked.n)) + '"></span>' +
        '<span class="tl-dn"><b>' + esc(TL.names[picked.n]) + '</b><span>starts at ' + tlFmt(picked.s) + ' &middot; lasts ' + fmtNs(picked.len) + ' &middot; depth ' + picked.d + '</span></span>' +
        [['Wall time', fmtNs(picked.len)], ['Own time', fmtNs(picked.own)], ['Memory at start', fmtBytes(before)],
         ['Memory change', (delta < 0 ? '−' : '+') + fmtBytes(Math.abs(delta))]]
          .map(f => '<span class="tl-fig"><span>' + f[0] + '</span><b>' + f[1] + '</b></span>').join('') + '</div>';
    }
    flame.innerHTML = head + '<div class="pcard tl">' +
      '<div class="tl-overax" id="tloverax"></div>' +
      '<div class="tl-over" id="tloverwrap"><canvas id="tlover"></canvas><div class="tl-dim l" id="tldiml"></div><div class="tl-dim r" id="tldimr"></div><div class="tl-brush" id="tlbrush"></div></div>' +
      '<canvas id="tlmain" class="tl-main"></canvas>' + note + '</div>' + detail;
    drawTlOverview(); drawTlMain();
    bindFlameModeSwitch();
    const home = document.getElementById('tlhome');
    if (home) home.addEventListener('click', ev => { ev.preventDefault(); tlT0 = 0; tlT1 = TL.totalNs; tlPick = -1; buildTimeline(); });
    const over = document.getElementById('tloverwrap');
    const at = ev => { const r = over.getBoundingClientRect(); return Math.max(0, Math.min(1, (ev.clientX - r.left) / r.width)) * TL.totalNs; };
    over.addEventListener('mousedown', ev => { ev.preventDefault(); tlDrag = at(ev);
      const move = e => { const t = at(e); tlZoom(Math.min(tlDrag, t), Math.max(tlDrag, t)); drawTlOverview(); drawTlMain(); };
      const up = e => { window.removeEventListener('mousemove', move); window.removeEventListener('mouseup', up);
        const t = at(e);
        // A click without a drag frames an eighth of the run around it.
        if (Math.abs(t - tlDrag) < TL.totalNs / 400) { const half = TL.totalNs / 16; tlZoom(t - half, t + half); }
        tlDrag = null; tlDragEnd = Date.now(); buildTimeline(); };
      window.addEventListener('mousemove', move); window.addEventListener('mouseup', up); });
    const main = document.getElementById('tlmain');
    main.addEventListener('mousemove', ev => {
      const i = tlHit(ev);
      if (i < 0) { hideTip(); main.style.cursor = 'default'; return; }
      main.style.cursor = 'pointer';
      const sp = TL_SPANS[i];
      tip.innerHTML = '<h3>' + esc(TL.names[sp.n]) + '</h3>' +
        '<div class="row"><span>wall</span><b>' + fmtNs(sp.len) + '</b></div>' +
        '<div class="row"><span>own</span><b>' + fmtNs(sp.own) + '</b></div>' +
        '<div class="row"><span>starts at</span><b>' + tlFmt(sp.s) + '</b></div>' +
        '<div class="row"><span>heap at its end</span><b>' + fmtBytes(memAt(sp.s + sp.len)) + '</b></div>';
      tip.style.display = 'block'; tip.style.left = (ev.clientX + 14) + 'px'; tip.style.top = (ev.clientY + 12) + 'px';
    });
    main.addEventListener('mouseleave', hideTip);
    main.addEventListener('click', ev => {
      // The end of a drag on the overview is not a click on a call.
      if (Date.now() - tlDragEnd < 400) return;
      const i = tlHit(ev); if (i < 0) return;
      const sp = TL_SPANS[i], pad = sp.len * 0.05;
      tlPick = i; tlZoom(sp.s - pad, sp.s + sp.len + pad);
      const v = nameId.get(TL.names[sp.n]);
      if (v != null) { selected = v; highlight(v); buildList(); }
      hideTip(); buildTimeline(); save();
    });
    // The wheel zooms around the pointer, as a map does.
    main.addEventListener('wheel', ev => {
      ev.preventDefault();
      const r = main.getBoundingClientRect(), f = Math.max(0, Math.min(1, (ev.clientX - r.left - TL_GUTTER) / (r.width - TL_GUTTER)));
      const span = tlT1 - tlT0, t = tlT0 + f * span, next = Math.min(TL.totalNs, span * Math.exp(ev.deltaY * 0.0025));
      tlZoom(t - f * next, t - f * next + next); drawTlOverview(); drawTlMain();
      const chip = flame.querySelector('.phead .pchip b'); if (chip) chip.textContent = tlFmt(tlT0) + ' – ' + fmtNs(Math.round(tlT1));
    }, { passive: false });
  }
  function flameModeSwitch() {
    if (!TL) return '';
    return '<span class="pseg" id="flamemode" role="group" aria-label="Flame mode">' +
      '<button data-mode="merged" class="' + (flameMode === 'merged' ? 'on' : '') + '" data-tip="Calls of one function merged into one bar; width is inclusive time">Merged</button>' +
      '<button data-mode="timeline" class="' + (flameMode === 'timeline' ? 'on' : '') + '" data-tip="Every call where and when it ran, with the memory in use">Timeline</button></span>';
  }
  function bindFlameModeSwitch() {
    flame.querySelectorAll('#flamemode button').forEach(b => b.addEventListener('click', () => {
      if (flameMode === b.dataset.mode) return; flameMode = b.dataset.mode; hideTip(); buildFlame(); save();
    }));
  }
  window.addEventListener('resize', () => { if (flameOn && TL && flameMode === 'timeline') { drawTlOverview(); drawTlMain(); } });

  // --- flame graph (icicle) built from the exact inclusive-time tree ---
  // Each edge's `pct` is the callee's inclusive time under that caller (a % of
  // the whole run), so a child cell's width is exactly that share of its
  // parent's width. Recursion is drawn once (we don't descend a cycle).
  function buildFlame() {
    if (!flameOn) return;
    if (TL && flameMode === 'timeline') { buildTimeline(); return; }
    const fN = frameNode[cur], fE = frameEdge[cur];
    const out = NAMES.map(() => []), incoming = new Array(NAMES.length).fill(0);
    EDGES.forEach(e => { if (fE.get(e.key) != null) { out[e.from].push(e); incoming[e.to]++; } });
    const nodeOf = v => fN.get(NAMES[v]);
    const inclOf = v => { const n = nodeOf(v); return n ? n.incl : 0; };
    let roots;
    if (flameRoot >= 0 && nodeOf(flameRoot)) roots = [flameRoot];
    else {
      roots = [];
      NAMES.forEach((name, v) => { if (fN.get(name) && incoming[v] === 0) roots.push(v); });
      if (!roots.length) { let best = -1, bv = -1;
        NAMES.forEach((name, v) => { const n = fN.get(name); if (n && n.incl > bv) { bv = n.incl; best = v; } });
        if (best >= 0) roots = [best]; }
    }
    const totalRoot = roots.reduce((s, v) => s + Math.max(1e-4, inclOf(v)), 0) || 1;
    const cells = []; let maxDepth = 0; const ROWH = 22;
    function walk(v, x0, w, depth, path) {
      if (w <= 0 || depth > 64) return;
      const n = nodeOf(v); if (!n) return;
      maxDepth = Math.max(maxDepth, depth);
      cells.push({ v, name: NAMES[v], x: x0, w, depth, incl: n.incl, excl: n.excl });
      if (path.has(v)) return;
      path.add(v);
      const parentIncl = Math.max(1e-4, n.incl);
      let cx = x0;
      out[v].forEach(e => {
        const pct = fE.get(e.key) || 0, cw = w * Math.min(1, pct / parentIncl);
        walk(e.to, cx, cw, depth + 1, path); cx += cw;
      });
      path.delete(v);
    }
    let x = 0;
    roots.forEach(v => { const w = Math.max(1e-4, inclOf(v)) / totalRoot; walk(v, x, w, 0, new Set()); x += w; });
    const W = Math.max(1, flame.clientWidth - 16), H = (maxDepth + 1) * ROWH;
    const rootLabel = (flameRoot >= 0 && nodeOf(flameRoot)) ? esc(NAMES[flameRoot]) : 'all roots';
    // A head that reads like the other panels': what is shown, from which root
    // (a chip, with its way back), and how to use it; the bars sit in a card.
    let html = '<div class="phead"><span class="pt">Flame</span>' + flameModeSwitch() + '<span class="pchip">root <b>' + rootLabel + '</b></span>' +
      (flameRoot >= 0 ? '<a href="#" id="freset" class="pbtn">Back to all roots</a>' : '') +
      '<span class="pnote">width is inclusive time, colour own time &middot; click a frame to zoom, Esc to reset</span></div>' +
      '<div class="pcard fcard"><div class="fcanvas" style="height:' + H + 'px">';
    cells.forEach(c => {
      const label = c.w * W > 34 ? esc(c.name) + '  ' + c.incl.toFixed(1) + '%' : '';
      html += '<div class="fcell' + (c.v === selected ? ' sel' : '') + '" data-v="' + c.v +
        '" title="' + escAttr(c.name) + ' — incl ' + c.incl.toFixed(1) + '% · self ' + c.excl.toFixed(1) + '%"' +
        ' style="left:' + (c.x * 100).toFixed(4) + '%;width:' + (c.w * 100).toFixed(4) + '%;top:' +
        (c.depth * ROWH) + 'px;background:' + heat(c.excl) + ';color:' + ink(c.excl) + '">' + label + '</div>';
    });
    flame.innerHTML = html + '</div></div>';
    flame.querySelectorAll('.fcell').forEach(el => el.addEventListener('click', () => {
      const v = +el.dataset.v; selected = v; flameRoot = v; highlight(v); buildList(); buildFlame(); save();
    }));
    bindFlameModeSwitch();
    const rst = document.getElementById('freset');
    if (rst) rst.addEventListener('click', ev => { ev.preventDefault(); flameRoot = -1; buildFlame(); save(); });
  }
  function setFlame(on) {
    flameOn = on;
    if (on && chkOn) setChk(false);
    if (on && sqlOn) setSql(false);
    if (on && srcOn) setSrc(false);
    document.body.classList.toggle('flame', on);
    flamebtn.classList.toggle('on', on);
    if (on) buildFlame();
    syncViewBar();
    save();
  }

  // --- SQL panel: distinct DB statements and their execution counts. Query
  // texts are normalized (literals -> ?), so an N+1's repeated statement folds
  // into one row whose count is the smoking gun. The heaviest few are flagged. ---
  // The statements opened to show who ran them, kept across repaints.
  const sqlOpen = new Set();
  function buildSql() {
    if (!sqlOn) return;
    const qs = (FRAMES[cur].queries || []).slice().sort((a, b) => b.count - a.count);
    const total = qs.reduce((s, q) => s + q.count, 0);
    const max = qs.length ? qs[0].count : 1;
    let html = '<div class="phead"><span class="pt">Queries</span><span class="pnote"><b>' + qs.length + '</b> distinct ' +
      (qs.length === 1 ? 'statement' : 'statements') + ' &middot; <b>' + total + '</b> executions</span></div>';
    if (!qs.length) { sqlpanel.innerHTML = html + '<div class="pcard pempty">no DB queries recorded</div>'; return; }
    html += '<div class="pcard"><table><thead><tr><th>Runs</th><th>Share</th><th>Statement</th></tr></thead><tbody>';
    qs.forEach((q, i) => {
      const share = q.count / max;
      const callers = q.callers || [];
      const open = callers.length > 0 && sqlOpen.has(q.sql);
      // Flag a likely N+1: the same statement executed many times.
      const cls = (q.count >= 20 ? 'warn' : '') + (callers.length ? ' qopen' : '') + (open ? ' open' : '');
      const badge = q.count >= 20 ? '<span class="qn1">N+1?</span>' : '';
      const chevron = callers.length
        ? '<svg class="ic qchev" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6"/></svg>' : '';
      html += '<tr class="' + cls + '" data-q="' + i + '"' + (callers.length ? ' aria-expanded="' + open + '" tabindex="0"' : '') + '>' +
        '<td class="qc">' + chevron + '×' + q.count + '</td>' +
        '<td class="qbar"><span class="qtrack"><i style="width:' + Math.max(4, share * 100) + '%;background:' + heat(share * 33) + '"></i></span></td>' +
        '<td class="qsql"><code>' + esc(q.sql) + '</code>' + badge +
        (callers.length ? '<span class="qfromn">' + callers.length + (callers.length === 1 ? ' caller' : ' callers') + '</span>' : '') + '</td></tr>';
      if (open) {
        // Who ran it: the application function above the driver, where it is
        // declared, and how many of the statement's executions it made.
        html += '<tr class="qfrom"><td></td><td colspan="2"><div class="qlist">' +
          '<div class="qlh"><span>Called from</span><span>File</span><span>Runs</span><span>Share</span></div>' +
          callers.map(c => {
            const at = c.loc || '', cut = at.lastIndexOf('/'), short = cut >= 0 ? at.slice(cut + 1) : at;
            return '<button class="qcaller" data-fn="' + escAttr(c.fn) + '"><span class="qfn">' + esc(c.fn) + '</span>' +
              '<span class="qloc" title="' + escAttr(at) + '">' + (short ? esc(short) : '&mdash;') + '</span>' +
              '<span class="qn">×' + c.n + '</span>' +
              '<span class="qtrack"><i style="width:' + Math.max(3, 100 * c.n / q.count).toFixed(1) + '%"></i></span></button>';
          }).join('') + '</div></td></tr>';
      }
    });
    sqlpanel.innerHTML = html + '</tbody></table></div>';
    sqlpanel.querySelectorAll('tr.qopen').forEach(row => {
      const toggle = () => { const q = qs[+row.dataset.q]; if (sqlOpen.has(q.sql)) sqlOpen.delete(q.sql); else sqlOpen.add(q.sql); buildSql(); };
      row.addEventListener('click', toggle);
      row.addEventListener('keydown', ev => { if (ev.key === 'Enter' || ev.key === ' ') { ev.preventDefault(); toggle(); } });
    });
    // A caller opens in the function list, as a click in the graph would.
    sqlpanel.querySelectorAll('.qcaller').forEach(b => b.addEventListener('click', () => {
      const v = nameId.get(b.dataset.fn); if (v == null) return;
      selected = v; highlight(v); buildList(); save();
    }));
  }
  function setSql(on) {
    sqlOn = on;
    if (on && chkOn) setChk(false);
    if (on && flameOn) setFlame(false);
    if (on && srcOn) setSrc(false);
    document.body.classList.toggle('sql', on);
    sqlbtn.classList.toggle('on', on);
    if (on) buildSql();
    syncViewBar();
    save();
  }

  // --- source panel: the PHP file annotated with per-line sampled cost.
  // Each line carries the share of samples that landed on it, heat-colored like
  // the graph. Sampled, not exact — a line's cost is where the sampler caught
  // the program, so hot lines are reliable and a 0% line is not proof of zero.
  function buildSrc() {
    if (!srcOn || !LINES) return;
    const hits = new Map(LINES.hits.map(h => [h[0], h[1]]));
    const total = LINES.total || 1;
    // Two ways to annotate a file, depending on what the capture measured.
    // Sampled: a share per line. Exact: no per-line data exists, but every
    // function's cost does, so each declaration is labelled and its body tinted
    // — the file read as a map of where the time went.
    const funcs = LINES.funcs || [];
    const perLine = hits.size > 0;
    // Line -> the innermost function covering it, so a nested declaration wins.
    const owner = new Map();
    funcs.forEach(f => {
      for (let n = f.start; n <= f.end; n++) {
        const cur = owner.get(n);
        if (!cur || (f.end - f.start) < (cur.end - cur.start)) owner.set(n, f);
      }
    });
    // Show the file NAME, not the absolute path: a path is mostly build
    // directories, and the part that identifies the file sits at the end. The
    // whole path stays available on hover.
    const cut = LINES.file.lastIndexOf('/');
    const base = cut >= 0 ? LINES.file.slice(cut + 1) : LINES.file;
    const summary = perLine
      ? total + ' samples attributed to ' + hits.size + ' lines &middot; share of the run per line'
      : funcs.length + ' measured function' + (funcs.length === 1 ? '' : 's') +
        ' &middot; self time per function (exact)';
    let html = '<div class="phead"><span class="pt" title="' + escAttr(LINES.file) + '">' + esc(base) +
      '</span><span class="pnote">' + summary + '</span></div><div class="pcard scode">';
    LINES.source.forEach((text, i) => {
      const n = i + 1;
      let cell = '<span class="sp"></span>', hot = false, tint = '';
      if (perLine) {
        const s = hits.get(n) || 0, pct = 100 * s / total;
        hot = pct >= 5;
        if (s > 0) cell = '<span class="sp" style="background:' + heat(pct) + ';color:' + ink(pct) + '">' +
          pct.toFixed(1) + '%</span>';
      } else {
        const f = owner.get(n);
        if (f) {
          // The declaration line carries the label; the body is tinted, faintly,
          // so a hot function reads as a block without drowning its own code.
          tint = ' style="background:' + heatA(f.selfPct, n === f.start ? 0.30 : 0.13) + '"';
          hot = f.selfPct >= 5 && n === f.start;
          if (n === f.start) {
            cell = '<span class="sp" style="background:' + heat(f.selfPct) + ';color:' + ink(f.selfPct) + '">' +
              f.selfPct.toFixed(1) + '%</span>';
          }
        }
      }
      const note = (!perLine && owner.get(n) && n === owner.get(n).start)
        ? '<span class="sfn">incl ' + owner.get(n).inclPct.toFixed(1) + '% &middot; ' +
          owner.get(n).calls + (owner.get(n).calls === 1 ? ' call' : ' calls') + '</span>'
        : '';
      html += '<div class="sline' + (hot ? ' hot' : '') + '"' + tint + '><span class="sn">' + n + '</span>' +
        cell + '<span class="sc">' + esc(text) + note + '</span></div>';
    });
    srcpanel.innerHTML = html + '</div>';
  }
  // --- checks panel: the performance budget, and how this run measured up.
  // Failures first — a gate is read when it is red, and the reason should not
  // sit below a screen of passing rows.
  function buildChecks() {
    if (!chkOn) return;
    const rank = { fail: 0, error: 1, pass: 2 };
    const rows = ASSERTS.slice().sort((a, b) => rank[a.status] - rank[b.status]);
    const n = s => ASSERTS.filter(a => a.status === s).length;
    let html = '<div class="phead"><span class="pt">Checks</span><span class="pchip pass"><b>' + n('pass') + '</b> passed</span>' +
      '<span class="pchip' + (n('fail') ? ' fail' : '') + '"><b>' + n('fail') + '</b> failed</span>' +
      (n('error') ? '<span class="pchip"><b>' + n('error') + '</b> not evaluated</span>' : '') + '</div>';
    if (!rows.length) { chkpanel.innerHTML = html + '<div class="pcard pempty">no assertions for this run</div>'; return; }
    html += '<div class="pcard">';
    rows.forEach(a => {
      const measured = a.actual == null
        ? '<span class="kval">' + esc(a.note || '-') + '</span>'
        : '<span class="kval"><b>' + fmtNum(a.actual) + '</b></span>';
      const budget = a.actual == null ? '' :
        '<span class="kbudget">' + esc(a.op) + ' ' + fmtNum(a.budget) + '</span>';
      html += '<div class="krow"><span class="kt ' + a.status + '">' +
        (a.status === 'pass' ? 'PASS' : a.status === 'fail' ? 'FAIL' : 'SKIP') + '</span>' +
        '<span class="kspec">' + esc(a.spec) +
        (a.label ? '<span class="klabel">' + esc(a.label) + '</span>' : '') + '</span>' +
        measured + budget + '</div>';
    });
    chkpanel.innerHTML = html + '</div>';
  }
  // Counts read as integers; times keep their decimals.
  function fmtNum(v) { return Math.abs(v - Math.round(v)) < 1e-9 ? String(Math.round(v)) : v.toFixed(3); }
  function setChk(on) {
    chkOn = on;
    if (on) { setFlame(false); setSql(false); setSrc(false); }
    document.body.classList.toggle('chk', on);
    chkbtn.classList.toggle('on', on);
    if (on) buildChecks();
    syncViewBar();
    save();
  }

  function setSrc(on) {
    srcOn = on;
    if (on && chkOn) setChk(false);
    if (on && flameOn) setFlame(false);
    if (on && sqlOn) setSql(false);
    document.body.classList.toggle('src', on);
    srcbtn.classList.toggle('on', on);
    if (on) buildSrc();
    syncViewBar();
    save();
  }

  // --- select a function: highlight it, sync the list, optionally center it ---
  function selectNode(v, center) {
    selected = v;
    highlight(v);
    if (center) {
      const rect = svg.getBoundingClientRect();
      tx = rect.width / 2 - (xy[v].x + W / 2) * scale;
      ty = rect.height / 2 - (xy[v].y + H / 2) * scale;
      apply();
    }
    buildList();
    save();
  }
  function clearSelection() {
    selected = -1;
    nodeEls.forEach(g => g.classList.remove('dim', 'sel'));
    edgeEls.forEach(p => p.classList.remove('hi'));
    buildList();
  }

  // --- critical path: the heaviest root->leaf chain for the current metric ---
  function applyCritical() {
    nodeEls.forEach(g => g.classList.remove('critical'));
    edgeEls.forEach(p => p.classList.remove('critical'));
    if (!critOn) return;
    const fN = frameNode[cur], fE = frameEdge[cur];
    const out = NAMES.map(() => []), incoming = new Array(NAMES.length).fill(0);
    EDGES.forEach((e, k) => { if (fE.has(e.key)) { out[e.from].push({ to: e.to, k }); incoming[e.to]++; } });
    let root = -1, rv = -1;
    NAMES.forEach((name, v) => { const n = fN.get(name); if (n && incoming[v] === 0) { const val = nodeInclShare(n); if (val > rv) { rv = val; root = v; } } });
    if (root < 0) return;
    let v = root, guard = 0; const seen = new Set();
    while (v >= 0 && !seen.has(v) && guard++ < 4096) {
      seen.add(v);
      nodeEls[v].classList.add('critical');
      let bestK = -1, bestTo = -1, bestVal = -1;
      out[v].forEach(o => { const cn = fN.get(NAMES[o.to]); const val = cn ? nodeInclShare(cn) : -1; if (val > bestVal) { bestVal = val; bestK = o.k; bestTo = o.to; } });
      if (bestK < 0) break;
      edgeEls[bestK].classList.add('critical');
      v = bestTo;
    }
  }

  // --- threshold pruning: hide functions below pruneT% of the current metric ---
  function applyPrune() {
    const fN = frameNode[cur], pruned = new Set();
    NAMES.forEach((name, v) => {
      const n = fN.get(name);
      const keep = n && (pruneT <= 0 || nodeInclShare(n) >= pruneT);
      nodeEls[v].classList.toggle('pruned', !keep);
      if (!keep) pruned.add(v);
    });
    edgeEls.forEach((p, k) => {
      const e = EDGES[k], hide = pruned.has(e.from) || pruned.has(e.to);
      p.classList.toggle('pruned', hide);
      if (p._lbl) p._lbl.classList.toggle('pruned', hide);
    });
  }
  critbtn.addEventListener('click', () => { critOn = !critOn; syncMenu(); paint(cur); save(); });
  pruneSel.addEventListener('change', () => { pruneT = parseFloat(pruneSel.value) || 0; paint(cur); save(); });
  flamebtn.addEventListener('click', () => setFlame(!flameOn));
  sqlbtn.addEventListener('click', () => setSql(!sqlOn));
  srcbtn.addEventListener('click', () => setSrc(!srcOn));
  groupbtn.addEventListener('click', () => { grouped = !grouped; groupbtn.classList.toggle('on', grouped); buildList(); save(); });
  // Self / Inclusive: re-ranks the list and re-prints the boxes; the heat stays.
  scopeEls.forEach(b => b.addEventListener('click', () => { if (scope === b.dataset.scope) return; scope = b.dataset.scope; paint(cur); }));
  // The graph-only tools are meaningless without exact per-edge weights.
  if (!DATA.exact) { critbtn.classList.add('off'); document.getElementById('prunerow').classList.add('off'); }
  if (hasQueries) sqlbtn.style.display = '';
  if (LINES && LINES.source && LINES.source.length) srcbtn.style.display = '';
  // Offer grouping only when names actually carry a class or namespace.
  // (Explicit value: the default `display:none` lives in a CSS rule, so ''
  // would fall back to it rather than reveal the button.)
  if (hasGroups) groupbtn.style.display = 'inline-grid';

  // --- navigator / pills ---
  if (DATA.live || FRAMES.length > 1) { nav.classList.add('show'); document.body.classList.add('hasnav'); }
  function buildPills() {
    pills.innerHTML = '';
    FRAMES.forEach((f, i) => {
      const b = document.createElement('div');
      b.className = 'pill';
      const maxExcl = f.nodes.reduce((m, n) => Math.max(m, n.excl), 0);
      b.style.background = heat(maxExcl);
      b.title = 'frame ' + (i+1) + ' · ' + f.total + ' samples';
      b.addEventListener('click', () => goto(i, true));
      pills.appendChild(b);
    });
  }
  function updateNav() {
    const kids = pills.children;
    for (let i = 0; i < kids.length; i++) kids[i].classList.toggle('active', i === cur);
    const f = FRAMES[cur];
    const ts = f.ts ? new Date(f.ts).toLocaleTimeString() : '';
    navlabel.textContent = 'frame ' + (cur+1) + '/' + FRAMES.length + (ts ? ' · ' + ts : '');
    liveBtn.classList.toggle('on', follow);
    document.getElementById('livetext').textContent = follow ? 'live' : 'paused';
  }

  // --- follow latest / live update ---
  // Served over http(s): re-fetch this page and merge new frames IN PLACE — no
  // reload, no flicker (a grown union relayouts in place too). Opened as a file:
  // fall back to a full reload.
  const canPoll = location.protocol !== 'file:';
  function startReload() {
    if (!DATA.live || reloadTimer) return;
    reloadTimer = setInterval(() => { if (canPoll) pollUpdate(); else { save(); location.reload(); } }, DATA.reloadMs || 3500);
  }
  function stopReload() { if (reloadTimer) { clearInterval(reloadTimer); reloadTimer = null; } }
  function pollUpdate() {
    fetch(location.href, {cache: 'no-store'}).then(r => r.text()).then(text => {
      const m = text.match(/const DATA = (\{[\s\S]*?\});/);
      if (m) ingest(JSON.parse(m[1]).frames);
    }).catch(() => {});
  }
  function ingest(nf) {
    if (!nf || !nf.length) return;
    const known = new Set(NAMES);
    const grew = nf.some(f => f.nodes.some(n => !known.has(n.name)) || f.edges.some(e => !uEdge.has(edgeKey(e.from, e.to))));
    const curTs = FRAMES[cur] ? FRAMES[cur].ts : 0;
    FRAMES.length = 0; nf.forEach(f => FRAMES.push(f));
    computeFrameMaps();
    if (grew) rebuild();   // a new function/edge: relayout the skeleton in place
    buildPills();
    let idx = follow ? FRAMES.length - 1 : FRAMES.findIndex(f => f.ts === curTs);
    if (idx < 0) idx = FRAMES.length - 1;
    paint(idx);
  }
  function goto(i, manual) { i = Math.max(0, Math.min(FRAMES.length - 1, i)); if (manual) { follow = false; stopReload(); } paint(i); }
  function setFollow(on) { follow = on; if (on) { paint(FRAMES.length - 1); startReload(); } else { stopReload(); paint(cur); } }
  document.getElementById('prev').addEventListener('click', () => goto(cur - 1, true));
  document.getElementById('next').addEventListener('click', () => goto(cur + 1, true));
  liveBtn.addEventListener('click', () => setFollow(!follow));
  diffBox.addEventListener('change', () => { diffOn = diffBox.checked; paint(cur); });
  const availMetrics = METRICS.filter(m => m.on);
  const metricEls = {};
  function setMetric(m) {
    metric = m;
    Object.keys(metricEls).forEach(k => metricEls[k].classList.toggle('on', k === metric));
    paint(cur);
  }
  function buildMetricBar() {
    if (availMetrics.length < 2) return; // nothing to switch (sampled captures)
    metricbar.classList.add('show');
    availMetrics.forEach(m => {
      const b = document.createElement('button');
      b.innerHTML = '<span class="g">' + icon(m.icon) + '</span><span>' + m.label + '</span>';
      b.dataset.tip = 'Colour every function by ' + m.label.toLowerCase() +
        ' \u2014 the list re-sorts too (m cycles)';
      b.addEventListener('click', () => setMetric(m.key));
      metricbar.appendChild(b);
      metricEls[m.key] = b;
    });
  }

  // --- view switcher: graph / flame / queries / source ---
  // These four are mutually exclusive, so they belong in ONE control. Three
  // independent on/off buttons made the exclusivity invisible and ate header
  // width the fixed-height bar cannot spare.
  const VIEWS = [
    { key: 'graph', label: 'Graph',   icon: 'graph', on: () => true,
      hint: 'Who calls whom, one box per function' },
    { key: 'flame', label: 'Flame',   icon: 'flame', on: () => !!DATA.exact,
      hint: 'The same tree as nested bars, width = time. Click to zoom (f)' },
    { key: 'sql',   label: 'Queries', icon: 'db', on: () => hasQueries,
      hint: 'Every distinct DB statement and how many times it ran (q)' },
    { key: 'src',   label: 'Source',  icon: 'code', on: () => !!LINES,
      hint: 'Your PHP file with the cost of each line (s)' },
    { key: 'chk',   label: 'Checks',  icon: 'check', on: () => ASSERTS.length > 0,
      hint: 'The performance budget and what this run measured (c)' },
  ];
  const viewbar = document.getElementById('viewbar');
  const viewEls = {};
  function currentView() { return flameOn ? 'flame' : sqlOn ? 'sql' : srcOn ? 'src' : chkOn ? 'chk' : 'graph'; }
  function setView(key) {
    if (key === 'flame') setFlame(true);
    else if (key === 'sql') setSql(true);
    else if (key === 'src') setSrc(true);
    else if (key === 'chk') setChk(true);
    else { setFlame(false); setSql(false); setSrc(false); setChk(false); }
    syncViewBar();
  }
  function syncViewBar() {
    const cur = currentView();
    Object.keys(viewEls).forEach(k => viewEls[k].classList.toggle('on', k === cur));
  }
  function buildViewBar() {
    const avail = VIEWS.filter(v => v.on());
    if (avail.length < 2) return;   // nothing to switch between
    viewbar.classList.add('show');
    avail.forEach(v => {
      const b = document.createElement('button');
      b.innerHTML = '<span class="g">' + icon(v.icon) + '</span><span>' + v.label + '</span>';
      b.dataset.tip = v.hint;
      b.addEventListener('click', () => setView(v.key));
      viewbar.appendChild(b);
      viewEls[v.key] = b;
    });
    syncViewBar();
  }
  buildMetricBar();
  buildViewBar();

  // --- the page's chrome (theme, header hints, the menu) is PAGE's, set up
  // before this script so it works on a page with no profile. The graph hooks
  // in: a theme change repaints it, and the menu shows its toggles' state.
  PAGE.graphReady();
  PAGE.onTheme(() => { if (typeof cur === 'number' && frameNode && frameNode[cur]) paint(cur); });
  // Toggles keep their state visible in the menu, since the header no longer shows it.
  function syncMenu() {
    document.getElementById('critbtn').classList.toggle('on', critOn);
  }
  PAGE.onMenuOpen(syncMenu);
  const setMenu = PAGE.setMenu, menuOpen = PAGE.menuOpen;

  // --- keyboard help ---
  // Listed only when the capture actually supports the shortcut, so the sheet
  // never advertises a key that does nothing on this page.
  const helpEl = document.getElementById('help'), helpbtn = document.getElementById('helpbtn');
  function buildHelp() {
    const rows = [];
    if (FRAMES.length > 1) rows.push(['← →', 'previous / next capture']);
    if (DATA.live) rows.push(['l', 'follow the newest capture']);
    if (FRAMES.length > 1) rows.push(['d', 'diff against the previous capture']);
    if (availMetrics.length > 1) rows.push(['m', 'cycle the cost dimension']);
    if (DATA.exact) rows.push(['f', 'flame graph'], ['p', 'critical path']);
    if (hasQueries) rows.push(['q', 'DB queries']);
    if (LINES) rows.push(['s', 'source, per line']);
    if (ASSERTS.length) rows.push(['c', 'the performance budget']);
    rows.push(['0', 'fit the graph in view']);
    rows.push(['r', 'start over — graph, time, no filters']);
    rows.push(['Esc', 'reset the flame zoom / close this']);
    rows.push(['?', 'this sheet']);
    document.getElementById('helplist').innerHTML = rows
      .map(([k, what]) => '<dt>' + esc(k) + '</dt><dd>' + esc(what) + '</dd>')
      .join('');
  }
  function toggleHelp(on) {
    if (on && helpEl.hasAttribute('hidden')) buildHelp();
    helpEl.toggleAttribute('hidden', !on);
  }
  helpbtn.addEventListener('click', () => toggleHelp(helpEl.hasAttribute('hidden')));
  helpEl.addEventListener('click', () => toggleHelp(false));

  window.addEventListener('keydown', ev => {
    if (ev.target.tagName === 'INPUT' || ev.target.tagName === 'SELECT') return;
    // Esc closes the sheet before anything else can consume it.
    if (ev.key === 'Escape' && menuOpen()) { setMenu(false); return; }
    if (ev.key === 'Escape' && !helpEl.hasAttribute('hidden')) { toggleHelp(false); return; }
    if (ev.key === '?') { toggleHelp(helpEl.hasAttribute('hidden')); return; }
    if (ev.key === 'ArrowLeft') goto(cur - 1, true);
    else if (ev.key === 'ArrowRight') goto(cur + 1, true);
    else if (ev.key === 'l' || ev.key === 'L') setFollow(!follow);
    else if (ev.key === 'd' || ev.key === 'D') { diffOn = !diffOn; diffBox.checked = diffOn; paint(cur); }
    else if (ev.key === 'p' || ev.key === 'P') critbtn.click();
    else if ((ev.key === 'f' || ev.key === 'F') && DATA.exact) flamebtn.click();
    else if ((ev.key === 'q' || ev.key === 'Q') && hasQueries) sqlbtn.click();
    else if ((ev.key === 's' || ev.key === 'S') && LINES) setSrc(!srcOn);
    else if ((ev.key === 'c' || ev.key === 'C') && ASSERTS.length) setChk(!chkOn);
    else if (ev.key === '0' || ev.key === 'Home') fitView(false);
    else if (ev.key === 'r' || ev.key === 'R') resetAll();
    else if (ev.key === 'Escape' && flameOn && flameRoot >= 0) { flameRoot = -1; buildFlame(); save(); }
    else if (ev.key === 'Escape' && flameOn && TL && flameMode === 'timeline' && (tlT0 > 0 || tlT1 < TL.totalNs)) { tlT0 = 0; tlT1 = TL.totalNs; tlPick = -1; buildFlame(); }
    else if ((ev.key === 'm' || ev.key === 'M') && availMetrics.length > 1) {
      const idx = availMetrics.findIndex(x => x.key === metric);
      setMetric(availMetrics[(idx + 1) % availMetrics.length].key);
    }
  });

  // --- zoom / pan (persisted so a live update keeps your view) ---
  function apply() { vp.setAttribute('transform', 'translate(' + tx + ',' + ty + ') scale(' + scale + ')'); }

  // --- fit the graph in view ---
  // Zoom and pan are unbounded, so it is easy to end up looking at empty space
  // with no idea which way the graph is. This always brings it back, framed on
  // what is actually on screen: pruned and absent nodes are excluded, so
  // fitting after pruning frames the nodes you kept, not the ones you hid.
  // True while nobody has moved the view by hand: an automatic framing may be
  // recomputed, a chosen one must never be.
  let viewIsAuto = true;
  // The smallest scale an arrival framing goes to: card names (12 px) still
  // read at about 9 px.
  const READABLE_SCALE = 0.75;
  function fitView(auto) {
    const rect = svg.getBoundingClientRect();
    // The page can be laid out before it has been given a size — an artifact
    // frame sizes its iframe after load — and fitting into a zero-width box
    // yields a scale that puts the graph nowhere. Wait to be measured.
    if (rect.width < 40 || rect.height < 40) return false;
    if (!auto) viewIsAuto = false;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity, any = false;
    NAMES.forEach((name, v) => {
      const g = nodeEls[v];
      if (!g || !xy[v] || g.classList.contains('pruned') || g.classList.contains('absent')) return;
      any = true;
      minX = Math.min(minX, xy[v].x); minY = Math.min(minY, xy[v].y);
      maxX = Math.max(maxX, xy[v].x + W); maxY = Math.max(maxY, xy[v].y + H);
    });
    if (!any) { scale = 1; tx = 20; ty = 20; apply(); save(); return true; }
    const pad = 28;
    const spanX = Math.max(1, maxX - minX), spanY = Math.max(1, maxY - minY);
    // Never zoom IN to fit: blowing a two-node graph up to fill the screen is
    // disorienting in the other direction.
    scale = Math.max(0.05, Math.min(1, Math.min(
      (rect.width - pad * 2) / spanX,
      (rect.height - pad * 2) / spanY)));
    tx = (rect.width - spanX * scale) / 2 - minX * scale;
    ty = (rect.height - spanY * scale) / 2 - minY * scale;
    // On arrival, a graph too big to fit at a readable size opens readable
    // instead: the top of the tree, centred on its entry point, at the smallest
    // scale whose card text still reads. A request of a real application has
    // dozens of functions, and fitting all of them shrank every card to a
    // smudge. Fit in view (0) still frames the whole graph.
    if (auto && scale < READABLE_SCALE) {
      scale = READABLE_SCALE;
      let rootX = 0, roots = 0;
      NAMES.forEach((name, v) => {
        const g = nodeEls[v];
        if (!g || !xy[v] || g.classList.contains('pruned') || g.classList.contains('absent') || xy[v].y !== minY) return;
        rootX += xy[v].x + W / 2; roots++;
      });
      const centre = roots ? rootX / roots : (minX + maxX) / 2;
      tx = rect.width / 2 - centre * scale;
      ty = pad - minY * scale;
    }
    apply(); save();
    return true;
  }

  /// Whether the current transform leaves any node inside the viewport.
  function anyNodeOnScreen() {
    const rect = svg.getBoundingClientRect();
    if (rect.width < 40) return true;   // unmeasured: do not judge it
    return NAMES.some((name, v) => {
      const g = nodeEls[v];
      if (!g || !xy[v] || g.classList.contains('pruned') || g.classList.contains('absent')) return false;
      const x = xy[v].x * scale + tx, y = xy[v].y * scale + ty;
      return x + W * scale > 0 && x < rect.width && y + H * scale > 0 && y < rect.height;
    });
  }

  // Re-frame while the view is still automatic, so a page that is measured
  // late — or a window that changes size — still opens on the graph.
  if (window.ResizeObserver) {
    let first = true;
    new ResizeObserver(() => {
      if (first) { first = false; return; }
      if (viewIsAuto) fitView(true);
    }).observe(stage);
  }
  const fitbtn = document.getElementById('fitbtn');
  fitbtn.addEventListener('click', () => fitView(false));

  // --- start over ---
  // Everything the reader can get lost in goes back to its opening state: the
  // call graph, timed, nothing selected, filtered or searched, framed. The
  // things that are preferences rather than state — theme, sidebar width,
  // grouping — are deliberately left alone; resetting those would be a
  // surprise, not a rescue.
  function resetAll() {
    selected = -1;
    nodeEls.forEach(g => g.classList.remove('dim'));
    edgeEls.forEach(p => p.classList.remove('hi'));
    searchInput.value = '';
    nodeEls.forEach(g => g.classList.remove('search-dim'));
    critOn = false;
    pruneT = 0; pruneSel.value = '0';
    flameRoot = -1;
    setView('graph');
    if (availMetrics.some(m => m.key === 'time')) metric = 'time';
    Object.keys(metricEls).forEach(k => metricEls[k].classList.toggle('on', k === metric));
    syncMenu();
    paint(cur);
    viewIsAuto = true;
    fitView(true);
  }
  document.getElementById('resetbtn').addEventListener('click', () => { resetAll(); setMenu(false); });
  // The flame, queries, source and checks panels are children of the stage, so
  // their wheel and mousedown events bubble here. Zooming and panning belong to
  // the graph alone: consuming those events in a panel meant the panel could
  // not scroll and its text could not be selected.
  function graphIsActive() { return !flameOn && !sqlOn && !srcOn && !chkOn; }
  stage.addEventListener('wheel', ev => {
    if (!graphIsActive()) return;   // let the open panel scroll normally
    ev.preventDefault();
    const f = ev.deltaY < 0 ? 1.1 : 0.9;
    const rect = svg.getBoundingClientRect();
    const mx = ev.clientX - rect.left, my = ev.clientY - rect.top;
    tx = mx - (mx - tx) * f; ty = my - (my - ty) * f; scale *= f; viewIsAuto = false; apply(); save();
  }, {passive: false});
  stage.addEventListener('mousedown', ev => {
    if (!graphIsActive()) return;
    panning = true; viewIsAuto = false; psx = ev.clientX - tx; psy = ev.clientY - ty;
    dragMoved = false; dragX = ev.clientX; dragY = ev.clientY;
    stage.classList.add('grabbing');
  });
  window.addEventListener('mousemove', ev => {
    if (!panning) return;
    // A few pixels of travel is a shaky click, not a drag.
    if (Math.abs(ev.clientX - dragX) > 4 || Math.abs(ev.clientY - dragY) > 4) dragMoved = true;
    tx = ev.clientX - psx; ty = ev.clientY - psy; apply();
  });
  window.addEventListener('mouseup', () => { if (panning) { panning = false; stage.classList.remove('grabbing'); save(); } });

  // --- resizable sidebar ---
  function applySidebar(px) { sbw = px; document.documentElement.style.setProperty('--sidebar-w', px + 'px'); }
  dragger.addEventListener('mousedown', ev => {
    ev.preventDefault();
    document.body.classList.add('resizing');
    const move = e => applySidebar(Math.max(200, Math.min(680, e.clientX)));
    const up = () => {
      document.body.classList.remove('resizing');
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
      save();
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  });

  // --- persistence: survive a live reload/update without losing place ---
  // A captured request's profile (`monitor --apm --serve`, `/?trace=<id>`)
  // carries the service's title too, but it is another graph: its pan and
  // zoom restored onto the service's page — or the reverse — opened on a view
  // that half-missed the graph. It gets a key of its own, the capture's id
  // and time, since ids start over with a new collector.
  const CAPTURE = DATA.apm && DATA.apm.trace;
  const STORE_KEY = 'elephc-callgraph:' + (DATA.title || '') +
    (CAPTURE ? ':trace:' + CAPTURE.id + ':' + CAPTURE.t : '');
  function save() { try { localStorage.setItem(STORE_KEY, JSON.stringify({follow: follow, diff: diffOn, metric: metric, scope: scope, fm: flameMode, sbw: sbw, crit: critOn, prune: pruneT, grouped: grouped, selTs: FRAMES[cur] ? FRAMES[cur].ts : 0, tx: tx, ty: ty, scale: scale})); } catch (e) {} writeHash(); }
  function load() { try { return JSON.parse(localStorage.getItem(STORE_KEY)) || {}; } catch (e) { return {}; } }

  // --- shareable URL: the discrete view (dimension, selection, active view,
  // prune, critical path) rides in location.hash so a copied link reopens it.
  // Pan/zoom stay out — those are per-viewer, and localStorage already keeps them.
  function writeHash() {
    try {
      const p = [];
      if (metric !== 'time') p.push('m=' + metric);
      if (scope === 'incl') p.push('sc=incl');
      if (selected >= 0 && NAMES[selected]) p.push('s=' + encodeURIComponent(NAMES[selected]));
      if (flameOn) { p.push('v=flame'); if (flameRoot >= 0 && NAMES[flameRoot]) p.push('fr=' + encodeURIComponent(NAMES[flameRoot])); }
      else if (sqlOn) p.push('v=sql');
      else if (srcOn) p.push('v=src');
      else if (chkOn) p.push('v=chk');
      if (TL && flameMode === 'merged') p.push('fm=merged');
      if (pruneT) p.push('pr=' + pruneT);
      if (critOn) p.push('c=1');
      // The transactions view (monitor --apm) owns `a`, `ar`, `at`, a shared link's period
      // and comparison (`afrom`, `ato`, `acmp`) and the section it opens on (`as`); keep
      // them all (review).
      (location.hash || '').replace(/^#/, '').split('&').forEach(kv => { if (/^(a|ar|at|afrom|ato|acmp|as)=/.test(kv)) p.push(kv); });
      const h = p.length ? '#' + p.join('&') : location.pathname + location.search;
      if (('#' + p.join('&')) !== location.hash) history.replaceState(null, '', h);
    } catch (e) {}
  }
  // A hand-edited link can carry a lone '%', which decodeURIComponent throws
  // on; uncaught, it stopped the page's script before the graph was drawn.
  function safeDecode(text) {
    try { return decodeURIComponent(text); } catch (_) { return text; }
  }
  function readHash() {
    const out = {};
    (location.hash || '').replace(/^#/, '').split('&').forEach(kv => {
      if (!kv) return; const i = kv.indexOf('='); const k = i < 0 ? kv : kv.slice(0, i);
      out[k] = i < 0 ? '' : safeDecode(kv.slice(i + 1));
    });
    return out;
  }

  // --- init ---
  // Monitor titles are file paths. Ellipsizing one hides the only part that
  // identifies it (the file name), so show the last segment and keep the whole
  // path on hover.
  (function shortenTitle() {
    const h1 = document.querySelector('header h1');
    if (!h1) return;
    const full = h1.textContent.trim();
    h1.title = full;
    const cut = full.lastIndexOf('/');
    if (cut >= 0 && cut < full.length - 1) h1.textContent = full.slice(cut + 1);
  })();
  computeFrameMaps(); rebuild(); buildPills();
  const saved = load();
  const hash = readHash();   // an explicit shared link wins over sticky localStorage
  diffOn = !!saved.diff; diffBox.checked = diffOn;
  if (saved.metric && availMetrics.some(m => m.key === saved.metric)) metric = saved.metric;
  if (hash.m && availMetrics.some(m => m.key === hash.m)) metric = hash.m;
  if (saved.scope === 'incl' || hash.sc === 'incl') scope = 'incl';
  if (TL && saved.fm === 'merged') flameMode = 'merged';
  if (TL && hash.fm) flameMode = hash.fm === 'merged' ? 'merged' : 'timeline';
  if (hash.sc === 'self') scope = 'self';
  Object.keys(metricEls).forEach(k => metricEls[k].classList.toggle('on', k === metric));
  if (saved.sbw) applySidebar(saved.sbw);
  critOn = !!saved.crit || hash.c === '1';
  grouped = !!saved.grouped; groupbtn.classList.toggle('on', grouped);
  pruneT = hash.pr ? (parseFloat(hash.pr) || 0) : (saved.prune || 0); pruneSel.value = String(pruneT);
  const hadSavedView = typeof saved.tx === 'number';
  if (hadSavedView) { tx = saved.tx; ty = saved.ty; scale = saved.scale; viewIsAuto = false; }
  apply();
  const startFollow = (typeof saved.follow === 'boolean') ? saved.follow : DATA.live;
  if (startFollow) { setFollow(true); }
  else { let idx = FRAMES.findIndex(f => f.ts === saved.selTs); if (idx < 0) idx = FRAMES.length - 1; follow = false; paint(idx); }
  // View + selection from the shared link, once the layout/names exist.
  if (hash.v === 'flame' && DATA.exact) { if (hash.fr && nameId.has(hash.fr)) flameRoot = nameId.get(hash.fr); setFlame(true); }
  else if (hash.v === 'sql' && hasQueries) setSql(true);
  else if (hash.v === 'src' && LINES) setSrc(true);
  else if (hash.v === 'chk' && ASSERTS.length) setChk(true);
  // First visit: frame the graph rather than open at a fixed corner, which on a
  // wide layout starts you looking at one node with the rest off-screen. Runs
  // after paint so pruning is already applied, and never overrides a view the
  // reader chose or a node a shared link points at.
  // Frame the graph unless the reader is being sent somewhere specific. A
  // restored view that shows no node at all is discarded rather than honoured:
  // opening on empty space is indistinguishable from a page that failed.
  if (!hash.s && (!hadSavedView || !anyNodeOnScreen())) { viewIsAuto = !hadSavedView; fitView(true); }
  if (hash.s && nameId.has(hash.s)) selectNode(nameId.get(hash.s), true);
})();
</script>
__APM_SCRIPT__</body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::*;

    /// A two-function graph with known shares, so an export's numbers can be
    /// asserted literally rather than approximately.
    fn sample_graph() -> CallGraph {
        CallGraph {
            nodes: vec![
                GraphNode {
                    name: "{main}".into(),
                    inclusive: 100,
                    exclusive: 2,
                    call_count: Some(1),
                    alloc_inclusive: 50,
                    alloc_exclusive: 1,
                    io_inclusive: 0,
                    io_exclusive: 0,
                    retained_inclusive: 0,
                    retained_exclusive: 0,
                    wait_inclusive: 0,
                    wait_exclusive: 0,
                    network_inclusive: 0,
                    network_exclusive: 0,
                    network_wait_inclusive: 0,
                    network_wait_exclusive: 0,
                    causes: vec![],
                },
                GraphNode {
                    name: "hot_leaf".into(),
                    inclusive: 80,
                    exclusive: 60,
                    call_count: Some(40),
                    alloc_inclusive: 40,
                    alloc_exclusive: 40,
                    io_inclusive: 0,
                    io_exclusive: 0,
                    retained_inclusive: 0,
                    retained_exclusive: 0,
                    wait_inclusive: 0,
                    wait_exclusive: 0,
                    network_inclusive: 0,
                    network_exclusive: 0,
                    network_wait_inclusive: 0,
                    network_wait_exclusive: 0,
                    causes: vec![("heap allocation".into(), 30), ("Mixed cell boxing".into(), 20)],
                },
            ],
            edges: vec![GraphEdge { from: 0, to: 1, weight: 80, count: None }],
            total: 100,
            queries: Vec::new(),
            query_callers: Default::default(),
            lines: None,
            trace: None,
        }
    }

    #[test]
    /// The DOT export carries the nodes, the edges and the cost breakdown — a
    /// graph without causes is a picture rather than a profile.
    fn dot_encodes_nodes_edges_and_causes() {
        let dot = render_dot(&sample_graph());
        assert!(dot.starts_with("digraph elephc_callgraph {"));
        assert!(dot.contains("incl 80.0% · self 60.0%"), "{dot}");
        assert!(dot.contains("calls 40"), "{dot}");
        assert!(dot.contains("heap allocation: 30%"), "{dot}");
        assert!(dot.contains("n0 -> n1 [label=\"80%\"]"), "{dot}");
    }

    #[test]
    /// Spans nest by parent and their bars scale within their own trace, so one
    /// slow trace does not flatten every other one on the page.
    fn trace_html_nests_spans_and_scales_bars_within_a_trace() {
        let span = |service: &str, span_id: &str, parent: &str, ns: u64| TraceSpan {
            service: service.into(),
            trace_id: "tr".into(),
            span_id: span_id.into(),
            parent_span_id: parent.into(),
            total_ns: ns,
            functions: 2,
            queries: 0,
            wait_ns: 0,
            network_ops: 0,
            network_wait_ns: 0,
            start_us: None,
            top: vec![("handle".into(), 100.0, 40.0)],
        };
        let mut gateway = span("gateway", "aaaa", "", 1_000_000);
        gateway.network_ops = 2;
        gateway.network_wait_ns = 300_000;
        let html = render_trace_html(
            &[
                gateway,
                span("inventory", "bbbb", "aaaa", 250_000),
                // Parent absent from the capture: must still render, as a root.
                span("billing", "cccc", "zzzz", 500_000),
            ],
            "demo",
        );
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("1 trace · 3 spans"), "{html}");
        // Depth drives the indent: the child is one level in, the orphan is not.
        assert!(html.contains("style=\"--d:0\"><summary><span class=\"svc\">gateway"), "{html}");
        assert!(html.contains("style=\"--d:1\"><summary><span class=\"svc\">inventory"), "{html}");
        assert!(html.contains("style=\"--d:0\"><summary><span class=\"svc\">billing"), "{html}");
        // Bars scale against the trace's slowest span, not a global maximum.
        assert!(html.contains("width:100.00%"), "{html}");
        assert!(html.contains("width:25.00%"), "{html}");
        assert!(html.contains("2 network"), "{html}");
        assert!(html.contains("300.0 µs network wait"), "{html}");
        // Self-contained: no network reference of any kind.
        assert!(!html.contains("http://"), "{html}");
        assert!(!html.contains("https://"), "{html}");
    }

    /// Dated spans are PLACED on a shared axis; sequential hops step rightwards
    /// and concurrent ones share an offset. Without this the chart compares
    /// durations and silently loses the sequential-vs-parallel distinction, which
    /// is the whole reason to correlate services in the first place.
    #[test]
    fn trace_html_places_dated_spans_on_a_shared_axis() {
        let span = |id: &str, parent: &str, start_us: Option<u64>, ns: u64| TraceSpan {
            service: format!("svc-{id}"),
            trace_id: "tr".into(),
            span_id: id.into(),
            parent_span_id: parent.into(),
            total_ns: ns,
            functions: 1,
            queries: 0,
            wait_ns: 0,
            network_ops: 0,
            network_wait_ns: 0,
            start_us,
            top: vec![],
        };
        // Root spans 0..1000ms; two children, one at +0ms and one at +500ms.
        let html = render_trace_html(
            &[
                span("aaaa", "", Some(1_000_000), 1_000_000_000),
                span("bbbb", "aaaa", Some(1_000_000), 200_000_000),
                span("cccc", "aaaa", Some(1_500_000), 200_000_000),
            ],
            "t",
        );
        // Count the bars' inline styles only — the stylesheet has `margin-left` rules
        // of its own, and matching those would make this assertion meaningless.
        let bars = html.matches("<i style=\"margin-left:").count();
        assert_eq!(bars, 3, "every span gets an offset: {html}");
        assert!(html.contains("margin-left:0.00%"), "the earliest starts at 0: {html}");
        // The late child opens 500ms into a 1000ms window.
        assert!(html.contains("margin-left:50.00%"), "the late hop is placed: {html}");

        // One undated member drops the whole trace back to flush-left bars, rather
        // than pinning the unknown one at the origin and lying about ordering.
        let mixed = render_trace_html(
            &[
                span("aaaa", "", Some(1_000_000), 1_000_000_000),
                span("bbbb", "aaaa", None, 200_000_000),
            ],
            "t",
        );
        assert!(!mixed.contains("margin-left:50.00%"), "must not place a partial trace");
        assert_eq!(
            mixed.matches("margin-left:0.00%").count(),
            2,
            "all bars flush left when the trace is not fully dated: {mixed}"
        );

        // The page must not contradict itself. The footer is static while the layout
        // is conditional, so it has to describe BOTH cases — and the previous wording
        // ("durations, not start times, so siblings are not on a shared time axis")
        // survived the switch to a waterfall and told readers the opposite of what the
        // chart above it was showing. Only looking at the rendered page caught that.
        assert!(html.contains("shared time axis"), "footer must state the axis");
        assert!(
            html.contains("falls back to durations"),
            "footer must also state the undated fallback"
        );
        assert!(
            !html.contains("not on a shared time axis"),
            "footer still carries the pre-waterfall claim"
        );
    }

    #[test]
    /// Service and function names reach the page as text: they come from a
    /// profiled program, and a page that renders them as markup is an injection.
    fn trace_html_escapes_service_and_function_names() {
        let html = render_trace_html(
            &[TraceSpan {
                service: "<script>alert(1)</script>".into(),
                trace_id: "t<r>".into(),
                span_id: "a".into(),
                parent_span_id: String::new(),
                total_ns: 10,
                functions: 1,
                queries: 0,
                wait_ns: 0,
                network_ops: 0,
                network_wait_ns: 0,
                start_us: None,
                top: vec![("evil</td><img>".into(), 1.0, 1.0)],
            }],
            "ti<tle>",
        );
        assert!(!html.contains("<script>alert"), "service name must not inject: {html}");
        assert!(!html.contains("evil</td><img>"), "function name must not inject: {html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
    }

    #[test]
    /// The gradient hits its named stops exactly at both ends and at the gold
    /// midpoint, so the colour scale means the same thing across reports.
    fn heat_ramps_over_the_elephc_gradient() {
        assert_eq!(heat_color(0.0), "#f2e9e4"); // cold: warm pale
        assert_eq!(heat_color(0.08), "#ffd900"); // gold stop
        assert_eq!(heat_color(1.0), "#ff0070"); // hot: elephc magenta
        // Text stays readable at both ends of the ramp.
        assert_eq!(ink_for(0.0), "#201a17"); // dark ink over pale
        assert_eq!(ink_for(1.0), "#fff8f4"); // light ink over magenta
    }

    #[test]
    /// A page with no profile yet — a monitor page before its first capture —
    /// keeps its ☰ menu and its theme: they are wired in the `PAGE` block,
    /// which runs before the graph script and the early return it takes when
    /// there is nothing to draw, and that return hides only the menu rows a
    /// profile is needed for.
    fn the_page_chrome_is_wired_before_the_graph_script() {
        let html = render_html(&sample_graph(), "demo");
        let at = |needle: &str| html.find(needle).unwrap_or_else(|| panic!("missing `{needle}`"));
        let page = at("const PAGE = (function()");
        let graph = at("const FRAMES = DATA.frames || [];");
        let empty = at("metaEl.textContent = 'no samples';");
        for chrome in ["function applyTheme()", "menubtn.addEventListener('click'", "function setMenu(on)"] {
            let found = at(chrome);
            assert!(page < found && found < graph, "`{chrome}` must be wired before the graph script");
        }
        assert!(graph < empty, "the early return belongs to the graph script");
        assert!(html.contains("'#resetbtn, #mh-graph, #fitbtn, #critbtn, #prunerow, #mh-help, #helpbtn'"));
        assert!(html.contains("PAGE.graphReady();"), "the graph tells the page it runs");
    }

    #[test]
    /// Selecting a function opens it in place in the list, Blackfire-style:
    /// its figures, then Callers / Callees buttons; the bottom panel it
    /// replaces is gone from the page.
    fn a_selected_function_opens_in_place() {
        let html = render_html(&sample_graph(), "demo");
        assert!(html.contains("function detailItem(row)"));
        assert!(html.contains(".fndetail {"), "the card is styled");
        assert!(html.contains("'>Callers · '") && html.contains("'>Callees · '"));
        assert!(html.contains("class=\"dgo\""), "the button that centres the graph");
        assert!(!html.contains("id=\"detail\""), "the bottom panel is gone");
        assert!(!html.contains("buildDetail"));
    }

    #[test]
    /// A name in an attribute goes through `escAttr`, which escapes quotes:
    /// `esc` leaves them, and a capture shared with `--baseline` could carry a
    /// function named `x" onmouseover="…` that breaks out of a `title` built
    /// with it. No attribute of the page is built with `esc`.
    fn names_in_attributes_escape_their_quotes() {
        let html = render_html(&sample_graph(), "demo");
        for attribute in ["title", "data-v", "href", "alt", "value"] {
            let needle = format!("{attribute}=\"' + esc(");
            assert!(!html.contains(&needle), "`{attribute}` built with esc(), not escAttr()");
        }
        assert!(html.contains("title=\"' + escAttr(NAMES[r.v])"), "the neighbour rows of an open function");
        assert!(html.contains("title=\"' + escAttr(c.name)"), "the flame cells");
    }

    #[test]
    /// A lone `%` in a hand-edited link's fragment cannot stop a page's
    /// script: `decodeURIComponent` throws on it, so every decode of the page
    /// and of its Transactions view goes through `safeDecode` (review).
    fn a_malformed_fragment_cannot_stop_the_page() {
        let guarded = "try { return decodeURIComponent(text); }";
        let page = render_html(&sample_graph(), "demo");
        let view = include_str!("monitor/apm/view/script.js");
        for (name, source) in [("page", page.as_str()), ("Transactions view", view)] {
            assert_eq!(source.matches("decodeURIComponent(").count(), 1, "{name}: one decode, the guarded one");
            assert!(source.contains(guarded) && source.contains(": safeDecode(kv.slice(i + 1))"), "{name}");
        }
    }

    #[test]
    /// The report is one file with its data inlined — it has to open from a
    /// laptop with no network and no sibling assets.
    fn html_is_self_contained_and_embeds_data() {
        let html = render_html(&sample_graph(), "demo");
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("const DATA = {"));
        assert!(html.contains("\"total\":100"));
        assert!(html.contains("\"name\":\"hot_leaf\""));
        // Single-frame render carries exactly one frame and no live reload.
        assert!(html.contains("\"live\":false"));
        // No external resources: nothing is fetched over the network. The only
        // scheme in the page is the SVG XML namespace URI, which is never
        // dereferenced — strip it, then no scheme may remain.
        let stripped = html
            .replace("http://www.w3.org", "")
            .replace("https://www.w3.org", "");
        assert!(!stripped.contains("http://"), "unexpected external http URL");
        assert!(!stripped.contains("https://"), "unexpected external https URL");
    }

    #[test]
    /// A function named `</script>` must not close the embedded data block; the
    /// escaping is what keeps a profiled program from writing the page.
    fn html_neutralizes_script_close_in_names() {
        let graph = CallGraph {
            nodes: vec![GraphNode {
                name: "evil</script><img>".into(),
                inclusive: 1,
                exclusive: 1,
                call_count: None,
                alloc_inclusive: 0,
                alloc_exclusive: 0,
                io_inclusive: 0,
                io_exclusive: 0,
                retained_inclusive: 0,
                retained_exclusive: 0,
                wait_inclusive: 0,
                wait_exclusive: 0,
                network_inclusive: 0,
                network_exclusive: 0,
                network_wait_inclusive: 0,
                network_wait_exclusive: 0,
                causes: vec![],
            }],
            edges: vec![],
            total: 1,
            queries: Vec::new(),
            query_callers: Default::default(),
            lines: None,
            trace: None,
        };
        let html = render_html(&graph, "x");
        // The literal tag-closer must not survive inside the data blob.
        assert!(!html.contains("evil</script>"), "{html}");
        assert!(html.contains("evil<\\/script>"), "{html}");
    }

    #[test]
    /// A multi-frame report carries every captured window and marks which one
    /// is live, which is what the timeline scrubber navigates.
    fn multi_frame_embeds_every_frame_and_marks_live() {
        let g1 = sample_graph();
        let g2 = sample_graph();
        let html = render_html_frames(&[(1000, &g1), (2000, &g2)], "svc", true, 3, false, &[]);
        assert!(html.contains("\"live\":true"));
        assert!(html.contains("\"reloadMs\":3500"));
        assert!(html.contains("\"ts\":1000"));
        assert!(html.contains("\"ts\":2000"));
        // Edges are embedded by NAME so the union layout can resolve them.
        assert!(html.contains("\"from\":\"{main}\""));
        assert!(html.contains("\"to\":\"hot_leaf\""));
    }
}
