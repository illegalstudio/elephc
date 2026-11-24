//! Purpose:
//! Tests for the runtime half of transaction monitoring (`--apm`): what the
//! request path records into the shared table, what it must not do when the
//! service was started without the flag, and what it costs per request.
//!
//! Called from:
//! - `cargo test -p elephc-probe`.
//!
//! Key details:
//! - The tests stand a heap buffer in for the shared mapping and serialize on
//!   the route-test lock, because `REGION`, `CURRENT_ROUTE` and `APM_ON` are
//!   process globals.
//! - A counting global allocator measures allocations per thread, so the cost
//!   test is exact and load-independent rather than a timing.

use super::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

/// Pass-through allocator that counts allocations made by the current thread.
struct CountingAllocator;

thread_local! {
    /// Allocations this thread has made. Const-initialized and destructor-free,
    /// so reading it from inside the allocator cannot recurse into allocation.
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for CountingAllocator {
    /// Counts the allocation on this thread, then defers to the system allocator.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        System.alloc(layout)
    }

    /// Frees through the system allocator; frees are not counted.
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }

    /// Counts the reallocation on this thread, then defers to the system allocator.
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Allocations the current thread has made so far.
fn allocations() -> u64 {
    ALLOCATIONS.with(Cell::get)
}

/// A stand-in shared region with transaction counting switched on, restored to
/// the dormant state when dropped so the other route tests see what they expect.
struct ApmFixture {
    _region: Vec<u64>,
    was_armed: i32,
}

impl ApmFixture {
    /// Maps a zeroed region of the probe's layout, turns APM on and claims the
    /// sampling timer for this process, so the probe counts into memory the test
    /// owns and never arms a real SIGPROF.
    fn new() -> Self {
        // u64 elements so the buffer has the alignment the atomics need.
        let mut region = vec![0u64; REGION_BYTES / 8 + 1];
        REGION.store(region.as_mut_ptr() as usize, Ordering::Relaxed);
        // Claim the timer for this pid so `set_route` never arms a real SIGPROF
        // in a test binary that installed no handler for it.
        let was_armed = ARMED_PID.swap(unsafe { libc::getpid() }, Ordering::Relaxed);
        APM_EPOCH.store(42, Ordering::Relaxed);
        APM_ON.store(true, Ordering::Release);
        Self { _region: region, was_armed }
    }
}

impl Drop for ApmFixture {
    /// Leaves the probe's process-wide state as a fresh process has it, so the
    /// next test starts from nothing: APM off, no route, no pending or in-flight
    /// request, the region detached and the timer claim it found put back.
    fn drop(&mut self) {
        APM_ON.store(false, Ordering::Release);
        APM_EPOCH.store(0, Ordering::Relaxed);
        CURRENT_ROUTE.store(0, Ordering::Relaxed);
        TRACK_IN_FLIGHT.store(false, Ordering::Relaxed);
        PENDING_LEN.store(0, Ordering::Relaxed);
        TXN_STARTED_NS.store(0, Ordering::Relaxed);
        GENERATION_SEEN.store(0, Ordering::Relaxed);
        CPU_AT_RESET_NS.store(0, Ordering::Relaxed);
        TXN_FAILED.store(false, Ordering::Relaxed);
        NESTED_DEPTH.store(0, Ordering::Relaxed);
        crate::apm_rules::set_request_ignored(false);
        RUN_STARTED_NS.store(0, Ordering::Relaxed);
        for counter in &RUN_IO {
            counter.store(0, Ordering::Relaxed);
        }
        CURRENT_LABEL_LEN.store(0, Ordering::Relaxed);
        for counter in &PENDING_IO {
            counter.store(0, Ordering::Relaxed);
        }
        INFLIGHT_SLOT.store(usize::MAX, Ordering::Relaxed);
        INFLIGHT_PID.store(0, Ordering::Relaxed);
        ARMED_PID.store(self.was_armed, Ordering::Relaxed);
        REGION.store(0, Ordering::Relaxed);
    }
}

/// Runs one request the way the `--web` bridge does: stamp the route, then
/// record its outcome against the stamped route.
unsafe fn serve(route: &str, status: u32, duration_ns: u64) {
    elephc_probe_set_route(route.as_ptr(), route.len());
    elephc_probe_note_transaction(std::ptr::null(), 0, status, duration_ns);
    elephc_probe_set_route(std::ptr::null(), 0);
}

/// Under `--apm`, requests are counted per route from the first one, with no
/// operator connected: that is the difference between monitoring and profiling.
#[test]
fn transactions_are_counted_per_route_without_an_ask() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    assert!(!ASKED.load(Ordering::Relaxed), "nobody asked to profile");
    unsafe {
        serve("GET /users/{id}", 200, 1_200_000);
        serve("GET /users/{id}", 200, 800_000);
        serve("GET /users/{id}", 503, 30_000_000);
        serve("POST /checkout", 302, 5_000_000);
        // Answered before PHP ran (a 413), so the bridge names the route itself.
        let route = "POST /upload";
        elephc_probe_note_transaction(route.as_ptr(), route.len(), 413, 50_000);
    }
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    assert_eq!(snapshot.epoch, 42);
    let find = |name: &str| {
        snapshot
            .routes
            .iter()
            .find(|route| route.name == name)
            .unwrap_or_else(|| panic!("no row for {name}: {snapshot:?}"))
    };
    let users = find("GET /users/{id}");
    assert_eq!(users.count, 3);
    assert_eq!(users.status, [0, 2, 0, 0, 1]);
    assert_eq!(users.sum_ns, 32_000_000);
    assert_eq!(users.hist.iter().sum::<u64>(), 3);
    assert_eq!(users.hist[apm::bucket_of(30_000_000)], 1);
    assert_eq!(find("POST /checkout").status, [0, 0, 1, 0, 0]);
    assert_eq!(find("POST /upload").status, [0, 0, 0, 1, 0]);
    assert_eq!(snapshot.routes.len(), 3, "no row for routes that served nothing");
}

/// A request served while no route was stamped is counted, under `<untagged>`,
/// rather than dropped: a missing row would understate the service's traffic.
#[test]
fn an_unrouted_request_is_counted_as_untagged() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    unsafe { elephc_probe_note_transaction(std::ptr::null(), 0, 200, 1_000) };
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    assert_eq!(snapshot.routes.len(), 1);
    assert_eq!(snapshot.routes[0].name, "<untagged>");
    assert_eq!(snapshot.routes[0].count, 1);
}

/// Without `--apm`, recording is a no-op and the service answers that it
/// counts nothing — and the route table stays as empty as profiling needs it.
#[test]
fn without_apm_nothing_is_recorded_or_interned() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = ApmFixture::new();
    APM_ON.store(false, Ordering::Release);
    unsafe {
        serve("GET /", 200, 1_000);
        let route = "GET /explicit";
        elephc_probe_note_transaction(route.as_ptr(), route.len(), 200, 1_000);
    }
    assert_eq!(apm_snapshot_text(), apm::SNAPSHOT_OFF);
    assert_eq!(unsafe { route_name(1) }, None, "no route was interned");
    let base = REGION.load(Ordering::Relaxed);
    for route_id in 0..EVENT_BUCKETS {
        let count = unsafe { txn_word(base, route_id, apm::WORD_COUNT) }.load(Ordering::Relaxed);
        assert_eq!(count, 0, "route {route_id} counted a request");
    }
    drop(fixture);
}

/// Two rows that carry one name — two workers racing to intern the same route
/// into different slots — come out as one row with both rows' requests.
#[test]
fn duplicate_route_rows_merge_in_the_snapshot() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let base = REGION.load(Ordering::Relaxed);
    unsafe {
        write_route_slot(base, 0, b"GET /");
        write_route_slot(base, 1, b"GET /");
        (*((base + RING_BYTES) as *const AtomicU64)).store(2, Ordering::Release);
    }
    record_transaction(base, 1, 200, 1_000);
    record_transaction(base, 2, 500, 2_000);
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    assert_eq!(snapshot.routes.len(), 1, "{snapshot:?}");
    assert_eq!(snapshot.routes[0].count, 2);
    assert_eq!(snapshot.routes[0].status, [0, 1, 0, 0, 1]);
}

