//! Purpose:
//! Correlates per-request slices from several service logs into span trees, so a
//! profile joins the distributed trace its caller already belongs to.
//!
//! Called from:
//! - `monitor::main`, for `--stitch <log>...`.
//!
//! Key details:
//! - Grouped by W3C trace id and nested by parent span; a span whose parent never
//!   arrived stays a root rather than being dropped.
//! - A slice records its duration but not its start time, so the view compares
//!   durations rather than placing spans on a shared axis.

use super::*;

/// Aggregates slices into one row per group.
///
/// Grouped by service, or by `service · route` when every slice names a route —
/// which is the breakdown worth paging on, since one slow endpoint is invisible
/// in a service-wide p95. All-or-nothing: mixing routed and unrouted rows would
/// double-count the same requests under two headings.
pub(crate) fn service_stats(slices: &[Slice]) -> Vec<ServiceStats> {
    use std::collections::BTreeMap;
    let routed = !slices.is_empty()
        && slices
            .iter()
            .all(|s| s.graph.trace.as_ref().is_some_and(|t| !t.route.is_empty()));
    let mut by_service: BTreeMap<(String, Option<String>), Vec<&Slice>> = BTreeMap::new();
    for slice in slices {
        let route = routed.then(|| {
            slice
                .graph
                .trace
                .as_ref()
                .map(|t| t.route.clone())
                .unwrap_or_default()
        });
        by_service
            .entry((slice.service.clone(), route))
            .or_default()
            .push(slice);
    }
    by_service
        .into_iter()
        .map(|((service, route), members)| {
            let mut durations: Vec<u64> = members.iter().map(|s| s.graph.total).collect();
            durations.sort_unstable();
            let total_ns: u64 = durations.iter().sum();
            let queries: u64 = members
                .iter()
                .flat_map(|s| s.graph.nodes.iter())
                .map(|n| n.io_exclusive)
                .sum();
            let wait_ns: u64 = members
                .iter()
                .flat_map(|s| s.graph.nodes.iter())
                .map(|n| n.wait_exclusive)
                .sum();
            let network: u64 = members
                .iter()
                .flat_map(|s| s.graph.nodes.iter())
                .map(|n| n.network_exclusive)
                .sum();
            let network_wait_ns: u64 = members
                .iter()
                .flat_map(|s| s.graph.nodes.iter())
                .map(|n| n.network_wait_exclusive)
                .sum();
            // The window runs from the first request opening to the last one
            // finishing, the only span a rate can honestly divide by.
            let starts: Option<Vec<u64>> = members
                .iter()
                .map(|s| s.graph.trace.as_ref().and_then(|t| t.start_us))
                .collect();
            let rps = starts.and_then(|starts| {
                let first = *starts.iter().min()?;
                let last = members
                    .iter()
                    .zip(&starts)
                    .map(|(s, start)| start + s.graph.total / 1_000)
                    .max()?;
                (last > first).then(|| members.len() as f64 / ((last - first) as f64 / 1e6))
            });
            ServiceStats {
                service,
                route,
                requests: members.len(),
                p50: percentile(&durations, 50.0),
                p90: percentile(&durations, 90.0),
                p95: percentile(&durations, 95.0),
                p99: percentile(&durations, 99.0),
                rps,
                queries_per_request: queries as f64 / members.len() as f64,
                wait_share: if total_ns == 0 {
                    0.0
                } else {
                    100.0 * wait_ns as f64 / total_ns as f64
                },
                network_per_request: network as f64 / members.len() as f64,
                network_wait_seconds_per_request: network_wait_ns as f64
                    / members.len() as f64
                    / 1e9,
            }
        })
        .collect()
}

