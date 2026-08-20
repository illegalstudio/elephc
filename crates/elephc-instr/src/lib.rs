//! Purpose:
//! Exact per-function instrumentation runtime for programs compiled with
//! `--instrument`. The compiler calls `elephc_instr_enter(id, allocs, frees)` in
//! every PHP function's prologue and `elephc_instr_exit(id, allocs, frees)` in
//! its epilogue; this crate maintains a shadow call stack and, from it, exact
//! per-function **inclusive** time, **exclusive** (self) time, **call counts**,
//! exact **allocation counts**, **retained** objects (allocated minus freed),
//! and caller→callee **edges** — the deterministic complement to the statistical
//! `--probe`/`monitor` sampler. The `allocs`/`frees` arguments are the program's
//! monotonic heap counters (`_gc_allocs` / `_gc_frees`) read by the compiler at
//! the call site, so both are attributed exactly the way time is. At exit
//! `elephc_instr_dump()` writes the table to stderr.
//!
//! Called from:
//! - compiler-emitted prologue/epilogue/exit code in `--instrument` binaries.
//!
//! Key details:
//! - Accounting is exact and recursion-safe for BOTH dimensions: inclusive is
//!   the outermost activation span (a per-function depth counter), exclusive is
//!   the frame delta minus its children's deltas. Time and allocations use the
//!   same shadow-stack math, so exclusives sum to the root's inclusive for each.
//! - State is thread-local; the dump reports the calling thread (the main
//!   thread at exit). elephc programs are overwhelmingly single-threaded, so
//!   this captures the whole program in the common case.
//! - `exit` resynchronizes if a frame was unwound by an exception without its
//!   exit running: it pops stale frames until the ids match, CLOSING each one at
//!   the instant the throw is observed passing it, so its cost stays on it
//!   instead of landing in the catching function's own time.
//! - The stack assumes strict call/return order. A suspended GENERATOR breaks
//!   that: its frame stays pushed, so work between two resumes is recorded as
//!   nested inside it. Exclusive time is still right — the frame's own delta
//!   subtracts its children — but the edges, and so the caller attribution,
//!   name the generator rather than the loop that did the work. Fixing it needs
//!   a pop on yield and a push on resume, which the codegen does not emit.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

/// Cap on live stack depth; deeper recursion stops being tracked (guarded, not
/// UB, and reported rather than silent).
///
/// It was 4096, on the claim that this "comfortably exceeds any hand-written PHP
/// call depth". That is true of hand-written *nesting* and false of recursion: a
/// naive tree walk, a recursive descent over a deep structure, or an unbounded
/// `fib` all pass it easily — the audit fixture for this profiler did, which is
/// how the cap's accounting bugs were found in the first place.
///
/// The stack is a `Vec`, so the cap buys no memory up front: a program only pays
/// for the depth it reaches (~88 bytes a frame, so ~5.6 MB at the ceiling, and
/// that only if something really goes 65k deep). Raising it costs nothing and
/// removes the caveat from almost every real profile; the guard stays, because a
/// runaway recursion must still degrade instead of growing without bound.
const MAX_STACK: usize = 65_536;

/// Per-function accumulators, indexed by the compiler-assigned function id.
#[derive(Clone, Copy, Default)]
struct FnAcc {
    calls: u64,
    incl_ns: u64,
    excl_ns: u64,
    incl_allocs: u64,
    excl_allocs: u64,
    /// Heap frees attributed to this function. Subtracted from the allocation
    /// counts at render time to give **retained** objects (what a call left on
    /// the heap) — the leak dimension.
    incl_frees: u64,
    excl_frees: u64,
    /// I/O operations (currently DB queries) attributed to this function.
    incl_io: u64,
    excl_io: u64,
    /// Nanoseconds blocked inside I/O calls, attributed to this function. Self
    /// time minus self wait is the function's actual CPU time.
    incl_wait: u64,
    excl_wait: u64,
    /// Live activations on the stack — inclusive is credited only when this
    /// returns to zero, so recursion is not double counted.
    depth: u32,
    /// Timestamp / allocation count / free count / io count at the outermost
    /// active entry.
    t_outer: u64,
    a_outer: u64,
    f_outer: u64,
    io_outer: u64,
    w_outer: u64,
}

/// One live call-stack frame.
struct Frame {
    id: u32,
    t_enter: u64,
    a_enter: u64,
    f_enter: u64,
    io_enter: u64,
    w_enter: u64,
    /// Summed elapsed time / allocations / frees / io ops / io wait of this
    /// frame's direct callees.
    children_ns: u64,
    children_allocs: u64,
    children_frees: u64,
    children_io: u64,
    children_wait: u64,
}

/// Thread-local instrumentation state.
#[derive(Default)]
struct State {
    fns: Vec<FnAcc>,
    stack: Vec<Frame>,
    /// (caller_id, callee_id) → (call_count, summed callee inclusive ns).
    edges: HashMap<(u32, u32), (u64, u64), BuildIdHasher>,
    /// Pushes dropped at MAX_STACK (reported so silent truncation is visible).
    dropped: u64,
    /// Activations running right now that were never pushed, because the stack
    /// was already full. Their exits must be ignored symmetrically: an exit
    /// looking for a frame that was never pushed would resync-pop the entire
    /// stack, discarding every enclosing function's accounting.
    dropped_depth: u32,
    /// Per-call spans `(id, enter_ns, exit_ns)` recorded only when tracing is on
    /// (`ELEPHC_INSTR_TRACE`), bounded by `TRACE_CAP`. Written as a Chrome trace.
    trace: Vec<(u32, u64, u64)>,
    /// Calls not recorded because the trace buffer was full.
    trace_dropped: u64,
}

impl State {
    fn ensure(&mut self, id: u32) {
        let idx = id as usize;
        if idx >= self.fns.len() {
            self.fns.resize(idx + 1, FnAcc::default());
        }
    }

    /// Records entry to `id` with the timestamp `t`, allocation counter `a`,
    /// free counter `f`, io counter `io`, and io-wait nanoseconds `w` sampled
    /// at the call site.
    fn enter_at(&mut self, id: u32, t: u64, a: u64, f: u64, io: u64, w: u64) {
        self.ensure(id);
        let parent = self.stack.last().map(|f| f.id);
        if let Some(pid) = parent {
            self.edges.entry((pid, id)).or_insert((0, 0)).0 += 1;
        }
        // Past the cap this activation cannot be timed, and must not pretend to
        // be: raising its depth would leave the function permanently "active",
        // so its inclusive time is never credited, while its exit would hunt
        // the stack for a frame that was never pushed and pop everything on the
        // way. The call still counts — it did happen — but nothing else does.
        if self.stack.len() >= MAX_STACK {
            self.dropped += 1;
            self.dropped_depth = self.dropped_depth.saturating_add(1);
            self.fns[id as usize].calls += 1;
            return;
        }
        let acc = &mut self.fns[id as usize];
        if acc.depth == 0 {
            acc.t_outer = t;
            acc.a_outer = a;
            acc.f_outer = f;
            acc.io_outer = io;
            acc.w_outer = w;
        }
        acc.depth += 1;
        acc.calls += 1;
        self.stack.push(Frame {
            id,
            t_enter: t,
            a_enter: a,
            f_enter: f,
            io_enter: io,
            w_enter: w,
            children_ns: 0,
            children_allocs: 0,
            children_frees: 0,
            children_io: 0,
            children_wait: 0,
        });
    }

    /// Records exit from `id` with timestamp `t`, allocation counter `a`, free
    /// counter `f`, io counter `io`, and io-wait nanoseconds `w`, resyncing past
    /// any frames left by exception unwinding.
    fn exit_at(&mut self, id: u32, t: u64, a: u64, f: u64, io: u64, w: u64) {
        // Dropped activations can only exist while the stack is full, so a
        // shorter stack means the count is stale — left behind by an exception
        // that unwound the dropped region — and is cleared rather than eating
        // the exits of frames that WERE tracked.
        if self.stack.len() < MAX_STACK {
            self.dropped_depth = 0;
        } else if self.dropped_depth > 0 {
            self.dropped_depth -= 1;
            return;
        }
        // Frames an exception unwound never ran their own exit hook. Closing
        // them HERE, at the instant the throw is observed passing them, keeps
        // their cost on them; simply discarding them left it inside the
        // catching function's own time, which then reads as the hot function
        // (measured: a catcher showing 99.7% self time for work it never did).
        while let Some(top) = self.stack.last() {
            if top.id == id {
                break;
            }
            let stale = self.stack.pop().expect("last() was Some");
            self.close_frame(stale, t, a, f, io, w);
        }
        let Some(frame) = self.stack.pop() else {
            return;
        };
        if frame.id != id {
            return;
        }
        if TRACE_ON.load(Ordering::Relaxed) {
            if self.trace.len() < TRACE_CAP.load(Ordering::Relaxed) {
                self.trace.push((id, frame.t_enter, t));
            } else {
                self.trace_dropped += 1;
            }
        }
        self.close_frame(frame, t, a, f, io, w);
    }