/// The transaction table follows the replay table and the in-flight table ends
/// the region: an offset computed one area early would land on the replay
/// table and corrupt spent signatures, which no counting test would notice.
#[test]
fn the_transaction_table_ends_the_region_without_overlap() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let base = REGION.load(Ordering::Relaxed);
    let replay_end = base
        + RING_BYTES
        + ROUTE_TABLE_BYTES
        + EVENT_TABLE_BYTES
        + CONTROL_BYTES
        + REPLAY_TABLE_BYTES;
    let first = unsafe { txn_word(base, 0, 0) } as *const _ as usize;
    let last = unsafe { txn_word(base, EVENT_BUCKETS - 1, apm::ROUTE_WORDS - 1) } as *const _
        as usize;
    assert_eq!(first, replay_end, "the table starts right after the replay table");
    let inflight_first = unsafe { inflight_word(base, 0, 0) } as *const _ as usize;
    let inflight_last = unsafe { inflight_word(base, INFLIGHT_SLOTS - 1, INFLIGHT_WORDS - 1) } as *const _ as usize;
    assert_eq!(last + 8, inflight_first, "the in-flight table follows it");
    assert_eq!(inflight_last + 8, base + ROUTE_BUDGET_OFFSET, "the new-route budget follows it");
    assert_eq!(ROUTES_GENERATION_OFFSET, ROUTE_BUDGET_OFFSET + 8, "then the route-table generation");
    assert_eq!(ROUTES_RESET_TICKET_OFFSET, ROUTES_GENERATION_OFFSET + 8, "then the reset ticket");
    assert_eq!(ERROR_TABLE_OFFSET, ROUTES_GATE_OFFSET + 8, "then the table gate");
    assert_eq!(ERROR_TABLE_OFFSET + crate::apm_errors::ERROR_TABLE_BYTES, REGION_BYTES, "and the error table ends the region");
}

/// Resetting the route table empties it — names, counters — starts a new run
/// of the counters and marks where the profiler's samples lose their route. A
/// request in flight across the reset holds an id the new table gives to
/// another route: its query and its answer land on its own route again, never
/// on that other route's row.
#[test]
fn a_reset_route_table_fills_again_and_keeps_requests_in_flight_apart() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let orders = "GET /orders";
    unsafe {
        serve(orders, 200, 1_000);
        for index in 0..40 {
            serve(&format!("GET /junk-{index}"), 200, 1_000);
        }
    }
    let base = REGION.load(Ordering::Relaxed);
    unsafe { (*(base as *const AtomicU64)).store(77, Ordering::Relaxed) };
    let epoch = APM_EPOCH.load(Ordering::Relaxed);
    // This process starts a request of `GET /orders` (route id 1)…
    unsafe { elephc_probe_set_route(orders.as_ptr(), orders.len()) };
    assert_eq!(CURRENT_ROUTE.load(Ordering::Relaxed), 1);
    // …the table is reset under it…
    assert_eq!(reset_routes(), "route table reset: 41 routes cleared\n");
    assert!(row_names().is_empty(), "nothing left: {:?}", row_names());
    assert_ne!(APM_EPOCH.load(Ordering::Relaxed), epoch, "a new run of the counters");
    assert_eq!(shared_word(base, ROUTES_RESET_TICKET_OFFSET).load(Ordering::Relaxed), 77);
    // …another worker's new route takes id 1…
    let fresh = "GET /fresh";
    unsafe { elephc_probe_note_transaction(fresh.as_ptr(), fresh.len(), 200, 1_000) };
    // …and the request in flight runs a query and ends.
    elephc_probe_note_io();
    unsafe {
        elephc_probe_note_transaction(std::ptr::null(), 0, 200, 1_000);
        elephc_probe_set_route(std::ptr::null(), 0);
    }
    let snapshot = apm::decode(&apm_snapshot_text()).unwrap();
    let count = |name: &str| snapshot.routes.iter().find(|r| r.name == name).map(|r| (r.count, r.io[apm::IO_DB_OPS]));
    assert_eq!(count(fresh), Some((1, 0)), "the other route keeps only its own request");
    assert_eq!(count(orders), Some((1, 1)), "the request in flight, with its query, on its own route");
}

/// Recording a request allocates nothing once its route is known, whichever
/// way the route arrives. This is the per-request cost `--apm` adds to a
/// service, so it is held to an exact count rather than a timing.
#[test]
fn recording_a_known_route_allocates_nothing() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    // Fill most of the table first, so the lookup really walks it.
    let names: Vec<String> = (0..200).map(|index| format!("GET /route{index}")).collect();
    for name in &names {
        unsafe { serve(name, 200, 1_000) };
    }
    let last = names.last().unwrap().as_str();
    let before = allocations();
    for _ in 0..1_000 {
        unsafe {
            serve(last, 200, 1_000);
            elephc_probe_note_transaction(last.as_ptr(), last.len(), 404, 2_000);
        }
    }
    assert_eq!(allocations() - before, 0, "the request path allocated");
}

/// Under `--apm` a request's database and network work is counted on its
/// route — through the probe's own slots (probe-only build) and through the
/// event the exact runtime forwards (combined build) — and the bridges are told
/// to time their calls from boot, with nobody profiling.
#[test]
fn a_routes_database_and_network_work_is_counted() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    assert_eq!(elephc_probe_event_active(), 1, "the bridges time their calls under --apm");
    let route = "GET /orders";
    unsafe {
        elephc_probe_set_route(route.as_ptr(), route.len());
        // Probe-only build: the bridges call the probe's own note functions.
        elephc_probe_note_io();
        elephc_probe_note_wait(2_000_000);
        // Combined build: the exact runtime forwards its events.
        elephc_probe_apm_event(apm::IO_DB_OPS as u32, 1);
        elephc_probe_apm_event(apm::IO_DB_WAIT as u32, 3_000_000);
        elephc_probe_apm_event(apm::IO_NET_OPS as u32, 1);
        elephc_probe_apm_event(apm::IO_NET_WAIT as u32, 5_000_000);
        // A kind this version does not know is ignored, not written out of row.
        elephc_probe_apm_event(99, 1);
        elephc_probe_note_transaction(std::ptr::null(), 0, 200, 12_000_000);
        elephc_probe_set_route(std::ptr::null(), 0);
    }
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    let orders = snapshot.routes.iter().find(|r| r.name == route).expect("the route's row");
    assert_eq!(orders.io, [2, 5_000_000, 1, 5_000_000]);
    assert_eq!(orders.count, 1);
}

/// Without `--apm` the event window stays the sampler's to open, and nothing
/// lands in the transaction table.
#[test]
fn without_apm_events_are_not_counted_as_transactions() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = ApmFixture::new();
    APM_ON.store(false, Ordering::Release);
    assert_eq!(elephc_probe_event_active(), 0, "nobody asked: the bridges stay untimed");
    elephc_probe_apm_event(apm::IO_DB_OPS as u32, 1);
    elephc_probe_note_io();
    let base = REGION.load(Ordering::Relaxed);
    for route_id in 0..EVENT_BUCKETS {
        let ops = unsafe { txn_word(base, route_id, apm::WORD_IO0 + apm::IO_DB_OPS) }.load(Ordering::Relaxed);
        assert_eq!(ops, 0, "route {route_id} counted a query");
    }
    drop(fixture);
}

/// The route of every row in a snapshot.
fn row_names() -> Vec<String> {
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    snapshot.routes.into_iter().map(|route| route.name).collect()
}

/// A scanner's paths cannot fill the route table under `--apm`: a request
/// answered "not found" is counted under one `<not found>` row and interns
/// nothing, and an endpoint first seen afterwards still gets its own row.
/// A route is interned once its request is answered.
#[test]
fn not_found_traffic_does_not_spend_the_route_table() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    for index in 0..400 {
        let route = format!("GET /scan-{index}.php");
        unsafe { serve(&route, 404, 1_000) };
    }
    let route = "GET /brand-new-endpoint";
    unsafe {
        elephc_probe_set_route(route.as_ptr(), route.len());
        assert_eq!(CURRENT_ROUTE.load(Ordering::Relaxed), 0, "not interned before its answer");
        elephc_probe_note_io();
        elephc_probe_note_transaction(std::ptr::null(), 0, 200, 2_000);
        elephc_probe_set_route(std::ptr::null(), 0);
    }
    let names = row_names();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"<not found>".to_string()) && names.contains(&route.to_string()));
    let snapshot = apm::decode(&apm_snapshot_text()).unwrap();
    let new = snapshot.routes.iter().find(|r| r.name == route).unwrap();
    assert_eq!((new.count, new.io[apm::IO_DB_OPS]), (1, 1), "its pending work landed on its row");
    let base = REGION.load(Ordering::Relaxed);
    let interned = unsafe { (*((base + RING_BYTES) as *const AtomicU64)).load(Ordering::Relaxed) };
    assert_eq!(interned, 2, "two slots used for 401 requests");
}