/// Grades every captured slice against the project's performance budget.
///
/// The budget is the same `.elephc` file a dev run is graded by, and the checks
/// are the same checks — which is the point. A profile taken in production
/// should answer the same questions as one taken on a laptop, or the budget only
/// ever describes the laptop.
///
/// Where it differs from a one-shot run is what "failed" means across many
/// requests: an assertion is reported with the NUMBER of requests that violated
/// it and the worst value seen, because "p99 blew the query budget" and "every
/// request did" call for different actions. With a single slice this degrades to
/// exactly the one-shot report.
pub(crate) fn stitch_assert_report(slices: &[Slice], asserts: &[(String, Option<String>)]) -> (String, bool) {
    if asserts.is_empty() || slices.is_empty() {
        return (String::new(), true);
    }
    // spec -> (label, violations, worst actual, budget, op, evaluated)
    let mut tally: Vec<crate::call_graph::AssertOutcome> = Vec::new();
    let mut violations: Vec<usize> = Vec::new();
    for slice in slices {
        let outcomes = evaluate_asserts(&slice.graph, asserts);
        if tally.is_empty() {
            tally = outcomes;
            violations = vec![0; tally.len()];
            for (index, outcome) in tally.iter().enumerate() {
                if outcome.status != crate::call_graph::AssertStatus::Pass {
                    violations[index] = 1;
                }
            }
            continue;
        }
        for (index, outcome) in outcomes.iter().enumerate() {
            if outcome.status != crate::call_graph::AssertStatus::Pass {
                violations[index] += 1;
            }
            // Keep the worst value seen, which is the one worth acting on.
            let worse = match (tally[index].actual, outcome.actual) {
                (Some(a), Some(b)) => b > a,
                (None, Some(_)) => true,
                _ => false,
            };
            if worse {
                tally[index].actual = outcome.actual;
                tally[index].status = outcome.status;
            }
        }
    }

    let failed = violations.iter().filter(|n| **n > 0).count();
    let mut out = format!(
        "\nchecks — {} of {} assertion(s) violated, over {} request(s)\n",
        failed,
        tally.len(),
        slices.len()
    );
    for (index, outcome) in tally.iter().enumerate() {
        let n = violations[index];
        // SKIP stays distinct from FAIL, exactly as in a one-shot run: an
        // assertion naming a function no request reached says something is wrong
        // with the BUDGET, not the code. It still fails the gate — a budget that
        // silently measures nothing is worse than none — but calling it a
        // violation would send someone looking in the wrong place.
        let mark = match (n, outcome.actual) {
            (0, _) => "PASS",
            (_, None) => "SKIP",
            _ => "FAIL",
        };
        let actual = outcome
            .actual
            .map(|v| format!("worst {}", trim_number(v)))
            .unwrap_or_else(|| format!("'{}' never ran", outcome.target));
        out.push_str(&format!(
            "  [{mark}] {} ({actual}){}{}\n",
            outcome.spec,
            match (n, outcome.actual) {
                (0, _) | (_, None) => String::new(),
                _ => format!(" — {n} of {} request(s)", slices.len()),
            },
            outcome
                .label
                .as_ref()
                .map(|l| format!("  — {l}"))
                .unwrap_or_default(),
        ));
    }
    (out, failed == 0)
}

/// Renders the per-service summary that opens a stitch report.
pub(crate) fn service_summary(slices: &[Slice]) -> String {
    let stats = service_stats(slices);
    if stats.is_empty() {
        return String::new();
    }
    let mut out = String::from("\nper service\n\n");
    out.push_str(&format!(
        "{:<34}{:>5}{:>11}{:>11}{:>11}{:>11}{:>9}{:>9}{:>8}\n",
        "service", "n", "p50", "p90", "p95", "p99", "rps", "q/req", "wait"
    ));
    let mut thin = false;
    for s in &stats {
        thin |= s.requests < 20;
        out.push_str(&format!(
            "{:<34}{:>5}{:>11}{:>11}{:>11}{:>11}{:>9}{:>9}{:>7.0}%\n",
            match &s.route {
                Some(route) => format!("{} · {}", s.service, route),
                None => s.service.clone(),
            },
            s.requests,
            fmt_ns(s.p50),
            fmt_ns(s.p90),
            fmt_ns(s.p95),
            fmt_ns(s.p99),
            s.rps
                .map(|r| format!("{r:.1}"))
                .unwrap_or_else(|| "-".to_string()),
            format!("{:.1}", s.queries_per_request),
            s.wait_share,
        ));
    }
    if thin {
        // Printing "p99" over a handful of requests would dress the maximum up as
        // a distribution. The numbers stay — they are still the right ones — but
        // the reader is told what they are actually looking at.
        out.push_str(
            "\nfewer than 20 requests for a service: its upper percentiles are that \
             service's\nslowest requests, not a distribution.\n",
        );
    }
    out
}

