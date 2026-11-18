//! Purpose:
//! Views over stored history: the terminal browser of `--apm --store <dir>`
//! and the transactions panel exported into a static monitor page.
//!
//! Called from:
//! - `apm::run` (browsing) and the monitor command's page export.
//!
//! Key details:
//! - Several services in one store are labelled `service · route`.

use super::*;

/// The stored history in the terminal: the table, the trends and, with
/// `--route`, one route's latency histogram, over `--duration` seconds when
/// given and the last hour otherwise.
pub(super) fn browse_store(cmd: &MonitorCommand, store: &Store) -> String {
    let Some(newest) = store.newest_ms() else {
        return format!("{} holds no stored windows yet\n", store.dir().display());
    };
    let span_ms = if cmd.duration_explicit { u64::from(cmd.duration_secs) * 1_000 } else { 3_600_000 };
    let mut windows = store.load(newest.saturating_sub(span_ms), newest);
    label_sources(&mut windows);
    let refs: Vec<&Window> = windows.iter().collect();
    let measured = measured_ms(&refs);
    let mut out = render_table(&format!("stored history in {}", store.dir().display()), &refs, measured);
    out.push_str(&terminal::render_trends(&refs, &hottest_routes(&refs)));
    if let Some(route) = &cmd.apm_route {
        out.push_str(&route_histogram(&refs, route));
    }
    out
}

/// Up to ten routes, most wall time first: the ones a trend view is for.
pub(super) fn hottest_routes(windows: &[&Window]) -> Vec<String> {
    hottest_routes_n(windows, 10)
}

/// Up to `n` routes, most wall time first.
fn hottest_routes_n(windows: &[&Window], n: usize) -> Vec<String> {
    let mut rows = merge_routes(windows.iter().copied());
    rows.sort_by(|a, b| b.sum_ns.cmp(&a.sum_ns).then_with(|| a.name.cmp(&b.name)));
    rows.into_iter().take(n).map(|row| row.name).collect()
}

/// The latency histogram of `route` over `windows`, or a note naming the
/// routes that exist when it is not one of them.
pub(super) fn route_histogram(windows: &[&Window], route: &str) -> String {
    let rows = merge_routes(windows.iter().copied());
    match rows.iter().find(|row| row.name == route) {
        Some(row) => terminal::render_histogram(row),
        None => format!(
            "\nno route named `{route}` in this span; routes: {}\n",
            rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// The Transactions view of an exported page: every transaction in the store
/// (or this run alone without one), embedded so the file opens anywhere.
///
/// One span — from the oldest stored window to now — with the series for all
/// transactions and for each of the busiest twenty, which is what the page's
/// route filter switches between.
pub(crate) fn static_view(cmd: &MonitorCommand, this_run: Option<&Window>) -> crate::call_graph::ApmView {
    let now = store::now_ms();
    let mut windows: Vec<Window> = match &cmd.apm_store {
        Some(dir) => Store::open(dir, cmd.apm_retention_days)
            .map(|store| store.load(now.saturating_sub(u64::from(store.retention_days()) * 86_400_000), now))
            .unwrap_or_default(),
        None => Vec::new(),
    };
    if windows.is_empty() {
        windows.extend(this_run.cloned());
    }
    label_sources(&mut windows);
    let from = windows.iter().map(|w| w.t.saturating_sub(w.span_ms.max(1_000))).min().unwrap_or(now);
    let to = windows.iter().map(|w| w.t).max().unwrap_or(now).max(from + 1_000);
    let all = api::build_series(&windows, from, to, None, &cmd.target, false, None);
    let refs: Vec<&Window> = windows.iter().collect();
    let mut routes = serde_json::Map::new();
    for name in hottest_routes_n(&refs, 20) {
        let series = api::build_series(&windows, from, to, Some(&name), &cmd.target, false, None);
        routes.insert(name, series);
    }
    crate::call_graph::ApmView {
        data: serde_json::json!({
            "mode": "static",
            "target": cmd.target,
            "capture": false,
            "all": all,
            "routes": routes,
        }),
    }
}
