//! Purpose:
//! Exact per-function instrumentation runtime for programs compiled with
//! `--instrument`. The compiler calls `elephc_instr_enter(id, allocs, frees,
//! frame)` in every PHP function's prologue and `elephc_instr_exit(id, allocs,
//! frees, frame)` in its epilogue, where `frame` is that activation's frame
//! pointer — already in a register at both sites, and what tells one activation
//! of a recursive function from another;
//! this crate maintains a shadow call stack and, from it, exact
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
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
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
///
/// Measured, because it decides whether the drop path below is reachable at all:
/// a compiled program recursing through a one-argument function dies of NATIVE
/// stack exhaustion at about 51,200 frames, with and without monitoring — below
/// this cap. So on a default 8 MB stack the process is gone before an activation
/// is ever dropped, and the code past this check is defensive rather than a road
/// real programs travel. It is not dead, though: a bigger `ulimit -s`, or frames
/// smaller than this fixture's, move that ceiling.
///
/// Dropped activations used to need reconciling, because an exit could not say
/// which activation it belonged to: they were counted, and their exits counted
/// down against — which a throw desynchronised, since it destroys them without
/// any exit arriving, so the count went on to eat the exit of a frame that WAS
/// tracked. The hooks now carry a frame pointer, so an exit for an activation
/// that was never pushed simply finds no frame and closes nothing. There is
/// nothing left to count.
const MAX_STACK: usize = 65_536;

/// How many suspended coroutines can be parked at once.
///
/// A `yield` does not return: the generator body's frame stays open while the
/// consumer runs, so its own bookkeeping has to go somewhere until the resume.
/// That somewhere is bounded for the same reason `MAX_STACK` is — an abandoned
/// generator is never resumed, and a program that builds them in a loop and
/// drops them would otherwise grow profiler state without limit.
///
/// Past the cap a suspension is REFUSED rather than parked: the frame stays on
/// the shadow stack exactly as it did before any of this existed, so the
/// measurement degrades to the old wrong one instead of to a wrong one nobody
/// has seen. The refusal is counted and reported, because a silent truncation
/// reads as a profile that covered everything.
const MAX_PARKED: usize = 4_096;

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
    /// Nanoseconds blocked inside DB driver calls, attributed to this function.
    /// Self time minus this wait is an unclassified non-DB remainder.
    incl_wait: u64,
    excl_wait: u64,
    /// Outgoing network operations, inclusive and exclusive like DB queries.
    incl_network: u64,
    excl_network: u64,
    /// Nanoseconds blocked in outgoing network work, inclusive and exclusive.
    incl_network_wait: u64,
    excl_network_wait: u64,
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
    /// This activation's frame pointer, passed by the hook that opened it.
    ///
    /// What separates one activation from another. A function id cannot: two
    /// activations of a recursive function share it, and the exit hook then has
    /// no way to say which one is returning. Live frames have distinct addresses
    /// by construction, which is the property the matching rests on.
    ///
    /// A returned frame's address IS handed back out, and this stack can hold
    /// frames that have returned — an unwind leaves them above the catcher until
    /// it exits. So a comparison against a stale frame is reachable, and the
    /// earlier claim that it never happens was wrong. Two things make it come out
    /// right, and both are tested rather than asserted: a call made afterwards is
    /// pushed ABOVE the stale frame, so a search from the top finds the live one
    /// first; and an activation dropped past the cap is pushed nowhere at all,
    /// which is why its exit is recognised by `dropped_fps` instead.
    fp: usize,
    t_enter: u64,
    a_enter: u64,
    f_enter: u64,
    io_enter: u64,
    w_enter: u64,
    /// Summed elapsed time, allocations, frees, DB queries, and DB-driver wait
    /// of this frame's direct callees.
    children_ns: u64,
    children_allocs: u64,
    children_frees: u64,
    children_io: u64,
    children_wait: u64,
    /// Network work issued directly by this activation.
    network: u64,
    network_wait: u64,
    /// Network work issued by direct and transitive callees.
    children_network: u64,
    children_network_wait: u64,
}

/// One suspended coroutine's activations, off the shadow stack until it resumes.
///
/// A generator body and a fiber body are ordinary emitted functions — they get
/// the same enter hook as anything else — but a `yield` or a `Fiber::suspend`
/// switches stacks instead of returning. Left on the shadow stack, that frame is
/// what `enter_at` reads as the caller of whatever the consumer does next, and
/// what the consumer's whole cost is charged to. Measured on a four-line
/// program: a generator whose body ran for 23 us reported 99.8% inclusive time
/// and an edge to a function it never called.
///
/// A suspension CLOSES its frames and a resume opens fresh ones, rather than
/// setting the old ones aside and rebasing their stamps on the way back. That
/// buys the whole of `close_frame` unchanged — self time, the caller's child
/// charge, the per-function inclusive span, the edge weight, the overdrawn
/// check — and it is what makes a coroutine that is never resumed report the
/// time it did run instead of nothing. What crosses the suspension is therefore
/// only the identity needed to open them again.
struct Parked {
    /// The frame pointer of the activation that suspended — how a resume finds
    /// its own group again.
    ///
    /// A coroutine stack that has been freed can be handed back out to the next
    /// one, so this is not unique over time; the newest match wins and the id is
    /// checked beside it, the same pairing `exit_at` uses.
    fp: usize,
    /// Which coroutine suspended: the runtime's `_fiber_current` at that instant,
    /// read by the emitted hook and passed through.
    ///
    /// The frame pointer answers "which activation" and this answers "which
    /// suspension", and only the second can be had from inside the runtime's own
    /// suspend helper — which is where the answer is needed, because that helper
    /// does not always return. It leaves three ways: `Fiber::suspend()` outside a
    /// fiber and a live `unserialize()` both raise before switching, and a
    /// `Fiber::throw()`/`Generator::throw()` delivered on resume raises after it.
    /// All three reach PHP handlers, and the post-call hook at the suspension site
    /// is never reached to put the frame back.
    ///
    /// The helper holds this value in a register at each of those points. A frame
    /// pointer does not survive the same way: reading the caller's from the frame
    /// chain gives the PHP function for a direct `Fiber::suspend()` and gives
    /// `__rt_gen_suspend`'s own frame for a `yield`, since generators reach the
    /// same helper one level deeper. Zero for a suspension attempted outside any
    /// coroutine, which is exactly the first of those three cases.
    coro: usize,
    /// `(id, fp)` per parked activation, the suspending one first.
    frames: Vec<(u32, usize)>,
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
    /// Frame pointers of activations dropped past `MAX_STACK`, innermost last.
    ///
    /// Their exits used to need nothing at all, on the reasoning that a dropped
    /// activation's exit "carries a frame pointer that is not on the stack and
    /// closes nothing". That holds while the shadow stack describes LIVE frames,
    /// and an unwind breaks it: the frames an exception destroyed stay here
    /// until the catcher exits, while the native stack they occupied is already
    /// free. A call the handler makes is pushed onto that reclaimed space, so
    /// its frame pointer does not merely risk colliding with a dead frame's — at
    /// a fixed frame size it IS that address. The exit then matched the stale
    /// frame and closed every frame above it, comparing ids only afterwards.
    ///
    /// Identities and not a count, which is what the previous design used and
    /// what a throw could desynchronise: a throw destroys these activations
    /// without their exits ever arriving, so it clears the list outright rather
    /// than leaving a number to be counted down against exits belonging to
    /// somebody else.
    dropped_fps: Vec<usize>,
    /// Coroutines suspended right now, one entry per `yield` / `Fiber::suspend`
    /// that has not been resumed. See `Parked`.
    parked: Vec<Parked>,
    /// Suspensions refused at `MAX_PARKED` (reported: a refused park leaves the
    /// old misattribution in place, which is a wrong row, not a missing one).
    parks_refused: u64,
    /// Frames whose children outran their own span — impossible when the
    /// accounting is right, so it is reported rather than absorbed.
    overdrawn: u64,
    /// Throws that had to share a record with an older one at the nesting cap.
    throws_merged: u64,
    /// The exception in flight, if one is.
    unwinding: Option<Unwind>,
    /// Per-call spans `(id, enter_ns, exit_ns)` recorded only when tracing is on
    /// (`ELEPHC_INSTR_TRACE`), bounded by `TRACE_CAP`. Written as a Chrome trace.
    trace: Vec<(u32, u64, u64)>,
    /// Calls not recorded because the trace buffer was full.
    trace_dropped: u64,
}

/// An exception that is still unwinding.
///
/// The runtime jumps to the catcher without telling the profiler which frame it
/// landed in, and the frames the exception destroyed never run their exits. Both
/// facts are only resolved at the catcher's own exit — the first exit whose
/// depth is back at or below the throw's. That is why what the handler spends in
/// the meantime has to wait here rather than be charged as it happens: the frame
/// it would be charged to is the one the exception just destroyed.
/// One throw, and where every counter stood when it happened.
///
/// All six, because a frame closed at the throw's clock but the catcher's
/// allocation counter is charged for every object the handler allocated.
#[derive(Default)]
struct Throw {
    /// Stack depth at the throw. The catcher is somewhere below it; every frame
    /// above the catcher is already dead but still on the stack.
    depth: usize,
    t: u64,
    a: u64,
    f: u64,
    io: u64,
    w: u64,
    /// What this throw's handler has spent, charged to no one yet.
    ///
    /// Per throw rather than per unwind because each has its own catcher: an
    /// exception raised and caught inside a call the outer handler made is
    /// resolved by an exit that is nowhere near the outer catcher, and pouring
    /// both handlers' work into one bucket handed it all to whichever exit came
    /// first.
    children_ns: u64,
    children_allocs: u64,
    children_frees: u64,
    children_io: u64,
    children_wait: u64,
    /// Network work performed directly by the handler before its catcher is known.
    network: u64,
    network_wait: u64,
    /// Network work performed by functions called from the handler.
    children_network: u64,
    children_network_wait: u64,
    /// `(callee, calls, summed inclusive ns)` for what this handler called.
    edges: Vec<(u32, u64, u64)>,
}

/// How many nested throws are recorded before the oldest ones have to serve for
/// the newest too. A handler that throws, whose handler throws, thirty-two deep,
/// is past the point where one more instant improves the answer.
const MAX_NESTED_THROWS: usize = 32;

#[derive(Default)]
struct Unwind {
    /// Every throw still unresolved, oldest first. More than one exists whenever
    /// a handler throws — or calls something that throws and catches — before
    /// the first exception reached its own catcher.
    throws: Vec<Throw>,
}

impl Unwind {
    /// Where every counter stood at the throw that killed the frame at `index`.
    ///
    /// A frame was on the stack at every throw deeper than its own index, and it
    /// died at the first of them; later throws found it already dead. The one
    /// shape this reads wrong is the frame that CAUGHT an earlier throw and then
    /// died to a later one: nothing on the wire distinguishes it from the corpse
    /// beside it, so it is closed at the earlier instant too, and the span
    /// between lands on its caller — an ancestor, rather than on a frame that
    /// had already stopped running.
    fn killer_of(&self, index: usize) -> Option<(u64, u64, u64, u64, u64)> {
        self.throws
            .iter()
            .find(|throw| throw.depth > index)
            .map(|k| (k.t, k.a, k.f, k.io, k.w))
    }
}

impl State {
    /// Marks the start of an unwind.
    fn note_throw(&mut self, t: u64, a: u64, f: u64, io: u64, w: u64) {
        // A throw inside a catch handler replaces the unwind, but must not drop
        // what the previous one was still holding: that cost has already been
        // taken out of its frames, so losing it here would stop the exclusives
        // from partitioning the root. It rides along to the next catcher, which
        // is where a rethrow puts it anyway.
        let depth = self.stack.len();
        // Nothing on the stack, so there is no frame this throw destroyed and no
        // catcher below it: a record at depth zero can never be resolved, since
        // resolution asks for a throw deeper than the exiting frame's index and
        // every index is at least zero. Thirty-two of them filled the table and
        // the next real throw recycled a live record.
        //
        // Every activation dropped past the cap that this unwind REACHES is
        // destroyed by it, and not one of their exits will arrive. Not every one
        // of them: a catcher can itself be past the cap, and the dropped
        // activations between the cap and it survive. Clearing those too costs
        // them their exit bookkeeping, which they never had — they were already
        // untracked — and their addresses lie below every tracked frame, so they
        // cannot be mistaken for one.
        // Their frame pointers are cleared for that reason and not as
        // housekeeping: left behind, they would be matched by the NEXT dropped
        // call to reuse the same address, and its exit would be swallowed as if
        // it belonged to an activation that no longer exists.
        //
        // Before the depth guard, not after. Nothing can be live above an empty
        // stack, so an identity still recorded there is stale by definition —
        // and returning early left exactly that behind.
        self.dropped_fps = Vec::new();
        if depth == 0 {
            return;
        }
        let unwind = self.unwinding.get_or_insert_with(Unwind::default);
        // A throw raised while another is in flight joins it rather than
        // replacing it: the frames the first one killed died then, and only its
        // own instants say when.
        if unwind.throws.len() < MAX_NESTED_THROWS {
            unwind.throws.push(Throw { depth, t, a, f, io, w, ..Throw::default() });
        } else if let Some(last) = unwind.throws.last_mut() {
            // Past the cap the newest throw takes over the last record and keeps
            // what it had accumulated. Dropping that charge instead would stop
            // the exclusives adding up to the root, which is the property the
            // whole table rests on; keeping it merges two handlers' call edges
            // onto one caller and loses the older throw's instant. Both are
            // wrong, one silently — so the trade is made and then reported.
            self.throws_merged = self.throws_merged.saturating_add(1);
            last.depth = depth;
            last.t = t;
            last.a = a;
            last.f = f;
            last.io = io;
            last.w = w;
        }
    }

    /// The unwind holding the charge for work at the current depth, if the
    /// frame below is one an exception already destroyed.
    ///
    /// Only the boundary depth is ambiguous. Deeper than the throw, the frame
    /// below is one the handler opened itself and is a real caller; at the
    /// throw's own depth it is either the catcher or a corpse, and nothing
    /// distinguishes them until the catcher returns.
    fn pending_charge(&mut self) -> Option<&mut Throw> {
        let at = self.stack.len();
        if at == 0 {
            return None;
        }
        // Any throw at this depth will do: throws at equal depths are resolved
        // by the same exit and hand their charge to the same frame.
        self.unwinding
            .as_mut()?
            .throws
            .iter_mut()
            .find(|throw| throw.depth == at)
    }

    /// Grows the per-function accumulator vector so `id` indexes into it.
    ///
    /// Ids are dense and assigned at compile time, so a vector indexed by id
    /// beats a map on the hot path; this is the one place that pays for it.
    fn ensure(&mut self, id: u32) {
        let idx = id as usize;
        if idx >= self.fns.len() {
            self.fns.resize(idx + 1, FnAcc::default());
        }
    }

