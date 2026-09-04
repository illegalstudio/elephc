//! Purpose:
//! The exact per-function capture: parsing an instrumentation dump into a call
//! graph, flattening it into stacks for the exporters, and evaluating the
//! project's performance budget against it.
//!
//! Called from:
//! - `local::run` for a program monitor launched.
//! - `remote::run` when `--exact` asked a service for one request's slice.
//!
//! Key details:
//! - `elephc-instr: note:` lines carry no metrics and are skipped by the parser;
//!   the reason they carry is surfaced by the caller.
//! - Self times partition the root's inclusive, in every dimension.

use super::*;

/// Surfaces a few Blackfire-style hints from the exact profile: the time
/// hotspot, the allocation hotspot, and functions whose per-call cost suggests
/// call overhead. Silent when nothing crosses a threshold.
pub(crate) fn instrument_recommendations(graph: &crate::call_graph::CallGraph) -> String {
    let root_ns = graph.nodes.iter().map(|n| n.inclusive).max().unwrap_or(1).max(1);
    let total_allocs: u64 = graph.nodes.iter().map(|n| n.alloc_exclusive).sum();
    let mut hints: Vec<String> = Vec::new();
    if let Some(hot) = graph.nodes.iter().max_by_key(|n| n.exclusive) {
        let pct = 100.0 * hot.exclusive as f64 / root_ns as f64;
        if pct >= 20.0 {
            hints.push(format!(
                "• {} is the hotspot — {:.0}% self time ({})",
                hot.name,
                pct,
                fmt_ns(hot.exclusive)
            ));
        }
    }
    if total_allocs > 0 {
        if let Some(am) = graph.nodes.iter().max_by_key(|n| n.alloc_exclusive) {
            let pct = 100.0 * am.alloc_exclusive as f64 / total_allocs as f64;
            if am.alloc_exclusive > 10_000 && pct >= 25.0 {
                hints.push(format!(
                    "• {} allocates the most — {} allocations ({:.0}% of total); cutting allocation here pays off",
                    am.name, am.alloc_exclusive, pct
                ));
            }
        }
    }
    for n in &graph.nodes {
        if let Some(calls) = n.call_count {
            if calls >= 100_000 && n.exclusive > 0 {
                let per = n.exclusive / calls;
                if per < 100 {
                    hints.push(format!(
                        "• {} is called {} times at ~{}ns each — call overhead may dominate; consider hoisting the work or inlining",
                        n.name, calls, per
                    ));
                }
            }
        }
    }
    // Retention hotspot: which function leaves the most objects on the heap.
    // Reported against what it allocated, since retaining most of a large
    // allocation is the signal — a function that allocates and frees is fine.
    if let Some(r) = graph.nodes.iter().max_by_key(|n| n.retained_exclusive) {
        if r.retained_exclusive > 0 && r.alloc_exclusive > 0 {
            let kept = 100.0 * r.retained_exclusive as f64 / r.alloc_exclusive as f64;
            if kept >= 50.0 {
                hints.push(format!(
                    "• {} retains the most — {} of its {} allocations ({:.0}%) are still live when it returns; check for a cache or collection that only grows",
                    r.name, r.retained_exclusive, r.alloc_exclusive, kept
                ));
            }
        }
    }
    // I/O-bound verdict: when most of the run is spent blocked in the driver,
    // optimizing PHP-side work is the wrong lever — say so.
    let root_ns = graph.nodes.iter().map(|n| n.inclusive).max().unwrap_or(0);
    let total_wait: u64 = graph.nodes.iter().map(|n| n.wait_exclusive).sum();
    if root_ns > 0 && total_wait > 0 {
        let pct = 100.0 * total_wait as f64 / root_ns as f64;
        if pct >= 25.0 {
            let worst = graph
                .nodes
                .iter()
                .max_by_key(|n| n.wait_exclusive)
                .filter(|n| n.wait_exclusive > 0);
            let who = worst.map_or(String::new(), |n| {
                format!(" — {} blocks longest ({})", n.name, fmt_ns(n.wait_exclusive))
            });
            hints.push(format!(
                "• the run is I/O-bound: {:.0}% of it ({}) is spent waiting on the database{who}; batching or caching queries will beat any PHP-side tuning",
                pct,
                fmt_ns(total_wait)
            ));
        }
    }
    let total_network_wait: u64 = graph
        .nodes
        .iter()
        .map(|n| n.network_wait_exclusive)
        .sum();
    if root_ns > 0 && total_network_wait > 0 {
        let pct = 100.0 * total_network_wait as f64 / root_ns as f64;
        if pct >= 25.0 {
            let worst = graph
                .nodes
                .iter()
                .max_by_key(|node| node.network_wait_exclusive)
                .filter(|node| node.network_wait_exclusive > 0);
            let who = worst.map_or(String::new(), |node| {
                format!(
                    " - {} blocks longest ({})",
                    node.name,
                    fmt_ns(node.network_wait_exclusive)
                )
            });
            hints.push(format!(
                "• the run is network-bound: {:.0}% of it ({}) is outgoing network wait{who}",
                pct,
                fmt_ns(total_network_wait)
            ));
        }
    }
    // Query hotspot: which function issues the most DB queries.
    let total_io: u64 = graph.nodes.iter().map(|n| n.io_exclusive).sum();
    if total_io > 0 {
        if let Some(q) = graph.nodes.iter().max_by_key(|n| n.io_exclusive) {
            if q.io_exclusive > 0 {
                hints.push(format!(
                    "• {} issues the most DB queries — {} of {} total",
                    q.name, q.io_exclusive, total_io
                ));
            }
        }
    }
    // High fan-out edges: a callee invoked many times from one caller. With exact
    // query counts we can tell a definite N+1 from a mere hot helper.
    let io_by_name: std::collections::HashMap<&str, u64> = graph
        .nodes
        .iter()
        .map(|n| (n.name.as_str(), n.io_exclusive))
        .collect();
    let mut fanout: Vec<(&str, &str, u64)> = graph
        .edges
        .iter()
        .filter_map(|e| {
            e.count.filter(|&c| c >= 100).map(|c| {
                (
                    graph.nodes[e.from].name.as_str(),
                    graph.nodes[e.to].name.as_str(),
                    c,
                )
            })
        })
        .collect();
    fanout.sort_by(|a, b| b.2.cmp(&a.2));
    for (from, to, count) in fanout.into_iter().take(3) {
        let callee_io = io_by_name.get(to).copied().unwrap_or(0);
        if callee_io > 0 {
            hints.push(format!(
                "• N+1: {from} calls {to} {count} times and {to} issues {callee_io} DB queries — batch them into one query",
            ));
        } else {
            hints.push(format!(
                "• {from} calls {to} {count} times — if {to} touches the DB, network, or filesystem, that is an N+1; batch or cache it",
            ));
        }
    }
    if hints.is_empty() {
        return String::new();
    }
    let mut out = String::from("\nrecommendations:\n");
    for hint in hints {
        out.push_str(&hint);
        out.push('\n');
    }
    out
}

