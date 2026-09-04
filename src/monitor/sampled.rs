//! Purpose:
//! The sampled path: `--live` and `--attach`, which read a process from the
//! outside through `/usr/bin/sample`, and the folded-stack parsing shared with
//! the endpoint's sampled answer.
//!
//! Called from:
//! - `monitor::main` for `--live`/`--attach`.
//! - `remote::run`, to parse what a service's ring returned.
//!
//! Key details:
//! - macOS-only, because no equivalent external sampler ships on Linux; the
//!   Linux answer is the endpoint.
//! - Sampled shares sharpen as samples accumulate and carry real noise; time
//!   spent blocked on I/O is not attributed on the CPU-time timer.

use super::*;

/// Aggregates the display stacks into a call graph: one node per PHP function
/// (inclusive/exclusive/causes reuse `table_stats`), and one edge per distinct
/// caller->callee adjacency, weighted by the samples whose stack traversed it.
pub(crate) fn build_call_graph(display: &[(Vec<(String, Kind)>, u64)]) -> crate::call_graph::CallGraph {
    use crate::call_graph::{CallGraph, GraphEdge, GraphNode};
    let stats = table_stats(display);
    // Node order is stable and reads top-down: hottest inclusive first, then name.
    let mut names: Vec<&String> = stats.totals.keys().collect();
    names.sort_by(|a, b| stats.totals[*b].cmp(&stats.totals[*a]).then_with(|| a.cmp(b)));
    let index: HashMap<&String, usize> =
        names.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let nodes: Vec<GraphNode> = names
        .iter()
        .map(|name| {
            let mut causes: Vec<(String, u64)> = stats
                .causes
                .get(*name)
                .map(|m| m.iter().map(|(c, w)| ((*c).to_string(), *w)).collect())
                .unwrap_or_default();
            causes.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            GraphNode {
                name: (*name).clone(),
                inclusive: stats.totals[*name],
                exclusive: stats.selfs.get(*name).copied().unwrap_or(0),
                // Sampling gives no exact count; --counters is the exact path.
                call_count: None,
                // Per-node allocation and DB metrics need the exact call graph;
                // the sampled service summary reports only window-level deltas.
                alloc_inclusive: 0,
                alloc_exclusive: 0,
                io_inclusive: 0,
                stream_inclusive: 0,
                stream_exclusive: 0,
                io_exclusive: 0,
                retained_inclusive: 0,
                retained_exclusive: 0,
                wait_inclusive: 0,
                wait_exclusive: 0,
                causes,
            }
        })
        .collect();
    // Edges: adjacent PHP frames within each stack, deduped per stack so a
    // mutually-recursive chain cannot push one edge past the stack's own weight.
    let mut edge_weights: BTreeMap<(usize, usize), u64> = BTreeMap::new();
    for (stack, weight) in display {
        let php: Vec<usize> = stack
            .iter()
            .filter(|(_, k)| matches!(k, Kind::Php | Kind::PhpInlined))
            .filter_map(|(n, _)| index.get(n).copied())
            .collect();
        let mut seen_pairs = HashSet::new();
        for pair in php.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            if from == to {
                continue; // self-recursion loops back on one node; the layered view drops it
            }
            if seen_pairs.insert((from, to)) {
                *edge_weights.entry((from, to)).or_default() += weight;
            }
        }
    }
    let edges = edge_weights
        .into_iter()
        .map(|((from, to), weight)| GraphEdge { from, to, weight, count: None })
        .collect();
    CallGraph {
        nodes,
        edges,
        total: stats.grand,
        queries: Vec::new(),
        // A sampled capture counts nothing exactly: stream operations come from
        // the instrumentation, like queries.
        stream_ops: Vec::new(),
        lines: None,
        trace: None,
    }
}