    /// Accounts one frame that is ending now: its own cost, its inclusive span
    /// when the outermost activation closes, and its total charged to the
    /// caller it is returning to.
    ///
    /// Shared by the normal return path and the exception-unwind path, so an
    /// unwound frame is measured the same way a returning one is — the two
    /// must not drift, or the exclusives stop partitioning the root.
    fn close_frame(&mut self, frame: Frame, t: u64, a: u64, f: u64, io: u64, w: u64) {
        let id = frame.id;
        let elapsed_ns = t.wrapping_sub(frame.t_enter);
        let elapsed_allocs = a.wrapping_sub(frame.a_enter);
        let elapsed_frees = f.wrapping_sub(frame.f_enter);
        let elapsed_io = io.wrapping_sub(frame.io_enter);
        let elapsed_wait = w.wrapping_sub(frame.w_enter);
        let acc = &mut self.fns[id as usize];
        acc.excl_ns = acc.excl_ns.wrapping_add(elapsed_ns.wrapping_sub(frame.children_ns));
        acc.excl_allocs = acc
            .excl_allocs
            .wrapping_add(elapsed_allocs.wrapping_sub(frame.children_allocs));
        acc.excl_frees = acc
            .excl_frees
            .wrapping_add(elapsed_frees.wrapping_sub(frame.children_frees));
        acc.excl_io = acc
            .excl_io
            .wrapping_add(elapsed_io.wrapping_sub(frame.children_io));
        acc.excl_wait = acc
            .excl_wait
            .wrapping_add(elapsed_wait.wrapping_sub(frame.children_wait));
        acc.depth = acc.depth.saturating_sub(1);
        if acc.depth == 0 {
            acc.incl_ns = acc.incl_ns.wrapping_add(t.wrapping_sub(acc.t_outer));
            acc.incl_allocs = acc.incl_allocs.wrapping_add(a.wrapping_sub(acc.a_outer));
            acc.incl_frees = acc.incl_frees.wrapping_add(f.wrapping_sub(acc.f_outer));
            acc.incl_io = acc.incl_io.wrapping_add(io.wrapping_sub(acc.io_outer));
            acc.incl_wait = acc.incl_wait.wrapping_add(w.wrapping_sub(acc.w_outer));
        }
        let parent = self.stack.last().map(|f| f.id);
        if let Some(top) = self.stack.last_mut() {
            top.children_ns = top.children_ns.wrapping_add(elapsed_ns);
            top.children_allocs = top.children_allocs.wrapping_add(elapsed_allocs);
            top.children_frees = top.children_frees.wrapping_add(elapsed_frees);
            top.children_io = top.children_io.wrapping_add(elapsed_io);
            top.children_wait = top.children_wait.wrapping_add(elapsed_wait);
        }
        if let Some(pid) = parent {
            let entry = self.edges.entry((pid, id)).or_insert((0, 0));
            entry.1 = entry.1.wrapping_add(elapsed_ns);
        }
    }

    /// Drops every accumulator so the next dump reports only what follows it.
    /// The live stack is cleared too: a dump happens at a point where nothing
    /// of this slice is still running, and carrying frames across the boundary
    /// would charge the next slice for the previous one's unfinished calls.
    fn reset(&mut self) {
        self.fns.clear();
        self.stack.clear();
        self.edges.clear();
        self.dropped = 0;
        self.dropped_depth = 0;
        self.trace.clear();
        self.trace_dropped = 0;
    }

    /// Renders the report, most inclusive-time first.
    ///
    /// Empty when nothing was recorded. A `--web` prefork server dumps once per
    /// worker at exit, and with the hooks dormant those dumps have no rows — but
    /// a trace context set by the last request they served. Emitting the header
    /// anyway produced one phantom slice per worker, which `--stitch` then counted
    /// as real requests.
    fn render(&self, names: &[String]) -> String {
        let name_of = |id: usize| -> String {
            names.get(id).cloned().unwrap_or_else(|| format!("#{id}"))
        };
        if self.fns.iter().all(|a| a.calls == 0) {
            return String::new();
        }
        let mut out = String::new();
        if self.dropped > 0 {
            out.push_str(&format!(
                "elephc-instr: note: {} calls beyond depth {} were not tracked\n",
                self.dropped, MAX_STACK
            ));
        }
        if PARTIAL.load(Ordering::Relaxed) {
            // Emitted before the rows so it cannot be missed by a reader who
            // stops at the first interesting line.
            out.push_str(
                "elephc-instr: note: selective instrumentation — self time includes any \
                 uninstrumented callees, so self values do not sum to the root's inclusive\n",
            );
        }
        let mut fns: Vec<(usize, &FnAcc)> = self
            .fns
            .iter()
            .enumerate()
            .filter(|(_, a)| a.calls > 0)
            .collect();
        fns.sort_by(|a, b| b.1.incl_ns.cmp(&a.1.incl_ns).then(a.0.cmp(&b.0)));
        for (id, acc) in fns {
            // Retained = allocated minus freed. Signed: a function that frees
            // more than it allocates (a cleanup) legitimately reports negative.
            let incl_ret = acc.incl_allocs as i64 - acc.incl_frees as i64;
            let excl_ret = acc.excl_allocs as i64 - acc.excl_frees as i64;
            out.push_str(&format!(
                "elephc-instr: {} calls={} incl_ns={} excl_ns={} incl_allocs={} excl_allocs={} incl_io={} excl_io={} incl_ret={} excl_ret={} incl_wait={} excl_wait={}\n",
                name_of(id),
                acc.calls,
                // Ticks became nanoseconds here, once, rather than twice per
                // call in the hot path. Every consumer downstream — the table,
                // the assertions, the graph — reads nanoseconds as it always did.
                ticks_to_ns(acc.incl_ns),
                ticks_to_ns(acc.excl_ns),
                acc.incl_allocs,
                acc.excl_allocs,
                acc.incl_io,
                acc.excl_io,
                incl_ret,
                excl_ret,
                acc.incl_wait,
                acc.excl_wait
            ));
        }
        let mut edges: Vec<(&(u32, u32), &(u64, u64))> = self.edges.iter().collect();
        edges.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));
        for ((caller, callee), (count, ticks)) in edges {
            let ns = ticks_to_ns(*ticks);
            out.push_str(&format!(
                "elephc-instr-edge: {} -> {} count={} ns={}\n",
                name_of(*caller as usize),
                name_of(*callee as usize),
                count,
                ns
            ));
        }
        out
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// The id→name table, set once at init and read only at dump.
static NAMES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Global I/O operation counter (currently DB queries). Bumped by
/// `elephc_instr_io`, called from the runtime through the `_elephc_instr_io_fn`
/// slot; snapshotted per function at enter/exit like the allocation counter.
static IO_OPS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Records one I/O operation (a DB query). Reached from bridge builtins through
/// the runtime `_elephc_instr_io_fn` pointer slot, which is null unless
/// `--instrument` linked and initialized this crate.
#[no_mangle]
pub extern "C" fn elephc_instr_io() {
    IO_OPS.fetch_add(1, Ordering::Relaxed);
}

/// Global nanoseconds spent blocked in I/O (currently inside DB calls). Bumped
/// by `elephc_instr_wait` from the bridge, which times the actual driver call;
/// snapshotted per function at enter/exit like the other counters, so each
/// function's time splits into CPU (compute) and wait.
static WAIT_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Records `ns` nanoseconds spent waiting on I/O. Reached from bridge builtins
/// through the runtime `_elephc_instr_wait_fn` pointer slot, null unless
/// `--instrument` linked and initialized this crate.
#[no_mangle]
pub extern "C" fn elephc_instr_wait(ns: u64) {
    WAIT_NS.fetch_add(ns, Ordering::Relaxed);
}

/// Distinct DB query texts and how many times each was executed, in first-seen
/// order. Populated by `elephc_instr_query`, reached from the PDO bridge through
/// the runtime `_elephc_instr_query_fn` pointer slot. Aggregating by normalized
/// text is what turns an N+1 into a single "run 200×" row.
static QUERIES: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());

