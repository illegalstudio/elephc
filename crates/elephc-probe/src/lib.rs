//! Purpose:
//! In-process sampling probe for elephc-compiled programs: a SIGPROF handler
//! walks the interrupted frame-pointer chain into a fixed ring buffer, and the
//! exit dump symbolizes the raw program counters against the symbol table the
//! COMPILER embedded — no DWARF, no external sampler, no target suspension.
//!
//! Called from:
//! - Generated code: `elephc_probe_init(table, len)` in main's prologue and
//!   `elephc_probe_dump()` in its epilogue, emitted under `--probe`.
//!
//! Key details:
//! - The handler is async-signal-safe by construction: raw pointer writes into
//!   preallocated statics, one atomic head increment, no allocation, no locks.
//! - Samples record raw PCs; symbolization happens at dump time, outside the
//!   handler. A PC below the first function or past the compiler-emitted text
//!   end sentinel reports as `<native>` (runtime helpers, libc).
//! - The ring holds the most recent `RING_SLOTS` samples (~1 sample/ms of CPU
//!   time); long runs keep the tail, which is what a probe window serves.

pub mod endpoint;
pub mod handshake;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// One entry of the compiler-embedded symbol table: function start address,
/// name pointer, name length. Layout must match the emitted `.quad` triples.
#[repr(C)]
pub struct SymtabEntry {
    pub address: u64,
    pub name_ptr: u64,
    pub name_len: u64,
}

/// Deepest stack the handler records; deeper frames are truncated at the root.
const MAX_FRAMES: usize = 32;
/// Ring capacity in samples. At the 1ms period this holds ~8s of CPU time.
const RING_SLOTS: usize = 8192;
/// Words per ring slot: `[depth, route_id, allocs, pc0, pc1, ...]`.
const SLOT_WORDS: usize = MAX_FRAMES + 3;
/// Word index of the first program counter in a slot.
const PC_WORD0: usize = 3;
/// Cap on distinct request routes recorded; overflow buckets into `<other>`.
const MAX_ROUTES: usize = 256;
/// Bytes per shared route slot: a 1-byte length then up to 63 name bytes.
const ROUTE_SLOT_BYTES: usize = 64;
/// Max route name length that fits a slot.
const ROUTE_NAME_MAX: usize = ROUTE_SLOT_BYTES - 1;
/// Ring bytes: an 8-byte head counter followed by the slot array.
const RING_BYTES: usize = 8 + RING_SLOTS * SLOT_WORDS * 8;
/// Route-table bytes: an 8-byte count followed by the fixed route slots.
const ROUTE_TABLE_BYTES: usize = 8 + MAX_ROUTES * ROUTE_SLOT_BYTES;
/// Words per route in the event table: `[io_ops, wait_ns]`.
const EVENT_WORDS: usize = 2;
/// One extra bucket for id 0, which the route table reserves for "untagged" —
/// a CLI run, or a `--web` request whose route did not fit the table. Dropping
/// those events would understate the totals silently, which is worse than a row
/// nobody expected.
const EVENT_BUCKETS: usize = MAX_ROUTES + 1;
/// Event-table bytes: fixed counters per route id.
const EVENT_TABLE_BYTES: usize = EVENT_BUCKETS * EVENT_WORDS * 8;
/// Shared-region byte size: the ring, then the route table, then the per-route
/// event counters — all inherited across a `--web` fork, so route ids stay
/// consistent and every worker's counters land in one place.
const REGION_BYTES: usize = RING_BYTES + ROUTE_TABLE_BYTES + EVENT_TABLE_BYTES;

/// I/O events are **not sampled**. A driver call fires exactly one, so these
/// counts are exact — the sampler's statistical nature applies to *time*, which
/// it observes 1000x/second, not to events it is told about. Keeping the two
/// apart matters: an exact query count printed beside a sampled time share,
/// with nothing saying which is which, is how a profile misleads.
///
/// `route_id` is the 1-based id the route table hands out; 0 means untagged.
unsafe fn event_word<'a>(
    base: usize,
    route_id: usize,
    word: usize,
) -> &'a std::sync::atomic::AtomicU64 {
    let offset = RING_BYTES + ROUTE_TABLE_BYTES + (route_id * EVENT_WORDS + word) * 8;
    &*((base + offset) as *const std::sync::atomic::AtomicU64)
}

/// Adds `amount` to one of the current request's event counters.
fn note_event(word: usize, amount: u64) {
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 {
        return;
    }
    let route_id = CURRENT_ROUTE.load(Ordering::Relaxed) as usize;
    if route_id < EVENT_BUCKETS {
        unsafe { event_word(base, route_id, word) }.fetch_add(amount, Ordering::Relaxed);
    }
}

/// Records one I/O operation against the request currently being served.
///
/// Reached through the same `_elephc_instr_io_fn` slot the exact profiler uses,
/// so the PDO bridge needs no knowledge of which profiler is linked. Costs one
/// atomic increment, paid only when a driver call happens — already orders of
/// magnitude slower — so unlike per-call instrumentation this is viable in
/// production.
#[no_mangle]
pub extern "C" fn elephc_probe_note_io() {
    note_event(0, 1);
}

/// Records nanoseconds blocked inside a driver call, against the current request.
#[no_mangle]
pub extern "C" fn elephc_probe_note_wait(ns: u64) {
    note_event(1, ns);
}

/// Renders the per-route event counters, one line per route that saw any I/O.
///
/// Deliberately its own line prefix rather than extra columns on the folded
/// samples: a consumer must not be able to mistake an exact count for a sampled
/// weight.
pub fn event_report(base: usize) -> String {
    let mut out = String::new();
    if base == 0 {
        return out;
    }
    let count = unsafe {
        (*((base + RING_BYTES) as *const std::sync::atomic::AtomicU64)).load(Ordering::Acquire)
    } as usize;
    for route_id in 0..EVENT_BUCKETS.min(count + 1) {
        let io = unsafe { event_word(base, route_id, 0) }.load(Ordering::Relaxed);
        let wait = unsafe { event_word(base, route_id, 1) }.load(Ordering::Relaxed);
        if io == 0 && wait == 0 {
            continue;
        }
        let name = if route_id == 0 {
            "<untagged>".to_string()
        } else {
            unsafe { read_route_slot(base, route_id - 1) }
        };
        out.push_str(&format!("elephc-probe-io: {name} ops={io} wait_ns={wait}\n"));
    }
    out
}
/// Sampling period, microseconds of CPU time between SIGPROF deliveries.
const PERIOD_MICROS: i64 = 1000;

/// Base address of the sample region: an atomic head counter at offset 0, then
/// `RING_SLOTS` slots of `[depth, pc...]`. Zero until `elephc_probe_init` maps
/// it. Mapped `MAP_SHARED` BEFORE any `--web` fork, so every worker's SIGPROF
/// handler cooperatively fills one shared ring the master's endpoint reads.
static REGION: AtomicUsize = AtomicUsize::new(0);