/// Evaluates `<metric>:<fn><op><value>` assertions against the exact profile.
/// Metrics: `calls`, `allocs` (self), `self_ms` (self time), `time_pct`
/// (inclusive share). Returns the report and whether every assertion passed.
pub(crate) fn evaluate_asserts(
    graph: &crate::call_graph::CallGraph,
    asserts: &[(String, Option<String>)],
) -> Vec<crate::call_graph::AssertOutcome> {
    use crate::call_graph::{AssertOutcome, AssertStatus};
    let root_ns = graph.nodes.iter().map(|n| n.inclusive).max().unwrap_or(1).max(1);
    let mut by_name: HashMap<&str, &crate::call_graph::GraphNode> = HashMap::new();
    for node in &graph.nodes {
        by_name.insert(node.name.as_str(), node);
    }
    let mut outcomes = Vec::new();
    for (spec, label) in asserts {
        let Some((metric, target, op, budget)) = parse_assert(spec) else {
            outcomes.push(AssertOutcome {
                spec: spec.clone(),
                label: label.clone(),
                metric: String::new(),
                target: String::new(),
                op: String::new(),
                budget: 0.0,
                actual: None,
                status: AssertStatus::Error,
                note: Some("cannot parse — expected <metric>:<function><op><value>".to_string()),
            });
            continue;
        };
        // `*` asserts on the whole run rather than one function.
        let actual = if target == "*" {
            assert_run_total(&metric, graph, root_ns)
        } else {
            by_name
                .get(target.as_str())
                .and_then(|node| assert_metric_value(&metric, node, root_ns))
        };
        let mut note = None;
        let status = match actual {
            Some(actual) => {
                let ok = match op.as_str() {
                    "<=" => actual <= budget,
                    ">=" => actual >= budget,
                    "==" => (actual - budget).abs() < 1e-9,
                    "<" => actual < budget,
                    ">" => actual > budget,
                    _ => false,
                };
                if ok { AssertStatus::Pass } else { AssertStatus::Fail }
            }
            None => {
                // Separate the two reasons: an unknown metric is a typo in the
                // budget, a missing function is a fact about the run.
                note = Some(if assert_metric_value(&metric, graph.nodes.first().unwrap_or(&EMPTY_NODE), root_ns).is_none()
                    && assert_run_total(&metric, graph, root_ns).is_none()
                {
                    format!(
                        "unknown metric '{metric}' — known: {}",
                        ASSERT_METRICS.iter().map(|(m, _)| *m).collect::<Vec<_>>().join(", ")
                    )
                } else {
                    format!("'{target}' never ran")
                });
                AssertStatus::Error
            }
        };
        outcomes.push(AssertOutcome {
            spec: spec.clone(),
            label: label.clone(),
            metric,
            target,
            op,
            budget,
            actual,
            status,
            note,
        });
    }
    outcomes
}