/// Collapses a SQL statement to its shape so repeated executions aggregate:
/// single-quoted string literals and standalone numeric literals become `?`,
/// and runs of whitespace collapse to one space. A digit that is part of an
/// identifier (`col2`, `md5`) is left alone. This mirrors what a profiler shows
/// so `INSERT ... VALUES ('user5')` and `('user6')` fold into one row.
fn normalize_query(sql: &str) -> String {
    let b = sql.as_bytes();
    let mut out = String::with_capacity(b.len());
    let mut i = 0;
    let mut prev = 0u8; // last byte copied to `out` (0 = start)
    let mut last_ws = false;
    while i < b.len() {
        let c = b[i];
        if c == b'\'' {
            out.push('?');
            i += 1;
            while i < b.len() {
                if b[i] == b'\'' {
                    if i + 1 < b.len() && b[i + 1] == b'\'' {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            prev = b'?';
            last_ws = false;
        } else if c.is_ascii_digit() && !(prev.is_ascii_alphanumeric() || prev == b'_') {
            out.push('?');
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            prev = b'?';
            last_ws = false;
        } else if c.is_ascii_whitespace() {
            if !last_ws {
                out.push(' ');
                prev = b' ';
                last_ws = true;
            }
            i += 1;
        } else {
            out.push(c as char);
            prev = c;
            last_ws = false;
            i += 1;
        }
    }
    out.trim().to_string()
}

/// Records one DB query execution by its SQL text. `ptr`/`len` are the UTF-8
/// bytes of the statement; copied immediately, so the caller's buffer need not
/// outlive the call. Reached from the PDO bridge through the runtime
/// `_elephc_instr_query_fn` pointer slot (null unless `--instrument` linked).
#[no_mangle]
pub extern "C" fn elephc_instr_query(ptr: *const u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let key = normalize_query(&String::from_utf8_lossy(bytes));
    if key.is_empty() {
        return;
    }
    let mut q = QUERIES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(row) = q.iter_mut().find(|(s, _)| *s == key) {
        row.1 += 1;
    } else {
        q.push((key, 1));
    }
}

/// Renders the recorded queries as `elephc-instr-query: <count> <text>` lines,
/// hottest first. The text is single-line (normalization collapsed newlines),
/// so the monitor parses count then the remainder of the line.
fn render_queries() -> String {
    let q = QUERIES.lock().unwrap_or_else(|e| e.into_inner());
    let mut rows: Vec<&(String, u64)> = q.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    let mut out = String::new();
    for (text, count) in rows {
        out.push_str(&format!("elephc-instr-query: {count} {text}\n"));
    }
    out
}

/// W3C Trace Context for the slice being profiled — the distributed-profiling
/// identity. Deliberately the *standard* `traceparent` shape rather than a
/// bespoke header, so an elephc profile joins whatever trace its caller already
/// belongs to (OpenTelemetry, Jaeger, Datadog, another elephc service) instead
/// of forming an island only elephc tooling can read.
///
/// `(trace_id, span_id, parent_span_id, start_unix_micros)`, ids lowercase hex,
/// parent empty at a root.
///
/// The timestamp is what turns the correlated view into a real waterfall: with
/// only durations, spans can be compared but not *placed*, so a reader cannot
/// tell a sequential chain from concurrent fan-out. It is wall clock
/// (`CLOCK_REALTIME`) on purpose — a monotonic clock is meaningless across
/// hosts. That inherits the usual caveat of every distributed tracer: two
/// services' clocks can disagree, so a hop may appear to start slightly before
/// its parent.
static TRACE_CTX: Mutex<Option<(String, String, String, u64, String)>> = Mutex::new(None);

/// Percent-encodes a value for the `key=value` trace line.
///
/// The route is built from an untrusted HTTP path, and the line's separators are
/// space (between fields) and newline (between records). Left raw, a request to
/// `/x start=0 route=whatever` would forge fields in the profile a reader trusts.
/// Encoding rather than replacing keeps it lossless, so `monitor` can show the
/// real path back.
fn encode_field(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        // Anything outside printable ASCII is escaped, and `%` and `=` with it.
        //
        // The old rule kept every byte above 0x7F and pushed it with
        // `byte as char`, which is not a pass-through: `char` widens the byte to
        // that Unicode scalar, and pushing it re-encodes it as UTF-8. A request
        // for `/café` (C3 A9) reached the profile as `cafÃ©` (C3 83 C2 A9), so
        // the operator was shown a route their server never received — silently,
        // for every non-ASCII path.
        //
        // Escaping them instead is both lossless (the reader percent-decodes back
        // to the original bytes) and safer: the field becomes pure ASCII, so no
        // Unicode separator — U+00A0, U+2028 — can survive into a line-oriented
        // format and split a record that a reader splits on whitespace. `=` goes
        // the same way, since the trace line spells its fields `name=value`.
        if byte.is_ascii_graphic() && byte != b'%' && byte != b'=' {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Microseconds since the Unix epoch, for placing a slice on a shared axis.
fn unix_micros() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `ts` is a valid, fully owned `timespec` for the duration of the call.
    unsafe {
        libc::clock_gettime(libc::CLOCK_REALTIME, &mut ts);
    }
    ts.tv_sec as u64 * 1_000_000 + ts.tv_nsec as u64 / 1_000
}

/// Random lowercase hex, `bytes` bytes wide, from the OS entropy pool. Falls
/// back to a clock-derived value if `/dev/urandom` cannot be read, because a
/// slightly weaker id is far better than no correlation at all.
fn random_hex(bytes: usize) -> String {
    use std::io::Read;
    let mut buf = vec![0u8; bytes];
    let ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .is_ok();
    if !ok {
        let mut seed = now_ns().wrapping_mul(0x9E3779B97F4A7C15);
        for slot in buf.iter_mut() {
            seed ^= seed >> 33;
            seed = seed.wrapping_mul(0xFF51AFD7ED558CCD);
            *slot = (seed >> 24) as u8;
        }
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// Parses a W3C `traceparent` value, returning `(trace_id, parent_span_id)`.
///
/// Shape: `<2 hex version>-<32 hex trace-id>-<16 hex parent-id>-<2 hex flags>`.
/// The all-zero trace and span ids are invalid per the spec and are rejected,
/// as is anything malformed — a bad header must start a fresh trace rather than
/// silently poison one, and must never let a caller inject arbitrary text into
/// our output.
fn parse_traceparent(value: &str) -> Option<(String, String)> {
    let value = value.trim();
    let mut parts = value.split('-');
    let version = parts.next()?;
    let trace_id = parts.next()?;
    let parent_id = parts.next()?;
    let _flags = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let hex = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit());
    if !hex(version, 2) || !hex(trace_id, 32) || !hex(parent_id, 16) {
        return None;
    }
    if version == "ff" || trace_id.bytes().all(|b| b == b'0') || parent_id.bytes().all(|b| b == b'0')
    {
        return None;
    }
    Some((trace_id.to_ascii_lowercase(), parent_id.to_ascii_lowercase()))
}

/// Opens a profiling slice's trace context from an inbound `traceparent`.
///
/// `ptr`/`len` are the header value as received (empty or null when absent).
/// A valid header continues that trace as a child span; anything else starts a
/// new trace. Either way a fresh span id is minted for THIS slice, and the
/// resulting `traceparent` is published in the environment as
/// `ELEPHC_TRACEPARENT` so outgoing calls can propagate it with no new builtin
/// and no change to the request-building assembly:
///
/// ```php
/// $ctx = stream_context_create(['http' => [
///     'header' => "traceparent: " . getenv('ELEPHC_TRACEPARENT') . "\r\n",
/// ]]);
/// ```
///
/// Reached from the web bridge through the `_elephc_instr_trace_fn` slot, so a
/// non-instrument binary carries no trace machinery at all.
#[no_mangle]
pub extern "C" fn elephc_instr_trace_begin(
    ptr: *const u8,
    len: usize,
    route_ptr: *const u8,
    route_len: usize,
) {
    let inbound = if ptr.is_null() || len == 0 {
        None
    } else {
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        std::str::from_utf8(bytes).ok().and_then(parse_traceparent)
    };
    let (trace_id, parent) = match inbound {
        Some((trace_id, parent)) => (trace_id, parent),
        None => (random_hex(16), String::new()),
    };
    let span_id = random_hex(8);
    std::env::set_var(
        "ELEPHC_TRACEPARENT",
        format!("00-{trace_id}-{span_id}-01"),
    );
    // The route the request was routed to, so an exact capture can be broken down
    // per endpoint the way a sampled one already is (the probe stamps it onto every
    // sample). Absent outside `--web`, where there is no route.
    let route = if route_ptr.is_null() || route_len == 0 {
        String::new()
    } else {
        let bytes = unsafe { std::slice::from_raw_parts(route_ptr, route_len) };
        String::from_utf8_lossy(bytes).into_owned()
    };
    if let Ok(mut guard) = TRACE_CTX.lock() {
        *guard = Some((trace_id, span_id, parent, unix_micros(), route));
    }
}

/// Renders the slice's trace identity, or nothing when no context was opened
/// (a plain one-shot run that never went through the web bridge).
fn render_trace() -> String {
    let Ok(guard) = TRACE_CTX.lock() else {
        return String::new();
    };
    let Some((trace_id, span_id, parent, start_us, route)) = guard.as_ref() else {
        return String::new();
    };
    let parent = if parent.is_empty() { "-" } else { parent.as_str() };
    let route = if route.is_empty() {
        "-".to_string()
    } else {
        encode_field(route)
    };
    format!(
        "elephc-instr-trace: trace={trace_id} span={span_id} parent={parent} \
         start={start_us} route={route}\n"
    )
}

/// Whether the hooks do anything.
///
/// A binary can ship WITH hooks and still cost almost nothing, because the hook
/// checks this before doing any work — the model Blackfire uses: instrumentation
/// present everywhere, activated per request, so only the request being profiled
/// pays.
///
/// Default **off**: a `--with-monitoring` binary is *capable* of profiling, not
/// busy doing it. Running one normally must behave — and cost — like a normal
/// binary; `monitor` turns the hooks on when it wants them, and a `--web` request
/// carrying the profile header turns them on for itself.
static ENABLED: AtomicBool = AtomicBool::new(false);

/// Brackets one request's profile: `begin != 0` starts recording, `0` ends it.
///
/// This is what makes a production binary able to answer the same question a dev
/// build answers. Hooks ship in the binary and stay dormant; a request that asks
/// to be profiled turns them on for its own duration and dumps its own slice.
/// Every other request pays only the dormant cost.
///
/// Ending dumps and clears, so consecutive profiled requests do not accumulate
/// into each other — the bug that made `--web` report running totals.
#[no_mangle]
pub extern "C" fn elephc_instr_request(begin: u32) {
    if begin != 0 {
        STATE.with(|s| s.borrow_mut().reset());
        switch_on();
    } else {
        ENABLED.store(false, Ordering::Relaxed);
        elephc_instr_dump();
    }
}

/// Reads the initial hook state from the environment, at program start.
///
/// `ELEPHC_MONITOR=1` asks a `--with-monitoring` binary to profile this whole
/// run — which is what `monitor` sets when it spawns one. Without it the hooks
/// stay dormant, so the decision moved from compile time to run time and one
/// build serves both "profile this" and "just run".
#[no_mangle]
pub extern "C" fn elephc_instr_boot() {
    // Set by the probe's init, which runs first and owns the one check: the
    // control channel's marker can only be read once, so asking twice would give
    // the second reader nothing.
    let asked = unsafe { std::ptr::addr_of!(elephc_monitor_active).read() };
    if asked != 0 {
        switch_on();
    }
}

extern "C" {
    /// Runtime `.comm` word: nonzero once this process has been asked to profile.
    static elephc_monitor_active: u64;
}

/// Turns the hooks on, and fixes the reference the timings are measured against.
///
/// One function rather than a store at each call site: there are three ways to
/// switch profiling on — the exported `enable`, a per-request `begin`, and the
/// boot-time check — and only one of them remembered to establish the tick
/// epoch, which left the other two (the paths every real run takes) converting
/// counter ticks against a reference of zero.
fn switch_on() {
    start_tick_epoch();
    ENABLED.store(true, Ordering::Relaxed);
}

/// Turns the hooks on for this thread's subsequent calls.
#[no_mangle]
pub extern "C" fn elephc_instr_enable() {
    switch_on();
}

/// Turns the hooks off. Calls still execute their prologue and epilogue, but the
/// hook returns before reading a clock or touching the shadow stack.
#[no_mangle]
pub extern "C" fn elephc_instr_disable() {
    ENABLED.store(false, Ordering::Relaxed);
}

/// Set when only a subset of the program's functions carry hooks.
///
/// It changes what "self" MEANS: an uninstrumented callee's time is spent inside
/// its instrumented caller's frame, so it lands in that caller's self rather than
/// in a child. Self values therefore stop partitioning the root's inclusive —
/// the property the full mode's numbers rest on. Reporting the same table
/// without saying so would quietly redefine every column.
static PARTIAL: AtomicBool = AtomicBool::new(false);

/// Marks this binary as selectively instrumented. Emitted by the compiler at
/// init when `--instrument=<names>` chose a subset.
#[no_mangle]
pub extern "C" fn elephc_instr_partial() {
    PARTIAL.store(true, Ordering::Relaxed);
}

/// Timeline tracing (Chrome Trace / Perfetto), enabled by `ELEPHC_INSTR_TRACE`.
static TRACE_ON: AtomicBool = AtomicBool::new(false);
/// Max per-call spans recorded (bounded so a hot program's trace stays sane).
static TRACE_CAP: AtomicUsize = AtomicUsize::new(500_000);
/// Output path for the Chrome trace, set from `ELEPHC_INSTR_TRACE`.
static TRACE_PATH: Mutex<Option<String>> = Mutex::new(None);

/// JSON-escapes a string for the Chrome trace event names.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Writes the recorded spans as Chrome Trace Format (open in Perfetto or
/// `chrome://tracing`). Complete ('X') events nest by ts/dur, so the whole call
/// tree renders as nested slices. Timestamps are microseconds from the first
/// span. Best-effort: a write failure is reported to stderr, not fatal.
fn write_chrome_trace(path: &str, spans: &[(u32, u64, u64)], names: &[String], dropped: u64) {
    use std::io::Write;
    let base = spans.iter().map(|s| s.1).min().unwrap_or(0);
    let name_of = |id: usize| -> String {
        names.get(id).cloned().unwrap_or_else(|| format!("#{id}"))
    };
    let mut out = String::from("{\"traceEvents\":[");
    for (i, (id, enter, exit)) in spans.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let ts_us = ticks_to_ns(enter.wrapping_sub(base)) as f64 / 1000.0;
        let dur_us = ticks_to_ns(exit.wrapping_sub(*enter)) as f64 / 1000.0;
        out.push_str(&format!(
            "{{\"name\":\"{}\",\"cat\":\"php\",\"ph\":\"X\",\"pid\":1,\"tid\":1,\"ts\":{ts_us},\"dur\":{dur_us}}}",
            json_escape(&name_of(*id as usize))
        ));
    }
    out.push_str("],\"displayTimeUnit\":\"ms\"}");
    match std::fs::File::create(path) {
        Ok(mut file) => {
            let _ = file.write_all(out.as_bytes());
            let note = if dropped > 0 {
                format!(" ({dropped} calls dropped past the trace cap)")
            } else {
                String::new()
            };
            let _ = writeln!(
                std::io::stderr(),
                "elephc-instr: wrote {} spans to {path}{note}",
                spans.len()
            );
        }
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "elephc-instr: cannot write trace {path}: {e}");
        }
    }
}

/// Monotonic nanoseconds — the instrumentation clock. Not async-signal-safe, but
/// enter/exit run in ordinary compiled code, never a signal handler.
fn now_ns() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: ts is a valid, owned timespec.
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
    }
    (ts.tv_sec as u64)
        .wrapping_mul(1_000_000_000)
        .wrapping_add(ts.tv_nsec as u64)
}