extern "C" {
    /// Runtime `.comm` word: 1 once this process has been asked to profile.
    /// Written here, read by the exact profiler's boot, which runs afterwards.
    static mut elephc_monitor_active: u64;
}
/// Set by `elephc_probe_dump` so a late signal cannot race the ring read.
static STOPPED: AtomicBool = AtomicBool::new(false);

/// Returns the shared head counter, or `None` before the region is mapped.
///
/// # Safety
/// Valid only after `elephc_probe_init` mapped the region; the returned
/// reference lives as long as the process (the mapping is never unmapped).
unsafe fn region_head<'a>() -> Option<&'a std::sync::atomic::AtomicU64> {
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 {
        return None;
    }
    Some(&*(base as *const std::sync::atomic::AtomicU64))
}

/// Returns word `word` of ring slot `index` as an atomic — the ring is shared
/// with a concurrent reader, so every slot word is accessed atomically.
///
/// # Safety
/// `base` must be the mapped region, `index < RING_SLOTS`, `word < SLOT_WORDS`.
unsafe fn region_word<'a>(
    base: usize,
    index: usize,
    word: usize,
) -> &'a std::sync::atomic::AtomicU64 {
    let addr = base + 8 + index * SLOT_WORDS * 8 + word * 8;
    &*(addr as *const std::sync::atomic::AtomicU64)
}

/// Symbol table pointer/length, published once by `elephc_probe_init`.
static TABLE_PTR: AtomicUsize = AtomicUsize::new(0);
static TABLE_LEN: AtomicUsize = AtomicUsize::new(0);
/// Build-key pointer, published by `elephc_probe_init` for the endpoint handshake.
static KEY_PTR: AtomicUsize = AtomicUsize::new(0);
/// Route id (1-based index into `ROUTES`) the SIGPROF handler stamps onto each
/// sample; 0 means no active request. Set by `elephc_probe_set_route`.
static CURRENT_ROUTE: AtomicUsize = AtomicUsize::new(0);

extern "C" {
    /// Runtime `.comm` slot holding the ADDRESS of the program's `_gc_allocs`
    /// counter, published by `--probe` init. Zero when the program was not built
    /// with `--probe`, which is also when this crate is not linked.
    ///
    /// A pointer rather than the counter itself: `_gc_allocs` is emitted with a
    /// hardcoded leading underscore, which is fine while only assembly names it
    /// and would break every ELF link the moment a Rust crate resolved it.
    static elephc_probe_allocs_ptr: usize;
}

/// Allocation count at the previous sample, for the delta this one is charged.
///
/// Process-local on purpose. `_gc_allocs` is ordinary memory, so each `--web`
/// worker gets its own copy at fork; a shared "last" would make every worker
/// subtract another's progress and produce negative deltas.
static LAST_ALLOCS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Reads the program's allocation counter, or `None` outside `--probe`.
///
/// # Safety
/// Called from the SIGPROF handler: a plain load of a `.comm` word, which is
/// async-signal-safe.
unsafe fn current_allocs() -> Option<u64> {
    let addr = std::ptr::addr_of!(elephc_probe_allocs_ptr).read();
    if addr == 0 {
        return None;
    }
    Some(*(addr as *const u64))
}

/// Interns `route` into the SHARED route table, returning its 1-based id — so a
/// route id stamped by one `--web` worker resolves to the same name in the
/// master's endpoint. Full table buckets into `<other>`. Runs in the worker's
/// normal context (append + scan), never the signal handler.
///
/// # Safety
/// Valid only after the region is mapped.
unsafe fn intern_route(route: &str) -> usize {
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 {
        return 0;
    }
    // The route comes from an untrusted HTTP path. Neutralize the folded-format
    // metacharacters — `;` (frame separator), newlines (line separator) — and any
    // other control byte, so a crafted path cannot forge frames or profile lines.
    let sanitized: String = route
        .chars()
        .map(|c| {
            if c == ';' || c.is_control() {
                '?'
            } else {
                c
            }
        })
        .collect();
    let mut name = sanitized.as_str();
    if name.len() > ROUTE_NAME_MAX {
        // Truncate on a char boundary so the stored name stays valid UTF-8.
        let mut end = ROUTE_NAME_MAX;
        while end > 0 && !name.is_char_boundary(end) {
            end -= 1;
        }
        name = &name[..end];
    }
    let count_ptr = (base + RING_BYTES) as *const std::sync::atomic::AtomicU64;
    let count = &*count_ptr;
    let existing = count.load(Ordering::Acquire) as usize;
    for index in 0..existing.min(MAX_ROUTES) {
        if read_route_slot(base, index) == name {
            return index + 1;
        }
    }
    if existing >= MAX_ROUTES {
        // Table full: leave the sample untagged (id 0) rather than mis-attribute
        // it to an arbitrary existing route.
        return 0;
    }
    // Claim the next slot. A benign race can duplicate a name across workers;
    // both ids resolve to the same text, so grouping is unaffected.
    let index = count.fetch_add(1, Ordering::AcqRel) as usize;
    if index >= MAX_ROUTES {
        return 0;
    }
    write_route_slot(base, index, name.as_bytes());
    index + 1
}

/// Writes a route name into shared slot `index` (`[len][bytes]`). The bytes are
/// stored first, then the length via an `AtomicU8` Release store as the slot's
/// readiness marker: a reader that loads a non-zero length Acquire is
/// guaranteed to see the bytes it covers. The length byte is a real atomic, so
/// the concurrent reader/writer pair is not a data race.
unsafe fn write_route_slot(base: usize, index: usize, name: &[u8]) {
    let slot = (base + RING_BYTES + 8 + index * ROUTE_SLOT_BYTES) as *mut u8;
    let len = name.len().min(ROUTE_NAME_MAX) as u8;
    std::ptr::copy_nonoverlapping(name.as_ptr(), slot.add(1), len as usize);
    (*(slot as *const std::sync::atomic::AtomicU8)).store(len, Ordering::Release);
}

/// Reads the route name from shared slot `index`. An empty (unpublished) slot
/// reads as `""`; callers treat that as "no route".
unsafe fn read_route_slot(base: usize, index: usize) -> String {
    let slot = (base + RING_BYTES + 8 + index * ROUTE_SLOT_BYTES) as *const u8;
    let len = ((*(slot as *const std::sync::atomic::AtomicU8)).load(Ordering::Acquire) as usize)
        .min(ROUTE_NAME_MAX);
    let bytes = std::slice::from_raw_parts(slot.add(1), len);
    String::from_utf8_lossy(bytes).into_owned()
}