    /// Records entry to `id` with the timestamp `t`, allocation counter `a`,
    /// free counter `f`, io counter `io`, and io-wait nanoseconds `w` sampled
    /// at the call site.
    fn enter_at(&mut self, id: u32, fp: usize, t: u64, a: u64, f: u64, io: u64, w: u64) {
        self.ensure(id);
        // A call made while an exception is still unwinding sits on top of the
        // frames it destroyed, so the frame below is not the caller — the
        // catcher is, and it has no name until it exits.
        match self.pending_charge() {
            Some(u) => match u.edges.iter_mut().find(|(c, ..)| *c == id) {
                Some(edge) => edge.1 += 1,
                None => u.edges.push((id, 1, 0)),
            },
            None => {
                if let Some(pid) = self.stack.last().map(|f| f.id) {
                    self.edges.entry((pid, id)).or_insert((0, 0)).0 += 1;
                }
            }
        }
        // Past the cap this activation cannot be timed, and must not pretend to
        // be: raising its depth would leave the function permanently "active",
        // so its inclusive time is never credited. The call still counts — it did
        // happen — but nothing else does.
        //
        // Its frame pointer is recorded because its exit has to be recognised
        // rather than merely unrecognised. This used to say the exit "will arrive
        // carrying a frame pointer that is not on the stack and close nothing",
        // which is true of a stack of LIVE frames and false after an unwind —
        // see `dropped_fps`, and the two tests that hold it.
        if self.stack.len() >= MAX_STACK {
            self.dropped += 1;
            // Recorded only while an unwind is in flight, and bounded even then.
            //
            // An identity is needed for exactly one reason: the shadow stack can
            // hold frames that have RETURNED, whose addresses the native stack has
            // already handed back out. That is true between a throw and its
            // catcher exiting, and at no other time — outside it every frame here
            // is live, so "this pointer is not on the stack, close nothing" is
            // sound on its own, which is what it was before any of this.
            //
            // So the common case allocates nothing at all. Recording
            // unconditionally made this a second shadow stack: `MAX_STACK` exists
            // to stop a runaway recursion growing profiler state without bound,
            // and a word per activation put that cost straight back, with the
            // vector keeping its peak capacity for the life of the thread.
            //
            // The cap still stands for the pathological case. Past it an identity
            // is not recorded and such an exit falls back to finding no frame of
            // its own — right unless an unwind left a stale frame at that address,
            // which needs twice MAX_STACK live frames on a stack that survives
            // them.
            if self.unwinding.is_some() && self.dropped_fps.len() < MAX_STACK {
                self.dropped_fps.push(fp);
            }
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
            fp,
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
            network: 0,
            network_wait: 0,
            children_network: 0,
            children_network_wait: 0,
        });
    }

    /// Closes a suspending coroutine's activations until it resumes.
    ///
    /// A `yield` and a `Fiber::suspend` switch stacks; they do not return. The
    /// body's frame therefore stays open across everything the consumer does
    /// next, and two things go wrong at once: `enter_at` reads that frame as the
    /// caller of the consumer's next call, and the frame's own span keeps
    /// running. Both were measured on a four-line program — a generator whose
    /// body ran 23 us reported 99.8% inclusive time and an edge it never called.
    ///
    /// Closed rather than set aside, because a suspension really is the end of a
    /// span: what the coroutine did up to here is finished, belongs to the
    /// caller that drove it here, and is not going to change. Saying so with
    /// `close_frame` is also the only way the coroutine that is never resumed —
    /// an abandoned generator — reports the time it did run.
    ///
    /// What is closed is the suspending frame and anything above it, which is
    /// normally nothing at all: a suspension is reached by a call, so whatever
    /// the body called has already returned. A body that suspends from INSIDE a
    /// nested call closes only the inner frame and leaves the outer one standing;
    /// that case is no better than before this existed and no worse, and the
    /// test named for it says so rather than a comment claiming otherwise.
    fn suspend_at(&mut self, id: u32, fp: usize, coro: usize, t: u64, a: u64, f: u64, io: u64, w: u64) {
        // Paired with the id, the way `exit_at` pairs them: a freed coroutine
        // stack is handed back out, so a frame pointer alone names an activation
        // only among the live ones.
        let Some(index) = self.stack.iter().rposition(|frame| frame.fp == fp) else {
            return;
        };
        if self.stack[index].id != id {
            return;
        }
        // A group already standing under this coroutine belongs to a DEAD one.
        //
        // `coro` is the running fiber's address, so two live coroutines never
        // share it and one coroutine never has two suspensions in flight. A
        // match therefore means the fiber that parked it was freed and its
        // address handed to this one, and nothing will ever resume those frames
        // — they were closed when they parked, and their own resume is keyed on
        // a frame pointer their fiber no longer owns.
        //
        // Dropped BEFORE the cap check, which is the case it exists for. A park
        // refused at `MAX_PARKED` leaves this activation on the stack and parks
        // nothing, and the runtime unpark that a non-returning suspend path
        // fires would then find the older occupant and push it ABOVE the live
        // activation — the `(id, fp)` guard in `restore` does not catch it,
        // because a different function's ids do not match.
        self.parked.retain(|group| group.coro != coro);
        if self.parked.len() >= MAX_PARKED {
            self.parks_refused += 1;
            return;
        }
        // Anything ABOVE the suspending frame is not part of this coroutine: a
        // suspension is reached by a call, so whatever the body called has
        // already returned. What can be up there is what an unwind leaves —
        // frames an exception destroyed, which stay until the catcher exits. A
        // catch handler that yields therefore finds them in its way.
        //
        // They are closed the way an exit closes them, at the instant of the
        // throw that killed each one. Parking them instead closed dead frames at
        // the suspension's timestamp and the resume then reopened them as LIVE
        // callees, charging everything after through activations that no longer
        // existed.
        self.close_frames_above(index, t, a, f, io, w);
        let frame = self.stack.pop().expect("the index is inside the stack");
        let frames = vec![(frame.id, frame.fp)];
        // Traced like an exit, because it ends a span exactly as an exit does.
        // Recording it only in `exit_at` left every pre-yield segment out of the
        // Chrome/Perfetto timeline, and an abandoned generator with no span at
        // all while the aggregate table accounted for it.
        self.trace_span(frame.id, frame.t_enter, t);
        self.close_frame(frame, t, a, f, io, w);
        self.parked.push(Parked { fp, coro, frames });
    }

    /// Opens the activations of the coroutine `coro` again, from inside the
    /// runtime's suspend helper, on a path that will not return to the
    /// suspension site.
    ///
    /// Three of those paths exist and every one of them reaches PHP handlers: a
    /// `Fiber::suspend()` outside a fiber and a live `unserialize()` raise
    /// `FiberError` before the stack switch, and a pending
    /// `Fiber::throw()`/`Generator::throw()` raises after it. The post-call hook
    /// is skipped in all three, so without this the activation stayed parked
    /// while its own `catch` ran: the handler's work was charged to whatever
    /// frame was below, and the function's own exit later found no frame and
    /// closed nothing.
    ///
    /// Keyed by the coroutine rather than by a frame pointer because that is what
    /// the helper has. A coroutine has one suspension in flight at a time, so the
    /// key is exact; `rposition` still takes the newest, which is that one.
    fn unpark_at(&mut self, coro: usize, t: u64, a: u64, f: u64, io: u64, w: u64) {
        let Some(at) = self.parked.iter().rposition(|group| group.coro == coro) else {
            return;
        };
        let Some(&(id, fp)) = self.parked[at].frames.first() else {
            return;
        };
        self.restore(at, id, fp, t, a, f, io, w);
    }

    /// Opens a resumed coroutine's activations again, at this instant.
    ///
    /// Fresh frames, not the closed ones brought back: the span that ended at
    /// the suspension is accounted for, and this is a new one. `calls` is not
    /// touched and no edge is recorded — a resume is the same activation the
    /// enter already counted, arriving through the same caller.
    fn resume_at(&mut self, id: u32, fp: usize, t: u64, a: u64, f: u64, io: u64, w: u64) {
        let Some(at) = self.parked.iter().rposition(|group| group.fp == fp) else {
            return;
        };
        // The suspending frame is the group's first, so this is the same
        // (pointer, id) pairing the park was keyed on. A coroutine stack reused
        // by a different function fails it and resumes nothing rather than
        // handing one coroutine's activations to another.
        if self.parked[at].frames.first().is_none_or(|(first, _)| *first != id) {
            return;
        }
        self.restore(at, id, fp, t, a, f, io, w);
    }

    /// Puts the parked group at `at` back on the stack, at this instant.
    ///
    /// Shared by the two ways a coroutine can come back: its own resume, and the
    /// runtime unparking it on a path that will not reach that resume.
    fn restore(&mut self, at: usize, id: u32, fp: usize, t: u64, a: u64, f: u64, io: u64, w: u64) {
        // A park refused at `MAX_PARKED` left this activation ON the stack, and
        // an older abandoned coroutine of the same function can own a parked
        // group at a coroutine-stack address that has since been handed back
        // out. Restoring then pushes a second, duplicate activation on top of
        // the live one. The pair being live already is the whole signal: a
        // suspension took its frame off, so a resume can never find it there.
        if self
            .stack
            .iter()
            .any(|frame| frame.fp == fp && frame.id == id)
        {
            return;
        }
        let group = self.parked.remove(at);
        // Restoring past the cap would push frames the stack has no room for and
        // silently lose whichever fell off. Refusing outright loses the same
        // frames, but their exits then find nothing and close nothing, which is
        // the behaviour an activation dropped at the cap already has.
        if self.stack.len() + group.frames.len() > MAX_STACK {
            self.dropped += group.frames.len() as u64;
            return;
        }
        // The group is stored suspending-frame first, which for a coroutine IS
        // outermost first — so it is walked forwards. Reversing it put the
        // callee underneath its own caller, and every depth-ordered thing after
        // that read the stack upside down.
        for (frame_id, frame_fp) in group.frames {
            let acc = &mut self.fns[frame_id as usize];
            if acc.depth == 0 {
                acc.t_outer = t;
                acc.a_outer = a;
                acc.f_outer = f;
                acc.io_outer = io;
                acc.w_outer = w;
            }
            acc.depth += 1;
            self.stack.push(Frame {
                id: frame_id,
                fp: frame_fp,
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
                network: 0,
                network_wait: 0,
                children_network: 0,
                children_network_wait: 0,
            });
        }
    }

    /// Records exit from `id` with timestamp `t`, allocation counter `a`, free
    /// counter `f`, io counter `io`, and io-wait nanoseconds `w`, resyncing past
    /// any frames left by exception unwinding.
    fn exit_at(&mut self, id: u32, fp: usize, t: u64, a: u64, f: u64, io: u64, w: u64) {
        // Which activation is ending, and whether it was ever tracked at all.
        //
        // An exit for a frame that is not on the stack has nothing to close: it
        // was dropped past the shadow-stack cap, or destroyed by an exception
        // that has since been caught. Both used to be indistinguishable from an
        // exit for a tracked frame — the first was reconciled by counting
        // dropped activations down, which a throw could desynchronise, and the
        // second resolved to whichever frame carried the same id.
        //
        // A dropped activation is recognised by its OWN frame pointer, before the
        // stack is searched for one. Recognising it by absence — no frame here
        // carries this pointer — is what an unwind invalidates, because the dead
        // frames it leaves behind carry pointers the native stack has already
        // handed back out.
        if !self.dropped_fps.is_empty() {
            // Matched by MEMBERSHIP, not by position. Checking only the last
            // entry assumes dropped activations exit in the order they were
            // entered — true of ordinary calls, and an assumption about the
            // whole language rather than about this function. Where it fails the
            // entry left behind is not dead weight: a later TRACKED frame
            // reusing that address matches it, returns early, and is never
            // popped — a ghost frame misattributing everything after it.
            //
            // The scan runs only past `MAX_STACK`, where this list is non-empty
            // at all, and the last entry is still tried first because that is
            // what the ordinary case gives.
            if self.dropped_fps.last() == Some(&fp) {
                self.dropped_fps.pop();
                return;
            }
            if let Some(index) = self.dropped_fps.iter().rposition(|entry| *entry == fp) {
                self.dropped_fps.remove(index);
                return;
            }
        }
        let Some(index) = (match self.stack.last() {
            // The overwhelmingly common case, and the reason the search below
            // costs nothing in a program without exceptions.
            Some(top) if top.fp == fp => Some(self.stack.len() - 1),
            _ => self.stack.iter().rposition(|frame| frame.fp == fp),
        }) else {
            return;
        };

        // The id is compared BEFORE anything is closed. It used to be checked
        // after the resync loop and the pop, so an exit whose frame pointer
        // aliased a dead frame destroyed every frame above it and only then
        // noticed the names did not match — a late abort rather than a guard,
        // and the ordering `Frame::fp` names as the original defect. Reaching it
        // needs an exit belonging to neither the dropped list nor the stack, so
        // it is out of reach in practice; the check costs one comparison and
        // stops being a matter of reach.
        if self.stack[index].id != id {
            return;
        }

        // Where this exit's frame sits, which is what says which throws it ends.
        //
        // A throw is caught below the depth it was raised at, so an exit at index
        // i ends every throw deeper than i and none of the others. Testing the
        // whole unwind instead — "is the stack back where the throw left it" —
        // was wrong twice over: a handler that calls anything returns from that
        // call first and ended the unwind early, and an exception raised AND
        // caught inside such a call ended the outer one from the wrong place
        // entirely.
        //
        // Frames an exception unwound never ran their own exit hook. Closing
        // them HERE, at the instant the throw is observed passing them, keeps
        // their cost on them; simply discarding them left it inside the
        // catching function's own time, which then reads as the hot function
        // (measured: a catcher showing 99.7% self time for work it never did).
        self.close_frames_above(index, t, a, f, io, w);
        let Some(mut frame) = self.stack.pop() else {
            return;
        };
        if frame.id != id {
            return;
        }
        // This frame caught every throw raised above it, so it is the caller
        // their handlers' work has been waiting for. Throws raised BELOW it — an
        // outer exception this one was unwound by — stay open for their own
        // catcher further down.
        //
        // Taken only once there is a frame to hand them to: draining before the
        // pop dropped the charge whenever the resync had emptied the stack, and
        // a dropped charge is one the exclusives no longer account for anywhere.
        if let Some(u) = self.unwinding.as_mut() {
            let mut resolved = Vec::new();
            u.throws.retain_mut(|throw| {
                if throw.depth > index {
                    resolved.push(std::mem::take(throw));
                    false
                } else {
                    true
                }
            });
            let empty = u.throws.is_empty();
            for throw in resolved {
                frame.children_ns = frame.children_ns.wrapping_add(throw.children_ns);
                frame.children_allocs =
                    frame.children_allocs.wrapping_add(throw.children_allocs);
                frame.children_frees = frame.children_frees.wrapping_add(throw.children_frees);
                frame.children_io = frame.children_io.wrapping_add(throw.children_io);
                frame.children_wait = frame.children_wait.wrapping_add(throw.children_wait);
                frame.network = frame.network.wrapping_add(throw.network);
                frame.network_wait = frame.network_wait.wrapping_add(throw.network_wait);
                frame.children_network = frame
                    .children_network
                    .wrapping_add(throw.children_network);
                frame.children_network_wait = frame
                    .children_network_wait
                    .wrapping_add(throw.children_network_wait);
                for (callee, calls, ns) in throw.edges {
                    let entry = self.edges.entry((id, callee)).or_insert((0, 0));
                    entry.0 = entry.0.wrapping_add(calls);
                    entry.1 = entry.1.wrapping_add(ns);
                }
            }
            if empty {
                self.unwinding = None;
                // The stale frames this unwind left are gone with it, so nothing
                // recorded against it can still be needed — and a record that
                // outlives its reason is the one thing this list must not hold.
                // Its capacity goes too: keeping the peak of a pathological
                // recursion for the life of the thread is the cost the cap was
                // added to avoid.
                self.dropped_fps = Vec::new();
            }
        }
        self.trace_span(id, frame.t_enter, t);
        self.close_frame(frame, t, a, f, io, w);
    }

    /// Records one span on the opt-in timeline, if tracing is on and there is
    /// room.
    ///
    /// Every event that ENDS a span goes through here. There are two — an exit
    /// and a suspension — and only the exit used to record, so the timeline was
    /// missing exactly the segments the aggregate table did account for: every
    /// stretch a generator ran before a `yield`, and an abandoned generator
    /// entirely.
    fn trace_span(&mut self, id: u32, from: u64, to: u64) {
        if !TRACE_ON.load(Ordering::Relaxed) {
            return;
        }
        if self.trace.len() < TRACE_CAP.load(Ordering::Relaxed) {
            self.trace.push((id, from, to));
        } else {
            self.trace_dropped += 1;
        }
    }

    /// Closes every frame above `index`, at the instant of the throw that killed
    /// each one.
    ///
    /// Frames an exception unwound never ran their own exit hook, and they stay
    /// on this stack until the catcher exits — deliberately, because closing
    /// them at the throw is what keeps their cost on them rather than inside the
    /// catching function's own time (measured: a catcher showing 99.7% self time
    /// for work it never did). Which throw killed a given frame depends on how
    /// deep it sat, so it is looked up per frame rather than shared.
    ///
    /// Shared by the two events that find such frames in their way: an exit, and
    /// a suspension by a catch handler that yields. Written once because the two
    /// must agree — a suspension that instead PARKED this debris would reopen
    /// dead activations as live callees on the resume, and charge everything
    /// after them through frames that no longer exist.
    fn close_frames_above(&mut self, index: usize, t: u64, a: u64, f: u64, io: u64, w: u64) {
        while self.stack.len() > index + 1 {
            let stale = self.stack.pop().expect("the index is inside the stack");
            let killer = self
                .unwinding
                .as_ref()
                .and_then(|u| u.killer_of(self.stack.len()));
            match killer {
                // A frame cannot be closed before it was entered, which is what
                // an older throw's instant would do to one the handler opened
                // after it. Its own entry reports it as instantaneous, which
                // understates it, where the subtraction would have wrapped.
                //
                // All five clamped, not just the clock. `close_frame` subtracts
                // each of them from what the frame entered with, and every one of
                // those subtractions wraps the same way — so guarding time alone
                // left four counters able to produce ~1.8e19 from the very case
                // the guard exists for. Either that case is unreachable and the
                // guard describes a phantom, or it is reachable and the counters
                // wrap; the asymmetry is what could not be right either way.
                Some((kt, ka, kf, kio, kw)) => {
                    let at = kt.max(stale.t_enter);
                    let allocs = ka.max(stale.a_enter);
                    let frees = kf.max(stale.f_enter);
                    let io_ops = kio.max(stale.io_enter);
                    let wait = kw.max(stale.w_enter);
                    self.close_frame(stale, at, allocs, frees, io_ops, wait)
                }
                None => self.close_frame(stale, t, a, f, io, w),
            }
        }
    }

    /// Closes the current function and every tracked ancestor at process exit.
    ///
    /// Language exits and uncaught generated errors do not return through
    /// generated epilogues, so without this path their live frames never receive
    /// an exit hook and no complete exact graph can be published. The current id
    /// is closed first so the ordinary exception-resynchronization path can
    /// identify a catch frame before the remaining ancestors are drained at the
    /// same final instant.
    /// Its frame pointer is paired with the function id so recursive or
    /// concurrently live activations cannot be confused.
    fn terminate_at(
        &mut self,
        current: Option<(u32, usize)>,
        t: u64,
        a: u64,
        f: u64,
        io: u64,
        w: u64,
    ) {
        // Let an activation dropped past the cap consume only its own recorded
        // frame pointer. Clearing these identities first would let a reused
        // address match an unwind-dead tracked frame instead.
        if let Some((id, fp)) = current {
            self.exit_at(id, fp, t, a, f, io, w);
        }
        // No dropped activation will return after process termination, and its
        // identity must not consume a tracked ancestor's synthetic exit.
        self.dropped_fps = Vec::new();
        while let Some((id, fp)) = self.stack.last().map(|frame| (frame.id, frame.fp)) {
            self.exit_at(id, fp, t, a, f, io, w);
        }
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
        let inclusive_network = frame.network.wrapping_add(frame.children_network);
        let inclusive_network_wait = frame
            .network_wait
            .wrapping_add(frame.children_network_wait);
        // Children can only exceed their parent's own span if the accounting
        // has gone wrong somewhere, and no sequence found so far reaches this —
        // the two that did were both fixed at their cause. It stays because of
        // what the alternative costs: wrapping turned it into roughly 1.8e19 ns
        // of self time, a number large enough to drown every real row in the
        // profile it appears in. Saturating costs one wrong row instead, and the
        // counter makes it something the report admits to rather than something
        // a reader has to notice.
        if frame.children_ns > elapsed_ns {
            self.overdrawn = self.overdrawn.saturating_add(1);
        }
        let acc = &mut self.fns[id as usize];
        acc.excl_ns = acc.excl_ns.wrapping_add(elapsed_ns.saturating_sub(frame.children_ns));
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
        acc.excl_network = acc.excl_network.wrapping_add(frame.network);
        acc.excl_network_wait = acc.excl_network_wait.wrapping_add(frame.network_wait);
        acc.depth = acc.depth.saturating_sub(1);
        if acc.depth == 0 {
            acc.incl_ns = acc.incl_ns.wrapping_add(t.wrapping_sub(acc.t_outer));
            acc.incl_allocs = acc.incl_allocs.wrapping_add(a.wrapping_sub(acc.a_outer));
            acc.incl_frees = acc.incl_frees.wrapping_add(f.wrapping_sub(acc.f_outer));
            acc.incl_io = acc.incl_io.wrapping_add(io.wrapping_sub(acc.io_outer));
            acc.incl_wait = acc.incl_wait.wrapping_add(w.wrapping_sub(acc.w_outer));
            acc.incl_network = acc.incl_network.wrapping_add(inclusive_network);
            acc.incl_network_wait = acc
                .incl_network_wait
                .wrapping_add(inclusive_network_wait);
        }
        // Charging this to the frame below would hand the handler's cost to a
        // function the exception had already stopped. It waits with the unwind
        // instead, and reaches the catcher when the catcher is known.
        if let Some(u) = self.pending_charge() {
            u.children_ns = u.children_ns.wrapping_add(elapsed_ns);
            u.children_allocs = u.children_allocs.wrapping_add(elapsed_allocs);
            u.children_frees = u.children_frees.wrapping_add(elapsed_frees);
            u.children_io = u.children_io.wrapping_add(elapsed_io);
            u.children_wait = u.children_wait.wrapping_add(elapsed_wait);
            u.children_network = u.children_network.wrapping_add(inclusive_network);
            u.children_network_wait = u
                .children_network_wait
                .wrapping_add(inclusive_network_wait);
            match u.edges.iter_mut().find(|(c, ..)| *c == id) {
                Some(edge) => edge.2 = edge.2.wrapping_add(elapsed_ns),
                None => u.edges.push((id, 0, elapsed_ns)),
            }
            return;
        }
        let parent = self.stack.last().map(|f| f.id);
        if let Some(top) = self.stack.last_mut() {
            top.children_ns = top.children_ns.wrapping_add(elapsed_ns);
            top.children_allocs = top.children_allocs.wrapping_add(elapsed_allocs);
            top.children_frees = top.children_frees.wrapping_add(elapsed_frees);
            top.children_io = top.children_io.wrapping_add(elapsed_io);
            top.children_wait = top.children_wait.wrapping_add(elapsed_wait);
            top.children_network = top.children_network.wrapping_add(inclusive_network);
            top.children_network_wait = top
                .children_network_wait
                .wrapping_add(inclusive_network_wait);
        }
        if let Some(pid) = parent {
            let entry = self.edges.entry((pid, id)).or_insert((0, 0));
            entry.1 = entry.1.wrapping_add(elapsed_ns);
        }
    }

    /// Attributes one outgoing network operation to the active PHP activation.
    fn note_network(&mut self) {
        if let Some(unwind) = self.pending_charge() {
            unwind.network = unwind.network.wrapping_add(1);
        } else if let Some(frame) = self.stack.last_mut() {
            frame.network = frame.network.wrapping_add(1);
        }
    }

    /// Attributes blocked outgoing-network time to the active PHP activation.
    fn note_network_wait(&mut self, ns: u64) {
        if let Some(unwind) = self.pending_charge() {
            unwind.network_wait = unwind.network_wait.wrapping_add(ns);
        } else if let Some(frame) = self.stack.last_mut() {
            frame.network_wait = frame.network_wait.wrapping_add(ns);
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
        // The stack these belonged to has just been cleared, so nothing they
        // could be matched against remains.
        self.dropped_fps.clear();
        // A coroutine suspended across a slice boundary would resume into a
        // stack that no longer holds its consumer, and its accrued span belongs
        // to the request that opened it. Released rather than cleared, so an
        // abandoned generator's peak does not outlive the slice that made it.
        self.parked = Vec::new();
        self.parks_refused = 0;
        self.overdrawn = 0;
        self.throws_merged = 0;
        // The stack goes with the slice, so the frame this unwind was holding a
        // charge for goes with it too. Carrying it across would hand the next
        // slice's first catcher the previous one's handler work.
        self.unwinding = None;
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
        if self.throws_merged > 0 {
            out.push_str(&format!(
                "elephc-instr: note: {} throw(s) past {} nested unwinds shared a record \
                 with an older one; their handlers' calls are merged onto one caller\n",
                self.throws_merged, MAX_NESTED_THROWS
            ));
        }
        if !self.stack.is_empty() {
            out.push_str(&format!(
                "elephc-instr: note: {} frame(s) were still open at this dump; they \
                 carry no inclusive time and the self values do not sum to the root\n",
                self.stack.len()
            ));
        }
        if self.overdrawn > 0 {
            out.push_str(&format!(
                "elephc-instr: note: {} frames were charged more by their callees than \
                 they ran for; their self time reads as zero\n",
                self.overdrawn
            ));
        }
        if self.parks_refused > 0 {
            out.push_str(&format!(
                "elephc-instr: note: {} suspension(s) past {} parked coroutines were not \
                 parked; their consumers' time is charged to them\n",
                self.parks_refused, MAX_PARKED
            ));
        }
        if tick_rate().is_none() {
            // Before the rows, like the note above, so a reader who stops at the
            // first interesting line still learns the columns are not what they
            // are called.
            // Named exactly: `incl_wait` and `excl_wait` come from a
             // nanosecond counter and never pass through the conversion, so
             // calling them ticks would be the same kind of wrong this note is
             // here to prevent.
            out.push_str(
                "elephc-instr: note: this run was too short to measure the counter rate — \
                 incl_ns and excl_ns below are raw counter ticks, not nanoseconds \
                 (the wait columns are unaffected)\n",
            );
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
                "elephc-instr: {} calls={} incl_ns={} excl_ns={} incl_allocs={} excl_allocs={} incl_io={} excl_io={} incl_ret={} excl_ret={} incl_wait={} excl_wait={} incl_network={} excl_network={} incl_network_wait={} excl_network_wait={}\n",
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
                acc.excl_wait,
                acc.incl_network,
                acc.excl_network,
                acc.incl_network_wait,
                acc.excl_network_wait,
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
    // Dormant means dormant. The slot is filled at init, so without this a
    // binary nobody asked still counted every query it ran.
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    IO_OPS.fetch_add(1, Ordering::Relaxed);
}

/// Global nanoseconds spent blocked inside database-driver calls. Bumped
/// by `elephc_instr_wait` from the bridge, which times the actual driver call;
/// snapshotted per function at enter/exit like the other counters, so recorded
/// DB wait is separated from the rest of each function's wall time.
static WAIT_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Records `ns` nanoseconds spent inside a database-driver call. Reached from bridge builtins
/// through the runtime `_elephc_instr_wait_fn` pointer slot, null unless
/// `--instrument` linked and initialized this crate.
#[no_mangle]
pub extern "C" fn elephc_instr_wait(ns: u64) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    WAIT_NS.fetch_add(ns, Ordering::Relaxed);
}

/// Records one outgoing network operation against the active function.
#[no_mangle]
pub extern "C" fn elephc_instr_network() {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    STATE.with(|state| state.borrow_mut().note_network());
}

/// Records blocked outgoing-network nanoseconds against the active function.
#[no_mangle]
pub extern "C" fn elephc_instr_network_wait(ns: u64) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    STATE.with(|state| state.borrow_mut().note_network_wait(ns));
}

