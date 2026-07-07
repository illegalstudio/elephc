//! Purpose:
//! Per-worker JSON metrics snapshot for the `--web` server's opt-in `--metrics`
//! endpoint. Owns the process-static counters (latency ring, RPS buckets,
//! status-class tallies, active-connection mirror, h2 stream gauge, handler
//! inflight depth) plus the `mark_started`/`mark_mode` one-shot setters, and
//! hand-rolls the `/_status` JSON response (no serde / no new deps).
//!
//! Called from:
//! - `crate::worker::serve` and `crate::worker_mode::enter_worker_loop`: call
//!   `mark_started` / `mark_mode` at loop entry, then the `service_fn` closure
//!   calls `record_request` at every return point and `snapshot_response` at the
//!   `/_status` intercept.
//! - `crate::worker::ActiveConnGuard` mirrors the active-connection count into
//!   `ACTIVE_CONNS` so the snapshot reports it without touching T1#2's drain
//!   `Rc<AtomicUsize>`.
//!
//! Key details:
//! - ALL recording happens single-threaded on the worker's I/O thread (the
//!   `service_fn` `async move` block); with `--handler-offload` the handler
//!   runs on a separate `php-handler` thread but the service_fn awaits the
//!   reply on the I/O thread and records AFTER `reply_rx.await` resolves. So
//!   every static uses `Ordering::Relaxed` (single writer; the `/_status` read
//!   happens on the same I/O thread too). No locks, no CAS.
//! - The snapshot is per-worker: under SO_REUSEPORT a request lands on a random
//!   worker, so the operator scrapes repeatedly for a cluster view (documented
//!   in `docs/beyond-php/web.md`).
//! - OFF path (`--metrics` absent): the service_fn allocates nothing new — the
//!   intercept is a single `if metrics && path == metrics_path` check that
//!   short-circuits on the `metrics: bool` captured by the closure.
//! - JSON is hand-rolled with `write!` into a `String` (small fixed shape; only
//!   `mode` and `metrics_path` are string fields and both are ASCII literals we
//!   control, so no escaping is needed).

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use http_body_util::Full;
use hyper::body::Bytes;
use hyper::Response;

/// Ring-buffer size for the latency sample window (last N request latencies).
const LATENCY_RING_SIZE: usize = 256;

/// Sliding-window bucket count for `rps_last_60s` (one bucket per second).
const RPS_BUCKETS: usize = 60;

/// Active connection count mirror (the metrics counterpart of T1#2's drain
/// `Rc<AtomicUsize>`). Incremented/decremented ONLY by `ActiveConnGuard` so
/// both `serve` and `enter_worker_loop` are covered by changing the guard;
/// the accept arms and `wait_for_drain_forever` are untouched (T1#2
/// byte-for-byte). `Relaxed`: single I/O thread writer + reader.
pub(crate) static ACTIVE_CONNS: AtomicUsize = AtomicUsize::new(0);

/// Ring buffer of the last `LATENCY_RING_SIZE` request latencies in
/// microseconds. Advanced by `LATENCY_IDX`. `Relaxed` (single I/O thread).
static LATENCY_RING: [AtomicU64; LATENCY_RING_SIZE] = [const { AtomicU64::new(0) }; LATENCY_RING_SIZE];

/// Next write index into `LATENCY_RING` (modulo `LATENCY_RING_SIZE`). `Relaxed`.
static LATENCY_IDX: AtomicUsize = AtomicUsize::new(0);

/// Total number of latency samples ever recorded (capped at `LATENCY_RING_SIZE`
/// for the `samples` field — once the ring is full, `samples` stays at
/// `LATENCY_RING_SIZE`). Incremented per recorded request. `Relaxed`.
static LATENCY_SAMPLES: AtomicU64 = AtomicU64::new(0);