/// Renders correlated slices as span trees, one section per trace id.
///
/// Slices arrive from separate service logs with no shared clock, so they are
/// grouped by trace and nested by parent; a span whose parent never arrived
/// stays a root rather than being dropped.
pub(crate) fn stitch_report(slices: &[Slice]) -> String {
    use std::collections::BTreeMap;
    let mut by_trace: BTreeMap<&str, Vec<&Slice>> = BTreeMap::new();
    for slice in slices {
        if let Some(trace) = &slice.graph.trace {
            by_trace.entry(trace.trace_id.as_str()).or_default().push(slice);
        }
    }
    if by_trace.is_empty() {
        return "no distributed traces found — are the services built with --web --instrument?\n"
            .to_string();
    }
    let mut out = service_summary(slices);
    out.push_str(&format!(
        "\ndistributed traces — {} trace(s) over {} slice(s)\n",
        by_trace.len(),
        slices.len()
    ));
    for (trace_id, members) in by_trace {
        let present: HashSet<&str> = members
            .iter()
            .filter_map(|s| s.graph.trace.as_ref())
            .map(|t| t.span_id.as_str())
            .collect();
        let root_total: u64 = members
            .iter()
            .map(|s| s.graph.total)
            .max()
            .unwrap_or(1)
            .max(1);
        // Placed layout needs every member dated — see `render_trace_html`.
        let window = members
            .iter()
            .map(|s| s.graph.trace.as_ref().and_then(|t| t.start_us))
            .collect::<Option<Vec<u64>>>()
            .and_then(|starts| {
                let t0 = *starts.iter().min()?;
                let end = members
                    .iter()
                    .map(|s| {
                        s.graph.trace.as_ref().and_then(|t| t.start_us).unwrap_or(t0)
                            + s.graph.total / 1_000
                    })
                    .max()?;
                (end > t0).then_some((t0, end - t0))
            });
        out.push_str(&format!("\ntrace {trace_id}\n"));
        // Children of each span, then walk from the roots.
        let mut children: HashMap<&str, Vec<&Slice>> = HashMap::new();
        let mut roots: Vec<&Slice> = Vec::new();
        for slice in &members {
            let trace = slice.graph.trace.as_ref().expect("filtered above");
            if trace.parent_span_id.is_empty() || !present.contains(trace.parent_span_id.as_str()) {
                roots.push(slice);
            } else {
                children
                    .entry(trace.parent_span_id.as_str())
                    .or_default()
                    .push(slice);
            }
        }
        let mut stack: Vec<(&Slice, usize)> =
            roots.into_iter().rev().map(|slice| (slice, 0)).collect();
        let mut guard = 0;
        while let Some((slice, depth)) = stack.pop() {
            if guard > 4096 {
                out.push_str("  … trace truncated (cycle or excessive depth)\n");
                break;
            }
            guard += 1;
            let trace = slice.graph.trace.as_ref().expect("filtered above");
            let (offset, share) = match window {
                Some((t0, span_us)) => {
                    let start = trace.start_us.unwrap_or(t0).saturating_sub(t0);
                    (
                        100.0 * start as f64 / span_us as f64,
                        100.0 * (slice.graph.total / 1_000) as f64 / span_us as f64,
                    )
                }
                None => (0.0, 100.0 * slice.graph.total as f64 / root_total as f64),
            };
            let queries: u64 = slice.graph.nodes.iter().map(|n| n.io_exclusive).sum();
            let waited: u64 = slice.graph.nodes.iter().map(|n| n.wait_exclusive).sum();
            let network: u64 = slice
                .graph
                .nodes
                .iter()
                .map(|node| node.network_exclusive)
                .sum();
            let network_wait: u64 = slice
                .graph
                .nodes
                .iter()
                .map(|node| node.network_wait_exclusive)
                .sum();
            out.push_str(&format!(
                "{:indent$}{} {}  {}  {}  {} fn{}{}{}{}\n",
                "",
                if depth == 0 { "●" } else { "└─" },
                slice.service,
                fmt_ns(slice.graph.total),
                if window.is_some() { placed_bar(offset, share, 12) } else { bar(share, 12) },
                slice.graph.nodes.len(),
                if queries > 0 {
                    format!("  {queries} queries")
                } else {
                    String::new()
                },
                if waited > 0 {
                    format!("  {} waiting", fmt_ns(waited))
                } else {
                    String::new()
                },
                if network > 0 {
                    format!("  {network} network")
                } else {
                    String::new()
                },
                if network_wait > 0 {
                    format!("  {} network-wait", fmt_ns(network_wait))
                } else {
                    String::new()
                },
                indent = depth * 2,
            ));
            if let Some(kids) = children.get(trace.span_id.as_str()) {
                for kid in kids.iter().rev() {
                    stack.push((kid, depth + 1));
                }
            }
        }
    }
    out
}

