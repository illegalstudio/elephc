//! Purpose:
//! The `--apm` half of the probe's runtime: the per-route transaction table a
//! `--web` service counts every request into, the pending route of a request
//! whose route the table has not seen, the in-flight rows the master reaps,
//! the new-route budget, the route-table reset, and the snapshot the collector
//! reads.
//!
//! Called from:
//! - The web server through the runtime slots (`elephc_probe_apm_enable`,
//!   `elephc_probe_note_transaction`, `elephc_probe_settle_request`,
//!   `elephc_probe_reap`, `elephc_probe_request_begin`/`_end`) and the
//!   bridges' I/O events.
//! - `elephc_probe_set_route`, which stamps a request's route.
//! - The endpoint, for `WANT_TRANSACTIONS` and `WANT_RESET_ROUTES`.
//!
//! Key details:
//! - Everything lives in the shared region, so a prefork's workers and isolated
//!   handler children count into one table; the counts are relaxed atomic
//!   increments with the request's count written last.
//! - A route is added only once its request is answered, never for an answer
//!   given before PHP ran or a "not found", and no faster than the new-route
//!   budget allows: the 255-route table is never evicted.

use super::*;

/// Whether this service counts its transactions (`--apm`).
///
/// A process static rather than a shared word because the one writer is the
/// `--web` master, before it forks: every worker and every isolated handler
/// child inherits the value with the rest of its memory, and nothing turns it
/// off again. Unlike `ASKED`, there is no later ask to propagate.
pub(crate) static APM_ON: AtomicBool = AtomicBool::new(false);

/// When transaction counting started, Unix nanoseconds: the identity of this
/// run's counters. A reader that sees it change knows the service restarted and
/// that the counters went back to zero.
pub(crate) static APM_EPOCH: AtomicU64 = AtomicU64::new(0);

/// The service's CPU total when its route table was last reset: a snapshot
/// reports the CPU of its run of the counters, which a reset starts anew.
pub(crate) static CPU_AT_RESET_NS: AtomicU64 = AtomicU64::new(0);

/// Turns transaction counting on for the life of the process.
///
/// Called by the `--web` master when it was started with `--apm`, before the
/// prefork. Returns 1 when counting is on, 0 when there is nowhere to count —
/// the shared region failed to map — so the server can refuse to start rather
/// than run a monitored service that silently reports nothing.
///
/// An explicit operator choice on the command line, never an environment
/// variable: the person who can pass a service its arguments already owns it,
/// whereas anyone who can set an environment variable could otherwise make a
/// service account for its traffic in a table someone else reads.
///
/// Bit 0 of `flags` (track in flight) is set when the processes that run PHP are the
/// master's own workers (`--web-isolation=worker`): only then can the master
/// see one die mid-request and count it (`elephc_probe_reap`). With isolated
/// handlers the web worker answers a dead handler's request with a 500 itself.
///
/// With `APM_FOLLOW` in `flags`, it turns nothing on: it answers whether
/// counting already runs — a build with `--apm-collector` counts from main's
/// prologue — and if so takes `track_in_flight`. A `--web` service started
/// without `--apm` follows its collector that way (review: it sent snapshots
/// in which no request was ever recorded).
#[no_mangle]
pub extern "C" fn elephc_probe_apm_enable(flags: u32) -> u32 {
    if REGION.load(Ordering::Relaxed) == 0 {
        return 0;
    }
    let track_in_flight = flags & 1;
    if flags & APM_FOLLOW != 0 {
        if !APM_ON.load(Ordering::Acquire) {
            return 0;
        }
        TRACK_IN_FLIGHT.store(track_in_flight != 0, Ordering::Relaxed);
        return 1;
    }
    TRACK_IN_FLIGHT.store(track_in_flight != 0, Ordering::Relaxed);
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(1)
        .max(1);
    APM_EPOCH.store(epoch, Ordering::Relaxed);
    APM_ON.store(true, Ordering::Release);
    1
}

/// `elephc_probe_apm_enable` flag: follow counting that already runs, start
/// none.
pub const APM_FOLLOW: u32 = 2;

/// Word `word` of route `route_id`'s transaction row.
///
/// # Safety
/// `base` must be the mapped region, `route_id < EVENT_BUCKETS`,
/// `word < apm::ROUTE_WORDS`.
pub(crate) unsafe fn txn_word<'a>(base: usize, route_id: usize, word: usize) -> &'a AtomicU64 {
    let offset = TXN_TABLE_OFFSET + (route_id * apm::ROUTE_WORDS + word) * 8;
    &*((base + offset) as *const AtomicU64)
}

/// Address of the runtime's live-heap watermark (`_gc_peak`), published by main's prologue
/// in a probe build; 0 otherwise. A `--web` request starts it over, so it holds the peak
/// memory of the request this process just ran.
#[no_mangle]
pub static mut elephc_probe_heap_peak: usize = 0;

/// The peak memory of the request this process just ran, bytes, when the build said where.
fn request_peak_bytes() -> Option<u64> {
    let address = unsafe { std::ptr::addr_of!(elephc_probe_heap_peak).read_volatile() };
    (address != 0).then(|| unsafe { (*(address as *const AtomicU64)).load(Ordering::Relaxed) })
}

/// Adds the peak memory of the request this process just ran to `route_id`'s row. Only from
/// the process whose PHP ran it: a web worker recording an isolated handler's request by
/// name never ran it.
pub(crate) fn record_memory(base: usize, route_id: usize) {
    if route_id >= EVENT_BUCKETS {
        return;
    }
    let Some(bytes) = request_peak_bytes() else {
        return;
    };
    unsafe {
        txn_word(base, route_id, apm::WORD_MEM_HIST0 + apm::mem_bucket_of(bytes)).fetch_add(1, Ordering::Relaxed);
        txn_word(base, route_id, apm::WORD_MEM_SUM).fetch_add(bytes, Ordering::Relaxed);
        txn_word(base, route_id, apm::WORD_MEM_N).fetch_add(1, Ordering::Relaxed);
    }
}

/// Adds one completed request to `route_id`'s row.
///
/// Four relaxed increments and no allocation. The count goes LAST and with
/// `Release`: a reader that acquires a non-zero count is then guaranteed to see
/// the route's published name and the request's other counters, so a snapshot
/// never shows a request it cannot also place.
pub(crate) fn record_transaction(base: usize, route_id: usize, status: u32, duration_ns: u64) {
    // The error the transaction noted is counted with it when it failed, and forgotten either way.
    crate::apm_errors::commit(base, route_id, route_id < EVENT_BUCKETS && status >= 500);
    if route_id >= EVENT_BUCKETS {
        return;
    }
    let status = u16::try_from(status).unwrap_or(0);
    unsafe {
        txn_word(base, route_id, apm::WORD_STATUS0 + apm::status_class(status))
            .fetch_add(1, Ordering::Relaxed);
        txn_word(base, route_id, apm::WORD_SUM_NS).fetch_add(duration_ns, Ordering::Relaxed);
        txn_word(base, route_id, apm::HEADER_WORDS + apm::bucket_of(duration_ns))
            .fetch_add(1, Ordering::Relaxed);
        txn_word(base, route_id, apm::WORD_COUNT).fetch_add(1, Ordering::Release);
    }
}