/// A route already in the table that answers "not found" — a product page
/// for an id that does not exist — keeps the request on its own row, with the
/// queries it ran: its 4xx rate and its queries per request stay its own. Only
/// a route the table has never seen goes to `<not found>`.
#[test]
fn a_known_route_answering_not_found_keeps_its_row() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let route = "GET /products/{id}";
    unsafe {
        serve(route, 200, 1_000);
        elephc_probe_set_route(route.as_ptr(), route.len());
        elephc_probe_note_io();
        elephc_probe_note_transaction(std::ptr::null(), 0, 404, 1_000);
        elephc_probe_set_route(std::ptr::null(), 0);
        // Served in another process, named at the end: same rule.
        elephc_probe_note_transaction(route.as_ptr(), route.len(), 404, 1_000);
        let unseen = "GET /never-answered";
        elephc_probe_note_transaction(unseen.as_ptr(), unseen.len(), 404, 1_000);
    }
    let snapshot = apm::decode(&apm_snapshot_text()).unwrap();
    let known = snapshot.routes.iter().find(|r| r.name == route).unwrap();
    assert_eq!(known.count, 3, "{snapshot:?}");
    assert_eq!(known.status[apm::status_class(404)], 2, "{snapshot:?}");
    assert_eq!(known.io[apm::IO_DB_OPS], 1, "its query stays with its request");
    let not_found = snapshot.routes.iter().find(|r| r.name == "<not found>").unwrap();
    assert_eq!(not_found.count, 1, "only the unseen route: {snapshot:?}");
    assert_eq!(snapshot.routes.len(), 2, "{snapshot:?}");
}

/// An isolated handler child runs a new route's request and its web worker
/// records it by name: the child settles the work it held as pending on the
/// row the worker counts the request under, not on the untagged row.
/// A "not found" answer settles on `<not found>`, as its recording does.
#[test]
fn an_isolated_handlers_pending_work_settles_on_its_requests_row() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let route = "GET /fresh";
    let missing = "GET /missing";
    unsafe {
        // The child: route stamped, a query, settled, route cleared.
        elephc_probe_set_route(route.as_ptr(), route.len());
        elephc_probe_note_io();
        elephc_probe_settle_request(200);
        elephc_probe_set_route(std::ptr::null(), 0);
        // Its web worker: records the request by name.
        elephc_probe_note_transaction(route.as_ptr(), route.len(), 200, 1_000);
        elephc_probe_set_route(missing.as_ptr(), missing.len());
        elephc_probe_note_io();
        elephc_probe_settle_request(404);
        elephc_probe_set_route(std::ptr::null(), 0);
        elephc_probe_note_transaction(missing.as_ptr(), missing.len(), 404, 1_000);
    }
    let snapshot = apm::decode(&apm_snapshot_text()).unwrap();
    let fresh = snapshot.routes.iter().find(|r| r.name == route).unwrap();
    assert_eq!((fresh.count, fresh.io[apm::IO_DB_OPS]), (1, 1), "{snapshot:?}");
    let not_found = snapshot.routes.iter().find(|r| r.name == "<not found>").unwrap();
    assert_eq!((not_found.count, not_found.io[apm::IO_DB_OPS]), (1, 1), "{snapshot:?}");
    assert!(snapshot.routes.iter().all(|r| r.name == route || r.name == "<not found>"), "{snapshot:?}");
}

/// Two endpoints whose labels share their first 63 bytes stay two rows: the
/// stored name keeps a prefix and appends a hash of the whole label.
#[test]
fn long_routes_that_share_a_prefix_stay_apart() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let stem = "GET /api/v1/organizations/acme/projects/storefront/environments/production";
    let a = format!("{stem}/deployments");
    let b = format!("{stem}/secrets");
    unsafe {
        serve(&a, 200, 1_000);
        serve(&b, 200, 1_000);
        serve(&a, 200, 1_000);
    }
    let snapshot = apm::decode(&apm_snapshot_text()).unwrap();
    assert_eq!(snapshot.routes.len(), 2, "{snapshot:?}");
    for route in &snapshot.routes {
        assert!(route.name.len() <= ROUTE_NAME_MAX && route.name.starts_with("GET /api/v1/"), "{}", route.name);
        assert!(route.name.contains('~'), "a cut name says so: {}", route.name);
    }
    assert_eq!(snapshot.routes.iter().map(|r| r.count).max(), Some(2));
    let mut buffer = [0u8; ROUTE_NAME_MAX];
    assert_eq!(route_key("GET /short", &mut buffer), "GET /short", "a short name is untouched");
    let mut buffer = [0u8; ROUTE_NAME_MAX];
    let accented = format!("GET /{}", "é".repeat(40));
    let key = route_key(&accented, &mut buffer).to_string();
    assert!(key.len() <= ROUTE_NAME_MAX && key.is_char_boundary(key.len()), "{key}");
}

/// A request a worker was serving when it died is counted as a server error
/// when the master reaps it; an answered one is not counted twice.
#[test]
fn a_request_cut_short_by_its_worker_dying_counts_as_a_failure() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    TRACK_IN_FLIGHT.store(true, Ordering::Relaxed);
    let pid = unsafe { libc::getpid() };
    unsafe { serve("GET /fine", 200, 1_000) };
    elephc_probe_reap(pid);
    let route = "GET /throws";
    unsafe { elephc_probe_set_route(route.as_ptr(), route.len()) };
    // The worker dies here: no transaction, no clear. The master reaps it.
    elephc_probe_reap(pid);
    elephc_probe_reap(pid);
    let snapshot = apm::decode(&apm_snapshot_text()).unwrap();
    let fine = snapshot.routes.iter().find(|r| r.name == "GET /fine").unwrap();
    assert_eq!((fine.count, fine.status[4]), (1, 0), "an answered request is not reaped again");
    let died = snapshot.routes.iter().find(|r| r.name == route).expect("the dead request is counted");
    assert_eq!((died.count, died.status[4]), (1, 1));
    PENDING_LEN.store(0, Ordering::Relaxed);
}

/// Without in-flight tracking (isolated handlers), reaping counts nothing.
#[test]
fn reaping_counts_nothing_without_in_flight_tracking() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let route = "GET /x";
    unsafe { elephc_probe_set_route(route.as_ptr(), route.len()) };
    elephc_probe_reap(unsafe { libc::getpid() });
    assert!(row_names().is_empty());
    PENDING_LEN.store(0, Ordering::Relaxed);
}

/// Someone having asked for a sampled profile — the page's "Capture a profile"
/// button, `monitor <service>` — does not turn the `<not found>` rule off: an
/// ask is never withdrawn, and interning every path at its start from then on
/// let a scanner fill the table for the rest of the service's life (review).
#[test]
fn an_ask_keeps_scans_out_of_the_route_table() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let was_asked = ASKED.swap(true, Ordering::Relaxed);
    unsafe {
        for index in 0..300 {
            serve(&format!("GET /scan-{index}"), 404, 1_000);
        }
        serve("GET /real-endpoint", 200, 1_000);
    }
    ASKED.store(was_asked, Ordering::Relaxed);
    let names = row_names();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"<not found>".to_string()) && names.contains(&"GET /real-endpoint".to_string()));
}

/// A request the server answered before PHP ran (a body too large or too slow
/// to arrive) never adds a route: any client picks its path. It keeps the row
/// of a route the table has, and otherwise goes to `<rejected>` (review).
#[test]
fn answers_given_before_php_never_add_a_route() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    unsafe {
        serve("POST /upload", 200, 1_000);
        for index in 0..300 {
            let route = format!("POST /x{index}");
            elephc_probe_note_transaction(route.as_ptr(), route.len(), 413 | ANSWERED_BEFORE_PHP, 1_000);
        }
        let known = "POST /upload";
        elephc_probe_note_transaction(known.as_ptr(), known.len(), 413 | ANSWERED_BEFORE_PHP, 1_000);
        serve("GET /real-endpoint", 200, 1_000);
    }
    let snapshot = apm::decode(&apm_snapshot_text()).unwrap();
    let names: Vec<&str> = snapshot.routes.iter().map(|route| route.name.as_str()).collect();
    assert_eq!(names.len(), 3, "{names:?}");
    let rejected = snapshot.routes.iter().find(|route| route.name == "<rejected>").unwrap();
    assert_eq!((rejected.count, rejected.status[3]), (300, 300), "counted, as the 4xx they were");
    let upload = snapshot.routes.iter().find(|route| route.name == "POST /upload").unwrap();
    assert_eq!((upload.count, upload.status[3]), (2, 1), "a known route keeps its own 413");
    assert!(names.contains(&"GET /real-endpoint"));
}