/// Reads the CPU's monotonic counter, in its own ticks.
///
/// The hooks used `clock_gettime(CLOCK_MONOTONIC)`, which on this machine costs
/// 23 ns a read and resolves to **one microsecond** — 23 ns spent to learn
/// something coarser than the functions being measured. The counter register
/// behind it costs 0.33 ns and ticks every 41 ns. Cheaper *and* twenty-four
/// times finer, which is not a trade at all.
///
/// Ticks, not nanoseconds: the conversion is a multiply and a divide, and doing
/// it twice per call to store a number nobody reads until the dump is the same
/// mistake in a smaller form. `ticks_to_ns` converts once, at render.
#[inline(always)]
fn now_ticks() -> u64 {
    #[cfg(target_arch = "aarch64")]
    {
        let value: u64;
        // SAFETY: `cntvct_el0` is readable from EL0 on every ARMv8 profile, and
        // the read has no memory effects.
        unsafe {
            std::arch::asm!("mrs {}, cntvct_el0", out(reg) value, options(nomem, nostack));
        }
        value
    }
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: `rdtsc` is unprivileged and has no memory effects.
        unsafe { core::arch::x86_64::_rdtsc() }
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        now_ns()
    }
}

/// Ticks per second, or 0 while unknown.
static TICK_HZ: AtomicU64 = AtomicU64::new(0);
/// The clock and counter, read together when profiling was switched on.
static EPOCH_NS: AtomicU64 = AtomicU64::new(0);
static EPOCH_TICKS: AtomicU64 = AtomicU64::new(0);