/// How many distinct statement shapes one slice may record.
///
/// The list was unbounded, and it is keyed by NORMALIZED text — literals
/// collapsed to `?` — so a well-behaved program converges on a small set and a
/// pathological one (statements built by string concatenation) does not. On a
/// long-lived `--web` worker that is a leak, and the linear scan below turns
/// quadratic. A thousand distinct shapes is far past any real schema.
const MAX_QUERY_SHAPES: usize = 1024;
/// Shapes refused by that cap, so the report can say so rather than imply the
/// list is complete.
static DROPPED_QUERY_SHAPES: AtomicU64 = AtomicU64::new(0);

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
    // Checked before the normalization below, which allocates and copies: the
    // expensive half of this function is the half a dormant binary was paying.
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
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
    } else if q.len() < MAX_QUERY_SHAPES {
        q.push((key, 1));
    } else {
        // Past the cap the shapes stop being recorded, and the fact is. A
        // profile that silently drops rows is worse than one that says it did:
        // the reader would take a partial list for the whole surface.
        DROPPED_QUERY_SHAPES.fetch_add(1, Ordering::Relaxed);
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
    let dropped = DROPPED_QUERY_SHAPES.load(Ordering::Relaxed);
    if dropped > 0 {
        out.push_str(&format!(
            "elephc-instr-query-dropped: {dropped} shapes past the {MAX_QUERY_SHAPES} cap\n"
        ));
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
    // Dormant costs nothing, like every other entry point: a service built with
    // the capability and nobody profiling it must not open /dev/urandom twice per
    // request to publish a trace id no one will read.
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
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
    // Published through the environment so userland propagates it with `getenv()`
    // and a stream context, with no new builtin and no change to the request
    // assembly. That places one constraint on the host, and it is worth stating
    // rather than assuming: `setenv` is not safe against a concurrent `getenv` on
    // another thread. elephc's own `--web` is prefork — workers are processes,
    // one request at a time, and the endpoint listener does not survive the fork
    // — so no elephc-built service has a thread to race with, and the dormancy
    // gate above means this does not even run unless the request is being
    // profiled. A host that embeds this runtime alongside its own threads is
    // outside that guarantee.
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
/// `1` starts a slice because the caller was authorized to ask; `2` starts one
/// only if something is waiting for it; `0` ends whatever was started.
///
/// The three-way encoding is what lets the endpoint ask for an exact slice
/// without a second channel into the request path. The web bridge cannot know
/// whether anyone armed a capture — that state lives here — so it reports what
/// it knows (`1` when the request carried a signed header, `2` otherwise) and
/// this decides. The alternative was a new runtime slot for a single bit, in the
/// area where a hardcoded symbol underscore once broke every Linux link.
///
/// `0` is unconditional and idempotent: a request that started no slice must not
/// dump one, or an unprofiled request would end the previous one's capture.
#[no_mangle]
pub extern "C" fn elephc_instr_request(begin: u32) {
    match begin {
        1 | 2 => {
            if begin == 2
                && capture_word(CAPTURE_ARMED_WORD)
                    .is_none_or(|armed| armed.load(Ordering::Acquire) == 0)
            {
                return;
            }
            STATE.with(|s| s.borrow_mut().reset());
            SLICE_OPEN.store(true, Ordering::Relaxed);
            CAPTURE_ONLY.store(begin == 2, Ordering::Relaxed);
            publish_active(true);
            switch_on();
        }
        _ => {
            if !SLICE_OPEN.swap(false, Ordering::Relaxed) {
                return;
            }
            ENABLED.store(false, Ordering::Relaxed);
            publish_active(false);
            elephc_instr_dump();
        }
    }
}

/// Whether a slice is currently being recorded for one request.
///
/// The end call arrives on every request now, so it needs to know whether there
/// was anything to end.
static SLICE_OPEN: AtomicBool = AtomicBool::new(false);

/// Whether the open slice exists only to answer the endpoint.
///
/// Such a slice is an answer to a question, and every request that started one
/// while the capture was armed but did not win the slot has nothing to hand
/// over. Writing those to stderr put a request profile in the service's log for
/// every request in the window.
static CAPTURE_ONLY: AtomicBool = AtomicBool::new(false);

/// Reads the initial hook state at program start.
///
/// A run `monitor` spawned is asked for in full, through the control channel it
/// inherits on fd 3 — the probe's init verifies that channel and publishes the
/// answer here. Without it the hooks stay dormant, so the decision moved from
/// compile time to run time and one build serves both "profile this" and "just
/// run". (An earlier version of this comment named an `ELEPHC_MONITOR`
/// environment variable; nothing has ever read one.)
#[no_mangle]
pub extern "C" fn elephc_instr_boot() {
    // Set by the probe's init, which runs first and owns the one check: the
    // control channel's marker can only be read once, so asking twice would give
    // the second reader nothing.
    let asked = unsafe { std::ptr::addr_of!(elephc_monitor_active).read() };
    // Set by the probe's init only when `monitor` spawned this process, which
    // asks for the whole run — so the slice that opens here is never closed and
    // the word stays true for every PDO statement, as it should.
    if asked != 0 {
        switch_on();
    }
}

/// Publishes whether an exact slice is open right now.
///
/// The word is how a crate that must not depend on this one — PDO, which gates
/// SQL-shape recovery and two clock reads per statement on it — asks whether
/// anyone is recording. It used to be written only by the probe's init, and only
/// to mean "was ever asked", which was wrong in both directions: a service with a
/// configured endpoint had it set from boot with nobody profiling, and a request
/// authorized by a signed `X-Elephc-Query` header had it CLEAR while its slice
/// was open, so that capture lost its query shapes and its DB-driver wait entirely.
fn publish_active(on: bool) {
    // Safety: a `.comm` word of the runtime's own, present in every binary the
    // instrumentation is linked into, and written from ordinary context only.
    unsafe {
        std::ptr::addr_of_mut!(elephc_monitor_active).write(u64::from(on));
    }
}

#[cfg(not(test))]
extern "C" {
    /// Runtime `.comm` word: nonzero while an exact slice is being recorded.
    static mut elephc_monitor_active: u64;
}

// Standalone `cargo test -p elephc-instr` has no compiled elephc program to
// define the runtime word, and `publish_active` is reached from
// `elephc_instr_request` on every test that opens a slice — so unlike the read in
// `elephc_instr_boot`, it cannot be dead-stripped. Defined in the test binary
// only, never in the staticlib, so a real program's `.comm` stays single.
#[cfg(test)]
#[no_mangle]
static mut elephc_monitor_active: u64 = 0;

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

// There were exported `elephc_instr_enable` and `elephc_instr_disable` symbols
// here, and they are gone. Nothing called them: not the compiler, which wires
// `enter`, `exit`, `dump`, `request`, `io`, `wait`, `query`, `throw` and
// `trace_begin` and no others; not the bridges; not the tests; not the docs.
//
// They could not be used correctly either. `disable` only cleared the flag, so
// the frames already on the shadow stack stayed, every exit that arrived while
// off was dropped, and the next `enable` pushed new frames on top of the
// corpses — measured, a function credited 50 ticks where it ran 20, with the
// self values still summing to the root so nothing looked wrong. Making them
// safe means closing the live stack at the toggle, which needs the allocation
// counters the call sites pass and these have no way to read. The paths that DO
// switch profiling on — `elephc_instr_request` and the boot check — do it where
// the stack is empty, which is what makes them correct.

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
fn write_chrome_trace(
    path: &str,
    spans: &[(u32, u64, u64)],
    names: &[String],
    dropped: u64,
    converted: bool,
) {
    use std::io::Write;
    let base = spans.iter().map(|s| s.1).min().unwrap_or(0);
    let name_of = |id: usize| -> String {
        names.get(id).cloned().unwrap_or_else(|| format!("#{id}"))
    };
    // `converted` is the same question the table asks before printing `incl_ns`,
    // and it comes from the caller so this writer is a function of what it is
    // given. Without it a run too short to measure the counter rate wrote raw
    // ticks into a file whose format calls them microseconds, and the note that
    // says so went only to the text profile — a viewer had no way to know.
    let mut out = String::from("{\"traceEvents\":[");
    if !converted {
        out.push_str(
            "{\"name\":\"process_name\",\"ph\":\"M\",\"pid\":1,\"tid\":1,\"args\":{\"name\":\
             \"elephc — UNCONVERTED counter ticks, run too short to measure the rate\"}},",
        );
    }
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
            let mut note = String::new();
            if dropped > 0 {
                note.push_str(&format!(" ({dropped} calls dropped past the trace cap)"));
            }
            if !converted {
                note.push_str(
                    " — WARNING: the run was too short to measure the counter rate, so \
                     these timestamps are raw counter ticks, not microseconds",
                );
            }
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

/// The shortest run the counter rate may be derived from.
///
/// The rate is elapsed ticks over elapsed nanoseconds, so the error is the
/// clock's own resolution over this window. `clock_gettime` resolves to tens of
/// nanoseconds through the vDSO, which over a hundred microseconds is under a
/// twentieth of a percent — far below anything a profile is read to that
/// precision for. The previous window was a millisecond, ten times longer than
/// it needed to be, and every run shorter than it reported raw ticks.
const MIN_RATE_WINDOW_NS: u64 = 100_000;

/// Ticks per second, if the counter rate is known or can be derived now.
///
/// `None` means a span cannot be converted at all — the rate is not published by
/// this platform and the run has not lasted long enough to measure it. The
/// caller then has a choice to make, and the report makes it out loud rather
/// than printing ticks under a column named for nanoseconds.
fn tick_rate() -> Option<u64> {
    match TICK_HZ.load(Ordering::Relaxed) {
        0 => {
            let ns = now_ns().wrapping_sub(EPOCH_NS.load(Ordering::Relaxed));
            let elapsed = now_ticks().wrapping_sub(EPOCH_TICKS.load(Ordering::Relaxed));
            if ns < MIN_RATE_WINDOW_NS || elapsed == 0 {
                return None;
            }
            let hz = (u128::from(elapsed) * 1_000_000_000u128 / u128::from(ns)) as u64;
            if hz == 0 {
                return None;
            }
            TICK_HZ.store(hz, Ordering::Relaxed);
            Some(hz)
        }
        hz => Some(hz),
    }
}

/// Converts a span of ticks to nanoseconds.
///
/// On a platform whose counter rate is not published, the rate is derived from
/// the run: how many ticks elapsed against how many nanoseconds, both measured.
/// A run too short even for that reports its ticks unconverted rather than
/// inventing a rate — and `render` says so, because a number under a column
/// called `incl_ns` is not visibly in the wrong units to anyone reading it.
fn ticks_to_ns(ticks: u64) -> u64 {
    match tick_rate() {
        Some(hz) => (u128::from(ticks) * 1_000_000_000u128 / u128::from(hz)) as u64,
        None => ticks,
    }
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
    /// Folds arbitrary bytes in, for completeness rather than for use.
    fn write(&mut self, bytes: &[u8]) {
        // Not expected — the derived Hash for (u32, u32) calls write_u32 — but a
        // Hasher must accept bytes, and silently hashing nothing would collapse
        // every key into one bucket.
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    /// The path that actually runs: packs the two `u32`s of a `(caller, callee)`
    /// key into one word, losing nothing, since an edge is exactly 64 bits.
    fn write_u32(&mut self, value: u32) {
        self.0 = (self.0 << 32) | u64::from(value);
    }

    /// Mixes the packed pair so adjacent ids do not land in adjacent buckets —
    /// dense compile-time ids would otherwise cluster hard.
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
    /// A fresh hasher per lookup; the state is one word, so this is free.
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
    // Before anything else, and before any `--web` fork: a mapping established
    // here is inherited by every worker, which is the whole point — the endpoint
    // that asks for a slice runs in the parent, and the request that renders one
    // runs in a child. Established even when the table is empty, since the
    // rendezvous is not about names.
    map_capture_region();
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

/// Records that an exception has begun unwinding, from the runtime's single
/// throw path.
///
/// Rare by construction — a throw, not a call — so it buys the two facts the
/// exit hook was guessing at for no per-call cost: that an unwind happened, and
/// when. Everything a catch handler does between here and the catcher's own exit
/// then stays out of the frames the exception passed through.
///
/// Reached from the emitted `__rt_throw_current` helper through the runtime
/// `_elephc_instr_throw_fn` slot, which is null unless `--with-monitoring`
/// linked and initialized this crate.
#[no_mangle]
pub extern "C" fn elephc_instr_throw(allocs: u64, frees: u64) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let t = now_ticks();
    let io = IO_OPS.load(Ordering::Relaxed);
    let w = WAIT_NS.load(Ordering::Relaxed);
    STATE.with(|s| s.borrow_mut().note_throw(t, allocs, frees, io, w));
}

/// Records entry to the function `id`; `allocs` / `frees` are the program's
/// live heap counters (`_gc_allocs` / `_gc_frees`) at the call site.
#[no_mangle]
pub extern "C" fn elephc_instr_enter(id: u32, allocs: u64, frees: u64, frame: usize) {
    // Checked before the clock reads: those are the expensive part, and a
    // dormant binary must not pay them.
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let t = now_ticks();
    let io = IO_OPS.load(Ordering::Relaxed);
    let w = WAIT_NS.load(Ordering::Relaxed);
    STATE.with(|s| s.borrow_mut().enter_at(id, frame, t, allocs, frees, io, w));
}

/// Records exit from the function `id`; `allocs` / `frees` are the program's
/// live heap counters (`_gc_allocs` / `_gc_frees`) at the call site, and `frame`
/// is this activation's frame pointer — the same one its entry passed.
#[no_mangle]
pub extern "C" fn elephc_instr_exit(id: u32, allocs: u64, frees: u64, frame: usize) {
    // Symmetrical with enter: a frame that was never pushed must not be popped,
    // and the frame pointer is what makes that decidable rather than inferred.
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let t = now_ticks();
    let io = IO_OPS.load(Ordering::Relaxed);
    let w = WAIT_NS.load(Ordering::Relaxed);
    STATE.with(|s| s.borrow_mut().exit_at(id, frame, t, allocs, frees, io, w));
}

/// Records that the function `id` is suspending a coroutine at a `yield` or a
/// `Fiber::suspend`; `frame` is this activation's frame pointer, the same one
/// its entry passed.
///
/// Emitted immediately before the stack switch. Between this and the matching
/// resume the activation is not running, and a profiler that says otherwise is
/// not reporting a small error: a generator body measured at 23 us of work
/// claimed 99.8% of a program's inclusive time, and the call graph gained an
/// edge from it to a function the consumer called.
#[no_mangle]
pub extern "C" fn elephc_instr_suspend(
    id: u32,
    allocs: u64,
    frees: u64,
    frame: usize,
    coro: usize,
) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let t = now_ticks();
    let io = IO_OPS.load(Ordering::Relaxed);
    let w = WAIT_NS.load(Ordering::Relaxed);
    STATE.with(|s| s.borrow_mut().suspend_at(id, frame, coro, t, allocs, frees, io, w));
}

/// Puts a coroutine's activations back when the runtime's suspend helper is
/// about to leave without returning to the suspension site.
///
/// Called from that helper rather than from emitted PHP code, through the same
/// null-checked slot the throw hook uses, so a binary without the capability
/// pays one load and a not-taken branch on paths only an error reaches. `coro`
/// is the runtime's `_fiber_current` — the one identity available at every point
/// where the helper raises, and the one the park recorded.
#[no_mangle]
pub extern "C" fn elephc_instr_unpark(allocs: u64, frees: u64, coro: usize) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let t = now_ticks();
    let io = IO_OPS.load(Ordering::Relaxed);
    let w = WAIT_NS.load(Ordering::Relaxed);
    STATE.with(|s| s.borrow_mut().unpark_at(coro, t, allocs, frees, io, w));
}

/// Records that the function `id` has been resumed at the suspension point
/// `frame` named; the counters are read the same way its entry read them.
///
/// Emitted immediately after the stack switch returns, which is where execution
/// picks up. A resume whose park was refused at `MAX_PARKED`, or whose frame the
/// stack has no room for, finds no group and restores nothing — the same
/// nothing an exit for an untracked activation does.
#[no_mangle]
pub extern "C" fn elephc_instr_resume(id: u32, allocs: u64, frees: u64, frame: usize) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let t = now_ticks();
    let io = IO_OPS.load(Ordering::Relaxed);
    let w = WAIT_NS.load(Ordering::Relaxed);
    STATE.with(|s| s.borrow_mut().resume_at(id, frame, t, allocs, frees, io, w));
}

/// Finalizes and publishes an exact slice before a generated terminal path exits.
///
/// `current_id` is the compiler-assigned id of the function executing the
/// terminal path, or `u32::MAX` when selective instrumentation did not assign
/// that function a frame. `frame` is the same activation identity passed to
/// enter/exit hooks. All live tracked frames close at one final counter snapshot,
/// then the normal dump path writes or hands over the slice.
#[no_mangle]
pub extern "C" fn elephc_instr_terminate(
    current_id: u32,
    allocs: u64,
    frees: u64,
    frame: usize,
) {
    if !ENABLED.swap(false, Ordering::Relaxed) {
        return;
    }
    let t = now_ticks();
    let io = IO_OPS.load(Ordering::Relaxed);
    let w = WAIT_NS.load(Ordering::Relaxed);
    let current = (current_id != u32::MAX).then_some((current_id, frame));
    STATE.with(|s| {
        s.borrow_mut()
            .terminate_at(current, t, allocs, frees, io, w)
    });
    SLICE_OPEN.store(false, Ordering::Relaxed);
    publish_active(false);
    elephc_instr_dump();
}

/// The rendezvous where an endpoint asks for a slice and a worker leaves it.
///
/// Mapped MAP_SHARED at init, BEFORE any `--web` fork, for exactly the reason
/// the sample ring is: the endpoint listener lives in the parent that accepts and
/// forks, and the requests it wants profiled run in the children. Process-local
/// state cannot span that, which is why the first version of this worked
/// everywhere except the one place it was for.
///
/// Layout: `armed: u32`, `len: u32`, `owner: u64`, `claimed_at: u32`,
/// `capture_epoch: u32`, `slice_epoch: u32`, padding, then the text. `owner` is
/// `EMPTY`, `READY`, or one indivisible `(process-start-id, pid)` token for the
/// writer currently filling the payload.
///
/// The owner token is the safety boundary. Publishing the pid and its start
/// identity in separate atomics left a window in which a suspended writer was
/// claimed under stale identity, could be reclaimed, and would later resume
/// copying over its successor. `capture_epoch` and `slice_epoch` independently
/// tie a published slice to the capture that asked for it.
const CAPTURE_HEADER: usize = 32;
/// Header word holding whether an endpoint is waiting.
const CAPTURE_ARMED_WORD: usize = 0;
/// Header word holding the published payload length.
const CAPTURE_LENGTH_WORD: usize = 1;
/// Byte offset of the naturally aligned 64-bit owner word.
const CAPTURE_OWNER_OFFSET: usize = 8;
/// Header word holding when the current claim was published.
const CAPTURE_CLAIMED_AT_WORD: usize = 4;
/// Header word holding the identity of the capture currently armed.
///
/// `armed` is a bare yes/no, so a capture that timed out and a NEW one that
/// armed afterwards look identical to a worker that read it. A worker
/// descheduled between reading `armed` and claiming the slot therefore published
/// a slice rendered for the FIRST capture into the second one's answer — a
/// complete, plausible profile of a request that finished before the operator
/// asked, with nothing in it to say so. Every arm takes a new identity, and a
/// slice carries the one it was offered for.
const CAPTURE_EPOCH_WORD: usize = 5;
/// Header word holding the capture identity the published slice answers.
const SLICE_EPOCH_WORD: usize = 6;
/// One mebibyte holds a large per-function table with room to spare; a slice
/// bigger than this is refused rather than cut, since a truncated profile is
/// indistinguishable from a complete one.
const CAPTURE_BYTES: usize = 1 << 20;
/// The states of the rendezvous' owner word: `EMPTY`, `READY`, or a packed
/// process identity for the process currently writing the payload.
///
/// A distinguishable "being written" state exists because with only two, a
/// writer had to publish the slot before it could fill it, or fill a slot it had
/// not reserved. It carries the claimer's PID rather than a flag because the
/// identity has to be established by the SAME atomic that establishes the
/// claim. Winning the compare-exchange now publishes both pieces at once.
const EMPTY: u64 = 0;
/// A rendered slice is waiting. Every real owner token has a non-reserved pid
/// in its low word, so the all-ones value cannot collide with one.
const READY: u64 = u64::MAX;
/// Age after which an unverifiable live owner is reported as blocking capture.
///
/// Age never authorizes reclamation; only a dead PID or a mismatched start
/// identity can do that. This threshold exists solely to avoid reporting a
/// legitimate writer that is briefly between claiming and publishing.
const ABANDONED_CLAIM_SECS: u32 = 60;

/// Whether `pid` still names a live process.
///
/// `kill(pid, 0)` sends no signal; it asks the kernel whether the process exists
/// and whether we could signal it. `EPERM` means it exists and belongs to
/// someone else, which for this question is still "alive" — only `ESRCH` says
/// the process is gone. Erring toward alive is the safe direction: it leaves a
/// slot claimed a little longer rather than handing it to a second writer.
fn process_is_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Seconds on a clock that is the same for every process sharing the mapping
/// and that no administrator can move.
///
/// `CLOCK_MONOTONIC` counts from boot, so two processes reading it agree, which
/// is the only property this needs — the value is compared against another
/// process's reading, never interpreted as a date. It replaced the wall clock
/// because that one CAN move: a forward correction, from NTP or by hand, ages
/// every outstanding claim by however much it jumped and can retire a live
/// writer's claim the instant it lands.
fn monotonic_seconds() -> u32 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // Safety: writes one `timespec` this call owns.
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) } != 0 {
        // Reads as "claimed just now", which delays reclamation rather than
        // hastening it. The other direction hands out a live writer's slot.
        return 0;
    }
    now.tv_sec as u32
}
/// A value that distinguishes the process now running under `pid` from any
/// earlier one that held the same pid, or `None` where it cannot be had.
///
/// The kernel is the only source: a pid alone does not identify a process,
/// because pids are recycled, and the recycled case is precisely the one where
/// revoking a claim is right. Start time is what both supported platforms
/// expose and what both `ps` and every process-supervision tool uses for the
/// same purpose. Folded to 32 bits so it and the pid fit in the indivisible
/// 64-bit owner token; this is an identity discriminator, not elapsed time. A
/// theoretical fold collision delays stale recovery (the safe direction) and
/// never authorizes another writer over a live claim.
#[cfg(target_os = "linux")]
fn process_start_id(pid: i32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Field 2 is the executable name in parentheses and may itself contain both
    // spaces and parentheses, so the fields after it are located from the LAST
    // `)` rather than by splitting the line. What follows it is field 3 onward,
    // and start time is field 22.
    let after_name = stat.get(stat.rfind(')')? + 1..)?;
    let start_ticks: u64 = after_name.split_whitespace().nth(19)?.parse().ok()?;
    Some(fold_start_id(start_ticks))
}

/// macOS has no `/proc`; `libproc` answers the same question.
#[cfg(target_os = "macos")]
fn process_start_id(pid: i32) -> Option<u32> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // Safety: `info` is exactly `size` writable bytes, which is what the flavor
    // is documented to fill.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            &mut info as *mut libc::proc_bsdinfo as *mut libc::c_void,
            size,
        )
    };
    // A short answer is not an answer: the call reports how much it wrote, and
    // anything less than the whole struct leaves the start time uninitialized.
    if written != size {
        return None;
    }
    Some(fold_start_id(
        (info.pbi_start_tvsec << 20) ^ info.pbi_start_tvusec,
    ))
}

/// Anywhere else the identity is unavailable, so exact handover refuses to
/// claim a slot rather than publishing an owner that cannot be reclaimed safely.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn process_start_id(_pid: i32) -> Option<u32> {
    None
}

/// Folds a start time into the owner token's upper word, never to 0.
fn fold_start_id(value: u64) -> u32 {
    let folded = (value ^ (value >> 32)) as u32;
    if folded == 0 {
        1
    } else {
        folded
    }
}

/// Packs one process incarnation into the indivisible owner token.
fn pack_capture_owner(pid: u32, start_id: u32) -> Option<u64> {
    if pid == 0 || pid == u32::MAX || start_id == 0 {
        return None;
    }
    Some((u64::from(start_id) << 32) | u64::from(pid))
}

/// Returns the current process's complete owner token, if the platform can
/// establish its start identity before the claim is published.
fn current_capture_owner() -> Option<u64> {
    let pid = std::process::id();
    pack_capture_owner(pid, process_start_id(pid as i32)?)
}

/// Splits a claimed owner token into `(pid, start identity)`.
fn unpack_capture_owner(owner: u64) -> Option<(i32, u32)> {
    if owner == EMPTY || owner == READY {
        return None;
    }
    let pid = owner as u32;
    let start_id = (owner >> 32) as u32;
    if pid == 0 || pid == u32::MAX || start_id == 0 {
        return None;
    }
    Some((pid as i32, start_id))
}

/// Whether an owner token no longer names the process that published it.
///
/// A dead pid is stale immediately. A live pid with a different kernel start
/// identity is a reused pid and is stale immediately. If the kernel cannot
/// answer, the safe outcome is to leave the claim standing: timeouts cannot
/// distinguish a dead writer from one suspended between claim and publication.
fn claim_may_be_taken(claimer_alive: bool, current_start_id: Option<u32>, recorded: u32) -> bool {
    !claimer_alive || current_start_id.is_some_and(|current| current != recorded)
}

/// Clears a completed slice nobody is going to take or a claim whose complete
/// owner identity is demonstrably dead/reused, leaving every live writer alone.
/// An old owner whose current identity cannot be read is reported but never
/// reclaimed, because elapsed time cannot distinguish a dead writer from one
/// suspended while copying.
fn release_stale_slice(owner: &AtomicU64, claimed_at: &AtomicU32) {
    if owner
        .compare_exchange(READY, EMPTY, Ordering::AcqRel, Ordering::Relaxed)
        .is_ok()
    {
        HELD_BY.store(0, Ordering::Relaxed);
        return;
    }
    let state = owner.load(Ordering::Acquire);
    let Some((claimer, recorded_start_id)) = unpack_capture_owner(state) else {
        HELD_BY.store(0, Ordering::Relaxed);
        return;
    };
    let alive = process_is_alive(claimer);
    let current_start_id = process_start_id(claimer);
    if claim_may_be_taken(alive, current_start_id, recorded_start_id) {
        // Against the exact identity inspected: a writer that finished or a
        // successor that claimed the slot meanwhile is left untouched.
        let _ = owner.compare_exchange(state, EMPTY, Ordering::AcqRel, Ordering::Relaxed);
        HELD_BY.store(0, Ordering::Relaxed);
        return;
    }
    let age = monotonic_seconds().wrapping_sub(claimed_at.load(Ordering::Relaxed));
    let blocked = if alive && current_start_id.is_none() && age > ABANDONED_CLAIM_SECS {
        claimer
    } else {
        0
    };
    HELD_BY.store(blocked, Ordering::Relaxed);
}

/// The pid of a claim this process declined to revoke because it could not tell
/// whether the holder was still writing, or 0.
///
/// Process-local on purpose: it is a diagnostic about a decision THIS process
/// made, read by the endpoint that lives in the same process, and putting it in
/// the shared mapping would have every worker overwrite every other's.
static HELD_BY: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// Reports a capture slot held by a process this one could not vouch for, or 0.
///
/// The endpoint asks after a capture times out, so an answer nobody can explain
/// — traffic flowing, requests completing, and no slice — names the process that
/// is holding the rendezvous instead of leaving the operator to guess.
#[no_mangle]
pub extern "C" fn elephc_instr_capture_blocked_by() -> i32 {
    HELD_BY.load(Ordering::Relaxed)
}
/// Base address of that mapping, or 0 when it could not be established — in
/// which case the capture is simply unavailable and the dump keeps logging.
static CAPTURE_REGION: AtomicUsize = AtomicUsize::new(0);

/// One `u32` field of the rendezvous header, addressed by index.
fn capture_word(index: usize) -> Option<&'static AtomicU32> {
    let base = CAPTURE_REGION.load(Ordering::Acquire);
    if base == 0 {
        return None;
    }
    debug_assert!(
        index * 4 < CAPTURE_HEADER,
        "header word {index} is past the {CAPTURE_HEADER}-byte header"
    );
    // Safety: the mapping is at least CAPTURE_HEADER bytes, established once at
    // init and never unmapped, and `AtomicU32` has the layout of `u32`.
    Some(unsafe { &*((base + index * 4) as *const AtomicU32) })
}

/// Returns the aligned 64-bit owner/publication word of the rendezvous.
fn capture_owner() -> Option<&'static AtomicU64> {
    let base = CAPTURE_REGION.load(Ordering::Acquire);
    if base == 0 {
        return None;
    }
    // Safety: mmap returns page-aligned storage, the offset is 8-byte aligned,
    // and the mapping lives for the process lifetime.
    Some(unsafe { &*((base + CAPTURE_OWNER_OFFSET) as *const AtomicU64) })
}

