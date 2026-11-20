//! Purpose:
//! The transactions table: per-route summaries (rates, percentiles, error
//! shares, database and network work) over a set of windows, and its text
//! rendering for the terminal.
//!
//! Called from:
//! - The terminal views, the dashboard's JSON and the stored-history browser.
//!
//! Key details:
//! - Rows are ordered by share of total wall time: where the service spends
//!   its time is where an optimization pays.

use super::*;

/// One row of the transactions table, derived from a route's counters.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct RouteSummary {
    pub route: String,
    pub n: u64,
    pub rps: f64,
    pub mean_ns: u64,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    /// Share of requests answered 4xx, percent.
    pub client_error_pct: f64,
    /// Share of requests answered 5xx, percent.
    pub server_error_pct: f64,
    /// Share of all wall time spent in this route, percent.
    pub time_pct: f64,
    /// Database statements per transaction.
    pub queries_per_txn: f64,
    /// Share of this route's wall time spent waiting on the database or the
    /// network, percent.
    pub io_wait_pct: f64,
    /// Mean peak memory of a request, bytes; 0 when none was measured.
    pub mem_mean_bytes: u64,
    /// The 95th percentile of a request's peak memory, as its bucket's upper edge, bytes;
    /// 0 when none was measured.
    pub mem_p95_bytes: u64,
}

/// Summarizes `row` over `span_ms`, with `total_ns` the wall time of every route.
pub(crate) fn summarize(row: &RouteStats, span_ms: u64, total_ns: u64) -> RouteSummary {
    let pct = |part: u64, whole: u64| {
        if whole == 0 {
            0.0
        } else {
            part as f64 * 100.0 / whole as f64
        }
    };
    RouteSummary {
        route: row.name.clone(),
        n: row.count,
        rps: if span_ms == 0 {
            0.0
        } else {
            row.count as f64 * 1_000.0 / span_ms as f64
        },
        mean_ns: row.mean_ns().unwrap_or(0),
        p50_ns: row.percentile_ns(50.0).unwrap_or(0),
        p95_ns: row.percentile_ns(95.0).unwrap_or(0),
        p99_ns: row.percentile_ns(99.0).unwrap_or(0),
        client_error_pct: pct(row.client_errors(), row.count),
        server_error_pct: pct(row.server_errors(), row.count),
        time_pct: pct(row.sum_ns, total_ns),
        queries_per_txn: 0.0,
        io_wait_pct: 0.0,
        mem_mean_bytes: row.mem_mean().unwrap_or(0),
        mem_p95_bytes: row.mem_percentile(95.0).unwrap_or(0),
    }
}

/// Fills a summary's database and network columns from `io`.
pub(super) fn with_io(mut summary: RouteSummary, io: Option<&Io>, sum_ns: u64) -> RouteSummary {
    if let Some(io) = io {
        if summary.n > 0 {
            summary.queries_per_txn = io.db_ops as f64 / summary.n as f64;
        }
        if sum_ns > 0 {
            summary.io_wait_pct = (io.db_wait_ns + io.net_wait_ns) as f64 * 100.0 / sum_ns as f64;
        }
    }
    summary
}

/// Route summaries over `windows`, hottest (most wall time) first, plus the
/// all-routes row.
pub(crate) fn summaries(windows: &[&Window], span_ms: u64) -> (Vec<RouteSummary>, RouteSummary) {
    let rows = merge_routes(windows.iter().copied());
    let io = merge_io(windows.iter().copied());
    let total = total_of(&rows);
    let mut total_io = Io::default();
    for each in io.values() {
        total_io.absorb(each);
    }
    let mut summaries: Vec<RouteSummary> = rows
        .iter()
        .map(|row| with_io(summarize(row, span_ms, total.sum_ns), io.get(&row.name), row.sum_ns))
        .collect();
    summaries.sort_by(|a, b| {
        b.time_pct
            .partial_cmp(&a.time_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.route.cmp(&b.route))
    });
    let total_summary = with_io(summarize(&total, span_ms, total.sum_ns), Some(&total_io), total.sum_ns);
    (summaries, total_summary)
}