/// Sets the route stamped onto subsequent samples until cleared. An empty
/// `len` clears it (id 0). Called by the web bridge around each request via a
/// `dlsym` lookup, so a non-`--web` binary never pays for it.
///
/// # Safety
/// `route`/`len` describe a UTF-8 route string valid for this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_set_route(route: *const u8, len: usize) {
    // Defensive: a bridge bug passing a wild length must not read out of bounds.
    if route.is_null() || len == 0 || len > 4096 {
        CURRENT_ROUTE.store(0, Ordering::Relaxed);
        return;
    }
    let bytes = std::slice::from_raw_parts(route, len);
    let Ok(text) = std::str::from_utf8(bytes) else {
        CURRENT_ROUTE.store(0, Ordering::Relaxed);
        return;
    };
    let id = intern_route(text);
    CURRENT_ROUTE.store(id, Ordering::Relaxed);
}

/// Resolves a route id to its interned name from the shared table.
///
/// # Safety
/// Valid only after the region is mapped.
unsafe fn route_name(id: usize) -> Option<String> {
    if id == 0 {
        return None;
    }
    let base = REGION.load(Ordering::Relaxed);
    if base == 0 || id > MAX_ROUTES {
        return None;
    }
    let name = read_route_slot(base, id - 1);
    // An unpublished (empty) slot is treated as no route, never a blank frame.
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Extracts the interrupted program counter and frame pointer from the signal
/// context, per platform and architecture.
///
/// Returns the interrupted `(pc, fp, sp)`. `sp` anchors the frame-pointer walk:
/// a valid frame lies at or above the current stack pointer, which rejects a
/// stale `fp` that only happens to look plausible before it faults.
///
/// # Safety
/// `context` must be the `ucontext_t` pointer SIGPROF delivered.
unsafe fn interrupted_pc_fp(context: *mut libc::c_void) -> (u64, u64, u64) {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let context = context as *mut libc::ucontext_t;
        let state = &(*(*context).uc_mcontext).__ss;
        (state.__pc, state.__fp, state.__sp)
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        let context = context as *mut libc::ucontext_t;
        let state = &(*(*context).uc_mcontext).__ss;
        (state.__rip, state.__rbp, state.__rsp)
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        let context = context as *mut libc::ucontext_t;
        let state = &(*context).uc_mcontext;
        (state.pc, state.regs[29], state.sp)
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let context = context as *mut libc::ucontext_t;
        let gregs = &(*context).uc_mcontext.gregs;
        (
            gregs[libc::REG_RIP as usize] as u64,
            gregs[libc::REG_RBP as usize] as u64,
            gregs[libc::REG_RSP as usize] as u64,
        )
    }
}

/// Upper bound of the frame-pointer walk above the interrupted stack pointer.
/// A valid frame chain stays within one stack; 64 MiB comfortably covers a
/// worker/CLI stack while rejecting a wild `fp` far from `sp`.
const STACK_WINDOW: u64 = 64 * 1024 * 1024;

/// The SIGPROF handler: records the interrupted PC plus the return addresses
/// of the frame-pointer chain into the next ring slot.
///
/// Both supported ABIs store `[fp] = caller fp, [fp + 8] = return address`,
/// which is what makes one walker serve AArch64 and x86_64.
extern "C" fn on_sigprof(
    _signal: libc::c_int,
    _info: *mut libc::siginfo_t,
    context: *mut libc::c_void,
) {
    if STOPPED.load(Ordering::Relaxed) {
        return;
    }
    unsafe {
        let base = REGION.load(Ordering::Relaxed);
        if base == 0 {
            return;
        }
        let head = &*(base as *const std::sync::atomic::AtomicU64);
        let (pc, mut fp, sp) = interrupted_pc_fp(context);
        let slot_index = (head.fetch_add(1, Ordering::Relaxed) as usize) % RING_SLOTS;
        // Slot words are atomics: the reader (endpoint/dump) runs concurrently
        // with this handler in another thread, so plain stores would be a data
        // race. depth (word 0) is the readiness gate, published Release last.
        let depth_word = region_word(base, slot_index, 0);
        let route_word = region_word(base, slot_index, 1);
        // Slot layout: [depth, route_id, pc0, pc1, ...]. Stamp the active request
        // route so the dump can group samples by endpoint.
        route_word.store(CURRENT_ROUTE.load(Ordering::Relaxed) as u64, Ordering::Relaxed);
        // Allocations since the previous sample, charged to the stack this one
        // captures — sampled attribution, exactly like Go's heap profile. The
        // counter only grows, so a wrapped or reset read yields 0 rather than a
        // wild delta.
        let allocs_delta = match current_allocs() {
            Some(now) => {
                let previous = LAST_ALLOCS.swap(now, Ordering::Relaxed);
                now.saturating_sub(previous)
            }
            None => 0,
        };
        region_word(base, slot_index, 2).store(allocs_delta, Ordering::Relaxed);
        let mut depth = 0usize;
        region_word(base, slot_index, PC_WORD0).store(pc, Ordering::Relaxed);
        depth += 1;
        while depth < MAX_FRAMES {
            // A valid frame pointer is nonzero, 16-byte aligned, and inside the
            // interrupted stack window `[sp, sp + STACK_WINDOW)`. Anchoring to sp
            // rejects a stale fp before dereferencing it can fault.
            if fp == 0
                || fp & 0xf != 0
                || fp < sp
                || fp.wrapping_sub(sp) >= STACK_WINDOW
                || fp > u64::MAX - 8
            {
                break;
            }
            let next_fp = *(fp as *const u64);
            let return_address = *((fp + 8) as *const u64);
            if return_address < 0x1000 {
                break;
            }
            region_word(base, slot_index, depth + PC_WORD0).store(return_address, Ordering::Relaxed);
            depth += 1;
            // Frames must strictly grow toward higher addresses or the chain
            // is corrupt (or we crossed into a differently-shaped frame).
            if next_fp <= fp {
                break;
            }
            fp = next_fp;
        }
        // Publish depth last with Release so a reader that loads it Acquire never
        // sees a higher depth than the PCs already stored.
        depth_word.store(depth as u64, Ordering::Release);
    }
}