/// Renders the probe's per-route database counters, when the capture carries any.
///
/// Printed apart from the cause table and labelled, because these numbers are a
/// different KIND from everything around them: the table's shares are sampled at
/// 1000 Hz, while a driver call fires exactly one event, so these counts are
/// exact. Measured on the demo service, a run the sampler saw only 17 times
/// still reported 551 queries — the same 551 `--instrument` reports. Presenting
/// the two without saying which is which is how a profile misleads.
pub(crate) fn probe_io_summary(text: &str) -> String {
    let mut rows = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("elephc-probe-io: ") else {
            continue;
        };
        // "<route> ops=<n> wait_ns=<n>" — the route may contain spaces, so read
        // the counters off the end rather than splitting from the left.
        let Some((route, tail)) = rest.rsplit_once(" ops=") else {
            continue;
        };
        let Some((ops, wait)) = tail.split_once(" wait_ns=") else {
            continue;
        };
        match (ops.trim().parse::<u64>(), wait.trim().parse::<u64>()) {
            (Ok(ops), Ok(wait)) => rows.push((route.to_string(), ops, wait)),
            _ => continue,
        }
    }
    if rows.is_empty() {
        return String::new();
    }
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    let total_ops: u64 = rows.iter().map(|r| r.1).sum();
    let total_wait: u64 = rows.iter().map(|r| r.2).sum();
    let mut out = format!(
        "\nDatabase — {total_ops} query operation(s), {} driver wait. Exact, not sampled: a driver call \
         reports itself,\nso these counts do not depend on how often the profiler looked.\n\n",
        fmt_ns(total_wait)
    );
    for (route, ops, wait) in rows {
        out.push_str(&format!("{route:<40}{ops:>8} ops{:>12}\n", fmt_ns(wait)));
    }
    out.push_str(&probe_alloc_summary(text));
    out
}

/// Renders the probe's sampled allocation attribution.
///
/// Two different claims live here and conflating them would be the mistake:
/// each reported delta is an exact counter difference between two samples, but
/// the window's first baseline and its unsampled tail are not counted. The
/// attribution is sampled too — a delta is credited to whichever stack the
/// later sample caught.
///
/// So this answers "where does allocation happen", not "how much did this
/// function allocate". `--instrument` is the mode that answers the second.
pub(crate) fn probe_alloc_summary(text: &str) -> String {
    let mut rows: Vec<(String, u64)> = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("elephc-probe-alloc: ") else {
            continue;
        };
        let Some((stack, allocs)) = rest.rsplit_once(' ') else {
            continue;
        };
        if let Ok(allocs) = allocs.trim().parse::<u64>() {
            rows.push((stack.to_string(), allocs));
        }
    }
    if rows.is_empty() {
        return String::new();
    }
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    let total: u64 = rows.iter().map(|r| r.1).sum();
    let mut out = format!(
        "\nallocations — {total} observed between samples; attribution is sampled, so it says\n\
         WHERE allocation happens rather than how much each function allocated. The first\n\
         baseline and allocations after the final sample are outside this total.\n\n"
    );
    for (stack, allocs) in rows.iter().take(5) {
        let leaf = stack.rsplit(';').find(|f| *f != "<native>").unwrap_or(stack);
        out.push_str(&format!(
            "{leaf:<40}{allocs:>10}  {:>5.1}%\n",
            100.0 * *allocs as f64 / total.max(1) as f64
        ));
    }
    out
}

/// Parses `sample`'s indented call-graph section into depth rows.
///
/// Depth is encoded by the prefix width — two columns per level, counting the
/// `+ ! : |` ancestry decorations as well as spaces. Thread headers carry no
/// `(in module)` suffix and are skipped by the shape check.
pub(crate) fn parse_call_graph(report: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut in_graph = false;
    for line in report.lines() {
        if line.starts_with("Call graph:") {
            in_graph = true;
            continue;
        }
        if in_graph && (line.starts_with("Total number") || line.trim().is_empty()) {
            break;
        }
        if !in_graph {
            continue;
        }
        let prefix_len = line
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'+' | b'!' | b':' | b'|'))
            .count();
        let rest = &line[prefix_len..];
        let Some((count_text, tail)) = rest.split_once(' ') else {
            continue;
        };
        let Ok(count) = count_text.parse::<u64>() else {
            continue;
        };
        let Some((symbol, module_tail)) = tail.split_once("  (in ") else {
            continue;
        };
        let module = module_tail
            .split_once(')')
            .map(|(module, _)| module)
            .unwrap_or("");
        // The bracketed list holds one address per merged offset bucket; the
        // first is the dominant one and enough for line attribution.
        let address = line
            .rsplit_once('[')
            .and_then(|(_, brackets)| brackets.split([',', ']']).next())
            .map(str::trim)
            .and_then(|first| first.strip_prefix("0x"))
            .and_then(|hex| u64::from_str_radix(hex, 16).ok());
        rows.push(Row {
            depth: prefix_len / 2,
            count,
            symbol: symbol.trim().to_string(),
            module: module.to_string(),
            address,
        });
    }
    // Drop loader scaffolding and rebase depths on the program frames.
    rows.retain(|row| row.module != "dyld");
    if let Some(base) = rows.iter().map(|row| row.depth).min() {
        for row in &mut rows {
            row.depth -= base;
        }
    }
    rows
}