/// Establishes the tick rate, or the reference point from which it is derived.
///
/// AArch64 publishes the counter's frequency in a register, so there is nothing
/// to measure. x86_64 does not: the TSC rate is whatever the machine's is, so
/// the rate comes from the run itself — a clock reading and a counter reading
/// taken together here, and again at the dump. Measuring over the whole run
/// beats any calibration loop at startup, and costs nothing while running.
fn start_tick_epoch() {
    #[cfg(target_arch = "aarch64")]
    {
        let hz: u64;
        // SAFETY: `cntfrq_el0` is readable from EL0 and has no memory effects.
        unsafe {
            std::arch::asm!("mrs {}, cntfrq_el0", out(reg) hz, options(nomem, nostack));
        }
        if hz > 0 {
            TICK_HZ.store(hz, Ordering::Relaxed);
        }
    }
    EPOCH_NS.store(now_ns(), Ordering::Relaxed);
    EPOCH_TICKS.store(now_ticks(), Ordering::Relaxed);
}

/// Converts a span of ticks to nanoseconds.
///
/// On a platform whose counter rate is not published, the rate is derived from
/// the run: how many ticks elapsed against how many nanoseconds, both measured.
/// A run too short to divide safely reports its ticks unconverted rather than
/// inventing a rate — wrong units are visible, a fabricated rate is not.
fn ticks_to_ns(ticks: u64) -> u64 {
    let hz = match TICK_HZ.load(Ordering::Relaxed) {
        0 => {
            let ns = now_ns().wrapping_sub(EPOCH_NS.load(Ordering::Relaxed));
            let elapsed = now_ticks().wrapping_sub(EPOCH_TICKS.load(Ordering::Relaxed));
            if ns < 1_000_000 || elapsed == 0 {
                return ticks;
            }
            let hz = (u128::from(elapsed) * 1_000_000_000u128 / u128::from(ns)) as u64;
            TICK_HZ.store(hz, Ordering::Relaxed);
            hz
        }
        hz => hz,
    };
    if hz == 0 {
        return ticks;
    }
    (u128::from(ticks) * 1_000_000_000u128 / u128::from(hz)) as u64
}

/// A hasher for keys that are function ids.
///
/// The default hasher is SipHash, chosen to make hash-flooding impractical when
/// keys come from outside. These keys do not: they are dense small integers the
/// compiler assigned, two per edge, and nothing at run time can influence them.
/// What that safety cost is measurable — 33 ns per lookup, twice per call, about
/// 30% of the whole instrumentation overhead, spent defending against an attack
/// this map cannot have.
///
/// The finaliser is murmur3's: two shifts and a multiply, enough mixing that
/// adjacent ids do not collide in a bucket.
#[derive(Default, Clone, Copy)]
struct IdHasher(u64);

impl std::hash::Hasher for IdHasher {
    fn write(&mut self, bytes: &[u8]) {
        // Not expected — the derived Hash for (u32, u32) calls write_u32 — but a
        // Hasher must accept bytes, and silently hashing nothing would collapse
        // every key into one bucket.
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn write_u32(&mut self, value: u32) {
        self.0 = (self.0 << 32) | u64::from(value);
    }

    fn finish(&self) -> u64 {
        let mut x = self.0;
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
        x ^= x >> 33;
        x
    }
}

#[derive(Default, Clone, Copy)]
struct BuildIdHasher;

impl std::hash::BuildHasher for BuildIdHasher {
    type Hasher = IdHasher;
    fn build_hasher(&self) -> IdHasher {
        IdHasher(0)
    }
}

/// Registers the id→name table: `count` entries of `(u64 name_ptr, u64 name_len)`
/// — the compiler emits these as `.quad name, len` pairs. Call once before any
/// enter/exit.
///
/// # Safety
/// `table` must point to `count` valid `(ptr,len)` pairs, each `ptr`/`len`
/// describing a live UTF-8(-ish) byte range for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_instr_init(table: *const u8, count: usize) {
    if table.is_null() {
        return;
    }
    let base = table as *const u64;
    let mut names = Vec::with_capacity(count);
    for i in 0..count {
        let ptr = *base.add(i * 2) as *const u8;
        let len = *base.add(i * 2 + 1) as usize;
        if ptr.is_null() {
            names.push(String::new());
            continue;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        names.push(String::from_utf8_lossy(slice).into_owned());
    }
    if let Ok(mut guard) = NAMES.lock() {
        *guard = names;
    }
    // Opt-in timeline tracing: ELEPHC_INSTR_TRACE=<path> records per-call spans
    // and writes a Chrome trace at exit; ELEPHC_INSTR_TRACE_MAX caps the count.
    if let Ok(path) = std::env::var("ELEPHC_INSTR_TRACE") {
        if !path.is_empty() {
            if let Ok(mut guard) = TRACE_PATH.lock() {
                *guard = Some(path);
            }
            if let Ok(max) = std::env::var("ELEPHC_INSTR_TRACE_MAX") {
                if let Ok(n) = max.parse::<usize>() {
                    if n > 0 {
                        TRACE_CAP.store(n, Ordering::Relaxed);
                    }
                }
            }
            TRACE_ON.store(true, Ordering::Relaxed);
        }
    }
}

/// Records entry to the function `id`; `allocs` / `frees` are the program's
/// live heap counters (`_gc_allocs` / `_gc_frees`) at the call site.
#[no_mangle]
pub extern "C" fn elephc_instr_enter(id: u32, allocs: u64, frees: u64) {
    // Checked before the clock reads: those are the expensive part, and a
    // dormant binary must not pay them.
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let t = now_ticks();
    let io = IO_OPS.load(Ordering::Relaxed);
    let w = WAIT_NS.load(Ordering::Relaxed);
    STATE.with(|s| s.borrow_mut().enter_at(id, t, allocs, frees, io, w));
}

/// Records exit from the function `id`; `allocs` / `frees` are the program's
/// live heap counters (`_gc_allocs` / `_gc_frees`) at the call site.
#[no_mangle]
pub extern "C" fn elephc_instr_exit(id: u32, allocs: u64, frees: u64) {
    // Symmetrical with enter: a frame that was never pushed must not be popped,
    // or a disable mid-call would unwind accounting that was never recorded.
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let t = now_ticks();
    let io = IO_OPS.load(Ordering::Relaxed);
    let w = WAIT_NS.load(Ordering::Relaxed);
    STATE.with(|s| s.borrow_mut().exit_at(id, t, allocs, frees, io, w));
}

/// Writes the exact per-function table and edge list to stderr, one line each,
/// then **clears the accumulators** so the next dump reports a fresh slice.
///
/// The reset is what makes the dump a *per-request* profile under `--web`,
/// where the compiler calls this at the end of every request on a worker that
/// serves many: without it, request N reported requests 1..N summed. At process
/// exit (the one-shot case) the reset is simply the last thing that happens.
///
/// The monotonic counters (`IO_OPS`, `WAIT_NS`) are deliberately NOT reset:
/// they are only ever read as deltas against a per-frame snapshot, so letting
/// them run keeps every attribution correct across slices.
#[no_mangle]
pub extern "C" fn elephc_instr_dump() {
    use std::io::Write;
    let names = NAMES.lock().map(|g| g.clone()).unwrap_or_default();
    let text = STATE.with(|s| s.borrow().render(&names));
    // Nothing recorded, nothing reported — not even the trace header. A `--web`
    // prefork server dumps once per worker at exit, and with dormant hooks those
    // dumps have no rows but do have the trace context of the last request that
    // worker served. Writing the header regardless produced one phantom slice per
    // worker, which `--stitch` then counted as a real request: ten idle workers
    // turned one profiled request into eleven.
    if text.is_empty() {
        STATE.with(|s| s.borrow_mut().reset());
        return;
    }
    let _ = std::io::stderr().write_all(render_trace().as_bytes());
    let _ = std::io::stderr().write_all(text.as_bytes());
    let _ = std::io::stderr().write_all(render_queries().as_bytes());
    if TRACE_ON.load(Ordering::Relaxed) {
        if let Some(path) = TRACE_PATH.lock().ok().and_then(|g| g.clone()) {
            STATE.with(|s| {
                let s = s.borrow();
                write_chrome_trace(path.as_str(), &s.trace, &names, s.trace_dropped);
            });
        }
    }
    STATE.with(|s| s.borrow_mut().reset());
    if let Ok(mut q) = QUERIES.lock() {
        q.clear();
    }
}

#[cfg(test)]
mod tests {
    /// Makes one tick worth one nanosecond, so a test that feeds synthetic
    /// timestamps reads them back unchanged.
    ///
    /// The hot path stores raw counter ticks and the renderer converts once. A
    /// test asserting rendered nanoseconds against hand-written timestamps is
    /// therefore asserting something about the host's counter unless it says
    /// which rate it means — on this machine, 24 MHz, `30` renders as `1250`.
    fn ticks_are_nanoseconds() {
        super::TICK_HZ.store(1_000_000_000, super::Ordering::Relaxed);
    }