/// Records one completed request: its HTTP status and its wall time.
///
/// `route`/`len` name the route when the caller has one the probe has not seen
/// yet — a request answered before PHP ran, or one served in another process.
/// A null `route` charges the route the last `elephc_probe_set_route` stamped,
/// which is the common case and costs no lookup.
///
/// # Safety
/// A non-null `route`/`len` describe a UTF-8 string valid for this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_note_transaction(
    route: *const u8,
    len: usize,
    status: u32,
    duration_ns: u64,
) {
    if !APM_ON.load(Ordering::Relaxed) {
        return;
    }
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    // A request the project's `[apm]` rules exclude counts nothing: the stamped
    // one (`elephc_probe_set_route` decided), or one recorded by name.
    if route.is_null() || len == 0 {
        if crate::apm_rules::request_ignored() {
            return;
        }
    } else if len <= 4096 {
        if let Ok(text) = std::str::from_utf8(std::slice::from_raw_parts(route, len)) {
            if !crate::apm_rules::counts(text) {
                return;
            }
        }
    }
    let _gate = TableGate::enter(base);
    // A "not found" answer goes to `<not found>` only for a route the table
    // has not seen: a known endpoint answering 404 for a missing record keeps
    // its row, so its 4xx rate — and its queries per request — stay its own.
    let before_php = status & ANSWERED_BEFORE_PHP != 0;
    let status = status & !ANSWERED_BEFORE_PHP;
    let route_id = if route.is_null() || len == 0 {
        rebase_after_reset(base);
        pending_route_id(status).unwrap_or_else(|| CURRENT_ROUTE.load(Ordering::Relaxed))
    } else if len > 4096 {
        0
    } else {
        match std::str::from_utf8(std::slice::from_raw_parts(route, len)) {
            // Answered by the server itself: any client picks the path.
            Ok(text) if before_php => find_route(text).unwrap_or_else(|| intern_route(REJECTED_ROUTE_NAME)),
            Ok(text) if is_not_found(status) => {
                find_route(text).unwrap_or_else(|| intern_route(NOT_FOUND_ROUTE_NAME))
            }
            Ok(text) => intern_new_route(base, text),
            Err(_) => 0,
        }
    };
    flush_pending_io(base, route_id);
    // The request is being counted: a death from here on must not count it
    // again as a 500 (see `answering_in_flight`).
    answering_in_flight(base);
    // Its memory, when its PHP ran here: the stamped route is this process's request.
    if route.is_null() || len == 0 {
        record_memory(base, route_id);
    }
    record_transaction(base, route_id, status, duration_ns);
    end_in_flight(base);
}

/// Flag on `elephc_probe_note_transaction`'s status: the server answered
/// before any PHP ran (a 408 or a 413), so the label names no route.
pub(crate) const ANSWERED_BEFORE_PHP: u32 = 1 << 31;

/// The row requests the server answered before PHP ran are counted under, when
/// their path is not a route the table has.
pub(crate) const REJECTED_ROUTE_NAME: &str = "<rejected>";

/// How many routes the table may take at once from a standing start, and how
/// many minutes earn one more.
pub(crate) const NEW_ROUTE_BURST: u32 = 64;

/// Interns a route a request was answered under, unless new routes are
/// arriving faster than a service grows them: a request beyond the budget is
/// counted under `<other>`, and the route gets its row at a later request.
///
/// A catch-all that answers 200 for any path (a single-page app, a "soft 404")
/// made every path a scanner sent a new row, and the 255-route table filled in
/// seconds (review). A service's real routes appear in a burst at start-up and
/// rarely afterwards; a scan keeps inventing them.
pub(crate) unsafe fn intern_new_route(base: usize, route: &str) -> usize {
    if let Some(id) = find_route(route) {
        return id;
    }
    if !take_new_route_token(base, monotonic_ns() / 60_000_000_000) {
        return other_route_id(base);
    }
    intern_route(route)
}

/// Takes one token from the shared new-route budget: a bucket of
/// `NEW_ROUTE_BURST` tokens refilled by one each minute. The word packs
/// `[minute of the last refill + 1 : 32][tokens : 32]`, 0 while untouched, so
/// every process of the service draws from one bucket.
pub(crate) fn take_new_route_token(base: usize, minute: u64) -> bool {
    let word = unsafe { &*((base + ROUTE_BUDGET_OFFSET) as *const AtomicU64) };
    let minute = minute as u32;
    loop {
        let current = word.load(Ordering::Acquire);
        let (last, tokens) = if current == 0 {
            (minute, NEW_ROUTE_BURST)
        } else {
            let last = ((current >> 32) as u32).wrapping_sub(1);
            let earned = minute.saturating_sub(last);
            (last.max(minute), (current as u32).saturating_add(earned).min(NEW_ROUTE_BURST))
        };
        if tokens == 0 {
            return false;
        }
        let next = (u64::from(last.wrapping_add(1)) << 32) | u64::from(tokens - 1);
        if word.compare_exchange(current, next, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            return true;
        }
    }
}

/// The row a request answered with `status` is counted under while its route
/// is still pending, or `None` when its route is already in the table: an
/// unseen route answered "not found" goes to `<not found>`, any other answer
/// adds the route.
pub(crate) unsafe fn pending_route_id(status: u32) -> Option<usize> {
    match PENDING_LEN.load(Ordering::Relaxed) {
        0 => None,
        pending => {
            let label = std::slice::from_raw_parts(std::ptr::addr_of!(PENDING_ROUTE) as *const u8, pending);
            let Ok(text) = std::str::from_utf8(label) else {
                return Some(0);
            };
            // Another process may have added the route since this request
            // began: then a "not found" belongs on its row too, as the
            // recorder of an isolated request would find it (review).
            if is_not_found(status) {
                return Some(find_route(text).unwrap_or_else(|| intern_route(NOT_FOUND_ROUTE_NAME)));
            }
            Some(intern_new_route(REGION.load(Ordering::Relaxed), text))
        }
    }
}

/// Ends a request this process served but does not record, settling its work
/// on the row its recorder will count it under.
///
/// An isolated handler child (`--web-isolation=pool|request`) runs the PHP,
/// and its web worker records the request by name. A route the table has not
/// seen yet holds its database and network work as pending until the answer;
/// clearing the route dropped that work on the untagged row, so the first
/// requests of a route reported too few queries. Called with the answer's
/// status before the route is cleared, it applies the recorder's rule: the
/// route's row, or `<not found>` for an unseen route answered "not found".
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_settle_request(status: u32) {
    if !APM_ON.load(Ordering::Relaxed) {
        return;
    }
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    let _gate = TableGate::enter(base);
    // An excluded request settles nothing: its work, its error and its memory are dropped,
    // not charged to `<untagged>` as a request the recorder never counts (review).
    if crate::apm_rules::request_ignored() && TXN_STARTED_NS.load(Ordering::Relaxed) == 0 {
        PENDING_LEN.store(0, Ordering::Relaxed);
        for counter in &PENDING_IO {
            counter.store(0, Ordering::Relaxed);
        }
        crate::apm_errors::clear();
        end_in_flight(base);
        return;
    }
    rebase_after_reset(base);
    let pending = pending_route_id(status);
    // Its work, its error and its memory are counted here, where the PHP ran: the recorder
    // is another process, which never saw them.
    let route_id = pending.unwrap_or_else(|| CURRENT_ROUTE.load(Ordering::Relaxed));
    flush_pending_io(base, route_id);
    crate::apm_errors::commit(base, route_id, route_id < EVENT_BUCKETS && status >= 500);
    // And its memory, which only this process measured.
    record_memory(base, route_id);
    end_in_flight(base);
}

