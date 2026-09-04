//! Purpose:
//! Writes a capture out in the formats other tools read: Speedscope JSON, pprof,
//! Graphviz DOT, and the self-contained HTML call graph.
//!
//! Called from:
//! - `local::run` and `sampled::run`, after a capture is rendered.
//!
//! Key details:
//! - Every export is one file with nothing to fetch: a report has to open on a
//!   laptop with no network.
//! - The exporters consume weighted stacks, so an exact graph is flattened first.

use super::*;

/// Writes the DOT and/or HTML call-graph exports when their flags are set.
pub(crate) fn write_graph_exports(
    cmd: &MonitorCommand,
    display: &[(Vec<(String, Kind)>, u64)],
    title: &str,
    lines: Option<&LineProfile>,
) -> Result<(), String> {
    if cmd.dot_out.is_none() && cmd.html_out.is_none() {
        return Ok(());
    }
    let mut graph = build_call_graph(display);
    if let Some(lines) = lines {
        graph.lines = Some(crate::call_graph::SourceLines {
            file: lines.file.clone(),
            source: lines.source.clone(),
            hits: lines
                .hits
                .iter()
                .map(|(line, samples)| (*line, *samples))
                .collect(),
            total: lines.total,
            funcs: Vec::new(),
        });
    }
    if let Some(path) = &cmd.dot_out {
        std::fs::write(path, crate::call_graph::render_dot(&graph))
            .map_err(|e| format!("cannot write {path}: {e}"))?;
        println!("wrote {path}");
    }
    if let Some(path) = &cmd.html_out {
        std::fs::write(path, crate::call_graph::render_html(&graph, title))
            .map_err(|e| format!("cannot write {path}: {e}"))?;
        println!("wrote {path}");
    }
    Ok(())
}

/// Live-mode graph export: append this window's call graph to a rolling ring of
/// the last 10 and rewrite the self-refreshing HTML (and, if asked, the latest
/// DOT). Writes are atomic so the auto-reloading page never reads a half file.
pub(crate) fn write_live_graphs(
    cmd: &MonitorCommand,
    display: &[(Vec<(String, Kind)>, u64)],
    title: &str,
    ring: &mut std::collections::VecDeque<(u128, crate::call_graph::CallGraph)>,
) {
    let graph = build_call_graph(display);
    if let Some(path) = &cmd.dot_out {
        let _ = write_atomic(path, &crate::call_graph::render_dot(&graph));
    }
    if let Some(path) = &cmd.html_out {
        ring.push_back((epoch_millis(), graph));
        while ring.len() > 10 {
            ring.pop_front();
        }
        let frames: Vec<(u128, &crate::call_graph::CallGraph)> =
            ring.iter().map(|(ts, g)| (*ts, g)).collect();
        let html = crate::call_graph::render_html_frames(&frames, title, true, cmd.duration_secs, false, &[]);
        let _ = write_atomic(path, &html);
    }
}