    /// Switching the hooks on must always fix the time reference too.
    ///
    /// There are three ways in — the exported `enable`, the per-request `begin`,
    /// and the boot-time check — and when only one of them established the tick
    /// epoch, the other two (which is to say every real run) converted counter
    /// ticks against a reference of zero. Nothing failed loudly: the profile
    /// still printed, with times derived from a rate nobody had measured.
    ///
    /// Read from the source because that is where the invariant lives: the two
    /// broken paths cannot be reached from a unit test — one reads a linker
    /// symbol, the other needs a web request.
    #[test]
    fn every_path_that_enables_also_starts_the_clock() {
        let source = include_str!("lib.rs");
        let body = source
            .split_once("mod tests {")
            .map(|(before, _)| before)
            .unwrap_or(source);
        let stores = body.matches("ENABLED.store(true").count();
        assert_eq!(
            stores, 1,
            "ENABLED is switched on in {stores} places; it must go through the one \
             function that also starts the tick epoch, or a path will convert \
             ticks against a reference nobody set"
        );
        assert!(
            body.contains("fn switch_on() {\n    start_tick_epoch();"),
            "the single enable path must start the tick epoch first"
        );
    }

    /// A counter tick is not a nanosecond, and the renderer must know it.
    #[test]
    fn ticks_convert_to_nanoseconds_at_the_rate_measured() {
        // A 24 MHz counter — what this class of machine actually reports.
        super::TICK_HZ.store(24_000_000, super::Ordering::Relaxed);
        // One second of ticks must read as one second.
        assert_eq!(super::ticks_to_ns(24_000_000), 1_000_000_000);
        // And a single tick as its period, not as a nanosecond.
        assert_eq!(super::ticks_to_ns(1), 41);
        // A rate of one tick per nanosecond is the identity.
        super::TICK_HZ.store(1_000_000_000, super::Ordering::Relaxed);
        assert_eq!(super::ticks_to_ns(1234), 1234);
        // Large spans must not overflow on the way through.
        assert_eq!(super::ticks_to_ns(u64::MAX / 2), u64::MAX / 2);
    }

    /// A route must come back out of the trace line exactly as it went in.
    ///
    /// The encoder used to push non-ASCII bytes with `byte as char`, which is a
    /// widening, not a pass-through: `/café` (C3 A9) was written `cafÃ©`
    /// (C3 83 C2 A9), so the profile showed a route the server never received.
    /// Silent, and on every non-ASCII path.
    ///
    /// Round-trip rather than golden output: what matters is that a reader gets
    /// the original bytes back, not which spelling the encoder chose.
    #[test]
    fn a_route_round_trips_through_the_trace_line() {
        // The decoder that ships in `monitor`, restated here so this crate can
        // test the pair without depending on the compiler crate.
        fn decode(value: &str) -> Vec<u8> {
            let bytes = value.as_bytes();
            let mut out = Vec::with_capacity(bytes.len());
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'%' && i + 2 < bytes.len() {
                    if let Some(byte) = std::str::from_utf8(&bytes[i + 1..i + 3])
                        .ok()
                        .and_then(|h| u8::from_str_radix(h, 16).ok())
                    {
                        out.push(byte);
                        i += 3;
                        continue;
                    }
                }
                out.push(bytes[i]);
                i += 1;
            }
            out
        }

        for route in [
            "GET /users/{id}",
            "GET /caf\u{e9}",              // non-ASCII, the corrupted case
            "GET /\u{1f600}/emoji",         // 4-byte scalar
            "GET /a b\tc",                 // ASCII whitespace
            "GET /x?q=1&r=2",              // the `=` the trace line uses
            "GET /100%25",                 // a literal percent
            "GET /\u{a0}nbsp",             // U+00A0: whitespace to a Unicode reader
            "GET /line\nbreak",            // must never reach the line raw
        ] {
            let encoded = super::encode_field(route);
            assert_eq!(
                decode(&encoded),
                route.as_bytes(),
                "route {route:?} did not survive encoding (got {encoded:?})"
            );
            // Pure ASCII graphics: nothing left that a line- or field-splitting
            // reader could mistake for a separator.
            assert!(
                encoded.bytes().all(|b| b.is_ascii_graphic() && b != b'='),
                "encoded field must carry no separator or non-ASCII byte: {encoded:?}"
            );
        }
    }

    use super::*;

    #[test]
    fn simple_parent_child_accounting() {
        let mut s = State::default();
        // Timestamps then allocation counters. a=main, b=child.
        // main enters @t0/alloc0, a enters, b enters @t10/alloc3, unwinds.
        // Args: (id, ns, allocs, frees, io). Only b does io (2 queries).
        s.enter_at(0, 0, 0, 0, 0, 0); // main
        s.enter_at(1, 0, 0, 0, 0, 0); // a
        s.enter_at(2, 10, 3, 0, 0, 0); // b
        s.exit_at(2, 40, 8, 0, 2, 0); // b: 30ns, 5 allocs, 2 io
        s.exit_at(1, 50, 9, 0, 2, 0); // a: children 30/5/2 -> excl 20ns/4allocs/0io
        s.exit_at(0, 60, 12, 0, 2, 0); // main: excl 10ns/3allocs/0io
        assert_eq!(s.fns[2].incl_ns, 30);
        assert_eq!(s.fns[2].excl_ns, 30);
        assert_eq!(s.fns[2].incl_allocs, 5);
        assert_eq!(s.fns[2].excl_allocs, 5);
        assert_eq!(s.fns[2].incl_io, 2);
        assert_eq!(s.fns[2].excl_io, 2);
        assert_eq!(s.fns[1].excl_ns, 20);
        assert_eq!(s.fns[1].excl_allocs, 4);
        assert_eq!(s.fns[1].incl_io, 2); // a's subtree did 2 io
        assert_eq!(s.fns[1].excl_io, 0); // a itself did none
        assert_eq!(s.fns[0].incl_ns, 60);
        assert_eq!(s.fns[0].incl_allocs, 12);
        assert_eq!(s.fns[0].excl_allocs, 3);
        assert_eq!(s.fns[0].incl_io, 2);
        // Exclusives partition the root's inclusive for every dimension.
        let sum_ns = s.fns.iter().map(|a| a.excl_ns).sum::<u64>();
        let sum_allocs = s.fns.iter().map(|a| a.excl_allocs).sum::<u64>();
        let sum_io = s.fns.iter().map(|a| a.excl_io).sum::<u64>();
        assert_eq!(sum_ns, s.fns[0].incl_ns);
        assert_eq!(sum_allocs, s.fns[0].incl_allocs);
        assert_eq!(sum_io, s.fns[0].incl_io);
    }

    #[test]
    fn recursion_does_not_double_count() {
        let mut s = State::default();
        s.enter_at(0, 0, 0, 0, 0, 0);
        s.enter_at(0, 0, 1, 0, 0, 0);
        s.enter_at(0, 0, 2, 0, 0, 0);
        s.exit_at(0, 30, 5, 0, 0, 0);
        s.exit_at(0, 60, 7, 0, 0, 0);
        s.exit_at(0, 90, 10, 0, 0, 0); // outermost span 0..90 ns, 0..10 allocs
        assert_eq!(s.fns[0].calls, 3);
        assert_eq!(s.fns[0].incl_ns, 90);
        assert_eq!(s.fns[0].incl_allocs, 10);
        // Exclusive equals inclusive (single function, all self).
        assert_eq!(s.fns[0].excl_ns, 90);
        assert_eq!(s.fns[0].excl_allocs, 10);
    }

    #[test]
    fn exit_resyncs_past_unwound_frames() {
        let mut s = State::default();
        s.enter_at(0, 0, 0, 0, 0, 0); // a
        s.enter_at(1, 5, 1, 0, 0, 0); // b
        s.enter_at(2, 10, 2, 0, 0, 0); // c — unwound, no exits for c or b
        s.exit_at(0, 100, 9, 0, 0, 0);
        assert_eq!(s.stack.len(), 0, "stack fully unwound");
        assert_eq!(s.fns[0].incl_ns, 100);
        assert_eq!(s.fns[0].incl_allocs, 9);
        assert_eq!(s.fns[1].depth, 0);
        assert_eq!(s.fns[2].depth, 0);
    }

