//! Purpose:
//! Tests for the `monitor --apm` client: windows and restarts, the summary
//! table, the on-disk store, the dashboard's data, HTTP surface and Prometheus
//! view, and the argument rules.
//!
//! Called from:
//! - `cargo test --bin elephc monitor::apm`.
//!
//! Key details:
//! - Built from real snapshots through the shared `elephc_probe::apm` model, so
//!   a change to the bucket arithmetic is caught here as well as in the probe.

use super::api::{build_series, metrics_text};
use super::dashboard::{percent_decode, query_param};
use super::store::{file_name, parse_file_name};
use super::*;
use crate::monitor::parse_monitor_args;

/// A route row with one request per `(microseconds, status)` pair.
fn route(name: &str, requests: &[(u64, u16)]) -> RouteStats {
    let mut stats = RouteStats::empty(name);
    for (micros, status) in requests {
        stats.count += 1;
        stats.status[apm::status_class(*status)] += 1;
        stats.sum_ns += micros * 1_000;
        stats.hist[apm::bucket_of(micros * 1_000)] += 1;
    }
    stats
}

/// A snapshot of `routes` taken at `time_ms` by the service run `epoch`.
fn snapshot(epoch: u64, time_ms: u64, routes: Vec<RouteStats>) -> Snapshot {
    Snapshot { epoch, time_ms, routes, resources: None, build: None, errors: Vec::new(), exits: Vec::new() }
}

/// Held by the tests that spawn a process and by those that need a file lock
/// released on time. A child forked while a lock file is open shares the open
/// file until it execs, and an `flock` lives as long as any copy does: the hook
/// test's `sh` made the collector-claim test fail one run in five.
static PROCESS_AND_LOCKS: Mutex<()> = Mutex::new(());