/// Installs the SIGPROF handler and arms the profiling timer.
///
/// `table`/`len` describe the compiler-embedded symbol table; the final entry
/// is the text-end sentinel (name `<end>`), which bounds the last function.
///
/// # Safety
/// Called once from generated code before user code runs; `table` must point
/// at `len` valid entries that live for the whole process.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_init(table: *const SymtabEntry, len: usize, key: *const u8) {
    TABLE_PTR.store(table as usize, Ordering::Relaxed);
    TABLE_LEN.store(len, Ordering::Relaxed);
    KEY_PTR.store(key as usize, Ordering::Relaxed);

    // Map the sample region MAP_SHARED before any --web fork: the mapping is
    // inherited by every worker, so all workers' SIGPROF handlers fill one ring
    // through the shared atomic head. Zero-filled by the kernel. If the map
    // fails the probe stays inert (REGION 0) rather than crash the process.
    #[cfg(target_os = "linux")]
    let anon = libc::MAP_ANONYMOUS;
    #[cfg(not(target_os = "linux"))]
    let anon = libc::MAP_ANON;
    let region = libc::mmap(
        std::ptr::null_mut(),
        REGION_BYTES,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_SHARED | anon,
        -1,
        0,
    );
    if region == libc::MAP_FAILED {
        return;
    }
    REGION.store(region as usize, Ordering::Relaxed);

    let mut action: libc::sigaction = std::mem::zeroed();
    // Cast through the fn-pointer type before usize, not the fn item directly.
    action.sa_sigaction =
        on_sigprof as extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void) as usize;
    action.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
    libc::sigemptyset(&mut action.sa_mask);
    if libc::sigaction(libc::SIGPROF, &action, std::ptr::null_mut()) != 0 {
        return;
    }

    // Embedded but dormant. `--with-monitoring` makes a binary CAPABLE of being
    // profiled; it must otherwise run — and cost — like any other binary, or the
    // flag would be a performance decision disguised as a capability. `monitor`
    // sets ELEPHC_MONITOR when it wants a whole run, and a live endpoint
    // (ELEPHC_PROBE_ADDR) implies the same.
    // `monitor` spawned us, or an endpoint was configured for a long-running
    // service. Publish the decision so the exact profiler, whose init runs after
    // this one, does not have to repeat the check — and would consume the marker
    // if it did.
    if control_fd_present() || std::env::var_os("ELEPHC_PROBE_ADDR").is_some() {
        let flag = std::ptr::addr_of_mut!(elephc_monitor_active);
        flag.write(1);
        arm_timer();
    }

    // fork() RESETS interval timers in the child (POSIX; `man 2 fork`), so a
    // plain fork+exec — every popen/exec/proc_open the profiled PHP does — is
    // already safe: the child starts with ITIMER_PROF disarmed. The atfork
    // child handler below is belt-and-suspenders for that path. The genuine
    // hazard is execve WITHOUT a preceding fork (a self re-exec / graceful
    // restart): the armed timer is PRESERVED across execve while execve resets
    // SIGPROF to its default (terminate), so the new image would die with
    // "Profiling timer expired". A host that re-execs itself must call
    // `elephc_probe_disarm` first. A --web worker, forked but kept running,
    // re-arms through `elephc_probe_rearm`.
    libc::pthread_atfork(None, None, Some(disarm_after_fork));

    // The remote endpoint is opt-in: a Unix socket path in ELEPHC_PROBE_ADDR
    // turns it on. A background thread accepts connections, runs the build-key
    // handshake, and serves the folded profile — so a live production process
    // can be profiled by `elephc monitor --probe-host` without SIGPROF from
    // outside and without suspending the process.
    if !key.is_null() {
        if let Ok(path) = std::env::var("ELEPHC_PROBE_ADDR") {
            if !path.is_empty() {
                endpoint::spawn(path);
            }
        }
    }
}

/// Arms the CPU-time profiling timer at the sampling period. Idempotent and
/// async-signal-safe (one `setitimer` syscall) — safe from a fork child.
unsafe fn arm_timer() {
    let interval = libc::timeval {
        tv_sec: 0,
        tv_usec: PERIOD_MICROS as libc::suseconds_t,
    };
    let timer = libc::itimerval {
        it_interval: interval,
        it_value: interval,
    };
    libc::setitimer(libc::ITIMER_PROF, &timer, std::ptr::null_mut());
}

/// Disarms the profiling timer. Async-signal-safe (one `setitimer` syscall).
unsafe fn disarm_timer() {
    let off = libc::itimerval {
        it_interval: libc::timeval { tv_sec: 0, tv_usec: 0 },
        it_value: libc::timeval { tv_sec: 0, tv_usec: 0 },
    };
    libc::setitimer(libc::ITIMER_PROF, &off, std::ptr::null_mut());
}

/// `pthread_atfork` child hook: disarm the timer in the child (belt-and-
/// suspenders — fork already resets it). A `--web` worker re-arms via
/// `elephc_probe_rearm`.
extern "C" fn disarm_after_fork() {
    unsafe { disarm_timer() };
}

/// Disarms the profiling timer. A host that calls `execve` WITHOUT forking (a
/// self re-exec / graceful restart) must call this first, otherwise the armed
/// timer survives the exec and the default SIGPROF action kills the new image.
///
/// # Safety
/// Ordinary FFI entry; just disarms the interval timer.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_disarm() {
    disarm_timer();
}

/// Re-arms the profiling timer in a process that forked but keeps sampling (a
/// `--web` worker). Called by the web bridge at worker startup through the
/// runtime `_elephc_probe_rearm_fn` slot; a no-op if the probe is not active.
///
/// # Safety
/// Ordinary FFI entry; just arms the interval timer.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_rearm() {
    if REGION.load(Ordering::Relaxed) != 0 {
        arm_timer();
    }
}

/// Returns the embedded build key, or `None` if unpublished.
fn build_key() -> Option<[u8; handshake::KEY_LEN]> {
    let ptr = KEY_PTR.load(Ordering::Relaxed) as *const u8;
    if ptr.is_null() {
        return None;
    }
    let mut key = [0u8; handshake::KEY_LEN];
    // Safety: the compiler embeds exactly KEY_LEN bytes at this symbol.
    unsafe { std::ptr::copy_nonoverlapping(ptr, key.as_mut_ptr(), handshake::KEY_LEN) };
    Some(key)
}

/// File descriptor `monitor` hands the child, one end of a socketpair it made
/// before forking.
const CONTROL_FD: i32 = 3;
/// What `monitor` writes into that socket before spawning, so the data is already
/// buffered when the child looks. A stray inherited socket on the same descriptor
/// says nothing and is ignored.
const CONTROL_MAGIC: &[u8] = b"ELEPHC-MONITOR-1";