/// Maps the rendezvous. Idempotent, and silent on failure: a diagnostic that
/// cannot allocate its scratch must not take the process down.
fn map_capture_region() {
    if CAPTURE_REGION.load(Ordering::Acquire) != 0 {
        return;
    }
    #[cfg(target_os = "linux")]
    let anon = libc::MAP_ANONYMOUS;
    #[cfg(not(target_os = "linux"))]
    let anon = libc::MAP_ANON;
    // Safety: an anonymous shared mapping with no fixed address.
    let region = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            CAPTURE_BYTES,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | anon,
            -1,
            0,
        )
    };
    if region == libc::MAP_FAILED {
        return;
    }
    // Init and `capture_arm` both call this, and both can find it unset. Storing
    // outright let the second mapping replace the first: the endpoint would then
    // arm one region while the workers inherited the other, and the operator
    // waited out the timeout for a slice that was written somewhere else.
    if CAPTURE_REGION
        .compare_exchange(0, region as usize, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        // Safety: this mapping is ours, was just created, and nothing has been
        // handed a pointer into it.
        unsafe { libc::munmap(region, CAPTURE_BYTES) };
    }
}

/// Asks for the next slice to be handed back instead of only printed.
///
/// Idempotent: arming twice leaves one waiter, because two readers of the same
/// slice would each get half an answer. The endpoint serializes its callers.
#[no_mangle]
pub extern "C" fn elephc_instr_capture_arm() {
    // A caller may arm before init has run in this process; mapping here as well
    // makes the entry point self-sufficient without changing where the mapping
    // that matters — the one inherited across the fork — is established.
    map_capture_region();
    if let (Some(owner), Some(armed), Some(claimed_at)) = (
        capture_owner(),
        capture_word(CAPTURE_ARMED_WORD),
        capture_word(CAPTURE_CLAIMED_AT_WORD),
    ) {
        release_stale_slice(owner, claimed_at);
        // A new identity for this capture, taken BEFORE it is announced as
        // armed, so no worker can read "armed" and pair it with the previous
        // capture's identity.
        if let Some(epoch) = capture_word(CAPTURE_EPOCH_WORD) {
            epoch.fetch_add(1, Ordering::AcqRel);
        }
        armed.store(1, Ordering::Release);
    }
}

/// Takes the captured slice, if one has been rendered since arming.
///
/// Copies at most `cap` bytes into `out` and returns the full length, so a
/// caller that guessed too small learns by how much rather than receiving a
/// silently truncated profile. Returns 0 when nothing has been captured yet.
///
/// # Safety
/// `out` must point to at least `cap` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_instr_capture_take(out: *mut u8, cap: usize) -> usize {
    let (Some(armed), Some(owner), Some(length)) = (
        capture_word(CAPTURE_ARMED_WORD),
        capture_owner(),
        capture_word(CAPTURE_LENGTH_WORD),
    )
    else {
        return 0;
    };
    // A claimed-but-unfinished slot reads as nothing: the writer is mid-copy,
    // and the caller polls until it is done or its own wait runs out.
    if owner.load(Ordering::Acquire) != READY {
        return 0;
    }
    // A slice offered to a capture that is no longer the one running answers the
    // wrong question. `armed` alone could not tell them apart — it is a yes/no,
    // so a capture that timed out and the one that armed next read the same — and
    // a worker descheduled between reading it and claiming the slot published a
    // profile of a request that had finished before this caller ever asked.
    // Discarded rather than served: the poll continues, and gets the request it
    // asked for.
    //
    // Compared against the CURRENT identity, which is this caller's own: the
    // endpoint runs one capture at a time.
    let current = capture_word(CAPTURE_EPOCH_WORD).map(|e| e.load(Ordering::Acquire));
    let offered = capture_word(SLICE_EPOCH_WORD).map(|e| e.load(Ordering::Acquire));
    if current != offered {
        // Re-armed, not merely discarded. The offer path refuses to publish for a
        // capture that has ended, but its check and its store are two
        // instructions apart, so a slice can still land here — and the worker
        // that wrote it may have disarmed on the way. This caller is still inside
        // its own poll and owns the capture, so putting the flag back is what
        // lets the request it asked for be offered at all.
        armed.store(1, Ordering::Release);
        // Released with a compare-exchange from READY, not a blind store. The
        // endpoint serializes today, but the rendezvous does not depend on that
        // caller-side discipline for correctness.
        let _ = owner.compare_exchange(READY, EMPTY, Ordering::AcqRel, Ordering::Relaxed);
        return 0;
    }
    let len = length.load(Ordering::Relaxed) as usize;
    // The header is shared, writable memory; a length it cannot back is a read
    // past the end of the mapping, so this is checked here rather than trusted
    // from any one writer.
    if len > CAPTURE_BYTES - CAPTURE_HEADER {
        let _ = owner.compare_exchange(READY, EMPTY, Ordering::AcqRel, Ordering::Relaxed);
        armed.store(0, Ordering::Relaxed);
        return 0;
    }
    // Peek. An earlier version consumed on every call, so the caller's "how long
    // is it?" poll disarmed the capture it was waiting for and the slice could
    // never arrive.
    if out.is_null() || cap < len {
        return len;
    }
    let base = CAPTURE_REGION.load(Ordering::Relaxed);
    std::ptr::copy_nonoverlapping((base + CAPTURE_HEADER) as *const u8, out, len);
    // Release, not Relaxed: this store is what frees the payload for the next
    // writer, and the writers are OTHER PROCESSES sharing this mapping. Relaxed
    // does not order the copy above against it, so on a weakly ordered machine a
    // worker could observe EMPTY, claim the slot, and start overwriting the bytes
    // this call is still reading — a profile stitched from two slices, with
    // nothing to detect it.
    // The copy above is done either way, so the length is returned whatever the
    // exchange says: losing it means somebody else already freed the slot, not
    // that this caller failed to read it.
    let _ = owner.compare_exchange(READY, EMPTY, Ordering::AcqRel, Ordering::Relaxed);
    armed.store(0, Ordering::Relaxed);
    len
}

/// Stops waiting, so a caller that gave up does not leave the next slice
/// diverted to nobody.
#[no_mangle]
pub extern "C" fn elephc_instr_capture_cancel() {
    if let (Some(armed), Some(owner), Some(claimed_at)) = (
        capture_word(CAPTURE_ARMED_WORD),
        capture_owner(),
        capture_word(CAPTURE_CLAIMED_AT_WORD),
    ) {
        armed.store(0, Ordering::Relaxed);
        release_stale_slice(owner, claimed_at);
    }
}

/// Hands a rendered slice to whoever armed the capture, if anyone did.
///
/// Returns whether it was taken, because the caller still has to decide about
/// stderr: a slice asked for over the endpoint is an answer to a question, not
/// a log line, and printing it as well would put one request's profile in the
/// service's log every time someone looked.
fn offer_capture(text: &str) -> bool {
    // The capture running when this request finished. Split out so a test can
    // drive the real path with a STALE value; staging the same thing by writing
    // header words by hand is how the first version of this fix came to assert a
    // property the production path did not have.
    let offered_for = capture_word(CAPTURE_EPOCH_WORD)
        .map(|epoch| epoch.load(Ordering::Acquire))
        .unwrap_or(0);
    offer_capture_for(text, offered_for)
}

/// Hands `text` over as the answer to the capture identified by `offered_for`.
fn offer_capture_for(text: &str, offered_for: u32) -> bool {
    let (Some(armed), Some(owner)) = (capture_word(CAPTURE_ARMED_WORD), capture_owner()) else {
        return false;
    };
    if armed.load(Ordering::Acquire) == 0 {
        return false;
    }
    // Claim the slot before writing a byte into it. Under `--web` the workers
    // are separate processes sharing this mapping, and every one of them serving
    // a request while a capture is armed reaches here: testing `filled` and then
    // setting it let two of them pass the test and copy two profiles over each
    // other, which the reader then served as one — valid UTF-8 often enough to
    // be believed.
    //
    // The claim and the claimer's name are ONE store. Winning this is what makes
    // the slot ours, and it carries both the pid and process-start identity in
    // the same instant. A platform that cannot establish both pieces cannot
    // safely distinguish PID reuse from a suspended live writer, so exact
    // handover is refused there.
    let Some(mine) = current_capture_owner() else {
        return false;
    };
    if owner
        .compare_exchange(EMPTY, mine, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return false;
    }
    // The time is published by the WINNER, after winning — never before, and
    // never by a loser. Stamping before the claim was defended as harmless
    // because a loser only makes a claim look fresher, and that is true of one
    // loser and false of a stream of them: under traffic every request reaching
    // here refreshes the age of whatever claim is standing, so a claim held by a
    // pid that has been recycled never reaches the backstop that exists to
    // recover it. The rendezvous stayed blocked for as long as the service kept
    // serving, which is exactly when an operator wants it.
    //
    // The complete owner identity was already published by the CAS above, so a
    // claimer preempted before this diagnostic timestamp is still identifiable
    // and cannot be revoked by age.
    if let Some(claimed_at) = capture_word(CAPTURE_CLAIMED_AT_WORD) {
        claimed_at.store(monotonic_seconds(), Ordering::Relaxed);
    }
    // Nothing is published for a capture that has since ended, and — this is the
    // part the first version got wrong — nothing is DISARMED for it either. A
    // stale slice was correctly discarded by the reader, but the winner had
    // already stored `armed = 0`, which belongs to the capture now running: no
    // later request offered, the endpoint waited out its whole timeout, and
    // answered "no request completed" while requests were completing. That trades
    // a wrong profile for no profile and a false reason, which is worse.
    //
    // The claim is given back so the capture that IS running can still be
    // answered by the next request to finish.
    if capture_word(CAPTURE_EPOCH_WORD)
        .is_some_and(|epoch| epoch.load(Ordering::Acquire) != offered_for)
    {
        let _ = owner.compare_exchange(mine, EMPTY, Ordering::AcqRel, Ordering::Relaxed);
        return false;
    }
    // The question has its answer, so stop asking it of everything else still
    // running. Leaving it armed kept every later request paying for a slice that
    // could no longer be handed over, and the refused offers fell through to the
    // log.
    armed.store(0, Ordering::Release);
    publish_capture(text, mine, offered_for)
}

/// Copies one claimed slice and publishes it only if `mine` still owns the
/// rendezvous. Kept separate so tests can suspend a writer exactly between the
/// owner publication and the payload publication.
fn publish_capture(text: &str, mine: u64, offered_for: u32) -> bool {
    let (Some(owner), Some(length)) = (capture_owner(), capture_word(CAPTURE_LENGTH_WORD)) else {
        return false;
    };
    if owner.load(Ordering::Acquire) != mine {
        return false;
    }
    let note;
    let mut bytes = text.as_bytes();
    if bytes.len() > CAPTURE_BYTES - CAPTURE_HEADER {
        // Too large to hand over, and truncating it would produce a profile that
        // reads as complete. Publishing the true length instead — which is what
        // this did, so "the reader can say so" — handed the reader a length its
        // own mapping could not back, and the reader allocated that much and
        // copied it straight past the end of the region. A sentence fits, says
        // what happened, and is the size it claims to be.
        // `note:`, like every other non-metric reason this crate publishes. Both
        // readers recognise a reason by that prefix and drop anything else as a
        // row they could not parse, so without it this sentence was discarded and
        // the operator was told the generic "no slice arrived within the wait" —
        // the one reason it was not. Three distinguishable outcomes, one of them
        // silently collapsed into another.
        note = format!(
            "elephc-instr: note: the exact profile is {} bytes, larger than the {} the \
             rendezvous carries; profile a narrower slice\n",
            bytes.len(),
            CAPTURE_BYTES - CAPTURE_HEADER,
        );
        bytes = note.as_bytes();
    }
    let base = CAPTURE_REGION.load(Ordering::Acquire);
    // Safety: the mapping holds CAPTURE_BYTES and the length was just checked.
    unsafe {
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            (base + CAPTURE_HEADER) as *mut u8,
            bytes.len(),
        );
    }
    length.store(bytes.len() as u32, Ordering::Relaxed);
    // Which capture this slice answers, published before READY so a reader that
    // sees the slice always sees the identity that goes with it.
    if let Some(slice_epoch) = capture_word(SLICE_EPOCH_WORD) {
        slice_epoch.store(offered_for, Ordering::Relaxed);
    }
    // The Release CAS publishes the payload while proving this writer still
    // owns the exact token it claimed. A stale writer can never turn a
    // successor's in-progress claim into READY.
    owner
        .compare_exchange(mine, READY, Ordering::Release, Ordering::Acquire)
        .is_ok()
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
        end_slice();
        return;
    }
    let slice = format!("{}{}{}", render_trace(), text, render_queries());
    // Someone asked for this one over the endpoint: hand it over instead of
    // logging it. Doing both would write a request's profile into the service's
    // log every time an operator looked at it — and so would logging the ones
    // that lose the race, which is every other request that was in flight when
    // the capture was armed.
    if !offer_capture(&slice) && !CAPTURE_ONLY.load(Ordering::Relaxed) {
        let _ = std::io::stderr().write_all(slice.as_bytes());
    }
    if TRACE_ON.load(Ordering::Relaxed) {
        if let Some(path) = TRACE_PATH.lock().ok().and_then(|g| g.clone()) {
            STATE.with(|s| {
                let s = s.borrow();
                write_chrome_trace(
                    path.as_str(),
                    &s.trace,
                    &names,
                    s.trace_dropped,
                    tick_rate().is_some(),
                );
            });
        }
    }
    end_slice();
}