/// Reads `--stitch` service logs and correlates their request slices into
/// distributed traces.
pub(crate) fn run_stitch(cmd: &MonitorCommand) -> i32 {
    let mut slices = Vec::new();
    for path in &cmd.stitch {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("elephc monitor: cannot read {path}: {error}");
                return 1;
            }
        };
        // Name the service after its log file, which is what the operator
        // recognizes; the profile itself carries no service identity.
        let service = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        for chunk in split_slices(&text) {
            let graph = parse_instrument_dump(&chunk);
            if graph.trace.is_some() && !graph.nodes.is_empty() {
                slices.push(Slice {
                    service: service.clone(),
                    graph,
                });
            }
        }
    }
    if slices.is_empty() {
        eprintln!(
            "elephc monitor: no profiled request slices in {} — a service must be built with \
             --web --instrument for its log to carry them",
            cmd.stitch.join(", ")
        );
        return 1;
    }
    print!("{}", stitch_report(&slices));
    // The same budget that grades a dev run grades this capture, so a production
    // profile answers the same questions rather than a reduced set.
    // The budget is found by walking up from the first log, the way a dev run
    // walks up from the source: one file at a project root serves both.
    let source = cmd.stitch.first().cloned().unwrap_or_default();
    let mut asserts: Vec<(String, Option<String>)> = cmd
        .asserts
        .iter()
        .map(|spec| (spec.clone(), None))
        .collect();
    match load_assert_file(cmd.assert_file.as_deref(), &source) {
        Ok(Some((from_file, path))) => {
            if !from_file.is_empty() {
                println!("budget: {path}");
            }
            asserts.extend(from_file);
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("elephc monitor: {error}");
            return 1;
        }
    }
    let (checks, ok) = stitch_assert_report(&slices, &asserts);
    print!("{checks}");
    if let Some(path) = &cmd.prom_out {
        if let Err(error) = std::fs::write(path, prometheus_text(&slices)) {
            eprintln!("elephc monitor: cannot write {path}: {error}");
            return 1;
        }
        println!("wrote {path}");
    }
    if let Some(endpoint) = &cmd.otlp {
        if export_otlp(&slices, endpoint) != 0 {
            return 1;
        }
    }
    if !ok {
        // Same exit code a dev run uses, so one gate serves both.
        return 2;
    }
    if let Some(path) = &cmd.html_out {
        let spans: Vec<crate::call_graph::TraceSpan> = slices
            .iter()
            .filter_map(|slice| {
                let trace = slice.graph.trace.as_ref()?;
                let root = slice.graph.total.max(1);
                // The heaviest functions of the slice, so a slow hop is
                // diagnosable from the waterfall without opening its profile.
                let mut nodes: Vec<&crate::call_graph::GraphNode> =
                    slice.graph.nodes.iter().collect();
                nodes.sort_by(|a, b| b.exclusive.cmp(&a.exclusive));
                let top = nodes
                    .iter()
                    .take(8)
                    .map(|n| {
                        (
                            n.name.clone(),
                            100.0 * n.inclusive as f64 / root as f64,
                            100.0 * n.exclusive as f64 / root as f64,
                        )
                    })
                    .collect();
                Some(crate::call_graph::TraceSpan {
                    service: slice.service.clone(),
                    trace_id: trace.trace_id.clone(),
                    span_id: trace.span_id.clone(),
                    parent_span_id: trace.parent_span_id.clone(),
                    total_ns: slice.graph.total,
                    functions: slice.graph.nodes.len(),
                    queries: slice.graph.nodes.iter().map(|n| n.io_exclusive).sum(),
                    wait_ns: slice.graph.nodes.iter().map(|n| n.wait_exclusive).sum(),
                    network_ops: slice
                        .graph
                        .nodes
                        .iter()
                        .map(|node| node.network_exclusive)
                        .sum(),
                    network_wait_ns: slice
                        .graph
                        .nodes
                        .iter()
                        .map(|node| node.network_wait_exclusive)
                        .sum(),
                    start_us: trace.start_us,
                    top,
                })
            })
            .collect();
        let html = crate::call_graph::render_trace_html(&spans, "elephc distributed trace");
        match std::fs::write(path, html) {
            Ok(()) => println!("wrote {path}"),
            Err(error) => {
                eprintln!("elephc monitor: cannot write {path}: {error}");
                return 1;
            }
        }
    }
    0
}