/// 60-second sliding-window RPS buckets. `RPS_SEC[i]` holds the
/// `uptime_secs % 60` that bucket `i` currently represents, or `u32::MAX` if
/// never written. `RPS_COUNT[i]` holds that bucket's count. `Relaxed`.
static RPS_SEC: [AtomicU32; RPS_BUCKETS] = [const { AtomicU32::new(u32::MAX) }; RPS_BUCKETS];

/// Per-second request count for the bucket whose second is in `RPS_SEC[i]`.
/// Reset to `1` when a new second lands in bucket `i`. `Relaxed`.
static RPS_COUNT: [AtomicU64; RPS_BUCKETS] = [const { AtomicU64::new(0) }; RPS_BUCKETS];

/// Status-class tallies, indexed `status / 100` (1..=5; index 0 unused).
/// `Relaxed`.
static STATUS_CLASSES: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];

/// Total `503` responses (a subset of `STATUS_CLASSES[5]`), reported separately
/// as the overload signal. `Relaxed`.
static OVERLOAD_503: AtomicU64 = AtomicU64::new(0);

/// Jobs sent to the `php-handler` thread but not yet replied (queue depth +
/// the one executing). Incremented in the offload arm right after a successful
/// `tx.try_send(job)`; decremented when `reply_rx` resolves (Ok or Err). Only
/// touched when `handler_offload` is on; zero otherwise. `Relaxed`.
pub(crate) static HANDLER_INFLIGHT: AtomicUsize = AtomicUsize::new(0);

/// Active h2 streams (requests in flight on h2 connections). Incremented at
/// service_fn entry when `is_h2`; decremented on exit via `H2StreamGuard`.
/// `Relaxed`.
static H2_STREAMS_ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// Worker start time, set ONCE at the top of `serve()` / `enter_worker_loop()`
/// via `mark_started()`. Read by the snapshot to compute `uptime_secs`.
static STARTED_AT: OnceLock<Instant> = OnceLock::new();

/// Web mode label (`"web"`, `"web-worker"`, or `"web-worker-script"`), set ONCE
/// via `mark_mode()` at the top of `serve()` / `enter_worker_loop()`. Read by
/// the snapshot for the `mode` field.
static MODE: OnceLock<&'static str> = OnceLock::new();

/// Returns the worker uptime in seconds, or `0` if `STARTED_AT` is not set yet
/// (it is set at serve entry before any request, so this is a defensive
/// default; never panics).
fn uptime_secs() -> u64 {
    STARTED_AT
        .get()
        .map_or(0, |i| i.elapsed().as_secs())
}

/// Records the worker start instant ONCE. Idempotent (`get_or_init`); called at
/// the top of `serve()` and `enter_worker_loop()` so the worker's uptime is
/// measured from loop entry, before any request is accepted.
pub(crate) fn mark_started() {
    STARTED_AT.get_or_init(Instant::now);
}

/// Records the web mode label ONCE. Idempotent (`get_or_init`); called at the
/// top of `serve()` with `"web"` and at the top of `enter_worker_loop()` with
/// `"web-worker"` or `"web-worker-script"` (the caller passes the right
/// literal). The snapshot reads it for the `mode` field.
pub(crate) fn mark_mode(m: &'static str) {
    MODE.get_or_init(|| m);
}