/// Moves the work of the request or transaction this process is recording onto the row it
/// was counted under, whether its route was known or pending.
pub(crate) fn flush_pending_io(base: usize, route_id: usize) {
    PENDING_LEN.store(0, Ordering::Relaxed);
    if PENDING_IO.iter().all(|counter| counter.load(Ordering::Relaxed) == 0) {
        return;
    }
    // The shared copy goes first: the work is the row's until here and the
    // route's from here, never both. Cleared after, a worker killed in
    // between was charged it again by the master's reap (review).
    clear_in_flight_io(base);
    for (kind, counter) in PENDING_IO.iter().enumerate() {
        let amount = counter.swap(0, Ordering::Relaxed);
        if amount != 0 && route_id < EVENT_BUCKETS {
            unsafe { txn_word(base, route_id, apm::WORD_IO0 + kind) }.fetch_add(amount, Ordering::Relaxed);
        }
    }
}

/// Monotonic nanoseconds, comparable across the processes of one machine.
pub(crate) fn monotonic_ns() -> u64 {
    let mut now: libc::timespec = unsafe { std::mem::zeroed() };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    (now.tv_sec as u64).saturating_mul(1_000_000_000).saturating_add(now.tv_nsec as u64)
}

/// Word `word` of in-flight row `slot`.
///
/// # Safety
/// `base` must be the mapped region and `slot < INFLIGHT_SLOTS`.
pub(crate) unsafe fn inflight_word<'a>(base: usize, slot: usize, word: usize) -> &'a AtomicU64 {
    &*((base + INFLIGHT_TABLE_OFFSET + (slot * INFLIGHT_WORDS + word) * 8) as *const AtomicU64)
}

/// Whether requests are marked in flight for the master (see `apm_enable`).
pub(crate) static TRACK_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// This process's in-flight row, claimed on first use and cached.
pub(crate) static INFLIGHT_SLOT: AtomicUsize = AtomicUsize::new(usize::MAX);
/// The pid the cached row was claimed for; a forked child claims its own.
pub(crate) static INFLIGHT_PID: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// Claims (or finds) this process's in-flight row.
pub(crate) fn inflight_slot(base: usize) -> Option<usize> {
    let pid = unsafe { libc::getpid() };
    if INFLIGHT_PID.load(Ordering::Relaxed) == pid {
        let slot = INFLIGHT_SLOT.load(Ordering::Relaxed);
        if slot < INFLIGHT_SLOTS {
            return Some(slot);
        }
    }
    let start = (pid as usize) % INFLIGHT_SLOTS;
    for probe in 0..INFLIGHT_SLOTS {
        let slot = (start + probe) % INFLIGHT_SLOTS;
        let owner = unsafe { inflight_word(base, slot, 0) };
        // Free, or left by a process with this pid that is gone (pid reuse)
        // once it served nothing: a row still serving holds a dead request
        // the master has yet to count, which taking it over erased and
        // charged to this process instead (review).
        let current = owner.load(Ordering::Acquire);
        let abandoned = current == pid as u64
            && unsafe { inflight_word(base, slot, 1) }.load(Ordering::Acquire) != INFLIGHT_SERVING;
        if abandoned
            || (current == 0
                && owner.compare_exchange(0, pid as u64, Ordering::AcqRel, Ordering::Acquire).is_ok())
        {
            INFLIGHT_SLOT.store(slot, Ordering::Relaxed);
            INFLIGHT_PID.store(pid, Ordering::Relaxed);
            return Some(slot);
        }
    }
    None
}

/// Marks this process as serving a request of `route_id` (or of the pending
/// label), so the master can count it if the process dies before answering.
pub(crate) fn begin_in_flight(base: usize, route_id: usize) {
    if !TRACK_IN_FLIGHT.load(Ordering::Relaxed) {
        return;
    }
    let Some(slot) = inflight_slot(base) else {
        return;
    };
    unsafe {
        inflight_word(base, slot, 2).store(monotonic_ns(), Ordering::Relaxed);
        inflight_word(base, slot, 3).store(route_id as u64, Ordering::Relaxed);
        let pending = PENDING_LEN.load(Ordering::Relaxed);
        let label = base + INFLIGHT_TABLE_OFFSET + (slot * INFLIGHT_WORDS + 4) * 8;
        std::ptr::copy_nonoverlapping(
            std::ptr::addr_of!(PENDING_ROUTE) as *const u8,
            (label + 1) as *mut u8,
            pending,
        );
        (*(label as *const std::sync::atomic::AtomicU8)).store(pending as u8, Ordering::Relaxed);
        for kind in 0..apm::IO_KINDS {
            inflight_word(base, slot, INFLIGHT_IO_WORD + kind).store(0, Ordering::Relaxed);
        }
        inflight_word(base, slot, 1).store(INFLIGHT_SERVING, Ordering::Release);
    }
}

/// Empties the I/O this process's in-flight row holds for its request.
fn clear_in_flight_io(base: usize) {
    if INFLIGHT_PID.load(Ordering::Relaxed) != unsafe { libc::getpid() } {
        return;
    }
    let slot = INFLIGHT_SLOT.load(Ordering::Relaxed);
    if slot < INFLIGHT_SLOTS {
        for kind in 0..apm::IO_KINDS {
            unsafe { inflight_word(base, slot, INFLIGHT_IO_WORD + kind) }.store(0, Ordering::Relaxed);
        }
    }
}

/// Adds `amount` of I/O `kind` to this process's in-flight row, when it
/// serves a request whose route is pending: the master charges it to the
/// request if the process dies serving it.
fn note_in_flight_io(base: usize, kind: usize, amount: u64) {
    if !TRACK_IN_FLIGHT.load(Ordering::Relaxed) || INFLIGHT_PID.load(Ordering::Relaxed) != unsafe { libc::getpid() } {
        return;
    }
    let slot = INFLIGHT_SLOT.load(Ordering::Relaxed);
    if slot < INFLIGHT_SLOTS {
        unsafe { inflight_word(base, slot, INFLIGHT_IO_WORD + kind) }.fetch_add(amount, Ordering::Relaxed);
    }
}

/// Marks this process's request as being counted, before its row is written:
/// a process killed between counting it and giving its row back must not have
/// it counted a second time, as a 500, by the master's reap (review). The
/// reap counts a row only while it reads "serving".
pub(crate) fn answering_in_flight(base: usize) {
    if INFLIGHT_PID.load(Ordering::Relaxed) != unsafe { libc::getpid() } {
        return;
    }
    let slot = INFLIGHT_SLOT.load(Ordering::Relaxed);
    if slot < INFLIGHT_SLOTS {
        unsafe { inflight_word(base, slot, 1) }.store(INFLIGHT_ANSWERING, Ordering::Release);
    }
}

/// In-flight row state: the process is serving the request.
pub(crate) const INFLIGHT_SERVING: u64 = 1;
/// In-flight row state: the process is counting the request it answered.
pub(crate) const INFLIGHT_ANSWERING: u64 = 2;

/// Marks this process's request as answered, and gives its row back.
///
/// Freed at every answer rather than kept per process: a row that outlives
/// its request is a row a later process with the same pid could be charged a
/// request through.
pub(crate) fn end_in_flight(base: usize) {
    if INFLIGHT_PID.load(Ordering::Relaxed) != unsafe { libc::getpid() } {
        return;
    }
    let slot = INFLIGHT_SLOT.swap(usize::MAX, Ordering::Relaxed);
    if slot < INFLIGHT_SLOTS {
        unsafe {
            inflight_word(base, slot, 1).store(0, Ordering::Release);
            inflight_word(base, slot, 0).store(0, Ordering::Release);
        }
    }
}

