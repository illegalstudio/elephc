//! Purpose:
//! Which `monitor --apm` flags apply to a target: a service answers with
//! counters, a launched script runs once.
//!
//! Called from:
//! - The monitor command's dispatch, before anything runs.
//!
//! Key details:
//! - A conflict is reported by flag name, so the error lists them all.

use super::*;

/// Flags that cannot apply to an `--apm` target of this kind.
///
/// A service answers with counters, so everything that shapes, exports or
/// gates a launched run's profile has nothing to act on; a launched script
/// runs once to completion, so the live views and the dashboard server, which
/// follow something running, have nothing to follow.
pub(crate) fn conflicts(cmd: &MonitorCommand, service: bool) -> Vec<&'static str> {
    let flags: Vec<(&'static str, bool)> = if service {
        vec![
            ("--out", cmd.out.is_some()),
            ("--pprof", cmd.pprof_out.is_some()),
            ("--dot", cmd.dot_out.is_some()),
            ("--html", cmd.html_out.is_some()),
            ("--trace", cmd.trace.is_some()),
            ("--save", cmd.save.is_some()),
            ("--baseline", cmd.baseline.is_some()),
            ("--fail-on-regression", cmd.fail_on_regression.is_some()),
            ("--assert", !cmd.asserts.is_empty()),
            ("--assert-file", cmd.assert_file.is_some()),
            ("--prometheus", cmd.prom_out.is_some()),
            ("-- <program arguments>", !cmd.target_args.is_empty()),
        ]
    } else {
        vec![
            ("--live", cmd.live),
            ("--serve", cmd.serve.is_some()),
            ("--prometheus", cmd.prom_out.is_some()),
            ("--otlp", cmd.otlp.is_some()),
            ("--capture-every", cmd.apm_capture_every > 0),
            ("--alert", !cmd.apm_alerts.is_empty()),
            ("--alert-exec", cmd.apm_alert_exec.is_some()),
            // A script's run has no alert window to measure: accepted, it was
            // ignored (review).
            ("--alert-window", cmd.apm_alert_window_explicit),
            ("--reset-routes", cmd.apm_reset_routes),
        ]
    };
    flags.into_iter().filter(|(_, given)| *given).map(|(name, _)| name).collect()
}

/// Flags a one-shot read of a service would accept and then ignore: they act
/// in the collector, which runs only with `--live`, `--store` or `--serve`.
///
/// `monitor <service> --apm --capture-every 5` printed one table and exited 0
/// having captured nothing (review). `--alert` alone is not here: without a
/// collector it checks the budgets once and exits 2 when one is broken.
pub(crate) fn need_a_collector(cmd: &MonitorCommand) -> Vec<&'static str> {
    let collecting = cmd.live || cmd.apm_store.is_some() || cmd.serve.is_some() || cmd.otlp.is_some();
    if collecting || cmd.target.is_empty() {
        return Vec::new();
    }
    [
        ("--capture-every", cmd.apm_capture_every > 0),
        ("--alert-exec", cmd.apm_alert_exec.is_some()),
        ("--alert-window", cmd.apm_alert_window_explicit),
    ]
    .into_iter()
    .filter(|(_, given)| *given)
    .map(|(name, _)| name)
    .collect()
}
