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
#[derive(serde::Serialize, serde::Deserialize)]
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
#[derive(serde::Serialize, serde::Deserialize)]
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
    /// `METHOD /path` the request was routed to. Empty outside `--web`, where
    /// there is no route, and for captures taken before slices recorded one.
    #[serde(default)]
    pub route: String,
}

/// Per-line self cost over one PHP source file, recovered from sampled
/// addresses via the dSYM. Sampled, not exact — see `monitor::line_profile`.
#[derive(serde::Serialize, serde::Deserialize)]
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

/// An aggregated call graph ready to render.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct CallGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub total: u64,
    /// Distinct DB queries and their execution counts (exact `--instrument`
    /// runs only), hottest first — the SQL panel / N+1 view. Empty otherwise.
    #[serde(default)]
    pub queries: Vec<(String, u64)>,
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

/// Renders a single **exact** (`--instrument`) capture: same page, relabeled so
/// the cost reads as measured time rather than a sample share.
pub(crate) fn render_html_exact(
    graph: &CallGraph,
    title: &str,
    asserts: &[AssertOutcome],
) -> String {
    render_html_frames(&[(0, graph)], title, false, 0, true, asserts)
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
                .map(|(sql, count)| serde_json::json!({"sql": sql, "count": count}))
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
    let data = serde_json::json!({
        "title": title,
        "live": live,
        "exact": exact,
        "reloadMs": reload_ms,
        "frames": frames_json,
        "lines": lines_json,
        "asserts": asserts_json,
    });
    // Neutralize `</script>` (and any `</…`) in embedded names: `\/` is a valid
    // JSON escape for `/`, so this stays parseable while it cannot close the tag.
    let data_json = serde_json::to_string(&data)
        .unwrap_or_else(|_| "{}".to_string())
        .replace("</", "<\\/");
    // Title is developer-supplied (the target path); insert it first. The frame
    // data — which carries attacker-influenceable PHP names — is inserted last so
    // no later replace can touch it.
    TEMPLATE_HTML
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
  --bg: #faf6f3; --panel: #ffffff; --ink: #201a17; --muted: #8a7f78;
  --border: #efe5df; --accent: #ff0070; --hover: rgba(0,0,0,.035);
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --bg: #17130f; --panel: #201b16; --ink: #f2ebe6; --muted: #a89c94;
    --border: #2c251f; --accent: #ff3d86; --hover: rgba(255,255,255,.05);
  }
}
:root[data-theme="dark"] {
  --bg: #17130f; --panel: #201b16; --ink: #f2ebe6; --muted: #a89c94;
  --border: #2c251f; --accent: #ff3d86; --hover: rgba(255,255,255,.05);
}
* { box-sizing: border-box; }
html, body { margin: 0; background: var(--bg); color: var(--ink);
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif; }
header { display: flex; align-items: center; gap: .75rem; height: 3.1rem; padding: 0 1rem;
  background: var(--panel); border-bottom: 1px solid var(--border); }
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
/* Warm neutrals biased toward the elephc gradient; magenta (#ff0070, the
   logo's hottest stop) is the interaction accent. */
:root {
  --bg: #faf6f3; --panel: #ffffff; --ink: #201a17; --muted: #8a7f78;
  --line: #e6dad2; --edge: #bcaca2; --border: #efe5df; --accent: #ff0070;
  --sidebar-w: 20rem; --sel: rgba(255,0,112,.09); --hover: rgba(0,0,0,.035);
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --bg: #17130f; --panel: #201b16; --ink: #f2ebe6; --muted: #a89c94;
    --line: #3a312b; --edge: #63564d; --border: #2c251f; --accent: #ff3d86;
    --sel: rgba(255,61,134,.17); --hover: rgba(255,255,255,.05);
  }
}
:root[data-theme="dark"] {
  --bg: #17130f; --panel: #201b16; --ink: #f2ebe6; --muted: #a89c94;
  --line: #3a312b; --edge: #63564d; --border: #2c251f; --accent: #ff3d86;
  --sel: rgba(255,61,134,.17); --hover: rgba(255,255,255,.05);
}
* { box-sizing: border-box; }
html, body { margin: 0; height: 100%; background: var(--bg); color: var(--ink);
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
/* Fixed-height app bar. The panes below are pinned at 3.1rem, so the header
   must never grow past it: a long title (monitor titles are file paths) would
   otherwise wrap and sit on top of the sidebar and the graph. Nothing wraps;
   the title is the only thing allowed to shrink, and it ellipsizes. */
header { display: flex; gap: .75rem; align-items: center; padding: 0 1rem;
  height: 3.1rem; box-sizing: border-box; flex-wrap: nowrap; overflow: hidden;
  border-bottom: 1px solid var(--border); background: var(--panel); }
header > * { flex: none; }
header .logo { flex: none; display: block; }
header h1 { font-size: 1rem; margin: 0; font-weight: 650; flex: 0 1 auto; min-width: 3rem;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
header .meta { color: var(--muted); font-size: .82rem; white-space: nowrap;
  flex: 0 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
header .spacer { flex: 1 1 auto; min-width: .25rem; }
header input { padding: .35rem .6rem; border: 1px solid var(--border);
  border-radius: 6px; background: var(--bg); color: var(--ink); font-size: .85rem; width: 13rem; }
/* Segmented controls: the view switcher and the dimension selector. Both are
   single-choice, so both read as one control rather than a row of toggles. */
#metricbar, #viewbar { display: none; align-items: center; gap: 2px; padding: 2px;
  background: var(--bg); border: 1px solid var(--border); border-radius: 8px; }
#metricbar.show, #viewbar.show { display: inline-flex; }
#metricbar button, #viewbar button { border: 0; background: transparent; color: var(--muted); cursor: pointer;
  padding: .26rem .55rem; border-radius: 6px; font-size: .78rem; display: inline-flex; align-items: center; gap: .3rem;
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
  background: var(--accent); color: #fff8f4; font-weight: 640; }
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
#flame { display: none; position: absolute; inset: 0; overflow: auto; cursor: default;
  background: var(--bg); padding: .4rem .5rem 1rem; }
body.flame #stage { cursor: default; }
body.flame #svg { display: none; }
body.flame #flame { display: block; }
#flame .frow { color: var(--muted); font-size: .74rem; padding: .1rem .1rem .35rem; }
#flame .frow b { color: var(--ink); }
#flame .fcanvas { position: relative; }
#flame .fcell { position: absolute; height: 20px; border-radius: 3px;
  border: 1px solid rgba(0,0,0,.16); box-sizing: border-box; overflow: hidden;
  font-size: .72rem; line-height: 18px; padding: 0 5px; white-space: nowrap; cursor: pointer;
  font-variant-numeric: tabular-nums; }