/// Writes `contents` to `path` atomically (temp file + rename), so a concurrent
/// reader (the live page reloading) sees either the old or the new file whole.
pub(crate) fn write_atomic(path: &str, contents: &str) -> std::io::Result<()> {
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

/// Renders the per-service stats in the Prometheus text exposition format.
///
/// A file rather than an endpoint: `monitor` runs and exits, so there is nothing
/// for a scraper to poll. The textfile collector is the established way to get
/// numbers from a batch job into Prometheus, and it needs no HTTP server here.
///
/// Percentiles map onto `summary`, which is what that type is for. They are NOT
/// exposed as a histogram: a histogram carries buckets a backend re-aggregates,
/// and we have exact per-request values rather than bucket counts, so presenting
/// them as buckets would invent a resolution the capture does not have.
pub(crate) fn prometheus_text(slices: &[Slice]) -> String {
    let stats = service_stats(slices);
    let mut out = String::new();
    out.push_str(
        "# HELP elephc_requests_total Profiled requests observed in this capture.\n\
         # TYPE elephc_requests_total counter\n",
    );
    for s in &stats {
        out.push_str(&format!("elephc_requests_total{} {}\n", labels(s), s.requests));
    }
    out.push_str(
        "# HELP elephc_request_duration_seconds Exact per-request duration.\n\
         # TYPE elephc_request_duration_seconds summary\n",
    );
    for s in &stats {
        for (quantile, value) in [
            ("0.5", s.p50),
            ("0.9", s.p90),
            ("0.95", s.p95),
            ("0.99", s.p99),
        ] {
            let mut with_quantile = labels(s);
            with_quantile.truncate(with_quantile.len() - 1);
            out.push_str(&format!(
                "elephc_request_duration_seconds{with_quantile},quantile=\"{quantile}\"}} {:.6}\n",
                value as f64 / 1e9
            ));
        }
    }
    out.push_str(
        "# HELP elephc_queries_per_request Mean database queries per request.\n\
         # TYPE elephc_queries_per_request gauge\n",
    );
    for s in &stats {
        out.push_str(&format!(
            "elephc_queries_per_request{} {:.3}\n",
            labels(s),
            s.queries_per_request
        ));
    }
    out.push_str(
        "# HELP elephc_network_operations_per_request Mean outgoing network operations per request.\n\
         # TYPE elephc_network_operations_per_request gauge\n",
    );
    for s in &stats {
        out.push_str(&format!(
            "elephc_network_operations_per_request{} {:.3}\n",
            labels(s),
            s.network_per_request
        ));
    }
    out.push_str(
        "# HELP elephc_network_wait_seconds_per_request Mean outgoing network wait per request.\n\
         # TYPE elephc_network_wait_seconds_per_request gauge\n",
    );
    for s in &stats {
        out.push_str(&format!(
            "elephc_network_wait_seconds_per_request{} {:.6}\n",
            labels(s),
            s.network_wait_seconds_per_request
        ));
    }
    out
}

/// Exports correlated slices to an OTLP/HTTP collector as OpenTelemetry spans.
///
/// elephc already speaks the identity half of tracing — every slice carries the
/// W3C trace id, span id and parent it was told — so a service already belongs
/// to its caller's trace. What was missing is that the spans never reached a
/// backend, leaving an elephc hop as a gap in someone else's waterfall.
///
/// A slice with no timestamp cannot be placed on a timeline, and OTel requires
/// both ends, so those are skipped and counted rather than exported at epoch 0,
/// which would put every one of them in 1970 and quietly poison the trace.
pub(crate) fn export_otlp(slices: &[Slice], endpoint: &str) -> i32 {
    let mut spans = Vec::new();
    let mut undated = 0usize;
    for slice in slices {
        let Some(trace) = slice.graph.trace.as_ref() else {
            continue;
        };
        let Some(start_us) = trace.start_us else {
            undated += 1;
            continue;
        };
        let start_ns = start_us.saturating_mul(1_000);
        let queries: u64 = slice.graph.nodes.iter().map(|n| n.io_exclusive).sum();
        let wait_ns: u64 = slice.graph.nodes.iter().map(|n| n.wait_exclusive).sum();
        let network_ops: u64 = slice
            .graph
            .nodes
            .iter()
            .map(|node| node.network_exclusive)
            .sum();
        let network_wait_ns: u64 = slice
            .graph
            .nodes
            .iter()
            .map(|node| node.network_wait_exclusive)
            .sum();
        let name = if trace.route.is_empty() {
            slice.service.clone()
        } else {
            trace.route.clone()
        };
        let mut string_attributes = vec![("elephc.service".to_string(), slice.service.clone())];
        if !trace.route.is_empty() {
            string_attributes.push(("http.route".to_string(), trace.route.clone()));
        }
        spans.push(crate::otlp::OtlpSpan {
            service: slice.service.clone(),
            trace_id: trace.trace_id.clone(),
            span_id: trace.span_id.clone(),
            parent_span_id: trace.parent_span_id.clone(),
            name,
            start_unix_nano: start_ns,
            end_unix_nano: start_ns.saturating_add(slice.graph.total),
            attributes: vec![
                ("elephc.functions".to_string(), slice.graph.nodes.len() as i64),
                ("elephc.queries".to_string(), queries as i64),
                ("elephc.wait_ns".to_string(), wait_ns as i64),
                ("elephc.network_operations".to_string(), network_ops as i64),
                ("elephc.network_wait_ns".to_string(), network_wait_ns as i64),
            ],
            string_attributes,
        });
    }
    if undated > 0 {
        eprintln!(
            "elephc monitor: {undated} slice(s) carry no timestamp and were not exported — \
             a span needs both ends of an interval"
        );
    }
    if spans.is_empty() {
        eprintln!("elephc monitor: nothing to export to {endpoint}");
        return 1;
    }
    let body = crate::otlp::encode_traces(&spans);
    match crate::otlp::post_traces(endpoint, &body) {
        Ok(()) => {
            println!("exported {} span(s) to {endpoint}", spans.len());
            0
        }
        Err(error) => {
            eprintln!("elephc monitor: {error}");
            1
        }
    }
}

/// Serializes both views as one Speedscope document.
pub(crate) fn write_speedscope(
    display: &[(Vec<(String, Kind)>, u64)],
    out_path: &str,
) -> Result<(), String> {
    let mut frames: Vec<serde_json::Value> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut frame_index = |name: &str, frames: &mut Vec<serde_json::Value>| -> usize {
        if let Some(&i) = index.get(name) {
            return i;
        }
        let i = frames.len();
        index.insert(name.to_string(), i);
        frames.push(serde_json::json!({ "name": name }));
        i
    };

    let aggregate = |view: &dyn Fn(&[(String, Kind)]) -> Vec<String>| {
        let mut merged: BTreeMap<Vec<String>, u64> = BTreeMap::new();
        for (stack, weight) in display {
            *merged.entry(view(stack)).or_default() += weight;
        }
        merged
    };
    let php_view = aggregate(&|stack: &[(String, Kind)]| {
        let folded: Vec<String> = stack
            .iter()
            .filter(|(_, kind)| matches!(kind, Kind::Php | Kind::PhpInlined))
            .map(|(name, _)| name.clone())
            .collect();
        if folded.is_empty() {
            vec!["<non-PHP>".to_string()]
        } else {
            folded
        }
    });
    let why_view = aggregate(&|stack: &[(String, Kind)]| {
        stack
            .iter()
            .map(|(name, kind)| match kind {
                Kind::Helper => match cause_for(name) {
                    Some(cause) => format!("{name} — {cause}"),
                    None => name.clone(),
                },
                _ => name.clone(),
            })
            .collect()
    });

    let mut profiles = Vec::new();
    for (name, view) in [("PHP (helpers folded)", php_view), ("Why (runtime)", why_view)] {
        let total: u64 = view.values().sum();
        let mut sample_lists = Vec::new();
        let mut weights = Vec::new();
        for (stack, weight) in view {
            sample_lists.push(
                stack
                    .iter()
                    .map(|f| frame_index(f, &mut frames))
                    .collect::<Vec<_>>(),
            );
            weights.push(weight);
        }
        profiles.push(serde_json::json!({
            "type": "sampled",
            "name": name,
            "unit": "none",
            "startValue": 0,
            "endValue": total,
            "samples": sample_lists,
            "weights": weights,
        }));
    }
    let doc = serde_json::json!({
        "$schema": "https://www.speedscope.app/file-format-schema.json",
        "shared": { "frames": frames },
        "profiles": profiles,
        "name": "elephc monitor profile",
    });
    let file =
        std::fs::File::create(out_path).map_err(|e| format!("cannot write {out_path}: {e}"))?;
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer(&mut writer, &doc).map_err(|e| format!("cannot serialize: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("cannot write {out_path}: {e}"))?;
    Ok(())
}

/// Appends a Markdown report to `$GITHUB_STEP_SUMMARY` when running in GitHub
/// Actions: a hot-function table plus a Mermaid pie of the runtime causes.
pub(crate) fn write_github_summary(display: &[(Vec<(String, Kind)>, u64)], processes: usize) {
    let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") else {
        return;
    };
    let stats = table_stats(display);
    let grand = stats.grand;
    let mut md = format!(
        "## elephc monitor\n\n{grand} samples · {processes} process{}\n\n| Function | Total | Self | Top cause |\n|---|---|---|---|\n",
        if processes > 1 { "es" } else { "" }
    );
    let mut by_weight: Vec<_> = stats.totals.iter().collect();
    by_weight.sort_by_key(|(_, weight)| std::cmp::Reverse(**weight));
    for (function, total) in by_weight.iter().take(12) {
        let pct = 100.0 * **total as f64 / grand as f64;
        let self_pct =
            100.0 * stats.selfs.get(*function).copied().unwrap_or(0) as f64 / grand as f64;
        let top_cause = stats
            .causes
            .get(*function)
            .and_then(|map| map.iter().max_by_key(|(_, weight)| **weight))
            .map(|(cause, weight)| {
                format!("{cause} ({:.1}%)", 100.0 * *weight as f64 / grand as f64)
            })
            .unwrap_or_else(|| "—".to_string());
        md.push_str(&format!(
            "| `{function}` | {pct:.1}% | {self_pct:.1}% | {top_cause} |\n"
        ));
    }
    let mut global_causes: BTreeMap<&'static str, u64> = BTreeMap::new();
    for per_function in stats.causes.values() {
        for (cause, weight) in per_function {
            *global_causes.entry(cause).or_default() += weight;
        }
    }
    if !global_causes.is_empty() {
        md.push_str("\n```mermaid\npie title Runtime time by cause\n");
        for (cause, weight) in &global_causes {
            md.push_str(&format!("    \"{cause}\" : {weight}\n"));
        }
        md.push_str("```\n");
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&path) {
        let _ = writeln!(file, "{md}");
    }
}