/// Counts the request a dead process was serving, as a server error.
///
/// Called by the `--web` master when it reaps a worker. A request whose PHP
/// ended the process — an uncaught exception, a fatal error, `exit`, a
/// `--max-execution-time` kill — never reached `elephc_probe_note_transaction`,
/// so without this it was missing from the counts while it failed: the error
/// rate read healthy exactly when requests were dying. Its time runs from the
/// request's start to the reap.
///
/// Every row the process held: a prefork worker holds at most one, but an
/// isolated web worker serves many requests at once and holds one per
/// request (`elephc_probe_request_begin`).
#[no_mangle]
pub extern "C" fn elephc_probe_reap(pid: i32) {
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 || !APM_ON.load(Ordering::Relaxed) || pid <= 0 {
        return;
    }
    let _gate = TableGate::enter(base);
    // Every row: rows are given back at each answer, so a pid's rows can sit
    // past free ones. Reaping is rare; the scan is a thousand loads.
    for slot in 0..INFLIGHT_SLOTS {
        let owner = unsafe { inflight_word(base, slot, 0) };
        if owner.load(Ordering::Acquire) != pid as u64 {
            continue;
        }
        let state = unsafe { inflight_word(base, slot, 1) };
        // Only a request still being served: one being counted when its
        // process died is already in the table.
        if state.swap(0, Ordering::AcqRel) == INFLIGHT_SERVING {
            let started = unsafe { inflight_word(base, slot, 2) }.load(Ordering::Relaxed);
            let mut route_id = unsafe { inflight_word(base, slot, 3) }.load(Ordering::Relaxed) as usize;
            let label = base + INFLIGHT_TABLE_OFFSET + (slot * INFLIGHT_WORDS + 4) * 8;
            let len = unsafe { (*(label as *const std::sync::atomic::AtomicU8)).load(Ordering::Relaxed) } as usize;
            if len != 0 {
                let bytes = unsafe { std::slice::from_raw_parts((label + 1) as *const u8, len.min(ROUTE_NAME_MAX)) };
                route_id = std::str::from_utf8(bytes).map_or(0, |text| unsafe { intern_new_route(base, text) });
            }
            // The work it did while its route was pending, which only its row
            // kept.
            for kind in 0..apm::IO_KINDS {
                let amount = unsafe { inflight_word(base, slot, INFLIGHT_IO_WORD + kind) }.swap(0, Ordering::Relaxed);
                if amount != 0 && route_id < EVENT_BUCKETS {
                    unsafe { txn_word(base, route_id, apm::WORD_IO0 + kind) }.fetch_add(amount, Ordering::Relaxed);
                }
            }
            record_transaction(base, route_id, 500, monotonic_ns().saturating_sub(started));
        }
        // The row is freed for the next process that gets this slot.
        owner.store(0, Ordering::Release);
    }
}

/// Marks one request an isolated web worker started as in flight, and
/// returns its token (0 when nothing is tracked).
///
/// Under `--web-isolation=pool|request` PHP runs in handler children and the
/// web worker records each request by name once answered. A web worker killed
/// itself — by the out-of-memory killer, say — took the requests it was
/// waiting on with it, uncounted (review). A web worker serves many requests at
/// once, so each holds a row of its own, carrying its label: the master's reap
/// counts every row of the dead worker still serving as a 500.
///
/// # Safety
/// `route`/`len` describe a UTF-8 string valid for this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_request_begin(route: *const u8, len: usize) -> u64 {
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 || !APM_ON.load(Ordering::Relaxed) || route.is_null() || len == 0 || len > 4096 {
        return 0;
    }
    let _gate = TableGate::enter(base);
    let Ok(text) = std::str::from_utf8(std::slice::from_raw_parts(route, len)) else {
        return 0;
    };
    // An excluded route is never counted, so it is never in flight either.
    if !crate::apm_rules::counts(text) {
        return 0;
    }
    let pid = libc::getpid();
    // Spread the claims so concurrent requests do not race for one row.
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let start = (pid as usize).wrapping_mul(31).wrapping_add(NEXT.fetch_add(1, Ordering::Relaxed));
    for probe in 0..INFLIGHT_SLOTS {
        let slot = start.wrapping_add(probe) % INFLIGHT_SLOTS;
        let owner = inflight_word(base, slot, 0);
        if owner.compare_exchange(0, pid as u64, Ordering::AcqRel, Ordering::Acquire).is_err() {
            continue;
        }
        inflight_word(base, slot, 2).store(monotonic_ns(), Ordering::Relaxed);
        let known = find_route(text);
        inflight_word(base, slot, 3).store(known.unwrap_or(0) as u64, Ordering::Relaxed);
        let label_at = base + INFLIGHT_TABLE_OFFSET + (slot * INFLIGHT_WORDS + 4) * 8;
        let mut name = [0u8; ROUTE_NAME_MAX];
        let label = if known.is_some() { "" } else { route_key(text, &mut name) };
        std::ptr::copy_nonoverlapping(label.as_ptr(), (label_at + 1) as *mut u8, label.len());
        (*(label_at as *const std::sync::atomic::AtomicU8)).store(label.len() as u8, Ordering::Relaxed);
        for kind in 0..apm::IO_KINDS {
            inflight_word(base, slot, INFLIGHT_IO_WORD + kind).store(0, Ordering::Relaxed);
        }
        inflight_word(base, slot, 1).store(INFLIGHT_SERVING, Ordering::Release);
        return slot as u64 + 1;
    }
    0
}

/// Ends the tracking of a request `elephc_probe_request_begin` started.
///
/// `answering` nonzero marks it as being counted — called right before the
/// worker records the answer, so a death from there on does not count it a
/// second time; zero gives the row back, after the answer was recorded or when
/// the request was dropped (the client went away) and is not to be counted.
#[no_mangle]
pub extern "C" fn elephc_probe_request_end(token: u64, answering: u32) {
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 || token == 0 || token > INFLIGHT_SLOTS as u64 {
        return;
    }
    let slot = (token - 1) as usize;
    let owner = unsafe { inflight_word(base, slot, 0) };
    if owner.load(Ordering::Acquire) != unsafe { libc::getpid() } as u64 {
        return;
    }
    let state = unsafe { inflight_word(base, slot, 1) };
    if answering != 0 {
        state.store(INFLIGHT_ANSWERING, Ordering::Release);
    } else {
        state.store(0, Ordering::Release);
        owner.store(0, Ordering::Release);
    }
}

/// The gate between the processes using the route table and a reset
/// emptying it (`ROUTES_GATE_OFFSET`): `[resetting : 1][users : 32]` in one
/// word, so a process cannot slip in between seeing "no reset" and counting
/// itself in.
///
/// A process that passed the generation check, paused, then wrote a count
/// after the reset charged it to an id the next route took (review). Every
/// entry that looks a route up or writes a count is a user for its length;
/// a reset waits for the users in progress, and new ones wait for it.
///
/// Both waits are bounded: a process killed inside the gate leaves a user
/// counted forever, and one killed during a reset leaves the flag up. A
/// reset then proceeds after its wait, and users after theirs — the race
/// comes back for that moment, nothing hangs.
pub(crate) struct TableGate {
    word: usize,
}