/// Formats a measured value without trailing zeros, so a count reads as `250`
/// rather than `250.000` while a millisecond figure keeps its precision.
pub(crate) fn trim_number(value: f64) -> String {
    if (value - value.round()).abs() < 1e-9 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.3}")
    }
}

/// Parses `--instrument` stderr lines into an exact call graph: `inclusive`/
/// `exclusive` carry nanoseconds, `call_count` the exact invocation count, and
/// edge weights the callee's inclusive ns under that caller.
pub(crate) fn parse_instrument_dump(text: &str) -> crate::call_graph::CallGraph {
    use crate::call_graph::{CallGraph, GraphEdge, GraphNode};
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut raw_edges: Vec<(String, String, u64, u64)> = Vec::new();
    let mut queries: Vec<(String, u64)> = Vec::new();
    let mut trace: Option<crate::call_graph::TraceContext> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("elephc-instr-trace: ") {
            // "trace=<id> span=<id> parent=<id|-> start=<unix micros>"
            // `start` is absent in captures taken before slices were timestamped.
            let field = |key: &str| -> String {
                rest.split_whitespace()
                    .find_map(|kv| kv.strip_prefix(key))
                    .unwrap_or("")
                    .to_string()
            };
            let parent = field("parent=");
            trace = Some(crate::call_graph::TraceContext {
                trace_id: field("trace="),
                span_id: field("span="),
                parent_span_id: if parent == "-" { String::new() } else { parent },
                start_us: field("start=").parse::<u64>().ok(),
                route: match field("route=").as_str() {
                    "" | "-" => String::new(),
                    encoded => decode_field(encoded),
                },
            });
        } else if let Some(rest) = line.strip_prefix("elephc-instr-query: ") {
            // "<count> <sql text on one line>"
            if let Some((count, sql)) = rest.split_once(' ') {
                if let Ok(count) = count.parse::<u64>() {
                    queries.push((sql.to_string(), count));
                }
            }
        } else if let Some(rest) = line.strip_prefix("elephc-instr-edge: ") {
            // "<caller> -> <callee> count=N ns=Y"
            let Some((endpoints, metrics)) = rest.split_once(" count=") else {
                continue;
            };
            let Some((caller, callee)) = endpoints.split_once(" -> ") else {
                continue;
            };
            let count = metrics
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            let ns = instr_field(metrics, "ns=");
            raw_edges.push((caller.to_string(), callee.to_string(), count, ns));
        } else if let Some(rest) = line.strip_prefix("elephc-instr: ") {
            // "<name> calls=N incl_ns=X excl_ns=Y" — the "note:" line has no calls= and is skipped.
            let Some((name, metrics)) = rest.split_once(" calls=") else {
                continue;
            };
            let calls = metrics
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0);
            let inclusive = instr_field(metrics, "incl_ns=");
            let exclusive = instr_field(metrics, "excl_ns=");
            let alloc_inclusive = instr_field(metrics, "incl_allocs=");
            let alloc_exclusive = instr_field(metrics, "excl_allocs=");
            let io_inclusive = instr_field(metrics, "incl_io=");
            let io_exclusive = instr_field(metrics, "excl_io=");
            let retained_inclusive = instr_field_i64(metrics, "incl_ret=");
            let retained_exclusive = instr_field_i64(metrics, "excl_ret=");
            let wait_inclusive = instr_field(metrics, "incl_wait=");
            let wait_exclusive = instr_field(metrics, "excl_wait=");
            let legacy_network = instr_field(metrics, "network_ops=");
            let legacy_network_wait = instr_field(metrics, "network_wait=");
            let mut network_inclusive = instr_field(metrics, "incl_network=");
            let mut network_exclusive = instr_field(metrics, "excl_network=");
            let mut network_wait_inclusive = instr_field(metrics, "incl_network_wait=");
            let mut network_wait_exclusive = instr_field(metrics, "excl_network_wait=");
            if network_inclusive == 0 && network_exclusive == 0 && legacy_network > 0 {
                network_inclusive = legacy_network;
                network_exclusive = legacy_network;
            }
            if network_wait_inclusive == 0
                && network_wait_exclusive == 0
                && legacy_network_wait > 0
            {
                network_wait_inclusive = legacy_network_wait;
                network_wait_exclusive = legacy_network_wait;
            }
            index.insert(name.to_string(), nodes.len());
            nodes.push(GraphNode {
                name: name.to_string(),
                inclusive,
                exclusive,
                call_count: Some(calls),
                alloc_inclusive,
                alloc_exclusive,
                io_inclusive,
                io_exclusive,
                retained_inclusive,
                retained_exclusive,
                wait_inclusive,
                wait_exclusive,
                network_inclusive,
                network_exclusive,
                network_wait_inclusive,
                network_wait_exclusive,
                causes: Vec::new(),
            });
        }
    }
    let total = nodes.iter().map(|n| n.inclusive).max().unwrap_or(1);
    let edges = raw_edges
        .into_iter()
        .filter_map(|(caller, callee, count, ns)| {
            Some(GraphEdge {
                from: *index.get(&caller)?,
                to: *index.get(&callee)?,
                weight: ns,
                count: Some(count),
            })
        })
        .collect();
    CallGraph {
        nodes,
        edges,
        total,
        queries,
        lines: None,
        trace,
    }
}