/// Records one completed (PHP or overload) request: latency sample into the
/// ring, status class into the buckets, RPS second-bucket, and overload-503 if
/// applicable. Single-threaded (I/O thread); `Relaxed` ordering. Called at every
/// return point of the service_fn EXCEPT the `/_status` intercept itself (the
/// endpoint must not record itself).
///
/// The RPS bucket algorithm: `sec = uptime_secs`; `idx = sec % 60`; if
/// `RPS_SEC[idx] == sec`, increment `RPS_COUNT[idx]`; else store `sec` into
/// `RPS_SEC[idx]` and store `1` into `RPS_COUNT[idx]` (resetting the bucket for
/// the new second). Single-threaded recording → no CAS; a stale read by
/// `/_status` is fine (best-effort approximation documented in the snapshot).
pub(crate) fn record_request(latency_us: u64, status: u16) {
    // Latency ring: write at the current index, then advance (wrap at
    // LATENCY_RING_SIZE). The ring holds the last N samples; older ones are
    // overwritten. `Relaxed` is safe: single writer (this I/O thread).
    let idx = LATENCY_IDX.fetch_add(1, Ordering::Relaxed) % LATENCY_RING_SIZE;
    LATENCY_RING[idx].store(latency_us, Ordering::Relaxed);
    let prev = LATENCY_SAMPLES.fetch_add(1, Ordering::Relaxed);
    if prev >= LATENCY_RING_SIZE as u64 {
        // Cap `samples` at the ring size so the snapshot reports the number of
        // in-window samples, not the lifetime total.
        LATENCY_SAMPLES.store(LATENCY_RING_SIZE as u64, Ordering::Relaxed);
    }
    // Status class bucket (1xx..5xx at indices 1..=5; 0 unused).
    let class = (status / 100) as usize;
    if class < STATUS_CLASSES.len() {
        STATUS_CLASSES[class].fetch_add(1, Ordering::Relaxed);
    }
    // Overload 503 is a subset of 5xx; report it separately as the signal.
    if status == 503 {
        OVERLOAD_503.fetch_add(1, Ordering::Relaxed);
    }
    // RPS sliding window: stamp the current second into its bucket.
    let sec = uptime_secs() as u32;
    let bidx = (sec as usize) % RPS_BUCKETS;
    let cur = RPS_SEC[bidx].load(Ordering::Relaxed);
    if cur == sec {
        RPS_COUNT[bidx].fetch_add(1, Ordering::Relaxed);
    } else {
        RPS_SEC[bidx].store(sec, Ordering::Relaxed);
        RPS_COUNT[bidx].store(1, Ordering::Relaxed);
    }
}

/// Immutable, `Copy`, `'static` config context the service_fn builds once
/// (captured into the closure) and passes to the snapshot at intercept time.
/// `draining` is read from the caller's `WORKER_DRAINING` at call time; the
/// closure has it in scope. `served_total` is read from the caller's module
/// `SERVED` at call time. The remaining fields come from `WorkerConfig` (Copy).
#[derive(Clone, Copy)]
pub(crate) struct MetricsCtx {
    /// Web mode label (`"web"`, `"web-worker"`, `"web-worker-script"`).
    pub mode: &'static str,
    /// Whether this worker is draining (SIGHUP reload graceful drain).
    pub draining: bool,
    /// Lifetime requests served by this worker (the module's `SERVED` counter).
    pub served_total: u64,
    /// `--max-rss` cap in BYTES (`0` = off); reported for operator visibility.
    pub max_rss_bytes: u64,
    /// `--max-pending` queue depth; reported for operator visibility.
    pub max_pending: usize,
    /// Whether `--handler-offload` is on (so `handler_inflight` is meaningful).
    pub handler_offload: bool,
    /// Whether `--http2` is on (so `h2_streams_active` is meaningful).
    pub h2_enabled: bool,
    /// `--reload-grace` seconds; reported for operator visibility.
    pub reload_grace_secs: u32,
}

