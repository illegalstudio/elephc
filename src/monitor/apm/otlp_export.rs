//! Purpose:
//! `monitor --apm --otlp <endpoint>`: the collector pushes what it measured to an
//! OpenTelemetry collector as OTLP/HTTP metrics, one batch per stored record, so the
//! transactions reach the dashboards a team already runs, beside `/metrics` for Prometheus.
//!
//! Called from:
//! - `collector::Collector::start` (`start`), `collector::write_record` (`send`) and
//!   `Collector::finish` (`flush`).
//!
//! Key details:
//! - Delta temporality: each batch counts its record's interval only, as the store does.
//! - Per transaction (`elephc.transaction` attribute): requests by status class, a latency
//!   histogram on OpenTelemetry's HTTP bounds, database statements, peak memory per
//!   request, and the errors by exception class and place.
//! - Sent from a thread of its own, so an endpoint that is slow or down never holds up the
//!   poll; a failure is reported once a minute at most, and the batch is dropped. The
//!   queue to it is bounded: while the endpoint is stuck, the newest batches are dropped
//!   and counted, not held in memory.
//!

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::{Mutex, OnceLock};

use crate::otlp::{OtlpMetric, OtlpMetrics, OtlpValue};

use super::*;

/// Latency bounds, seconds: OpenTelemetry's for `http.server.request.duration`.
const DURATION_BOUNDS_S: [f64; 14] =
    [0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0];

/// The sender to the export thread, once started.
static SENDER: OnceLock<Mutex<SyncSender<Window>>> = OnceLock::new();
/// Batches waiting for the export thread at most: about ten minutes of records.
const QUEUED: usize = 64;
/// Batches dropped, a full queue's or a failed post's, since the last report.
static DROPPED: AtomicU64 = AtomicU64::new(0);
/// Records handed to the thread and not yet posted.
static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
/// When a failure was last reported, Unix milliseconds.
static LAST_REPORT_MS: AtomicU64 = AtomicU64::new(0);

/// Starts exporting to `endpoint`; `service` names windows that carry no source.
pub(super) fn start(endpoint: &str, service: &str) {
    let (sender, receiver) = std::sync::mpsc::sync_channel::<Window>(QUEUED);
    if SENDER.set(Mutex::new(sender)).is_err() {
        return;
    }
    let shown = endpoint.to_string();
    let endpoint = endpoint.to_string();
    let service = service.to_string();
    std::thread::spawn(move || {
        while let Ok(record) = receiver.recv() {
            let body = crate::otlp::encode_metrics(&[metrics_of(&record, &service)]);
            if let Err(error) = crate::otlp::post_metrics(&endpoint, &body) {
                DROPPED.fetch_add(1, Ordering::Relaxed);
                report(&error);
            }
            IN_FLIGHT.fetch_sub(1, Ordering::Release);
        }
    });
    println!("exporting transactions to {shown} as OTLP metrics");
}

/// Hands one stored record to the export thread, when exporting.
pub(super) fn send(record: &Window) {
    let Some(sender) = SENDER.get() else {
        return;
    };
    IN_FLIGHT.fetch_add(1, Ordering::Acquire);
    match sender.lock().unwrap_or_else(|e| e.into_inner()).try_send(record.clone()) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            IN_FLIGHT.fetch_sub(1, Ordering::Release);
            DROPPED.fetch_add(1, Ordering::Relaxed);
            report("the endpoint is not keeping up");
        }
        Err(TrySendError::Disconnected(_)) => {
            IN_FLIGHT.fetch_sub(1, Ordering::Release);
        }
    }
}

/// Reports an export failure, once a minute at most, with the batches dropped since
/// the last report. The time moves only when a report is printed: a failure every ten
/// seconds must still be reported every minute, not once (review).
fn report(error: &str) {
    let now = store::now_ms();
    let last = LAST_REPORT_MS.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 60_000 {
        return;
    }
    if LAST_REPORT_MS.compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
        let dropped = DROPPED.swap(0, Ordering::Relaxed);
        eprintln!("elephc monitor: OTLP export: {error} ({dropped} batch{} dropped)", if dropped == 1 { "" } else { "es" });
    }
}