/// The gate word's reset flag.
pub(crate) const GATE_RESETTING: u64 = 1 << 63;
/// How long a user waits for a reset to finish, and a reset for the users.
const GATE_WAIT: std::time::Duration = std::time::Duration::from_millis(100);
/// How long a reset waits for another process's reset before taking the
/// flag over as left by a dead process. A live reset clears a table in well
/// under a millisecond.
pub(crate) const RESET_TAKEOVER: std::time::Duration = std::time::Duration::from_secs(1);
/// Resets of this process, one at a time.
static RESETS: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl TableGate {
    /// Counts this process in as a user of the table, once no reset runs.
    pub(crate) fn enter(base: usize) -> Option<Self> {
        if base == 0 {
            return None;
        }
        let word = shared_word(base, ROUTES_GATE_OFFSET);
        let started = std::time::Instant::now();
        loop {
            let current = word.load(Ordering::Acquire);
            if current & GATE_RESETTING != 0 && started.elapsed() < GATE_WAIT {
                std::hint::spin_loop();
                continue;
            }
            if word.compare_exchange_weak(current, current + 1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                return Some(Self { word: base + ROUTES_GATE_OFFSET });
            }
        }
    }

    /// Raises the reset flag and waits for the users in progress, then runs
    /// `reset` with the table to itself.
    ///
    /// The flag has one owner: two resets — two connections to the endpoint,
    /// each served on its own thread — both raised it, and the first to finish
    /// lowered it while the other still cleared counters and reused route ids,
    /// letting requests and snapshots into a half-reset table (review). Resets
    /// of this process take turns on a lock; a reset of another process is
    /// waited for while it holds the flag, and a flag still up after
    /// `RESET_TAKEOVER` is a reset whose process died, which this one takes
    /// over.
    pub(crate) fn exclusive<T>(base: usize, reset: impl FnOnce() -> T) -> T {
        let _turn = RESETS.lock().unwrap_or_else(|e| e.into_inner());
        let word = shared_word(base, ROUTES_GATE_OFFSET);
        let claimed = std::time::Instant::now();
        loop {
            let current = word.load(Ordering::Acquire);
            if current & GATE_RESETTING != 0 {
                if claimed.elapsed() < RESET_TAKEOVER {
                    std::thread::yield_now();
                    continue;
                }
                break;
            }
            if word
                .compare_exchange_weak(current, current | GATE_RESETTING, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                break;
            }
        }
        let started = std::time::Instant::now();
        while word.load(Ordering::Acquire) & !GATE_RESETTING != 0 && started.elapsed() < GATE_WAIT {
            std::thread::yield_now();
        }
        let result = reset();
        word.fetch_and(!GATE_RESETTING, Ordering::AcqRel);
        result
    }
}

impl Drop for TableGate {
    /// Counts this process out.
    fn drop(&mut self) {
        let word = unsafe { &*(self.word as *const AtomicU64) };
        let _ = word.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current & !GATE_RESETTING != 0).then(|| current - 1)
        });
    }
}

/// When this thread's open transaction started (monotonic nanoseconds), 0
/// while none is open. Process-local: PHP runs on one thread.
pub(crate) static TXN_STARTED_NS: AtomicU64 = AtomicU64::new(0);
/// Whether the open transaction was marked failed (`elephc_probe_txn_fail`).
pub(crate) static TXN_FAILED: AtomicBool = AtomicBool::new(false);

/// The longest name read; the route table keeps the first `ROUTE_NAME_MAX` bytes.
const TXN_NAME_MAX: usize = 4096;

/// Opens a transaction named `name` outside HTTP, and returns its token
/// (0 when nothing is counted).
///
/// A transaction is one entry into compiled code: a message a consumer
/// handles (`MSG App\Message\SendEmail`), a call a host makes into a library
/// export (`CALL render`), a method named with `--apm-transaction`. Its name
/// takes a row of the route table like a route does, under the same rules:
/// interned once its outcome is known, within the new-route budget.
///
/// A transaction opened inside another — a message dispatched synchronously
/// while a web request runs, an export a host calls from a callback — is part
/// of the outer one and counts nothing of its own: the token is 0.
///
/// # Safety
/// `name` is null or a NUL-terminated string valid for this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_txn_begin(name: *const libc::c_char) -> u64 {
    if name.is_null() || !APM_ON.load(Ordering::Relaxed) {
        return 0;
    }
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 {
        return 0;
    }
    if in_transaction() {
        // Nested: counted as part of the open one, and remembered, so a
        // failure reported inside it does not fail the open one.
        NESTED_DEPTH.fetch_add(1, Ordering::Relaxed);
        return 0;
    }
    let _gate = TableGate::enter(base);
    let bytes = std::ffi::CStr::from_ptr(name).to_bytes();
    let Ok(text) = std::str::from_utf8(&bytes[..bytes.len().min(TXN_NAME_MAX)]) else {
        return 0;
    };
    if text.is_empty() {
        return 0;
    }
    // Excluded by the project's `[apm]` rules: not opened, and its own work goes
    // nowhere, not on an untagged row. A counted transaction opened inside it is
    // still counted, with its work.
    if !crate::apm_rules::counts(text) {
        // The state before the outermost exclusion is what its end restores: one ending
        // inside another must not count the outer one's remaining work (review).
        if EXCLUDED_DEPTH.fetch_add(1, Ordering::Relaxed) == 0 {
            EXCLUDED_BEFORE.store(crate::apm_rules::request_ignored(), Ordering::Relaxed);
        }
        crate::apm_rules::set_request_ignored(true);
        return 0;
    }
    rebase_after_reset(base);
    remember_route(text);
    PENDING_LEN.store(0, Ordering::Relaxed);
    let id = match find_route(text) {
        Some(id) => id,
        None => {
            make_current_route_pending();
            0
        }
    };
    // The route it replaces: an excluded request's, named for the sampler, which it
    // gets back when this transaction ends.
    TXN_PRIOR_ROUTE.store(CURRENT_ROUTE.load(Ordering::Relaxed), Ordering::Relaxed);
    CURRENT_ROUTE.store(id, Ordering::Relaxed);
    TXN_FAILED.store(false, Ordering::Relaxed);
    crate::apm_errors::clear();
    TXN_STARTED_NS.store(monotonic_ns().max(1), Ordering::Relaxed);
    1
}

/// How many nested transactions are open inside the current one: their
/// tokens are 0, and a failure reported from inside them is theirs.
pub(crate) static NESTED_DEPTH: AtomicU64 = AtomicU64::new(0);
/// The route a transaction this API opened replaced, given back at its end: the
/// sampler's name of an excluded request it ran inside, else 0.
pub(crate) static TXN_PRIOR_ROUTE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// How many excluded transactions are open, one inside another.
pub(crate) static EXCLUDED_DEPTH: AtomicU64 = AtomicU64::new(0);
/// Whether the request was already excluded when the outermost of them opened.
pub(crate) static EXCLUDED_BEFORE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Marks the transaction `token` names failed: an exception left it, or the
/// code that ran it reported a failure. It is counted with the 5xx. A zero
/// token is a nested transaction's: an exception the outer code catches does
/// not fail the outer transaction, and one it lets through fails it there.
#[no_mangle]
pub extern "C" fn elephc_probe_txn_fail(token: u64) {
    if token != 0 && TXN_STARTED_NS.load(Ordering::Relaxed) != 0 {
        TXN_FAILED.store(true, Ordering::Relaxed);
    }
}

/// Marks whatever transaction is open failed, when the failure is reported
/// away from the code that opened it: Messenger's worker catches a failing
/// handler's exception and reports it by building a failure event, which is
/// where this is called from (`crate::apm_prelude` in the compiler).
#[no_mangle]
pub extern "C" fn elephc_probe_txn_fail_current() {
    // Not from inside a nested transaction: a message handled synchronously
    // inside a job, whose failure the job contains, failed the job (review).
    // An exception the nested code lets through fails the open one where it
    // leaves it.
    if TXN_STARTED_NS.load(Ordering::Relaxed) != 0 && NESTED_DEPTH.load(Ordering::Relaxed) == 0 {
        TXN_FAILED.store(true, Ordering::Relaxed);
    }
}