    #[test]
    fn render_lists_metrics_and_edges() {
        ticks_are_nanoseconds();
        let mut s = State::default();
        s.enter_at(0, 0, 0, 0, 0, 0);
        s.enter_at(1, 0, 0, 0, 0, 0);
        s.exit_at(1, 40, 7, 5, 3, 0); // hot: 7 allocs, 5 frees, 3 io ops
        s.exit_at(0, 50, 8, 5, 3, 0);
        let names = vec!["{main}".to_string(), "hot".to_string()];
        let out = s.render(&names);
        // Retained = allocs - frees: hot keeps 2 of its 7; main's own 1 alloc is
        // never freed, so the run retains 3 in total.
        assert!(out.contains("elephc-instr: {main} calls=1 incl_ns=50 excl_ns=10 incl_allocs=8 excl_allocs=1 incl_io=3 excl_io=0 incl_ret=3 excl_ret=1"), "{out}");
        assert!(out.contains("elephc-instr: hot calls=1 incl_ns=40 excl_ns=40 incl_allocs=7 excl_allocs=7 incl_io=3 excl_io=3 incl_ret=2 excl_ret=2"), "{out}");
        assert!(out.contains("elephc-instr-edge: {main} -> hot count=1 ns=40"), "{out}");
    }

    #[test]
    fn retained_is_signed_and_partitions_like_the_other_dimensions() {
        ticks_are_nanoseconds();
        // `cleanup` frees more than it allocates (it releases what main built),
        // so its retained is negative — the dimension must not clamp at zero.
        let mut s = State::default();
        s.enter_at(0, 0, 0, 0, 0, 0); // main
        s.enter_at(1, 10, 10, 0, 0, 0); // cleanup, entered after main made 10 objects
        s.exit_at(1, 20, 10, 8, 0, 0); // cleanup: 0 allocs, 8 frees -> retained -8
        s.exit_at(0, 30, 10, 8, 0, 0); // main: 10 allocs, 8 frees -> retained +2
        let names = vec!["{main}".to_string(), "cleanup".to_string()];
        let out = s.render(&names);
        assert!(out.contains("cleanup calls=1 incl_ns=10 excl_ns=10 incl_allocs=0 excl_allocs=0 incl_io=0 excl_io=0 incl_ret=-8 excl_ret=-8"), "{out}");
        assert!(out.contains("{main} calls=1 incl_ns=30 excl_ns=20 incl_allocs=10 excl_allocs=10 incl_io=0 excl_io=0 incl_ret=2 excl_ret=10"), "{out}");
        // Self retained across the program sums to the root's inclusive retained.
        let sum: i64 = s
            .fns
            .iter()
            .map(|a| a.excl_allocs as i64 - a.excl_frees as i64)
            .sum();
        assert_eq!(sum, s.fns[0].incl_allocs as i64 - s.fns[0].incl_frees as i64);
    }

    #[test]
    fn traceparent_accepts_valid_headers_and_rejects_everything_else() {
        let good = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        assert_eq!(
            parse_traceparent(good),
            Some((
                "4bf92f3577b34da6a3ce929d0e0e4736".to_string(),
                "00f067aa0ba902b7".to_string()
            ))
        );
        // Surrounding whitespace is normal in a header value; case is normalized.
        assert_eq!(
            parse_traceparent("  00-4BF92F3577B34DA6A3CE929D0E0E4736-00F067AA0BA902B7-01 ")
                .map(|(t, _)| t),
            Some("4bf92f3577b34da6a3ce929d0e0e4736".to_string())
        );
        // A caller controls this header, so everything malformed must start a
        // FRESH trace rather than propagate junk into the output.
        for bad in [
            "",
            "garbage",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7",  // too few fields
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-extra", // too many
            "00-4bf92f3577b34da6a3ce929d0e0e473-00f067aa0ba902b7-01", // short trace id
            "00-4bf92f3577b34da6a3ce929d0e0e473g-00f067aa0ba902b7-01", // non-hex
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01", // all-zero trace
            "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01", // all-zero span
            "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01", // forbidden version
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01\r\nX-Evil: 1", // injection
        ] {
            assert_eq!(parse_traceparent(bad), None, "must reject {bad:?}");
        }
    }

    /// A crafted request path must not be able to forge fields in the profile.
    ///
    /// The trace line is `key=value` separated by spaces and terminated by a
    /// newline, and the route is built from an untrusted HTTP path — so a request
    /// to `/x start=0 route=other` would otherwise write fields a reader trusts.
    /// Encoding is chosen over replacing so `monitor` can still show the real path.
    #[test]
    fn an_untrusted_route_cannot_forge_trace_line_fields() {
        let forged = encode_field("GET /x start=0 route=evil");
        assert!(!forged.contains(' '), "a space would open a new field: {forged}");
        // `=` is escaped too. This used to assert the opposite — that
        // `start=0` and `route=evil` survived raw — which contradicted the
        // test's own name: the separator a `key=value` line splits on was
        // exactly the character left under an attacker's control.
        assert_eq!(forged, "GET%20/x%20start%3D0%20route%3Devil");
        assert!(!forged.contains('='), "a `=` would forge a field: {forged}");

        // Newlines would open a whole new record, which is worse.
        let multiline = encode_field("GET /a\nelephc-instr-trace: trace=deadbeef");
        assert!(!multiline.contains('\n'), "{multiline}");

        // Control bytes and the escape character itself round-trip unambiguously.
        assert_eq!(encode_field("a\tb"), "a%09b");
        assert_eq!(encode_field("100%"), "100%25");
        // Ordinary paths stay readable, so encoding costs nothing in the common case.
        assert_eq!(encode_field("/orders/42"), "/orders/42");
    }

    #[test]
    fn trace_begin_continues_a_valid_trace_and_starts_one_otherwise() {
        // Inbound header: same trace, our span is a child of the caller's.
        let hdr = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        elephc_instr_trace_begin(hdr.as_ptr(), hdr.len(), std::ptr::null(), 0);
        let line = render_trace();
        assert!(line.contains("trace=4bf92f3577b34da6a3ce929d0e0e4736"), "{line}");
        assert!(line.contains("parent=00f067aa0ba902b7"), "{line}");
        // The published traceparent carries OUR span, for the next hop.
        let published = std::env::var("ELEPHC_TRACEPARENT").unwrap();
        assert!(published.starts_with("00-4bf92f3577b34da6a3ce929d0e0e4736-"), "{published}");
        assert!(!published.contains("00f067aa0ba902b7"), "must mint a new span: {published}");
        // No inbound header: a fresh trace, no parent.
        elephc_instr_trace_begin(std::ptr::null(), 0, std::ptr::null(), 0);
        let root = render_trace();
        assert!(root.contains("parent=-"), "{root}");
        assert!(!root.contains("4bf92f3577b34da6a3ce929d0e0e4736"), "{root}");
    }

    #[test]
    fn reset_makes_each_dump_a_fresh_slice() {
        ticks_are_nanoseconds();
        // Two identical "requests" on one worker. Without the reset the second
        // reports calls=2 and double the time — the --web bug this fixes.
        let mut s = State::default();
        s.enter_at(0, 0, 0, 0, 0, 0);
        s.exit_at(0, 100, 5, 0, 0, 0);
        let names = vec!["work".to_string()];
        let first = s.render(&names);
        assert!(first.contains("work calls=1 incl_ns=100"), "{first}");
        s.reset();
        s.enter_at(0, 1_000, 90, 0, 0, 0);
        s.exit_at(0, 1_100, 95, 0, 0, 0);
        let second = s.render(&names);
        assert!(
            second.contains("work calls=1 incl_ns=100 excl_ns=100 incl_allocs=5"),
            "second slice must not carry the first: {second}"
        );
        // Edges and the live stack are cleared with the rest.
        assert!(s.stack.is_empty());
        assert_eq!(s.edges.len(), 0);
    }