/// Attaches the profiled source to an exact capture, with each declared
/// function located and carrying what the run measured.
///
/// An exact capture has no per-line data — `--instrument` times whole calls, not
/// statements — but it does know every function's cost, and the declaration
/// ranges say where each one lives. That is enough to read the file as a map of
/// where the time went, which is the point of having the source view at all.
pub(crate) fn attach_exact_source(graph: &mut crate::call_graph::CallGraph, target: &str) {
    let Ok(text) = std::fs::read_to_string(target) else {
        return;
    };
    let root_ns = graph.nodes.iter().map(|n| n.inclusive).max().unwrap_or(1).max(1);
    let measured: HashMap<&str, &crate::call_graph::GraphNode> =
        graph.nodes.iter().map(|n| (n.name.as_str(), n)).collect();
    let funcs: Vec<crate::call_graph::SourceFunc> = php_decl_ranges(&text)
        .into_iter()
        .filter_map(|range| {
            let node = measured.get(range.name.as_str())?;
            Some(crate::call_graph::SourceFunc {
                name: range.name,
                start: range.start,
                end: range.end,
                self_pct: 100.0 * node.exclusive as f64 / root_ns as f64,
                incl_pct: 100.0 * node.inclusive as f64 / root_ns as f64,
                calls: node.call_count.unwrap_or(0),
            })
        })
        .collect();
    graph.lines = Some(crate::call_graph::SourceLines {
        file: target.to_string(),
        source: text.lines().map(str::to_string).collect(),
        hits: Vec::new(),
        total: 0,
        funcs,
    });
}