/// Closes the transaction `token` names, counting it with its wall time:
/// failed when `failed` is nonzero or it was marked failed, succeeded
/// otherwise. A zero token closes nothing.
#[no_mangle]
pub extern "C" fn elephc_probe_txn_end(token: u64, failed: u32) {
    // A zero token is a nested transaction's: the outer one stays open.
    if token == 0 {
        // Not a nested one: an excluded one (or one never opened), which ends here. The
        // outermost exclusion's end restores what was before it.
        if NESTED_DEPTH.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |depth| depth.checked_sub(1)).is_err()
            && EXCLUDED_DEPTH.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |depth| depth.checked_sub(1)) == Ok(1)
        {
            crate::apm_rules::set_request_ignored(EXCLUDED_BEFORE.load(Ordering::Relaxed));
        }
        return;
    }
    NESTED_DEPTH.store(0, Ordering::Relaxed);
    let started = TXN_STARTED_NS.swap(0, Ordering::Relaxed);
    if started == 0 {
        return;
    }
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    let _gate = TableGate::enter(base);
    let failed = failed != 0 || TXN_FAILED.swap(false, Ordering::Relaxed);
    let status = if failed { 500 } else { 200 };
    rebase_after_reset(base);
    let route_id = unsafe { pending_route_id(status) }.unwrap_or_else(|| CURRENT_ROUTE.load(Ordering::Relaxed));
    flush_pending_io(base, route_id);
    record_transaction(base, route_id, status, monotonic_ns().saturating_sub(started));
    CURRENT_ROUTE.store(TXN_PRIOR_ROUTE.swap(0, Ordering::Relaxed), Ordering::Relaxed);
    CURRENT_LABEL_LEN.store(0, Ordering::Relaxed);
}

/// When this program's run started (monotonic nanoseconds), 0 when its run
/// is not counted: a script built with `--apm-collector` counts its whole run
/// as `CLI <script> [<command>]`, like a run `monitor` launched (review).
pub(crate) static RUN_STARTED_NS: AtomicU64 = AtomicU64::new(0);
/// The runs this process ended, with the exact code each exited with, sent in every
/// later snapshot: a program pushing to a collector reports its run in its last send.
pub(crate) static RUN_EXITS: std::sync::Mutex<Vec<(String, i32)>> = std::sync::Mutex::new(Vec::new());
/// Database and network work of the run outside its transactions, by
/// `apm::IO_*` index: charged to the run's row once the run is counted.
pub(crate) static RUN_IO: [AtomicU64; apm::IO_KINDS] = [const { AtomicU64::new(0) }; apm::IO_KINDS];

/// Starts counting this program's run, from main's prologue. Unlike a
/// transaction it does not hold the current one: the messages and named
/// boundaries a consumer runs inside its run are counted as their own.
#[no_mangle]
pub extern "C" fn elephc_probe_run_begin() {
    if APM_ON.load(Ordering::Relaxed) {
        RUN_STARTED_NS.store(monotonic_ns().max(1), Ordering::Relaxed);
        crate::apm_errors::clear();
    }
}

/// Counts what the process leaves open as it exits with `exit_code`, before
/// its last snapshot is taken: a transaction whose `finally` an `exit()`
/// skipped (review), failed when the exit is, and the program's run.
pub(crate) fn close_at_exit(exit_code: i32) {
    if TXN_STARTED_NS.load(Ordering::Relaxed) != 0 {
        elephc_probe_txn_end(1, u32::from(exit_code != 0));
    }
    let started = RUN_STARTED_NS.swap(0, Ordering::Relaxed);
    let base = REGION.load(Ordering::Relaxed);
    if started == 0 || base == 0 || !APM_ON.load(Ordering::Relaxed) {
        return;
    }
    // `args_os`, lossily: `args` panics on an argument that is not UTF-8, and a
    // panic here aborted the exit of a program PHP ran fine with it (review).
    let args: Vec<String> = std::env::args_os().map(|arg| arg.to_string_lossy().into_owned()).collect();
    let Some((script, rest)) = args.split_first() else {
        return;
    };
    let name = apm::cli_run_name(script, rest);
    // A run the project's `[apm]` rules exclude is not counted, nor its work.
    if !crate::apm_rules::counts(&name) {
        for counter in &RUN_IO {
            counter.store(0, Ordering::Relaxed);
        }
        return;
    }
    let _gate = TableGate::enter(base);
    let route_id = unsafe { intern_new_route(base, &name) };
    if route_id < EVENT_BUCKETS {
        for (kind, counter) in RUN_IO.iter().enumerate() {
            let amount = counter.swap(0, Ordering::Relaxed);
            if amount != 0 {
                unsafe { txn_word(base, route_id, apm::WORD_IO0 + kind) }.fetch_add(amount, Ordering::Relaxed);
            }
        }
    }
    let status = if exit_code == 0 { 200 } else { 500 };
    record_transaction(base, route_id, status, monotonic_ns().saturating_sub(started));
    RUN_EXITS.lock().unwrap_or_else(|e| e.into_inner()).push((name, exit_code));
}

/// Whether this process is inside a counted transaction already: one this API
/// opened, or a web request (its route stamped, known or pending). An excluded
/// request is none: the route it carries names it for the sampler only, and a
/// counted boundary inside it opens a transaction of its own (review).
fn in_transaction() -> bool {
    TXN_STARTED_NS.load(Ordering::Relaxed) != 0
        || (!crate::apm_rules::request_ignored()
            && (CURRENT_ROUTE.load(Ordering::Relaxed) != 0 || PENDING_LEN.load(Ordering::Relaxed) != 0))
}

/// Adds `amount` to one of the current route's `--apm` I/O counters.
///
/// A request whose route is not in the table yet (see `PENDING_ROUTE`) keeps
/// its counters here, in this process, until its outcome decides which row
/// they belong to.
pub(crate) fn note_apm_io(kind: usize, amount: u64) {
    if !APM_ON.load(Ordering::Relaxed) || kind >= apm::IO_KINDS {
        return;
    }
    // An excluded request's or transaction's own work is not counted anywhere, not on
    // an untagged row either; a counted transaction open inside it keeps its own.
    // Not even when the sampler named the request: it would wait to be published with a
    // request that is never recorded, and be charged to the next one.
    if crate::apm_rules::request_ignored() && TXN_STARTED_NS.load(Ordering::Relaxed) == 0 {
        return;
    }
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    // A request's or transaction's work stays in this process until it is recorded
    // (`flush_pending_io`), published once with it: a statement costs no pass through the
    // table gate and no write to a counter every worker of the route shares. It is mirrored
    // in this process's own in-flight row, so the master still charges it to the request
    // should the process die serving it.
    //
    // Not in an isolated handler (`--web-isolation=pool|request`): nothing tracks it in
    // flight, and a handler that dies never settles, so a request on a known route writes
    // its work to the route's row as each statement runs, as before (review). A route
    // still pending keeps its work here, as it always did.
    let route = CURRENT_ROUTE.load(Ordering::Relaxed);
    if !TRACK_IN_FLIGHT.load(Ordering::Relaxed) && TXN_STARTED_NS.load(Ordering::Relaxed) == 0 && route != 0 {
        let _gate = TableGate::enter(base);
        rebase_after_reset(base);
        // A reset under the request made its route pending: its work then waits below.
        let route = CURRENT_ROUTE.load(Ordering::Relaxed);
        if route != 0 && route < EVENT_BUCKETS {
            unsafe { txn_word(base, route, apm::WORD_IO0 + kind) }.fetch_add(amount, Ordering::Relaxed);
            return;
        }
    }
    // should the process die serving it.
    if in_transaction() {
        PENDING_IO[kind].fetch_add(amount, Ordering::Relaxed);
        note_in_flight_io(base, kind, amount);
        return;
    }
    // Work outside every transaction of a script counting its run is the
    // run's: kept here until its row is known, at exit (review).
    if RUN_STARTED_NS.load(Ordering::Relaxed) != 0 {
        RUN_IO[kind].fetch_add(amount, Ordering::Relaxed);
        return;
    }
    // Work outside any transaction: the untagged row, shared.
    let _gate = TableGate::enter(base);
    unsafe { txn_word(base, 0, apm::WORD_IO0 + kind) }.fetch_add(amount, Ordering::Relaxed);
}