#flame .fcell:hover { outline: 2px solid var(--ink); outline-offset: -2px; z-index: 3; }
#flame .fcell.sel { outline: 2px solid var(--accent, #ff522c); outline-offset: -2px; z-index: 2; }
/* SQL panel: distinct DB statements and their execution counts (the N+1 view). */
#sqlpanel { display: none; position: absolute; inset: 0; overflow: auto; cursor: default;
  background: var(--bg); padding: .6rem .7rem 1.2rem; }
body.sql #stage { cursor: default; }
body.sql #svg, body.sql #flame { display: none; }
body.sql #sqlpanel { display: block; }
#sqlpanel .qhead { color: var(--muted); font-size: .76rem; padding: .1rem .1rem .55rem; }
#sqlpanel .qhead b { color: var(--ink); }
#sqlpanel table { border-collapse: collapse; width: 100%; max-width: 1100px; }
#sqlpanel th { text-align: left; font-size: .68rem; text-transform: uppercase; letter-spacing: .04em;
  color: var(--muted); font-weight: 600; padding: .3rem .5rem; border-bottom: 1px solid var(--border); }
#sqlpanel td { padding: .4rem .5rem; border-bottom: 1px solid var(--border); vertical-align: middle; }
#sqlpanel td.qc { font-variant-numeric: tabular-nums; font-weight: 650; white-space: nowrap; text-align: right; }
#sqlpanel td.qbar { width: 26%; }
#sqlpanel td.qbar i { display: block; height: 12px; border-radius: 3px; min-width: 2px; }
#sqlpanel td.qsql { width: 100%; }
#sqlpanel code { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .82rem;
  color: var(--ink); white-space: pre-wrap; word-break: break-word; }
#sqlpanel tr.warn td.qc { color: var(--accent, #ff522c); }
#sqlpanel .qn1 { display: inline-block; margin-left: .5rem; font-size: .68rem; font-weight: 700;
  color: #fff8f4; background: var(--accent, #ff522c); border-radius: 4px; padding: .02rem .32rem;
  vertical-align: middle; }
/* Checks panel: the performance budget and how this run measured against it. */
#chkpanel { display: none; position: absolute; inset: 0; overflow: auto; cursor: default;
  background: var(--bg); padding: .6rem .7rem 1.2rem; }
body.chk #stage { cursor: default; }
body.chk #svg, body.chk #flame, body.chk #sqlpanel, body.chk #srcpanel { display: none; }
/* Only the graph pans, so its whole tool section is irrelevant elsewhere. */
body.flame #mh-graph, body.sql #mh-graph, body.src #mh-graph, body.chk #mh-graph,
body.flame #fitbtn, body.sql #fitbtn, body.src #fitbtn, body.chk #fitbtn,
body.flame #critbtn, body.sql #critbtn, body.src #critbtn, body.chk #critbtn,
body.flame #prunerow, body.sql #prunerow, body.src #prunerow, body.chk #prunerow { display: none; }
body.chk #chkpanel { display: block; }
#chkpanel .khead { color: var(--muted); font-size: .78rem; padding: .1rem .1rem .6rem; }
#chkpanel .khead b { color: var(--ink); }
#chkpanel .krow { display: grid; grid-template-columns: 4.2rem 1fr auto auto; gap: .7rem;
  align-items: baseline; padding: .42rem .5rem; border-bottom: 1px solid var(--border); }
#chkpanel .kt { font-size: .68rem; font-weight: 700; letter-spacing: .05em; border-radius: 4px;
  padding: .08rem .35rem; text-align: center; color: #fff8f4; }
#chkpanel .kt.pass { background: #30a46c; }
#chkpanel .kt.fail { background: #e5484d; }
#chkpanel .kt.error { background: var(--muted); }
#chkpanel .kspec { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .82rem;
  color: var(--ink); min-width: 0; overflow-wrap: anywhere; }
#chkpanel .klabel { display: block; font-family: inherit; font-size: .76rem; color: var(--muted); }
#chkpanel .kval { font-variant-numeric: tabular-nums; font-size: .82rem; white-space: nowrap; }
#chkpanel .kval b { font-weight: 700; }
#chkpanel .kbudget { font-variant-numeric: tabular-nums; font-size: .78rem; color: var(--muted);
  white-space: nowrap; }