/// Waits, five seconds at most, for the records handed over to be posted: the collector
/// is about to exit.
pub(super) fn flush() {
    let deadline = Instant::now() + Duration::from_secs(5);
    while IN_FLIGHT.load(Ordering::Acquire) > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The metrics of one record: its interval, and per transaction what it counted.
pub(crate) fn metrics_of(record: &Window, fallback_service: &str) -> OtlpMetrics {
    let attrs = |route: &str| vec![("elephc.transaction".to_string(), route.to_string())];
    let mut requests = Vec::new();
    let mut durations = Vec::new();
    let mut statements = Vec::new();
    let mut memory = Vec::new();
    for route in &record.routes {
        for (class, n) in route.status.iter().enumerate() {
            if *n > 0 {
                let mut labels = attrs(&route.route);
                labels.push(("http.response.status_class".to_string(), format!("{}xx", class + 1)));
                requests.push((labels, OtlpValue::Count(*n)));
            }
        }
        if !route.hist.is_empty() {
            let mut counts = vec![0u64; DURATION_BOUNDS_S.len() + 1];
            for (bucket, n) in &route.hist {
                let upper_s = apm::bucket_upper_ns(usize::from(*bucket)) as f64 / 1e9;
                let at = DURATION_BOUNDS_S.iter().position(|bound| upper_s <= *bound).unwrap_or(DURATION_BOUNDS_S.len());
                counts[at] += n;
            }
            durations.push((
                attrs(&route.route),
                OtlpValue::Histogram { bounds: DURATION_BOUNDS_S.to_vec(), counts, sum: route.sum_ns as f64 / 1e9 },
            ));
        }
        if route.io.db_ops > 0 {
            statements.push((attrs(&route.route), OtlpValue::Count(route.io.db_ops)));
        }
        if !route.mem.is_empty() {
            let bounds: Vec<f64> = (0..apm::MEM_BUCKETS - 1).map(|bucket| apm::mem_bucket_upper(bucket) as f64).collect();
            let mut counts = vec![0u64; apm::MEM_BUCKETS];
            for (bucket, n) in &route.mem.hist {
                if let Some(slot) = counts.get_mut(usize::from(*bucket)) {
                    *slot += n;
                }
            }
            memory.push((attrs(&route.route), OtlpValue::Histogram { bounds, counts, sum: route.mem.sum as f64 }));
        }
    }
    let errors = record
        .errors
        .iter()
        .map(|error| {
            let mut labels = attrs(&error.route);
            labels.push(("exception.type".to_string(), error.class.clone()));
            labels.push(("code.location".to_string(), error.at.clone()));
            (labels, OtlpValue::Count(error.n))
        })
        .collect();
    let metric = |name: &str, unit: &str, description: &str, points| OtlpMetric {
        name: name.to_string(),
        unit: unit.to_string(),
        description: description.to_string(),
        points,
    };
    OtlpMetrics {
        service: if record.source.is_empty() { fallback_service.to_string() } else { record.source.clone() },
        start_unix_nano: record.t.saturating_sub(record.span_ms).saturating_mul(1_000_000),
        end_unix_nano: record.t.saturating_mul(1_000_000),
        metrics: vec![
            metric("elephc.transactions", "{transaction}", "Transactions completed, by status class", requests),
            metric("elephc.transaction.duration", "s", "Wall time of a transaction", durations),
            metric("elephc.db.statements", "{statement}", "Database statements executed", statements),
            metric("elephc.transaction.memory", "By", "Peak memory of a request", memory),
            metric("elephc.errors", "{error}", "Failed transactions, by the exception they ended with", errors),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record becomes one batch for its source over its interval: requests by status
    /// class, a latency histogram holding every request, statements, memory and errors.
    #[test]
    fn a_record_becomes_delta_metrics_per_transaction() {
        let record = Window {
            build: String::new(),
            t: 10_000,
            span_ms: 10_000,
            epoch: 1,
            source: "shop:9411".into(),
            routes: vec![RouteWindow {
                route: "GET /pay".into(),
                n: 3,
                status: [0, 2, 0, 0, 1],
                sum_ns: 30_000_000,
                hist: vec![(60, 2), (200, 1)],
                io: Io { db_ops: 6, ..Io::default() },
                mem: Mem { n: 3, sum: 3 << 20, hist: vec![(4, 3)] },
                exits: Vec::new(),
            }],
            res: None,
            errors: vec![WindowError { route: "GET /pay".into(), class: "App\\Declined".into(), at: "Pay.php:4".into(), n: 1, trace: String::new() }],
        };
        let batch = metrics_of(&record, "fallback");
        assert_eq!(batch.service, "shop:9411");
        assert_eq!((batch.start_unix_nano, batch.end_unix_nano), (0, 10_000_000_000));
        let points = |name: &str| batch.metrics.iter().find(|m| m.name == name).map_or(0, |m| m.points.len());
        assert_eq!(points("elephc.transactions"), 2, "2xx and 5xx");
        assert_eq!((points("elephc.transaction.duration"), points("elephc.db.statements")), (1, 1));
        assert_eq!((points("elephc.transaction.memory"), points("elephc.errors")), (1, 1));
        let duration = batch.metrics.iter().find(|m| m.name == "elephc.transaction.duration").unwrap();
        let OtlpValue::Histogram { counts, bounds, .. } = &duration.points[0].1 else { panic!("a histogram") };
        assert_eq!((counts.iter().sum::<u64>(), counts.len()), (3, bounds.len() + 1), "every request in a bucket");
        let body = crate::otlp::encode_metrics(&[batch]);
        let text = String::from_utf8_lossy(&body);
        for name in ["service.name", "shop:9411", "elephc.transactions", "elephc.errors", "App\\Declined", "GET /pay"] {
            assert!(text.contains(name), "{name} is encoded");
        }
    }
}