/// The label of this process's current request, whatever its route's state:
/// what names it again if the route table is reset while it runs.
pub(crate) static mut CURRENT_LABEL: [u8; ROUTE_NAME_MAX] = [0; ROUTE_NAME_MAX];
/// Length of `CURRENT_LABEL`.
pub(crate) static CURRENT_LABEL_LEN: AtomicUsize = AtomicUsize::new(0);
/// The route-table generation this process's route id belongs to.
pub(crate) static GENERATION_SEEN: AtomicU64 = AtomicU64::new(0);

/// A word of the shared region's tail, by byte offset.
pub(crate) fn shared_word<'a>(base: usize, offset: usize) -> &'a AtomicU64 {
    unsafe { &*((base + offset) as *const AtomicU64) }
}

/// Remembers the current request's label and the route-table generation its
/// route id will be looked up in.
///
/// # Safety
/// Called on the thread serving the request (one request at a time).
pub(crate) unsafe fn remember_route(text: &str) {
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    GENERATION_SEEN.store(shared_word(base, ROUTES_GENERATION_OFFSET).load(Ordering::Acquire), Ordering::Relaxed);
    let mut name = [0u8; ROUTE_NAME_MAX];
    let label = route_key(text, &mut name);
    std::ptr::copy_nonoverlapping(label.as_ptr(), std::ptr::addr_of_mut!(CURRENT_LABEL) as *mut u8, label.len());
    CURRENT_LABEL_LEN.store(label.len(), Ordering::Relaxed);
}

/// Makes the current request's route pending: it is interned once the
/// request's outcome is known (see `PENDING_ROUTE`).
///
/// # Safety
/// Called on the thread serving the request.
pub(crate) unsafe fn make_current_route_pending() {
    let len = CURRENT_LABEL_LEN.load(Ordering::Relaxed);
    std::ptr::copy_nonoverlapping(
        std::ptr::addr_of!(CURRENT_LABEL) as *const u8,
        std::ptr::addr_of_mut!(PENDING_ROUTE) as *mut u8,
        len,
    );
    PENDING_LEN.store(len, Ordering::Relaxed);
}

/// After a route-table reset, the current request's route id names nothing
/// (or another route): its route is made pending again, by its label, so its
/// work and its answer land on the row the new table gives it.
pub(crate) fn rebase_after_reset(base: usize) {
    let generation = shared_word(base, ROUTES_GENERATION_OFFSET).load(Ordering::Acquire);
    if generation == GENERATION_SEEN.load(Ordering::Relaxed) {
        return;
    }
    GENERATION_SEEN.store(generation, Ordering::Relaxed);
    if CURRENT_ROUTE.swap(0, Ordering::Relaxed) != 0 && PENDING_LEN.load(Ordering::Relaxed) == 0 {
        unsafe { make_current_route_pending() };
    }
}

/// Empties the route table: every route, its counters and its name, so the
/// table fills again from the traffic that comes next.
///
/// The way out of a table a scan filled: its 255 rows are otherwise never
/// given back. The service's counters start a new run (a new epoch), which a
/// collector reads as a restart — nothing counted twice, nothing negative.
/// Requests in flight keep their route by name; samples the profiler took
/// before are read without a route, their ids belonging to the old table.
/// Called from the endpoint, in the master, for an authenticated client.
pub fn reset_routes() -> String {
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 {
        return "this service has no route table\n".to_string();
    }
    // The new run's CPU baseline is measured first, outside the gate: it
    // walks the whole process tree, and requests waited at the gate for that
    // walk on every reset (review).
    let cpu_now = resources::service_usage().cpu_ns;
    // With the table to itself: no process is between a lookup and a write.
    TableGate::exclusive(base, || reset_routes_exclusive(base, cpu_now))
}

/// The reset itself, run while no process uses the table (`reset_routes`).
pub(crate) fn reset_routes_exclusive(base: usize, cpu_now: u64) -> String {
    let routes = unsafe { (*((base + RING_BYTES) as *const AtomicU64)).load(Ordering::Acquire) }
        .min(OTHER_ROUTE_INDEX as u64);
    // Requests in flight, by name rather than by an id about to mean nothing.
    for slot in 0..INFLIGHT_SLOTS {
        let state = unsafe { inflight_word(base, slot, 1) }.load(Ordering::Acquire);
        let id_word = unsafe { inflight_word(base, slot, 3) };
        let id = id_word.load(Ordering::Relaxed) as usize;
        let label_at = base + INFLIGHT_TABLE_OFFSET + (slot * INFLIGHT_WORDS + 4) * 8;
        let len = unsafe { &*(label_at as *const std::sync::atomic::AtomicU8) };
        if state != INFLIGHT_SERVING || id == 0 || len.load(Ordering::Relaxed) != 0 {
            continue;
        }
        if let Some(name) = unsafe { route_name(id) } {
            let name = &name.as_bytes()[..name.len().min(ROUTE_NAME_MAX)];
            unsafe { std::ptr::copy_nonoverlapping(name.as_ptr(), (label_at + 1) as *mut u8, name.len()) };
            len.store(name.len() as u8, Ordering::Relaxed);
        }
        id_word.store(0, Ordering::Relaxed);
    }
    // Samples taken so far keep their stacks, without a route.
    let head = unsafe { region_head() }.map_or(0, |head| head.load(Ordering::Acquire));
    shared_word(base, ROUTES_RESET_TICKET_OFFSET).store(head, Ordering::Release);
    // The counters, the per-route profiling events, the names and the ration.
    for route_id in 0..EVENT_BUCKETS {
        for word in 0..apm::ROUTE_WORDS {
            unsafe { txn_word(base, route_id, word) }.store(0, Ordering::Relaxed);
        }
    }
    reset_event_counters(base);
    // The errors name routes by id, which the emptied table gives to others.
    crate::apm_errors::reset(base);
    unsafe { (*((base + RING_BYTES) as *const AtomicU64)).store(0, Ordering::Release) };
    shared_word(base, ROUTE_BUDGET_OFFSET).store(0, Ordering::Relaxed);
    shared_word(base, ROUTES_GENERATION_OFFSET).fetch_add(1, Ordering::AcqRel);
    // A new run of the counters, so a reader diffing against the old one does
    // not subtract a zeroed table from a full one.
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(1);
    let previous = APM_EPOCH.load(Ordering::Relaxed);
    // The new run's CPU starts here too: a reader takes a new epoch for a
    // restart and charges the run its whole CPU total, which would otherwise
    // still hold the service's lifetime (review).
    CPU_AT_RESET_NS.store(cpu_now, Ordering::Relaxed);
    APM_EPOCH.store(epoch.max(previous + 1), Ordering::Relaxed);
    format!("route table reset: {routes} routes cleared\n")
}