/// Whether this process was started by `elephc monitor`.
///
/// The credential is the CHANNEL, not a token: only the parent that forked this
/// process holds the other end of that socketpair. Nothing to copy out of a
/// process list, nothing left in a log, nothing to replay — which is what a
/// signed environment variable, however well signed, cannot offer, because it is
/// visible to everything on the machine.
///
/// Reads without blocking and without consuming more than the marker, so a
/// descriptor that happens to be open says no rather than hanging the program.
fn control_fd_present() -> bool {
    unsafe {
        // Must be a socket: an inherited file or pipe on the same number is not
        // a control channel, and treating it as one would start profiling a
        // program nobody asked about.
        let mut kind: libc::c_int = 0;
        let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        let ok = libc::getsockopt(
            CONTROL_FD,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            &mut kind as *mut _ as *mut libc::c_void,
            &mut len,
        );
        if ok != 0 || kind != libc::SOCK_STREAM {
            return false;
        }
        // PEEK, then take only what is ours.
        //
        // Reading first and asking afterwards destroys data belonging to a
        // program that never asked to be profiled: fd 3 is an ordinary number, a
        // supervisor may hand a child a connected socket on it, and consuming 16
        // bytes of someone else's protocol is silent and unrecoverable. Measured
        // before this changed: a 35-byte payload came back 19 bytes long.
        let mut buf = [0u8; 16];
        let read = libc::recv(
            CONTROL_FD,
            buf.as_mut_ptr() as *mut libc::c_void,
            buf.len(),
            libc::MSG_PEEK | libc::MSG_DONTWAIT,
        );
        if read != CONTROL_MAGIC.len() as isize || buf != CONTROL_MAGIC {
            return false;
        }
        // It is ours: consume the marker so nothing downstream reads it back.
        libc::recv(
            CONTROL_FD,
            buf.as_mut_ptr() as *mut libc::c_void,
            CONTROL_MAGIC.len(),
            libc::MSG_DONTWAIT,
        );
        true
    }
}

/// How far a signed profiling request may be from the server's clock, in
/// seconds. Wide enough for real clock skew between two hosts, narrow enough that
/// a captured header stops working long before anyone finds it in a log.
const QUERY_WINDOW_SECS: i64 = 300;

/// Verifies an `X-Elephc-Query` value against the embedded build key.
///
/// Format: `t=<unix seconds>,v=<hex hmac of the timestamp>`. Turning profiling on
/// is a privileged act — it costs the request real time and exposes the shape of
/// the code — so asking has to be something only a holder of the build key can
/// do. An unsigned trigger means anyone who can set a header can profile your
/// production, which is the hole a bare on/off flag leaves open.
///
/// The timestamp is what stops a captured header being replayed forever; the
/// comparison is constant-time, so a wrong tag leaks nothing about the right one.
///
/// Returns 1 when the value is authentic and current, 0 otherwise. Reached
/// through a slot, so the web bridge needs no knowledge of this crate.
#[no_mangle]
pub extern "C" fn elephc_probe_verify_query(ptr: *const u8, len: usize) -> u32 {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    let Some(key) = build_key() else {
        return 0;
    };
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let Ok(value) = std::str::from_utf8(bytes) else {
        return 0;
    };
    let mut stamp: Option<i64> = None;
    let mut tag: Option<Vec<u8>> = None;
    for field in value.split(',') {
        let field = field.trim();
        if let Some(raw) = field.strip_prefix("t=") {
            stamp = raw.parse::<i64>().ok();
        } else if let Some(raw) = field.strip_prefix("v=") {
            tag = decode_hex(raw);
        }
    }
    let (Some(stamp), Some(tag)) = (stamp, tag) else {
        return 0;
    };

    let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut now) };
    if !within_query_window(now.tv_sec as i64, stamp) {
        return 0;
    }
    let expected = handshake::hmac_sha256(&key, stamp.to_string().as_bytes());
    u32::from(handshake::tags_equal(&expected, &tag))
}

/// Whether a signed timestamp is close enough to now to be accepted.
///
/// Saturating on purpose. `stamp` is parsed straight out of an HTTP header, so a
/// client picks it: with plain `now - stamp` a value near `i64::MIN` overflows —
/// which panics outright in debug, and in release wraps to `i64::MIN`, where
/// `.abs()` panics unconditionally. Either way one crafted header aborts the
/// process, and the panic crosses an `extern "C"` boundary on its way out. The
/// header is accepted from untrusted clients by design, so the arithmetic that
/// reads it has to be total.
///
/// Extracted rather than left inline because inline it was untestable: the
/// function around it returns early when no build key is embedded, which every
/// test build is, so a test could never reach the expression.
fn within_query_window(now: i64, stamp: i64) -> bool {
    now.saturating_sub(stamp).saturating_abs() <= QUERY_WINDOW_SECS
}

/// Lowercase hex to bytes; `None` on anything malformed, so a truncated tag is
/// rejected rather than compared against a prefix.
fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 || text.is_empty() {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let pair = std::str::from_utf8(&bytes[i..i + 2]).ok()?;
        out.push(u8::from_str_radix(pair, 16).ok()?);
        i += 2;
    }
    Some(out)
}

/// Renders the current folded profile for the endpoint responder.
pub fn current_folded_profile() -> Option<String> {
    unsafe { folded_profile() }
}

/// Disarms the timer and writes the folded profile to stderr: one
/// `elephc-probe: root;...;leaf <count>` line per distinct stack — Brendan
/// Gregg's folded format, flamegraph- and diff-friendly.
///
/// # Safety
/// Called from generated code after user code finished; no handler runs
/// concurrently once the stop flag is set and the timer is disarmed.
#[no_mangle]
pub unsafe extern "C" fn elephc_probe_dump() {
    STOPPED.store(true, Ordering::Relaxed);
    let disarm = libc::itimerval {
        it_interval: libc::timeval { tv_sec: 0, tv_usec: 0 },
        it_value: libc::timeval { tv_sec: 0, tv_usec: 0 },
    };
    libc::setitimer(libc::ITIMER_PROF, &disarm, std::ptr::null_mut());

    if let Some(text) = folded_profile() {
        eprint!("{text}");
    }
}

