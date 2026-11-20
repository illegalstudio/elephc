//! Purpose:
//! Script runs as transactions: `elephc monitor script.php --apm` records one
//! run, named `CLI <script> [<command>]`, and shows the script's runs.
//!
//! Called from:
//! - The monitor command's local launch path.
//!
//! Key details:
//! - A run carries epoch 0 and no service: it covers no watched time.

use super::*;

/// What a script's transaction names start with (`apm::CLI_PREFIX`).
pub(crate) const CLI_PREFIX: &str = apm::CLI_PREFIX;

/// The transaction a launched script's run counts under: `apm::cli_run_name`,
/// the rule a program built with `--apm-collector` names its own runs by.
pub(crate) fn cli_transaction_name(target: &str, args: &[String]) -> String {
    apm::cli_run_name(target, args)
}

/// Database and network totals of an exact capture.
pub(crate) fn io_of(graph: &crate::call_graph::CallGraph) -> Io {
    let sum = |f: fn(&crate::call_graph::GraphNode) -> u64| graph.nodes.iter().map(f).sum();
    Io {
        db_ops: sum(|node| node.io_exclusive),
        db_wait_ns: sum(|node| node.wait_exclusive),
        net_ops: sum(|node| node.network_exclusive),
        net_wait_ns: sum(|node| node.network_wait_exclusive),
    }
}

/// One run of a script as a transaction window.
///
/// A run that exits 0 counts in the 2xx class and any other end — a non-zero
/// exit, a signal — in the 5xx class, so failed runs read as errors wherever
/// routes' errors do. The wall time is the process's own, measured by the
/// launcher, so it holds even when the run died before writing a profile. The
/// exact `exit_code` is kept too (a signal as 128 + its number, as a shell says it).
pub(crate) fn cli_run_window(name: &str, exit_code: i32, wall_ns: u64, io: Io, ended_ms: u64) -> Window {
    let succeeded = exit_code == 0;
    let mut stats = RouteStats::empty(name);
    stats.count = 1;
    stats.status[if succeeded { 1 } else { 4 }] = 1;
    stats.sum_ns = wall_ns;
    stats.hist[apm::bucket_of(wall_ns)] = 1;
    Window {
        build: String::new(),
        t: ended_ms,
        span_ms: wall_ns / 1_000_000,
        epoch: 0,
        source: String::new(),
        routes: vec![RouteWindow { io, exits: vec![(exit_code, 1)], ..RouteWindow::from_stats(&stats) }],
        // A run is not a stretch of the service's time: no service resources.
        res: None,
        errors: Vec::new(),
    }
}

/// Records a launched script's run and renders its history.
///
/// Called by the exact path when `--apm` is given, after the run, whatever
/// its outcome. With `--store`, the run joins the stored history and the
/// table covers every stored run of the same transaction; without it, the
/// table is this run alone.
pub(crate) fn record_cli_run(
    cmd: &MonitorCommand,
    exit_code: i32,
    wall_ns: u64,
    io: Io,
    status_label: &str,
    graph: Option<&crate::call_graph::CallGraph>,
) -> Result<(String, Window), String> {
    let name = cli_transaction_name(&cmd.target, &cmd.target_args);
    let now = store::now_ms();
    let succeeded = exit_code == 0;
    let window = cli_run_window(&name, exit_code, wall_ns, io, now);
    let history = match &cmd.apm_store {
        Some(dir) => {
            let store = Store::open(dir, cmd.apm_retention_days)?;
            store.append(&window)?;
            // The run's exact profile, dated beside it: the page finds a run's
            // profile by its period, as it finds a captured request's.
            if let Some(graph) = graph {
                let record = super::captures::CaptureRecord {
                    id: super::captures::next_capture_id(now),
                    source: String::new(),
                    t: now,
                    kind: "run".to_string(),
                    route: name.clone(),
                    status: Some(if succeeded { 200 } else { 500 }),
                    duration_ns: wall_ns,
                    functions: graph.nodes.len(),
                    queries: graph.queries.iter().map(|(_, n)| n).sum(),
                };
                store.append_capture(&record, graph)?;
            }
            // A store fed only by cron runs has no collector to keep it in
            // bounds, so a run prunes it — but only when told how long to keep:
            // the default must not cut a collector's longer retention.
            if cmd.apm_retention_explicit {
                store.prune_now();
            }
            // The runs only: a run's history has nothing to do with the
            // services sharing the store, and loading their windows over the
            // whole retention on every cron run cost more than the run (review).
            let from = now.saturating_sub(u64::from(store.retention_days()) * 86_400_000);
            store
                .runs_in(from, now)
                .into_iter()
                .filter_map(|mut stored| {
                    stored.routes.retain(|route| route.route == name);
                    (!stored.routes.is_empty()).then_some(stored)
                })
                .collect()
        }
        None => vec![window.clone()],
    };
    let refs: Vec<&Window> = history.iter().collect();
    let runs = merge_routes(refs.iter().copied()).iter().find(|row| row.name == name).map_or(0, |row| row.count);
    // A distribution says something once there are a few runs to distribute.
    let histogram = if cmd.apm_route.as_deref() == Some(name.as_str()) || runs >= 5 {
        route_histogram(&refs, &name)
    } else {
        String::new()
    };
    Ok((
        format!(
            "\nthis run: {}, {status_label}\n{}{}{}",
            fmt_ns(wall_ns),
            render_runs(&name, &history),
            terminal::render_run_trend(&history, &name),
            histogram,
        ),
        window,
    ))
}

/// The terminal table of one script's runs: how many, how long, how often
/// they failed, and what they spent waiting on.
pub(crate) fn render_runs(name: &str, runs: &[Window]) -> String {
    let rows = merge_routes(runs.iter());
    let io = merge_io(runs.iter());
    let Some(row) = rows.iter().find(|row| row.name == name) else {
        return format!("{name}: no recorded runs\n");
    };
    let summary = with_io(summarize(row, 0, row.sum_ns), io.get(name), row.sum_ns);
    let first = runs.iter().map(|w| w.t.saturating_sub(w.span_ms)).min().unwrap_or(0);
    let last = runs.iter().map(|w| w.t).max().unwrap_or(0);
    let mut out = format!(
        "\n{name} — {}{}\n\n",
        plural(row.count, "run"),
        if row.count > 1 {
            format!(" over {}", fmt_span(last.saturating_sub(first)))
        } else {
            String::new()
        },
    );
    out.push_str(&format!(
        "{:>6} {:>9} {:>9} {:>9} {:>9} {:>7} {:>6} {:>6}\n",
        "runs", "mean", "p50", "p95", "p99", "failed", "q/run", "wait",
    ));
    out.push_str(&format!(
        "{:>6} {:>9} {:>9} {:>9} {:>9} {:>6.1}% {:>6.1} {:>5.1}%\n",
        row.count,
        fmt_ns(summary.mean_ns),
        fmt_ns(summary.p50_ns),
        fmt_ns(summary.p95_ns),
        fmt_ns(summary.p99_ns),
        summary.server_error_pct,
        summary.queries_per_txn,
        summary.io_wait_pct,
    ));
    out
}