#chkpanel .knone { color: var(--muted); font-size: .8rem; padding: .4rem .5rem; }

/* Source panel: the PHP file annotated with per-line sampled cost. */
#srcpanel { display: none; position: absolute; inset: 0; overflow: auto; cursor: default;
  background: var(--bg); padding: .6rem 0 1.2rem; }
body.src #stage { cursor: default; }
body.src #svg, body.src #flame, body.src #sqlpanel { display: none; }
body.src #srcpanel { display: block; }
#srcpanel .shead { color: var(--muted); font-size: .76rem; padding: 0 .8rem .55rem; }
#srcpanel .shead b { color: var(--ink); }
#srcpanel .sline { display: grid; grid-template-columns: 4.6rem 3.2rem 1fr; align-items: baseline;
  gap: .5rem; padding: .02rem .8rem; font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: .8rem; line-height: 1.5; }
#srcpanel .sline.hot { font-weight: 600; }
#srcpanel .sn { color: var(--muted); text-align: right; font-variant-numeric: tabular-nums;
  font-size: .72rem; }
#srcpanel .sp { font-variant-numeric: tabular-nums; text-align: right; border-radius: 3px;
  padding: 0 .3rem; }
#srcpanel .sc { white-space: pre; overflow-x: auto; color: var(--ink); }
#srcpanel .sfn { font-family: -apple-system, system-ui, sans-serif; font-size: .7rem;
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
.side-head .t { font-size: .85rem; font-weight: 650; color: var(--ink); }
.side-head #groupbtn { display: none; margin-right: auto; background: transparent; border: 1px solid var(--border);
  border-radius: 5px; color: var(--muted); cursor: pointer; font-size: .8rem; line-height: 1; padding: .12rem .3rem; }
.side-head #groupbtn:hover { color: var(--ink); }
.side-head #groupbtn.on { background: var(--accent); color: #fff8f4; border-color: var(--accent); }
.side-head .sort { font-size: .7rem; color: var(--muted); text-transform: uppercase; letter-spacing: .05em;
  font-variant-numeric: tabular-nums; }
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
.fnrow.ingroup { margin-left: .7rem; }
#fnlist { list-style: none; margin: 0; padding: .3rem; overflow-y: auto; flex: 1; }
.fnrow { display: grid; grid-template-columns: 8px 1fr auto; gap: .55rem; align-items: center;
  padding: .4rem .5rem; border-radius: 8px; cursor: pointer; border: 1px solid transparent; }
.fnrow:hover { background: var(--hover); }
.fnrow.sel { background: var(--sel); border-color: var(--accent); }
.fnrow.hide { display: none; }
.fnrow .sw { display: block; width: 8px; height: 28px; border-radius: 3px; }
.fnrow .nm { min-width: 0; }
.fnrow .nm .n { display: block; font-size: .82rem; color: var(--ink); white-space: nowrap;
  overflow: hidden; text-overflow: ellipsis; }
.fnrow .nm .m2 { display: block; font-size: .68rem; color: var(--muted); font-variant-numeric: tabular-nums;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 1px; }
.fnrow .val { display: flex; flex-direction: column; align-items: flex-end; }
.fnrow .val .pv { display: block; font-size: .82rem; font-weight: 650; color: var(--ink); font-variant-numeric: tabular-nums; }
.fnrow .val .mini { display: block; height: 4px; width: 4rem; border-radius: 2px; background: var(--line); margin-top: 3px; overflow: hidden; }
.fnrow .val .mini > i { display: block; height: 100%; background: var(--accent); }
.qbadge { display: inline-block; font-size: .62rem; line-height: 1.4; padding: 0 .32rem; border-radius: 4px;
  margin-left: .35rem; background: rgba(255,0,112,.13); color: var(--accent); font-variant-numeric: tabular-nums;
  vertical-align: middle; }
#empty { color: var(--muted); font-size: .78rem; padding: 1rem .85rem; }
/* Bottom-up detail: callers/callees of the selected function. */
#detail { display: none; border-top: 1px solid var(--border); background: var(--panel);
  max-height: 45%; overflow-y: auto; padding: .5rem .3rem .3rem; }
#detail.show { display: block; }
#detail .dh { font-size: .72rem; color: var(--muted); text-transform: uppercase; letter-spacing: .04em;
  padding: .2rem .55rem; margin-top: .3rem; }
#detail .dsel { font-size: .82rem; font-weight: 650; color: var(--ink); padding: 0 .55rem .1rem;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
#detail .drow { display: flex; justify-content: space-between; gap: .5rem; align-items: baseline;
  padding: .28rem .55rem; border-radius: 6px; cursor: pointer; font-size: .8rem; }
#detail .drow:hover { background: var(--hover); }
#detail .drow .dn { min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--ink); }
#detail .drow .dc { color: var(--muted); font-variant-numeric: tabular-nums; flex: none; }
#detail .dnone { color: var(--muted); font-size: .74rem; padding: .1rem .55rem; }
svg { width: 100%; height: 100%; display: block; }
.edge { fill: none; stroke: var(--edge); stroke-width: 1; opacity: .5; }
.edge.hi { stroke: var(--accent); stroke-width: 2; opacity: 1; }
.elabel { fill: var(--muted); font-size: 9.5px; text-anchor: middle; pointer-events: none;
  paint-order: stroke; stroke: var(--panel); stroke-width: 3px; stroke-linejoin: round;
  font-variant-numeric: tabular-nums; }