/// Renders the current ring contents as folded-stack text: one
/// `elephc-probe: root;...;leaf <count>` line per distinct stack, followed by
/// `elephc-probe-samples: <total>`. Returns `None` if the symbol table is
/// unpublished. Shared by the exit dump and the endpoint responder.
///
/// # Safety
/// Reads the ring; call after the timer is disarmed (exit dump) or accept that
/// a concurrent handler may add a sample mid-read (endpoint) — a raced slot
/// only skews one count, never corrupts memory.
unsafe fn folded_profile() -> Option<String> {
    let table = TABLE_PTR.load(Ordering::Relaxed) as *const SymtabEntry;
    let table_len = TABLE_LEN.load(Ordering::Relaxed);
    if table.is_null() || table_len == 0 {
        return None;
    }
    let entries = std::slice::from_raw_parts(table, table_len);
    let mut symbols: Vec<(u64, &str)> = entries
        .iter()
        .map(|entry| {
            let name = std::str::from_utf8(std::slice::from_raw_parts(
                entry.name_ptr as *const u8,
                entry.name_len as usize,
            ))
            .unwrap_or("<bad-name>");
            (entry.address, name)
        })
        .collect();
    symbols.sort_by_key(|(address, _)| *address);

    let head = region_head()?;
    let base = REGION.load(Ordering::Relaxed);
    let taken = head.load(Ordering::Relaxed) as usize;
    let available = taken.min(RING_SLOTS);
    let mut valid = 0u64;
    let mut folded: std::collections::BTreeMap<Vec<String>, u64> = std::collections::BTreeMap::new();
    // Allocations charged to each stack, kept apart from the sample counts: they
    // are a different quantity, and summing them into one weight would produce a
    // profile whose bars mean two things at once.
    let mut allocated: std::collections::BTreeMap<Vec<String>, u64> =
        std::collections::BTreeMap::new();
    for index in 0..available {
        // Acquire the depth gate before reading the PCs the handler stored; a
        // torn or in-flight slot with depth 0 is skipped.
        let depth = (region_word(base, index, 0).load(Ordering::Acquire) as usize).min(MAX_FRAMES);
        if depth == 0 {
            continue;
        }
        valid += 1;
        // Recorded leaf-first; fold root-first like every consumer expects. The
        // leaf (frame 0) is the interrupted PC; frames 1.. are RETURN addresses,
        // so bias them by -1 to land inside the calling instruction rather than
        // the start of whatever follows the call.
        let mut stack: Vec<String> = (0..depth)
            .map(|frame| {
                let raw = region_word(base, index, frame + PC_WORD0).load(Ordering::Relaxed);
                let pc = if frame == 0 { raw } else { raw.wrapping_sub(1) };
                symbolize(&symbols, pc).to_string()
            })
            .collect();
        stack.reverse();
        stack.dedup();
        // Prefix the request route as the outermost frame, so every consumer —
        // the table, the flamegraph, pprof — groups samples by endpoint for
        // free. Samples outside a request keep their plain stack.
        let route_id = region_word(base, index, 1).load(Ordering::Relaxed) as usize;
        if let Some(route) = route_name(route_id) {
            stack.insert(0, route);
        }
        let allocs = region_word(base, index, 2).load(Ordering::Relaxed);
        if allocs > 0 {
            *allocated.entry(stack.clone()).or_default() += allocs;
        }
        *folded.entry(stack).or_default() += 1;
    }
    // A dormant binary took no samples and recorded no events. Saying
    // "elephc-probe-samples: 0" would still announce a profiler to anyone
    // reading the program's own stderr, which is exactly what
    // `--with-monitoring` promises not to do until asked.
    if folded.is_empty() && allocated.is_empty() && event_report(base).is_empty() {
        return Some(String::new());
    }
    let mut out = String::new();
    for (stack, count) in &folded {
        out.push_str("elephc-probe: ");
        out.push_str(&stack.join(";"));
        out.push(' ');
        out.push_str(&count.to_string());
        out.push('\n');
    }
    // Report the recorded (valid) sample count, not the raw interrupt count:
    // interrupts that produced no walkable stack are not in the folded lines.
    out.push_str(&format!("elephc-probe-samples: {valid}\n"));
    // Allocation weights, under their own prefix. Sampled attribution — the
    // delta since the previous sample is charged to the stack that sample caught,
    // exactly as Go's heap profile does — so this says WHERE allocation happens,
    // not how much precisely. `--instrument` is the mode that counts it exactly.
    for (stack, allocs) in &allocated {
        out.push_str("elephc-probe-alloc: ");
        out.push_str(&stack.join(";"));
        out.push(' ');
        out.push_str(&allocs.to_string());
        out.push('\n');
    }
    // Event counters last, and under their own prefix: these are exact, and a
    // reader must not be able to mistake one for a sampled weight.
    out.push_str(&event_report(REGION.load(Ordering::Relaxed)));
    Some(out)
}

/// Maps a program counter to the function whose `[start, next start)` range
/// holds it. The table's final sentinel bounds the last real function, so
/// runtime helpers and libc land on `<native>`.
fn symbolize<'a>(symbols: &[(u64, &'a str)], pc: u64) -> &'a str {
    let index = match symbols.binary_search_by(|(address, _)| address.cmp(&pc)) {
        Ok(index) => index,
        Err(0) => return "<native>",
        Err(insertion) => insertion - 1,
    };
    if index + 1 == symbols.len() {
        // At or past the text-end sentinel: runtime helpers, libc.
        return "<native>";
    }
    symbols[index].1
}