/// Renders the transactions table for a span.
pub(crate) fn render_table(title: &str, windows: &[&Window], span_ms: u64) -> String {
    let (rows, total) = summaries(windows, span_ms);
    let mut out = String::new();
    out.push_str(&format!(
        "{title} — {} over {}, {} route{}\n\n",
        plural(total.n, "request"),
        fmt_span(span_ms),
        rows.len(),
        if rows.len() == 1 { "" } else { "s" },
    ));
    if let Some(res) = resources_of(windows) {
        out.push_str(&format!(
            "service: CPU {} · memory {} · {} process{}\n\n",
            match res.cpu_pct() {
                Some(pct) => format!("{pct:.1}% of {} cores ({:.2} busy)", res.cores_available, res.cores),
                None => format!("{:.2} cores", res.cores),
            },
            fmt_bytes(res.memory_bytes),
            res.processes,
            if res.processes == 1 { "" } else { "es" },
        ));
    }
    if rows.is_empty() {
        out.push_str("  no requests completed in this span\n");
        return out;
    }
    let width = rows
        .iter()
        .map(|row| row.route.chars().count())
        .max()
        .unwrap_or(5)
        .clamp(10, 48);
    // Database and network columns only when something recorded them, so a
    // web service's table keeps its width.
    let with_io = rows.iter().any(|row| row.queries_per_txn > 0.0 || row.io_wait_pct > 0.0);
    // A request's peak memory, when a --web service measured it: before the status columns,
    // so the columns counted from either end keep their place.
    let with_mem = rows.iter().any(|row| row.mem_mean_bytes > 0);
    out.push_str(&format!(
        "{:<width$} {:>8} {:>8} {:>9} {:>9} {:>9} {:>9}",
        "route", "req", "req/s", "mean", "p50", "p95", "p99",
    ));
    if with_mem {
        out.push_str(&format!(" {:>10}", "mem/req"));
    }
    out.push_str(&format!(" {:>6} {:>6} {:>6}", "4xx", "5xx", "time"));
    if with_io {
        out.push_str(&format!(" {:>6} {:>6}", "q/req", "wait"));
    }
    out.push('\n');
    for row in rows.iter().chain(std::iter::once(&total)) {
        let name: String = if row.route.chars().count() > width {
            let kept: String = row.route.chars().take(width - 1).collect();
            format!("{kept}…")
        } else {
            row.route.clone()
        };
        out.push_str(&format!(
            "{:<width$} {:>8} {:>8.1} {:>9} {:>9} {:>9} {:>9}",
            name,
            row.n,
            row.rps,
            fmt_ns(row.mean_ns),
            fmt_ns(row.p50_ns),
            fmt_ns(row.p95_ns),
            fmt_ns(row.p99_ns),
        ));
        if with_mem {
            let mem = if row.mem_mean_bytes > 0 { fmt_bytes(row.mem_mean_bytes) } else { "–".to_string() };
            out.push_str(&format!(" {mem:>10}"));
        }
        out.push_str(&format!(" {:>5.1}% {:>5.1}% {:>5.1}%", row.client_error_pct, row.server_error_pct, row.time_pct));
        if with_io {
            out.push_str(&format!(" {:>6.1} {:>5.1}%", row.queries_per_txn, row.io_wait_pct));
        }
        out.push('\n');
    }
    out.push_str(
        "\nwall time from request head to handler return; percentiles are the upper edge of a \
         latency bucket at most 12.5% wide\n",
    );
    out.push_str(&render_errors(windows));
    out
}

/// The most errors the terminal lists.
const ERRORS_SHOWN: usize = 10;

/// The errors failed transactions ended with over a span, most frequent first: count,
/// exception class, where it was thrown, transaction. Empty when there were none.
pub(crate) fn render_errors(windows: &[&Window]) -> String {
    let errors = merge_errors(windows.iter().copied());
    if errors.is_empty() {
        return String::new();
    }
    let total: u64 = errors.iter().map(|error| error.n).sum();
    let mut out = format!("\nerrors of failed transactions — {total} in {} group{}\n", errors.len(), if errors.len() == 1 { "" } else { "s" });
    for error in errors.iter().take(ERRORS_SHOWN) {
        out.push_str(&format!("  {:>6}  {}  at {}  ({})\n", error.n, error.class, error.at, error.route));
    }
    if errors.len() > ERRORS_SHOWN {
        out.push_str(&format!("  … and {} more\n", errors.len() - ERRORS_SHOWN));
    }
    out
}

/// `1 request` / `12 requests`.
pub(super) fn plural(n: u64, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// Memory as an operator reads it, in powers of 1024: `512 KB`, `182.4 MB`,
/// `1.25 GB`.
///
/// The unit follows the ROUNDED figure, so a value just under a unit reads
/// `1.00 GB`, not `1024.0 MB`.
pub(crate) fn fmt_bytes(bytes: u64) -> String {
    let kb = bytes as f64 / 1024.0;
    let mb = kb / 1024.0;
    if kb.round() < 1024.0 {
        format!("{kb:.0} KB")
    } else if (mb * 10.0).round() / 10.0 < 1024.0 {
        format!("{mb:.1} MB")
    } else {
        format!("{:.2} GB", mb / 1024.0)
    }
}

/// A span as an operator reads it: `4.0 s`, `12 min`, `3.5 h`, `2.0 d`.
pub(crate) fn fmt_span(span_ms: u64) -> String {
    let secs = span_ms as f64 / 1_000.0;
    if secs < 120.0 {
        format!("{secs:.1} s")
    } else if secs < 7_200.0 {
        format!("{:.0} min", secs / 60.0)
    } else if secs < 172_800.0 {
        format!("{:.1} h", secs / 3_600.0)
    } else {
        format!("{:.1} d", secs / 86_400.0)
    }
}