/// Converts depth rows into leaf samples: each node contributes its self weight
/// (count minus its children's counts) under its full ancestor stack.
pub(crate) fn build_samples(rows: &[Row]) -> Vec<(Vec<Frame>, u64)> {
    let mut samples = Vec::new();
    let mut stack: Vec<(usize, Frame, i64)> = Vec::new();
    let flush = |stack: &mut Vec<(usize, Frame, i64)>, samples: &mut Vec<(Vec<Frame>, u64)>| {
        let (_, frame, remaining) = stack.pop().expect("flush on empty stack");
        if remaining > 0 {
            let mut frames: Vec<Frame> = stack.iter().map(|(_, frame, _)| frame.clone()).collect();
            frames.push(frame);
            samples.push((frames, remaining as u64));
        }
    };
    for row in rows {
        while stack
            .last()
            .is_some_and(|(depth, _, _)| *depth >= row.depth)
        {
            flush(&mut stack, &mut samples);
        }
        if let Some((_, _, remaining)) = stack.last_mut() {
            *remaining -= row.count as i64;
        }
        stack.push((
            row.depth,
            Frame {
                symbol: row.symbol.clone(),
                address: row.address,
                inlined: false,
            },
            row.count as i64,
        ));
    }
    while !stack.is_empty() {
        flush(&mut stack, &mut samples);
    }
    samples
}

/// Parses the endpoint's folded text (`elephc-probe: a;b;c <count>`) into the
/// display stacks the renderers consume. Probe frames are already PHP names or
/// `<native>`, so classification is a name test.
pub(crate) fn folded_text_to_display(text: &str) -> Vec<(Vec<(String, Kind)>, u64)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("elephc-probe: ") else {
            continue;
        };
        let Some((stack_text, count_text)) = rest.rsplit_once(' ') else {
            continue;
        };
        let Ok(count) = count_text.trim().parse::<u64>() else {
            continue;
        };
        let stack: Vec<(String, Kind)> = stack_text
            .split(';')
            .map(|name| {
                let kind = if name == "<native>" {
                    Kind::Native
                } else {
                    Kind::Php
                };
                (name.to_string(), kind)
            })
            .collect();
        if !stack.is_empty() {
            out.push((stack, count));
        }
    }
    out
}

// --------------------------------------------------------------------------
// Rendering
// --------------------------------------------------------------------------

/// Whether a file is a Speedscope document rather than an exact capture.
///
/// Used only to explain a failure, so it reads the shape rather than validating:
/// a `profiles` array plus the `$schema` Speedscope writes is enough to be sure
/// which of the two files someone reached for.
pub(crate) fn looks_like_speedscope(path: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    doc.get("profiles").map(|p| p.is_array()).unwrap_or(false)
        && doc
            .get("$schema")
            .and_then(|s| s.as_str())
            .map(|s| s.contains("speedscope"))
            .unwrap_or(false)
}

/// Marks a folded subtree as accounted for, so the leftover pass leaves it alone.
///
/// Iterative rather than recursive: this runs precisely when a graph turned out
/// to be deeper or wider than expected, which is the worst moment to add stack
/// frames of its own.
pub(crate) fn mark_folded(
    children: &[Vec<(usize, u64)>],
    start: usize,
    on_path: &[bool],
    seen: &mut [bool],
) {
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        if seen[node] {
            continue;
        }
        seen[node] = true;
        for &(child, _) in &children[node] {
            // A node on the current path is an ancestor, not part of what was
            // folded away; its own frame still emits.
            if !seen[child] && !on_path[child] {
                stack.push(child);
            }
        }
    }
}