/// Builds the `/_status` JSON response from the process-static counters + the
/// passed context. Content-Type `application/json`, status 200, body a
/// `Full<Bytes>`. Reads `rss::current_rss_bytes()` live (one cheap syscall; cold
/// path). Computes p50/p99 by copying `LATENCY_RING` into a `Vec<u64>`, sorting,
/// and picking the indices `len/2` and `ceil(len*99/100)-1` (clamped). `samples`
/// is the number of non-zero ring entries actually written so far (capped at
/// `LATENCY_RING_SIZE`). `rps_last_60s` = sum of `RPS_COUNT[i]` for buckets whose
/// `RPS_SEC[i] >= uptime_secs.saturating_sub(59)`, divided by
/// `min(uptime_secs, 60)` (as f64, rounded to 1 decimal in JSON). Hand-rolled
/// with `write!` (no serde); the only string fields are `mode` and `metrics_path`
/// which are ASCII literals we control, so no escaping is needed.
pub(crate) fn snapshot_response(ctx: MetricsCtx) -> Response<Full<Bytes>> {
    let pid = std::process::id();
    let uptime = uptime_secs();
    // Mode: prefer the process-static `MODE` (set via `mark_mode` at serve
    // entry); fall back to `ctx.mode` (defensive — `mark_mode` runs before any
    // request, so `MODE` is always set in practice).
    let mode = MODE.get().copied().unwrap_or(ctx.mode);
    // Latency percentiles: copy the ring into a Vec, drop zeros (unwritten
    // slots), sort, and pick the p50/p99 indices. `samples` is the capped
    // count of recorded samples (≤ LATENCY_RING_SIZE).
    let samples = LATENCY_SAMPLES.load(Ordering::Relaxed).min(LATENCY_RING_SIZE as u64);
    let mut latencies: Vec<u64> = Vec::with_capacity(LATENCY_RING_SIZE);
    for i in 0..LATENCY_RING_SIZE {
        let v = LATENCY_RING[i].load(Ordering::Relaxed);
        if v > 0 {
            latencies.push(v);
        }
    }
    latencies.sort_unstable();
    let lat = &latencies;
    let p50 = if lat.is_empty() {
        0u64
    } else {
        lat[lat.len() / 2]
    };
    let p99 = if lat.is_empty() {
        0u64
    } else {
        let idx = (lat.len() * 99 + 99) / 100; // ceil(len*99/100)
        let idx = idx.saturating_sub(1).min(lat.len() - 1);
        lat[idx]
    };
    let max_lat = lat.last().copied().unwrap_or(0);
    // RPS sliding window: sum buckets whose second is within the last 59s of
    // uptime, then divide by the window size (min(uptime, 60)) for the rate.
    let window = uptime.min(RPS_BUCKETS as u64);
    let mut rps_sum: u64 = 0;
    let cutoff = uptime.saturating_sub(59) as u32;
    for i in 0..RPS_BUCKETS {
        let s = RPS_SEC[i].load(Ordering::Relaxed);
        if s != u32::MAX && s >= cutoff {
            rps_sum = rps_sum.saturating_add(RPS_COUNT[i].load(Ordering::Relaxed));
        }
    }
    let rps_last_60s = if window > 0 {
        let r = rps_sum as f64 / window as f64;
        (r * 10.0).round() / 10.0
    } else {
        0.0
    };
    // Status class tallies (1xx..5xx; index 0 unused).
    let c1 = STATUS_CLASSES[1].load(Ordering::Relaxed);
    let c2 = STATUS_CLASSES[2].load(Ordering::Relaxed);
    let c3 = STATUS_CLASSES[3].load(Ordering::Relaxed);
    let c4 = STATUS_CLASSES[4].load(Ordering::Relaxed);
    let c5 = STATUS_CLASSES[5].load(Ordering::Relaxed);
    let overload_503 = OVERLOAD_503.load(Ordering::Relaxed);
    let handler_inflight = HANDLER_INFLIGHT.load(Ordering::Relaxed);
    let active_conns = ACTIVE_CONNS.load(Ordering::Relaxed);
    let rss_bytes = crate::rss::current_rss_bytes().unwrap_or(0);
    let h2_streams_active = H2_STREAMS_ACTIVE.load(Ordering::Relaxed);
    let draining = ctx.draining;
    let body = format!(
        concat!(
            "{{",
            "\"pid\":{pid},",
            "\"mode\":\"{mode}\",",
            "\"uptime_secs\":{uptime},",
            "\"served_total\":{served_total},",
            "\"rps_last_60s\":{rps_last_60s},",
            "\"latency_us\":{{\"p50\":{p50},\"p99\":{p99},\"max\":{max_lat},\"samples\":{samples}}},",
            "\"status_classes\":{{\"1xx\":{c1},\"2xx\":{c2},\"3xx\":{c3},\"4xx\":{c4},\"5xx\":{c5}}},",
            "\"overload_503\":{overload_503},",
            "\"handler_offload\":{handler_offload},",
            "\"handler_inflight\":{handler_inflight},",
            "\"max_pending\":{max_pending},",
            "\"active_conns\":{active_conns},",
            "\"rss_bytes\":{rss_bytes},",
            "\"max_rss_bytes\":{max_rss_bytes},",
            "\"h2_enabled\":{h2_enabled},",
            "\"h2_streams_active\":{h2_streams_active},",
            "\"reload_grace_secs\":{reload_grace_secs},",
            "\"draining\":{draining}",
            "}}"
        ),
        pid = pid,
        mode = mode,
        uptime = uptime,
        served_total = ctx.served_total,
        rps_last_60s = rps_last_60s,
        p50 = p50,
        p99 = p99,
        max_lat = max_lat,
        samples = samples,
        c1 = c1,
        c2 = c2,
        c3 = c3,
        c4 = c4,
        c5 = c5,
        overload_503 = overload_503,
        handler_offload = ctx.handler_offload,
        handler_inflight = handler_inflight,
        max_pending = ctx.max_pending,
        active_conns = active_conns,
        rss_bytes = rss_bytes,
        max_rss_bytes = ctx.max_rss_bytes,
        h2_enabled = ctx.h2_enabled,
        h2_streams_active = h2_streams_active,
        reload_grace_secs = ctx.reload_grace_secs,
        draining = draining,
    );
    Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::from_static(b""))))
}