.edge.fresh { stroke: var(--accent); opacity: .95; }
.node rect { stroke: var(--line); stroke-width: 1; rx: 6; }
.node.dim { opacity: .25; }
.node.search-dim { opacity: .12; }
.node.absent { opacity: .16; }
.node.hotter rect { stroke: var(--accent); stroke-width: 2.5; }
/* A/B diff (baseline vs current): regression red, improvement green. */
.node.grew rect { stroke: #e5484d; stroke-width: 3; }
.node.shrank rect { stroke: #30a46c; stroke-width: 3; }
.node.newfn rect { stroke: var(--accent); stroke-width: 2.5; stroke-dasharray: 5 3; }
.node.removed rect { stroke: #30a46c; stroke-width: 2; stroke-dasharray: 4 4; }
.node.removed { opacity: .4; }
.edge.gone { stroke-dasharray: 5 4; opacity: .4; }
.edge.critical { stroke: var(--accent); stroke-width: 3.5; opacity: 1; }
.node.critical rect { stroke: var(--accent); stroke-width: 3; }
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
#menu .mseg button.on { background: var(--accent); color: #fff8f4; }

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
.htool { padding: .26rem .55rem; border: 1px solid var(--border); border-radius: 6px;
  background: var(--bg); color: var(--ink); font-size: .78rem; cursor: pointer; }
.htool.on { background: var(--accent); color: #fff8f4; border-color: var(--accent); }
select.htool { padding: .2rem .35rem; }
.node text { fill: #201a17; font-size: 11px; pointer-events: none; }
.node .sub { font-size: 9.5px; opacity: .72; }
#tip { position: absolute; pointer-events: none; background: var(--panel);
  border: 1px solid var(--border); border-radius: 8px; padding: .55rem .7rem;
  font-size: .82rem; box-shadow: 0 6px 24px rgba(0,0,0,.18); max-width: 22rem; display: none; }
#tip h3 { margin: 0 0 .3rem; font-size: .85rem; }
#tip .row { display: flex; justify-content: space-between; gap: 1rem; color: var(--muted); }
#tip .bar { height: 6px; border-radius: 3px; background: var(--accent); margin-top: 2px; }
#tip .cause { margin-top: .35rem; }
.legend { position: absolute; top: 3.4rem; right: .8rem; font-size: .74rem;
  color: var(--muted); background: var(--panel); border: 1px solid var(--border);
  border-radius: 6px; padding: .4rem .6rem; max-width: 20rem; }
#nav { position: absolute; left: 0; right: 0; bottom: 0; display: none; gap: .55rem;
  align-items: center; padding: .45rem .8rem; background: var(--panel); border-top: 1px solid var(--border); }
#nav.show { display: flex; }
#nav button { border: 1px solid var(--border); background: var(--bg); color: var(--ink);
  border-radius: 6px; cursor: pointer; padding: .25rem .5rem; font-size: .8rem; line-height: 1; }
#nav button:hover { border-color: var(--accent); }
#nav .pills { display: flex; gap: 3px; flex: 1; align-items: center; }
#nav .pill { flex: 1; min-width: 4px; max-width: 44px; height: 22px; border-radius: 3px;
  cursor: pointer; opacity: .5; border: 2px solid transparent; }
#nav .pill:hover { opacity: .85; }
#nav .pill.active { opacity: 1; border-color: var(--ink); }
#nav .tog { font-size: .78rem; color: var(--muted); display: inline-flex; align-items: center;
  gap: .3rem; cursor: pointer; user-select: none; }
#nav .live { display: inline-flex; align-items: center; gap: .35rem; }
#nav .live .dot { width: 8px; height: 8px; border-radius: 50%; background: #b8a99f; }
#nav .live.on .dot { background: #2ec26a; box-shadow: 0 0 0 3px rgba(46,194,106,.22); }
#nav .label { font-size: .76rem; color: var(--muted); font-variant-numeric: tabular-nums; white-space: nowrap; }
@media (prefers-reduced-motion: no-preference) {
  .node.hotter rect { animation: hotpulse 1.2s ease-in-out infinite; }
}
@keyframes hotpulse { 0%, 100% { stroke-opacity: 1; } 50% { stroke-opacity: .4; } }
</style>
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
  <button id="menubtn" class="htool" data-tip="Graph tools, appearance and shortcuts">☰</button>
</header>
<aside id="sidebar">
  <div class="side-head"><span class="t">Functions</span><button id="groupbtn" data-tip="Group the list under its classes and namespaces">⊞</button><span class="sort" id="sortlabel"></span></div>
  <ul id="fnlist"></ul>
  <div id="detail"></div>