/// Shortens `name` to `width` columns, ending it with `…` when it did not fit.
///
/// Truncates by CHARACTER, not by byte. Both callers used to slice `&name[..n]`
/// after testing `name.len()`, which counts bytes: when byte `n` landed inside a
/// multi-byte character, `str` slicing panicked and took the command down while
/// rendering a table. No hostile peer was needed — a function or closure named
/// in a source file with an accented character reaches it, and the names come
/// from the profiled program.
pub(crate) fn ellipsize(name: &str, width: usize) -> String {
    if name.chars().count() <= width {
        return name.to_string();
    }
    let kept: String = name.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// Prints a per-function delta table of the exact capture against a baseline:
/// inclusive time share and call count, before → after, most-changed first.
pub(crate) fn instrument_delta_table(
    base: &crate::call_graph::CallGraph,
    cur: &crate::call_graph::CallGraph,
) -> String {
    let base_root = base.nodes.iter().map(|n| n.inclusive).max().unwrap_or(1).max(1);
    let cur_root = cur.nodes.iter().map(|n| n.inclusive).max().unwrap_or(1).max(1);
    let pct = |incl: u64, root: u64| 100.0 * incl as f64 / root as f64;
    let mut base_by: HashMap<&str, &crate::call_graph::GraphNode> = HashMap::new();
    for n in &base.nodes {
        base_by.insert(n.name.as_str(), n);
    }
    let mut cur_by: HashMap<&str, &crate::call_graph::GraphNode> = HashMap::new();
    for n in &cur.nodes {
        cur_by.insert(n.name.as_str(), n);
    }
    let mut names: Vec<&str> = base_by.keys().chain(cur_by.keys()).copied().collect();
    names.sort();
    names.dedup();
    // Sort by absolute time-share change, most-moved first.
    names.sort_by(|a, b| {
        let da = (cur_by.get(a).map_or(0.0, |n| pct(n.inclusive, cur_root))
            - base_by.get(a).map_or(0.0, |n| pct(n.inclusive, base_root)))
        .abs();
        let db = (cur_by.get(b).map_or(0.0, |n| pct(n.inclusive, cur_root))
            - base_by.get(b).map_or(0.0, |n| pct(n.inclusive, base_root)))
        .abs();
        db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out = String::from("\n--- vs baseline (exact) ---\n");
    for name in names {
        let c_pct = cur_by.get(name).map_or(0.0, |n| pct(n.inclusive, cur_root));
        let b_pct = base_by.get(name).map_or(0.0, |n| pct(n.inclusive, base_root));
        let c_calls = cur_by.get(name).and_then(|n| n.call_count).unwrap_or(0);
        let b_calls = base_by.get(name).and_then(|n| n.call_count).unwrap_or(0);
        let short = ellipsize(name, 24);
        out.push_str(&format!(
            "{short:<25} time {b_pct:>5.1}% -> {c_pct:>5.1}% ({:+5.1})  calls {b_calls} -> {c_calls} ({:+})\n",
            c_pct - b_pct,
            c_calls as i64 - b_calls as i64
        ));
    }
    out
}

/// Renders the exact `--instrument` profile as a stdout bar table, hottest
/// inclusive first.
pub(crate) fn instrument_table(graph: &crate::call_graph::CallGraph) -> String {
    let root = graph.nodes.iter().map(|n| n.inclusive).max().unwrap_or(1).max(1);
    let mut nodes: Vec<&crate::call_graph::GraphNode> = graph.nodes.iter().collect();
    nodes.sort_by(|a, b| b.inclusive.cmp(&a.inclusive).then(a.name.cmp(&b.name)));
    let mut out = format!(
        "\nexact profile — {} functions, total {}\n\n",
        graph.nodes.len(),
        fmt_ns(root)
    );
    // The queries column only appears when the program issued any DB query.
    let has_io = graph.nodes.iter().any(|n| n.io_inclusive > 0);
    // Likewise the retained column: only when some function ends with a net
    // heap delta (allocated minus freed), i.e. the run kept or released objects.
    let has_ret = graph.nodes.iter().any(|n| n.retained_inclusive != 0);
    // And the wait column: only when some call actually blocked in a driver.
    let has_wait = graph.nodes.iter().any(|n| n.wait_inclusive > 0);
    let has_network = graph.nodes.iter().any(|node| node.network_inclusive > 0);
    let has_network_wait = graph
        .nodes
        .iter()
        .any(|node| node.network_wait_inclusive > 0);
    for node in nodes {
        let incl = 100.0 * node.inclusive as f64 / root as f64;
        let excl = 100.0 * node.exclusive as f64 / root as f64;
        let calls = node.call_count.unwrap_or(0);
        let name = ellipsize(&node.name, 26);
        let queries = if has_io {
            format!("  queries {}", node.io_exclusive)
        } else {
            String::new()
        };
        let retained = if has_ret {
            format!("  retained {:+}", node.retained_exclusive)
        } else {
            String::new()
        };
        // Self time splits into recorded DB wait and the unclassified remainder.
        let wait = if has_wait {
            format!(
                "  wait {} non-DB {}",
                fmt_ns(node.wait_exclusive),
                fmt_ns(node.exclusive.saturating_sub(node.wait_exclusive))
            )
        } else {
            String::new()
        };
        let network = if has_network {
            format!("  network {}", node.network_exclusive)
        } else {
            String::new()
        };
        let network_wait = if has_network_wait {
            format!(
                "  network-wait {}",
                fmt_ns(node.network_wait_exclusive)
            )
        } else {
            String::new()
        };
        out.push_str(&format!(
            "{name:<27} {}  incl {incl:>5.1}%  self {excl:>5.1}%  calls {calls}  self {}  allocs {}{queries}{retained}{wait}{network}{network_wait}\n",
            bar(incl, 20),
            fmt_ns(node.exclusive),
            node.alloc_exclusive,
        ));
    }
    out
}

/// Rewrites sample stacks so a PHP frame sampled on a line owned by ANOTHER
/// function's declaration range grows a virtual `(inlined)` child frame — the
/// call boundary the inliner erased, recovered from the source span it kept.
pub(crate) fn inject_inlined_frames(
    samples: &mut [(Vec<Frame>, u64)],
    report: &str,
    binary: &Path,
    php_source: &Path,
) {
    let Ok(source) = std::fs::read_to_string(php_source) else {
        return;
    };
    let ranges = php_decl_ranges(&source);
    if ranges.is_empty() {
        return;
    }
    let Some(load_address) = report
        .lines()
        .find_map(|line| line.strip_prefix("Load Address:").map(str::trim))
    else {
        return;
    };
    let addresses: Vec<u64> = samples
        .iter()
        .flat_map(|(stack, _)| stack.iter())
        .filter(|frame| is_php_symbol(&frame.symbol))
        .filter_map(|frame| frame.address)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let lines = resolve_lines(binary, load_address, &addresses);
    if lines.is_empty() {
        return;
    }
    for (stack, _) in samples.iter_mut() {
        let mut rewritten: Vec<Frame> = Vec::with_capacity(stack.len());
        for frame in stack.iter() {
            let own_name = demangle(&frame.symbol);
            rewritten.push(frame.clone());
            if !is_php_symbol(&frame.symbol) {
                continue;
            }
            let Some(line) = frame.address.and_then(|a| lines.get(&a)) else {
                continue;
            };
            if let Some(owner) = ranges
                .iter()
                .find(|range| range.start <= *line && *line <= range.end)
            {
                if owner.name != own_name {
                    rewritten.push(Frame {
                        symbol: format!("inlined:{}", owner.name),
                        address: None,
                        inlined: true,
                    });
                }
            }
        }
        *stack = rewritten;
    }
}

// --------------------------------------------------------------------------
// Local sampling without /usr/bin/sample
// --------------------------------------------------------------------------


// --------------------------------------------------------------------------
// --probe-host: connect to a running probe endpoint
// --------------------------------------------------------------------------

/// Flattens an exact call graph into weighted leaf-first stacks.
///
/// The exporters consume stacks, not graphs, so a measured capture is walked
/// from its roots and each path is emitted with the self weight of its leaf.
/// Edges naming a node outside the graph are skipped rather than trusted.
pub(crate) fn exact_stacks(graph: &crate::call_graph::CallGraph) -> Vec<(Vec<(String, Kind)>, u64)> {
    let mut children: Vec<Vec<(usize, u64)>> = vec![Vec::new(); graph.nodes.len()];
    let mut incoming: Vec<u64> = vec![0; graph.nodes.len()];
    for edge in &graph.edges {
        if edge.from >= graph.nodes.len() || edge.to >= graph.nodes.len() {
            continue;
        }
        children[edge.from].push((edge.to, edge.weight));
        // A self-edge is not a caller. Counting it as one makes a self-recursive
        // function that nothing else calls look reachable, so it is never
        // treated as a root and its whole time vanishes from the export.
        if edge.from != edge.to {
            incoming[edge.to] += edge.weight;
        }
    }
    let mut stacks = Vec::new();
    let mut path: Vec<(String, Kind)> = Vec::new();
    let mut on_path = vec![false; graph.nodes.len()];
    let mut seen = vec![false; graph.nodes.len()];
    // Roots: everything nothing calls. A capture whose every node has a caller
    // (mutual recursion at the top) would otherwise emit nothing at all, so the
    // heaviest node stands in as the root.
    let mut roots: Vec<usize> = (0..graph.nodes.len())
        .filter(|&i| incoming[i] == 0)
        .collect();
    if roots.is_empty() {
        if let Some(top) = (0..graph.nodes.len()).max_by_key(|&i| graph.nodes[i].inclusive) {
            roots.push(top);
        }
    }
    // The floor is a share of the capture, so it means the same thing whatever
    // unit the numbers are in.
    let budget_total: u64 = roots.iter().map(|&r| graph.nodes[r].inclusive).sum();
    let floor = budget_total / EXACT_STACK_FLOOR_DIVISOR;
    for root in roots {
        let inclusive = graph.nodes[root].inclusive;
        exact_walk(
            graph,
            &children,
            root,
            inclusive,
            floor,
            &mut path,
            &mut on_path,
            &mut seen,
            &mut stacks,
        );
    }
    if stacks.len() >= EXACT_STACK_CAP {
        eprintln!(
            "elephc monitor: the call graph produced more than {EXACT_STACK_CAP} distinct \
             stacks; deeper paths are folded into their callers. Totals are unaffected, \
             the flame view is coarser."
        );
    }
    // Anything left over sits in a cycle no root reaches. Its own time is real
    // and belongs in the total, but re-descending from it would count its
    // already-placed callees twice — so it is emitted flat, as itself.
    for (i, node) in graph.nodes.iter().enumerate() {
        if !seen[i] && node.exclusive > 0 {
            stacks.push((vec![(node.name.clone(), Kind::Php)], node.exclusive));
        }
    }
    stacks
}

/// One node of `exact_stacks`: place `budget` nanoseconds of this function on
/// the current path, then hand each child the time its edge measured.
pub(crate) fn exact_walk(
    graph: &crate::call_graph::CallGraph,
    children: &[Vec<(usize, u64)>],
    node: usize,
    budget: u64,
    floor: u64,
    path: &mut Vec<(String, Kind)>,
    on_path: &mut [bool],
    seen: &mut [bool],
    stacks: &mut Vec<(Vec<(String, Kind)>, u64)>,
) {
    path.push((graph.nodes[node].name.clone(), Kind::Php));
    on_path[node] = true;
    seen[node] = true;
    // The denominator is whichever is larger: the function's own inclusive time,
    // or what its edges actually claim. They should agree, and in every capture
    // measured here they do — but a truncated or corrupted dump can name edges
    // summing past the parent, and then the children walk away with more than
    // the budget while `own` saturates to zero. Taking the larger keeps the
    // proportions and makes "the export totals the run" true by construction
    // rather than true by luck.
    let claimed: u64 = children[node]
        .iter()
        .filter(|(child, _)| !on_path[*child])
        .map(|(_, edge_ns)| *edge_ns)
        .sum();
    let inclusive = graph.nodes[node].inclusive.max(claimed).max(1);
    // This path's share of the function, so a function with several callers is
    // divided among them instead of counted once per caller.
    let scale = |value: u64| -> u64 {
        (u128::from(value) * u128::from(budget) / u128::from(inclusive)) as u64
    };
    let mut spent = 0u64;
    for &(child, edge_ns) in &children[node] {
        if on_path[child] {
            // Recursion: the cycle's remaining time stays here rather than
            // descending forever.
            continue;
        }
        let share = scale(edge_ns);
        // Too small to say anything, too deep to descend safely, or past the
        // ceiling: the time stays here rather than being dropped, so the total
        // is unchanged and only the shape gets coarser.
        if share == 0 {
            continue;
        }
        if share <= floor || path.len() >= EXACT_STACK_MAX_DEPTH || stacks.len() >= EXACT_STACK_CAP
        {
            // Folded into this frame: `spent` deliberately does not grow, so the
            // share stays in `own` below.
            //
            // Everything under it must then be marked accounted-for. The leftover
            // pass emits the self time of every node it never saw, and a folded
            // subtree is exactly a set of nodes nobody saw — so without this its
            // time is counted twice, once inside the caller and once flat. It
            // showed up as +0.0053% on a real capture, which is small enough to
            // have been read as rounding.
            mark_folded(children, child, on_path, seen);
            continue;
        }
        spent = spent.saturating_add(share);
        exact_walk(graph, children, child, share, floor, path, on_path, seen, stacks);
    }
    let own = budget.saturating_sub(spent);
    if own > 0 {
        stacks.push((path.clone(), own));
    }
    on_path[node] = false;
    path.pop();
}