/// RAII guard that increments `H2_STREAMS_ACTIVE` on construction and
/// decrements on drop. Created only when the request is h2 (`is_h2`), so the
/// h1 path pays nothing. Used as `let _h2g = is_h2.then(H2StreamGuard::new);`
/// near the top of the service_fn; dropped at the end of the `async move`
/// block (including early returns), decrementing on the way out.
pub(crate) struct H2StreamGuard;

impl H2StreamGuard {
    /// Builds a guard that will decrement `H2_STREAMS_ACTIVE` when dropped.
    /// The caller is responsible for only constructing this when the request
    /// is h2 so the h1 path allocates nothing.
    pub(crate) fn new() -> Self {
        H2_STREAMS_ACTIVE.fetch_add(1, Ordering::Relaxed);
        H2StreamGuard
    }
}

impl Drop for H2StreamGuard {
    /// Decrements the active h2 stream counter when a stream task ends.
    fn drop(&mut self) {
        H2_STREAMS_ACTIVE.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Returns the current `OVERLOAD_503` count (used only by tests).
#[cfg(test)]
pub(crate) fn overload_503_load() -> u64 {
    OVERLOAD_503.load(Ordering::Relaxed)
}

/// Returns the current `STATUS_CLASSES[i]` count (used only by tests).
#[cfg(test)]
pub(crate) fn status_class_load(i: usize) -> u64 {
    if i < STATUS_CLASSES.len() {
        STATUS_CLASSES[i].load(Ordering::Relaxed)
    } else {
        0
    }
}

/// Returns the current `H2_STREAMS_ACTIVE` count (used only by tests).
#[cfg(test)]
pub(crate) fn h2_streams_active_load() -> usize {
    H2_STREAMS_ACTIVE.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    // Purpose:
    // Unit tests for the per-worker metrics counters + `snapshot_response`. The
    // statics are process-global and shared across tests, so each test asserts
    // a DELTA from a baseline `load` rather than an absolute value (concurrent
    // test ordering across the module is not guaranteed, but within one test
    // function the sequence is sequential).
    //
    // Called from:
    // - `cargo test -p elephc-web` through Rust's test harness.
    //
    // Key details:
    // - `record_request` and `snapshot_response` are the public surface; the
    //   counters are `pub(crate)` statics read via the `#[cfg(test)]` accessor
    //   helpers above.
    // - Tests do not reset statics (they are `const`-init atomic statics with no
    //   `OnceLock` reset API); they assert deltas from a baseline instead.

    /// Verifies `record_request` bumps the right status-class bucket and the
    /// overload-503 counter. Uses delta-from-baseline because the statics are
    /// process-global and shared across tests.
    #[test]
    fn record_request_updates_status_classes() {
        let base_2 = status_class_load(2);
        let base_5 = status_class_load(5);
        let base_503 = overload_503_load();
        record_request(100, 200);
        record_request(200, 503);
        assert!(
            status_class_load(2) >= base_2 + 1,
            "2xx bucket must increment after a 200"
        );
        assert!(
            status_class_load(5) >= base_5 + 1,
            "5xx bucket must increment after a 503"
        );
        assert!(
            overload_503_load() >= base_503 + 1,
            "overload_503 must increment after a 503"
        );
    }

    /// Verifies `snapshot_response` returns a 200 with `application/json` and a
    /// body that is a JSON object containing the expected top-level keys.
    #[tokio::test]
    async fn snapshot_response_is_valid_json() {
        let ctx = MetricsCtx {
            mode: "web",
            draining: false,
            served_total: 0,
            max_rss_bytes: 0,
            max_pending: 16,
            handler_offload: false,
            h2_enabled: false,
            reload_grace_secs: 10,
        };
        let resp = snapshot_response(ctx);
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers().get("content-type").map(|v| v.to_str().unwrap_or("")),
            Some("application/json"),
        );
        // Collect the body bytes via `BodyExt::collect`.
        let collected = resp.into_body().collect().await.unwrap();
        let bytes: Vec<u8> = collected.to_bytes().to_vec();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.starts_with('{'), "body must be a JSON object: {}", s);
        assert!(s.contains("\"pid\""), "body must contain pid: {}", s);
        assert!(
            s.contains("\"served_total\""),
            "body must contain served_total: {}",
            s,
        );
        assert!(s.contains("\"mode\":\"web\""), "body must contain mode: {}", s);
        assert!(
            s.contains("\"active_conns\""),
            "body must contain active_conns: {}",
            s,
        );
    }