/// New routes come from a shared bucket: 64 at once, then one a minute. A
/// catch-all answering 200 for any path made every scanned path a row, and the
/// table filled in seconds; now the scan beyond the bucket is counted under
/// `<other>`, and an endpoint still gets its row once a token is earned.
#[test]
fn new_routes_arrive_no_faster_than_the_budget() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    unsafe {
        for index in 0..300 {
            serve(&format!("GET /anything-{index}"), 200, 1_000);
        }
    }
    let snapshot = apm::decode(&apm_snapshot_text()).unwrap();
    let other = snapshot.routes.iter().find(|route| route.name == "<other>").map_or(0, |route| route.count);
    assert_eq!(snapshot.routes.len(), NEW_ROUTE_BURST as usize + 1, "the burst, then <other>");
    assert_eq!(other, 300 - u64::from(NEW_ROUTE_BURST), "no request lost");
    // The bucket itself: empty now, one token per minute after, capped.
    let base = REGION.load(Ordering::Relaxed);
    let now = monotonic_ns() / 60_000_000_000;
    assert!(!take_new_route_token(base, now));
    assert!(take_new_route_token(base, now + 1), "a minute earns one");
    assert!(!take_new_route_token(base, now + 1));
    let earned = (0..200).filter(|_| take_new_route_token(base, now + 1_000)).count();
    assert_eq!(earned, NEW_ROUTE_BURST as usize, "an idle hour refills the burst, no more");
}

/// A process killed after counting its request but before giving its row back
/// is not counted a second time, as a 500, when the master reaps it (review).
#[test]
fn a_process_dying_while_counting_its_request_is_counted_once() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    TRACK_IN_FLIGHT.store(true, Ordering::Relaxed);
    let pid = unsafe { libc::getpid() };
    unsafe { serve("GET /known", 200, 1_000) };
    let route = "GET /known";
    unsafe { elephc_probe_set_route(route.as_ptr(), route.len()) };
    let base = REGION.load(Ordering::Relaxed);
    // What `elephc_probe_note_transaction` does before giving the row back;
    // the process dies right after.
    answering_in_flight(base);
    record_transaction(base, CURRENT_ROUTE.load(Ordering::Relaxed), 200, 1_000);
    elephc_probe_reap(pid);
    let snapshot = apm::decode(&apm_snapshot_text()).unwrap();
    let known = snapshot.routes.iter().find(|r| r.name == route).unwrap();
    assert_eq!((known.count, known.status[4]), (2, 0), "{known:?}");
    INFLIGHT_SLOT.store(usize::MAX, Ordering::Relaxed);
}

/// An isolated web worker marks each request it waits on, many at once: when
/// the master reaps it, every one still served is counted as a 500 under its
/// route, while one answered (or being counted) and one the client abandoned
/// are not (review: a killed web worker's requests went uncounted).
#[test]
fn a_dead_web_workers_requests_are_each_counted() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let pid = unsafe { libc::getpid() };
    unsafe { serve("GET /known", 200, 1_000) };
    let begin = |route: &str| unsafe { elephc_probe_request_begin(route.as_ptr(), route.len()) };
    let (slow_known, slow_new, answered, abandoned, counting) =
        (begin("GET /known"), begin("GET /new"), begin("GET /known"), begin("GET /gone"), begin("GET /counting"));
    assert!([slow_known, slow_new, answered, abandoned, counting].iter().all(|token| *token != 0));
    // One answered the usual way: marked, counted, given back.
    elephc_probe_request_end(answered, 1);
    let known = "GET /known";
    unsafe { elephc_probe_note_transaction(known.as_ptr(), known.len(), 200, 1_000) };
    elephc_probe_request_end(answered, 0);
    // One dropped: the client went away.
    elephc_probe_request_end(abandoned, 0);
    // One being counted when the worker dies.
    elephc_probe_request_end(counting, 1);
    elephc_probe_reap(pid);
    let snapshot = apm::decode(&apm_snapshot_text()).unwrap();
    let count = |name: &str| snapshot.routes.iter().find(|r| r.name == name).map(|r| (r.count, r.status[4]));
    assert_eq!(count("GET /known"), Some((3, 1)), "two answered, one died serving");
    assert_eq!(count("GET /new"), Some((1, 1)), "an unseen route's dying request gets its row");
    assert_eq!(count("GET /gone"), None, "an abandoned request is not counted");
    assert_eq!(count("GET /counting"), None, "nor one already being counted");
    elephc_probe_reap(pid);
    let again = apm::decode(&apm_snapshot_text()).unwrap();
    assert_eq!(again.routes.iter().map(|r| r.count).sum::<u64>(), 4, "a second reap counts nothing");
}

/// In a combined build the exact profiler owns the event slots: a sampled
/// window alone does not make the bridges time their calls; --apm does.
#[test]
fn the_event_window_answers_for_the_runtime_that_receives_events() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = ApmFixture::new();
    APM_ON.store(false, Ordering::Release);
    let base = REGION.load(Ordering::Relaxed);
    unsafe { region_asked(base) }.store(ASK_ACTIVE, Ordering::Release);
    let probe_only = elephc_probe_note_io as extern "C" fn() as usize;
    unsafe { std::ptr::addr_of_mut!(super::elephc_instr_io_fn).write(probe_only) };
    assert_eq!(elephc_probe_event_active(), 1, "probe-only build: the sampled window times calls");
    unsafe { std::ptr::addr_of_mut!(super::elephc_instr_io_fn).write(0xdead_beef) };
    assert_eq!(elephc_probe_event_active(), 0, "combined build: nothing would receive them");
    APM_ON.store(true, Ordering::Release);
    assert_eq!(elephc_probe_event_active(), 1, "--apm counts them");
    unsafe { std::ptr::addr_of_mut!(super::elephc_instr_io_fn).write(0) };
    drop(fixture);
}

/// Runs one transaction outside HTTP the way a hook does: open it by name,
/// do its database work, mark it failed or not, close it.
unsafe fn handle(name: &str, queries: u64, failed: bool) -> u64 {
    let name = std::ffi::CString::new(name).expect("a name without NUL");
    let token = elephc_probe_txn_begin(name.as_ptr());
    for _ in 0..queries {
        elephc_probe_apm_event(apm::IO_DB_OPS as u32, 1);
    }
    if failed {
        elephc_probe_txn_fail(token);
    }
    elephc_probe_txn_end(token, 0);
    token
}

/// Messages a consumer handles are transactions of their own, named by the
/// hook: counted, failed ones with the 5xx, their queries on their row.
#[test]
fn transactions_outside_http_are_counted_by_name() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    unsafe {
        assert_ne!(handle("MSG App\\Message\\SendEmail", 2, false), 0);
        handle("MSG App\\Message\\SendEmail", 2, false);
        handle("MSG App\\Message\\SendEmail", 1, true);
        handle("CALL render", 0, false);
        // A status the caller reports: a library export that returned an error.
        let name = std::ffi::CString::new("CALL render").unwrap();
        let token = elephc_probe_txn_begin(name.as_ptr());
        elephc_probe_txn_end(token, 1);
    }
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    let find = |name: &str| {
        snapshot.routes.iter().find(|route| route.name == name).unwrap_or_else(|| panic!("no {name}: {snapshot:?}"))
    };
    let message = find("MSG App\\Message\\SendEmail");
    assert_eq!(message.count, 3);
    assert_eq!(message.status, [0, 2, 0, 0, 1]);
    assert_eq!(message.io[apm::IO_DB_OPS], 5, "the queries of every message, failed one included");
    assert_eq!(find("CALL render").status, [0, 1, 0, 0, 1]);
    assert!(snapshot.routes.iter().all(|route| route.name != "<untagged>"), "{snapshot:?}");
}