/// The label of this process's current request when its route is not in the
/// table yet, so it is interned only once the request's outcome is known.
///
/// Under `--apm` every request would otherwise intern its path at its start,
/// and the table — 255 shapes, never evicted — filled with whatever a scanner
/// sent: every endpoint first seen afterwards was counted under `<other>`. A
/// request of an unseen route answered "not found" now goes to one
/// `<not found>` row instead of spending a slot. Process-local: a worker serves one request at a time.
pub(crate) static mut PENDING_ROUTE: [u8; ROUTE_NAME_MAX] = [0; ROUTE_NAME_MAX];
/// Length of `PENDING_ROUTE`; 0 when the current request's route is known.
pub(crate) static PENDING_LEN: AtomicUsize = AtomicUsize::new(0);
/// Database and network work of the pending request, by `apm::IO_*` index.
pub(crate) static PENDING_IO: [AtomicU64; apm::IO_KINDS] = [const { AtomicU64::new(0) }; apm::IO_KINDS];

/// The row requests answered "not found" (404, 405, 410) are counted under.
pub(crate) const NOT_FOUND_ROUTE_NAME: &str = "<not found>";

/// Whether a status says the path served nothing, so it names no route.
pub(crate) fn is_not_found(status: u32) -> bool {
    matches!(status, 404 | 405 | 410)
}

/// Records one database or network event for `--apm`, from the exact runtime.
///
/// A combined `--with-monitoring` binary gives the bridges' event slots to the
/// exact profiler, which drops every event while no slice is being recorded.
/// It forwards each one here first, through the `elephc_probe_apm_event_fn`
/// slot, so a service started with `--apm` counts its routes' queries and
/// network calls whether or not anyone is profiling. `kind` is an
/// `apm::IO_*` index; `amount` is 1 for an operation, nanoseconds for a wait.
#[no_mangle]
pub extern "C" fn elephc_probe_apm_event(kind: u32, amount: u64) {
    note_apm_io(kind as usize, amount);
}

/// Renders every route's transaction counters as an `apm::Snapshot` text.
///
/// Answers `apm::SNAPSHOT_OFF` for a service started without `--apm`, so the
/// client can say which flag is missing instead of showing an idle service.
/// Rows that share a name — two workers racing to intern the same route — are
/// merged, because a reader keyed by name would otherwise see one of them
/// replace the other between polls.
pub fn apm_snapshot_text() -> String {
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 || !APM_ON.load(Ordering::Acquire) {
        return apm::SNAPSHOT_OFF.to_string();
    }
    // Counters, epoch and CPU baseline must come from one run of the
    // counters: a reset between them paired the old counters with the new
    // run's epoch, and a reader counted them again (review). The table gate
    // keeps a reset out while the read runs, but its waits are bounded, and a
    // read of the whole process tree can outlast them; so the read is also
    // checked, as a sequence lock is: no reset in progress before or after
    // it, and the same route-table generation at both ends. A read a reset
    // crossed is taken again.
    // When resets keep crossing every read, no consistent answer exists: the
    // reader is told so and asks again on its next poll, rather than given
    // counters it would count twice (review).
    for _ in 0..SNAPSHOT_ATTEMPTS {
        let before = reset_mark(base);
        let (mut snapshot, cpu_at_reset) = {
            let _gate = TableGate::enter(base);
            read_snapshot(base)
        };
        // The process tree is walked after the gate is left: the gate keeps a
        // reset out, and a reset waited for that walk (review). The mark
        // below still covers it, so a reset during the walk is read again.
        let mut usage = resources::service_usage();
        usage.cpu_ns = usage.cpu_ns.saturating_sub(cpu_at_reset);
        snapshot.resources = Some(usage);
        if before.is_some() && before == reset_mark(base) {
            return apm::encode(&snapshot);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    apm::SNAPSHOT_BUSY.to_string()
}

/// How many times a snapshot is read again when resets keep crossing it.
const SNAPSHOT_ATTEMPTS: usize = 20;

/// The route-table generation when no reset is in progress, `None` while one
/// is: equal marks around a read mean no reset touched it.
fn reset_mark(base: usize) -> Option<u64> {
    if shared_word(base, ROUTES_GATE_OFFSET).load(Ordering::Acquire) & GATE_RESETTING != 0 {
        return None;
    }
    Some(shared_word(base, ROUTES_GENERATION_OFFSET).load(Ordering::Acquire))
}

/// One read of every route's counters, with the run they belong to and the
/// run's CPU baseline; the resources are measured by the caller.
fn read_snapshot(base: usize) -> (apm::Snapshot, u64) {
    let epoch = APM_EPOCH.load(Ordering::Relaxed);
    let cpu_at_reset = CPU_AT_RESET_NS.load(Ordering::Relaxed);
    let mut routes: Vec<apm::RouteStats> = Vec::new();
    for route_id in 0..EVENT_BUCKETS {
        let count = unsafe { txn_word(base, route_id, apm::WORD_COUNT) }.load(Ordering::Acquire);
        // A route can have work before its first request ends: a slow one
        // runs its queries first. Those rows are kept so the work is not lost.
        let busy = (0..apm::IO_KINDS)
            .any(|kind| unsafe { txn_word(base, route_id, apm::WORD_IO0 + kind) }.load(Ordering::Relaxed) != 0);
        if count == 0 && !busy {
            continue;
        }
        let name = if route_id == 0 {
            "<untagged>".to_string()
        } else {
            unsafe { read_route_slot(base, route_id - 1) }
        };
        let mut row = apm::RouteStats::empty(&name);
        row.count = count;
        for class in 0..apm::STATUS_CLASSES {
            row.status[class] = unsafe { txn_word(base, route_id, apm::WORD_STATUS0 + class) }
                .load(Ordering::Relaxed);
        }
        row.sum_ns = unsafe { txn_word(base, route_id, apm::WORD_SUM_NS) }.load(Ordering::Relaxed);
        for (kind, slot) in row.io.iter_mut().enumerate() {
            *slot = unsafe { txn_word(base, route_id, apm::WORD_IO0 + kind) }.load(Ordering::Relaxed);
        }
        for (bucket, slot) in row.hist.iter_mut().enumerate() {
            *slot = unsafe { txn_word(base, route_id, apm::HEADER_WORDS + bucket) }
                .load(Ordering::Relaxed);
        }
        for (word, slot) in row.mem.iter_mut().enumerate() {
            *slot = unsafe { txn_word(base, route_id, apm::WORD_MEM_N + word) }.load(Ordering::Relaxed);
        }
        match routes.iter_mut().find(|existing| existing.name == row.name) {
            Some(existing) => existing.absorb(&row),
            None => routes.push(row),
        }
    }
    let mut errors: Vec<apm::ErrorStats> = Vec::new();
    for (route_id, class, location, count, trace) in crate::apm_errors::read(base) {
        let route = match route_id {
            0 => "<untagged>".to_string(),
            id if id < EVENT_BUCKETS => unsafe { read_route_slot(base, id - 1) },
            _ => continue,
        };
        let row = apm::ErrorStats { route, class, location, count, trace };
        // Two ids of one name (a route interned twice across a reset race) are one row.
        match errors.iter_mut().find(|e| e.route == row.route && e.class == row.class && e.location == row.location) {
            Some(existing) => existing.count += row.count,
            None => errors.push(row),
        }
    }
    let time_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0);
    let build = crate::apm_rules::build().map(str::to_string);
    (
        apm::Snapshot {
            resources: None,
            build,
            errors,
            exits: RUN_EXITS.lock().map(|exits| exits.clone()).unwrap_or_default(),
            epoch,
            time_ms,
            routes,
        },
        cpu_at_reset,
    )
}