    /// Recursion deeper than the shadow stack must cost only the frames it
    /// could not hold, never the ones it did.
    ///
    /// The dropped activation used to raise the function's depth without
    /// pushing a frame, so its exit hunted the stack for something that was
    /// never there and popped everything on the way: every enclosing frame lost
    /// its accounting, the recursive function's inclusive time was never
    /// credited (depth could not return to zero) and its exclusive time ran
    /// past 100% of the run. Measured at exactly MAX_STACK on a real program.
    /// A frame unwound by an exception keeps its own cost instead of donating
    /// it to whoever caught the throw.
    ///
    /// The unwind path used to discard stale frames, so their elapsed time
    /// stayed inside the catcher's exclusive total: on a real program the
    /// catching function reported 99.7% self time for work it never did, the
    /// unwound frames reported zero, and the exclusives summed past 100%.
    #[test]
    fn an_unwound_frame_keeps_its_cost_instead_of_the_catcher() {
        let mut s = State::default();
        // catcher(0) -> middle(1) -> inner(2); inner and middle are unwound by
        // a throw, so only catcher's exit hook ever runs.
        s.enter_at(0, 0, 0, 0, 0, 0);
        s.enter_at(1, 10, 0, 0, 0, 0);
        s.enter_at(2, 30, 0, 0, 0, 0);
        s.exit_at(0, 100, 0, 0, 0, 0);

        assert!(s.stack.is_empty(), "the throw unwound everything");
        // Each unwound frame is closed at the instant the throw was observed.
        assert_eq!(s.fns[2].incl_ns, 70, "inner ran 30..100");
        assert_eq!(s.fns[2].excl_ns, 70, "and had no callees of its own");
        assert_eq!(s.fns[1].incl_ns, 90, "middle ran 10..100");
        assert_eq!(s.fns[1].excl_ns, 20, "minus inner's 70");
        assert_eq!(s.fns[0].incl_ns, 100);
        assert_eq!(s.fns[0].excl_ns, 10, "the catcher only owns what it ran");
        // Which is the property that matters: the exclusives partition again.
        let sum: u64 = s.fns.iter().map(|a| a.excl_ns).sum();
        assert_eq!(sum, s.fns[0].incl_ns);
        // Depths are settled, so a later call is not mis-timed.
        assert!(s.fns.iter().all(|a| a.depth == 0));
    }

    #[test]
    fn overflowing_the_shadow_stack_keeps_the_frames_it_did_hold() {
        let mut s = State::default();
        // id 0 wraps everything and must survive intact.
        s.enter_at(0, 0, 0, 0, 0, 0);
        // Fill the stack to the cap with id 1.
        for i in 0..(MAX_STACK - 1) {
            s.enter_at(1, i as u64, 0, 0, 0, 0);
        }
        assert_eq!(s.stack.len(), MAX_STACK);
        // Two further activations of a DIFFERENT id cannot be pushed.
        s.enter_at(2, 9_000, 0, 0, 0, 0);
        s.enter_at(2, 9_001, 0, 0, 0, 0);
        assert_eq!(s.dropped, 2);
        assert_eq!(s.stack.len(), MAX_STACK, "nothing was pushed past the cap");
        // Their exits must not disturb the stack.
        s.exit_at(2, 9_002, 0, 0, 0, 0);
        s.exit_at(2, 9_003, 0, 0, 0, 0);
        assert_eq!(s.stack.len(), MAX_STACK, "a dropped exit pops nothing");
        assert_eq!(s.dropped_depth, 0);
        // Unwind normally.
        for i in 0..(MAX_STACK - 1) {
            s.exit_at(1, 10_000 + i as u64, 0, 0, 0, 0);
        }
        s.exit_at(0, 100_000, 0, 0, 0, 0);
        assert!(s.stack.is_empty(), "fully unwound");
        // The outermost frame kept its span, which is what used to be destroyed.
        assert_eq!(s.fns[0].incl_ns, 100_000);
        assert_eq!(s.fns[0].depth, 0);
        assert_eq!(s.fns[1].depth, 0);
        // The dropped calls are counted but carry no timing.
        assert_eq!(s.fns[2].calls, 2);
        assert_eq!(s.fns[2].incl_ns, 0);
        // Exclusive time still partitions the root's inclusive.
        let sum: u64 = s.fns.iter().map(|a| a.excl_ns).sum();
        assert_eq!(sum, s.fns[0].incl_ns);
    }

    #[test]
    fn wait_splits_self_time_into_cpu_and_io() {
        // `query` spends 80 of its 100ns blocked in the driver; `compute` runs
        // 50ns of pure CPU. Wait is attributed like every other dimension, so
        // the caller's own wait excludes what its callees waited on.
        let mut s = State::default();
        s.enter_at(0, 0, 0, 0, 0, 0); // main
        s.enter_at(1, 10, 0, 0, 0, 0); // query
        s.exit_at(1, 110, 0, 0, 1, 80); // 100ns elapsed, 80ns of it waiting
        s.enter_at(2, 110, 0, 0, 1, 80); // compute
        s.exit_at(2, 160, 0, 0, 1, 80); // 50ns, no wait
        s.exit_at(0, 170, 0, 0, 1, 80); // main: 170ns total, 80 waited by a child
        assert_eq!(s.fns[1].incl_wait, 80);
        assert_eq!(s.fns[1].excl_wait, 80);
        assert_eq!(s.fns[2].excl_wait, 0, "pure CPU function waits for nothing");
        assert_eq!(s.fns[0].incl_wait, 80, "main's subtree waited 80ns");
        assert_eq!(s.fns[0].excl_wait, 0, "main itself never blocked");
        // CPU time = self time minus self wait.
        assert_eq!(s.fns[1].excl_ns - s.fns[1].excl_wait, 20);
        // Self wait partitions the root's inclusive wait, like the other dimensions.
        let sum: u64 = s.fns.iter().map(|a| a.excl_wait).sum();
        assert_eq!(sum, s.fns[0].incl_wait);
    }

    #[test]
    fn normalize_query_folds_literals_but_keeps_identifiers() {
        // String and numeric literals become ?, so an N+1 aggregates.
        assert_eq!(
            normalize_query("INSERT INTO users (name) VALUES ('user5')"),
            "INSERT INTO users (name) VALUES (?)"
        );
        assert_eq!(
            normalize_query("INSERT INTO users (name) VALUES ('user6')"),
            "INSERT INTO users (name) VALUES (?)"
        );
        assert_eq!(
            normalize_query("SELECT  name\n FROM users WHERE id =  42"),
            "SELECT name FROM users WHERE id = ?"
        );
        // Already-parameterized statements are unchanged; identifiers with
        // digits (col2, md5) keep their digits.
        assert_eq!(
            normalize_query("SELECT col2 FROM t WHERE id = ?"),
            "SELECT col2 FROM t WHERE id = ?"
        );
        // Escaped quote inside a literal does not end it early.
        assert_eq!(normalize_query("SELECT 'a''b'"), "SELECT ?");
    }

    #[test]
    fn instr_query_aggregates_by_shape() {
        // Fresh isolation: this test owns the process's QUERIES if run alone;
        // assert on the delta for the two shapes it records.
        let before: std::collections::HashMap<String, u64> = QUERIES
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect();
        for i in 0..3 {
            let s = format!("INSERT INTO t VALUES ('x{i}')");
            elephc_instr_query(s.as_ptr(), s.len());
        }
        let sel = "SELECT * FROM t WHERE id = ?";
        elephc_instr_query(sel.as_ptr(), sel.len());
        let after: std::collections::HashMap<String, u64> =
            QUERIES.lock().unwrap().iter().cloned().collect();
        let ins_key = "INSERT INTO t VALUES (?)";
        assert_eq!(
            after.get(ins_key).copied().unwrap_or(0) - before.get(ins_key).copied().unwrap_or(0),
            3,
            "three inserts fold into one shape"
        );
        assert_eq!(
            after.get(sel).copied().unwrap_or(0) - before.get(sel).copied().unwrap_or(0),
            1
        );
        let out = render_queries();
        assert!(out.contains("elephc-instr-query: 3 INSERT INTO t VALUES (?)"), "{out}");
    }

    #[test]
    fn chrome_trace_is_well_formed() {
        ticks_are_nanoseconds();
        // Spans in ns; base is the min enter. Complete ('X') events, µs.
        let spans = vec![(0u32, 1_000u64, 5_000u64), (1u32, 2_000u64, 3_500u64)];
        let names = vec!["{main}".to_string(), "child".to_string()];
        let dir = std::env::temp_dir();
        let path = dir.join("elephc_instr_trace_test.json");
        write_chrome_trace(path.to_str().unwrap(), &spans, &names, 0);
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(text.starts_with("{\"traceEvents\":["), "{text}");
        assert!(text.contains("\"displayTimeUnit\":\"ms\""), "{text}");
        // {main}: enter 1000ns == base -> ts 0 µs, dur (5000-1000)/1000 = 4 µs.
        assert!(text.contains("\"name\":\"{main}\",\"cat\":\"php\",\"ph\":\"X\",\"pid\":1,\"tid\":1,\"ts\":0,\"dur\":4"), "{text}");
        // child: enter 2000ns -> ts 1 µs, dur 1.5 µs.
        assert!(text.contains("\"name\":\"child\""), "{text}");
        assert!(text.contains("\"ts\":1,\"dur\":1.5"), "{text}");
    }

    #[test]
    fn json_escape_handles_specials() {
        assert_eq!(json_escape("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(json_escape("x\ny"), "x\\ny");
    }
}