/// A transaction opened inside another is part of it: a message dispatched
/// synchronously during a web request, or a nested hook, counts nothing.
#[test]
fn a_transaction_inside_another_counts_nothing_of_its_own() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    unsafe {
        let route = "POST /checkout";
        elephc_probe_set_route(route.as_ptr(), route.len());
        assert_eq!(handle("MSG App\\Message\\Charge", 1, false), 0, "inside a web request");
        elephc_probe_note_transaction(std::ptr::null(), 0, 200, 1_000_000);
        elephc_probe_set_route(std::ptr::null(), 0);

        let outer = std::ffi::CString::new("MSG App\\Message\\Outer").unwrap();
        let token = elephc_probe_txn_begin(outer.as_ptr());
        assert_ne!(token, 0);
        assert_eq!(handle("MSG App\\Message\\Inner", 1, true), 0, "inside a message");
        elephc_probe_txn_end(token, 0);
    }
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    let names: Vec<&str> = snapshot.routes.iter().map(|route| route.name.as_str()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    let checkout = snapshot.routes.iter().find(|route| route.name == "POST /checkout").unwrap();
    assert_eq!(checkout.io[apm::IO_DB_OPS], 1, "the inner message's query is the request's");
    let outer = snapshot.routes.iter().find(|route| route.name == "MSG App\\Message\\Outer").unwrap();
    assert_eq!(outer.status, [0, 1, 0, 0, 0], "the inner failure is not the outer's");
    assert_eq!(outer.io[apm::IO_DB_OPS], 1);
}

/// Without `--apm` nothing is opened, so a binary that carries the hooks but
/// was not asked to count runs as if it had none.
#[test]
fn transactions_outside_http_count_nothing_without_apm() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = ApmFixture::new();
    APM_ON.store(false, Ordering::Release);
    assert_eq!(unsafe { handle("MSG App\\Message\\SendEmail", 1, true) }, 0);
    assert_eq!(TXN_STARTED_NS.load(Ordering::Relaxed), 0);
    APM_ON.store(true, Ordering::Release);
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    assert!(snapshot.routes.is_empty(), "{snapshot:?}");
    drop(fixture);
}

/// A reset waits for the process between a route lookup and its write, so
/// the write cannot land on an id the next route takes (review).
#[test]
fn a_reset_waits_for_a_write_in_progress() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let base = REGION.load(Ordering::Relaxed);
    let writing = TableGate::enter(base).expect("the table is open");
    let started = std::time::Instant::now();
    let reset = std::thread::spawn(reset_routes);
    // Checked as soon as the reset has raised its flag: a fixed sleep raced
    // the bounded `GATE_WAIT` on loaded CI runners, where the sleeping
    // thread woke after the reset had already given up waiting.
    let gate = shared_word(base, ROUTES_GATE_OFFSET);
    while gate.load(Ordering::Acquire) & GATE_RESETTING == 0 {
        assert!(!reset.is_finished(), "the reset raises its flag before running");
        std::thread::yield_now();
    }
    assert!(!reset.is_finished(), "the reset waits for the write in progress");
    drop(writing);
    assert!(reset.join().unwrap().contains("route table reset"));
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    // Once done, the table is open again.
    assert!(TableGate::enter(base).is_some());
    assert_eq!(shared_word(base, ROUTES_GATE_OFFSET).load(Ordering::Relaxed), 0);
}

/// A process killed inside the gate cannot hang a reset: it waits a bounded
/// time and proceeds.
#[test]
fn a_reset_does_not_wait_forever_for_a_dead_writer() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let base = REGION.load(Ordering::Relaxed);
    std::mem::forget(TableGate::enter(base));
    let started = std::time::Instant::now();
    reset_routes();
    assert!(started.elapsed() < std::time::Duration::from_secs(1), "{:?}", started.elapsed());
}

/// A reset starts the CPU of the new run of the counters: a reader that takes
/// the new epoch for a restart would otherwise charge it the service's whole
/// lifetime (review).
#[test]
fn a_reset_starts_the_cpu_of_the_new_run() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let cpu = |text: String| apm::decode(&text).expect("a snapshot").resources.expect("resources").cpu_ns;
    // Burn until the process has measurably spent CPU — by CPU time, not by
    // wall time, which a loaded machine stretches (CI).
    let busy = std::time::Instant::now();
    let mut spin = 0u64;
    while cpu(apm_snapshot_text()) < 200_000_000 && busy.elapsed() < std::time::Duration::from_secs(20) {
        for _ in 0..1_000_000 {
            spin = std::hint::black_box(spin.wrapping_add(1));
        }
    }
    let before = cpu(apm_snapshot_text());
    assert!(before >= 200_000_000, "the test burned CPU: {before}");
    reset_routes();
    let after = cpu(apm_snapshot_text());
    assert!(after < before / 2, "the new run starts near zero: {after} after {before}");
}

/// After a reset, a request that started before it keeps an id the next
/// route may take: samples and events charge nothing to it until it is
/// named again (review).
#[test]
fn an_id_from_before_a_reset_names_nothing() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let base = REGION.load(Ordering::Relaxed);
    let route = "GET /orders";
    unsafe { elephc_probe_set_route(route.as_ptr(), route.len()) };
    unsafe { elephc_probe_note_transaction(std::ptr::null(), 0, 200, 1_000) };
    unsafe { elephc_probe_set_route(route.as_ptr(), route.len()) };
    assert_ne!(route_of_this_generation(base), 0, "the route is named");
    reset_routes();
    assert_eq!(route_of_this_generation(base), 0, "its id belongs to the old table");
    unsafe { elephc_probe_set_route(route.as_ptr(), route.len()) };
    unsafe { elephc_probe_set_route(std::ptr::null(), 0) };
}

/// What an exit leaves open is counted before the last snapshot: a named
/// boundary an `exit()` left without its `finally`, failed with the exit,
/// and the program's run, while the transactions inside the run still count
/// as their own (review).
#[test]
fn an_exit_counts_the_open_transaction_and_the_run() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    elephc_probe_run_begin();
    unsafe {
        assert_ne!(handle("MSG App\\Message\\Done", 0, false), 0, "a message inside the run is its own");
        let job = std::ffi::CString::new("TXN import_batch").unwrap();
        assert_ne!(elephc_probe_txn_begin(job.as_ptr()), 0);
    }
    close_at_exit(3);
    assert_eq!(TXN_STARTED_NS.load(Ordering::Relaxed), 0, "nothing stays open");
    assert_eq!(RUN_STARTED_NS.load(Ordering::Relaxed), 0);
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    let find = |name: &str| snapshot.routes.iter().find(|route| route.name.starts_with(name)).unwrap_or_else(|| panic!("no {name}: {snapshot:?}"));
    assert_eq!(find("MSG App\\Message\\Done").status, [0, 1, 0, 0, 0]);
    assert_eq!(find("TXN import_batch").status, [0, 0, 0, 0, 1], "the exit failed the job it cut short");
    assert_eq!(find("CLI ").status, [0, 0, 0, 0, 1], "the run, failed with its exit status");
    // A second exit sequence counts nothing again.
    close_at_exit(0);
    let again = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    assert_eq!(again.routes.iter().map(|route| route.count).sum::<u64>(), 3);
}

/// A script's queries outside its transactions are its run's, not the
/// untagged row's; those inside a message stay the message's (review).
#[test]
fn a_runs_own_work_is_charged_to_its_run() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    elephc_probe_run_begin();
    elephc_probe_apm_event(apm::IO_DB_OPS as u32, 1);
    unsafe { handle("MSG App\\Message\\Import", 2, false) };
    elephc_probe_apm_event(apm::IO_DB_OPS as u32, 1);
    close_at_exit(0);
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    let find = |name: &str| snapshot.routes.iter().find(|route| route.name.starts_with(name)).unwrap_or_else(|| panic!("no {name}: {snapshot:?}"));
    assert_eq!(find("CLI ").io[apm::IO_DB_OPS], 2, "the run's own two queries");
    assert_eq!(find("MSG App\\Message\\Import").io[apm::IO_DB_OPS], 2);
    assert!(snapshot.routes.iter().all(|route| route.name != "<untagged>"), "{snapshot:?}");
}

