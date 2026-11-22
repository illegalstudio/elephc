//! Purpose:
//! `elephc monitor <service> --apm`: continuous transaction monitoring of a
//! `--web` service started with `--apm`. Reads the per-route counters the
//! service keeps, turns consecutive snapshots into windows, and shows them as a
//! table, a refreshing live view, an on-disk history and a live dashboard.
//!
//! Called from:
//! - `monitor::run`, when `--apm` is given.
//!
//! Key details:
//! - The service only counts; everything with a notion of time lives here. One
//!   poll per second is one handshake and one pass over a fixed table, and a
//!   missed poll only widens the next window: the counters are cumulative.
//! - A changed epoch means the service restarted. Its counters started from
//!   zero, so the new snapshot is itself the window since the restart.
//! - Windows keep their histograms sparse; a dense 256-bucket histogram is
//!   rebuilt only to compute percentiles over whatever span is asked for.
//! - This file dispatches (`run`); the work lives in `window` (the model),
//!   `summary` (the table), `history` (memory and store ranges), `collector`
//!   (the poll loop), `browse` (stored views), `runs` (script runs), `options`
//!   (flag conflicts), `store`, `dashboard`, `terminal`, `alerts` and `traces`.

use std::collections::VecDeque;
use std::io::IsTerminal as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use elephc_probe::apm::{self, RouteStats, Snapshot};
use serde::{Deserialize, Serialize};

use super::{fmt_ns, resolve_probe_key, MonitorCommand};

mod api;
mod browse;
mod captures;
mod collector;
mod dashboard;
mod deploys;
mod history;
mod options;
mod otlp_export;
mod runs;
mod store;
mod summary;
mod terminal;
mod window;
pub(crate) mod alerts;
pub(crate) mod traces;
#[cfg(test)]
mod tests;

pub(crate) use browse::*;
use collector::*;
pub(crate) use history::*;
pub(crate) use options::*;
pub(crate) use runs::*;
pub(crate) use store::Store;
pub(crate) use summary::*;
pub(crate) use window::*;

/// How often the collector reads the service.
const POLL_INTERVAL: Duration = Duration::from_secs(1);
/// Windows kept in memory: one hour at one per poll. Longer ranges come from
/// the store, at its coarser resolution.
const MEMORY_WINDOWS: usize = 3_600;
/// Default store resolution, seconds: one record per route per ten seconds is
/// fine enough to see an incident and small enough to keep weeks on disk.
pub(crate) const DEFAULT_RESOLUTION_SECS: u64 = 10;
/// Default number of days of history the store keeps.
pub(crate) const DEFAULT_RETENTION_DAYS: u32 = 7;
/// Default number of days the store keeps at the collector's resolution; older days are
/// rolled up by the minute.
pub(crate) const DEFAULT_FULL_DAYS: u32 = 2;
/// Default number of days the store keeps at least by the minute; older days are rolled
/// up by the hour.
pub(crate) const DEFAULT_MINUTE_DAYS: u32 = 7;

/// Runs `monitor --apm`; returns the exit code.
pub(crate) fn run(cmd: &MonitorCommand) -> i32 {
    let store = match &cmd.apm_store {
        Some(dir) => match Store::open(dir, cmd.apm_retention_days) {
            Ok(store) => Some(store.with_tiers(cmd.apm_full_days, cmd.apm_minute_days)),
            Err(error) => {
                eprintln!("elephc monitor: {error}");
                return 1;
            }
        },
        None => None,
    };
    // Programs that send their counters here (`--apm-collector`).
    if let Some(address) = &cmd.apm_listen {
        let key = match resolve_probe_key(cmd, address) {
            Ok(key) => key,
            Err(error) => {
                eprintln!("elephc monitor: {error}");
                return 1;
            }
        };
        return collect_pushed(cmd, &key, address, store);
    }
    // No service: replay what a previous collector stored.
    if cmd.target.is_empty() {
        let Some(addr) = &cmd.serve else {
            // No service and no server: the stored history, in the terminal.
            return match &store {
                Some(store) => {
                    print!("{}", browse_store(cmd, store));
                    0
                }
                None => {
                    eprintln!("elephc monitor: --apm needs a service to read, or --store to browse a stored history");
                    2
                }
            };
        };
        let shared: Shared = Arc::new((Mutex::new(History::new("stored history", store, false)), Condvar::new()));
        return match dashboard::serve(addr, shared) {
            Ok((local, handle)) => {
                println!("monitor page (stored history) on http://{local}/ — Ctrl-C to stop");
                let _ = handle.join();
                0
            }
            Err(error) => {
                eprintln!("elephc monitor: cannot serve on {addr}: {error}");
                1
            }
        };
    }
    let key = match resolve_probe_key(cmd, &cmd.target) {
        Ok(key) => key,
        Err(error) => {
            eprintln!("elephc monitor: {error}");
            return 1;
        }
    };
    if cmd.apm_reset_routes {
        return match collector::reset_service_routes(&cmd.target, &key) {
            Ok(answer) => {
                print!("{}: {answer}", cmd.target);
                0
            }
            Err(error) => {
                eprintln!("elephc monitor: {error}");
                1
            }
        };
    }
    let first = match fetch_snapshot(&cmd.target, &key) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!("elephc monitor: {error}");
            return 1;
        }
    };
    println!(
        "connected to probe build {}",
        elephc_probe::handshake::fingerprint(&key)
    );
    let collecting = cmd.live || store.is_some() || cmd.serve.is_some() || cmd.otlp.is_some();
    if !collecting {
        let window = Window::since_start(&first);
        print!(
            "{}",
            render_table("transactions since the service started", &[&window], window.span_ms)
        );
        if let Some(route) = &cmd.apm_route {
            print!("{}", route_histogram(&[&window], route));
        }
        if !cmd.apm_alerts.is_empty() {
            return check_budgets_once(cmd, &window);
        }
        return 0;
    }
    collect(cmd, &key, first, store)
}