</aside>
<div id="dragger" title="Drag to resize the list"></div>
<div id="stage"><svg id="svg"><g id="viewport"></g></svg><div id="flame"></div><div id="sqlpanel"></div><div id="srcpanel"></div><div id="chkpanel"></div></div>
<div id="tip"></div>
<div id="htip" hidden></div>
<div id="menu" hidden>
  <button id="resetbtn" class="mrow mfirst"><span class="mi">↺</span><span class="ml">Start over<em>Call graph, time, nothing selected or filtered</em></span><kbd>r</kbd></button>
  <div class="mh" id="mh-graph">Graph</div>
  <button id="fitbtn" class="mrow"><span class="mi">⤢</span><span class="ml">Fit in view<em>Frame it again if zoom or pan lost it</em></span><kbd>0</kbd></button>
  <button id="critbtn" class="mrow"><span class="mi">⚡</span><span class="ml">Critical path<em>The heaviest chain, entry point down</em></span><kbd>p</kbd></button>
  <label class="mrow" id="prunerow"><span class="mi">⌀</span><span class="ml">Hide the small<em>Drop functions under a share of the run</em></span>
    <select id="prune">
      <option value="0">all</option>
      <option value="1">≥1%</option>
      <option value="5">≥5%</option>
      <option value="10">≥10%</option>
    </select>
  </label>
  <div class="mh">Appearance</div>
  <div class="mrow mstatic"><span class="mi">◐</span><span class="ml">Theme</span>
    <span class="mseg" id="themeseg"></span>
  </div>
  <div class="mh">Help</div>
  <button id="helpbtn" class="mrow"><span class="mi">?</span><span class="ml">Keyboard shortcuts</span><kbd>?</kbd></button>
</div>
<div id="help" hidden><div class="card">
  <div class="hh">Keyboard</div>
  <dl id="helplist"></dl>
  <div class="hf">Esc or ? to close</div>
</div></div>
<div class="legend" id="legend">Sampled — cost is a share of samples, not instrumented time. Hotter = more self time.</div>
<div id="nav">
  <button id="prev" title="Previous frame (←)">◀</button>
  <button id="next" title="Next frame (→)">▶</button>
  <div class="pills" id="pills"></div>
  <label class="tog"><input type="checkbox" id="diff"> Δ vs previous</label>
  <button class="live" id="live" title="Follow latest / pause (l)"><span class="dot"></span><span id="livetext">live</span></button>
  <span class="label" id="navlabel"></span>