/// A request whose route was unseen when it began, answered "not found"
/// after another process added the route, is counted on the route's row
/// with its work — as an isolated request's recorder would find it (review).
#[test]
fn a_not_found_answer_finds_a_route_added_meanwhile() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let route = "GET /orders/{id}";
    unsafe {
        // This request starts while the route is unseen: its route is pending.
        elephc_probe_set_route(route.as_ptr(), route.len());
        elephc_probe_apm_event(apm::IO_DB_OPS as u32, 1);
        // Another worker answers the same route first, and adds it.
        assert_ne!(intern_new_route(REGION.load(Ordering::Relaxed), route), 0);
        elephc_probe_note_transaction(std::ptr::null(), 0, 404, 1_000);
        elephc_probe_set_route(std::ptr::null(), 0);
    }
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    let orders = snapshot.routes.iter().find(|row| row.name == route).unwrap_or_else(|| panic!("{snapshot:?}"));
    assert_eq!(orders.status, [0, 0, 0, 1, 0], "the 404 is the route's");
    assert_eq!(orders.io[apm::IO_DB_OPS], 1, "with its query");
    assert!(snapshot.routes.iter().all(|row| row.name != NOT_FOUND_ROUTE_NAME), "{snapshot:?}");
}

/// A snapshot waits for a reset in progress: read across it, the old
/// counters would carry the new run's epoch and be counted again (review).
#[test]
fn a_snapshot_waits_for_a_reset_in_progress() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let base = REGION.load(Ordering::Relaxed);
    let reader = TableGate::exclusive(base, || {
        let reader = std::thread::spawn(apm_snapshot_text);
        std::thread::sleep(std::time::Duration::from_millis(40));
        assert!(!reader.is_finished(), "the snapshot waits for the reset");
        reader
    });
    assert!(apm::decode(&reader.join().unwrap()).is_ok(), "and is read once the reset is done");
}

/// Snapshots read while a reset runs never pair the old counters with the
/// new run's epoch: a read the reset crossed is read again (review).
#[test]
fn snapshots_crossed_by_a_reset_are_read_again() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    for _ in 0..100 {
        unsafe { serve("GET /orders", 200, 1_000) };
    }
    let resetter = std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(20));
        reset_routes()
    });
    // Read until the reset is seen done, then once more: however slow the
    // runner, the reads span the reset and end after it (review).
    let check = |text: String| {
        let snapshot = apm::decode(&text).expect("a snapshot");
        let total: u64 = snapshot.routes.iter().map(|route| route.count).sum();
        if snapshot.epoch == 42 {
            assert_eq!(total, 100, "the old run, whole");
        } else {
            assert_eq!(total, 0, "the new run carries none of the old counts");
        }
        snapshot.epoch
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !resetter.is_finished() && std::time::Instant::now() < deadline {
        check(apm_snapshot_text());
    }
    assert!(resetter.join().unwrap().contains("route table reset"));
    assert_ne!(check(apm_snapshot_text()), 42, "read after the reset: the new run");
}

/// Resets that never stop crossing the read leave no consistent answer: the
/// reader is told the service is busy and asks again, rather than given one
/// run's counters under another's epoch (review).
#[test]
fn a_read_no_reset_lets_through_answers_busy() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    unsafe { serve("GET /orders", 200, 1_000) };
    let base = REGION.load(Ordering::Relaxed);
    let gate = shared_word(base, ROUTES_GATE_OFFSET);
    // A reset that never ends, as one whose process died mid-way.
    gate.fetch_or(GATE_RESETTING, Ordering::AcqRel);
    let answer = apm_snapshot_text();
    gate.fetch_and(!GATE_RESETTING, Ordering::AcqRel);
    assert_eq!(answer, apm::SNAPSHOT_BUSY);
    assert_eq!(apm::decode(&answer), Err(apm::DecodeError::Busy));
    assert!(apm::decode(&apm_snapshot_text()).is_ok(), "read again once the reset is over");
}

/// A reset that outlasts the table gate's bounded wait (a process killed
/// mid-reset, a slow one) cannot pair a snapshot's old counters with the new
/// run's epoch: the read sees the reset in progress and is taken again once
/// it is done (review).
#[test]
fn a_read_a_long_reset_crossed_is_read_again() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    for _ in 0..100 {
        unsafe { serve("GET /orders", 200, 1_000) };
    }
    let base = REGION.load(Ordering::Relaxed);
    let gate = shared_word(base, ROUTES_GATE_OFFSET);
    // A reset in progress for longer than a reader waits at the gate.
    gate.fetch_or(GATE_RESETTING, Ordering::AcqRel);
    let reader = std::thread::spawn(apm_snapshot_text);
    std::thread::sleep(std::time::Duration::from_millis(250));
    reset_routes_exclusive(base, resources::service_usage().cpu_ns);
    gate.fetch_and(!GATE_RESETTING, Ordering::AcqRel);
    let snapshot = apm::decode(&reader.join().unwrap()).expect("a snapshot");
    assert_ne!(snapshot.epoch, 42, "read again after the reset");
    assert_eq!(snapshot.routes.iter().map(|route| route.count).sum::<u64>(), 0, "with none of the old counts");
}

/// Two resets never overlap: each raised the flag, and the first to finish
/// lowered it under the other (review). Resets of one process take turns.
#[test]
fn resets_of_one_process_take_turns() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let base = REGION.load(Ordering::Relaxed);
    let inside = std::sync::Arc::new(AtomicBool::new(false));
    let resets: Vec<_> = (0..4)
        .map(|_| {
            let inside = inside.clone();
            std::thread::spawn(move || {
                TableGate::exclusive(base, || {
                    assert!(!inside.swap(true, Ordering::AcqRel), "two resets at once");
                    let flag = shared_word(base, ROUTES_GATE_OFFSET).load(Ordering::Acquire);
                    assert_ne!(flag & GATE_RESETTING, 0, "the flag stays up for the whole reset");
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    inside.store(false, Ordering::Release);
                })
            })
        })
        .collect();
    for reset in resets {
        reset.join().unwrap();
    }
    assert_eq!(shared_word(base, ROUTES_GATE_OFFSET).load(Ordering::Acquire) & GATE_RESETTING, 0);
}

/// Another process's reset holds the flag: this one waits for it to be
/// lowered rather than run beside it (review).
#[test]
fn a_reset_waits_for_another_processs_reset() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let base = REGION.load(Ordering::Relaxed);
    let gate = shared_word(base, ROUTES_GATE_OFFSET);
    gate.fetch_or(GATE_RESETTING, Ordering::AcqRel);
    let ran = std::sync::Arc::new(AtomicBool::new(false));
    let reset = {
        let ran = ran.clone();
        std::thread::spawn(move || TableGate::exclusive(base, || ran.store(true, Ordering::Release)))
    };
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(!ran.load(Ordering::Acquire), "waits while the other reset runs, past the users' wait");
    gate.fetch_and(!GATE_RESETTING, Ordering::AcqRel);
    reset.join().unwrap();
    assert!(ran.load(Ordering::Acquire));
    assert_eq!(gate.load(Ordering::Acquire) & GATE_RESETTING, 0);
}

/// A flag a dead process left up does not block resets forever: it is taken
/// over after `RESET_TAKEOVER`.
#[test]
fn a_reset_takes_over_a_flag_left_by_a_dead_process() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let base = REGION.load(Ordering::Relaxed);
    let gate = shared_word(base, ROUTES_GATE_OFFSET);
    gate.fetch_or(GATE_RESETTING, Ordering::AcqRel);
    let started = std::time::Instant::now();
    TableGate::exclusive(base, || ());
    assert!(started.elapsed() >= RESET_TAKEOVER);
    assert_eq!(gate.load(Ordering::Acquire) & GATE_RESETTING, 0, "and lowered afterwards");
}

/// A process that reuses a dead one's pid never takes over the dead one's row
/// while it still holds an uncounted request: the takeover erased that
/// request, and charged the master's reap of it to the new process (review).
#[test]
fn a_reused_pid_leaves_a_dead_requests_row_alone() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    TRACK_IN_FLIGHT.store(true, Ordering::Relaxed);
    let base = REGION.load(Ordering::Relaxed);
    let route = "GET /died";
    unsafe { elephc_probe_set_route(route.as_ptr(), route.len()) };
    let dead = INFLIGHT_SLOT.load(Ordering::Relaxed);
    assert!(dead < INFLIGHT_SLOTS);
    // The dying process is gone; a new one with its pid looks for a row.
    INFLIGHT_PID.store(0, Ordering::Relaxed);
    INFLIGHT_SLOT.store(usize::MAX, Ordering::Relaxed);
    let fresh = inflight_slot(base).expect("a row");
    assert_ne!(fresh, dead, "the dead request's row is not taken over");
    assert_eq!(unsafe { inflight_word(base, dead, 1) }.load(Ordering::Acquire), INFLIGHT_SERVING);
    // An abandoned row of that pid that serves nothing is taken back.
    unsafe { inflight_word(base, fresh, 0) }.store(0, Ordering::Release);
    unsafe { inflight_word(base, dead, 1) }.store(0, Ordering::Release);
    INFLIGHT_PID.store(0, Ordering::Relaxed);
    INFLIGHT_SLOT.store(usize::MAX, Ordering::Relaxed);
    assert_eq!(inflight_slot(base), Some(dead));
    end_in_flight(base);
    PENDING_LEN.store(0, Ordering::Relaxed);
}

