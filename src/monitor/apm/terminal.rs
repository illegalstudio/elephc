//! Purpose:
//! The terminal views of `monitor --apm` beyond the table: per-route trends as
//! sparklines, and the latency distribution of one route as a histogram — so
//! an operator on an SSH session sees the shape of the traffic, not only its
//! totals.
//!
//! Called from:
//! - `apm::run` / `apm::collect` (services: one-shot with a store, `--live`).
//! - `apm::record_cli_run` (a script's runs).
//!
//! Key details:
//! - Plain text: Unicode block characters, no colour and no cursor control, so
//!   the output is the same in a log file as on a terminal.
//! - A slot with no window at all is drawn as a space, not as zero: the
//!   collector was not looking, which is not the same as no traffic.

use elephc_probe::apm::{self, RouteStats};

use super::{fmt_span, merge_routes, Window};
use crate::monitor::fmt_ns;

/// Characters of a sparkline, lowest to highest.
const TICKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
/// Slots in a trend.
pub(crate) const TREND_SLOTS: usize = 24;
/// Widest bar of the histogram, in characters.
const HISTOGRAM_WIDTH: usize = 40;

/// A sparkline of `values`, scaled to their own maximum; `None` is a gap.
pub(crate) fn sparkline(values: &[Option<f64>]) -> String {
    let max = values.iter().flatten().copied().fold(0.0_f64, f64::max);
    values
        .iter()
        .map(|value| match value {
            None => ' ',
            Some(_) if max <= 0.0 => TICKS[0],
            Some(value) => {
                let index = ((value / max) * (TICKS.len() - 1) as f64).round() as usize;
                TICKS[index.min(TICKS.len() - 1)]
            }
        })
        .collect()
}

/// Throughput and p95 of `route` in `slots` equal stretches of the span the
/// windows cover; a stretch no window ended in is `None`.
pub(crate) fn trend(windows: &[&Window], route: &str, slots: usize) -> (Vec<Option<f64>>, Vec<Option<f64>>) {
    let start = windows.iter().map(|w| w.t.saturating_sub(w.span_ms)).min().unwrap_or(0);
    let end = windows.iter().map(|w| w.t).max().unwrap_or(0);
    let width = (end.saturating_sub(start)).max(1);
    let mut grouped: Vec<Option<(RouteStats, Vec<&Window>)>> = vec![None; slots];
    for window in windows {
        let offset = window.t.saturating_sub(start).saturating_sub(1);
        let slot = ((offset as u128 * slots as u128) / width as u128).min(slots as u128 - 1) as usize;
        let entry = grouped[slot].get_or_insert_with(|| (RouteStats::empty(route), Vec::new()));
        entry.1.push(*window);
        for row in merge_routes(std::iter::once(*window)).iter().filter(|row| row.name == route) {
            entry.0.absorb(row);
        }
    }
    // A slot's rate is over the time its windows cover, counted once, as the
    // table measures it: two services watched over the same seconds did not
    // watch twice as long.
    let rates = grouped
        .iter()
        .map(|slot| {
            slot.as_ref().map(|(row, covered)| match super::measured_ms(covered) {
                0 => 0.0,
                span => row.count as f64 * 1_000.0 / span as f64,
            })
        })
        .collect();
    let p95 = grouped
        .iter()
        .map(|slot| slot.as_ref().map(|(row, _)| row.percentile_ns(95.0).unwrap_or(0) as f64))
        .collect();
    (rates, p95)
}

/// One trend line per route: throughput and p95 sparklines over the span.
pub(crate) fn render_trends(windows: &[&Window], routes: &[String]) -> String {
    if windows.len() < 2 || routes.is_empty() {
        return String::new();
    }
    let start = windows.iter().map(|w| w.t.saturating_sub(w.span_ms)).min().unwrap_or(0);
    let end = windows.iter().map(|w| w.t).max().unwrap_or(0);
    let width = routes.iter().map(|r| r.chars().count()).max().unwrap_or(5).clamp(10, 40);
    let mut out = format!("\ntrends over the last {} (oldest → newest)\n\n", fmt_span(end.saturating_sub(start)));
    out.push_str(&format!(
        "{:<width$}  {:<slots$}  {:>9}  {:<slots$}  {:>9}\n",
        "route", "req/s", "peak", "p95", "worst",
        slots = TREND_SLOTS.min(windows.len()),
    ));
    // No more slots than windows: a collector that has just started has a few
    // seconds of windows, and spreading them over more slots draws blanks the
    // collector did see.
    let slots = TREND_SLOTS.min(windows.len());
    for route in routes {
        let (rates, p95) = trend(windows, route, slots);
        let peak = rates.iter().flatten().copied().fold(0.0_f64, f64::max);
        let worst = p95.iter().flatten().copied().fold(0.0_f64, f64::max);
        let name: String = if route.chars().count() > width {
            let kept: String = route.chars().take(width - 1).collect();
            format!("{kept}…")
        } else {
            route.clone()
        };
        out.push_str(&format!(
            "{:<width$}  {}  {:>9.1}  {}  {:>9}\n",
            name,
            sparkline(&rates),
            peak,
            sparkline(&p95),
            fmt_ns(worst as u64),
        ));
    }
    out
}