/// A unique scratch directory, removed first so a rerun starts empty.
fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("elephc-apm-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Pins `store::now_ms` on this thread to noon UTC of today until the guard is
/// dropped, and returns the guard with that time: windows placed a minute or
/// an hour before it then fall on today whatever time the test runs (#2513).
fn pin_noon_today() -> (PinnedNow, u64) {
    let day = 86_400_000u64;
    let noon = store::now_ms() / day * day + day / 2;
    store::PINNED_NOW_MS.with(|now| now.set(Some(noon)));
    (PinnedNow, noon)
}

/// Unpins `store::now_ms` when dropped, a failed assertion included.
struct PinnedNow;

impl Drop for PinnedNow {
    /// Gives this thread back the wall clock.
    fn drop(&mut self) {
        store::PINNED_NOW_MS.with(|now| now.set(None));
    }
}

/// Two snapshots of one run make the window between them; a restart makes the
/// new run's own counters the window, spanning from the restart.
#[test]
fn a_window_is_the_difference_or_the_whole_new_run() {
    let epoch = 1_000_000_000_000_000_000;
    let start_ms = epoch / 1_000_000;
    let a = snapshot(epoch, start_ms + 5_000, vec![route("GET /", &[(100, 200), (200, 200)])]);
    let mut b_routes = vec![route("GET /", &[(100, 200), (200, 200), (300, 500)])];
    b_routes.push(route("GET /new", &[(50, 404)]));
    let b = snapshot(epoch, start_ms + 6_000, b_routes);
    let window = Window::between(&a, &b);
    assert_eq!(window.span_ms, 1_000);
    assert_eq!(window.routes.len(), 2);
    let root = window.routes.iter().find(|r| r.route == "GET /").unwrap();
    assert_eq!((root.n, root.status[4], root.sum_ns), (1, 1, 300_000));

    let restarted_epoch = epoch + 60_000_000_000;
    let c = snapshot(restarted_epoch, start_ms + 62_000, vec![route("GET /", &[(80, 200)])]);
    let window = Window::between(&b, &c);
    assert_eq!(window.epoch, restarted_epoch);
    assert_eq!(window.span_ms, 2_000, "the window runs from the restart");
    assert_eq!(window.routes[0].n, 1, "the new run's counters are the window");
}

/// Coalescing windows for the store adds every counter and the spans.
#[test]
fn coalesced_windows_keep_every_request() {
    let base = snapshot(1, 0, vec![]);
    let one = Window::between(&base, &snapshot(1, 1_000, vec![route("GET /a", &[(10, 200)])]));
    let two = Window::between(
        &snapshot(1, 1_000, vec![route("GET /a", &[(10, 200)])]),
        &snapshot(1, 2_000, vec![route("GET /a", &[(10, 200), (20, 503)]), route("GET /b", &[(5, 200)])]),
    );
    let record = coalesce(&[one, two]).unwrap();
    assert_eq!(record.t, 2_000);
    assert_eq!(record.span_ms, 2_000);
    let a = record.routes.iter().find(|r| r.route == "GET /a").unwrap();
    assert_eq!((a.n, a.status[1], a.status[4]), (2, 1, 1));
    assert!(record.routes.iter().any(|r| r.route == "GET /b"));
    assert_eq!(coalesce(&[]), None);
}

/// The table puts the route that costs the most wall time first, and the
/// totals row adds every route; an empty span says so.
#[test]
fn the_table_ranks_routes_by_time_spent() {
    let window = Window::since_start(&snapshot(
        2_000_000_000,
        12_000,
        vec![
            route("GET /cheap", &[(100, 200); 50]),
            route("GET /slow", &[(50_000, 200), (60_000, 500)]),
        ],
    ));
    let (rows, total) = summaries(&[&window], 10_000);
    assert_eq!(rows[0].route, "GET /slow");
    assert_eq!(total.n, 52);
    assert!((total.rps - 5.2).abs() < 1e-9);
    assert!((rows[0].server_error_pct - 50.0).abs() < 1e-9);
    let text = render_table("t", &[&window], 10_000);
    assert!(text.contains("52 requests over 10.0 s, 2 routes"), "{text}");
    let slow = text.find("GET /slow").unwrap();
    let cheap = text.find("GET /cheap").unwrap();
    assert!(slow < cheap, "{text}");
    assert!(text.contains("all routes"));
    assert!(render_table("t", &[], 1_000).contains("no requests completed"));
}

/// Day file names round-trip, and nothing else in the directory is taken for
/// one — the store deletes by this test, so it must be exact.
#[test]
fn only_store_files_are_recognized() {
    assert_eq!(file_name(0), "apm-1970-01-01.jsonl");
    assert_eq!(file_name(20_407), "apm-2025-11-15.jsonl");
    for day in [0u64, 59, 365, 11_016, 20_407, 40_000] {
        assert_eq!(parse_file_name(&file_name(day)), Some(day));
    }
    for name in [
        "apm-2025-11-15.json",
        "apm-2025-13-01.jsonl",
        "apm-2025-02-30.jsonl",
        "apm-2025-1-5.jsonl",
        "notes.txt",
        "apm-2025-11-15.jsonl.bak",
        // A year the day arithmetic overflows on, and one before the epoch.
        "apm-99999999999999999-01-01.jsonl",
        "apm-1969-12-31.jsonl",
    ] {
        assert_eq!(parse_file_name(name), None, "{name}");
    }
}

/// Windows written across a day boundary come back in order, a torn line is
/// skipped rather than failing the day, and retention drops whole days while
/// leaving the user's other files alone.
#[test]
fn the_store_round_trips_and_prunes_by_day() {
    let dir = scratch("store");
    let store = Store::open(dir.to_str().unwrap(), 2).unwrap();
    let day = 86_400_000u64;
    let today = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        / day
        * day;
    let window = |t: u64, n: u64| Window { build: String::new(), res: None, errors: Vec::new(),
        t,
        span_ms: 10_000,
        epoch: 7,
        source: String::new(),
        routes: vec![RouteWindow { route: "GET /".into(), n, status: [0, n, 0, 0, 0], sum_ns: n * 1_000, hist: vec![(3, n)], io: Io::default(), mem: Mem::default(), exits: Vec::new() }],
    };
    store.append(&window(today - day + 5_000, 1)).unwrap();
    store.append(&window(today + 5_000, 2)).unwrap();
    store.append(&window(today + 15_000, 3)).unwrap();
    // A crash mid-write leaves half a line behind.
    let today_file = dir.join(file_name(today / day));
    let mut text = std::fs::read_to_string(&today_file).unwrap();
    text.push_str("{\"t\":12,\"span");
    std::fs::write(&today_file, text).unwrap();
    std::fs::write(dir.join("keep-me.txt"), "mine").unwrap();

    let loaded = store.load(today - day, today + day);
    assert_eq!(loaded.iter().map(|w| w.routes[0].n).collect::<Vec<_>>(), vec![1, 2, 3]);
    assert_eq!(loaded[1], window(today + 5_000, 2));
    assert_eq!(store.newest_ms(), Some(today + 15_000));
    assert_eq!(store.load(today + 6_000, today + 20_000).len(), 1, "bounds are inclusive and exact");

    // Two days kept: today and yesterday survive, the day before goes.
    std::fs::write(dir.join(file_name(today / day - 2)), "").unwrap();
    store.prune(today + 1);
    assert!(!dir.join(file_name(today / day - 2)).exists());
    assert!(dir.join(file_name(today / day - 1)).exists());
    assert!(today_file.exists());
    assert!(dir.join("keep-me.txt").exists(), "a foreign file is never deleted");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A span longer than memory holds reads the store up to where memory starts,
/// and never counts a window twice where the two overlap.
#[test]
fn history_joins_the_store_and_memory_without_double_counting() {
    let dir = scratch("history");
    let store = Store::open(dir.to_str().unwrap(), 30).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let window = |t: u64| Window { build: String::new(), res: None, errors: Vec::new(),
        t,
        span_ms: 1_000,
        epoch: 1,
        source: String::new(),
        routes: vec![RouteWindow { route: "GET /".into(), n: 1, status: [0, 1, 0, 0, 0], sum_ns: 1, hist: vec![(0, 1)], io: Io::default(), mem: Mem::default(), exits: Vec::new() }],
    };
    for offset in (10..=40).rev() {
        store.append(&window(now - offset * 1_000)).unwrap();
    }
    let mut history = History::new("t", Some(store), true);
    // Memory overlaps the store's newest stretch, as it does on a live collector.
    for offset in (0..=20).rev() {
        history.push(window(now - offset * 1_000));
    }
    let all = history.windows_in(now - 41_000, now);
    let mut times: Vec<u64> = all.iter().map(|w| w.t).collect();
    let before = times.len();
    times.dedup();
    assert_eq!(before, times.len(), "a window was counted twice");
    assert_eq!(all.len(), 41, "every second from -40 s to now, once");
    let recent = history.windows_in(now - 5_000, now);
    assert_eq!(recent.len(), 5, "memory alone answers a span it covers");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The newest stored window is found in a long day file whose lines are not in
/// time order, and later appends are picked up — each call reading only what
/// was appended since the last.
#[test]
fn the_newest_stored_window_follows_appends() {
    let dir = scratch("newest-tail");
    let store = Store::open(dir.to_str().unwrap(), 30).unwrap();
    let base = store::now_ms() - 3_600_000;
    let window = |t: u64, routes: usize| Window { build: String::new(), res: None, errors: Vec::new(),
        t,
        span_ms: 1_000,
        epoch: 1,
        source: "a:1".into(),
        routes: (0..routes)
            .map(|i| RouteWindow::from_stats(&route(&format!("GET /route-{i:04}"), &[(100, 200)])))
            .collect(),
    };
    for second in 1..=2_000u64 {
        store.append(&window(base + second * 1_000, 1)).unwrap();
    }
    store.append(&window(base + 2_100_000, 1_000)).unwrap();
    // Another collector's slightly older window, appended after it.
    store.append(&window(base + 2_099_000, 1)).unwrap();
    assert_eq!(store.newest_ms(), Some(base + 2_100_000));
    // A clone shares what was read; a later append moves the answer on, and
    // reading it costs that one line, not the day.
    let clone = store.clone();
    let line = serde_json::to_string(&window(base + 2_101_000, 1)).unwrap().len() as u64 + 1;
    store.append(&window(base + 2_101_000, 1)).unwrap();
    let before = store::DAY_BYTES_READ.with(|count| count.get());
    assert_eq!(clone.newest_ms(), Some(base + 2_101_000));
    let read = store::DAY_BYTES_READ.with(|count| count.get()) - before;
    assert_eq!(read, line, "only the appended line is read");
    let before = store::DAY_BYTES_READ.with(|count| count.get());
    assert_eq!(store.newest_ms(), Some(base + 2_101_000));
    assert_eq!(store::DAY_BYTES_READ.with(|count| count.get()), before, "nothing new, nothing read");
    store.append(&window(base + 2_000_500, 1)).unwrap();
    assert_eq!(store.newest_ms(), Some(base + 2_101_000), "an older append does not move it back");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A script run sharing the collector's store does not hide the service's
/// recent windows: a two-hour job ending inside memory once set the cut after
/// which memory joined, and the last hour of requests vanished from a long
/// view. Runs come from the store over the whole range, so a run
/// that ended in the last hour shows on a short view too, and reading them
/// again costs only what was appended.
#[test]
fn script_runs_neither_hide_nor_miss_recent_service_windows() {
    let dir = scratch("history-runs");
    let store = Store::open(dir.to_str().unwrap(), 30).unwrap();
    let now = store::now_ms();
    let service = |t: u64| Window { build: String::new(), res: None, errors: Vec::new(),
        t,
        span_ms: 1_000,
        epoch: 7,
        source: "svc:1".into(),
        routes: vec![RouteWindow::from_stats(&route("GET /", &[(100, 200)]))],
    };
    // Stored service history before memory, then memory: the last 100 s.
    for second in (101..=400u64).rev() {
        store.append(&service(now - second * 1_000)).unwrap();
    }
    let mut history = History::new("svc:1", Some(store.clone()), true);
    for second in (0..=99u64).rev() {
        history.push(service(now - second * 1_000));
    }
    // A two-hour job that ended 10 s ago, recorded by `monitor job.php --apm`.
    let job = cli_run_window("CLI job.php", 0, 7_200_000_000_000, Io::default(), now - 10_000);
    store.append(&job).unwrap();

    let long = history.windows_in(now - 401_000, now);
    let served: u64 = long.iter().filter(|w| !is_run(w)).flat_map(|w| &w.routes).map(|r| r.n).sum();
    assert_eq!(served, 400, "every second of service traffic, once");
    assert_eq!(long.iter().filter(|w| is_run(w)).count(), 1, "and the job");
    let short = history.windows_in(now - 60_000, now);
    assert_eq!(short.iter().filter(|w| is_run(w)).count(), 1, "a recent run shows on a view memory covers");
    assert_eq!(short.iter().filter(|w| !is_run(w)).count(), 60);
    let before = store::DAY_BYTES_READ.with(|count| count.get());
    let _ = history.windows_in(now - 60_000, now);
    assert_eq!(store::DAY_BYTES_READ.with(|count| count.get()), before, "runs are not read again");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The stored window memory begins inside is read whole: with ten-second
/// stored windows and one-second memory ones, the oldest memory window starts
/// mid-window, and cutting the store there lost that window's earlier
/// requests. One request a second, every second, once.
#[test]
fn history_reads_the_stored_window_memory_begins_inside() {
    let dir = scratch("history-boundary");
    let store = Store::open(dir.to_str().unwrap(), 30).unwrap();
    let base = store::now_ms() - 120_000;
    let window = |t: u64, span_ms: u64| Window { build: String::new(), res: None, errors: Vec::new(),
        t,
        span_ms,
        epoch: 1,
        source: "t".into(),
        routes: vec![RouteWindow::from_stats(&route("GET /", &vec![(100, 200); (span_ms / 1_000) as usize]))],
    };
    for tenth in 1..=6u64 {
        store.append(&window(base + tenth * 10_000, 10_000)).unwrap();
    }
    let mut history = History::new("t", Some(store), true);
    // Memory begins at base + 55 s, half way through the stored window ending at 60 s.
    for second in 56..=70u64 {
        history.push(window(base + second * 1_000, 1_000));
    }
    let windows = history.windows_in(base, base + 70_000);
    let requests: u64 = windows.iter().flat_map(|w| &w.routes).map(|r| r.n).sum();
    assert_eq!(requests, 70, "{:?}", windows.iter().map(|w| (w.t - base, w.span_ms)).collect::<Vec<_>>());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Query strings decode the way browsers encode route labels.
#[test]
fn query_parameters_decode_route_labels() {
    assert_eq!(percent_decode("GET%20%2Fusers%2F%7Bid%7D"), "GET /users/{id}");
    assert_eq!(percent_decode("a+b"), "a b");
    assert_eq!(percent_decode("100%"), "100%", "a dangling escape stays literal");
    assert_eq!(percent_decode("%zz"), "%zz");
    let query = "range=3600&route=GET%20%2F";
    assert_eq!(query_param(query, "range").as_deref(), Some("3600"));
    assert_eq!(query_param(query, "route").as_deref(), Some("GET /"));
    assert_eq!(query_param(query, "missing"), None);
}

/// The chart series marks stretches with no window as gaps, groups windows
/// into steps, and narrows to one route without narrowing the table.
#[test]
fn the_series_has_gaps_steps_and_a_route_filter() {
    let from = 1_000_000u64;
    let windows: Vec<Window> = [10_000u64, 11_000, 50_000]
        .iter()
        .map(|offset| Window { build: String::new(), res: None, errors: Vec::new(),
            t: from + offset,
            span_ms: 1_000,
            epoch: 1,
            source: String::new(),
            routes: vec![
                RouteWindow::from_stats(&route("GET /a", &[(100, 200), (100, 500)])),
                RouteWindow::from_stats(&route("GET /b", &[(900, 200)])),
            ],
        })
        .collect();
    let series = build_series(&windows, from, from + 60_000, None, "svc", true, None);
    assert_eq!(series["step_ms"], 1_000);
    let points = series["points"].as_array().unwrap();
    assert_eq!(points.len(), 60);
    let filled: Vec<&serde_json::Value> = points.iter().filter(|p| p.get("gap").is_none()).collect();
    assert_eq!(filled.len(), 3);
    assert_eq!(filled[0]["n"], 3);
    assert!((filled[0]["rps"].as_f64().unwrap() - 3.0).abs() < 1e-9);
    assert_eq!(series["total"]["n"], 9);
    assert_eq!(series["routes"].as_array().unwrap().len(), 2);

    let narrowed = build_series(&windows, from, from + 60_000, Some("GET /b"), "svc", true, None);
    let filled: Vec<&serde_json::Value> = narrowed["points"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p.get("gap").is_none())
        .collect();
    assert_eq!(filled[0]["n"], 1, "points follow the selected route");
    assert_eq!(narrowed["selected"]["route"], "GET /b");
    assert_eq!(narrowed["routes"].as_array().unwrap().len(), 2, "the table still lists every route");

    // Stored history at a coarser resolution sets the step, so it draws as a
    // line rather than one point per window with empty steps between.
    let stored: Vec<Window> = (1..=10u64)
        .map(|k| Window { build: String::new(), res: None, errors: Vec::new(), t: from + k * 5_000, span_ms: 5_000, epoch: 1, source: String::new(), routes: windows[0].routes.clone() })
        .collect();
    let coarse = build_series(&stored, from, from + 60_000, None, "svc", true, None);
    assert_eq!(coarse["step_ms"], 5_000);
    let coarse_points = coarse["points"].as_array().unwrap();
    assert!(coarse_points[..10].iter().all(|p| p.get("gap").is_none()), "{coarse_points:?}");

    let long = build_series(&windows, from, from + 86_400_000, None, "svc", true, None);
    assert!(long["points"].as_array().unwrap().len() <= 180, "a long span is grouped into wider steps");
}

/// Prometheus buckets are cumulative, end at the request count, and never
/// count a request in a boundary it may have exceeded.
#[test]
fn metrics_buckets_are_cumulative_and_conservative() {
    let shared: Shared = Arc::new((Mutex::new(History::new("svc", None, true)), Condvar::new()));
    shared.0.lock().unwrap().latest = Some(snapshot(
        3_000_000_000,
        1,
        vec![route("GET /x", &[(900, 200), (4_900, 200), (40_000, 503), (3_000_000, 200)])],
    ));
    let text = metrics_text(&shared);
    assert!(text.contains("elephc_apm_requests_total{route=\"GET /x\",class=\"5xx\"} 1"), "{text}");
    let buckets: Vec<u64> = text
        .lines()
        .filter(|line| line.starts_with("elephc_apm_request_duration_seconds_bucket"))
        .map(|line| line.rsplit(' ').next().unwrap().parse().unwrap())
        .collect();
    assert!(buckets.windows(2).all(|pair| pair[0] <= pair[1]), "{buckets:?}");
    assert_eq!(*buckets.last().unwrap(), 4, "+Inf holds every request");
    assert!(text.contains("le=\"0.001\"} 1\n"), "the 900 µs request is under 1 ms: {text}");
    assert!(text.contains("le=\"0.005\"} 1\n"), "4.9 ms may lie past 5 ms in its bucket, so it is not counted: {text}");
    assert!(text.contains("le=\"2.5\"} 3\n"), "{text}");
    assert!(text.contains("elephc_apm_request_duration_seconds_count{route=\"GET /x\"} 4"));
}

/// Sends one HTTP GET to the dashboard and returns the whole response.
fn http_get(addr: std::net::SocketAddr, path: &str) -> String {
    raw_request(addr, &format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\n\r\n"))
}

/// Sends `request` as written and returns the whole response.
fn raw_request(addr: std::net::SocketAddr, request: &str) -> String {
    use std::io::{Read as _, Write as _};
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut out = String::new();
    let _ = stream.read_to_string(&mut out);
    out
}

/// The dashboard serves its page, its data and its metrics, and nothing else.
#[test]
fn the_dashboard_serves_its_page_data_and_metrics() {
    let shared: Shared = Arc::new((Mutex::new(History::new("svc:1", None, true)), Condvar::new()));
    {
        let mut history = shared.0.lock().unwrap();
        history.latest = Some(snapshot(1, 2_000, vec![route("GET /", &[(100, 200)])]));
        history.push(Window::since_start(&snapshot(1, 2_000, vec![route("GET /", &[(100, 200)])])));
    }
    let (addr, _) = dashboard::serve("127.0.0.1:0", shared).unwrap();
    let page = http_get(addr, "/");
    assert!(page.starts_with("HTTP/1.1 200 OK"), "{page}");
    assert!(page.contains("<title>svc:1 — call graph</title>"), "the monitor page itself is served");
    assert!(page.contains("id=\"apm\""), "with its Transactions view");
    assert!(page.contains("\"mode\":\"live\""), "fed live from the collector");
    assert!(!page.contains("<script src") && !page.contains("<link "), "the page loads nothing from the network");
    let data = http_get(addr, "/api/series?range=300");
    let body = data.split("\r\n\r\n").nth(1).unwrap();
    let json: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(json["target"], "svc:1");
    assert_eq!(json["total"]["n"], 1);
    assert!(http_get(addr, "/metrics").contains("elephc_apm_requests_total"));
    assert!(http_get(addr, "/nope").starts_with("HTTP/1.1 404"));
}

/// The dashboard refuses what a page on another site could make a browser
/// send: a rebinding Host, a capture by GET, a cross-origin capture POST; and
/// it bounds what it reads.
#[test]
fn the_dashboard_refuses_foreign_requests_and_bounds_its_input() {
    let shared: Shared = Arc::new((Mutex::new(History::new("svc:1", None, true)), Condvar::new()));
    let (addr, _) = dashboard::serve("127.0.0.1:0", shared).unwrap();
    let port = addr.port();
    let rebound = raw_request(addr, "GET /api/series HTTP/1.1\r\nHost: evil.example\r\n\r\n");
    assert!(rebound.starts_with("HTTP/1.1 403"), "{rebound}");
    let no_host = raw_request(addr, "GET / HTTP/1.1\r\n\r\n");
    assert!(no_host.starts_with("HTTP/1.1 403"), "{no_host}");
    let localhost = raw_request(addr, &format!("GET /metrics HTTP/1.1\r\nHost: localhost:{port}\r\n\r\n"));
    assert!(localhost.starts_with("HTTP/1.1 200"), "{localhost}");
    let by_get = http_get(addr, "/api/capture");
    assert!(by_get.starts_with("HTTP/1.1 405"), "a GET must not arm the sampler: {by_get}");
    let cross = raw_request(
        addr,
        &format!("POST /api/capture HTTP/1.1\r\nHost: {addr}\r\nOrigin: http://evil.example\r\nContent-Length: 0\r\n\r\n"),
    );
    assert!(cross.starts_with("HTTP/1.1 403"), "{cross}");
    let same = raw_request(
        addr,
        &format!("POST /api/capture HTTP/1.1\r\nHost: {addr}\r\nOrigin: http://{addr}\r\nContent-Length: 0\r\n\r\n"),
    );
    assert!(same.starts_with("HTTP/1.1 502"), "allowed through to the capture, which has no service: {same}");
    let huge = format!("GET /{} HTTP/1.1\r\nHost: {addr}\r\n\r\n", "a".repeat(20_000));
    assert!(raw_request(addr, &huge).starts_with("HTTP/1.1 400"), "an oversized head is refused");
    let mut many = format!("GET / HTTP/1.1\r\nHost: {addr}\r\n");
    for index in 0..200 {
        many.push_str(&format!("X-{index}: y\r\n"));
    }
    many.push_str("\r\n");
    assert!(raw_request(addr, &many).starts_with("HTTP/1.1 400"), "too many headers are refused");
}

/// The Host check applies to loopback listeners, on their own port only.
#[test]
fn only_loopback_names_reach_a_loopback_dashboard() {
    let local: std::net::SocketAddr = "127.0.0.1:9000".parse().unwrap();
    assert!(dashboard::host_is_allowed(Some("127.0.0.1:9000"), local));
    assert!(dashboard::host_is_allowed(Some("localhost:9000"), local));
    assert!(dashboard::host_is_allowed(Some("[::1]:9000"), local));
    assert!(!dashboard::host_is_allowed(Some("localhost:9001"), local));
    assert!(!dashboard::host_is_allowed(Some("attacker.example:9000"), local));
    assert!(!dashboard::host_is_allowed(Some("localhost"), local));
    assert!(!dashboard::host_is_allowed(None, local));
    let public: std::net::SocketAddr = "10.0.0.5:9000".parse().unwrap();
    assert!(dashboard::host_is_allowed(Some("monitor.internal:9000"), public), "a public bind cannot know its names");
}

/// The command-line words `list`, as `parse_monitor_args` receives them.
fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| arg.to_string()).collect()
}

/// `--apm` takes its own options, refuses profile ones, and needs either a
/// service or a store to browse.
#[test]
fn apm_arguments_are_validated() {
    let cmd = parse_monitor_args(&args(&["127.0.0.1:9411", "--apm", "--store", "/tmp/x", "--serve", "127.0.0.1:0"]))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(cmd.apm);
    assert_eq!(cmd.apm_retention_days, DEFAULT_RETENTION_DAYS);
    assert_eq!(cmd.apm_resolution_secs, DEFAULT_RESOLUTION_SECS);
    let cmd = parse_monitor_args(&args(&["h:1", "--apm", "--live", "--store", "/tmp/x", "--retention", "30", "--resolution", "60"])).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((cmd.apm_retention_days, cmd.apm_resolution_secs, cmd.live), (30, 60, true));
    assert!(cmd.apm_retention_explicit);
    assert_eq!((cmd.apm_full_days, cmd.apm_minute_days), (DEFAULT_FULL_DAYS, DEFAULT_MINUTE_DAYS));
    let cmd = parse_monitor_args(&args(&["h:1", "--apm", "--store", "/tmp/x", "--retention-full", "3", "--retention-minute", "30"]))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((cmd.apm_full_days, cmd.apm_minute_days), (3, 30));
    // Store settings with no store, and collection flags while browsing, are refused.
    for bad in [
        vec!["h:1", "--apm", "--retention", "30"],
        vec!["h:1", "--apm", "--retention-full", "3"],
        vec!["--apm", "--store", "/tmp/x", "--retention-minute", "30"],
        vec!["h:1", "--apm", "--store", "/tmp/x", "--retention-full", "soon"],
        vec!["h:1", "--apm", "--resolution", "60"],
        vec!["--apm", "--store", "/tmp/x", "--live"],
        vec!["--apm", "--store", "/tmp/x", "--alert", "p95:*<=1"],
        vec!["--apm", "--store", "/tmp/x", "--capture-every", "5"],
        vec!["--apm", "--store", "/tmp/x", "--retention", "3"],
    ] {
        assert!(parse_monitor_args(&args(&bad)).is_err(), "{bad:?} was accepted");
    }
    // Browsing a store needs no service, in a browser or in the terminal.
    assert!(parse_monitor_args(&args(&["--apm", "--store", "/tmp/x", "--serve", "127.0.0.1:0"])).is_ok());
    assert!(parse_monitor_args(&args(&["--apm", "--store", "/tmp/x", "--route", "GET /"])).is_ok());

    for bad in [
        vec!["h:1", "--apm", "--exact"],
        vec!["--apm", "--attach", "12"],
        vec!["--apm"],
        vec!["h:1", "--store", "/tmp/x"],
        vec!["h:1", "--route", "GET /"],
        vec!["h:1", "--apm", "--retention", "0"],
        vec!["h:1", "--apm", "--resolution", "soon"],
    ] {
        assert!(parse_monitor_args(&args(&bad)).is_err(), "{bad:?} was accepted");
    }
    // Everything after `--` belongs to the launched program, options included.
    let cmd = parse_monitor_args(&args(&["app.php", "--apm", "--", "app:import", "--force", "-v"]))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(cmd.target_args, ["app:import", "--force", "-v"]);
    assert!(!cmd.live, "a program's own flags are not monitor's");
}

/// Which flags combine with `--apm` depends on the target: a service answers
/// with counters, a script runs once to completion.
#[test]
fn apm_flags_follow_the_target() {
    let parse = |list: &[&str]| parse_monitor_args(&args(list)).unwrap_or_else(|e| panic!("{e}"));
    let service = parse(&["h:1", "--apm", "--html", "x.html", "--assert", "calls:f<=1"]);
    assert_eq!(conflicts(&service, true), ["--html", "--assert"]);
    assert!(conflicts(&service, false).is_empty(), "a script may export and assert");
    let script = parse(&["app.php", "--apm", "--live", "--serve", "127.0.0.1:0"]);
    assert_eq!(conflicts(&script, false), ["--live", "--serve"]);
    assert!(conflicts(&script, true).is_empty(), "a service has live views");
    let with_args = parse(&["h:1", "--apm", "--", "x"]);
    assert_eq!(conflicts(&with_args, true), ["-- <program arguments>"]);
}

/// A run is named by its script and console command; arguments that are data
/// do not create rows of their own.
#[test]
fn a_cli_run_is_named_by_script_and_command() {
    let name = |target: &str, list: &[&str]| cli_transaction_name(target, &args(list));
    assert_eq!(name("bin/console", &["app:import", "rows-17.csv"]), "CLI console app:import");
    assert_eq!(name("bin/console", &["--env=prod", "cache:clear"]), "CLI console cache:clear");
    assert_eq!(name("artisan", &["queue:work", "--once"]), "CLI artisan queue:work");
    assert_eq!(name("/srv/jobs/nightly.php", &[]), "CLI nightly");
    assert_eq!(name("import.php", &["/data/rows.csv"]), "CLI import", "a path is data");
    assert_eq!(name("import.php", &["2026-11-15"]), "CLI import", "a date is data");
    assert_eq!(name("import.php", &["rows.csv"]), "CLI import", "a file name is data");
}

/// A run that exits 0 is a 2xx-class transaction, anything else a 5xx one,
/// with the run's wall time and I/O.
#[test]
fn a_cli_run_window_carries_status_time_and_io() {
    let io = Io { db_ops: 12, db_wait_ns: 30_000_000, net_ops: 1, net_wait_ns: 10_000_000 };
    let ok = cli_run_window("CLI x", 0, 200_000_000, io, 5_000);
    assert_eq!(ok.routes[0].status, [0, 1, 0, 0, 0]);
    assert_eq!((ok.t, ok.span_ms), (5_000, 200));
    assert_eq!(ok.routes[0].io, io);
    let failed = cli_run_window("CLI x", 1, 100_000_000, Io::default(), 6_000);
    assert_eq!(failed.routes[0].status, [0, 0, 0, 0, 1]);
    let table = render_runs("CLI x", &[ok, failed]);
    assert!(table.contains("CLI x — 2 runs"), "{table}");
    let numbers: Vec<&str> = table.lines().last().unwrap().split_whitespace().collect();
    assert_eq!(numbers[0], "2");
    assert_eq!(numbers.iter().filter(|cell| **cell == "50.0%").count(), 1, "half the runs failed: {table}");
    assert!(table.contains("6.0"), "12 queries over 2 runs: {table}");
    assert!(table.contains("13.3%"), "40 ms of 300 ms spent waiting: {table}");
}

/// The page's Scripts section gets each script's runs, oldest first, with each
/// run's duration, failure and statements; a service's routes are not runs.
#[test]
fn the_series_lists_each_scripts_runs() {
    let io = Io { db_ops: 4, ..Io::default() };
    let windows = vec![
        cli_run_window("CLI import", 255, 300_000_000, Io::default(), 1_020_000),
        cli_run_window("CLI import", 0, 200_000_000, io, 1_010_000),
        cli_run_window("CLI mailer", 0, 50_000_000, Io::default(), 1_030_000),
        Window {
            build: String::new(),
            res: None,
            errors: Vec::new(),
            t: 1_040_000,
            span_ms: 1_000,
            epoch: 7,
            source: "svc:1".into(),
            routes: vec![RouteWindow::from_stats(&route("GET /", &[(100, 200)]))],
        },
    ];
    let series = build_series(&windows, 1_000_000, 1_060_000, None, "svc", true, None);
    let scripts = series["runs"].as_array().unwrap();
    assert_eq!(scripts.len(), 2, "two scripts, and the route is not one: {scripts:?}");
    assert_eq!(scripts[0]["name"], "CLI import");
    let runs = scripts[0]["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!((runs[0]["t"].as_u64(), runs[0]["mean"].as_u64()), (Some(1_010_000), Some(200_000_000)), "oldest first");
    assert_eq!((runs[0]["failed"].as_u64(), runs[0]["q"].as_u64()), (Some(0), Some(4)));
    assert_eq!(runs[1]["failed"].as_u64(), Some(1), "the later run failed");
    assert_eq!(runs[1]["exits"], serde_json::json!([[255, 1]]), "with its exact code");
    assert_eq!(runs[0]["exits"], serde_json::json!([]), "a run that succeeded lists none");
    assert_eq!((scripts[0]["total"].as_u64(), scripts[0]["failed"].as_u64()), (Some(2), Some(1)), "the totals count every run");
    assert!(scripts[0]["p95"].as_u64().unwrap() >= 200_000_000, "the p95 of the runs' durations");
    assert_eq!(scripts[1]["name"], "CLI mailer");
}

/// A period with more runs than the page draws still counts them all: an early
/// failure, cut from the drawn list, stays in the script's failures (review).
#[test]
fn a_scripts_totals_cover_the_runs_it_does_not_draw() {
    let windows: Vec<Window> = (0..300u64).map(|i| cli_run_window("CLI job", i32::from(i == 0), 1_000_000, Io::default(), 1_000_000 + i)).collect();
    let series = build_series(&windows, 1_000_000, 1_060_000, None, "svc", true, None);
    let script = &series["runs"][0];
    assert_eq!(script["runs"].as_array().unwrap().len(), 240, "the last runs drawn");
    assert_eq!((script["total"].as_u64(), script["failed"].as_u64()), (Some(300), Some(1)), "the first run failed");
}

/// Runs that programs sent read `<instance> · CLI <script>` once several sources
/// share the page: the Scripts section still lists them under their script.
#[test]
fn the_series_lists_the_runs_programs_sent_under_their_script() {
    let sent = |source: &str, code: i32, t: u64| Window { source: source.into(), epoch: 3, ..cli_run_window("CLI shopkit", code, 1_000_000, Io::default(), t) };
    let mut windows = vec![sent("shopkit@10.0.0.1 pid 1", 0, 1_010_000), sent("shopkit@10.0.0.2 pid 2", 3, 1_020_000)];
    label_sources(&mut windows);
    assert!(windows[0].routes[0].route.ends_with(" · CLI shopkit"), "labelled: {:?}", windows[0].routes[0].route);
    let series = build_series(&windows, 1_000_000, 1_060_000, None, "listen", true, None);
    let scripts = series["runs"].as_array().unwrap();
    assert_eq!(scripts.len(), 1, "one script, two instances: {scripts:?}");
    assert_eq!(scripts[0]["name"], "CLI shopkit");
    assert_eq!(scripts[0]["runs"][1]["exits"], serde_json::json!([[3, 1]]));
}

/// A program sending to a collector repeats in every snapshot the runs it ended,
/// each with its exact exit code: a window counts the new ones only, and a
/// stored minute adds them up code by code.
#[test]
fn a_window_counts_the_exit_codes_of_the_runs_that_ended_in_it() {
    let cli = |n: u64| RouteStats { count: n, status: [0, 0, 0, 0, n], sum_ns: n * 1_000, ..RouteStats::empty("CLI import") };
    let mut before = snapshot(9, 1_000, vec![cli(1)]);
    before.exits = vec![("CLI import".into(), 255)];
    let mut after = snapshot(9, 2_000, vec![cli(3)]);
    after.exits = vec![("CLI import".into(), 255), ("CLI import".into(), 255), ("CLI import".into(), 1)];
    let window = Window::between(&before, &after);
    assert_eq!(window.routes[0].exits, vec![(1, 1), (255, 1)], "the first 255 was counted before");
    assert_eq!(Window::since_start(&before).routes[0].exits, vec![(255, 1)]);
    let minute = coalesce(&[Window::since_start(&before), window]).unwrap();
    assert_eq!(minute.routes[0].exits, vec![(1, 1), (255, 2)]);
}

/// The page's Apps section gets each instance of a pushing program with its
/// build and whether it is sending, silent or gone, and the program's figures
/// per build; a polled service is not an app.
#[test]
fn the_series_lists_apps_by_instance_and_version() {
    // `ok` calls answered 200 and `failed` ones 500, a millisecond each.
    let pushed = |source: &str, build: &str, t: u64, ok: usize, failed: usize| Window {
        build: build.to_string(),
        res: None,
        errors: Vec::new(),
        t,
        span_ms: 5_000,
        epoch: 7,
        source: source.into(),
        routes: vec![RouteWindow::from_stats(&route(
            "CALL render",
            &[vec![(1_000_000u64, 200u16); ok], vec![(1_000_000u64, 500u16); failed]].concat(),
        ))],
    };
    let now = 10_000_000;
    let windows = vec![
        pushed("shop@10.0.0.7 pid 812 #3fa2c1d0", "1700000000 v2.3.0", now - 2_000, 8, 2),
        pushed("shop@10.0.0.8 pid 77 #0be9d114", "1700000000 v2.2.1", now - 30_000, 10, 0),
        pushed("shop@10.0.0.9 pid 95 #5d10fe31", "1700000000 v2.2.1", now - 900_000, 5, 0),
        Window {
            build: String::new(),
            res: None,
            errors: Vec::new(),
            t: now - 1_000,
            span_ms: 1_000,
            epoch: 3,
            source: "127.0.0.1:9411".into(),
            routes: vec![RouteWindow::from_stats(&route("GET /", &[(200, 3)]))],
        },
    ];
    let series = build_series(&windows, now - 3_600_000, now, None, "listen", true, None);
    let apps = &series["apps"];
    let instances = apps["instances"].as_array().unwrap();
    assert_eq!(instances.len(), 3, "the polled service is not an app: {instances:?}");
    let state_of = |pid: &str| instances.iter().find(|i| i["source"].as_str().unwrap().contains(pid)).unwrap()["state"].clone();
    assert_eq!(state_of("pid 812"), "sending");
    assert_eq!(state_of("pid 77"), "silent");
    assert_eq!(state_of("pid 95"), "gone");
    let versions = apps["versions"].as_array().unwrap();
    let of = |build: &str| versions.iter().find(|v| v["build"] == build).unwrap().clone();
    assert_eq!(of("v2.3.0")["n"], 10);
    assert_eq!(of("v2.3.0")["errors"].as_f64(), Some(20.0), "2 of 10 calls failed");
    // The chart's slices hold the same calls and failures, over the span.
    let slices = of("v2.3.0")["slices"].as_array().unwrap().clone();
    assert_eq!(slices.len(), 60);
    let sum = |k: usize| slices.iter().map(|s| s[k].as_u64().unwrap()).sum::<u64>();
    assert_eq!((sum(0), sum(1)), (10, 2));
    assert_eq!(of("v2.2.1")["n"], 15);
    assert_eq!(of("v2.2.1")["instances"], 2, "both v2.2.1 instances, the gone one included");
    assert_eq!(of("v2.2.1")["program"], "shop");
    assert!(build_series(&windows[3..], now - 3_600_000, now, None, "svc", true, None)["apps"].is_null());
    // Two builds without a label differ by their build time: two rows, not one (review).
    let unlabelled = vec![
        pushed("tool@10.0.0.1 pid 1 #aaaaaaaa", "1700000000", now - 2_000, 4, 0),
        pushed("tool@10.0.0.2 pid 2 #bbbbbbbb", "1700086400", now - 2_000, 4, 4),
    ];
    let apps = build_series(&unlabelled, now - 3_600_000, now, None, "listen", true, None)["apps"].clone();
    let builts: Vec<u64> = apps["versions"].as_array().unwrap().iter().map(|v| v["built"].as_u64().unwrap()).collect();
    assert_eq!(builts.len(), 2, "{apps}");
    assert!(builts.contains(&1_700_000_000) && builts.contains(&1_700_086_400));
}

/// A live page's recent range reads each instance against the clock, not against its newest
/// window, which stops moving once every program stops; and a script the Scripts section
/// groups answers to its name when picked, every instance's runs included (review).
#[test]
fn a_live_page_reads_app_states_against_the_clock_and_picks_grouped_scripts() {
    let dir = scratch("apps-clock");
    let store = Store::open(dir.to_str().unwrap(), 2).unwrap();
    let now = store::now_ms();
    for (source, code) in [("job@10.0.0.1 pid 1 #11111111", 0), ("job@10.0.0.2 pid 2 #22222222", 3)] {
        let window = Window { source: source.into(), epoch: 5, ..cli_run_window("CLI job", code, 1_000_000, Io::default(), now - 120_000) };
        store.append(&window).unwrap();
    }
    let mut history = History::new("listening on 127.0.0.1:9475", Some(store), true);
    history.pushed = true;
    let shared: Shared = Arc::new((Mutex::new(history), Condvar::new()));
    let series = api::series_json(&shared, "range=900");
    let states: Vec<&str> = series["apps"]["instances"].as_array().unwrap().iter().map(|i| i["state"].as_str().unwrap()).collect();
    assert_eq!(states, vec!["silent", "silent"], "two minutes without a send");
    let picked = api::series_json(&shared, "range=900&route=CLI%20job");
    let n: u64 = picked["points"].as_array().unwrap().iter().filter_map(|p| p["n"].as_u64()).sum();
    assert_eq!(n, 2, "both instances' runs: {}", picked["points"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The exported page builds its views with `build_series` alone: the grouped errors come
/// with it, or its Errors section stayed hidden (review).
#[test]
fn the_shared_series_carries_the_grouped_errors() {
    let error = WindowError { route: "GET /pay".into(), class: "App\\Declined".into(), at: "/app/Pay.php:9".into(), n: 2, trace: String::new() };
    let window = Window { build: String::new(), res: None, errors: vec![error], t: 1_030_000, span_ms: 1_000, epoch: 1, source: String::new(), routes: vec![RouteWindow::from_stats(&route("GET /pay", &[(1, 500), (1, 500)]))] };
    let series = build_series(&[window], 1_000_000, 1_060_000, None, "svc", false, None);
    assert_eq!(series["errors"]["total"], 2, "{}", series["errors"]);
}

/// A memory reading cut between its bytes and its count is data, not nothing: the stored
/// window keeps it, since the next one takes it as its baseline (review).
#[test]
fn a_partial_memory_reading_is_stored() {
    let partial = Mem { n: 0, sum: 4 << 20, hist: vec![(5, 1)] };
    assert!(!partial.is_empty());
    assert!(Mem::default().is_empty());
    let window = Window { build: String::new(), res: None, errors: Vec::new(), t: 1_000, span_ms: 1_000, epoch: 1, source: String::new(), routes: vec![RouteWindow { mem: partial.clone(), ..RouteWindow::from_stats(&route("GET /", &[(1, 200)])) }] };
    let line = serde_json::to_string(&window).unwrap();
    let back: Window = serde_json::from_str(&line).unwrap();
    assert_eq!(back.routes[0].mem, partial, "{line}");
}

/// Database and network work survives coalescing for the store.
#[test]
fn io_survives_coalescing() {
    let io = Io { db_ops: 2, db_wait_ns: 5, net_ops: 0, net_wait_ns: 0 };
    let a = cli_run_window("CLI x", 0, 1_000_000, io, 1_000);
    let b = cli_run_window("CLI x", 0, 1_000_000, io, 2_000);
    let record = coalesce(&[a, b]).unwrap();
    assert_eq!(record.routes[0].io.db_ops, 4);
    assert_eq!(record.routes[0].io.db_wait_ns, 10);
}

/// Sparklines scale to their own peak, keep zero distinct from "not
/// measured", and a trend leaves a slot no window ended in empty.
#[test]
fn trends_scale_and_keep_gaps() {
    assert_eq!(terminal::sparkline(&[Some(0.0), Some(5.0), Some(10.0), None]), "▁▅█ ");
    assert_eq!(terminal::sparkline(&[Some(0.0), Some(0.0)]), "▁▁");
    let window = |t: u64, n: usize| Window { build: String::new(), res: None, errors: Vec::new(),
        t,
        span_ms: 1_000,
        epoch: 1,
        source: String::new(),
        routes: vec![RouteWindow::from_stats(&route("GET /", &vec![(100, 200); n]))],
    };
    // Four seconds of traffic, then a silence the collector did not see.
    let windows = [window(1_000, 2), window(2_000, 4), window(3_000, 4), window(4_000, 2), window(10_000, 1)];
    let refs: Vec<&Window> = windows.iter().collect();
    let (rates, p95) = terminal::trend(&refs, "GET /", 10);
    assert_eq!(rates.len(), 10);
    assert!(rates[5..9].iter().all(Option::is_none), "{rates:?}");
    assert_eq!(rates[9], Some(1.0));
    assert!(p95.iter().flatten().all(|v| *v > 0.0));
    let text = terminal::render_trends(&refs, &["GET /".to_string()]);
    assert!(text.contains("GET /") && text.contains("4.0"), "the peak rate is shown: {text}");
}

/// The histogram's rows cover every request once, in octaves whose ranges
/// contain them.
#[test]
fn the_histogram_counts_every_request_once() {
    let stats = route("GET /x", &[(100, 200), (150, 200), (900, 200), (40_000, 200), (41_000, 500)]);
    let text = terminal::render_histogram(&stats);
    assert!(text.contains("latency of GET /x — 5 requests"), "{text}");
    let counts: u64 = text
        .lines()
        .filter(|line| line.contains('–'))
        .map(|line| line.split_whitespace().rev().nth(1).unwrap().parse::<u64>().unwrap())
        .sum();
    assert_eq!(counts, 5, "{text}");
    assert!(text.contains("2  40.0%"), "the two 40 ms requests share a row: {text}");
}

/// A script's run trend marks the failed runs under the durations.
#[test]
fn the_run_trend_marks_failures() {
    let runs: Vec<Window> = (0..4u64)
        .map(|i| cli_run_window("CLI job", i32::from(i == 2), (i + 1) * 100_000_000, Io::default(), 1_000 * (i + 1)))
        .collect();
    let text = terminal::render_run_trend(&runs, "CLI job");
    assert!(text.contains("last 4 runs"), "{text}");
    assert!(text.contains("  ✗ ") || text.contains("✗ failed"), "{text}");
    assert_eq!(text.matches('✗').count(), 2, "one mark and the legend: {text}");
    assert_eq!(terminal::render_run_trend(&runs[..1], "CLI job"), "", "one run has no trend");
}

/// A minute the store folded holds several runs: its point is their mean run,
/// and the heading counts runs, not minutes (review).
#[test]
fn the_run_trend_reads_folded_minutes_as_mean_runs() {
    let run = |ms: u64, t: u64| cli_run_window("CLI job", 0, ms * 1_000_000, Io::default(), t);
    let sixty_fast = coalesce(&(0..60).map(|i| run(10, 60_000 + i)).collect::<Vec<_>>()).expect("a minute");
    let one_fast = run(10, 120_000);
    let one_slow = run(600, 180_000);
    let text = terminal::render_run_trend(&[sixty_fast, one_fast, one_slow], "CLI job");
    assert!(text.contains("last 62 runs over 3 minutes"), "{text}");
    let bars: Vec<char> = text.lines().nth(2).unwrap().trim().chars().collect();
    assert_eq!(bars[0], bars[1], "sixty 10 ms runs read like one: {text}");
    assert_ne!(bars[1], bars[2], "{text}");
}

/// Two collectors of the same service cannot write one store; collectors of
/// different services can.
#[test]
fn one_collector_per_service_and_store() {
    let _serial = PROCESS_AND_LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("claim");
    let store = Store::open(dir.to_str().unwrap(), 1).unwrap();
    let first = store.claim_collector("127.0.0.1:9411").expect("the first claim succeeds");
    assert!(store.claim_collector("127.0.0.1:9411").is_err(), "a second collector would double every count");
    let other = store.claim_collector("10.0.0.2:9411");
    assert!(other.is_ok(), "another service is not a conflict");
    drop(first);
    assert!(store.claim_collector("127.0.0.1:9411").is_ok(), "the claim ends with its collector");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Without `--apm` the monitor page carries no trace of the Transactions
/// view: the placeholders are removed and nothing of the view is inserted.
#[test]
fn the_page_without_apm_has_no_transactions_view() {
    let graph = crate::monitor::build_call_graph(&[]);
    let page = crate::call_graph::render_html_frames(&[(0, &graph)], "t", false, 0, false, &[]);
    for marker in ["__APM_", "id=\"apm\"", "apm-capture", "#apm ", "\"apm\":"] {
        assert!(!page.contains(marker), "{marker} leaked into a page without --apm");
    }
}

/// An exported page embeds every stored transaction and one series per route,
/// so the file opens anywhere and its route filter works offline.
#[test]
fn an_export_embeds_the_stored_transactions() {
    let dir = scratch("export");
    let store = Store::open(dir.to_str().unwrap(), 2).unwrap();
    let now = store::now_ms();
    for (i, ok) in [true, false, true].iter().enumerate() {
        store.append(&cli_run_window("CLI job", i32::from(!*ok), 100_000_000, Io::default(), now - 60_000 + i as u64 * 10_000)).unwrap();
    }
    store.append(&Window { build: String::new(), res: None, errors: Vec::new(), t: now - 5_000, span_ms: 1_000, epoch: 1, source: String::new(), routes: vec![RouteWindow::from_stats(&route("GET /", &[(100, 200)]))] }).unwrap();
    let cmd = parse_monitor_args(&args(&["job.php", "--apm", "--store", dir.to_str().unwrap()])).unwrap();
    let view = static_view(&cmd, None);
    assert_eq!(view.data["mode"], "static");
    assert_eq!(view.data["all"]["total"]["n"], 4);
    assert_eq!(view.data["routes"]["CLI job"]["selected"]["n"], 3);
    assert!((view.data["routes"]["CLI job"]["selected"]["server_error_pct"].as_f64().unwrap() - 100.0 / 3.0).abs() < 1e-6);
    assert!(view.data["routes"].get("GET /").is_some());
    let graph = crate::monitor::build_call_graph(&[]);
    let page = crate::call_graph::render_html_page(&[(0, &graph)], "job", false, 0, true, &[], Some(&view));
    assert!(page.contains("id=\"apm\"") && page.contains("\"mode\":\"static\""));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Runs spread over an hour are a rate per hour, not their summed durations:
/// nine 160 ms runs did not happen in 1.4 s. Service windows still count the
/// time they tiled.
#[test]
fn runs_are_rated_over_the_time_they_spread_over() {
    let runs: Vec<Window> = (0..9u64)
        .map(|i| cli_run_window("CLI job", 0, 160_000_000, Io::default(), 1_000_000 + i * 450_000))
        .collect();
    let refs: Vec<&Window> = runs.iter().collect();
    let measured = measured_ms(&refs);
    assert!((3_600_000..3_700_000).contains(&measured), "{measured}");
    let (_, total) = summaries(&refs, measured);
    assert!(total.rps < 0.01, "nine runs in an hour: {}", total.rps);
    let service = [
        Window { build: String::new(), res: None, errors: Vec::new(), t: 5_000, span_ms: 1_000, epoch: 7, source: String::new(), routes: vec![] },
        Window { build: String::new(), res: None, errors: Vec::new(), t: 6_000, span_ms: 1_000, epoch: 7, source: String::new(), routes: vec![] },
    ];
    let service_refs: Vec<&Window> = service.iter().collect();
    assert_eq!(measured_ms(&service_refs), 2_000);
}

/// An exact slice with its trace line, as a `--web` service hands it over.
fn slice(route: &str, status: Option<u16>, root_ns: u64) -> String {
    let status = status.map_or(String::new(), |code| format!(" status={code}"));
    format!(
        "elephc-instr-trace: trace=0af7651916cd43dd8448eb211c80319c span=b7ad6b7169203331 parent=- start=1 route={}{status}\n\
         elephc-instr: {{main}} calls=1 incl_ns={root_ns} excl_ns={root_ns}\n",
        route.replace(' ', "%20")
    )
}

/// A slice's route, status and duration are read from it, and a slice from
/// an older binary — no status field — still reads.
#[test]
fn a_captured_slice_carries_route_status_and_duration() {
    let (route, status, duration, graph) =
        traces::TraceBook::trace_from_slice(&slice("GET /orders/{id}", Some(503), 42_000_000)).expect("a profile");
    assert_eq!(route, "GET /orders/{id}");
    assert_eq!(status, Some(503));
    assert!(duration > 0, "{duration}");
    assert!(!graph.nodes.is_empty());
    let (_, status, _, _) = traces::TraceBook::trace_from_slice(&slice("GET /", None, 1_000)).unwrap();
    assert_eq!(status, None);
    assert!(traces::TraceBook::trace_from_slice("elephc-instr: note: no request completed\n").is_none());
}

/// The book keeps the slowest captures of each route and its recent failures,
/// and drops what a newcomer pushes out.
#[test]
fn the_trace_book_keeps_the_slowest_and_the_failures() {
    let mut book = traces::TraceBook::default();
    let graph = || crate::monitor::build_call_graph(&[]);
    for (i, ms) in [10u64, 50, 20, 90, 30, 70, 40].iter().enumerate() {
        book.offer("GET /a".into(), Some(200), ms * 1_000_000, graph(), i as u64);
    }
    let kept: Vec<u64> = book.list().iter().map(|t| t.duration_ns / 1_000_000).collect();
    assert_eq!(kept, [90, 70, 50, 40, 30], "the five slowest of the route");
    assert!(!book.offer("GET /a".into(), Some(200), 5_000_000, graph(), 99), "a fast one is not kept");
    assert!(book.offer("GET /a".into(), Some(500), 1_000_000, graph(), 100), "a failure is kept however fast");
    assert!(book.offer("GET /b".into(), Some(200), 1_000_000, graph(), 101), "another route has its own set");
    let failed: Vec<bool> = book.list().iter().map(|t| t.failed()).collect();
    assert_eq!(failed.iter().filter(|f| **f).count(), 1);
    let id = book.list()[0].id;
    assert_eq!(book.get(id).map(|t| t.duration_ns), Some(90_000_000));
}

/// A slow failure is among its route's slowest too: five newer, faster
/// failures do not push out the slowest request the route served.
#[test]
fn the_slowest_capture_survives_newer_faster_failures() {
    let mut book = traces::TraceBook::default();
    let graph = || crate::monitor::build_call_graph(&[]);
    assert!(book.offer("GET /a".into(), Some(504), 5_000_000_000, graph(), 1), "a slow timeout");
    for i in 0..5u64 {
        book.offer("GET /a".into(), Some(500), 1_000_000, graph(), 10 + i);
    }
    let kept: Vec<u64> = book.list().iter().map(|t| t.duration_ns / 1_000_000).collect();
    assert_eq!(kept.first(), Some(&5_000), "the slowest request is kept: {kept:?}");
    assert_eq!(kept.len(), 6, "and the five recent failures: {kept:?}");
}

/// `--capture-every` belongs to a collecting service run, never to a script.
#[test]
fn capture_every_is_a_collector_option() {
    let cmd = parse_monitor_args(&args(&["h:1", "--apm", "--serve", "127.0.0.1:0", "--capture-every", "5"])).unwrap();
    assert_eq!(cmd.apm_capture_every, 5);
    assert!(conflicts(&cmd, true).is_empty());
    let script = parse_monitor_args(&args(&["job.php", "--apm", "--capture-every", "5"])).unwrap();
    assert_eq!(conflicts(&script, false), ["--capture-every"]);
    assert!(parse_monitor_args(&args(&["h:1", "--capture-every", "5"])).is_err(), "needs --apm");
    assert!(parse_monitor_args(&args(&["h:1", "--apm", "--capture-every", "0"])).is_err());
}

/// Alert rules use the `--assert` syntax, name a route or `*`, and refuse a
/// metric they cannot measure.
#[test]
fn alert_rules_parse_like_assertions() {
    let rule = alerts::parse_rule("p95:GET /users/{id}<=150").unwrap();
    assert_eq!(rule.metric, alerts::Metric::P95);
    assert_eq!(rule.route.as_deref(), Some("GET /users/{id}"));
    assert_eq!((rule.op.as_str(), rule.threshold), ("<=", 150.0));
    assert_eq!(alerts::parse_rule("error_pct:*<1").unwrap().route, None);
    assert!(alerts::parse_rule("latency:GET /<=1").is_err(), "unknown metric");
    assert!(alerts::parse_rule("p95:GET /").is_err(), "no operator");
    assert!(parse_monitor_args(&args(&["h:1", "--alert", "p95:*<=1"])).is_err(), "needs --apm");
    assert!(parse_monitor_args(&args(&["h:1", "--apm", "--alert", "bogus"])).is_err());
    let script = parse_monitor_args(&args(&["job.php", "--apm", "--alert", "p95:*<=1"])).unwrap();
    assert_eq!(conflicts(&script, false), ["--alert"]);
    // Nor an alert window: a run has none to measure (review).
    let script = parse_monitor_args(&args(&["job.php", "--apm", "--alert-window", "30"])).unwrap();
    assert_eq!(conflicts(&script, false), ["--alert-window"]);
}

/// Rules measure their route over the windows given; a route with no traffic
/// keeps its state instead of resolving or firing — except for throughput,
/// where no request is the measurement and a floor fires on it.
#[test]
fn alerts_fire_resolve_and_hold_through_silence() {
    let window = |t: u64, requests: &[(u64, u16)]| Window { build: String::new(), res: None, errors: Vec::new(),
        t,
        span_ms: 1_000,
        epoch: 1,
        source: String::new(),
        routes: vec![RouteWindow::from_stats(&route("GET /a", requests))],
    };
    let mut state = alerts::Alerts::new(vec![
        alerts::parse_rule("p95:GET /a<=10").unwrap(),
        alerts::parse_rule("error_pct:*<=10").unwrap(),
        alerts::parse_rule("rps:GET /b>=1").unwrap(),
    ]);
    let fast = window(1_000, &[(1_000, 200); 20]);
    // GET /b serves nothing: its throughput is zero, below its floor.
    let changes = state.check(&[&fast], 1);
    assert_eq!(changes.len(), 1, "only the throughput floor breaks: {changes:?}");
    assert!(changes[0].firing && changes[0].spec == "rps:GET /b>=1" && changes[0].value == 0.0);
    let slow = window(2_000, &[(50_000, 500); 20]);
    let changes = state.check(&[&slow], 2);
    assert_eq!(changes.len(), 2, "{changes:?}");
    assert!(changes.iter().all(|c| c.firing));
    assert!(changes[0].value > 10.0 && changes[0].unit == "ms");
    // No traffic at all: latency and errors stay broken, nothing resolves.
    let silent = Window { build: String::new(), res: None, errors: Vec::new(), t: 3_000, span_ms: 1_000, epoch: 1, source: String::new(), routes: vec![] };
    assert!(state.check(&[&silent], 3).is_empty());
    assert!(state.alerts.iter().all(|alert| alert.firing));
    let changes = state.check(&[&fast], 4);
    assert_eq!(changes.iter().filter(|c| !c.firing).count(), 2, "latency and errors resolve: {changes:?}");
    assert!(state.alerts[2].firing, "GET /b still serves nothing");
}

/// The hook runs with the transition in its environment, so a webhook is one
/// `curl` in the command.
#[test]
fn the_alert_hook_receives_the_transition() {
    let _serial = PROCESS_AND_LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("alert-hook");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("hook.txt");
    let hook = format!(
        "printf '%s|%s|%s|%s' \"$ELEPHC_ALERT_STATE\" \"$ELEPHC_ALERT_RULE\" \"$ELEPHC_ALERT_VALUE\" \"$ELEPHC_ALERT_TARGET\" > {}",
        out.display()
    );
    let transition = alerts::Transition { spec: "p95:GET /<=10".into(), firing: true, value: 42.5, unit: "ms" };
    alerts::announce("127.0.0.1:9411", &transition, Some(&hook), false);
    // The hook writes asynchronously: wait for the whole line, not for the
    // file to exist, so a loaded machine cannot read it half written.
    let expected = "firing|p95:GET /<=10|42.50ms|127.0.0.1:9411";
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut seen = String::new();
    while std::time::Instant::now() < deadline {
        seen = std::fs::read_to_string(&out).unwrap_or_default();
        if seen == expected {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(seen, expected);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A request's queries are published just before its count, so a poll can see
/// the work before it sees the request end. That window must keep the work, and
/// so must the record the store folds it into.
#[test]
fn a_window_keeps_work_for_a_request_still_in_flight() {
    let before = snapshot(9, 1_000, vec![RouteStats::empty("GET /slow")]);
    let mut working = RouteStats::empty("GET /slow");
    working.io[apm::IO_DB_OPS] = 20;
    let during = snapshot(9, 2_000, vec![working.clone()]);
    let mut done = route("GET /slow", &[(3_000_000, 200)]);
    done.io[apm::IO_DB_OPS] = 30;
    let after = snapshot(9, 3_000, vec![done]);
    let first = Window::between(&before, &during);
    let second = Window::between(&during, &after);
    assert_eq!(first.routes.len(), 1, "the in-flight work is kept: {first:?}");
    assert_eq!(first.routes[0].io.db_ops, 20);
    assert_eq!(second.routes[0].io.db_ops, 10);
    let record = coalesce(&[first.clone(), second.clone()]).unwrap();
    assert_eq!((record.routes[0].n, record.routes[0].io.db_ops), (1, 30));
    let (rows, _) = summaries(&[&first, &second], 2_000);
    assert_eq!(rows[0].queries_per_txn, 30.0, "all thirty statements belong to the one request");
}

/// Opening a store deletes nothing; only an explicit prune does.
#[test]
fn opening_a_store_never_prunes_it() {
    let dir = scratch("noprune");
    std::fs::create_dir_all(&dir).unwrap();
    let old = dir.join(file_name(10));
    std::fs::write(&old, "").unwrap();
    let store = Store::open(dir.to_str().unwrap(), 1).unwrap();
    assert!(old.exists(), "browsing must not cut a collector's longer history");
    store.prune_now();
    assert!(!old.exists(), "the collector's prune does");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A script's run that crosses midnight appends without pruning: the store's
/// older days belong to a collector that may keep them longer than the
/// script's default retention. A collector's window crossing midnight prunes
/// (review).
#[test]
fn only_a_collectors_window_prunes_at_midnight() {
    let dir = scratch("midnight-prune");
    std::fs::create_dir_all(&dir).unwrap();
    let old = dir.join(file_name(10));
    let store = Store::open(dir.to_str().unwrap(), 1).unwrap();
    let midnight = store::now_ms() / 86_400_000 * 86_400_000;
    let crossing = Window {
        build: String::new(),
        res: None,
        errors: Vec::new(),
        t: midnight + 1_000,
        span_ms: 5_000,
        epoch: 7,
        source: String::new(),
        routes: vec![RouteWindow { route: "CLI job.php".into(), n: 1, status: [0, 1, 0, 0, 0], sum_ns: 1_000, hist: vec![(3, 1)], io: Io::default(), mem: Mem::default(), exits: Vec::new() }],
    };
    std::fs::write(&old, "").unwrap();
    store.append(&crossing).unwrap();
    assert!(old.exists(), "a script's run keeps the collector's history");
    store.append_collected(&crossing).unwrap();
    assert!(!old.exists(), "a collector's new day prunes");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Two services in one store over the same minute measured one minute, and
/// their same-named routes stay apart once labelled.
#[test]
fn a_shared_store_rates_and_names_each_service() {
    let window = |source: &str, t: u64| Window { build: String::new(), res: None, errors: Vec::new(),
        t,
        span_ms: 1_000,
        epoch: 1,
        source: source.to_string(),
        routes: vec![RouteWindow::from_stats(&route("GET /", &[(100, 200); 10]))],
    };
    let mut windows: Vec<Window> = (1..=60u64)
        .flat_map(|second| [window("a:1", second * 1_000), window("b:1", second * 1_000)])
        .collect();
    let refs: Vec<&Window> = windows.iter().collect();
    assert_eq!(measured_ms(&refs), 60_000, "overlapping services cover one minute, not two");
    label_sources(&mut windows);
    let refs: Vec<&Window> = windows.iter().collect();
    let (rows, total) = summaries(&refs, measured_ms(&refs));
    let names: Vec<&str> = rows.iter().map(|row| row.route.as_str()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"a:1 · GET /") && names.contains(&"b:1 · GET /"));
    assert!((rows[0].rps - 10.0).abs() < 1e-9, "each service served 10/s: {}", rows[0].rps);
    assert!((total.rps - 20.0).abs() < 1e-9);
    let mut alone = vec![window("a:1", 1_000)];
    label_sources(&mut alone);
    assert_eq!(alone[0].routes[0].route, "GET /", "one service needs no label");
}

/// A page over a store two services write to keeps their `GET /` apart, and
/// its chart points and terminal trends count the seconds both covered once:
/// each service's 10 requests a second read 10/s, and both together 20/s.
#[test]
fn a_shared_store_page_and_trends_rate_each_service() {
    let dir = scratch("shared-page");
    let (_pinned, now) = pin_noon_today();
    let store = Store::open(dir.to_str().unwrap(), 2).unwrap();
    let window = |source: &str, t: u64| Window { build: String::new(), res: None, errors: Vec::new(),
        t,
        span_ms: 1_000,
        epoch: 1,
        source: source.to_string(),
        routes: vec![RouteWindow::from_stats(&route("GET /", &[(100, 200); 10]))],
    };
    let mut windows = Vec::new();
    for second in 1..=30u64 {
        for source in ["a:1", "b:1"] {
            let w = window(source, now - 60_000 + second * 1_000);
            store.append(&w).unwrap();
            windows.push(w);
        }
    }
    let shared: Shared = Arc::new((Mutex::new(History::new("", Some(store), false)), Condvar::new()));
    let series = api::series_json(&shared, "range=60");
    let names: Vec<&str> = series["routes"].as_array().unwrap().iter().map(|r| r["route"].as_str().unwrap()).collect();
    assert!(names.contains(&"a:1 · GET /") && names.contains(&"b:1 · GET /"), "{names:?}");
    let filled: Vec<&serde_json::Value> =
        series["points"].as_array().unwrap().iter().filter(|p| p.get("gap").is_none()).collect();
    assert!(!filled.is_empty());
    for point in &filled {
        let per_second = point["n"].as_f64().unwrap() / (series["step_ms"].as_f64().unwrap() / 1_000.0);
        assert!((point["rps"].as_f64().unwrap() - per_second).abs() < 1e-9, "{point}");
    }
    assert!((filled[0]["rps"].as_f64().unwrap() - 20.0).abs() < 1e-9, "{}", filled[0]);
    label_sources(&mut windows);
    let refs: Vec<&Window> = windows.iter().collect();
    let (rates, _) = terminal::trend(&refs, "a:1 · GET /", 10);
    let peak = rates.iter().flatten().copied().fold(0.0_f64, f64::max);
    assert!((peak - 10.0).abs() < 1e-9, "one service's peak: {rates:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A live collector's page reads only its own service from a shared store.
#[test]
fn a_live_history_reads_only_its_own_service_from_the_store() {
    let dir = scratch("own");
    let store = Store::open(dir.to_str().unwrap(), 2).unwrap();
    let now = store::now_ms();
    for (source, t) in [("a:1", now - 60_000), ("b:1", now - 60_000), ("a:1", now - 50_000)] {
        store
            .append(&Window { build: String::new(), res: None, errors: Vec::new(), t, span_ms: 1_000, epoch: 1, source: source.into(), routes: vec![RouteWindow::from_stats(&route("GET /", &[(1, 200)]))] })
            .unwrap();
    }
    let history = History::new("a:1", Some(store), true);
    let windows = history.windows_in(now - 120_000, now);
    assert_eq!(windows.len(), 2, "{windows:?}");
    assert!(windows.iter().all(|w| w.source == "a:1"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// One long window — a stretch the collector could not reach — does not turn
/// the chart around it into a handful of points.
#[test]
fn one_long_window_does_not_coarsen_the_chart() {
    let mut windows: Vec<Window> = (1..=300u64)
        .map(|second| Window { build: String::new(), res: None, errors: Vec::new(), t: 1_000_000 + second * 1_000, span_ms: 1_000, epoch: 1, source: String::new(), routes: vec![] })
        .collect();
    windows.push(Window { build: String::new(), res: None, errors: Vec::new(), t: 1_000_000 + 900_000, span_ms: 600_000, epoch: 1, source: String::new(), routes: vec![] });
    let series = api::build_series(&windows, 1_000_000, 1_000_000 + 900_000, None, "svc", true, None);
    assert_eq!(series["step_ms"], 5_000, "the typical one-second window sets the step");
}

/// Route labels carrying `<` and `>` parse with strict operators.
#[test]
fn synthetic_route_labels_parse_in_alerts() {
    let rule = alerts::parse_rule("error_pct:<other><5").unwrap();
    assert_eq!((rule.route.as_deref(), rule.op.as_str(), rule.threshold), (Some("<other>"), "<", 5.0));
    let rule = alerts::parse_rule("p95:<untagged>>=2").unwrap();
    assert_eq!((rule.route.as_deref(), rule.op.as_str()), (Some("<untagged>"), ">="));
    assert!(alerts::parse_rule("p95:GET /<=").is_err());
}

/// An exported page of a single run without a store shows that run.
#[test]
fn an_export_without_a_store_shows_the_run() {
    let cmd = parse_monitor_args(&args(&["job.php", "--apm"])).unwrap();
    let run = cli_run_window("CLI job", 0, 1_000_000, Io::default(), store::now_ms());
    let view = static_view(&cmd, Some(&run));
    assert_eq!(view.data["all"]["total"]["n"], 1);
}

/// A snapshot measured at `time_ms` with `cpu_ns` of CPU so far and
/// `memory_bytes` in use, for the resource tests.
fn measured(epoch: u64, time_ms: u64, cpu_ns: u64, memory_bytes: u64) -> Snapshot {
    Snapshot {
        resources: Some(apm::Resources { cpu_ns, memory_bytes, processes: 3, cores: 10 }),
        ..snapshot(epoch, time_ms, vec![route("GET /", &[(100, 200)])])
    }
}

/// A window carries the CPU spent between two snapshots and the memory at its
/// end; after a restart, the new run's CPU from zero. A stored record adds the
/// CPU of its windows and keeps their highest memory (service CPU and memory).
#[test]
fn windows_carry_the_services_cpu_and_memory() {
    let first = measured(7, 1_000, 2_000_000_000, 100 << 20);
    let second = measured(7, 2_000, 2_500_000_000, 120 << 20);
    let window = Window::between(&first, &second);
    assert_eq!(window.res, Some(WindowRes { cpu_ns: 500_000_000, memory_bytes: 120 << 20, processes: 3, cores: 10 }));
    let restarted = measured(8, 3_000, 300_000_000, 90 << 20);
    assert_eq!(Window::between(&second, &restarted).res.map(|res| res.cpu_ns), Some(300_000_000));
    assert_eq!(Window::between(&snapshot(7, 1_000, vec![]), &second).res.map(|res| res.cpu_ns), Some(2_500_000_000));
    let record = coalesce(&[window.clone(), Window { build: String::new(), res: Some(WindowRes { cpu_ns: 100, memory_bytes: 80 << 20, processes: 2, cores: 10 }), ..window.clone() }]).unwrap();
    assert_eq!(record.res, Some(WindowRes { cpu_ns: 500_000_100, memory_bytes: 120 << 20, processes: 3, cores: 10 }));
    let line = serde_json::to_string(&window).unwrap();
    assert!(line.contains("\"res\":{\"cpu_ns\":500000000,\"mem\":125829120,\"procs\":3,\"cores\":10}"), "{line}");
    assert_eq!(serde_json::from_str::<Window>(&line).unwrap(), window);
    let bare = Window { build: String::new(), res: None, errors: Vec::new(), ..window };
    assert!(!serde_json::to_string(&bare).unwrap().contains("res"), "nothing measured, nothing stored");
}

/// Over a set of windows, CPU is cores over the time covered once — two
/// services each busy half a core over the same minute use one core — and
/// memory is each service's newest figure, added up. The chart points, the
/// Prometheus metrics and the terminal table say the same.
#[test]
fn resources_add_up_across_services_and_reach_every_view() {
    let window = |source: &str, t: u64, memory: u64| Window {
        build: String::new(),
        t,
        span_ms: 1_000,
        epoch: 1,
        source: source.to_string(),
        routes: vec![],
        res: Some(WindowRes { cpu_ns: 500_000_000, memory_bytes: memory, processes: 2, cores: 4 }),
        errors: Vec::new(),
    };
    let windows: Vec<Window> = (1..=60u64)
        .flat_map(|second| [window("a:1", 1_000_000 + second * 1_000, (100 + second) << 20), window("b:1", 1_000_000 + second * 1_000, 50 << 20)])
        .collect();
    let refs: Vec<&Window> = windows.iter().collect();
    let res = resources_of(&refs).unwrap();
    assert!((res.cores - 1.0).abs() < 1e-9, "{res:?}");
    assert_eq!(res.memory_bytes, (160 << 20) + (50 << 20), "newest of each service");
    assert_eq!(res.processes, 4);
    assert_eq!(res.cores_available, 8, "each service's cores, added up");
    assert_eq!(res.cpu_pct(), Some(12.5), "one busy core of eight");
    let series = build_series(&windows, 1_000_000, 1_060_000, None, "svc", true, None);
    let point = series["points"].as_array().unwrap().iter().find(|p| p.get("gap").is_none()).unwrap();
    assert!((point["cpu"].as_f64().unwrap() - 1.0).abs() < 1e-9, "{point}");
    assert_eq!(point["cpus"], 8, "what the point's CPU is a share of: {point}");
    assert_eq!(series["resources"]["cpu_pct"], 12.5);
    assert_eq!(series["resources"]["memory_bytes"], (160u64 << 20) + (50 << 20));
    let table = render_table("t", &refs, measured_ms(&refs));
    assert!(table.contains("service: CPU 12.5% of 8 cores (1.00 busy) · memory 210.0 MB · 4 processes"), "{table}");
    assert!(resources_of(&[&cli_run_window("CLI job", 0, 1, Io::default(), 1)]).is_none(), "a run has none");

    let shared: Shared = Arc::new((Mutex::new(History::new("svc", None, true)), Condvar::new()));
    shared.0.lock().unwrap().latest = Some(measured(1, 1_000, 12_500_000_000, 64 << 20));
    let text = metrics_text(&shared);
    assert!(text.contains("elephc_apm_cpu_seconds_total 12.5\n"), "{text}");
    assert!(text.contains("elephc_apm_memory_bytes 67108864\n"), "{text}");
    assert!(text.contains("elephc_apm_processes 3\n"), "{text}");
    assert!(text.contains("elephc_apm_cpu_cores 10\n"), "{text}");
}

/// The page lays a budget out in columns, so `/api/alerts` sends the rule's
/// parts beside the rule as written: metric, route (`null` for `*`, every
/// route), operator and threshold.
#[test]
fn the_alerts_json_carries_each_rules_parts() {
    let mut history = History::new("svc", None, true);
    history.alerts = alerts::Alerts::new(vec![
        alerts::parse_rule("queries:GET /orders/{id}<=3").unwrap(),
        alerts::parse_rule("error_pct:*<1").unwrap(),
    ]);
    let shared: Shared = Arc::new((Mutex::new(history), Condvar::new()));
    let json = api::alerts_json(&shared);
    let first = &json["alerts"][0];
    assert_eq!(first["rule"], "queries:GET /orders/{id}<=3");
    assert_eq!(first["metric"], "queries");
    assert_eq!(first["route"], "GET /orders/{id}");
    assert_eq!(first["op"], "<=");
    assert_eq!(first["threshold"], 3.0);
    assert_eq!(first["unit"], " q/req");
    let all = &json["alerts"][1];
    assert_eq!((&all["metric"], &all["route"], &all["op"]), (&serde_json::json!("error_pct"), &serde_json::Value::Null, &serde_json::json!("<")));
}

/// Retention deletes a day's file; the runs the store had read from it are
/// dropped with it, so a page open for weeks keeps only the days on disk.
#[test]
fn a_pruned_day_leaves_the_read_cache() {
    let dir = scratch("cache-prune");
    let store = Store::open(dir.to_str().unwrap(), 30).unwrap();
    let day = 86_400_000u64;
    let today = store::now_ms() / day * day;
    for t in [today - 2 * day + 5_000, today - day + 5_000, today + 5_000] {
        store.append(&cli_run_window("CLI job.php", 0, 1_000_000, Io::default(), t)).unwrap();
    }
    assert_eq!(store.runs_in(today - 3 * day, today + day).len(), 3);
    assert_eq!(store.cached_days(), 3);
    // What retention does to a day past it.
    std::fs::remove_file(dir.join(store::file_name((today - 2 * day) / day))).unwrap();
    assert_eq!(store.runs_in(today - 3 * day, today + day).len(), 2, "the deleted day's run is gone");
    assert_eq!(store.cached_days(), 2, "and so is what was read of it");
    assert!(store.newest_ms().is_some());
    assert_eq!(store.cached_days(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A captured request's page is another graph than the service's, under the
/// same title: it embeds the capture's id and time, and the page keys its
/// saved pan and zoom on them, so neither page restores the other's view.
#[test]
fn a_capture_page_keeps_its_own_saved_view() {
    let mut history = History::new("127.0.0.1:9411", None, true);
    let graph = crate::monitor::build_call_graph(&[(vec![("show_order".to_string(), crate::monitor::Kind::Php)], 3)]);
    assert!(history.traces.offer("GET /orders/{id}".into(), Some(200), 1_000_000, graph, 1_794_700_000_000));
    let id = history.traces.list()[0].id;
    let shared: Shared = Arc::new((Mutex::new(history), Condvar::new()));
    let capture = api::page_html(&shared, Some(id));
    assert!(capture.contains(&format!("\"id\":{id}")) && capture.contains("\"t\":1794700000000"), "the capture's identity is in the page");
    assert!(capture.contains("(CAPTURE ? ':trace:' + CAPTURE.id + ':' + CAPTURE.t : '')"), "and keys its saved view");
    let service = api::page_html(&shared, None);
    assert!(!service.contains("\"trace\":{"), "the service's page carries no capture");
}

/// A poll that caught a request between its counters — status, time and
/// latency bucket written, count not yet — keeps the row, so the request's
/// status and latency are not lost: the next window is measured from that
/// poll, and only adds the count (review).
#[test]
fn a_request_read_between_its_counters_keeps_its_status_and_latency() {
    let epoch = 1_000_000_000_000_000_000;
    let start_ms = epoch / 1_000_000;
    let before = snapshot(epoch, start_ms + 1_000, vec![route("GET /quiet", &[(100, 200)])]);
    // The torn read: a 5xx's status, time and bucket, but the old count.
    let mut torn = route("GET /quiet", &[(100, 200), (9_000, 500)]);
    torn.count = 1;
    let torn = snapshot(epoch, start_ms + 2_000, vec![torn]);
    let after = snapshot(epoch, start_ms + 3_000, vec![route("GET /quiet", &[(100, 200), (9_000, 500)])]);
    let first = Window::between(&before, &torn);
    let second = Window::between(&torn, &after);
    let merged = merge_routes([&first, &second]);
    let row = merged.iter().find(|row| row.name == "GET /quiet").expect("the row was kept");
    assert_eq!((row.count, row.status[4], row.hist.iter().sum::<u64>()), (1, 1, 1), "{row:?}");
    let stored = coalesce(&[first]).expect("a record");
    assert_eq!(stored.routes.len(), 1, "a record keeps the row too: {stored:?}");
}

/// A budget is measured over the windows that lie in its window, not over
/// one that only ends in it: the first window after an outage spans the
/// whole outage (review).
#[test]
fn an_alert_window_leaves_out_what_started_before_it() {
    let now = 10_000_000u64;
    let window = |t: u64, span_ms: u64| Window { build: String::new(), res: None, errors: Vec::new(), t, span_ms, epoch: 1, source: "s".into(), routes: Vec::new() };
    let windows = [window(now - 2_000, 1_000), window(now - 1_000, 600_000), window(now, 1_000)];
    let kept: Vec<u64> = super::collector::alert_windows(windows.iter(), now, 60_000).iter().map(|w| w.t).collect();
    assert_eq!(kept, vec![now - 2_000, now], "the ten-minute window is not the last minute");
}

/// While the service does not answer, no request is seen: a throughput floor
/// fires once the alert window has passed without one, as it does when the
/// traffic stops, and a latency budget keeps its state (review).
#[test]
fn a_dead_service_breaks_a_throughput_floor() {
    let cmd = parse_monitor_args(&args(&[
        "127.0.0.1:9411",
        "--apm",
        "--live",
        "--alert",
        "rps:*>=1",
        "--alert",
        "p95:*<=50",
        "--alert-window",
        "10",
    ]))
    .unwrap();
    let mut history = History::new("127.0.0.1:9411", None, true);
    history.alerts = alerts::Alerts::new(cmd.apm_alerts.iter().map(|spec| alerts::parse_rule(spec).unwrap()).collect());
    let busy = route("GET /", &(0..20).map(|_| (1_000, 200)).collect::<Vec<_>>());
    let start = 50_000_000u64;
    for second in 1..=10 {
        history.push(Window {
            build: String::new(),
            res: None,
            errors: Vec::new(),
            t: start + second * 1_000,
            span_ms: 1_000,
            epoch: 7,
            source: "127.0.0.1:9411".into(),
            routes: vec![RouteWindow::from_stats(&busy)],
        });
        super::collector::check_alerts(&mut history, &cmd, start + second * 1_000, false);
    }
    assert!(history.alerts.alerts.iter().all(|alert| !alert.firing), "20 req/s at 1 ms keeps both");
    // The service stops answering: polls fail for twenty seconds.
    for second in 11..=30 {
        super::collector::check_alerts(&mut history, &cmd, start + second * 1_000, true);
    }
    let rps = &history.alerts.alerts[0];
    assert!(rps.firing && rps.value == Some(0.0), "the floor fires: {rps:?}");
    assert!(!history.alerts.alerts[1].firing, "no request is no latency: {:?}", history.alerts.alerts[1]);
}

/// Flags that act only in the collector are refused on a one-shot read of a
/// service instead of being accepted and ignored (review); with a collector,
/// or `--alert` alone (a one-shot budget check), they are fine.
#[test]
fn collector_flags_need_a_collector() {
    let one_shot = |extra: &[&str]| {
        let mut words = vec!["127.0.0.1:9411", "--apm"];
        words.extend_from_slice(extra);
        need_a_collector(&parse_monitor_args(&args(&words)).unwrap())
    };
    assert_eq!(one_shot(&["--capture-every", "5"]), vec!["--capture-every"]);
    assert_eq!(
        one_shot(&["--alert", "rps:*>=1", "--alert-exec", "true", "--alert-window", "30"]),
        vec!["--alert-exec", "--alert-window"]
    );
    assert!(one_shot(&["--alert", "rps:*>=1"]).is_empty(), "a one-shot budget check");
    assert!(one_shot(&["--live", "--capture-every", "5", "--alert-exec", "true"]).is_empty());
    assert!(one_shot(&["--serve", "127.0.0.1:0", "--capture-every", "5"]).is_empty());
}

/// Durations are bounded where they are parsed: they become milliseconds, and
/// an unbounded one overflowed the multiplication (review).
#[test]
fn apm_durations_are_bounded() {
    for (flag, max) in [("--resolution", "86400"), ("--capture-every", "86400"), ("--alert-window", "3600")] {
        let accepted = parse_monitor_args(&args(&["127.0.0.1:9411", "--apm", "--store", "/tmp/x", "--live", flag, max]));
        assert!(accepted.is_ok(), "{flag} {max}: {:?}", accepted.err());
        for value in ["18446744073709552", "0"] {
            let refused = parse_monitor_args(&args(&["127.0.0.1:9411", "--apm", "--store", "/tmp/x", "--live", flag, value]));
            let Err(error) = refused else { panic!("{flag} {value} is refused") };
            assert!(error.contains(&format!("from 1 to {max}")), "{error}");
        }
    }
}

/// Two targets the lock's readable stem makes alike still get locks of their
/// own, so neither collector refuses the other (review).
#[test]
fn collectors_of_lookalike_targets_do_not_share_a_lock() {
    let _serial = PROCESS_AND_LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("lookalike");
    let store = Store::open(dir.to_str().unwrap(), 2).unwrap();
    let first = store.claim_collector("/srv/a-b.sock").expect("the first claim");
    let second = store.claim_collector("/srv/a_b.sock").expect("another target, another lock");
    assert!(store.claim_collector("/srv/a-b.sock").is_err(), "the same target is still refused");
    drop((first, second));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The store's reads are incremental for every reader, ranges included: a day
/// file is parsed once, then only for what was appended. A past day's service
/// windows and every day's script runs are folded by the minute, without
/// losing a request; today's service windows keep their resolution (review).
#[test]
fn stored_ranges_read_only_what_was_appended() {
    let dir = scratch("ranges");
    let (_pinned, now) = pin_noon_today();
    let store = Store::open(dir.to_str().unwrap(), 7).unwrap();
    let day = 86_400_000u64;
    let yesterday_noon = (now / day - 1) * day + day / 2;
    let service = |t: u64| Window {
        build: String::new(),
        res: None,
        errors: Vec::new(),
        t,
        span_ms: 10_000,
        epoch: 3,
        source: "a:1".into(),
        routes: vec![RouteWindow::from_stats(&route("GET /", &[(100, 200)]))],
    };
    for index in 0..30 {
        store.append(&service(yesterday_noon + index * 10_000)).unwrap();
    }
    for index in 0..3 {
        store.append(&service(now - 50_000 + index * 10_000)).unwrap();
    }
    for index in 0..40u64 {
        let run = runs::cli_run_window("CLI job", if index % 4 != 0 { 0 } else { 1 }, 1_000_000, Io::default(), yesterday_noon + 500 * index);
        store.append(&run).unwrap();
    }
    store::DAY_BYTES_READ.with(|count| count.set(0));
    let all = store.load(yesterday_noon - day / 2, now);
    let first_read = store::DAY_BYTES_READ.with(|count| count.get());
    let services: Vec<&Window> = all.iter().filter(|w| !is_run(w)).collect();
    let runs: Vec<&Window> = all.iter().filter(|w| is_run(w)).collect();
    let requests = |windows: &[&Window]| merge_routes(windows.iter().copied()).iter().map(|row| row.count).sum::<u64>();
    assert_eq!(requests(&services), 33, "no request lost to folding");
    assert_eq!(requests(&runs), 40, "no run lost either");
    assert_eq!(services.len(), 5 + 3, "yesterday's 300 s fold into five minutes; today's three stay");
    assert!(runs.len() <= 2, "forty runs over twenty seconds are one or two minutes: {}", runs.len());
    let failed: u64 = runs.iter().flat_map(|w| &w.routes).map(|r| r.status[4]).sum();
    assert_eq!(failed, 10, "folded runs keep their failures");
    store.load(yesterday_noon - day / 2, now);
    assert_eq!(store::DAY_BYTES_READ.with(|count| count.get()), first_read, "nothing re-read");
    store.append(&service(now)).unwrap();
    assert_eq!(store.load(now - 100_000, now).len(), 4, "an append is picked up");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A size just under a unit reads in the next unit, as it rounds (review: it
/// read `1024.0 MB`).
#[test]
fn sizes_take_the_unit_of_their_rounded_figure() {
    assert_eq!(fmt_bytes(1_000), "1 KB");
    assert_eq!(fmt_bytes(1024 * 1024 - 100), "1.0 MB");
    assert_eq!(fmt_bytes(1024 * 1024 * 1024 - 1_000), "1.00 GB");
    assert_eq!(fmt_bytes(5 * 1024 * 1024 + 512 * 1024), "5.5 MB");
}

/// A connection's slot comes back however its thread ends, a panic included,
/// and a request head that dribbles in past its deadline is cut off: neither
/// can leave the server answering 503 to everyone (review).
#[test]
fn connection_slots_are_never_lost() {
    use std::io::Read as _;
    let open = Arc::new(std::sync::atomic::AtomicUsize::new(1));
    let slot = dashboard::Slot(open.clone());
    let joined = std::thread::spawn(move || {
        let _slot = slot;
        panic!("a request handler fails");
    })
    .join();
    assert!(joined.is_err());
    assert_eq!(open.load(std::sync::atomic::Ordering::Relaxed), 0, "the panicking request gave its slot back");
    let mut late = dashboard::Deadline { inner: &b"GET / HTTP/1.1\r\n"[..], until: std::time::Instant::now() };
    let error = late.read(&mut [0u8; 16]).expect_err("past the deadline nothing more is read");
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
}

/// The event stream tells the browser how soon to reconnect, since the
/// server ends each stream after a while to free its slot (review).
#[test]
fn the_event_stream_says_how_to_reconnect() {
    use std::io::{Read as _, Write as _};
    let shared: Shared = Arc::new((Mutex::new(History::new("svc:1", None, true)), Condvar::new()));
    let (addr, _) = dashboard::serve("127.0.0.1:0", shared).unwrap();
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    stream.write_all(format!("GET /api/stream HTTP/1.1\r\nHost: {addr}\r\n\r\n").as_bytes()).unwrap();
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
    let mut head = vec![0u8; 512];
    let n = stream.read(&mut head).unwrap();
    let text = String::from_utf8_lossy(&head[..n]).to_string();
    assert!(text.contains("text/event-stream") && text.contains("\r\n\r\nretry: 1000\n\n"), "{text}");
}

/// A service whose clock is days off — behind or ahead — still has its
/// windows stored in today's file, kept by retention and found by a range
/// over the last minutes: windows carry the collector's clock, which the day
/// files and the pruning go by. With the service's clock, ten days behind
/// landed in a file retention deleted at once, ten days ahead in one it never
/// reached (review).
#[test]
fn a_service_with_a_skewed_clock_is_stored_by_the_collectors() {
    let dir = scratch("skew");
    let store = Store::open(dir.to_str().unwrap(), 7).unwrap();
    let now = store::now_ms();
    let day = 86_400_000u64;
    for skew in [-10i64, 10] {
        let service_now = (now as i64 + skew * day as i64) as u64;
        let epoch = (service_now - 60_000) * 1_000_000;
        let before = snapshot(epoch, service_now - 1_000, vec![route("GET /", &[(100, 200)])]);
        let after = snapshot(epoch, service_now, vec![route("GET /", &[(100, 200), (100, 200)])]);
        let window = super::collector::collected_window(&before, &after, "far:1", now);
        assert_eq!((window.t, window.span_ms, window.source.as_str()), (now, 1_000, "far:1"));
        store.append(&window).unwrap();
        assert!(dir.join(file_name(now / day)).exists(), "today's file, whatever the service's clock");
    }
    store.prune_now();
    let stored = store.load(now - 60_000, now);
    assert_eq!(stored.len(), 2, "both kept by retention and found in the last minute");
    assert_eq!(store.newest_ms(), Some(now));
    let _ = std::fs::remove_dir_all(&dir);
}

/// `--reset-routes` is an action on a running service, given alone: never
/// with a collection, a store browse, a script, or without `--apm`.
#[test]
fn reset_routes_is_given_a_service_alone() {
    let parse = |words: &[&str]| parse_monitor_args(&args(words));
    let alone = parse(&["127.0.0.1:9411", "--apm", "--reset-routes", "--key", "k"]).unwrap();
    assert!(alone.apm_reset_routes && conflicts(&alone, true).is_empty());
    for extra in [&["--live"][..], &["--store", "/tmp/x"], &["--serve", "127.0.0.1:0"], &["--alert", "rps:*>=1"]] {
        let mut words = vec!["127.0.0.1:9411", "--apm", "--reset-routes"];
        words.extend_from_slice(extra);
        let error = parse(&words).err().unwrap_or_default();
        assert!(error.contains("give it the service alone") && error.contains(extra[0]), "{extra:?}: {error}");
    }
    let browse = parse(&["--apm", "--reset-routes"]).err().unwrap_or_default();
    assert!(browse.contains("give it the service alone"), "{browse}");
    let without = parse(&["127.0.0.1:9411", "--reset-routes"]).err().unwrap_or_default();
    assert!(without.contains("only applies to --apm"), "{without}");
    let script = parse(&["./console", "--apm", "--reset-routes"]).unwrap();
    assert_eq!(conflicts(&script, false), vec!["--reset-routes"]);
}

/// The page's reset is a POST from its own origin, like a capture, and a page
/// browsing a store has no service to reset.
#[test]
fn the_page_resets_routes_only_by_a_same_origin_post() {
    let shared: Shared = Arc::new((Mutex::new(History::new("stored history", None, false)), Condvar::new()));
    let (addr, _) = dashboard::serve("127.0.0.1:0", shared).unwrap();
    assert!(http_get(addr, "/api/reset-routes").starts_with("HTTP/1.1 405"));
    let foreign = raw_request(
        addr,
        &format!("POST /api/reset-routes HTTP/1.1\r\nHost: {addr}\r\nOrigin: http://attacker.example\r\n\r\n"),
    );
    assert!(foreign.starts_with("HTTP/1.1 403"), "{foreign}");
    let own = raw_request(addr, &format!("POST /api/reset-routes HTTP/1.1\r\nHost: {addr}\r\nOrigin: http://{addr}\r\n\r\n"));
    assert!(own.starts_with("HTTP/1.1 502") && own.contains("no service to reset"), "{own}");
}

/// A day file deleted and written again is read from its start even when the
/// new one is longer than what was read of the old one (review).
#[test]
fn a_rewritten_day_file_is_read_again() {
    let dir = scratch("rewritten");
    let store = Store::open(dir.to_str().unwrap(), 7).unwrap();
    let now = store::now_ms() / 86_400_000 * 86_400_000 + 43_200_000; // noon today: one day file whenever it runs
    let window = |t: u64, n: u64| Window {
        build: String::new(),
        res: None,
        errors: Vec::new(),
        t,
        span_ms: 1_000,
        epoch: 1,
        source: "a:1".into(),
        routes: vec![RouteWindow { route: "GET /".into(), n, status: [0, n, 0, 0, 0], sum_ns: n, hist: vec![(0, n)], io: Io::default(), mem: Mem::default(), exits: Vec::new() }],
    };
    store.append(&window(now - 1_000, 1)).unwrap();
    assert_eq!(store.load(now - 10_000, now).len(), 1);
    let path = dir.join(file_name(now / 86_400_000));
    std::fs::remove_file(&path).unwrap();
    for index in 0..3 {
        store.append(&window(now - 3_000 + index * 1_000, 5)).unwrap();
    }
    let loaded = store.load(now - 10_000, now);
    let counts: Vec<u64> = loaded.iter().map(|w| w.routes[0].n).collect();
    assert_eq!(counts, vec![5, 5, 5], "the new file, from its first line");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A day file written again under the same inode — what Linux does when it
/// reuses a freed one at once, which failed CI — is read from its start: its
/// identity alone cannot tell the new file from the old.
#[test]
fn a_day_file_rewritten_under_the_same_inode_is_read_again() {
    let dir = scratch("same-inode");
    let store = Store::open(dir.to_str().unwrap(), 7).unwrap();
    let now = store::now_ms() / 86_400_000 * 86_400_000 + 43_200_000; // noon today: one day file whenever it runs
    let window = |t: u64, n: u64| Window {
        build: String::new(),
        res: None,
        errors: Vec::new(),
        t,
        span_ms: 1_000,
        epoch: 1,
        source: "a:1".into(),
        routes: vec![RouteWindow { route: "GET /".into(), n, status: [0, n, 0, 0, 0], sum_ns: n, hist: vec![(0, n)], io: Io::default(), mem: Mem::default(), exits: Vec::new() }],
    };
    store.append(&window(now - 1_000, 1)).unwrap();
    assert_eq!(store.load(now - 10_000, now).len(), 1);
    // Truncated and written again in place: the same file, other lines.
    let path = dir.join(file_name(now / 86_400_000));
    let lines: String = (0..3u64)
        .map(|index| serde_json::to_string(&window(now - 3_000 + index * 1_000, 5)).unwrap() + "\n")
        .collect();
    std::fs::write(&path, lines).unwrap();
    let counts: Vec<u64> = store.load(now - 10_000, now).iter().map(|w| w.routes[0].n).collect();
    assert_eq!(counts, vec![5, 5, 5], "the new content, from its first line");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `--listen` receives what programs send: it takes --apm, no service, and
/// none of what only a service can be asked for.
#[test]
fn listen_receives_without_a_service() {
    let parse = |words: &[&str]| parse_monitor_args(&args(words));
    let listening = parse(&["--apm", "--listen", "0.0.0.0:9412", "--key", "k", "--store", "/tmp/x", "--serve", "127.0.0.1:0"]).unwrap();
    assert_eq!(listening.apm_listen.as_deref(), Some("0.0.0.0:9412"));
    assert!(conflicts(&listening, true).is_empty());
    let cases: [(&[&str], &str); 5] = [
        (&["--listen", "0.0.0.0:9412"], "needs --apm"),
        (&["127.0.0.1:9411", "--apm", "--listen", "0.0.0.0:9412"], "reads no service"),
        (&["--apm", "--listen", "nowhere"], "expected HOST:PORT"),
        (&["--apm", "--listen", "0.0.0.0:9412", "--capture-every", "5"], "--capture-every asks a service"),
        (&["--apm", "--listen"], "--listen needs a host:port"),
    ];
    for (words, expected) in cases {
        let error = parse(words).err().unwrap_or_default();
        assert!(error.contains(expected), "{words:?}: {error}");
    }
}

/// A source's first snapshot is counted whole when the program had sent
/// nothing before, and is where counting starts otherwise; later ones are
/// what happened since the one before.
#[test]
fn a_pushed_snapshot_counts_from_the_programs_start_only_once() {
    let epoch = 1_000_000_000_000_000_000;
    let start_ms = epoch / 1_000_000;
    let first = snapshot(epoch, start_ms + 2_000, vec![route("CALL render", &[(100, 200), (200, 500)])]);
    let pushed = |snapshot: &Snapshot, from_start: bool| collector::Pushed {
        source: "app@10.0.0.7 pid 42".to_string(),
        snapshot: snapshot.clone(),
        received_ms: 7_000,
        from_start,
    };
    let whole = collector::pushed_window(None, &pushed(&first, true)).expect("a short run is counted whole");
    assert_eq!(whole.routes[0].n, 2);
    assert_eq!(whole.source, "app@10.0.0.7 pid 42");
    assert_eq!(whole.t, 7_000, "stamped with the collector's clock");
    assert!(collector::pushed_window(None, &pushed(&first, false)).is_none(), "a restarted collector starts from it");
    let second = snapshot(epoch, start_ms + 7_000, vec![route("CALL render", &[(100, 200), (200, 500), (300, 200)])]);
    let since = collector::pushed_window(Some(&first), &pushed(&second, false)).expect("a window");
    assert_eq!(since.routes[0].n, 1);
}

/// A listening collector runs in memory too: `--live` or `--serve` without
/// `--store` (review).
#[test]
fn listen_runs_without_a_store() {
    for viewer in [&["--live"][..], &["--serve", "127.0.0.1:0"]] {
        let mut words = vec!["--apm", "--listen", "127.0.0.1:9412", "--key", "k"];
        words.extend_from_slice(viewer);
        assert!(parse_monitor_args(&args(&words)).is_ok(), "{words:?}");
    }
}

/// A pushing program's source name is told apart from a service's address,
/// so a listening collector's page reads its programs' stored windows back.
#[test]
fn pushed_sources_are_recognised() {
    let pushed = collector::source_name("worker", "10.0.0.7", 42, 0xdead_beef_0000_0001);
    assert!(collector::is_pushed_source(&pushed), "{pushed}");
    // Two containers running one build as pid 1 behind one address (review).
    assert_ne!(
        collector::source_name("worker", "10.0.0.7", 1, 0x1111_2222_0000_0000),
        collector::source_name("worker", "10.0.0.7", 1, 0x3333_4444_0000_0000),
    );
    // Two instances sharing their high half, as a clock-seeded one started in the same few
    // seconds does, stay apart too (#2499).
    assert_ne!(
        collector::source_name("worker", "10.0.0.7", 1, 0x1111_2222_0000_0001),
        collector::source_name("worker", "10.0.0.7", 1, 0x1111_2222_0000_0002),
    );
    for service in ["127.0.0.1:9411", "/tmp/shop.sock", "", "a@b pid x", "user@host"] {
        assert!(!collector::is_pushed_source(service), "{service}");
    }
    assert!(SourceFilter::Pushed.keeps(&pushed));
    assert!(!SourceFilter::Pushed.keeps("127.0.0.1:9411"));
    assert!(SourceFilter::Service("127.0.0.1:9411".to_string()).keeps("127.0.0.1:9411"));
    assert!(!SourceFilter::Service("127.0.0.1:9411".to_string()).keeps(&pushed));
    assert!(SourceFilter::Any.keeps(&pushed));
}

/// A source silent past the reconnect window is forgotten; one that sent
/// recently is kept (review: a job per pid grew the collector forever).
#[test]
fn silent_sources_are_forgotten() {
    let now = Instant::now() + collector::SOURCE_FORGOTTEN_AFTER * 2;
    let mut previous = std::collections::HashMap::new();
    previous.insert("old@h pid 1".to_string(), (snapshot(1, 0, vec![]), Instant::now()));
    previous.insert("new@h pid 2".to_string(), (snapshot(1, 0, vec![]), now));
    assert_eq!(collector::expired_sources(&previous, now), vec!["old@h pid 1".to_string()]);
}

/// Several programs pushing to one collector each get their own `/metrics`
/// series, labelled with their source (review).
#[test]
fn metrics_keep_each_pushed_program_apart() {
    let shared: Shared = Arc::new((Mutex::new(History::new("listening", None, true)), Condvar::new()));
    {
        let mut history = shared.0.lock().unwrap();
        history.pushed = true;
        history.latest_by_source.insert("a@h pid 1".to_string(), snapshot(7, 0, vec![route("MSG A", &[(10, 200)])]));
        history.latest_by_source.insert("b@h pid 2".to_string(), snapshot(8, 0, vec![route("MSG B", &[(10, 500), (10, 200)])]));
    }
    let text = metrics_text(&shared);
    assert!(text.contains("elephc_apm_requests_total{source=\"a@h pid 1\",route=\"MSG A\",class=\"2xx\"} 1"), "{text}");
    assert!(text.contains("elephc_apm_requests_total{source=\"b@h pid 2\",route=\"MSG B\",class=\"5xx\"} 1"), "{text}");
    assert_eq!(text.matches("# TYPE elephc_apm_requests_total").count(), 1, "one family header: {text}");
    assert!(text.contains("elephc_apm_service_start_seconds{source=\"b@h pid 2\"}"), "{text}");
    assert!(text.contains("# requests since start: 3"), "{text}");
}

/// With several programs pushing, memory joins the store per program: one
/// program's stored window crossing into memory must not hide another's
/// memory windows (review).
#[test]
fn pushed_programs_join_the_store_each_on_their_own() {
    let dir = scratch("history-pushed");
    let store = Store::open(dir.to_str().unwrap(), 30).unwrap();
    let base = store::now_ms() - 120_000;
    let (a, b) = (collector::source_name("a", "h", 1, 1 << 40), collector::source_name("b", "h", 2, 2 << 40));
    let window = |source: &str, t: u64, span_ms: u64, requests: usize| Window {
        build: String::new(),
        res: None,
        errors: Vec::new(),
        t,
        span_ms,
        epoch: 1,
        source: source.to_string(),
        routes: vec![RouteWindow::from_stats(&route("MSG X", &vec![(100, 200); requests]))],
    };
    // A's stored record runs from 50 s to 60 s, past where memory begins (55 s).
    store.append(&window(&a, base + 60_000, 10_000, 10)).unwrap();
    let mut history = History::new("listening", Some(store), true);
    history.pushed = true;
    for second in 56..=70u64 {
        history.push(window(&a, base + second * 1_000, 1_000, 1));
    }
    // B sent only from memory, before A's stored record ends.
    history.push(window(&b, base + 57_500, 1_000, 3));
    let windows = history.windows_in(base, base + 70_000);
    let of = |source: &str| -> u64 {
        windows.iter().filter(|w| w.source == source).flat_map(|w| &w.routes).map(|r| r.n).sum()
    };
    assert_eq!(of(&b), 3, "B's memory window is kept: {:?}", windows.iter().map(|w| (&w.source, w.t - base)).collect::<Vec<_>>());
    assert_eq!(of(&a), 20, "A: its stored record, then its memory after it");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A listening collector whose last program was forgotten exports nothing,
/// not that program's counters again without their label (review).
#[test]
fn metrics_of_a_listener_with_no_program_are_empty() {
    let shared: Shared = Arc::new((Mutex::new(History::new("listening", None, true)), Condvar::new()));
    {
        let mut history = shared.0.lock().unwrap();
        history.pushed = true;
        history.latest = Some(snapshot(7, 0, vec![route("MSG A", &[(10, 200)])]));
    }
    assert_eq!(metrics_text(&shared), "# no snapshot yet\n");
}

/// A listening collector's memory holds an hour of every program it serves,
/// not a count they share: with 256 programs pushing every five seconds, a
/// 3 600-window cap kept about a minute of each, and an hour-long alert
/// window forgot the rest (review).
#[test]
fn memory_keeps_an_hour_of_every_pushed_program() {
    let mut history = History::new("listen", None, false);
    let base = 1_000_000_000u64;
    let window = |t: u64, source: usize| Window {
        build: String::new(),
        res: None,
        errors: Vec::new(),
        t,
        span_ms: 5_000,
        epoch: 1,
        source: format!("job@127.0.0.1 pid {source}"),
        routes: Vec::new(),
    };
    for step in 0..720u64 {
        for source in 0..256 {
            history.push(window(base + step * 5_000, source));
        }
    }
    assert_eq!(history.windows.len(), 256 * 720, "the whole hour of every program");
    assert_eq!(history.windows.front().unwrap().t, base);
    for source in 0..256 {
        history.push(window(base + 720 * 5_000, source));
    }
    assert_eq!(history.windows.front().unwrap().t, base + 5_000, "older than the hour falls out");
    assert_eq!(history.windows.len(), 256 * 720);
}

/// An hour-long budget keeps the hour's first failures however many
/// short-lived jobs send counters after them: each job is its own source and
/// sends a window as it exits, enough to push the failures out of memory's
/// ceiling, but the budgets measure their own folded hour (review).
#[test]
fn an_hour_long_budget_survives_many_short_lived_jobs() {
    let cmd = parse_monitor_args(&args(&[
        "127.0.0.1:9411",
        "--apm",
        "--live",
        "--alert",
        "error_pct:*<=0.01",
        "--alert-window",
        "3600",
    ]))
    .unwrap();
    let mut history = History::new("listen", None, true);
    history.alerts = alerts::Alerts::new(cmd.apm_alerts.iter().map(|spec| alerts::parse_rule(spec).unwrap()).collect());
    let failed = route("CLI job.php", &(0..100).map(|_| (1_000, 500)).collect::<Vec<_>>());
    let fine = route("CLI job.php", &[(1_000, 200)]);
    let start = 50_000_000u64;
    let job = |t: u64, pid: u64, stats: &RouteStats| Window {
        build: String::new(),
        res: None,
        errors: Vec::new(),
        t,
        span_ms: 200,
        epoch: 1,
        source: format!("job@127.0.0.1 pid {pid} #{:08x}", pid),
        routes: vec![RouteWindow::from_stats(stats)],
    };
    history.push(job(start + 1_000, 1, &failed));
    let mut pid = 2;
    for second in 2..3_300u64 {
        for slot in 0..60u64 {
            history.push(job(start + second * 1_000 + slot * 10, pid, &fine));
            pid += 1;
        }
    }
    let now = start + 3_300 * 1_000;
    assert!(
        history.windows.front().unwrap().t > start + 1_000,
        "memory's ceiling dropped the failures: the case this covers"
    );
    super::collector::check_alerts(&mut history, &cmd, now, false);
    let alert = &history.alerts.alerts[0];
    let expected = 100.0 * 100.0 / (100.0 + 3_298.0 * 60.0);
    assert!(alert.firing, "the failures still count: {alert:?}");
    assert!((alert.value.unwrap() - expected).abs() < 1e-9, "{alert:?} vs {expected}");
    assert!(history.alert_windows_held <= 2 * 3_300, "a couple of windows a second: {}", history.alert_windows_held);
}

/// Two short jobs in the same second with a gap between them measure the
/// time they covered, not the gap: folding 100–200 ms and 800–900 ms into one
/// 100–900 ms window read their throughput as 2.5/s instead of 10/s (review).
#[test]
fn folding_keeps_the_gaps_between_short_jobs() {
    let cmd = parse_monitor_args(&args(&["127.0.0.1:9411", "--apm", "--live", "--alert", "rps:*>=5", "--alert-window", "60"]))
        .unwrap();
    let mut history = History::new("listen", None, true);
    history.alerts = alerts::Alerts::new(cmd.apm_alerts.iter().map(|spec| alerts::parse_rule(spec).unwrap()).collect());
    let one = route("CLI job.php", &[(1_000, 200)]);
    let start = 50_000_000u64;
    let job = |t: u64, pid: u64| Window {
        build: String::new(),
        res: None,
        errors: Vec::new(),
        t,
        span_ms: 100,
        epoch: 1,
        source: format!("job@127.0.0.1 pid {pid} #{pid:08x}"),
        routes: vec![RouteWindow::from_stats(&one)],
    };
    history.push(job(start + 200, 1));
    history.push(job(start + 900, 2));
    // A third one overlapping the first folds with it.
    history.push(job(start + 250, 3));
    assert_eq!(history.alert_windows_held, 2, "the overlapping ones fold, the gap keeps the other apart");
    super::collector::check_alerts(&mut history, &cmd, start + 1_000, false);
    let alert = &history.alerts.alerts[0];
    assert!(!alert.firing, "{alert:?}");
    assert!((alert.value.unwrap() - 3.0 / 0.25).abs() < 1e-9, "three jobs over the 250 ms they covered: {alert:?}");
}

/// A day file dated far ahead — a writer whose clock runs years fast — does
/// not anchor the store's views to a future with nothing in it (review).
#[test]
fn a_future_day_file_does_not_anchor_the_store() {
    let dir = scratch("future-day");
    let store = Store::open(dir.to_str().unwrap(), 7).unwrap();
    let now = store::now_ms();
    let window = |t: u64| Window { build: String::new(), res: None, errors: Vec::new(), t, span_ms: 1_000, epoch: 1, source: String::new(), routes: Vec::new() };
    store.append(&window(now - 5_000)).unwrap();
    store.append(&window(now + 400 * 86_400_000)).unwrap();
    assert_eq!(store.newest_ms(), Some(now - 5_000));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Today's read cache folds its past hours by the minute once it is large: a
/// listening collector's two million windows a day were all kept fine, and
/// walked by every read (review). The last hour keeps its resolution, and
/// the counts stay whole.
#[test]
fn todays_past_hours_fold_once_the_cache_is_large() {
    let dir = scratch("today-fold");
    let store = Store::open(dir.to_str().unwrap(), 7).unwrap();
    let now = store::now_ms();
    // The test needs two hours of today behind it.
    if now % 86_400_000 < 3 * 3_600_000 {
        return;
    }
    let one = route("GET /", &[(1_000, 200)]);
    let window = |t: u64| Window {
        build: String::new(),
        res: None,
        errors: Vec::new(),
        t,
        span_ms: 1_000,
        epoch: 1,
        source: "svc".into(),
        routes: vec![RouteWindow::from_stats(&one)],
    };
    let old = now - 2 * 3_600_000;
    for second in 0..150 {
        store.append(&window(old + second * 1_000)).unwrap();
    }
    for second in 0..10 {
        store.append(&window(now - 60_000 + second * 1_000)).unwrap();
    }
    let loaded = store.load(now - 3 * 3_600_000, now);
    let total: u64 = loaded.iter().flat_map(|w| &w.routes).map(|r| r.n).sum();
    assert_eq!(total, 160, "every request is kept");
    let recent = loaded.iter().filter(|w| w.t >= now - 60_000).count();
    assert_eq!(recent, 10, "the last hour keeps its resolution");
    assert!(loaded.len() <= 10 + 4, "the past hour is folded by the minute: {} windows", loaded.len());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A collector's prune keeps what another live collector of the store was
/// told to keep: started with the default, it deleted the longer history of a
/// collector of another service sharing the store (review). Once that one is
/// gone, its claim keeps nothing.
#[test]
fn a_prune_keeps_a_live_collectors_longer_retention() {
    let dir = scratch("shared-retention");
    let long = Store::open(dir.to_str().unwrap(), 30).unwrap();
    let short = Store::open(dir.to_str().unwrap(), 2).unwrap();
    let today = store::now_ms() / 86_400_000;
    let old = dir.join(file_name(today - 10));
    std::fs::write(&old, "").unwrap();
    let claim = long.claim_collector("127.0.0.1:9411").unwrap();
    let _own = short.claim_collector("127.0.0.1:9412").unwrap();
    short.prune_now();
    assert!(old.exists(), "the live collector keeps thirty days");
    drop(claim);
    short.prune_now();
    assert!(!old.exists(), "its claim is gone, and so is its retention");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A collector's roll-ups keep the finest resolution another live collector of the store
/// asked for, and only one collector at a time rewrites the store (review).
#[test]
fn roll_ups_keep_a_live_collectors_finer_tiers() {
    let dir = scratch("shared-tiers");
    let fine = Store::open(dir.to_str().unwrap(), 30).unwrap().with_tiers(30, 30);
    let coarse = Store::open(dir.to_str().unwrap(), 30).unwrap().with_tiers(2, 7);
    let today = store::now_ms() / 86_400_000;
    let day = today - 10;
    let window = |t: u64| Window { build: String::new(), res: None, errors: Vec::new(), t, span_ms: 1_000, epoch: 1, source: "a:1".into(), routes: vec![RouteWindow::from_stats(&route("GET /", &[(1, 200)]))] };
    for second in 0..30u64 {
        coarse.append(&window(day * 86_400_000 + 3_600_000 + second * 1_000)).unwrap();
    }
    let lines = || std::fs::read_to_string(dir.join(file_name(day))).unwrap().lines().count();
    let claim = fine.claim_collector("127.0.0.1:9411").unwrap();
    let busy = coarse.maintenance_lock_for_test().expect("free");
    coarse.prune_now();
    assert_eq!(lines(), 30, "busy: another collector is at it");
    drop(busy);
    coarse.prune_now();
    assert_eq!(lines(), 30, "the live collector keeps thirty days at full resolution");
    drop(claim);
    // The lock files go too: a test elsewhere in this binary that forks without exec
    // inherits their locks, which then outlive the drops and made this flaky.
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        if entry.file_name().to_string_lossy().ends_with(".lock") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    let _ = std::fs::remove_file(dir.join("rollups.json"));
    coarse.prune_now();
    assert_eq!(lines(), 1, "its claim gone, the day folds by the hour");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A line a crash cut short costs only itself: the next window appended
/// starts on a line of its own instead of being read as part of the damaged
/// one and lost with it (review).
#[test]
fn a_torn_line_loses_only_itself() {
    let dir = scratch("torn-tail");
    let store = Store::open(dir.to_str().unwrap(), 7).unwrap();
    let now = store::now_ms();
    let window = |t: u64| Window { build: String::new(), res: None, errors: Vec::new(), t, span_ms: 1_000, epoch: 1, source: String::new(), routes: Vec::new() };
    store.append(&window(now - 3_000)).unwrap();
    let file = dir.join(file_name(now / 86_400_000));
    let mut text = std::fs::read_to_string(&file).unwrap();
    text.push_str("{\"t\":12,\"span");
    std::fs::write(&file, text).unwrap();
    store.append(&window(now - 1_000)).unwrap();
    let times: Vec<u64> = store.load(now - 10_000, now).iter().map(|w| w.t).collect();
    assert_eq!(times, vec![now - 3_000, now - 1_000], "only the torn line is lost");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A call graph with one function, for the capture tests.
fn capture_graph(name: &str) -> crate::call_graph::CallGraph {
    crate::monitor::build_call_graph(&[(vec![(name.to_string(), crate::monitor::Kind::Php)], 3)])
}

/// Each route keeps its slowest and failed captures per hour, so any period has examples;
/// an hour that is over is handed to the store once, and a stop hands the rest.
#[test]
fn captures_are_kept_per_hour_and_stored_once_the_hour_is_over() {
    let mut book = traces::TraceBook::default();
    let hour = 3_600_000u64;
    let base = 500_000 * hour;
    // A bad first hour: six slow requests.
    for i in 0..6u64 {
        book.offer("GET /a".into(), Some(200), (100 + i) * 1_000_000_000, capture_graph("slow"), base + i);
    }
    // A quiet second hour: fast ones, still kept, since they are that hour's slowest.
    assert!(book.offer("GET /a".into(), Some(200), 1_000_000, capture_graph("fast"), base + hour + 1));
    let per_hour = |book: &traces::TraceBook, from: u64| {
        book.list().iter().filter(|t| t.t_ms >= from && t.t_ms < from + hour).count()
    };
    assert_eq!(per_hour(&book, base), 5, "five slowest of the first hour");
    assert_eq!(per_hour(&book, base + hour), 1, "the second hour keeps its own");
    let due: Vec<u64> = book.unwritten_hours(base + hour + 10).iter().map(|t| t.t_ms).collect();
    assert_eq!(due.len(), 5, "the first hour is over");
    assert!(book.unwritten_hours(base + hour + 20).is_empty(), "and handed over once");
    assert_eq!(book.unwritten_hours(u64::MAX).len(), 1, "a stop hands the current hour too");
}

/// With a store, the cap drops only captures it saved: forty-one routes of five captures
/// each pass it within one hour, before that hour is saved (review). A save that failed is
/// handed over again.
#[test]
fn the_cap_keeps_unsaved_captures_and_a_failed_save_retries() {
    let mut book = traces::TraceBook::stored(true);
    let base = 500_000 * 3_600_000u64;
    for route in 0..41u64 {
        for i in 0..5u64 {
            book.offer(format!("GET /{route}"), Some(200), 1_000_000 + i, capture_graph("x"), base + route * 10 + i);
        }
    }
    assert_eq!(book.list().len(), 205, "none unsaved is dropped");
    let due: Vec<u64> = book.unwritten_hours(u64::MAX).iter().map(|t| t.id).collect();
    assert_eq!(due.len(), 205);
    book.unsaved(&due[..1]);
    assert_eq!(book.unwritten_hours(u64::MAX).len(), 1, "the failed save is retried");
    book.offer("GET /late".into(), Some(200), 1, capture_graph("x"), base + 9_999);
    assert!(book.list().len() <= 201, "saved ones make room: {}", book.list().len());
    let mut memory = traces::TraceBook::default();
    for route in 0..41u64 {
        for i in 0..5u64 {
            memory.offer(format!("GET /{route}"), Some(200), 1_000_000 + i, capture_graph("x"), base + route * 10 + i);
        }
    }
    assert_eq!(memory.list().len(), 200, "without a store, memory stays bounded");
    // A store that keeps failing: the unsaved stay bounded too (review).
    let mut failing = traces::TraceBook::stored(true);
    for route in 0..230u64 {
        for i in 0..5u64 {
            failing.offer(format!("GET /{route}"), Some(200), 1_000_000 + i, capture_graph("x"), base + route * 10 + i);
        }
    }
    assert_eq!(failing.list().len(), 1_000, "at most a thousand unsaved");
}

/// A period picked on the page is absolute and kept within the longest range; without one,
/// the span is the last `range` seconds.
#[test]
fn a_requested_span_is_absolute_or_relative() {
    assert_eq!(api::requested_span("range=300", 1_000_000), (700_000, 1_000_000));
    assert_eq!(api::requested_span("from=5000&to=9000", 1_000_000), (5_000, 9_000));
    assert_eq!(api::requested_span("from=9000&to=5000", 1_000_000), (5_000, 9_000), "put in order");
    let (from, to) = api::requested_span("from=0&to=10000000000000", 1);
    assert_eq!(to - from, 90 * 86_400_000, "kept within the longest range");
}

/// The captures of a period come from memory and from the store, each once, slowest first,
/// and a stored capture's page opens from the store when memory no longer holds it.
#[test]
fn captures_of_a_period_come_from_memory_and_the_store() {
    let dir = scratch("captures-period");
    let store = Store::open(dir.to_str().unwrap(), 30).unwrap();
    let now = store::now_ms();
    let day = 86_400_000u64;
    let old = captures::CaptureRecord {
        id: captures::capture_id(now - 3 * day, 1),
        t: now - 3 * day,
        kind: "trace".into(),
        route: "GET /orders/{id}".into(),
        status: Some(500),
        duration_ns: 9_000_000_000,
        functions: 1,
        queries: 4,
        source: String::new(),
    };
    store.append_capture(&old, &capture_graph("checkout_three_days_ago")).unwrap();
    // Another service sharing the store: its capture is not this page's (review).
    let theirs = captures::CaptureRecord { id: captures::capture_id(now - 3 * day + 5, 1), source: "10.0.0.9:9411".into(), ..old.clone() };
    store.append_capture(&theirs, &capture_graph("their_checkout")).unwrap();
    let mut history = History::new("127.0.0.1:9411", Some(store), true);
    assert!(history.traces.offer("GET /".into(), Some(200), 1_000_000, capture_graph("home"), now - 1_000));
    let shared: Shared = Arc::new((Mutex::new(history), Condvar::new()));
    let span = |from: u64, to: u64| -> Vec<u64> {
        api::traces_json(&shared, &format!("from={from}&to={to}"))["traces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|trace| trace["id"].as_u64().unwrap())
            .collect()
    };
    assert_eq!(span(now - 4 * day, now - 2 * day), vec![old.id], "the stored one, by date");
    let both = span(now - 4 * day, now);
    assert_eq!(both.len(), 2, "{both:?}");
    assert_eq!(both[0], old.id, "slowest first");
    let page = api::page_html(&shared, Some(old.id));
    assert!(page.contains("checkout_three_days_ago"), "its graph opens from the store");
    assert!(!api::page_html(&shared, Some(theirs.id)).contains("their_checkout"), "nor does theirs open here");
    let _ = std::fs::remove_dir_all(&dir);
}