/// A failure reported from inside a nested transaction — a message handled
/// synchronously inside a job, whose failure the job contains — does not fail
/// the open one; the same report outside any nesting does (review).
#[test]
fn a_failure_reported_inside_a_nested_transaction_stays_there() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let outcome = |report_inside: bool| unsafe {
        let job = std::ffi::CString::new("TXN App\\Job::run").unwrap();
        let message = std::ffi::CString::new("MSG App\\Message\\Charge").unwrap();
        let token = elephc_probe_txn_begin(job.as_ptr());
        assert_ne!(token, 0);
        let nested = elephc_probe_txn_begin(message.as_ptr());
        assert_eq!(nested, 0);
        if report_inside {
            elephc_probe_txn_fail_current();
        }
        elephc_probe_txn_end(nested, 0);
        if !report_inside {
            elephc_probe_txn_fail_current();
        }
        elephc_probe_txn_end(token, 0);
        let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
        let job = snapshot.routes.iter().find(|route| route.name == "TXN App\\Job::run").unwrap().clone();
        job.status
    };
    assert_eq!(outcome(true), [0, 1, 0, 0, 0], "the contained failure is the message's");
    assert_eq!(outcome(false), [0, 1, 0, 0, 1], "a failure reported in the job fails it");
    assert_eq!(NESTED_DEPTH.load(Ordering::Relaxed), 0);
}

/// Following turns nothing on: it answers whether counting already runs, as
/// it does from the start in a build with `--apm-collector`, so a `--web`
/// service started without `--apm` records its requests then, and only then
/// (review).
#[test]
fn following_counts_only_when_counting_already_runs() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    APM_ON.store(false, Ordering::Release);
    assert_eq!(elephc_probe_apm_enable(1 | APM_FOLLOW), 0);
    assert!(!APM_ON.load(Ordering::Acquire), "nothing was turned on");
    assert!(!TRACK_IN_FLIGHT.load(Ordering::Relaxed));
    elephc_probe_apm_enable(0);
    let epoch = APM_EPOCH.load(Ordering::Relaxed);
    assert_eq!(elephc_probe_apm_enable(1 | APM_FOLLOW), 1);
    assert!(TRACK_IN_FLIGHT.load(Ordering::Relaxed), "the service's in-flight choice is taken");
    assert_eq!(APM_EPOCH.load(Ordering::Relaxed), epoch, "the run counting already is not restarted");
}

/// The work a request did while its route was new reaches its row when its
/// worker dies serving it: it lived in the worker alone, and the master
/// counted the failed request with no queries (review).
#[test]
fn a_dead_workers_pending_work_is_charged_to_its_request() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    TRACK_IN_FLIGHT.store(true, Ordering::Relaxed);
    let route = "GET /imports/new";
    unsafe { elephc_probe_set_route(route.as_ptr(), route.len()) };
    assert_ne!(PENDING_LEN.load(Ordering::Relaxed), 0, "a route the table has not seen is pending");
    note_apm_io(apm::IO_DB_OPS, 3);
    note_apm_io(apm::IO_DB_WAIT, 2_000_000);
    // The worker dies here; the master reaps it.
    elephc_probe_reap(unsafe { libc::getpid() });
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    let died = snapshot.routes.iter().find(|r| r.name == route).expect("the dead request is counted");
    assert_eq!((died.count, died.status[4]), (1, 1));
    assert_eq!(died.io[apm::IO_DB_OPS], 3, "its queries are its own");
    assert_eq!(died.io[apm::IO_DB_WAIT], 2_000_000);
    PENDING_LEN.store(0, Ordering::Relaxed);
}

/// A request of a known route keeps its statements in its process and publishes them once,
/// with the request: nothing reaches the shared row while it runs. A worker that dies
/// serving one still has them charged to it, from its in-flight row.
#[test]
fn a_known_routes_work_is_published_once_with_its_request() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    TRACK_IN_FLIGHT.store(true, Ordering::Relaxed);
    let route = "GET /orders/{id}";
    let io = |name: &str| {
        let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
        snapshot.routes.iter().find(|r| r.name == name).map_or([0; apm::IO_KINDS], |r| r.io)
    };
    unsafe {
        serve(route, 200, 1_000);
        elephc_probe_set_route(route.as_ptr(), route.len());
        assert_eq!(PENDING_LEN.load(Ordering::Relaxed), 0, "the route is known");
        for _ in 0..3 {
            note_apm_io(apm::IO_DB_OPS, 1);
            note_apm_io(apm::IO_DB_WAIT, 1_000);
        }
        assert_eq!(io(route)[apm::IO_DB_OPS], 0, "nothing shared while the request runs");
        elephc_probe_note_transaction(std::ptr::null(), 0, 200, 5_000);
        assert_eq!((io(route)[apm::IO_DB_OPS], io(route)[apm::IO_DB_WAIT]), (3, 3_000), "all of it, once");
        // The next request dies serving: its statements still reach the row.
        elephc_probe_set_route(route.as_ptr(), route.len());
        note_apm_io(apm::IO_DB_OPS, 2);
        elephc_probe_reap(libc::getpid());
    }
    assert_eq!(io(route)[apm::IO_DB_OPS], 5, "the dead request's statements are its own");
    PENDING_IO.iter().for_each(|counter| counter.store(0, Ordering::Relaxed));
}

/// A worker killed after its pending work moved to the route, but before its
/// request was marked as being counted, has that work counted once: the
/// master's reap charged the row's copy again (review).
#[test]
fn pending_work_moved_to_its_route_is_not_charged_again() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    TRACK_IN_FLIGHT.store(true, Ordering::Relaxed);
    let base = REGION.load(Ordering::Relaxed);
    let route = "GET /imports/moved";
    unsafe { elephc_probe_set_route(route.as_ptr(), route.len()) };
    note_apm_io(apm::IO_DB_OPS, 3);
    // The answer's first step, then the worker dies.
    let route_id = unsafe { intern_new_route(base, route) };
    flush_pending_io(base, route_id);
    elephc_probe_reap(unsafe { libc::getpid() });
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    let row = snapshot.routes.iter().find(|r| r.name == route).expect("counted");
    assert_eq!(row.io[apm::IO_DB_OPS], 3, "the queries are counted once");
    PENDING_LEN.store(0, Ordering::Relaxed);
}

/// Micro-benchmark of the request path under contention (ignored; run by
/// hand): N forked processes sharing the region each enter and leave the
/// table gate and record a transaction on one route, as N `--web` workers
/// serving the same route do. Prints nanoseconds per operation for N = 1, 4,
/// 16, so a lever on the shared words can be compared on the same machine.
#[test]
#[ignore]
fn bench_gate_and_counters_under_contention() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    let base = REGION.load(Ordering::Relaxed);
    let iterations: u64 =
        std::env::var("APM_BENCH_ITERATIONS").ok().and_then(|v| v.parse().ok()).unwrap_or(2_000_000);
    for processes in [1usize, 4, 16] {
        let started = std::time::Instant::now();
        let mut children = Vec::new();
        for _ in 0..processes {
            let pid = unsafe { libc::fork() };
            if pid == 0 {
                for _ in 0..iterations {
                    let _gate = TableGate::enter(base);
                    record_transaction(base, 1, 200, 1_000);
                }
                unsafe { libc::_exit(0) };
            }
            children.push(pid);
        }
        for pid in children {
            let mut status = 0;
            unsafe { libc::waitpid(pid, &mut status, 0) };
        }
        // Wall time over each process's own iterations: what one request pays.
        let elapsed = started.elapsed().as_nanos() as f64;
        println!("processes={processes:2} ns/op={:.1}", elapsed / iterations as f64);
    }
}