/// Ends the current slice: everything that describes it, dropped together.
///
/// The per-thread accumulators and the process-wide query list are two places,
/// and clearing one without the other is what let an idle worker's empty dump
/// carry its statement shapes into the next profiled request. Callers get one
/// function so a third exit cannot clear half.
///
/// `IO_OPS` and `WAIT_NS` are deliberately NOT reset: they are only ever read as
/// deltas against a per-frame snapshot, so letting them run keeps every
/// attribution correct across slices.
fn end_slice() {
    CAPTURE_ONLY.store(false, Ordering::Relaxed);
    STATE.with(|s| s.borrow_mut().reset());
    if let Ok(mut q) = QUERIES.lock() {
        q.clear();
    }
    DROPPED_QUERY_SHAPES.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {

    /// Reads the runtime word the way another crate does — PDO's whole gate.
    fn published_active() -> u64 {
        unsafe { std::ptr::addr_of!(crate::elephc_monitor_active).read() }
    }

    /// The word PDO gates on follows the SLICE, in both directions.
    ///
    /// One test rather than three because there is one word per process and the
    /// suite runs in parallel: split across tests these assertions raced each
    /// other, passed alone and failed together.
    #[test]
    fn the_published_word_follows_the_open_slice() {
        // The same lock the other bracket-driving test takes. The slice state is
        // one word and one shadow stack per PROCESS, and the runner is parallel.
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        elephc_instr_request(0);
        assert_eq!(published_active(), 0, "nothing open to begin with");

        elephc_instr_request(1);
        assert_eq!(
            published_active(),
            1,
            "a signed-header request opens a slice, and PDO has to see it — this \
             was the state where an authorized capture silently lost its query \
             shapes and its DB-driver wait",
        );

        elephc_instr_request(0);
        assert_eq!(
            published_active(),
            0,
            "and the slice closing puts the process back to costing nothing",
        );

        // `2` offers the request: with no capture armed there is nothing to
        // record, so no slice opens and nothing is published.
        elephc_instr_request(2);
        assert_eq!(
            published_active(),
            0,
            "an unsigned request on an unasked service must stay dormant",
        );
    }
    /// A throw out of the dropped region must not swallow a tracked frame's exit.
    ///
    /// Past `MAX_STACK` activations are counted rather than pushed. Reconciling
    /// their exits used to mean counting them down, and a throw destroys them
    /// without any exit arriving — so the count went on to eat the exit of a
    /// frame that WAS tracked, which then never closed. There is no count now:
    /// an exit carries the frame it belongs to, and one that is not on the stack
    /// closes nothing on its own.
    #[test]
    fn a_throw_out_of_the_dropped_region_does_not_swallow_a_real_exit() {
        let mut state = State::default();
        for depth in 0..MAX_STACK {
            state.enter_sim((depth % 3) as u32, depth as u64, 0, 0, 0, 0);
        }
        let dropped: Vec<usize> = (0..3)
            .map(|_| state.enter_sim(7, 1_000, 0, 0, 0, 0))
            .collect();
        assert_eq!(state.dropped, 3, "the deeper calls should have been dropped");
        assert_eq!(state.stack.len(), MAX_STACK, "and none of them pushed");

        // The throw destroys those activations; none of them will ever exit.
        state.note_throw(1_500, 0, 0, 0, 0);

        let catcher = state.stack.last().expect("full stack").id;
        let catcher_fp = state.stack.last().expect("full stack").fp;
        let before = state.stack.len();
        state.exit_at(catcher, catcher_fp, 2_000, 0, 0, 0, 0);
        assert!(
            state.stack.len() < before,
            "the catching frame's exit was swallowed: stack still {} deep",
            state.stack.len()
        );

        // And an exit that DOES arrive for a dropped activation still closes
        // nothing, throw or no throw.
        let after = state.stack.len();
        state.exit_at(7, dropped[0], 2_100, 0, 0, 0, 0);
        assert_eq!(state.stack.len(), after, "a dropped exit disturbed the stack");
    }

    /// A dropped exit is ignored even when its frame pointer ALIASES a stale one.
    ///
    /// The reason a dropped activation needs no bookkeeping is that its exit
    /// "will arrive carrying a frame pointer that is not on the stack". That
    /// holds while the stack describes live frames, and an unwind breaks it: the
    /// frames an exception destroyed stay on the shadow stack until the catcher
    /// exits, while the native stack they occupied is already free. A call the
    /// handler makes is pushed onto that reclaimed space, so its frame pointer
    /// is not merely able to collide with a dead frame's — at a fixed frame size
    /// it is the SAME address.
    ///
    /// The existing dropped-exit test picks a frame pointer below every live
    /// frame, so it cannot collide and says nothing about this. Reported on the
    /// pull request with this recipe, and it reproduced.
    #[test]
    fn a_dropped_exit_whose_frame_pointer_aliases_a_stale_one_closes_nothing() {
        let mut state = State::default();
        for depth in 0..MAX_STACK {
            state.enter_sim((depth % 3) as u32, depth as u64, 0, 0, 0, 0);
        }
        assert_eq!(state.stack.len(), MAX_STACK);

        // Raised at the top and caught at the root: every frame above index 0 is
        // dead, and every one of them is still on the shadow stack.
        state.note_throw(1_500, 0, 0, 0, 0);

        // The handler calls something. The stack is full, so the activation is
        // dropped — and the address it runs at is one the unwind freed.
        let stale_fp = state.stack[1].fp;
        let handler = 9u32;
        state.enter_at(handler, stale_fp, 1_600, 0, 0, 0, 0);
        assert_eq!(state.stack.len(), MAX_STACK, "a dropped call pushes nothing");

        let before = state.stack.len();
        state.exit_at(handler, stale_fp, 1_700, 0, 0, 0, 0);
        assert_eq!(
            state.stack.len(),
            before,
            "a dropped activation's exit closed {} frames it never owned",
            before - state.stack.len()
        );
    }

    /// Where a simulated COROUTINE stack starts.
    ///
    /// Far from `SIM_BASE`, because that is what a real one is: a generator body
    /// runs on its own allocation, not on the consumer's stack, and a test that
    /// let the two ranges meet would be testing address arithmetic instead of
    /// the matching.
    const CORO_BASE: usize = 0x7ffe_0000_0000;

    /// A suspended coroutine is not the caller of what the consumer does next.
    ///
    /// `yield` switches stacks; it does not return. Left on the shadow stack the
    /// generator's frame is what `enter_at` reads as the caller of the next
    /// call, and what that call's whole cost is charged to. Measured on four
    /// lines of PHP before this existed: a body that ran 23 us reported 99.8%
    /// inclusive time, and the call graph carried `producer -> heavy x3` — an
    /// edge to a function the consumer called and the generator never did.
    #[test]
    fn a_suspended_coroutine_is_not_the_caller_of_what_runs_next() {
        let mut state = State::default();
        // drain() on the main stack, then the generator body on its own.
        state.enter_sim(0, 0, 0, 0, 0, 0);
        state.enter_at(1, CORO_BASE, 10, 0, 0, 0, 0);
        // The body runs 20 ns of its own, then yields.
        state.suspend_sim(1, CORO_BASE, 30, 0, 0, 0, 0);
        assert!(
            state.stack.iter().all(|frame| frame.id != 1),
            "a suspended body must not sit on the stack the consumer is running on"
        );

        // The consumer's work, which the generator neither did nor called.
        state.enter_sim(2, 30, 0, 0, 0, 0);
        state.exit_sim(2, 1030, 0, 0, 0, 0);
        assert_eq!(
            state.edges.get(&(0, 2)).map(|e| e.0),
            Some(1),
            "the consumer's call must be attributed to the consumer"
        );
        assert!(
            !state.edges.contains_key(&(1, 2)),
            "the profile invented an edge from a coroutine that was not running"
        );

        state.resume_at(1, CORO_BASE, 1030, 0, 0, 0, 0);
        state.exit_at(1, CORO_BASE, 1040, 0, 0, 0, 0);
        state.exit_sim(0, 1040, 0, 0, 0, 0);

        assert_eq!(
            state.fns[1].incl_ns, 30,
            "the body ran 20 ns before the yield and 10 ns after; the 1000 ns \
             between them belong to the consumer"
        );
        assert_eq!(state.fns[2].incl_ns, 1000, "the consumer's own work is untouched");
        assert_eq!(
            state.fns[0].excl_ns, 10,
            "and drain's self time is its span minus its two real children"
        );
    }

    /// Every dimension survives the suspension, not just the clock.
    ///
    /// `close_frame` subtracts five counters the same way, so parking has to
    /// exclude the suspended span from five or the ones it forgot report the
    /// consumer's work as the coroutine's. Allocations are the one that gives
    /// the recommendation engine its "allocates the most" line.
    #[test]
    fn a_resumed_coroutine_excludes_the_suspended_span_in_every_dimension() {
        let mut state = State::default();
        state.enter_at(0, CORO_BASE, 100, 10, 5, 2, 50);
        // 20 ns, 3 allocs, 1 free, 1 io op and 10 ns of wait before the yield.
        state.suspend_sim(0, CORO_BASE, 120, 13, 6, 3, 60);
        // The consumer spends a great deal of everything while it is away.
        state.resume_at(0, CORO_BASE, 5_120, 913, 406, 303, 5_060);
        // Then 5 ns, 2 allocs, 1 free, 1 io op and 5 ns of wait after it.
        state.exit_at(0, CORO_BASE, 5_125, 915, 407, 304, 5_065);

        let acc = &state.fns[0];
        assert_eq!(acc.excl_ns, 25, "time");
        assert_eq!(acc.excl_allocs, 5, "allocations");
        assert_eq!(acc.excl_frees, 2, "frees");
        assert_eq!(acc.excl_io, 2, "io operations");
        assert_eq!(acc.excl_wait, 15, "io wait");
        assert_eq!(acc.incl_ns, 25, "and the inclusive span agrees with the self time");
        assert_eq!(acc.calls, 1, "a resume is the same activation, not a second call");
    }

    /// A coroutine that is never resumed still reports the time it ran.
    ///
    /// Most generators are abandoned: a `foreach` that breaks early, a
    /// `current()` with no `next()`. Its body never returns, so no exit hook
    /// ever arrives for it, and setting its frame aside to be rebased at a
    /// resume that never comes would report a function that ran as one that did
    /// not. A suspension closing the frame is what makes the work it did visible
    /// — and is why this is not the shape it was first written in.
    #[test]
    fn an_abandoned_coroutine_still_reports_what_it_ran() {
        let mut state = State::default();
        state.enter_sim(0, 0, 0, 0, 0, 0); // consumer
        state.enter_at(1, CORO_BASE, 10, 0, 0, 0, 0); // the body, on its own stack
        state.suspend_sim(1, CORO_BASE, 40, 0, 0, 0, 0); // 30 ns, then it yields
        // Nobody ever resumes it. The consumer goes on and returns.
        state.exit_sim(0, 1_040, 0, 0, 0, 0);

        assert_eq!(
            state.fns[1].excl_ns, 30,
            "the body's own 30 ns were lost with the resume that never came"
        );
        assert_eq!(state.fns[1].incl_ns, 30, "and its inclusive span agrees");
        assert_eq!(
            state.fns[0].excl_ns, 1_010,
            "the consumer keeps its own 1010 ns and is charged the body's 30 as a child"
        );
        assert_eq!(
            state.edges.get(&(0, 1)).map(|edge| edge.1),
            Some(30),
            "the edge carries the span the body actually ran, not the consumer's"
        );
        assert_eq!(state.parked.len(), 1, "the group is still parked, and bounded");
    }

    /// A nested activation suspending does not end the outer one's span.
    ///
    /// Inclusive time is per FUNCTION, spanning its outermost activation, so
    /// that recursion is counted once — `depth` is what says whether the
    /// outermost one is still running. A suspension closes a frame and a resume
    /// opens one, so both have to move that counter, and the resume's half is
    /// invisible until two activations of the same function are live: with one,
    /// the span is reopened by the restamp either way and the totals agree by
    /// accident. A generator delegating to another instance of itself is the
    /// ordinary way to have two.
    #[test]
    fn a_nested_activation_suspending_does_not_end_the_outer_ones_span() {
        let mut state = State::default();
        let outer = CORO_BASE;
        let inner = CORO_BASE - 64;
        state.enter_at(0, outer, 0, 0, 0, 0, 0);
        state.enter_at(0, inner, 10, 0, 0, 0, 0);

        state.suspend_sim(0, inner, 20, 0, 0, 0, 0);
        assert_eq!(
            state.fns[0].depth, 1,
            "the outer activation is still running and still holds the span"
        );
        assert_eq!(
            state.fns[0].incl_ns, 0,
            "no inclusive time is credited while the outermost activation runs"
        );

        state.resume_at(0, inner, 1_020, 0, 0, 0, 0);
        assert_eq!(state.fns[0].depth, 2, "and the resume puts the activation back");
        state.exit_at(0, inner, 1_030, 0, 0, 0, 0);
        state.exit_at(0, outer, 1_040, 0, 0, 0, 0);

        assert_eq!(
            state.fns[0].incl_ns, 1_040,
            "the outermost activation's span, counted once"
        );
        assert_eq!(state.fns[0].calls, 2, "and neither resume counted as a call");
    }

    /// A catch handler that suspends does not park the frames the throw killed.
    ///
    /// Between a throw and its catcher exiting, the frames the exception
    /// destroyed stay on this stack — deliberately, so their cost lands on them
    /// rather than on the handler. If that catcher is a generator body and it
    /// yields, those dead frames sit ABOVE the suspending one, and parking
    /// everything above the index swept them into the group: closed at the
    /// suspension's timestamp rather than at their killer's, then reopened by
    /// the resume as LIVE callees, with everything after charged through
    /// activations that no longer existed.
    ///
    /// A suspension is reached by a call, so a coroutine never has a live frame
    /// above it. Anything up there is unwind debris and is closed the way an
    /// exit closes it.
    #[test]
    fn a_catch_handler_that_suspends_does_not_park_the_dead() {
        let mut state = State::default();
        // The catcher is a generator body on its own stack, with two frames
        // above it that a throw is about to destroy.
        state.enter_at(0, CORO_BASE, 0, 0, 0, 0, 0);
        state.enter_at(1, CORO_BASE - 64, 10, 0, 0, 0, 0);
        state.enter_at(2, CORO_BASE - 128, 20, 0, 0, 0, 0);
        state.note_throw(30, 0, 0, 0, 0);
        assert_eq!(state.stack.len(), 3, "the throw leaves them standing");

        // The catcher yields, 1000 ns later.
        state.suspend_sim(0, CORO_BASE, 1_030, 0, 0, 0, 0);

        let group = state.parked.first().expect("the catcher parked");
        assert_eq!(
            group.frames,
            vec![(0, CORO_BASE)],
            "only the suspending frame belongs to the coroutine"
        );
        assert!(state.stack.is_empty(), "and the debris is gone, not parked");
        assert_eq!(
            state.fns[2].excl_ns, 10,
            "the innermost dead frame is closed at the throw that killed it, \
             not at the suspension 1000 ns later"
        );
        assert_eq!(state.fns[1].excl_ns, 10, "and so is the one below it");
    }

    /// A resume does not open a second copy of an activation that is already live.
    ///
    /// A park refused at `MAX_PARKED` leaves its frame on the stack. A coroutine
    /// STACK that has been freed is handed back out too, so a later generator of
    /// the same function can run at the same address under a different fiber —
    /// and its resume, which is keyed on the frame pointer and the id, then finds
    /// the older group.
    ///
    /// The coroutines differ here, which is what keeps this reachable: a park
    /// drops any group standing under its own `coro`, so a genuine twin at the
    /// same coroutine is already gone by the time this could matter. What is left
    /// is the pair that agrees on `(id, fp)` and disagrees on nothing the lookup
    /// checks — and being live is the signal, since a suspension takes its frame
    /// off the stack and a resume can never find its own still there.
    #[test]
    fn a_refused_park_does_not_resume_an_abandoned_twin() {
        let mut state = State::default();
        let (first_coro, second_coro) = (0x1000usize, 0x2000usize);

        // An older generator of function 7, on the coroutine stack at CORO_BASE.
        state.enter_at(7, CORO_BASE, 0, 0, 0, 0, 0);
        state.suspend_at(7, CORO_BASE, first_coro, 10, 0, 0, 0, 0);
        assert_eq!(state.parked.len(), 1);

        // The table fills with unrelated coroutines.
        for index in 1..MAX_PARKED {
            let fp = CORO_BASE + index * 64;
            state.enter_at(8, fp, index as u64, 0, 0, 0, 0);
            state.suspend_at(8, fp, fp, index as u64, 0, 0, 0, 0);
        }

        // A new generator of the SAME function reuses that stack address under a
        // different fiber, and its park is refused at the cap.
        state.enter_at(7, CORO_BASE, 2_000, 0, 0, 0, 0);
        state.suspend_at(7, CORO_BASE, second_coro, 2_010, 0, 0, 0, 0);
        assert_eq!(state.parks_refused, 1, "the cap refused it");
        assert_eq!(state.stack.len(), 1, "so its frame is still standing");

        state.resume_at(7, CORO_BASE, 2_020, 0, 0, 0, 0);
        assert_eq!(
            state.stack.len(),
            1,
            "the abandoned twin was opened on top of the live activation"
        );
        assert!(
            state.parked.iter().any(|group| group.coro == first_coro),
            "and its group is untouched, not consumed by somebody else's resume"
        );
    }

    /// A coroutine address handed to a new occupant does not resurrect the old one.
    ///
    /// `coro` is the running fiber's address, so a freed fiber's is handed to the
    /// next. At the cap the new occupant's park is refused and its frame stays on
    /// the stack; if the suspension then takes one of the runtime's non-returning
    /// paths, the unpark looks that address up — and found the ABANDONED group.
    /// The `(id, fp)` guard in `restore` does not catch that one, because the two
    /// activations are different functions and their ids do not match, so the
    /// dead group was pushed above the live activation.
    ///
    /// Driven through `suspend_at` rather than `suspend_sim` because this is the
    /// case where the frame pointer and the coroutine must differ: two different
    /// activations, one fiber address.
    #[test]
    fn a_reused_coroutine_address_does_not_resurrect_its_last_occupant() {
        let mut state = State::default();
        let old_frame = CORO_BASE;
        let new_frame = CORO_BASE - 4_096;
        let shared_coro = 0x9_000usize;

        // An older coroutine at this fiber address, function 7, never resumed.
        state.enter_at(7, old_frame, 0, 0, 0, 0, 0);
        state.suspend_at(7, old_frame, shared_coro, 10, 0, 0, 0, 0);
        assert_eq!(state.parked.len(), 1);

        // Everything else fills the table, each under its own coroutine.
        for index in 1..MAX_PARKED {
            let fp = CORO_BASE + index * 64;
            state.enter_at(8, fp, index as u64, 0, 0, 0, 0);
            state.suspend_at(8, fp, fp, index as u64, 0, 0, 0, 0);
        }

        // A new fiber takes the freed address, under a different function, and
        // suspends into one of the paths that raises instead of returning.
        state.enter_at(9, new_frame, 2_000, 0, 0, 0, 0);
        state.suspend_at(9, new_frame, shared_coro, 2_010, 0, 0, 0, 0);
        state.unpark_at(shared_coro, 2_020, 0, 0, 0, 0);

        assert_eq!(
            state.stack.iter().map(|frame| frame.id).collect::<Vec<_>>(),
            vec![9],
            "the dead occupant of this address was opened over the live one"
        );
        assert!(
            !state
                .parked
                .iter()
                .any(|group| group.frames.contains(&(7, old_frame))),
            "and its group is gone rather than waiting to be found again"
        );
    }

    /// A suspension records a timeline span, the same as an exit.
    ///
    /// Both end a span. Only the exit recorded one, so the Chrome/Perfetto
    /// output was missing exactly what the aggregate table did account for —
    /// every stretch a generator ran before a `yield`, and an abandoned
    /// generator's whole life.
    #[test]
    fn a_suspension_records_a_timeline_span() {
        let _serial = ticks_are_nanoseconds();
        let restore = TRACE_ON.swap(true, Ordering::Relaxed);
        let mut state = State::default();

        state.enter_at(0, CORO_BASE, 100, 0, 0, 0, 0);
        state.suspend_sim(0, CORO_BASE, 130, 0, 0, 0, 0);
        state.resume_at(0, CORO_BASE, 500, 0, 0, 0, 0);
        state.exit_at(0, CORO_BASE, 520, 0, 0, 0, 0);

        TRACE_ON.store(restore, Ordering::Relaxed);
        assert_eq!(
            state.trace,
            vec![(0, 100, 130), (0, 500, 520)],
            "the pre-yield stretch is missing from the timeline"
        );
    }

    /// A suspension past the cap is refused, and the refusal is reported.
    ///
    /// Parking is bounded for the reason `MAX_STACK` is: an abandoned generator
    /// is never resumed, so a program that builds them in a loop and drops them
    /// would grow this without limit. Past the cap the frame stays where it was,
    /// which is the old wrong measurement rather than a new one — and the note
    /// is what stops that from being a silent truncation.
    #[test]
    fn a_suspension_past_the_cap_is_refused_and_reported() {
        let mut state = State::default();
        for index in 0..MAX_PARKED {
            let fp = CORO_BASE + index * 64;
            state.enter_at(0, fp, index as u64, 0, 0, 0, 0);
            state.suspend_sim(0, fp, index as u64, 0, 0, 0, 0);
        }
        assert_eq!(state.parked.len(), MAX_PARKED);
        assert_eq!(state.parks_refused, 0, "everything up to the cap is parked");

        let over = CORO_BASE + MAX_PARKED * 64;
        state.enter_at(0, over, 1_000, 0, 0, 0, 0);
        state.suspend_sim(0, over, 1_000, 0, 0, 0, 0);
        assert_eq!(state.parked.len(), MAX_PARKED, "the cap holds");
        assert_eq!(state.parks_refused, 1);
        assert_eq!(
            state.stack.last().map(|frame| frame.fp),
            Some(over),
            "a refused park must leave the frame alone, not lose it"
        );

        let report = state.render(&["body".into()]);
        assert!(
            report.contains(&format!("1 suspension(s) past {MAX_PARKED} parked")),
            "a refused park was not reported: {report}"
        );
    }

    /// A coroutine stack handed back out does not resume somebody else's frames.
    ///
    /// A freed coroutine stack is reused, so a frame pointer names an activation
    /// only among the live ones — the same reason `exit_at` pairs it with the id
    /// rather than trusting it alone. Here an abandoned generator's group is
    /// still parked when a different function starts on its address.
    #[test]
    fn a_resume_at_a_reused_coroutine_address_restores_nothing() {
        let mut state = State::default();
        state.enter_at(7, CORO_BASE, 0, 0, 0, 0, 0);
        state.suspend_sim(7, CORO_BASE, 10, 0, 0, 0, 0);
        assert_eq!(state.parked.len(), 1);

        // Another function, the same address, never parked.
        state.resume_at(9, CORO_BASE, 20, 0, 0, 0, 0);
        assert_eq!(
            state.parked.len(),
            1,
            "one coroutine's frames were handed to another"
        );
        assert!(state.stack.is_empty(), "and nothing was pushed for it");

        // Its rightful owner still finds them.
        state.resume_at(7, CORO_BASE, 20, 0, 0, 0, 0);
        assert_eq!(state.stack.len(), 1);
        assert!(state.parked.is_empty());
    }

    /// A suspension from inside a nested call parks only the inner frame.
    ///
    /// The whole coroutine stack suspends, but what this is told is one frame
    /// pointer, and the frames below it on that stack are indistinguishable from
    /// the consumer's without a record of where the coroutine began. So the
    /// outer body keeps running in the profile, exactly as it did before any of
    /// this — no better and no worse.
    ///
    /// Written down as a test rather than as a comment because a sentence
    /// claiming a limit is the thing this branch keeps finding to be wrong. If
    /// the coroutine's root is ever recorded, this test is what changes.
    #[test]
    fn a_suspension_from_a_nested_call_parks_only_the_inner_frame() {
        let mut state = State::default();
        // body() on the coroutine stack, then inner() above it, which suspends.
        state.enter_at(0, CORO_BASE, 0, 0, 0, 0, 0);
        state.enter_at(1, CORO_BASE - 64, 10, 0, 0, 0, 0);
        state.suspend_sim(1, CORO_BASE - 64, 20, 0, 0, 0, 0);

        assert_eq!(state.parked.len(), 1);
        assert_eq!(
            state.stack.last().map(|frame| frame.id),
            Some(0),
            "the outer body is still standing, and is still read as the caller"
        );
    }

    /// A dropped activation is recognised whatever order its exit arrives in.
    ///
    /// Matching only the newest entry assumes dropped activations exit in the
    /// order they were entered. That is true of ordinary calls, and it is an
    /// assumption about the whole language rather than about this function —
    /// reason enough not to rest on it. Where it fails the entry left behind is
    /// not dead weight: a later TRACKED frame reusing that address matches it,
    /// returns early, and is never popped, so every measurement after it is
    /// attributed to a frame that already returned.
    ///
    /// Set up under an unwind, because that is the only state in which identities
    /// are recorded at all — outside it every frame on the shadow stack is live,
    /// so an unknown frame pointer closes nothing on its own.
    #[test]
    fn a_dropped_exit_out_of_order_leaves_nothing_behind() {
        let mut state = State::default();
        for depth in 0..MAX_STACK {
            state.enter_sim((depth % 3) as u32, depth as u64, 0, 0, 0, 0);
        }
        // A throw puts stale frames on the stack; from here a dropped
        // activation's address can alias one of them.
        state.note_throw(5, 0, 0, 0, 0);

        let first = 0x5000usize;
        let second = 0x6000usize;
        state.enter_at(8, first, 10, 0, 0, 0, 0);
        state.enter_at(9, second, 20, 0, 0, 0, 0);
        assert_eq!(state.dropped_fps, vec![first, second]);

        state.exit_at(8, first, 30, 0, 0, 0, 0);
        assert_eq!(
            state.dropped_fps,
            vec![second],
            "an out-of-order dropped exit must take its own identity with it"
        );
        state.exit_at(9, second, 40, 0, 0, 0, 0);
        assert!(state.dropped_fps.is_empty());
        assert_eq!(state.stack.len(), MAX_STACK, "and neither touched the stack");
    }

    /// Nothing is recorded when no unwind is in flight.
    ///
    /// An identity exists for one reason: the shadow stack can hold frames that
    /// have RETURNED, whose addresses the native stack has handed back out. That
    /// is true between a throw and its catcher exiting and at no other time.
    /// Recording unconditionally made a second shadow stack — a word per
    /// activation past the cap, with the vector keeping its peak for the life of
    /// the thread, which is the cost `MAX_STACK` exists to prevent.
    #[test]
    fn dropped_identities_cost_nothing_outside_an_unwind() {
        let mut state = State::default();
        for depth in 0..MAX_STACK {
            state.enter_sim((depth % 3) as u32, depth as u64, 0, 0, 0, 0);
        }

        // No throw: every frame here is live, so an unknown frame pointer closes
        // nothing on its own and needs no identity.
        state.enter_at(8, 0x5000, 10, 0, 0, 0, 0);
        assert!(
            state.dropped_fps.is_empty(),
            "an identity was recorded with nothing stale for it to guard against"
        );
        assert_eq!(state.dropped_fps.capacity(), 0, "and nothing was allocated");

        let before = state.stack.len();
        state.exit_at(8, 0x5000, 20, 0, 0, 0, 0);
        assert_eq!(state.stack.len(), before, "its exit still closes nothing");

        // And the capacity is handed back when the unwind that needed it ends,
        // rather than kept for the life of the thread.
        state.note_throw(30, 0, 0, 0, 0);
        state.enter_at(9, 0x6000, 40, 0, 0, 0, 0);
        assert_eq!(state.dropped_fps, vec![0x6000usize]);
        let catcher = state.stack[0].fp;
        let catcher_id = state.stack[0].id;
        state.exit_at(catcher_id, catcher, 50, 0, 0, 0, 0);
        assert!(state.unwinding.is_none(), "the catcher ended the unwind");
        assert_eq!(
            state.dropped_fps.capacity(),
            0,
            "the list outlived the unwind that gave it a reason"
        );
    }

    /// A throw clears the dropped identities even with nothing on the stack.
    ///
    /// The clear used to sit after the depth guard, so a throw raised with an
    /// empty shadow stack returned first and left whatever was recorded. Nothing
    /// can be live above an empty stack, so an identity still there is stale by
    /// definition — and stale is the one thing this list must never hold.
    #[test]
    fn a_throw_on_an_empty_stack_still_forgets_dropped_identities() {
        let mut state = State::default();
        state.dropped_fps.push(0x7000);
        state.note_throw(10, 0, 0, 0, 0);
        assert!(
            state.dropped_fps.is_empty(),
            "a throw at depth zero kept an identity nothing can vouch for"
        );
    }

    /// A TRACKED call reusing a stale frame's address closes only itself.
    ///
    /// The dropped case needed its own record; this is the other half of the
    /// same collision, and it holds for a reason worth stating rather than
    /// assuming. Frames an unwind destroyed sit above the catcher and below
    /// anything the handler calls afterwards, because a new frame is pushed on
    /// top of the whole stack. So when two frames carry one address, the live one
    /// is always the HIGHER index, and a search from the top finds it first.
    ///
    /// Nothing pinned that before: the comment claimed such a comparison never
    /// happened at all, which an unwind disproves. It happens, and the ordering
    /// is what makes it come out right.
    #[test]
    fn a_tracked_call_reusing_a_stale_address_closes_only_itself() {
        let mut state = State::default();
        state.enter_sim(0, 10, 0, 0, 0, 0);
        let catcher = state.stack[0].fp;
        state.enter_sim(1, 20, 0, 0, 0, 0);
        let stale = state.stack[1].fp;
        state.enter_sim(2, 30, 0, 0, 0, 0);

        // Raised at the top, caught at the root: frames 1 and 2 are dead and
        // still on the shadow stack.
        state.note_throw(40, 0, 0, 0, 0);
        assert_eq!(state.stack.len(), 3);

        // The handler calls something, and the native stack hands it the address
        // frame 1 used to occupy. This one is TRACKED — the stack is not full.
        state.enter_at(7, stale, 50, 0, 0, 0, 0);
        assert_eq!(state.stack.len(), 4, "a tracked call is pushed");
        assert_eq!(
            state.stack.iter().filter(|f| f.fp == stale).count(),
            2,
            "two frames now carry one address, which is the whole point"
        );

        // Its exit must close itself and nothing else — not the stale twin, and
        // not the frames between.
        state.exit_at(7, stale, 60, 0, 0, 0, 0);
        assert_eq!(
            state.stack.len(),
            3,
            "the exit closed frames belonging to the stale twin"
        );
        assert_eq!(state.stack[0].fp, catcher, "and the catcher is untouched");
    }

    /// A throw forgets the dropped activations it destroyed.
    ///
    /// They will never exit, so a record of them left standing is not merely
    /// stale — it is a trap. The next dropped call reuses the address the unwind
    /// freed, and its own exit would be matched against the dead entry and
    /// swallowed, leaving ITS identity behind for the one after that. The list is
    /// cleared at the throw for the same reason the old count was: an entry that
    /// can never be closed must not be left to close somebody else's.
    #[test]
    fn a_throw_forgets_the_dropped_activations_it_destroyed() {
        let mut state = State::default();
        for depth in 0..MAX_STACK {
            state.enter_sim((depth % 3) as u32, depth as u64, 0, 0, 0, 0);
        }
        let reused = state.stack[1].fp;

        // Dropped past the cap, then destroyed by the throw before it can exit.
        state.enter_at(9, reused, 1_000, 0, 0, 0, 0);
        state.note_throw(1_500, 0, 0, 0, 0);
        assert!(
            state.dropped_fps.is_empty(),
            "an activation the unwind destroyed is still expected to exit"
        );

        // The handler now calls something at the very same address. Its exit is
        // its own, and must be recognised rather than charged to the ghost.
        state.enter_at(10, reused, 1_600, 0, 0, 0, 0);
        let before = state.stack.len();
        state.exit_at(10, reused, 1_700, 0, 0, 0, 0);
        assert_eq!(state.stack.len(), before, "the live stack was disturbed");
        assert!(
            state.dropped_fps.is_empty(),
            "and its own identity was consumed, not left for the next one"
        );
    }

    /// What a catch handler does belongs to the catcher, not to the thrower.
    ///
    /// The frames an exception unwound never run their own exit, so they are
    /// closed when the catcher exits — and were closed AT that moment, which
    /// charged them everything the handler did on the way. They are now closed
    /// at the instant of the throw.
    #[test]
    fn a_catch_handler_is_not_charged_to_the_frames_it_unwound() {
        let mut state = State::default();
        state.enter_sim(0, 0, 0, 0, 0, 0); // catcher
        state.enter_sim(1, 10, 0, 0, 0, 0); // thrower, runs 10..20

        state.note_throw(20, 0, 0, 0, 0);
        // The handler then works for a long time before the catcher returns.
        state.exit_sim(0, 1_020, 0, 0, 0, 0);

        let thrower = state.fns[1].incl_ns;
        assert_eq!(
            thrower, 10,
            "the thrower should carry its own 10 ticks, not the handler's 1000"
        );
        assert!(
            state.fns[0].incl_ns >= 1_020,
            "the catcher should carry the whole span it was on the stack for"
        );
    }

    /// The same, when the handler CALLS something — which is what a handler
    /// normally does.
    ///
    /// The test above lets the catcher return without running another
    /// instrumented function, and that is the one shape where the unwind
    /// bookkeeping is never disturbed. A handler that logs, retries, or builds
    /// a response goes through this path instead.
    #[test]
    fn a_catch_handler_that_calls_something_still_leaves_the_unwound_frames_alone() {
        let mut state = State::default();
        state.enter_sim(0, 0, 0, 0, 0, 0); // catcher
        state.enter_sim(1, 10, 0, 0, 0, 0); // thrower, runs 10..20
        state.note_throw(20, 0, 0, 0, 0);

        // The handler calls one instrumented function, which returns normally.
        state.enter_sim(2, 30, 0, 0, 0, 0);
        state.exit_sim(2, 1_000, 0, 0, 0, 0);
        state.exit_sim(0, 1_020, 0, 0, 0, 0);

        assert_eq!(
            state.fns[1].incl_ns, 10,
            "the thrower should carry its own 10 ticks, not the handler's work"
        );
        assert_eq!(
            state.fns[2].excl_ns, 970,
            "the function the handler called should carry its own time"
        );
        assert!(
            state.fns[1].excl_ns < 1_000,
            "the thrower's self time should not include the handler's work              (got {})",
            state.fns[1].excl_ns
        );
    }

    /// Winning the slot closes the window behind it.
    ///
    /// Every request that starts while a capture is armed pays for a slice, and
    /// only one can be handed over. Leaving it armed kept the rest paying, and
    /// their refused offers fell through to the service log.
    #[test]
    fn the_worker_that_wins_the_slot_disarms_the_capture() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        reset_capture();
        elephc_instr_capture_arm();
        assert_eq!(
            capture_word(CAPTURE_ARMED_WORD)
                .expect("mapped")
                .load(Ordering::Acquire),
            1,
            "arming did not arm"
        );

        assert!(super::offer_capture("elephc-instr: won calls=1 incl_ns=5\n"));

        assert_eq!(
            capture_word(CAPTURE_ARMED_WORD)
                .expect("mapped")
                .load(Ordering::Acquire),
            0,
            "the capture stayed armed after its answer was in hand"
        );
        // And a later request finds nothing to start a slice for.
        assert!(
            !super::offer_capture("elephc-instr: late calls=1 incl_ns=5\n"),
            "a second profile was accepted after the answer was taken"
        );
        reset_capture();
    }

    /// A slice that exists only to answer the endpoint is never logged.
    ///
    /// Under `--web` every request in the armed window opens one, and all but the
    /// first find the slot taken. The dump's own contract is that such a slice is
    /// "an answer to a question, not a log line".
    #[test]
    fn an_endpoint_slice_that_loses_the_race_is_not_written_to_the_log() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        reset_capture();

        // A capture is armed and this request opens a slice for it.
        elephc_instr_capture_arm();
        elephc_instr_request(2);
        assert!(
            CAPTURE_ONLY.load(Ordering::Relaxed),
            "a slice opened for the endpoint was not marked as one"
        );

        // Another worker got there first.
        assert!(super::offer_capture("elephc-instr: other calls=1 incl_ns=5\n"));

        // This one has nothing to hand over, and nothing to say either.
        elephc_instr_request(0);
        assert!(
            !CAPTURE_ONLY.load(Ordering::Relaxed),
            "the marking outlived the slice it described"
        );

        // A slice opened because the binary is being profiled outright still is
        // a log line, as it always was.
        elephc_instr_request(1);
        assert!(!CAPTURE_ONLY.load(Ordering::Relaxed));
        elephc_instr_request(0);
        reset_capture();
    }

    /// Claiming the slot publishes pid and process-start identity together.
    #[test]
    fn a_claim_names_the_exact_process_incarnation_in_one_store() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        reset_capture();
        elephc_instr_capture_arm();

        let owner = capture_owner().expect("mapped");
        assert_eq!(owner.load(Ordering::Acquire), EMPTY, "nothing claimed yet");

        let mine = current_capture_owner().expect("this supported target identifies a process");
        let (pid, start_id) = unpack_capture_owner(mine).expect("a real owner token");
        assert_eq!(pid, std::process::id() as i32);
        assert_eq!(start_id, process_start_id(pid).expect("stable start identity"));

        owner.store(mine, Ordering::Release);
        assert!(
            !super::offer_capture("a second worker's profile"),
            "a claimed slot must refuse a second writer"
        );
        assert_eq!(
            owner.load(Ordering::Acquire),
            mine,
            "the refusal must leave the complete owner identity untouched"
        );

        owner.store(EMPTY, Ordering::Release);
        assert!(super::offer_capture("elephc-instr: probe calls=1 incl_ns=5\n"));
        assert_eq!(
            owner.load(Ordering::Acquire),
            READY,
            "a finished offer publishes the payload"
        );
        assert_eq!(READY, u64::MAX);
        assert!(EMPTY != READY);
    }

    /// A writer suspended between claim and payload publication cannot be
    /// reclaimed, cannot admit a successor, and resumes into its own slot.
    #[test]
    fn a_suspended_writer_cannot_be_reclaimed_or_overwritten() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        reset_capture();
        elephc_instr_capture_arm();

        let owner = capture_owner().expect("mapped");
        let mine = current_capture_owner().expect("this supported target identifies a process");
        assert_eq!(
            owner.compare_exchange(EMPTY, mine, Ordering::AcqRel, Ordering::Acquire),
            Ok(EMPTY),
            "stage the first writer immediately after its claim"
        );

        // The endpoint intervenes while that writer is frozen. It may clean a
        // previous READY answer, but a live matching owner must survive.
        let claimed_at = capture_word(CAPTURE_CLAIMED_AT_WORD).expect("mapped");
        release_stale_slice(owner, claimed_at);
        assert_eq!(owner.load(Ordering::Acquire), mine);
        assert!(
            !offer_capture("successor must not enter"),
            "the reclaimer illegally admitted a second writer"
        );
        let (_, recorded_start_id) = unpack_capture_owner(mine).expect("a real owner token");
        assert!(
            !super::claim_may_be_taken(true, None, recorded_start_id),
            "a live claimer nobody can identify keeps its slot: the age says \
             nothing about it, and the alternative is two writers in one payload"
        );

        let offered_for = capture_word(CAPTURE_EPOCH_WORD)
            .expect("mapped")
            .load(Ordering::Acquire);
        assert!(publish_capture("first writer resumes", mine, offered_for));
        let needed = unsafe { elephc_instr_capture_take(std::ptr::null_mut(), 0) };
        let mut bytes = vec![0; needed];
        assert_eq!(
            unsafe { elephc_instr_capture_take(bytes.as_mut_ptr(), bytes.len()) },
            needed
        );
        assert_eq!(String::from_utf8(bytes).unwrap(), "first writer resumes");
    }

    /// Only the winner of a claim publishes its time.
    ///
    /// The time used to be stamped BEFORE the compare-exchange, defended on the
    /// grounds that a loser only makes a claim look fresher and so delays
    /// reclamation by a moment. That is true of one loser and false of a stream
    /// of them: under `--web` every request reaching the offer stamps the age of
    /// whatever claim is standing, so a claim held by a pid that has since been
    /// recycled never reaches the backstop that exists to recover it. The
    /// rendezvous stayed blocked for as long as the service kept serving —
    /// exactly when an operator wants it.
    ///
    /// Reading the source because the property is an ORDER between statements,
    /// which no executable test can schedule reliably.
    #[test]
    fn a_losing_offer_does_not_refresh_someone_elses_claim() {
        let source = include_str!("lib.rs");
        let offer = source
            .split_once("fn offer_capture_for(")
            .expect("the offer path must exist")
            .1;
        let body = offer.split_once("\n}\n").expect("a function body").0;
        let claim = body
            .find(".compare_exchange(EMPTY, mine")
            .expect("the claim must be a compare-exchange to this pid");
        let stamp = body
            .find("claimed_at.store(monotonic_seconds()")
            .expect("the claim's time must be published somewhere");
        assert!(
            claim < stamp,
            "the time is published by the winner, after winning — a loser that \
             stamped first would keep another process's claim permanently young"
        );

        // If a reclaimer cannot read the current start identity, age still does
        // not authorize it to revoke the indivisible owner token.
        let recorded = unpack_capture_owner(
            current_capture_owner().expect("this supported target identifies a process"),
        )
        .expect("a real owner token")
        .1;
        assert!(
            !super::claim_may_be_taken(true, None, recorded),
            "an unidentifiable live claim is kept whatever its age reads"
        );
    }

    /// Dead and recycled owners are reclaimable immediately, while an unknown
    /// identity never grants permission to revoke a possibly suspended writer.
    #[test]
    fn stale_claim_decisions_use_liveness_and_start_identity_without_timeouts() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        assert!(claim_may_be_taken(false, None, 7), "dead means stale");
        assert!(claim_may_be_taken(true, Some(8), 7), "reused pid means stale");
        assert!(!claim_may_be_taken(true, Some(7), 7), "same live process owns it");
        assert!(
            !claim_may_be_taken(true, None, 7),
            "unknown identity must not revoke a suspended live writer"
        );

        reset_capture();
        let owner = capture_owner().expect("mapped");
        let pid = std::process::id();
        let actual = process_start_id(pid as i32).expect("supported process identity");
        let recycled = if actual == 1 { 2 } else { actual - 1 };
        let claimed_at = capture_word(CAPTURE_CLAIMED_AT_WORD).expect("mapped");
        owner.store(pack_capture_owner(pid, recycled).unwrap(), Ordering::Release);
        release_stale_slice(owner, claimed_at);
        assert_eq!(owner.load(Ordering::Acquire), EMPTY, "a reused pid is released");

        owner.store(
            pack_capture_owner(i32::MAX as u32, 1).unwrap(),
            Ordering::Release,
        );
        release_stale_slice(owner, claimed_at);
        assert_eq!(owner.load(Ordering::Acquire), EMPTY, "a dead pid is released");

        owner.store(READY, Ordering::Release);
        release_stale_slice(owner, claimed_at);
        assert_eq!(
            owner.load(Ordering::Acquire),
            EMPTY,
            "a completed slice nobody will take is released"
        );
    }

    /// The supported kernel identifies this process stably enough to construct
    /// the owner token before publishing the claim.
    #[test]
    fn a_claim_can_identify_this_process_incarnation() {
        let mine = std::process::id() as i32;
        let first = super::process_start_id(mine).expect("this platform must identify a process");
        assert_ne!(first, 0, "0 is reserved out of owner tokens");
        assert_eq!(first, super::process_start_id(mine).unwrap(), "and be stable");
        assert_eq!(
            super::process_start_id(-1),
            None,
            "a pid that cannot exist has no identity"
        );
        let token = pack_capture_owner(mine as u32, first).expect("valid owner token");
        assert_eq!(unpack_capture_owner(token), Some((mine, first)));
    }

    /// `process_is_alive` answers about this process and about no process.
    ///
    /// Only the two ends are testable: a test cannot hold a pid that is
    /// reliably dead and unreused, and must not signal an unrelated one.
    #[test]
    fn liveness_recognises_this_process_and_rejects_a_non_pid() {
        assert!(
            super::process_is_alive(std::process::id() as i32),
            "the running test is alive by any definition"
        );
        assert!(!super::process_is_alive(0), "0 is not a pid this asks about");
        assert!(!super::process_is_alive(-1), "nor is a negative, which kill() reads as a group");
    }

    /// A take that cannot fit reports the length and consumes NOTHING.
    ///
    /// This is the contract the endpoint's reader leans on. It peeks the length,
    /// allocates that much, and takes; if the published length has grown in
    /// between, the take reports the new one and leaves the slice in place. The
    /// reader used to `truncate(written.min(needed))` and return the buffer
    /// regardless — which in that case is the zeros it was allocated with, and a
    /// run of NUL bytes is valid UTF-8, so it was served as the profile. The
    /// reader now retries instead, and that only works if the slice really is
    /// still there afterwards, which is what this pins.
    ///
    /// A length that moves while the slot reads READY takes a misbehaving or
    /// corrupted header — the region is shared and writable, which is why the
    /// take validates rather than trusts it. This is hardening, not a path a
    /// well-behaved writer reaches.
    #[test]
    fn a_take_that_does_not_fit_leaves_the_slice_where_it_was() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        reset_capture();
        elephc_instr_capture_arm();

        let slice = "elephc-instr: probe calls=1 incl_ns=5\n";
        assert!(super::offer_capture(slice), "an armed capture refused a slice");

        let needed = unsafe { elephc_instr_capture_take(std::ptr::null_mut(), 0) };
        assert_eq!(needed, slice.len(), "the peek reports the slice's length");

        // Ask with one byte less than the slice needs, the shape a reader whose
        // peek is now out of date presents.
        let mut small = vec![0u8; needed - 1];
        let written = unsafe { elephc_instr_capture_take(small.as_mut_ptr(), needed - 1) };
        assert_eq!(written, needed, "it reports what the slice actually needs");
        assert!(
            small.iter().all(|byte| *byte == 0),
            "nothing may be copied into a buffer that cannot hold the slice"
        );

        // And the slice survived, so the retry the reader now performs finds it.
        let mut big = vec![0u8; needed];
        let written = unsafe { elephc_instr_capture_take(big.as_mut_ptr(), needed) };
        assert_eq!(written, needed);
        assert_eq!(
            String::from_utf8(big).expect("the slice is text"),
            slice,
            "the retry must get the slice itself, not a buffer of zeros"
        );
    }

    /// A slice for a capture that has ended costs the next capture nothing.
    ///
    /// `armed` is a yes/no, so a capture that timed out and the one that armed
    /// after it read the same to a worker. A worker descheduled between reading
    /// it and claiming the slot published a slice rendered for the FIRST capture,
    /// and the second received it — a complete, plausible profile of a request
    /// that finished before the operator asked.
    ///
    /// Discarding that slice and stopping there was worse than what it replaced:
    /// the stale worker had already stored `armed = 0`, so no later request
    /// offered, and the endpoint waited out its timeout and answered "no request
    /// completed" while requests were completing.
    ///
    /// Driven through `offer_capture_for`, which is the production path with the
    /// stale identity injected. The version of this test that came with the first
    /// fix wrote the header words by hand, bypassing the disarm, and so asserted
    /// "the capture stays armed" while the real path was clearing it.
    #[test]
    fn a_slice_for_a_finished_capture_costs_the_next_one_nothing() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        reset_capture();

        elephc_instr_capture_arm();
        let stale = capture_word(CAPTURE_EPOCH_WORD)
            .expect("a mapped region")
            .load(Ordering::Acquire);

        elephc_instr_capture_cancel();
        elephc_instr_capture_arm();
        let live = capture_word(CAPTURE_EPOCH_WORD)
            .expect("a mapped region")
            .load(Ordering::Acquire);
        assert_ne!(stale, live, "each arm takes a new identity");

        let slice = "elephc-instr: stale calls=1 incl_ns=5\n";
        assert!(
            !super::offer_capture_for(slice, stale),
            "a slice for a capture that ended must not be published"
        );
        assert_eq!(
            capture_owner().expect("a mapped region").load(Ordering::Acquire),
            EMPTY,
            "and the claim must be given back"
        );
        assert_ne!(
            capture_word(CAPTURE_ARMED_WORD)
                .expect("a mapped region")
                .load(Ordering::Acquire),
            0,
            "B is still waiting for its own request, so it must still be armed"
        );

        assert!(super::offer_capture(slice), "B was starved by the stale offer");
        let needed = unsafe { elephc_instr_capture_take(std::ptr::null_mut(), 0) };
        assert_eq!(needed, slice.len(), "B's own slice was withheld");
    }

    /// A stale slice that reaches the reader anyway leaves the capture armed.
    ///
    /// The offer path refuses to publish for a capture that has ended, but its
    /// check and its store are two instructions apart, so a slice can still land,
    /// written by a worker that disarmed on the way. Discarding it and stopping
    /// there leaves the caller polling a flag nobody will answer.
    #[test]
    fn a_stale_slice_that_reaches_the_reader_leaves_the_capture_armed() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        reset_capture();
        elephc_instr_capture_arm();
        let stale = capture_word(CAPTURE_EPOCH_WORD)
            .expect("a mapped region")
            .load(Ordering::Acquire);
        elephc_instr_capture_cancel();
        elephc_instr_capture_arm();

        let slice = "elephc-instr: stale calls=1 incl_ns=5\n";
        let base = CAPTURE_REGION.load(Ordering::Acquire);
        unsafe {
            std::ptr::copy_nonoverlapping(
                slice.as_ptr(),
                (base + CAPTURE_HEADER) as *mut u8,
                slice.len(),
            );
        }
        capture_word(CAPTURE_LENGTH_WORD)
            .expect("a mapped region")
            .store(slice.len() as u32, Ordering::Relaxed);
        capture_word(SLICE_EPOCH_WORD)
            .expect("a mapped region")
            .store(stale, Ordering::Relaxed);
        capture_word(CAPTURE_ARMED_WORD)
            .expect("a mapped region")
            .store(0, Ordering::Release);
        capture_owner()
            .expect("a mapped region")
            .store(READY, Ordering::Release);

        assert_eq!(
            unsafe { elephc_instr_capture_take(std::ptr::null_mut(), 0) },
            0,
            "the next capture was served the previous one's slice"
        );
        assert_ne!(
            capture_word(CAPTURE_ARMED_WORD)
                .expect("a mapped region")
                .load(Ordering::Acquire),
            0,
            "the caller was left polling a flag nobody will answer"
        );
        assert!(super::offer_capture(slice), "and its own request can be answered");
    }

    /// The READER refuses a length the mapping cannot back, whoever wrote it.
    ///
    /// The writer's own guard is tested below, but that only proves this crate's
    /// writer behaves. The header is shared, writable memory: the check exists
    /// because the reader must not trust ANY writer, and nothing exercised it,
    /// because nothing in the test suite ever wrote a bad length. A length past
    /// the region is a copy past the end of the mapping — it would be read as a
    /// profile, not as a fault.
    #[test]
    fn the_reader_refuses_a_length_the_mapping_cannot_back() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        reset_capture();
        elephc_instr_capture_arm();

        // Written straight into the header, which is what a corrupted or hostile
        // writer sharing this mapping would do.
        let owner = capture_owner().expect("a mapped region");
        let length = capture_word(CAPTURE_LENGTH_WORD).expect("a mapped region");
        // Offered for the capture that is actually running, so this reaches the
        // length check rather than being discarded as a stale slice first.
        let current = capture_word(CAPTURE_EPOCH_WORD)
            .expect("a mapped region")
            .load(Ordering::Acquire);
        capture_word(SLICE_EPOCH_WORD)
            .expect("a mapped region")
            .store(current, Ordering::Release);
        length.store((CAPTURE_BYTES - CAPTURE_HEADER + 1) as u32, Ordering::Release);
        owner.store(READY, Ordering::Release);

        let needed = unsafe { elephc_instr_capture_take(std::ptr::null_mut(), 0) };
        assert_eq!(needed, 0, "the reader accepted a length past the mapping");
        assert_eq!(
            owner.load(Ordering::Acquire),
            EMPTY,
            "and left the slot claimed by a length nobody can honour"
        );
        assert_eq!(
            capture_word(CAPTURE_ARMED_WORD)
                .expect("a mapped region")
                .load(Ordering::Acquire),
            0,
            "an unusable slice must also stop the caller waiting for one"
        );
    }

    #[test]
    fn a_profile_too_large_to_carry_does_not_publish_a_length_it_cannot_back() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        reset_capture();
        elephc_instr_capture_arm();

        let huge = "x".repeat(CAPTURE_BYTES * 2);
        assert!(super::offer_capture(&huge), "an armed capture refused a slice");

        let needed = unsafe { elephc_instr_capture_take(std::ptr::null_mut(), 0) };
        assert!(
            needed > 0 && needed <= CAPTURE_BYTES - CAPTURE_HEADER,
            "the reader was told to read {needed} bytes out of a {} byte region",
            CAPTURE_BYTES - CAPTURE_HEADER
        );

        let mut buffer = vec![0u8; needed];
        let written = unsafe { elephc_instr_capture_take(buffer.as_mut_ptr(), needed) };
        assert_eq!(written, needed);
        let text = String::from_utf8(buffer).expect("the note was not text");
        assert!(
            text.contains("larger than"),
            "the reader got no explanation, only silence: {text:?}"
        );
        // The framing, not just the sentence. Both readers recognise a reason by
        // this prefix and drop everything else as an unparseable metric row, so a
        // correct explanation without it never reaches the operator: they were
        // told "no slice arrived within the wait", the one reason it was not.
        // Asserting only `contains("larger than")` is what let that ship.
        assert!(
            text.starts_with("elephc-instr: note: "),
            "the explanation must carry the prefix the readers strip: {text:?}"
        );
    }

    /// The copy checks the length itself, whoever wrote it.
    ///
    /// The header lives in shared, writable memory that every worker can reach,
    /// so "the producer would never write that" is not a property this side can
    /// rely on before copying out of the mapping.
    #[test]
    fn a_length_the_mapping_cannot_back_is_refused_rather_than_copied() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        reset_capture();
        elephc_instr_capture_arm();
        assert!(super::offer_capture("elephc-instr: small\n"));

        capture_word(CAPTURE_LENGTH_WORD)
            .expect("mapped")
            .store((CAPTURE_BYTES * 4) as u32, Ordering::Relaxed);

        assert_eq!(
            unsafe { elephc_instr_capture_take(std::ptr::null_mut(), 0) },
            0,
            "a length past the end of the mapping was accepted"
        );
    }

    /// Puts the rendezvous back to untouched.
    ///
    /// `capture_cancel` deliberately will not do this: it leaves a slot a worker
    /// is mid-copy into alone, because clearing one is how two profiles ended up
    /// written over each other. Setup needs the unconditional version, and saying
    /// so in the header rather than through the API keeps the difference visible.
    /// Clears the WHOLE rendezvous header between tests.
    ///
    /// It used to clear the first four words, which was the whole header when it
    /// was written and has not been since. Every word added after that — the
    /// claimer's identity, the capture identities — leaked from one test into the
    /// next, and the tests share this mapping. That surfaced as a test failing
    /// for a state some earlier test had left behind, which is the least useful
    /// kind of red there is. Derived from the header's size so it cannot fall
    /// behind again.
    fn reset_capture() {
        map_capture_region();
        for index in [
            CAPTURE_ARMED_WORD,
            CAPTURE_LENGTH_WORD,
            CAPTURE_CLAIMED_AT_WORD,
            CAPTURE_EPOCH_WORD,
            SLICE_EPOCH_WORD,
        ] {
            capture_word(index).expect("mapped").store(0, Ordering::Relaxed);
        }
        capture_owner()
            .expect("mapped")
            .store(EMPTY, Ordering::Release);
        HELD_BY.store(0, Ordering::Relaxed);
    }

    /// Two workers reach an armed slot at once. Only one may write it.
    ///
    /// The interleaving is staged rather than raced: the loser is the worker that
    /// arrives while the winner is mid-copy, and a threaded test would only
    /// sometimes produce that. The claimed state is set outright here, which is
    /// exactly what the winner leaves behind while it copies.
    #[test]
    fn a_claimed_capture_slot_is_neither_written_again_nor_read_early() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        reset_capture();
        elephc_instr_capture_arm();

        // A worker has claimed the slot and is still copying into it.
        let other_worker = pack_capture_owner(std::process::id() + 1, 1).unwrap();
        capture_owner()
            .expect("mapped")
            .store(other_worker, Ordering::Release);

        assert!(
            !super::offer_capture("a second worker's profile"),
            "two workers were allowed to write the same slot"
        );
        assert_eq!(
            unsafe { elephc_instr_capture_take(std::ptr::null_mut(), 0) },
            0,
            "a slice still being written was served as complete"
        );
        // A claim this fresh outlives `cancel` by design, so it has to be cleared
        // here rather than left for the next test to trip over.
        reset_capture();
    }

    /// The same property under real concurrency: many threads, one winner.
    #[test]
    fn only_one_of_many_concurrent_offers_wins_the_slot() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        reset_capture();
        elephc_instr_capture_arm();

        let start = std::sync::Barrier::new(8);
        let winners = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for i in 0..8 {
                let (start, winners) = (&start, &winners);
                s.spawn(move || {
                    // Distinct lengths, so a mixture of two would be visible.
                    let slice = format!("elephc-instr: w{i} {}\n", "x".repeat(i * 4096));
                    start.wait();
                    if super::offer_capture(&slice) {
                        winners.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }
        });

        assert_eq!(
            winners.load(Ordering::Relaxed),
            1,
            "the slot accepted more than one profile"
        );
        let needed = unsafe { elephc_instr_capture_take(std::ptr::null_mut(), 0) };
        let mut buffer = vec![0u8; needed];
        unsafe { elephc_instr_capture_take(buffer.as_mut_ptr(), needed) };
        let text = String::from_utf8(buffer).expect("the served slice was not one profile");
        assert!(
            text.starts_with("elephc-instr: w") && text.ends_with('\n'),
            "the served slice is a mixture, not one worker's profile"
        );
    }

    /// A handler's calls belong to the catcher in the graph as well as in the
    /// numbers.
    ///
    /// `enter_at` names the caller from the top of the stack, and during an
    /// unwind the top of the stack is a frame the exception already destroyed.
    #[test]
    fn a_call_made_by_a_handler_is_an_edge_from_the_catcher() {
        let mut state = State::default();
        state.enter_sim(0, 0, 0, 0, 0, 0); // catcher
        state.enter_sim(1, 10, 0, 0, 0, 0); // thrower
        state.note_throw(20, 0, 0, 0, 0);
        state.enter_sim(2, 30, 0, 0, 0, 0); // what the handler calls
        state.exit_sim(2, 900, 0, 0, 0, 0);
        state.exit_sim(0, 1_000, 0, 0, 0, 0);

        assert_eq!(
            state.edges.get(&(0, 2)),
            Some(&(1, 870)),
            "the handler's call should be an edge from the catcher"
        );
        assert!(
            !state.edges.contains_key(&(1, 2)),
            "the handler's call was attributed to the function that threw"
        );
    }

    /// Self time still partitions the root when a handler does the work.
    ///
    /// This is the invariant a wrong fix breaks silently: closing the dead frames
    /// at the throw while their children are charged to them underflows the
    /// subtraction, and the thrower reports about 1.8e19 ns of self time.
    #[test]
    fn a_handlers_work_does_not_underflow_the_frame_it_unwound() {
        let mut state = State::default();
        state.enter_sim(0, 0, 0, 0, 0, 0);
        state.enter_sim(1, 10, 0, 0, 0, 0);
        state.note_throw(20, 0, 0, 0, 0);
        state.enter_sim(2, 30, 0, 0, 0, 0);
        state.exit_sim(2, 900, 0, 0, 0, 0);
        state.exit_sim(0, 1_000, 0, 0, 0, 0);

        let total: u64 = state.fns.iter().map(|a| a.excl_ns).sum();
        assert_eq!(
            total, 1_000,
            "self time should partition the root's 1000 ticks, not overflow it"
        );
        assert_eq!(state.fns[1].excl_ns, 10, "the thrower ran for 10 ticks");
        assert_eq!(state.fns[2].excl_ns, 870, "the handler's call ran for 870");
        assert_eq!(state.fns[0].excl_ns, 120, "the catcher owns the rest");
    }

    /// The heap dimension gets the same treatment as the clock.
    ///
    /// A frame the exception destroyed is closed at the instant of the throw. It
    /// was closed at the throw's CLOCK but the catcher's allocation counter, so
    /// every object the handler allocated was charged to the function that threw
    /// — and subtracted from the one that caught, which then reported allocating
    /// nothing at all.
    #[test]
    fn an_unwound_frame_does_not_inherit_the_handlers_allocations() {
        let mut state = State::default();
        state.enter_sim(0, 0, 0, 0, 0, 0); // catcher
        state.enter_sim(1, 10, 0, 0, 0, 0); // thrower

        // The thrower allocates ten objects and frees two, then throws.
        state.note_throw(20, 10, 2, 0, 0);
        // The handler allocates ninety more and frees eight.
        state.exit_sim(0, 1_000, 100, 10, 0, 0);

        assert_eq!(
            state.fns[1].incl_allocs, 10,
            "the thrower allocated ten objects, not the handler's ninety"
        );
        assert_eq!(
            state.fns[1].excl_frees, 2,
            "the thrower freed two, not the handler's eight"
        );
        assert_eq!(
            state.fns[0].excl_allocs, 90,
            "the catcher owns what its handler allocated"
        );
        let allocs: u64 = state.fns.iter().map(|a| a.excl_allocs).sum();
        assert_eq!(allocs, 100, "allocations should partition the root's hundred");
    }

    /// A function that catches what it threw itself keeps its handler's work.
    ///
    /// Nothing distinguishes this from an outer frame catching for an inner one
    /// at the moment the handler runs — the stack looks identical either way,
    /// and which frame is the catcher is only known when one of them returns.
    /// The deferred charge has to land correctly in both, and this is the case
    /// where "correctly" means the frame the charge was deferred FROM.
    #[test]
    fn a_function_that_catches_its_own_throw_keeps_the_handlers_work() {
        let mut state = State::default();
        state.enter_sim(0, 0, 0, 0, 0, 0); // caller
        state.enter_sim(1, 10, 0, 0, 0, 0); // throws and catches, all its own

        state.note_throw(20, 0, 0, 0, 0);
        state.enter_sim(2, 30, 0, 0, 0, 0); // the handler calls something
        state.exit_sim(2, 900, 0, 0, 0, 0);
        state.exit_sim(1, 950, 0, 0, 0, 0);
        state.exit_sim(0, 1_000, 0, 0, 0, 0);

        assert_eq!(
            state.edges.get(&(1, 2)).map(|e| e.0),
            Some(1),
            "the handler's call belongs to the frame that ran it"
        );
        assert_eq!(state.fns[2].excl_ns, 870, "the call ran for 870");
        let total: u64 = state.fns.iter().map(|a| a.excl_ns).sum();
        assert_eq!(total, 1_000, "self time should still partition the root");
    }

    /// A handler that throws again does not lose what it already spent.
    ///
    /// The second throw replaces the unwind record. Replacing it outright
    /// dropped the charge the first one was holding — already subtracted from
    /// its frames, so nothing would have accounted for it again.
    #[test]
    fn a_rethrow_carries_the_first_unwinds_charge_to_the_next_catcher() {
        let mut state = State::default();
        state.enter_sim(0, 0, 0, 0, 0, 0); // outer catcher
        state.enter_sim(1, 10, 0, 0, 0, 0); // thrower

        state.note_throw(20, 0, 0, 0, 0);
        state.enter_sim(2, 30, 0, 0, 0, 0); // the handler works
        state.exit_sim(2, 500, 0, 0, 0, 0);
        state.note_throw(600, 0, 0, 0, 0); // and then throws again
        state.exit_sim(0, 1_000, 0, 0, 0, 0);

        let total: u64 = state.fns.iter().map(|a| a.excl_ns).sum();
        assert_eq!(
            total, 1_000,
            "the first unwind's charge went missing across the rethrow"
        );
        assert_eq!(state.fns[2].excl_ns, 470, "the handler's call ran for 470");
    }

    /// An exception raised and caught inside a call the handler made.
    ///
    /// The inner one is resolved by an exit nowhere near the outer catcher, so
    /// resolving unwinds in a batch handed the outer exception's bookkeeping to
    /// the inner catcher: the logger's cost charged to the frame the outer
    /// exception had already destroyed, an invented self-edge, the real edge
    /// lost, and the outer dead frames closed at the outer catcher's clock. The
    /// self times still summed to the root, which is why nothing caught it.
    #[test]
    fn an_exception_caught_inside_a_handlers_callee_leaves_the_outer_one_alone() {
        let mut state = State::default();
        state.enter_sim(0, 0, 0, 0, 0, 0); // outer catcher
        state.enter_sim(1, 10, 0, 0, 0, 0); // throws at 20

        state.note_throw(20, 0, 0, 0, 0);
        state.enter_sim(2, 30, 0, 0, 0, 0); // the handler calls the logger
        state.enter_sim(3, 40, 0, 0, 0, 0); // which calls something that throws
        state.note_throw(50, 0, 0, 0, 0);
        state.exit_sim(2, 100, 0, 0, 0, 0); // and the logger catches it itself
        state.exit_sim(0, 1_000, 0, 0, 0, 0);

        assert!(state.stack.is_empty(), "the outer catcher did not resync");
        let total: u64 = state.fns.iter().map(|a| a.excl_ns).sum();
        assert_eq!(total, 1_000, "self time did not partition the root");

        assert_eq!(
            state.fns[1].excl_ns, 10,
            "the outer thrower ran ten ticks and died at its own throw"
        );
        assert_eq!(state.fns[3].excl_ns, 10, "the inner thrower ran ten ticks");
        assert_eq!(
            state.fns[2].excl_ns, 60,
            "the logger ran from 30 to 100, less the ten its callee took"
        );

        assert_eq!(
            state.edges.get(&(0, 2)),
            Some(&(1, 70)),
            "the handler's call belongs to the catcher that ran it"
        );
        assert!(
            !state.edges.contains_key(&(1, 2)),
            "the call was attributed to the frame the exception destroyed"
        );
        assert!(
            !state.edges.contains_key(&(2, 2)),
            "the inner catcher was given an edge to itself"
        );
    }

    /// A handler that throws again, from inside a call that never returns.
    ///
    /// The frame the handler opened is still on the stack when the catcher
    /// finally exits, because the second throw unwound it and unwound frames run
    /// no exit. Stack height therefore never comes back down to the throw's
    /// depth, and asking it whether the unwind is over answers no — leaving the
    /// dead frames to be closed at the catcher's clock and the pending charge
    /// with nobody to go to.
    #[test]
    fn a_second_throw_from_an_open_handler_frame_still_ends_at_the_catcher() {
        let mut state = State::default();
        state.enter_sim(0, 0, 0, 0, 0, 0); // outer catcher
        state.enter_sim(1, 10, 0, 0, 0, 0); // thrower

        state.note_throw(20, 0, 0, 0, 0);
        state.enter_sim(2, 30, 0, 0, 0, 0); // the handler calls this...
        state.enter_sim(3, 40, 0, 0, 0, 0); // ...which calls this
        state.exit_sim(3, 50, 0, 0, 0, 0); // only the inner one returns
        state.note_throw(600, 0, 0, 0, 0); // and then the handler throws again

        state.exit_sim(0, 1_000, 0, 0, 0, 0);

        assert!(state.stack.is_empty(), "the catcher's exit did not resync");
        assert_eq!(
            state.fns[1].incl_ns, 10,
            "the first thrower still carries only its own ten ticks"
        );
        let total: u64 = state.fns.iter().map(|a| a.excl_ns).sum();
        assert!(
            total <= 1_000,
            "self time ran past the root's thousand ticks: {total}"
        );
    }

    /// Every tick of a nested unwind lands on exactly one function.
    ///
    /// This is the sequence that forced the accounting to record one instant per
    /// throw rather than one per unwind. With a single instant there is no right
    /// answer: keep the first and the frame the handler opened is closed before
    /// it started, keep the second and the frame the first throw killed is
    /// credited with everything the handler did. Both were tried here, and both
    /// showed up as a total that was not the root's.
    #[test]
    fn a_nested_unwind_still_accounts_for_exactly_the_roots_span() {
        let mut state = State::default();
        state.enter_sim(0, 0, 0, 0, 0, 0);
        state.enter_sim(1, 10, 0, 0, 0, 0);
        state.note_throw(20, 0, 0, 0, 0);
        state.enter_sim(2, 30, 0, 0, 0, 0);
        state.enter_sim(3, 40, 0, 0, 0, 0);
        state.exit_sim(3, 50, 0, 0, 0, 0);
        state.note_throw(600, 0, 0, 0, 0);
        state.exit_sim(0, 1_000, 0, 0, 0, 0);

        for (id, acc) in state.fns.iter().enumerate() {
            assert!(
                acc.excl_ns <= 1_000,
                "#{id} reports {} ns of self time in a run of 1000",
                acc.excl_ns
            );
        }
        let total: u64 = state.fns.iter().map(|a| a.excl_ns).sum();
        assert_eq!(total, 1_000, "self time did not partition the root");
        assert_eq!(
            state.fns[2].excl_ns, 560,
            "the frame the handler opened ran from 30 until the second throw at \
             600, less the 10 its own callee took"
        );
        assert_eq!(
            state.fns[1].excl_ns, 10,
            "the first thrower died at the FIRST throw, not the second"
        );
        assert_eq!(
            state.overdrawn, 0,
            "a frame was charged more than it ran for, and the total only holds \
             because the subtraction saturated"
        );
    }

    /// A throw with nothing on the stack accounts for nothing, and breaks nothing.
    #[test]
    fn a_throw_at_depth_zero_leaves_the_next_slice_alone() {
        let mut state = State::default();
        state.note_throw(0, 0, 0, 0, 0);

        state.enter_sim(0, 100, 0, 0, 0, 0);
        state.enter_sim(1, 110, 0, 0, 0, 0);
        state.exit_sim(1, 200, 0, 0, 0, 0);
        state.exit_sim(0, 300, 0, 0, 0, 0);

        assert_eq!(state.fns[1].excl_ns, 90, "the callee ran for 90");
        assert_eq!(state.fns[0].excl_ns, 110, "the caller ran for 110");
        assert_eq!(
            state.edges.get(&(0, 1)).map(|e| e.0),
            Some(1),
            "the call was deferred to a catcher that does not exist"
        );
    }

    /// A slice boundary that lands mid-unwind does not carry the charge over.
    #[test]
    fn a_reset_mid_unwind_does_not_charge_the_next_slice() {
        let mut state = State::default();
        state.enter_sim(0, 0, 0, 0, 0, 0);
        state.enter_sim(1, 10, 0, 0, 0, 0);
        state.note_throw(20, 0, 0, 0, 0);
        state.enter_sim(2, 30, 0, 0, 0, 0);
        state.exit_sim(2, 900, 0, 0, 0, 0);
        state.reset();

        // A fresh slice: one call, ten ticks, and nothing else.
        state.enter_sim(0, 1_000, 0, 0, 0, 0);
        state.exit_sim(0, 1_010, 0, 0, 0, 0);
        assert_eq!(
            state.fns[0].excl_ns, 10,
            "the previous slice's handler work followed the reset"
        );
    }

    /// Arm, offer, take — the three steps the endpoint and a worker perform,
    /// with nothing between them.
    #[test]
    fn a_slice_offered_while_armed_is_handed_to_the_taker() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        reset_capture();
        elephc_instr_capture_arm();

        let slice = "elephc-instr: hot calls=3 incl_ns=10\n";
        assert!(super::offer_capture(slice), "an armed capture refused a slice");

        // First call reports the length and keeps the slice.
        let needed = unsafe { elephc_instr_capture_take(std::ptr::null_mut(), 0) };
        assert_eq!(needed, slice.len(), "the taker was told the wrong length");

        let mut buffer = vec![0u8; needed];
        let written = unsafe { elephc_instr_capture_take(buffer.as_mut_ptr(), needed) };
        assert_eq!(written, slice.len());
        assert_eq!(String::from_utf8(buffer).unwrap(), slice);

        // Consumed: a second take finds nothing, and nothing is left armed.
        assert_eq!(unsafe { elephc_instr_capture_take(std::ptr::null_mut(), 0) }, 0);
        assert!(!super::offer_capture("later"), "the capture stayed armed after a take");
    }

    /// A slice left by a forked child is taken by the parent.
    ///
    /// This is the point of mapping the rendezvous instead of keeping a static:
    /// under `--web` the endpoint that asks lives in the parent that accepts and
    /// forks, and the request that renders a slice runs in a worker. A test that
    /// only exercised one process would pass on exactly the design that failed.
    #[test]
    fn a_slice_left_by_a_forked_child_reaches_the_parent() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        reset_capture();
        elephc_instr_capture_arm();

        let slice = "elephc-instr: forked calls=7 incl_ns=99\n";
        // Safety: the child does nothing but write into the shared mapping and
        // leave immediately, so it runs no destructors and takes no lock the
        // parent holds.
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0, "fork failed");
        if pid == 0 {
            let offered = super::offer_capture(slice);
            unsafe { libc::_exit(i32::from(!offered)) };
        }

        let mut status = 0;
        unsafe { libc::waitpid(pid, &mut status, 0) };
        assert_eq!(
            libc::WEXITSTATUS(status),
            0,
            "the child could not offer the slice"
        );

        let needed = unsafe { elephc_instr_capture_take(std::ptr::null_mut(), 0) };
        assert_eq!(needed, slice.len(), "the parent saw no slice from the child");
        let mut buffer = vec![0u8; needed];
        let written = unsafe { elephc_instr_capture_take(buffer.as_mut_ptr(), needed) };
        assert_eq!(written, slice.len());
        assert_eq!(String::from_utf8(buffer).unwrap(), slice);
    }

    /// Nobody waiting means the dump keeps logging, which is the default and has
    /// to stay free.
    #[test]
    fn a_slice_offered_to_nobody_is_refused() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        elephc_instr_capture_cancel();
        assert!(!super::offer_capture("elephc-instr: hot calls=1\n"));
    }

    /// Makes one tick worth one nanosecond, so a test that feeds synthetic
    /// timestamps reads them back unchanged.
    ///
    /// The hot path stores raw counter ticks and the renderer converts once. A
    /// test asserting rendered nanoseconds against hand-written timestamps is
    /// therefore asserting something about the host's counter unless it says
    /// which rate it means — on this machine, 24 MHz, `30` renders as `1250`.
    /// Where a simulated stack starts. High enough that the deepest frame a test
    /// can reach is still a plausible address, and never zero.
    const SIM_BASE: usize = 0x7fff_0000_0000;

    impl State {
        /// Enters `id` at the frame pointer a real stack would hand this depth,
        /// and returns it, so a test can hold on to a particular activation.
        fn enter_sim(&mut self, id: u32, t: u64, a: u64, f: u64, io: u64, w: u64) -> usize {
            let fp = SIM_BASE - self.stack.len() * 64;
            self.enter_at(id, fp, t, a, f, io, w);
            fp
        }

        /// Suspends the activation at `fp`, taking its address as the coroutine
        /// it belongs to.
        ///
        /// One coroutine per address is what a simulated stack means, and the two
        /// keys answer different questions in production — the pointer says which
        /// activation, `_fiber_current` says which suspension — so a test that
        /// needs them to differ says so by calling `suspend_at` itself.
        fn suspend_sim(&mut self, id: u32, fp: usize, t: u64, a: u64, f: u64, io: u64, w: u64) {
            self.suspend_at(id, fp, fp, t, a, f, io, w);
        }

        /// Exits the innermost live activation of `id`.
        ///
        /// Which is what the hook used to resolve to when it had only an id, and
        /// what every non-recursive test means: with one activation live there is
        /// nothing to choose between. A test about recursion says which
        /// activation it means by passing the frame pointer `enter_sim` returned.
        fn exit_sim(&mut self, id: u32, t: u64, a: u64, f: u64, io: u64, w: u64) {
            let fp = self
                .stack
                .iter()
                .rposition(|frame| frame.id == id)
                .map(|index| self.stack[index].fp)
                // Not on the stack: dropped past the cap, or never entered. The
                // address cannot collide with a live frame, which is the point.
                .unwrap_or(SIM_BASE + 64);
            self.exit_at(id, fp, t, a, f, io, w);
        }
    }

    fn ticks_are_nanoseconds() -> std::sync::MutexGuard<'static, ()> {
        // The guard comes back with it: the rate is one global, every test that
        // switches the profiler on replaces it with the hardware's, and these run
        // in parallel. Handing back the lock is what makes it impossible to set
        // the rate without also holding the right to rely on it.
        let serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        super::TICK_HZ.store(1_000_000_000, super::Ordering::Relaxed);
        serial
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
        // The rate is one global and the tests run in parallel, so setting it
        // means holding the lock — the same reason `ticks_are_nanoseconds` hands
        // its guard back. This test set it bare, which could swap the rate under
        // a render test mid-assertion.
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
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
        /// Decodes the wire escaping, restated from `monitor` so the pair can be
        /// tested here without depending on the compiler crate.
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
    /// The base case: a callee's time is inclusive in its caller and excluded
    /// from that caller's self.
    fn simple_parent_child_accounting() {
        let mut s = State::default();
        // Timestamps then allocation counters. a=main, b=child.
        // main enters @t0/alloc0, a enters, b enters @t10/alloc3, unwinds.
        // Args: (id, ns, allocs, frees, io). Only b does io (2 queries).
        s.enter_sim(0, 0, 0, 0, 0, 0); // main
        s.enter_sim(1, 0, 0, 0, 0, 0); // a
        s.enter_sim(2, 10, 3, 0, 0, 0); // b
        s.exit_sim(2, 40, 8, 0, 2, 0); // b: 30ns, 5 allocs, 2 io
        s.exit_sim(1, 50, 9, 0, 2, 0); // a: children 30/5/2 -> excl 20ns/4allocs/0io
        s.exit_sim(0, 60, 12, 0, 2, 0); // main: excl 10ns/3allocs/0io
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

    /// A language-level process exit closes the current function and `{main}`
    /// even though neither generated epilogue can run.
    #[test]
    fn termination_closes_the_live_stack_at_one_counter_snapshot() {
        let mut s = State::default();
        s.enter_at(0, 0x1000, 10, 2, 0, 0, 0); // {main}
        s.enter_at(1, 0x2000, 20, 3, 0, 0, 0); // function that calls exit(0)

        s.terminate_at(Some((1, 0x2000)), 50, 7, 1, 2, 5);

        assert!(s.stack.is_empty(), "termination must close every tracked frame");
        assert_eq!(s.fns[1].incl_ns, 30);
        assert_eq!(s.fns[1].excl_ns, 30);
        assert_eq!(s.fns[0].incl_ns, 40);
        assert_eq!(s.fns[0].excl_ns, 10);
        assert_eq!(s.fns[0].incl_allocs, 5);
        assert_eq!(s.fns[0].incl_frees, 1);
        assert_eq!(s.fns[0].incl_io, 2);
        assert_eq!(s.fns[0].incl_wait, 5);
        assert_eq!(s.edges.get(&(0, 1)), Some(&(1, 30)));
    }

    /// Termination identifies a recursive catcher by frame pointer instead of
    /// closing the unwind-dead activation with the same function id.
    #[test]
    fn termination_resynchronizes_to_the_exact_recursive_activation() {
        let _serial = ticks_are_nanoseconds();
        let restore_trace = TRACE_ON.swap(true, Ordering::Relaxed);
        let mut s = State::default();
        s.enter_at(0, 0x1000, 0, 0, 0, 0, 0); // {main}
        s.enter_at(1, 0x2000, 5, 0, 0, 0, 0); // recursive catcher
        s.enter_at(1, 0x3000, 10, 0, 0, 0, 0); // recursive thrower
        s.note_throw(20, 0, 0, 0, 0);

        s.terminate_at(Some((1, 0x2000)), 50, 0, 0, 0, 0);

        TRACE_ON.store(restore_trace, Ordering::Relaxed);
        assert!(s.stack.is_empty(), "termination must drain the resynchronized stack");
        assert_eq!(
            s.trace,
            vec![(1, 5, 50), (0, 0, 50)],
            "the unwind-dead recursive activation was closed as the current one"
        );
    }

    #[test]
    /// Inclusive time is credited at the OUTERMOST activation, so a recursive
    /// function does not accumulate its own nested time repeatedly.
    fn recursion_does_not_double_count() {
        let mut s = State::default();
        s.enter_sim(0, 0, 0, 0, 0, 0);
        s.enter_sim(0, 0, 1, 0, 0, 0);
        s.enter_sim(0, 0, 2, 0, 0, 0);
        s.exit_sim(0, 30, 5, 0, 0, 0);
        s.exit_sim(0, 60, 7, 0, 0, 0);
        s.exit_sim(0, 90, 10, 0, 0, 0); // outermost span 0..90 ns, 0..10 allocs
        assert_eq!(s.fns[0].calls, 3);
        assert_eq!(s.fns[0].incl_ns, 90);
        assert_eq!(s.fns[0].incl_allocs, 10);
        // Exclusive equals inclusive (single function, all self).
        assert_eq!(s.fns[0].excl_ns, 90);
        assert_eq!(s.fns[0].excl_allocs, 10);
    }

    #[test]
    /// An exit for a frame below the top closes the frames above it, which is
    /// how the stack recovers from an unwind it never saw.
    fn exit_resyncs_past_unwound_frames() {
        let mut s = State::default();
        s.enter_sim(0, 0, 0, 0, 0, 0); // a
        s.enter_sim(1, 5, 1, 0, 0, 0); // b
        s.enter_sim(2, 10, 2, 0, 0, 0); // c — unwound, no exits for c or b
        s.exit_sim(0, 100, 9, 0, 0, 0);
        assert_eq!(s.stack.len(), 0, "stack fully unwound");
        assert_eq!(s.fns[0].incl_ns, 100);
        assert_eq!(s.fns[0].incl_allocs, 9);
        assert_eq!(s.fns[1].depth, 0);
        assert_eq!(s.fns[2].depth, 0);
    }

    /// Recursion where the throw comes from a function of its own.
    ///
    /// The catcher is then the innermost activation of the recursive function,
    /// which IS the occurrence `exit_at` resolves to, so this shape is exact.
    /// It is not recursion in general — see the test below, where the recursive
    /// call itself throws and the resolution has no way to be right.
    #[test]
    fn a_recursive_function_that_catches_around_its_own_call_accounts_exactly() {
        let mut s = State::default();
        s.enter_sim(0, 0, 0, 0, 0, 0); // main, 0..1000
        s.enter_sim(1, 10, 0, 0, 0, 0); // rec, outer, 10..600
        s.enter_sim(1, 20, 0, 0, 0, 0); // rec, inner, 20..500
        s.enter_sim(2, 30, 0, 0, 0, 0); // its callee, 30..40, killed by the throw
        s.note_throw(40, 0, 0, 0, 0);
        s.exit_sim(1, 500, 0, 0, 0, 0); // the inner activation catches
        s.exit_sim(1, 600, 0, 0, 0, 0); // and the outer one returns normally
        s.exit_sim(0, 1_000, 0, 0, 0, 0);

        assert!(s.stack.is_empty(), "the stack did not unwind");
        assert_eq!(s.fns[2].excl_ns, 10, "the callee ran ten ticks before the throw");
        assert_eq!(
            s.fns[1].excl_ns, 580,
            "both activations of the recursive function: 470 inner, 110 outer"
        );
        assert_eq!(s.fns[1].calls, 2, "two activations, one function");
        assert_eq!(s.fns[0].excl_ns, 410, "the caller keeps what it actually ran");
        let total: u64 = s.fns.iter().map(|a| a.excl_ns).sum();
        assert_eq!(total, 1_000, "self time did not partition the root");
    }

    /// Recursion where the recursive call itself throws.
    ///
    /// ```php
    /// function f(int $n): void {
    ///     if ($n > 0) { try { f($n - 1); } catch (\Exception $e) { recover(); } }
    ///     else { throw new \Exception('x'); }
    /// }
    /// ```
    ///
    /// Every activation carries the `try`, and the one that catches is not the
    /// one the id alone would find: the deepest took the `else` branch and threw,
    /// so the topmost frame carrying that id is the corpse of the thrower rather
    /// than the frame returning. With an id and nothing else this cost 20 ticks
    /// of 120 — off `{main}`, onto `f` — with the self times still summing to the
    /// root, so nothing looked wrong. The exit says which activation it is now.
    #[test]
    fn a_recursive_call_that_throws_is_charged_to_the_activation_that_caught() {
        let mut s = State::default();
        s.enter_sim(0, 0, 0, 0, 0, 0); // {main}, 0..120
        let outer = s.enter_sim(1, 10, 0, 0, 0, 0); // f(1), carries the try, 10..100
        s.enter_sim(1, 20, 0, 0, 0, 0); // f(0), throws at 30
        s.note_throw(30, 0, 0, 0, 0);
        s.enter_sim(2, 40, 0, 0, 0, 0); // recover(), 40..70
        s.exit_sim(2, 70, 0, 0, 0, 0);
        s.exit_at(1, outer, 100, 0, 0, 0, 0); // f(1) — the OUTER one — returns
        s.exit_sim(0, 120, 0, 0, 0, 0);

        assert!(s.stack.is_empty(), "the stack did not unwind");
        assert_eq!(
            s.fns[1].excl_ns, 60,
            "f: 10 for the activation that threw, 50 for the one that caught"
        );
        assert_eq!(s.fns[0].excl_ns, 30, "{{main}} keeps what it actually ran");
        assert_eq!(s.fns[2].excl_ns, 30, "recover ran 40..70");
        let total: u64 = s.fns.iter().map(|a| a.excl_ns).sum();
        assert_eq!(total, 120, "self time did not partition the root");
        assert_eq!(
            s.edges.get(&(1, 2)).map(|e| e.0),
            Some(1),
            "the handler's call belongs to the activation that ran it"
        );
    }

    /// A dump taken while frames are still running says so.
    ///
    /// Inclusive time is credited when a function's depth returns to zero, so a
    /// dump reached without the enclosing epilogues — a PHP `exit()`, a fatal
    /// path — prints a root with `incl_ns=0` and self values that do not sum to
    /// it. Both are true statements about a truncated capture; neither is
    /// readable without knowing it was truncated.
    #[test]
    fn a_dump_with_frames_still_running_says_how_many() {
        let _serial = ticks_are_nanoseconds();
        let mut s = State::default();
        s.enter_sim(0, 0, 0, 0, 0, 0); // {main}, still running
        s.enter_sim(1, 10, 0, 0, 0, 0); // and its callee
        s.exit_sim(1, 40, 0, 0, 0, 0);

        let names = ["{main}".to_string(), "work".to_string()];
        let out = s.render(&names);
        assert!(
            out.contains("1 frame(s) were still open at this dump"),
            "a truncated capture read as a complete one:\n{out}"
        );
        assert!(
            out.contains("elephc-instr: {main} calls=1 incl_ns=0"),
            "the root should show no inclusive time, which is what the note explains:\n{out}"
        );

        // And a complete capture says nothing of the kind.
        s.exit_sim(0, 100, 0, 0, 0, 0);
        assert!(
            !s.render(&names).contains("still open at this dump"),
            "a complete capture claimed to be truncated"
        );
    }

    /// A trace whose timestamps are counter ticks says so inside the file.
    ///
    /// The format calls them microseconds, so a viewer has no way to tell — and
    /// the note that warns about it in the text profile does not travel with the
    /// trace. On a 24 MHz counter a real millisecond reads as 24 µs, which is a
    /// plausible number and a wrong one.
    #[test]
    fn a_trace_written_before_the_rate_is_known_declares_its_units() {
        let dir = std::env::temp_dir().join("elephc_instr_trace_units_test.json");
        let spans = vec![(0u32, 100u64, 24_100u64)];
        let names = vec!["work".to_string()];

        write_chrome_trace(dir.to_str().unwrap(), &spans, &names, 0, false);
        let text = std::fs::read_to_string(&dir).expect("trace written");
        assert!(
            text.contains("UNCONVERTED counter ticks"),
            "a trace in counter ticks read as one in microseconds:\n{text}"
        );
        assert!(
            text.starts_with("{\"traceEvents\":[{\"name\":\"process_name\""),
            "the marker is not where a viewer reads it:\n{text}"
        );

        // And a converted one carries no such claim.
        write_chrome_trace(dir.to_str().unwrap(), &spans, &names, 0, true);
        let text = std::fs::read_to_string(&dir).expect("trace written");
        assert!(
            !text.contains("UNCONVERTED"),
            "a converted trace claimed its units were wrong:\n{text}"
        );
        let _ = std::fs::remove_file(&dir);
    }

    /// A throw with nothing on the stack records nothing.
    ///
    /// Resolution asks for a throw deeper than the exiting frame's index, and
    /// every index is at least zero, so a record at depth zero can never be
    /// resolved. Thirty-two of them filled the table and left the next real
    /// throw recycling a live record.
    #[test]
    fn a_throw_with_an_empty_stack_records_nothing_to_resolve() {
        let mut s = State::default();
        for tick in 0..64 {
            s.note_throw(tick, 0, 0, 0, 0);
        }
        assert!(
            s.unwinding.is_none(),
            "{} unresolvable records were kept",
            s.unwinding.as_ref().map(|u| u.throws.len()).unwrap_or(0)
        );

        // And a real throw afterwards still works.
        s.enter_sim(0, 100, 0, 0, 0, 0);
        s.enter_sim(1, 110, 0, 0, 0, 0);
        s.note_throw(120, 0, 0, 0, 0);
        s.exit_sim(0, 200, 0, 0, 0, 0);
        assert_eq!(s.fns[1].excl_ns, 10, "the thrower died at its throw");
        let total: u64 = s.fns.iter().map(|a| a.excl_ns).sum();
        assert_eq!(total, 100, "self time did not partition the root");
    }

    /// Past the nesting cap, a throw shares a record — and the report says so.
    ///
    /// The alternative to sharing is dropping the older throw's accumulated
    /// charge, which would stop the exclusives adding up to the root. Both are
    /// wrong; only one of them is wrong out loud.
    #[test]
    fn throws_past_the_nesting_cap_share_a_record_and_the_report_says_so() {
        let _serial = ticks_are_nanoseconds();
        let mut s = State::default();
        s.enter_sim(0, 0, 0, 0, 0, 0);
        for tick in 0..(MAX_NESTED_THROWS as u64 + 4) {
            s.enter_sim(1, 10 + tick, 0, 0, 0, 0);
            s.note_throw(20 + tick, 0, 0, 0, 0);
        }
        assert_eq!(
            s.throws_merged, 4,
            "records were recycled without the report admitting it"
        );

        s.exit_sim(0, 10_000, 0, 0, 0, 0);
        let names = ["{main}".to_string(), "f".to_string()];
        assert!(
            s.render(&names).contains("shared a record with an older one"),
            "the report does not carry the note"
        );
    }

    /// An exit for a frame that was never pushed closes nothing.
    ///
    /// The resync loop stops when it finds the id, so an exit for one that is not
    /// there walked the whole stack and closed every frame on the way. Past the
    /// shadow-stack cap and from the exported enable/disable toggles, that is a
    /// reachable exit — and it discarded the accounting of every enclosing
    /// function, which is a far worse answer than doing nothing.
    #[test]
    fn an_exit_for_a_frame_that_was_never_pushed_leaves_the_stack_alone() {
        let mut s = State::default();
        s.enter_sim(0, 0, 0, 0, 0, 0);
        s.enter_sim(1, 10, 0, 0, 0, 0);

        s.exit_sim(9, 50, 0, 0, 0, 0);

        assert_eq!(s.stack.len(), 2, "an unknown exit emptied the stack");
        assert_eq!(s.fns[0].excl_ns, 0, "an enclosing frame was closed by it");
        assert_eq!(s.fns[1].excl_ns, 0, "an enclosing frame was closed by it");

        // And the real exits still work afterwards.
        s.exit_sim(1, 60, 0, 0, 0, 0);
        s.exit_sim(0, 100, 0, 0, 0, 0);
        assert_eq!(s.fns[1].excl_ns, 50);
        assert_eq!(s.fns[0].excl_ns, 50);
    }

    #[test]
    /// The dump carries both per-function metrics and caller→callee edges, in
    /// the exact line shapes `monitor` parses.
    fn render_lists_metrics_and_edges() {
        let _serial = ticks_are_nanoseconds();
        let mut s = State::default();
        s.enter_sim(0, 0, 0, 0, 0, 0);
        s.enter_sim(1, 0, 0, 0, 0, 0);
        s.note_network();
        s.note_network_wait(2_000);
        s.exit_sim(1, 40, 7, 5, 3, 0); // hot: 7 allocs, 5 frees, 3 io ops
        s.exit_sim(0, 50, 8, 5, 3, 0);
        let names = vec!["{main}".to_string(), "hot".to_string()];
        let out = s.render(&names);
        // Retained = allocs - frees: hot keeps 2 of its 7; main's own 1 alloc is
        // never freed, so the run retains 3 in total.
        assert!(out.contains("elephc-instr: {main} calls=1 incl_ns=50 excl_ns=10 incl_allocs=8 excl_allocs=1 incl_io=3 excl_io=0 incl_ret=3 excl_ret=1"), "{out}");
        assert!(out.contains("elephc-instr: hot calls=1 incl_ns=40 excl_ns=40 incl_allocs=7 excl_allocs=7 incl_io=3 excl_io=3 incl_ret=2 excl_ret=2"), "{out}");
        let hot = out
            .lines()
            .find(|line| line.starts_with("elephc-instr: hot "))
            .expect("hot row");
        assert!(
            hot.contains(
                "incl_network=1 excl_network=1 incl_network_wait=2000 \
                 excl_network_wait=2000"
            ),
            "{out}"
        );
        let main = out
            .lines()
            .find(|line| line.starts_with("elephc-instr: {main} "))
            .expect("main row");
        assert!(
            main.contains(
                "incl_network=1 excl_network=0 incl_network_wait=2000 \
                 excl_network_wait=0"
            ),
            "{out}"
        );
        assert!(out.contains("elephc-instr-edge: {main} -> hot count=1 ns=40"), "{out}");
    }

    /// Network work in an exception handler stays on the catcher and its live callees.
    #[test]
    fn network_metrics_survive_exception_resynchronization() {
        let mut s = State::default();
        let root = s.enter_sim(0, 0, 0, 0, 0, 0);
        let catcher = s.enter_sim(1, 10, 0, 0, 0, 0);
        s.enter_sim(2, 20, 0, 0, 0, 0);
        s.note_throw(30, 0, 0, 0, 0);

        s.note_network();
        s.note_network_wait(5);
        let child = s.enter_sim(3, 40, 0, 0, 0, 0);
        s.note_network();
        s.note_network_wait(7);
        s.exit_at(3, child, 50, 0, 0, 0, 0);

        s.exit_at(1, catcher, 100, 0, 0, 0, 0);
        s.exit_at(0, root, 110, 0, 0, 0, 0);

        assert_eq!((s.fns[2].incl_network, s.fns[2].excl_network), (0, 0));
        assert_eq!((s.fns[3].incl_network, s.fns[3].excl_network), (1, 1));
        assert_eq!((s.fns[1].incl_network, s.fns[1].excl_network), (2, 1));
        assert_eq!((s.fns[0].incl_network, s.fns[0].excl_network), (2, 0));
        assert_eq!(s.fns[1].incl_network_wait, 12);
        assert_eq!(s.fns[1].excl_network_wait, 5);
        assert_eq!(s.fns[0].incl_network_wait, 12);
    }

    #[test]
    /// Retained objects go negative for a function that frees what it did not
    /// allocate, and still partition — clamping at zero would hide a release.
    fn retained_is_signed_and_partitions_like_the_other_dimensions() {
        let _serial = ticks_are_nanoseconds();
        // `cleanup` frees more than it allocates (it releases what main built),
        // so its retained is negative — the dimension must not clamp at zero.
        let mut s = State::default();
        s.enter_sim(0, 0, 0, 0, 0, 0); // main
        s.enter_sim(1, 10, 10, 0, 0, 0); // cleanup, entered after main made 10 objects
        s.exit_sim(1, 20, 10, 8, 0, 0); // cleanup: 0 allocs, 8 frees -> retained -8
        s.exit_sim(0, 30, 10, 8, 0, 0); // main: 10 allocs, 8 frees -> retained +2
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
    /// W3C `traceparent` parsing: a well-formed header continues its trace, and
    /// anything malformed is refused rather than half-read.
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
    /// A request joins its caller's trace when one arrives, and starts a fresh
    /// one when nothing usable does — never an island either way.
    fn trace_begin_continues_a_valid_trace_and_starts_one_otherwise() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let hdr = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

        // Dormant, it does nothing at all — no trace id minted, no environment
        // published, no /dev/urandom read. This is the contract the capability is
        // defined by, and this entry point was the one place it did not hold.
        ENABLED.store(false, Ordering::Relaxed);
        std::env::remove_var("ELEPHC_TRACEPARENT");
        elephc_instr_trace_begin(hdr.as_ptr(), hdr.len(), std::ptr::null(), 0);
        assert!(
            std::env::var("ELEPHC_TRACEPARENT").is_err(),
            "a dormant binary published a trace context"
        );

        switch_on();
        // Inbound header: same trace, our span is a child of the caller's.
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
        ENABLED.store(false, Ordering::Relaxed);
    }

    #[test]
    /// Each request is its own slice: without the reset the second request on a
    /// worker reported the first one's calls and time as well.
    fn reset_makes_each_dump_a_fresh_slice() {
        let _serial = ticks_are_nanoseconds();
        // Two identical "requests" on one worker. Without the reset the second
        // reports calls=2 and double the time — the --web bug this fixes.
        let mut s = State::default();
        s.enter_sim(0, 0, 0, 0, 0, 0);
        s.exit_sim(0, 100, 5, 0, 0, 0);
        let names = vec!["work".to_string()];
        let first = s.render(&names);
        assert!(first.contains("work calls=1 incl_ns=100"), "{first}");
        s.reset();
        s.enter_sim(0, 1_000, 90, 0, 0, 0);
        s.exit_sim(0, 1_100, 95, 0, 0, 0);
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
        s.enter_sim(0, 0, 0, 0, 0, 0);
        s.enter_sim(1, 10, 0, 0, 0, 0);
        s.enter_sim(2, 30, 0, 0, 0, 0);
        s.exit_sim(0, 100, 0, 0, 0, 0);

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
    /// Past the depth cap the frames already on the stack stay exact; what is
    /// beyond it is dropped rather than corrupting what is below.
    fn overflowing_the_shadow_stack_keeps_the_frames_it_did_hold() {
        let mut s = State::default();
        // id 0 wraps everything and must survive intact.
        s.enter_sim(0, 0, 0, 0, 0, 0);
        // Fill the stack to the cap with id 1. The timestamps only ever move
        // forward, as the counter this stands in for does: an exit stamped
        // before its own entry produces a negative span, and the totals below
        // then hold by wrapping around rather than by adding up.
        for i in 0..(MAX_STACK - 1) {
            s.enter_sim(1, 1 + i as u64, 0, 0, 0, 0);
        }
        assert_eq!(s.stack.len(), MAX_STACK);
        // Two further activations of a DIFFERENT id cannot be pushed.
        s.enter_sim(2, 70_000, 0, 0, 0, 0);
        s.enter_sim(2, 70_001, 0, 0, 0, 0);
        assert_eq!(s.dropped, 2);
        assert_eq!(s.stack.len(), MAX_STACK, "nothing was pushed past the cap");
        // Their exits must not disturb the stack.
        s.exit_sim(2, 70_002, 0, 0, 0, 0);
        s.exit_sim(2, 70_003, 0, 0, 0, 0);
        assert_eq!(s.stack.len(), MAX_STACK, "a dropped exit pops nothing");
        // Unwind normally.
        for i in 0..(MAX_STACK - 1) {
            s.exit_sim(1, 80_000 + i as u64, 0, 0, 0, 0);
        }
        s.exit_sim(0, 200_000, 0, 0, 0, 0);
        assert!(s.stack.is_empty(), "fully unwound");
        // The outermost frame kept its span, which is what used to be destroyed.
        assert_eq!(s.fns[0].incl_ns, 200_000);
        assert_eq!(s.fns[0].depth, 0);
        assert_eq!(s.fns[1].depth, 0);
        // The dropped calls are counted but carry no timing.
        assert_eq!(s.fns[2].calls, 2);
        assert_eq!(s.fns[2].incl_ns, 0);
        // Exclusive time still partitions the root's inclusive.
        let sum: u64 = s.fns.iter().map(|a| a.excl_ns).sum();
        assert_eq!(sum, s.fns[0].incl_ns);
        assert_eq!(s.overdrawn, 0, "no frame was charged more than it ran for");
    }

    #[test]
    /// Self time divides into recorded driver wait and a non-DB remainder, so a
    /// slow function is legible as driver-bound or elsewhere in its wall time.
    fn wait_splits_self_time_into_driver_wait_and_remainder() {
        // `query` spends 80 of its 100ns blocked in the driver; `compute` runs
        // 50ns outside recorded DB wait. Wait is attributed like every other
        // dimension, so the caller's own wait excludes its callees' wait.
        let mut s = State::default();
        s.enter_sim(0, 0, 0, 0, 0, 0); // main
        s.enter_sim(1, 10, 0, 0, 0, 0); // query
        s.exit_sim(1, 110, 0, 0, 1, 80); // 100ns elapsed, 80ns of it waiting
        s.enter_sim(2, 110, 0, 0, 1, 80); // compute
        s.exit_sim(2, 160, 0, 0, 1, 80); // 50ns, no wait
        s.exit_sim(0, 170, 0, 0, 1, 80); // main: 170ns total, 80 waited by a child
        assert_eq!(s.fns[1].incl_wait, 80);
        assert_eq!(s.fns[1].excl_wait, 80);
        assert_eq!(s.fns[2].excl_wait, 0, "the compute fixture has no DB wait");
        assert_eq!(s.fns[0].incl_wait, 80, "main's subtree waited 80ns");
        assert_eq!(s.fns[0].excl_wait, 0, "main itself never blocked");
        // The non-DB remainder is self wall time minus recorded DB wait.
        assert_eq!(s.fns[1].excl_ns - s.fns[1].excl_wait, 20);
        // Self wait partitions the root's inclusive wait, like the other dimensions.
        let sum: u64 = s.fns.iter().map(|a| a.excl_wait).sum();
        assert_eq!(sum, s.fns[0].incl_wait);
    }

    #[test]
    /// Literals collapse to `?` so repeated queries aggregate into one shape,
    /// while table and column names survive — a shape with no identifiers
    /// names nothing.
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

    /// Serializes the tests that flip `ENABLED`.
    ///
    /// They are the only two that mutate it, and they mutate it in opposite
    /// directions: run concurrently, the dormant one suppresses the other's
    /// recording and the failure reads as a broken assertion.
    static ENABLED_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    /// Statements sharing a shape accumulate into one row with a count, which
    /// is what turns two hundred identical selects into a visible N+1.
    fn instr_query_aggregates_by_shape() {
        // Recording is gated on having been asked, so a test that does not ask
        // measures the gate rather than the aggregation.
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let was_enabled = ENABLED.swap(true, Ordering::Relaxed);
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
        ENABLED.store(was_enabled, Ordering::Relaxed);
    }

    /// A dormant binary records nothing, however many queries it runs.
    ///
    /// The slots are filled at init, so these entry points are reached on every
    /// database call whether or not anyone asked — which is exactly why the
    /// check has to be inside them, and why deleting it would otherwise leave
    /// every test green.
    #[test]
    fn a_dormant_binary_records_no_queries_io_or_wait() {
        let _serial = ENABLED_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let was_enabled = ENABLED.swap(false, Ordering::Relaxed);
        let io_before = IO_OPS.load(Ordering::Relaxed);
        let wait_before = WAIT_NS.load(Ordering::Relaxed);
        let shapes_before = QUERIES.lock().map(|q| q.len()).unwrap_or(0);

        let sql = "SELECT * FROM dormant WHERE id = 42";
        elephc_instr_query(sql.as_ptr(), sql.len());
        elephc_instr_io();
        elephc_instr_wait(1_000_000);

        assert_eq!(IO_OPS.load(Ordering::Relaxed), io_before, "counted an I/O op");
        assert_eq!(WAIT_NS.load(Ordering::Relaxed), wait_before, "counted wait time");
        assert_eq!(
            QUERIES.lock().map(|q| q.len()).unwrap_or(0),
            shapes_before,
            "kept a statement shape"
        );
        ENABLED.store(was_enabled, Ordering::Relaxed);
    }

    #[test]
    /// The timeline parses as the Chrome/Perfetto trace format, with matched
    /// begin/end phases — a viewer rejects the file otherwise.
    fn chrome_trace_is_well_formed() {
        let _serial = ticks_are_nanoseconds();
        // Spans in ns; base is the min enter. Complete ('X') events, µs.
        let spans = vec![(0u32, 1_000u64, 5_000u64), (1u32, 2_000u64, 3_500u64)];
        let names = vec!["{main}".to_string(), "child".to_string()];
        let dir = std::env::temp_dir();
        let path = dir.join("elephc_instr_trace_test.json");
        write_chrome_trace(path.to_str().unwrap(), &spans, &names, 0, true);
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
    /// Quotes, backslashes and control characters are escaped, so a PHP
    /// function name cannot break the trace file that carries it.
    fn json_escape_handles_specials() {
        assert_eq!(json_escape("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(json_escape("x\ny"), "x\\ny");
    }
}