</div>
<script>
const DATA = __DATA_JSON__;
(function() {
  const FRAMES = DATA.frames || [];
  const metaEl = document.getElementById('meta');
  if (!FRAMES.length || !FRAMES.some(f => f.nodes.length)) { metaEl.textContent = 'no samples'; return; }
  if (DATA.exact) {
    const legend = document.getElementById('legend');
    if (legend) legend.textContent = 'Exact (--instrument) — pick a dimension above (or press m to cycle). Hotter = more of it.';
  }

  const NS = 'http://www.w3.org/2000/svg';
  const W = 190, H = 46, GAPX = 34, GAPY = 78;
  const vp = document.getElementById('viewport'), tip = document.getElementById('tip');
  const nav = document.getElementById('nav'), pills = document.getElementById('pills');
  const navlabel = document.getElementById('navlabel'), liveBtn = document.getElementById('live');
  const diffBox = document.getElementById('diff'), metricbar = document.getElementById('metricbar');
  const stage = document.getElementById('stage'), svg = document.getElementById('svg');
  const fnlist = document.getElementById('fnlist'), sortlabel = document.getElementById('sortlabel');
  const searchInput = document.getElementById('search'), dragger = document.getElementById('dragger');
  const critbtn = document.getElementById('critbtn'), pruneSel = document.getElementById('prune');
  const detail = document.getElementById('detail');
  const flame = document.getElementById('flame'), flamebtn = document.getElementById('flamebtn');
  const sqlpanel = document.getElementById('sqlpanel'), sqlbtn = document.getElementById('sqlbtn');
  const groupbtn = document.getElementById('groupbtn');
  const srcpanel = document.getElementById('srcpanel'), srcbtn = document.getElementById('srcbtn');
  const chkpanel = document.getElementById('chkpanel'), chkbtn = document.getElementById('chkbtn');

  // Rebuildable graph state: recomputed when a live update grows the union.
  let NAMES, nameId, uEdge, EDGES, xy, nodeEls, edgeEls, frameNode, frameEdge;
  let cur = FRAMES.length - 1, diffOn = false, follow = DATA.live, reloadTimer = null, curNodes = null;
  let selected = -1, sbw = 0, critOn = false, pruneT = 0;
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
  const hasMem = !!DATA.exact && FRAMES.some(f => f.totalAllocs > 0);
  const hasIo = !!DATA.exact && FRAMES.some(f => f.nodes.some(n => (n.ioInclN || 0) > 0));
  const hasRet = !!DATA.exact && FRAMES.some(f => f.nodes.some(n => (n.retInclN || 0) !== 0));
  const hasWait = !!DATA.exact && FRAMES.some(f => f.nodes.some(n => (n.waitInclN || 0) > 0));
  const hasNetwork = !!DATA.exact && FRAMES.some(f => f.nodes.some(n => (n.networkInclN || 0) > 0));
  const hasNetworkWait = !!DATA.exact && FRAMES.some(f => f.nodes.some(n => (n.networkWaitInclN || 0) > 0));
  // Selectable cost dimensions (Blackfire-style). Availability depends on data.
  const METRICS = [
    { key: 'time',  label: 'Time',     icon: '⏱', on: !!DATA.exact },
    { key: 'mem',   label: 'Memory',   icon: '🧠', on: hasMem },
    { key: 'ret',   label: 'Retained', icon: '💧', on: hasRet },
    { key: 'wait',  label: 'Wait',     icon: '⏳', on: hasWait },
    { key: 'io',    label: 'SQL',      icon: '🗄', on: hasIo },
    { key: 'network', label: 'Network', icon: '↗', on: hasNetwork },
    { key: 'networkWait', label: 'Net wait', icon: '🌐', on: hasNetworkWait },
    { key: 'calls', label: 'Calls',    icon: '#',  on: !!DATA.exact },
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
  function diffInk(d) { const c = diffRgb(d); return (0.299*c[0] + 0.587*c[1] + 0.114*c[2]) < 150 ? '#fff8f4' : '#201a17'; }
  function ink(excl) { const c = heatRgb(excl); return (0.299*c[0] + 0.587*c[1] + 0.114*c[2]) < 150 ? '#fff8f4' : '#201a17'; }
  function clip(s, n) { return s.length > n ? s.slice(0, n-1) + '…' : s; }
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
      const r = document.createElementNS(NS, 'rect');
      r.setAttribute('width', W); r.setAttribute('height', H); r.setAttribute('rx', 6);
      g.appendChild(r);
      const t1 = document.createElementNS(NS, 'text');
      t1.setAttribute('x', 8); t1.setAttribute('y', 18); t1.setAttribute('class', 'nm');
      t1.textContent = clip(name, 26); g.appendChild(t1);
      const t2 = document.createElementNS(NS, 'text');
      t2.setAttribute('x', 8); t2.setAttribute('y', 34); t2.setAttribute('class', 'sub');
      g.appendChild(t2);
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
    nodeEls.forEach((g, i) => g.classList.toggle('dim', !near.has(i)));
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
    NAMES.forEach((name, v) => {
      const g = nodeEls[v], n = fN.get(name);
      const rect = g.querySelector('rect'), nm = g.querySelector('.nm'), sub = g.querySelector('.sub');
      if (!n) {
        g.classList.add('absent'); g.classList.remove('hotter', 'grew', 'shrank', 'newfn');
        // In diff mode, a node present in the baseline but gone now is "removed".
        g.classList.toggle('removed', !!(prev && prev.get(name)));
        // ink(0), not a literal: the cold end of the ramp is pale on a light
        // ground and dark on a dark one, so the label has to follow it.
        rect.setAttribute('fill', heat(0)); nm.setAttribute('fill', ink(0)); sub.setAttribute('fill', ink(0)); sub.textContent = ''; return;
      }
      g.classList.remove('absent', 'removed');
      const share = nodeShare(n);
      let fill = heat(share), tone = ink(share);
      g.classList.remove('hotter', 'grew', 'shrank', 'newfn');
      if (prev) {
        const p = prev.get(name);
        if (!p) { g.classList.add('newfn'); }             // appeared since the baseline
        else {
          const d = share - nodeShare(p);
          if (d > 1.5) { g.classList.add('grew'); fill = diffFill(d); tone = diffInk(d); }
          else if (d < -1.5) { g.classList.add('shrank'); fill = diffFill(d); tone = diffInk(d); }
        }
      }
      rect.setAttribute('fill', fill); nm.setAttribute('fill', tone); sub.setAttribute('fill', tone);
      sub.textContent = nodeSub(n);
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
    sortlabel.textContent = 'by ' + (mdef ? mdef.label.toLowerCase() : 'self time');
    const rows = [];
    NAMES.forEach((name, v) => { const n = fN.get(name); if (n) rows.push({ name, v, n }); });
    rows.sort((a, b) => nodeShare(b.n) - nodeShare(a.n));
    fnlist.textContent = '';
    if (!rows.length) { const d = document.createElement('li'); d.id = 'empty'; d.textContent = 'no functions'; fnlist.appendChild(d); buildDetail(); return; }
    const makeRow = r => {
      const n = r.n, share = nodeShare(n);
      const li = document.createElement('li');
      li.className = 'fnrow' + (r.v === selected ? ' sel' : '') + (q && !r.name.toLowerCase().includes(q) ? ' hide' : '');
      const val = metricValue(n);
      let sub = metricSub(n);
      if (n.calls != null && metric !== 'calls') sub += ' · ' + n.calls + (n.calls === 1 ? ' call' : ' calls');
      const qb = (n.ioExclN > 0 && metric !== 'io') ? '<span class="qbadge">' + n.ioExclN + ' q</span>' : '';
      li.innerHTML =
        '<span class="sw" style="background:' + heat(share) + '"></span>' +
        '<span class="nm"><span class="n">' + esc(r.name) + qb + '</span><span class="m2">' + sub + '</span></span>' +
        '<span class="val"><span class="pv">' + val + '</span><span class="mini"><i style="width:' + Math.min(100, share * 3) + '%"></i></span></span>';
      li.addEventListener('click', () => selectNode(r.v, true));
      return li;
    };
    if (grouped) {
      // Cluster by class (before `::`) or namespace (before the last `\`); the
      // rest stay under "· global". Groups rank by aggregate share; rows within
      // keep the metric order. Headers collapse the group.
      const groups = new Map();
      rows.forEach(r => { const k = groupKey(r.name); let g = groups.get(k);
        if (!g) { g = { key: k, rows: [], agg: 0 }; groups.set(k, g); } g.rows.push(r); g.agg += nodeShare(r.n); });
      [...groups.values()].sort((a, b) => b.agg - a.agg).forEach(g => {
        const isColl = collapsed.has(g.key);
        const anyMatch = !q || g.rows.some(r => r.name.toLowerCase().includes(q));
        const h = document.createElement('li');
        h.className = 'grouphdr' + (isColl ? ' coll' : '') + (anyMatch ? '' : ' hide');
        h.innerHTML = '<span class="gtw">▾</span><span class="gname">' + esc(g.key || '· global') +
          '</span><span class="gcount">' + g.rows.length + '</span>';
        h.addEventListener('click', () => { if (collapsed.has(g.key)) collapsed.delete(g.key); else collapsed.add(g.key); buildList(); });
        fnlist.appendChild(h);
        if (!isColl) g.rows.forEach(r => { const li = makeRow(r); li.classList.add('ingroup'); fnlist.appendChild(li); });
      });
    } else {
      rows.forEach(r => fnlist.appendChild(makeRow(r)));
    }
    buildDetail();
  }
  // Class/namespace a function belongs to, for grouping ('' = global scope).
  function groupKey(name) {
    const ci = name.lastIndexOf('::');
    if (ci > 0) return name.slice(0, ci);
    const ni = name.lastIndexOf('\\');
    if (ni > 0) return name.slice(0, ni);
    return '';
  }

  // --- bottom-up detail: exact callers/callees of the selected function ---
  // `pct` on each edge is the callee's inclusive time under that caller (as a
  // % of the whole run) — a genuine per-edge attribution, shown beside the
  // exact call count. Clicking a row walks the graph to that neighbour.
  function buildDetail() {
    if (selected < 0) { detail.className = ''; detail.textContent = ''; return; }
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
    const rowHtml = r => {
      const bits = [];
      if (r.c != null) bits.push('×' + fmtK(r.c));
      if (DATA.exact && r.p != null) bits.push(r.p.toFixed(1) + '%');
      return '<div class="drow" data-v="' + r.v + '"><span class="dn">' + esc(NAMES[r.v]) +
        '</span><span class="dc">' + bits.join(' · ') + '</span></div>';
    };
    let html = '<div class="dsel" title="' + esc(NAMES[selected]) + '">' + esc(NAMES[selected]) + '</div>';
    const section = (title, list) => {
      html += '<div class="dh">' + title + '</div>';
      html += list.length ? list.map(rowHtml).join('') : '<div class="dnone">— none —</div>';
    };
    section('Callers · ' + callers.length, callers);
    section('Callees · ' + callees.length, callees);
    detail.innerHTML = html;
    detail.className = 'show';
    detail.querySelectorAll('.drow').forEach(row =>
      row.addEventListener('click', () => selectNode(+row.dataset.v, true)));
  }

  // --- flame graph (icicle) built from the exact inclusive-time tree ---
  // Each edge's `pct` is the callee's inclusive time under that caller (a % of
  // the whole run), so a child cell's width is exactly that share of its
  // parent's width. Recursion is drawn once (we don't descend a cycle).
  function buildFlame() {
    if (!flameOn) return;
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
    let html = '<div class="frow">Flame · time · root: <b>' + rootLabel + '</b>' +
      (flameRoot >= 0 ? ' · <a href="#" id="freset">⌂ reset</a>' : '') +
      ' · click a frame to zoom, Esc to reset</div><div class="fcanvas" style="height:' + H + 'px">';
    cells.forEach(c => {
      const label = c.w * W > 34 ? esc(c.name) + '  ' + c.incl.toFixed(1) + '%' : '';
      html += '<div class="fcell' + (c.v === selected ? ' sel' : '') + '" data-v="' + c.v +
        '" title="' + esc(c.name) + ' — incl ' + c.incl.toFixed(1) + '% · self ' + c.excl.toFixed(1) + '%"' +
        ' style="left:' + (c.x * 100).toFixed(4) + '%;width:' + (c.w * 100).toFixed(4) + '%;top:' +
        (c.depth * ROWH) + 'px;background:' + heat(c.excl) + ';color:' + ink(c.excl) + '">' + label + '</div>';
    });
    flame.innerHTML = html + '</div>';
    flame.querySelectorAll('.fcell').forEach(el => el.addEventListener('click', () => {
      const v = +el.dataset.v; selected = v; flameRoot = v; highlight(v); buildList(); buildFlame(); save();
    }));
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
  function buildSql() {
    if (!sqlOn) return;
    const qs = (FRAMES[cur].queries || []).slice().sort((a, b) => b.count - a.count);
    const total = qs.reduce((s, q) => s + q.count, 0);
    const max = qs.length ? qs[0].count : 1;
    let html = '<div class="qhead"><b>' + qs.length + '</b> distinct ' +
      (qs.length === 1 ? 'statement' : 'statements') + ' · <b>' + total + '</b> executions</div>';
    if (!qs.length) { sqlpanel.innerHTML = html + '<div class="qhead">no DB queries recorded</div>'; return; }
    html += '<table><thead><tr><th>Runs</th><th></th><th>Statement</th></tr></thead><tbody>';
    qs.forEach(q => {
      const share = q.count / max;
      // Flag a likely N+1: the same statement executed many times.
      const warn = q.count >= 20 ? ' class="warn"' : '';
      const badge = q.count >= 20 ? '<span class="qn1">N+1?</span>' : '';
      html += '<tr' + warn + '><td class="qc">×' + q.count + '</td>' +
        '<td class="qbar"><i style="width:' + Math.max(4, share * 100) + '%;background:' + heat(share * 33) + '"></i></td>' +
        '<td class="qsql"><code>' + esc(q.sql) + '</code>' + badge + '</td></tr>';
    });
    sqlpanel.innerHTML = html + '</tbody></table>';
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
    let html = '<div class="shead"><b title="' + escAttr(LINES.file) + '">' + esc(base) +
      '</b> &middot; ' + summary + '</div>';
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
    srcpanel.innerHTML = html;
  }
  // --- checks panel: the performance budget, and how this run measured up.
  // Failures first — a gate is read when it is red, and the reason should not
  // sit below a screen of passing rows.
  function buildChecks() {
    if (!chkOn) return;
    const rank = { fail: 0, error: 1, pass: 2 };
    const rows = ASSERTS.slice().sort((a, b) => rank[a.status] - rank[b.status]);
    const n = s => ASSERTS.filter(a => a.status === s).length;
    let html = '<div class="khead"><b>' + n('pass') + '</b> passed &middot; <b>' + n('fail') +
      '</b> failed' + (n('error') ? ' &middot; <b>' + n('error') + '</b> not evaluated' : '') + '</div>';
    if (!rows.length) { chkpanel.innerHTML = html + '<div class="knone">no assertions for this run</div>'; return; }
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
    chkpanel.innerHTML = html;
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
    nodeEls.forEach(g => g.classList.remove('dim'));
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
  // The graph-only tools are meaningless without exact per-edge weights.
  if (!DATA.exact) { critbtn.classList.add('off'); document.getElementById('prunerow').classList.add('off'); }
  if (hasQueries) sqlbtn.style.display = '';
  if (LINES && LINES.source && LINES.source.length) srcbtn.style.display = '';
  // Offer grouping only when names actually carry a class or namespace.
  // (Explicit value: the default `display:none` lives in a CSS rule, so ''
  // would fall back to it rather than reveal the button.)
  if (hasGroups) groupbtn.style.display = 'inline-block';

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
      b.innerHTML = '<span class="g">' + m.icon + '</span><span>' + m.label + '</span>';
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
    { key: 'graph', label: 'Graph',   icon: '🕸', on: () => true,
      hint: 'Who calls whom, one box per function' },
    { key: 'flame', label: 'Flame',   icon: '🔥', on: () => !!DATA.exact,
      hint: 'The same tree as nested bars, width = time. Click to zoom (f)' },
    { key: 'sql',   label: 'Queries', icon: '🗄', on: () => hasQueries,
      hint: 'Every distinct DB statement and how many times it ran (q)' },
    { key: 'src',   label: 'Source',  icon: '📄', on: () => !!LINES,
      hint: 'Your PHP file with the cost of each line (s)' },
    { key: 'chk',   label: 'Checks',  icon: '✅', on: () => ASSERTS.length > 0,
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
      b.innerHTML = '<span class="g">' + v.icon + '</span><span>' + v.label + '</span>';
      b.dataset.tip = v.hint;
      b.addEventListener('click', () => setView(v.key));
      viewbar.appendChild(b);
      viewEls[v.key] = b;
    });
    syncViewBar();
  }
  buildMetricBar();
  buildViewBar();

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
    if (typeof cur === 'number' && frameNode && frameNode[cur]) paint(cur);
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
    syncMenu();
  }
  // Toggles keep their state visible here, since the header no longer shows it.
  function syncMenu() {
    document.getElementById('critbtn').classList.toggle('on', critOn);
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
  const STORE_KEY = 'elephc-callgraph:' + (DATA.title || '');
  // Theme is a reader preference, not a property of one capture, so it is
  // stored per site rather than per page like the rest of the view state.
  const THEME_KEY = 'elephc-callgraph:theme';
  function save() { try { localStorage.setItem(STORE_KEY, JSON.stringify({follow: follow, diff: diffOn, metric: metric, sbw: sbw, crit: critOn, prune: pruneT, grouped: grouped, selTs: FRAMES[cur] ? FRAMES[cur].ts : 0, tx: tx, ty: ty, scale: scale})); } catch (e) {} writeHash(); }
  function load() { try { return JSON.parse(localStorage.getItem(STORE_KEY)) || {}; } catch (e) { return {}; } }

  // --- shareable URL: the discrete view (dimension, selection, active view,
  // prune, critical path) rides in location.hash so a copied link reopens it.
  // Pan/zoom stay out — those are per-viewer, and localStorage already keeps them.
  function writeHash() {
    try {
      const p = [];
      if (metric !== 'time') p.push('m=' + metric);
      if (selected >= 0 && NAMES[selected]) p.push('s=' + encodeURIComponent(NAMES[selected]));
      if (flameOn) { p.push('v=flame'); if (flameRoot >= 0 && NAMES[flameRoot]) p.push('fr=' + encodeURIComponent(NAMES[flameRoot])); }
      else if (sqlOn) p.push('v=sql');
      else if (srcOn) p.push('v=src');
      else if (chkOn) p.push('v=chk');
      if (pruneT) p.push('pr=' + pruneT);
      if (critOn) p.push('c=1');
      const h = p.length ? '#' + p.join('&') : location.pathname + location.search;
      if (('#' + p.join('&')) !== location.hash) history.replaceState(null, '', h);
    } catch (e) {}
  }
  function readHash() {
    const out = {};
    (location.hash || '').replace(/^#/, '').split('&').forEach(kv => {
      if (!kv) return; const i = kv.indexOf('='); const k = i < 0 ? kv : kv.slice(0, i);
      out[k] = i < 0 ? '' : decodeURIComponent(kv.slice(i + 1));
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
</body>
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