/// What one SQL statement costs the probe while its request runs: the statement and its
/// wait, two events, from 1, 4 and 16 processes serving the same route. Ignored: a
/// measurement (`cargo test -p elephc-probe bench_statement_io -- --ignored --nocapture`).
#[test]
#[ignore]
fn bench_statement_io_under_contention() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    TRACK_IN_FLIGHT.store(true, Ordering::Relaxed);
    let route = "GET /bench";
    unsafe { serve(route, 200, 1_000) };
    let iterations: u64 =
        std::env::var("APM_BENCH_ITERATIONS").ok().and_then(|v| v.parse().ok()).unwrap_or(2_000_000);
    for processes in [1usize, 4, 16] {
        let started = std::time::Instant::now();
        let mut children = Vec::new();
        for _ in 0..processes {
            let pid = unsafe { libc::fork() };
            if pid == 0 {
                unsafe { elephc_probe_set_route(route.as_ptr(), route.len()) };
                for _ in 0..iterations {
                    note_apm_io(apm::IO_DB_OPS, 1);
                    note_apm_io(apm::IO_DB_WAIT, 1_000);
                }
                unsafe { libc::_exit(0) };
            }
            children.push(pid);
        }
        for pid in children {
            let mut status = 0;
            unsafe { libc::waitpid(pid, &mut status, 0) };
        }
        let elapsed = started.elapsed().as_nanos() as f64;
        println!("processes={processes:2} ns/statement={:.1}", elapsed / iterations as f64);
    }
    TRACK_IN_FLIGHT.store(false, Ordering::Relaxed);
}

/// What the project's `[apm]` rules exclude counts nothing: a stamped web request and its
/// queries, a request an isolated worker records by name, a named transaction. What they
/// let in is counted as before.
#[test]
fn excluded_transactions_count_nothing() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    crate::apm_rules::set_test_rules(Some("-GET /health\n-MSG App\\Message\\Ping\n"));
    TRACK_IN_FLIGHT.store(true, Ordering::Relaxed);
    unsafe {
        let health = "GET /health";
        elephc_probe_set_route(health.as_ptr(), health.len());
        elephc_probe_apm_event(apm::IO_DB_OPS as u32, 1);
        elephc_probe_note_transaction(std::ptr::null(), 0, 200, 1_000);
        elephc_probe_set_route(std::ptr::null(), 0);
        elephc_probe_note_transaction(health.as_ptr(), health.len(), 200, 1_000);
        assert_eq!(elephc_probe_request_begin(health.as_ptr(), health.len()), 0, "never in flight");
        assert_eq!(handle("MSG App\\Message\\Ping", 1, false), 0, "never opened");
        serve("GET /products", 200, 1_000);
        assert_ne!(handle("MSG App\\Message\\SendEmail", 0, false), 0);
    }
    crate::apm_rules::set_test_rules(None);
    let names = row_names();
    assert!(names.contains(&"GET /products".to_string()) && names.contains(&"MSG App\\Message\\SendEmail".to_string()));
    assert!(!names.iter().any(|name| name.contains("health") || name.contains("Ping")), "{names:?}");
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    assert!(snapshot.routes.iter().all(|route| route.io[apm::IO_DB_OPS] == 0), "the excluded queries went nowhere");
}

/// An excluded boundary called inside another excluded one ends without counting the
/// outer one's remaining work; and an excluded request an isolated handler settles
/// publishes neither its queries nor its error (review).
#[test]
fn nested_exclusions_and_settled_excluded_requests_count_nothing() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    crate::apm_rules::set_test_rules(Some("-TXN outer\n-TXN inner\n-GET /health\n"));
    unsafe {
        let outer = elephc_probe_txn_begin(c"TXN outer".as_ptr());
        let inner = elephc_probe_txn_begin(c"TXN inner".as_ptr());
        assert_eq!((outer, inner), (0, 0));
        elephc_probe_txn_end(inner, 0);
        assert!(crate::apm_rules::request_ignored(), "the outer exclusion still runs");
        elephc_probe_apm_event(apm::IO_DB_OPS as u32, 1);
        elephc_probe_txn_end(outer, 0);
        assert!(!crate::apm_rules::request_ignored(), "and ends with its end");
        let health = "GET /health";
        elephc_probe_set_route(health.as_ptr(), health.len());
        elephc_probe_apm_event(apm::IO_DB_OPS as u32, 1);
        elephc_probe_settle_request(500);
        elephc_probe_set_route(std::ptr::null(), 0);
    }
    crate::apm_rules::set_test_rules(None);
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    assert!(snapshot.routes.iter().all(|route| route.io[apm::IO_DB_OPS] == 0 && route.mem[0] == 0), "{:?}", snapshot.routes);
    assert!(snapshot.errors.is_empty(), "{:?}", snapshot.errors);
}

/// An isolated handler (`--web-isolation=pool|request`) that dies never settles: on a known
/// route its queries reach the route's row as they run, so its death loses none (review).
#[test]
fn an_isolated_handler_writes_a_known_routes_queries_as_they_run() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    TRACK_IN_FLIGHT.store(false, Ordering::Relaxed);
    unsafe {
        serve("GET /orders", 200, 1_000);
        let route = "GET /orders";
        elephc_probe_set_route(route.as_ptr(), route.len());
        elephc_probe_apm_event(apm::IO_DB_OPS as u32, 3);
    }
    // The handler dies here: no settle, no end of request.
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    let row = snapshot.routes.iter().find(|route| route.name == "GET /orders").expect("the route");
    PENDING_LEN.store(0, Ordering::Relaxed);
    for counter in &PENDING_IO {
        counter.store(0, Ordering::Relaxed);
    }
    assert_eq!(row.io[apm::IO_DB_OPS], 3, "on the row already");
}

/// An exception noted inside a nested boundary and caught by its caller does not become
/// the outer transaction's error: the one the outer transaction fails with is counted
/// (review).
#[test]
fn a_handled_nested_error_does_not_hide_the_real_failure() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    unsafe {
        let outer = elephc_probe_txn_begin(c"TXN import".as_ptr());
        assert_ne!(outer, 0);
        let inner = elephc_probe_txn_begin(c"TXN fetch".as_ptr());
        assert_eq!(inner, 0, "nested");
        crate::apm_errors::elephc_probe_note_error(c"App\\Timeout".as_ptr(), c"/app/fetch.php".as_ptr(), 7);
        elephc_probe_txn_end(inner, 0);
        crate::apm_errors::elephc_probe_note_error(c"App\\Broken".as_ptr(), c"/app/import.php".as_ptr(), 30);
        elephc_probe_txn_fail(outer);
        elephc_probe_txn_end(outer, 0);
    }
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    let classes: Vec<&str> = snapshot.errors.iter().map(|error| error.class.as_str()).collect();
    assert_eq!(classes, vec!["App\\Broken"], "{:?}", snapshot.errors);
}

/// With a profile asked for, an excluded request still carries a route, for the
/// sampler only: a counted boundary inside it is opened and counted with its work,
/// and the request gets its sampler route back after it (review).
#[test]
fn a_counted_boundary_inside_an_excluded_request_is_counted_while_sampling() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    crate::apm_rules::set_test_rules(Some("-GET /health\n"));
    let was_asked = ASKED.swap(true, Ordering::Relaxed);
    let sampler_route;
    unsafe {
        let health = "GET /health";
        elephc_probe_set_route(health.as_ptr(), health.len());
        sampler_route = CURRENT_ROUTE.load(Ordering::Relaxed);
        assert_ne!(sampler_route, 0, "named for the sampler");
        assert_ne!(handle("TXN warm_cache", 2, false), 0, "the boundary is opened, not nested");
        assert_eq!(CURRENT_ROUTE.load(Ordering::Relaxed), sampler_route, "the sampler's route is back");
        elephc_probe_set_route(std::ptr::null(), 0);
    }
    ASKED.store(was_asked, Ordering::Relaxed);
    crate::apm_rules::set_test_rules(None);
    let snapshot = apm::decode(&apm_snapshot_text()).expect("a snapshot");
    let row = snapshot.routes.iter().find(|route| route.name == "TXN warm_cache").expect("counted");
    assert_eq!((row.count, row.io[apm::IO_DB_OPS]), (1, 2));
}

/// An include counts only what it names; an exclude carves exceptions out of it.
#[test]
fn an_include_counts_only_what_it_names() {
    let _guard = tests::ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = ApmFixture::new();
    crate::apm_rules::set_test_rules(Some("+GET /api/*\n-GET /api/ping\n"));
    unsafe {
        serve("GET /api/orders", 200, 1_000);
        serve("GET /api/ping", 200, 1_000);
        serve("GET /admin", 200, 1_000);
    }
    crate::apm_rules::set_test_rules(None);
    assert_eq!(row_names(), vec!["GET /api/orders".to_string()]);
}