/// The latency distribution of one route, one row per power of two.
///
/// Rows are octaves of the fine histogram: coarse enough to read at a glance,
/// and each row's range is exact — every request counted in it took at least
/// the lower bound and less than the upper one.
pub(crate) fn render_histogram(route: &RouteStats) -> String {
    let total: u64 = route.hist.iter().sum();
    if total == 0 {
        return format!("\n{}: no requests to chart\n", route.name);
    }
    // Group the fine buckets by the octave of their lower edge.
    let mut octaves: Vec<(u64, u64, u64)> = Vec::new();
    for (bucket, n) in route.hist.iter().enumerate() {
        let lower = apm::bucket_lower_ns(bucket);
        let octave_lower = if lower < 1_000 { 0 } else { 1_000u64 << (63 - (lower / 1_000).leading_zeros()) };
        let octave_upper = if octave_lower == 0 { 1_000 } else { octave_lower * 2 };
        match octaves.last_mut() {
            Some((low, _, count)) if *low == octave_lower => *count += n,
            _ => octaves.push((octave_lower, octave_upper, *n)),
        }
    }
    let first = octaves.iter().position(|(_, _, n)| *n > 0).unwrap_or(0);
    let last = octaves.iter().rposition(|(_, _, n)| *n > 0).unwrap_or(0);
    let rows = &octaves[first..=last];
    let max = rows.iter().map(|(_, _, n)| *n).max().unwrap_or(1).max(1);
    let noun = if route.name.starts_with(super::CLI_PREFIX) { "run" } else { "request" };
    let mut out = format!("\nlatency of {} — {} {noun}{}\n\n", route.name, total, if total == 1 { "" } else { "s" });
    for (lower, upper, n) in rows {
        let bar = ((*n as f64 / max as f64) * HISTOGRAM_WIDTH as f64).round() as usize;
        let bar = if *n > 0 { bar.max(1) } else { 0 };
        out.push_str(&format!(
            "{:>9} – {:<9} {:<HISTOGRAM_WIDTH$} {:>7} {:>5.1}%\n",
            fmt_ns(*lower),
            fmt_ns(*upper),
            "█".repeat(bar),
            n,
            *n as f64 * 100.0 / total as f64,
        ));
    }
    out
}

/// Run durations, oldest first, as a sparkline with failed runs marked under it.
pub(crate) fn render_run_trend(runs: &[Window], name: &str) -> String {
    // A stored window can hold several runs: the store folds a minute's runs
    // into one record. Each point is then that minute's mean run, so sixty
    // fast runs in a minute do not read as one slow one (review).
    let mut points: Vec<(u64, u64, u64, bool)> = runs
        .iter()
        .flat_map(|w| {
            w.routes
                .iter()
                .filter(|r| r.route == name && r.n > 0)
                .map(move |r| (w.t, r.sum_ns, r.n, r.status[4] > 0))
        })
        .collect();
    if points.len() < 2 {
        return String::new();
    }
    points.sort_by_key(|(t, ..)| *t);
    let recent = &points[points.len().saturating_sub(48)..];
    let durations: Vec<Option<f64>> = recent.iter().map(|(_, ns, n, _)| Some(*ns as f64 / *n as f64)).collect();
    let failures: String = recent.iter().map(|(.., failed)| if *failed { '✗' } else { ' ' }).collect();
    let total: u64 = recent.iter().map(|(_, _, n, _)| n).sum();
    let heading = if total == recent.len() as u64 {
        format!("last {total} runs (oldest → newest)")
    } else {
        format!("last {total} runs over {} minutes, mean run per minute (oldest → newest)", recent.len())
    };
    let mut out = format!("\n{heading}\n  {}\n", sparkline(&durations));
    if failures.contains('✗') {
        out.push_str(&format!("  {failures}  ✗ failed\n"));
    }
    out
}

/// The captured slow and failed requests under the live table.
pub(crate) fn render_traces(traces: &[&super::traces::Trace]) -> String {
    if traces.is_empty() {
        return "\ncaptured requests: none yet\n".to_string();
    }
    let mut out = format!("\ncaptured requests — {} kept, slowest first\n\n", traces.len());
    for trace in traces.iter().take(8) {
        let status = trace.status.map_or("-".to_string(), |status| status.to_string());
        out.push_str(&format!(
            "  {:>9}  {:>4}  {}{}\n",
            fmt_ns(trace.duration_ns),
            status,
            trace.route,
            if trace.failed() { "  ✗ failed" } else { "" },
        ));
    }
    out
}

/// Every budget and where it stands, broken ones first.
pub(crate) fn render_alerts(state: &super::alerts::Alerts, window_secs: u64) -> String {
    let mut rows: Vec<&super::alerts::Alert> = state.alerts.iter().collect();
    rows.sort_by_key(|alert| !alert.firing);
    let broken = rows.iter().filter(|alert| alert.firing).count();
    let mut out = format!(
        "\nbudgets — {} of {} broken over the last {}\n\n",
        broken,
        rows.len(),
        fmt_span(window_secs * 1_000),
    );
    for alert in rows {
        let value = alert
            .value
            .map_or("no traffic".to_string(), |value| format!("{value:.2}{}", alert.rule.metric.unit()));
        out.push_str(&format!(
            "  {}  {:<40} {}\n",
            if alert.firing { "✗ broken" } else if alert.value.is_some() { "✓ ok    " } else { "· idle  " },
            alert.rule.spec,
            value,
        ));
    }
    out
}