    /// Verifies the latency ring records samples and the snapshot reports
    /// p50/p99/samples. Records 256 requests with increasing latencies (0..256
    /// µs); the ring holds the last 256, so `samples` should be 256 and p50
    /// should be present.
    #[tokio::test]
    async fn latency_ring_picks_p50_p99() {
        for us in 0..LATENCY_RING_SIZE as u64 {
            record_request(us, 200);
        }
        let ctx = MetricsCtx {
            mode: "web",
            draining: false,
            served_total: 0,
            max_rss_bytes: 0,
            max_pending: 16,
            handler_offload: false,
            h2_enabled: false,
            reload_grace_secs: 10,
        };
        let resp = snapshot_response(ctx);
        let collected = resp.into_body().collect().await.unwrap();
        let bytes: Vec<u8> = collected.to_bytes().to_vec();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("\"p50\""), "body must contain p50: {}", s);
        assert!(
            s.contains("\"samples\":256"),
            "samples must be 256 after 256 records: {}",
            s,
        );
    }

    /// Verifies `H2StreamGuard` increments and decrements `H2_STREAMS_ACTIVE`
    /// by asserting the count returns to its baseline after the guard drops.
    #[test]
    fn h2_stream_guard_round_trip() {
        let before = h2_streams_active_load();
        {
            let _g = H2StreamGuard::new();
            assert_eq!(
                h2_streams_active_load(),
                before + 1,
                "guard must increment the active h2 stream count",
            );
        }
        assert_eq!(
            h2_streams_active_load(),
            before,
            "guard drop must decrement the active h2 stream count back to baseline",
        );
    }
}

// `BodyExt::collect` is used by the test module above to drain the
// `Full<Bytes>` body; the `use` is colocated with the test module.