#[cfg(test)]
mod tests {
    /// The capability check must not consume a stream that is not its own.
    ///
    /// fd 3 is just a number: a supervisor can hand a child a connected socket
    /// there, and this check runs on every start of every monitored binary. It
    /// used to `recv` 16 bytes and *then* compare — measured, a 35-byte payload
    /// came back 19 bytes long, silently, with nothing in the program able to
    /// notice. Both ends stay open here, because closing one discards whatever
    /// is queued and would hide the very thing being measured.
    #[test]
    fn the_capability_check_leaves_a_foreign_stream_intact() {
        const PAYLOAD: &[u8] = b"HELLO-FROM-SUPERVISOR-PROTOCOL-DATA";
        unsafe {
            let mut fds = [0i32; 2];
            assert_eq!(
                libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()),
                0
            );
            let (ours, theirs) = (fds[0], fds[1]);
            assert_eq!(
                libc::send(ours, PAYLOAD.as_ptr() as *const libc::c_void, PAYLOAD.len(), 0),
                PAYLOAD.len() as isize
            );
            let saved = libc::dup(super::CONTROL_FD);
            libc::dup2(theirs, super::CONTROL_FD);

            let verdict = super::control_fd_present();

            let mut buf = [0u8; 256];
            let left = libc::recv(
                ours,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
                libc::MSG_DONTWAIT,
            );
            let left = if left < 0 { 0 } else { left as usize };

            if saved >= 0 {
                libc::dup2(saved, super::CONTROL_FD);
                libc::close(saved);
            } else {
                libc::close(super::CONTROL_FD);
            }
            libc::close(ours);
            libc::close(theirs);

            assert!(!verdict, "non-magic data must not read as a control channel");
            assert_eq!(
                left,
                PAYLOAD.len(),
                "the check consumed {} byte(s) of someone else's stream",
                PAYLOAD.len() - left
            );
        }
    }

    /// ...and it must still recognise the real thing, and consume its marker.
    #[test]
    fn the_capability_check_still_recognises_its_own_channel() {
        unsafe {
            let mut fds = [0i32; 2];
            assert_eq!(
                libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()),
                0
            );
            let (ours, theirs) = (fds[0], fds[1]);
            let magic = super::CONTROL_MAGIC;
            let trailing = b"AFTER";
            libc::send(ours, magic.as_ptr() as *const libc::c_void, magic.len(), 0);
            libc::send(ours, trailing.as_ptr() as *const libc::c_void, trailing.len(), 0);
            let saved = libc::dup(super::CONTROL_FD);
            libc::dup2(theirs, super::CONTROL_FD);

            let verdict = super::control_fd_present();

            // The marker is gone; whatever followed it is not.
            let mut buf = [0u8; 64];
            let left = libc::recv(
                super::CONTROL_FD,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
                libc::MSG_DONTWAIT,
            );
            let left = if left < 0 { 0 } else { left as usize };

            if saved >= 0 {
                libc::dup2(saved, super::CONTROL_FD);
                libc::close(saved);
            } else {
                libc::close(super::CONTROL_FD);
            }
            libc::close(ours);
            libc::close(theirs);

            assert!(verdict, "the real magic must be recognised");
            assert_eq!(&buf[..left], trailing, "the marker must be consumed, and only it");
        }
    }

    /// A client-supplied timestamp must never be able to abort the process.
    ///
    /// `i64::MIN + now` is the value that makes `now - stamp` overflow: it
    /// panicked in debug at the subtraction and in release at `.abs()`, so a
    /// single `X-Elephc-Query` header took down a `--web` service. Both extremes
    /// are checked, plus the ordinary cases, so the window itself stays correct
    /// while being total.
    #[test]
    fn a_crafted_timestamp_cannot_abort_the_window_check() {
        let now: i64 = 1_800_000_000;
        for stamp in [i64::MIN, i64::MAX, i64::MIN.wrapping_add(now), i64::MIN + 1] {
            assert!(
                !super::within_query_window(now, stamp),
                "a forged stamp must fall outside the window, not panic"
            );
        }
        // And the window still means what it says.
        assert!(super::within_query_window(now, now));
        assert!(super::within_query_window(now, now - super::QUERY_WINDOW_SECS));
        assert!(super::within_query_window(now, now + super::QUERY_WINDOW_SECS));
        assert!(!super::within_query_window(now, now - super::QUERY_WINDOW_SECS - 1));
        assert!(!super::within_query_window(now, now + super::QUERY_WINDOW_SECS + 1));
    }

    use super::*;

    /// The route tests mutate the process-global `REGION`/`CURRENT_ROUTE`, so
    /// they must not run concurrently. A poisoned lock is recovered, not
    /// propagated, so one test's panic does not cascade.
    static ROUTE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn symbolize_uses_start_ranges_and_the_end_sentinel() {
        let symbols = vec![
            (0x1000, "main"),
            (0x2000, "hot_leaf"),
            (0x3000, "<end>"),
        ];
        assert_eq!(symbolize(&symbols, 0x0500), "<native>");
        assert_eq!(symbolize(&symbols, 0x1000), "main");
        assert_eq!(symbolize(&symbols, 0x1fff), "main");
        assert_eq!(symbolize(&symbols, 0x2000), "hot_leaf");
        assert_eq!(symbolize(&symbols, 0x2fff), "hot_leaf");
        // At or past the sentinel start: runtime/libc territory.
        assert_eq!(symbolize(&symbols, 0x3000), "<native>");
        assert_eq!(symbolize(&symbols, 0x3001), "<native>");
    }

    /// Interns routes into a stand-in shared region and checks id stability,
    /// resolution, and the empty-clears-current-route contract.
    #[test]
    fn routes_intern_into_shared_memory_and_resolve() {
        let _guard = ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // A heap region standing in for the mmap: only the route table area is
        // exercised, which lives at `base + RING_BYTES`.
        let mut region = vec![0u8; REGION_BYTES];
        let base = region.as_mut_ptr() as usize;
        REGION.store(base, Ordering::Relaxed);

        unsafe {
            let a = intern_route("GET /api/orders");
            let b = intern_route("POST /checkout");
            let a_again = intern_route("GET /api/orders");
            assert_eq!(a, 1);
            assert_eq!(b, 2);
            assert_eq!(a_again, a, "the same route reuses its id");
            assert_eq!(route_name(a).as_deref(), Some("GET /api/orders"));
            assert_eq!(route_name(b).as_deref(), Some("POST /checkout"));
            assert_eq!(route_name(0), None, "id 0 is no route");

            // set_route publishes the id; empty clears it.
            let route = "GET /api/orders";
            elephc_probe_set_route(route.as_ptr(), route.len());
            assert_eq!(CURRENT_ROUTE.load(Ordering::Relaxed), a);
            elephc_probe_set_route(std::ptr::null(), 0);
            assert_eq!(CURRENT_ROUTE.load(Ordering::Relaxed), 0);
        }
        REGION.store(0, Ordering::Relaxed);
    }

    /// A route carrying folded-format metacharacters (from an untrusted HTTP
    /// path) is neutralized so it cannot forge frames or profile lines.
    #[test]
    fn route_names_are_sanitized() {
        let _guard = ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut region = vec![0u8; REGION_BYTES];
        let base = region.as_mut_ptr() as usize;
        REGION.store(base, Ordering::Relaxed);
        unsafe {
            let id = intern_route("GET /x\nfake;frame\t/y");
            let name = route_name(id).unwrap();
            assert!(!name.contains(';'), "{name}");
            assert!(!name.contains('\n'), "{name}");
            assert!(!name.contains('\t'), "{name}");
            assert_eq!(name, "GET /x?fake?frame?/y");
        }
        REGION.store(0, Ordering::Relaxed);
    }

    /// A full route table returns id 0 (untagged) rather than mis-attributing an
    /// overflow sample to an arbitrary existing route.
    /// I/O events are counted exactly, per route, and survive the untagged case.
    ///
    /// The point of the whole exercise: a driver call fires exactly one event, so
    /// these counts do not depend on sampling luck. A CLI run has no route, and
    /// dropping its events would understate the totals silently — worse than a row
    /// nobody expected — so id 0 gets its own bucket.
    /// Only a holder of the build key may turn profiling on.
    ///
    /// Without this, anyone who can set a header profiles your production: the
    /// request pays real time and the response reveals the shape of the code. The
    /// cases below are the ones an attacker actually has — no signature, a
    /// signature over a different message, a stale one captured from a log, and a
    /// truncated tag hoping for a prefix comparison.
    /// Installs `fd` as CONTROL_FD for the duration of a check, then restores.
    ///
    /// Tests share one descriptor table, so this saves and puts back whatever was
    /// on 3 — otherwise one test's socket becomes the next one's answer.
    fn with_control_fd(fd: i32, body: impl FnOnce() -> bool) -> bool {
        unsafe {
            let saved = libc::dup(CONTROL_FD);
            libc::dup2(fd, CONTROL_FD);
            let result = body();
            if saved >= 0 {
                libc::dup2(saved, CONTROL_FD);
                libc::close(saved);
            } else {
                libc::close(CONTROL_FD);
            }
            result
        }
    }

    /// Only the channel `monitor` created may turn profiling on.
    ///
    /// Every case below is a way a descriptor could end up on 3 without anyone
    /// asking for a profile. Getting this wrong does not leak data, but it makes a
    /// program start emitting profiler output for reasons its author cannot see —
    /// which is exactly the surprise `--with-monitoring` promises not to spring.
    #[test]
    fn only_the_control_channel_enables_profiling() {
        let _guard = ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            // The real thing: a socketpair carrying the marker.
            let mut pair = [0i32; 2];
            assert_eq!(libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, pair.as_mut_ptr()), 0);
            libc::send(
                pair[0],
                CONTROL_MAGIC.as_ptr() as *const libc::c_void,
                CONTROL_MAGIC.len(),
                0,
            );
            assert!(with_control_fd(pair[1], control_fd_present), "the real channel must pass");
            // And the marker is consumed, so a second read cannot re-enable later.
            assert!(!with_control_fd(pair[1], control_fd_present), "the marker is single-use");
            libc::close(pair[0]);
            libc::close(pair[1]);

            // A socket with the wrong contents — someone else's channel.
            let mut wrong = [0i32; 2];
            libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, wrong.as_mut_ptr());
            let junk = b"HELLO-WORLD-1234";
            libc::send(wrong[0], junk.as_ptr() as *const libc::c_void, junk.len(), 0);
            assert!(!with_control_fd(wrong[1], control_fd_present), "wrong marker must fail");
            libc::close(wrong[0]);
            libc::close(wrong[1]);

            // A socket with nothing in it: no marker, no profiling, and no block.
            let mut empty = [0i32; 2];
            libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, empty.as_mut_ptr());
            assert!(!with_control_fd(empty[1], control_fd_present), "empty must fail");
            libc::close(empty[0]);
            libc::close(empty[1]);

            // A PIPE on the same descriptor — inherited from a shell, say.
            let mut pipe = [0i32; 2];
            assert_eq!(libc::pipe(pipe.as_mut_ptr()), 0);
            libc::write(pipe[1], CONTROL_MAGIC.as_ptr() as *const libc::c_void, CONTROL_MAGIC.len());
            assert!(
                !with_control_fd(pipe[0], control_fd_present),
                "a pipe is not a control channel even carrying the right bytes"
            );
            libc::close(pipe[0]);
            libc::close(pipe[1]);

            // A DATAGRAM socket: right family, wrong type.
            let mut dgram = [0i32; 2];
            libc::socketpair(libc::AF_UNIX, libc::SOCK_DGRAM, 0, dgram.as_mut_ptr());
            libc::send(
                dgram[0],
                CONTROL_MAGIC.as_ptr() as *const libc::c_void,
                CONTROL_MAGIC.len(),
                0,
            );
            assert!(!with_control_fd(dgram[1], control_fd_present), "SOCK_DGRAM must fail");
            libc::close(dgram[0]);
            libc::close(dgram[1]);

            // A closed descriptor answers no rather than faulting.
            let closed = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
            libc::close(closed);
            assert!(!with_control_fd(closed, control_fd_present));
        }
    }

    #[test]
    fn only_a_signed_query_enables_profiling() {
        let key = [7u8; handshake::KEY_LEN];
        let published: Vec<u8> = key.to_vec();
        KEY_PTR.store(published.as_ptr() as usize, Ordering::Relaxed);

        let now = {
            let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
            unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut ts) };
            ts.tv_sec as i64
        };
        let sign = |stamp: i64| {
            let tag = handshake::hmac_sha256(&key, stamp.to_string().as_bytes());
            let hex: String = tag.iter().map(|b| format!("{b:02x}")).collect();
            format!("t={stamp},v={hex}")
        };
        let check = |value: &str| {
            elephc_probe_verify_query(value.as_ptr(), value.len()) == 1
        };

        assert!(check(&sign(now)), "a fresh signature must be accepted");

        // Replay: a header captured from a log stops working once it ages out.
        assert!(!check(&sign(now - QUERY_WINDOW_SECS - 60)), "stale must be refused");
        // And a clock running ahead is refused symmetrically.
        assert!(!check(&sign(now + QUERY_WINDOW_SECS + 60)));

        // Forged: right shape, wrong key.
        let wrong = handshake::hmac_sha256(&[9u8; handshake::KEY_LEN], now.to_string().as_bytes());
        let hex: String = wrong.iter().map(|b| format!("{b:02x}")).collect();
        assert!(!check(&format!("t={now},v={hex}")));

        // Truncated: must not pass a prefix comparison.
        let good = sign(now);
        assert!(!check(&good[..good.len() - 4]));
        // Malformed and empty values are refused rather than parsed loosely.
        assert!(!check("t=abc,v=zz"));
        assert!(!check(""));

        KEY_PTR.store(0, Ordering::Relaxed);
        // With no key published there is nothing to verify against, so nothing passes.
        assert!(!check(&sign(now)));
    }

    #[test]
    fn io_events_are_counted_exactly_per_route() {
        // Shares REGION and CURRENT_ROUTE with the other route tests, and cargo
        // runs tests in parallel; without this they trample each other.
        let _guard = ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut region = vec![0u8; REGION_BYTES];
        let base = region.as_mut_ptr() as usize;
        REGION.store(base, Ordering::Relaxed);

        // No route set yet: everything lands in the untagged bucket.
        CURRENT_ROUTE.store(0, Ordering::Relaxed);
        for _ in 0..551 {
            elephc_probe_note_io();
        }
        elephc_probe_note_wait(2_290_874);

        let id = unsafe { intern_route("GET /orders") };
        assert!(id > 0, "the route table must hand out a 1-based id");
        CURRENT_ROUTE.store(id, Ordering::Relaxed);
        elephc_probe_note_io();
        elephc_probe_note_io();
        elephc_probe_note_wait(1_000);

        let report = event_report(base);
        assert!(
            report.contains("elephc-probe-io: <untagged> ops=551 wait_ns=2290874"),
            "{report}"
        );
        assert!(
            report.contains("elephc-probe-io: GET /orders ops=2 wait_ns=1000"),
            "{report}"
        );
        // Routes that saw no I/O must not produce an empty row.
        assert_eq!(report.lines().count(), 2, "{report}");

        REGION.store(0, Ordering::Relaxed);
        CURRENT_ROUTE.store(0, Ordering::Relaxed);
        // With no region mapped the entry points are inert rather than unsafe.
        elephc_probe_note_io();
        elephc_probe_note_wait(1);
        assert!(event_report(0).is_empty());
    }

    #[test]
    fn route_table_overflow_returns_untagged() {
        let _guard = ROUTE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut region = vec![0u8; REGION_BYTES];
        let base = region.as_mut_ptr() as usize;
        REGION.store(base, Ordering::Relaxed);
        unsafe {
            for i in 0..MAX_ROUTES {
                assert_eq!(intern_route(&format!("route-{i}")), i + 1);
            }
            // The table is now full; a new route cannot be interned.
            assert_eq!(intern_route("one-too-many"), 0);
            // route_name never resolves an out-of-range id to a real slot.
            assert_eq!(route_name(MAX_ROUTES + 1), None);
        }
        REGION.store(0, Ordering::Relaxed);
    }
}
