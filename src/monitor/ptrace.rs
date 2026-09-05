//! Purpose:
//! Reads program counters out of a process this tool did not launch: attach,
//! stop, read the registers and walk the frame chain, detach. Linux only.
//!
//! Called from:
//! - `monitor::attach`, for `--attach <pid>`, which is handed a pid already
//!   running under someone else's control and has no channel to ask over.
//!
//! Key details:
//! - This is the one file that cannot be exercised without `ptrace`, which is
//!   why it holds the syscalls AND the loop that drives them, and nothing else.
//!   Everything that turns what it read into a profile lives in `elf` and
//!   `attach`, takes numbers and returns names, and is tested on any host.
//! - Attaching is INTRUSIVE in a way asking is not: the target is stopped for
//!   the duration of each sample. Every path out of a stop resumes it, because
//!   a target left stopped by a profiler that walked away is worse than no
//!   profile at all.
//! - A frame walk trusts the frame pointer. Code built without one cannot be
//!   walked this way, and the walk stops rather than inventing frames.

use std::io;

/// How deep a single frame walk may go.
///
/// A chain longer than this is a corrupt frame pointer, not a deep program: a
/// cycle in the chain would otherwise walk until some unrelated read failed.
/// Stopping at a fixed depth bounds the time the target is held stopped, which
/// is the cost every sample charges to the program being profiled.
const MAX_DEPTH: usize = 256;

/// A thread stopped and read.
pub(crate) struct Registers {
    /// Where the thread was interrupted.
    pub(crate) pc: u64,
    /// The frame pointer, which is where the chain walk starts.
    pub(crate) fp: u64,
}

/// Every thread of a process, as the kernel lists them.
///
/// A process is sampled thread by thread: `ptrace` acts on threads, not
/// processes, and a program whose work happens off the main thread would
/// otherwise profile as idle. The list is re-read every window because threads
/// come and go.
pub(crate) fn thread_ids(pid: u32) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
        return Vec::new();
    };
    let mut tids: Vec<u32> = entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
        .collect();
    // Ascending, so the main thread leads and two windows line up rather than
    // reshuffling on directory order.
    tids.sort_unstable();
    tids
}

/// The path of the executable behind a pid, as the kernel resolved it.
///
/// Read through `/proc`, not from a command line: a program started through a
/// relative path, a symlink or a `PATH` lookup would otherwise be looked for
/// where it is not, and a deleted-but-running binary is still readable here.
pub(crate) fn executable_path(pid: u32) -> io::Result<std::path::PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe"))
}

/// The kernel's own memory map for a pid, which is what says where the
/// executable actually landed.
pub(crate) fn memory_maps(pid: u32) -> io::Result<String> {
    std::fs::read_to_string(format!("/proc/{pid}/maps"))
}

/// Why the kernel is likely to refuse an attach, when it is going to.
///
/// `ptrace_scope` refuses non-descendants on most distributions, and the
/// refusal a caller gets is a bare `EPERM` that reads like a bug in this tool.
/// Naming the knob is the difference between an operator who can fix it in one
/// line and one who files an issue.
pub(crate) fn attach_refusal_hint() -> Option<String> {
    let scope = std::fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope").ok()?;
    Some(explain_ptrace_scope(scope.trim())?.to_string())
}

/// What one `ptrace_scope` setting means for attaching, in the operator's
/// terms. Split out from reading the file so the wording is testable on a host
/// that does not have one — which includes every macOS developer machine.
pub(crate) fn explain_ptrace_scope(scope: &str) -> Option<&'static str> {
    match scope {
        "0" => None,
        "1" => Some(
            "the kernel's yama/ptrace_scope is 1, which allows attaching to descendants only; \
             run as root, grant CAP_SYS_PTRACE, or set it to 0",
        ),
        "2" => Some(
            "the kernel's yama/ptrace_scope is 2, which allows attaching to CAP_SYS_PTRACE \
             holders only",
        ),
        _ => Some("the kernel's yama/ptrace_scope forbids attaching entirely"),
    }
}

/// Attaches to one thread, leaving it running.
///
/// `SEIZE` rather than `ATTACH`: seizing does not stop the thread as a side
/// effect, so each later stop is one this code asked for and can undo exactly.
/// `ATTACH` conflates the two and leaves a thread stopped by a signal that a
/// resume then has to guess at.
pub(crate) fn seize(tid: u32) -> io::Result<()> {
    // SAFETY: a ptrace request with no memory operands. An invalid tid is the
    // kernel's to reject, which it does with ESRCH.
    let result = unsafe { libc::ptrace(libc::PTRACE_SEIZE, tid as libc::pid_t, 0, 0) };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Asks a seized thread to stop. Does not wait for it.
///
/// Split from the wait deliberately. Once this SUCCEEDS the thread is stopped or
/// about to be, and from that moment every exit owes it a resume — so the split
/// is what lets the caller put the resume where nothing can step around it.
pub(crate) fn interrupt(tid: u32) -> io::Result<()> {
    // SAFETY: a ptrace request with no memory operands.
    let result = unsafe { libc::ptrace(libc::PTRACE_INTERRUPT, tid as libc::pid_t, 0, 0) };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Waits until an interrupted thread is actually stopped.
///
/// The wait is not optional. `INTERRUPT` only *requests* the stop; reading
/// registers before the kernel delivered it returns whatever was there — a
/// plausible-looking address from an arbitrary moment, which is a wrong profile
/// rather than a missing one.
///
/// `EINTR` is retried rather than reported. A profiler runs with signals about
/// (its own timers, a Ctrl-C on the way) and an interrupted wait says nothing
/// about the thread; treating it as a failure would abandon a thread that is
/// stopped and waiting to be read.
pub(crate) fn wait_for_stop(tid: u32) -> io::Result<libc::c_int> {
    loop {
        let mut status: libc::c_int = 0;
        // SAFETY: `status` is a live local for the duration of the call. __WALL
        // is required for threads, which are not children in the waitpid sense.
        let waited = unsafe { libc::waitpid(tid as libc::pid_t, &mut status, libc::__WALL) };
        if waited != -1 {
            return Ok(status);
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

/// The signal a stop was carrying, which the restart has to hand back.
///
/// A seized tracee reports EVERY signal to its tracer and stops until the tracer
/// restarts it. `PTRACE_CONT` and `PTRACE_DETACH` take that signal as their last
/// argument, and passing zero — which is what this file used to do — does not
/// mean "no signal was pending", it means "throw the pending one away". A
/// profiler that samples at 99 Hz holds a lot of stops, so a target being
/// profiled quietly lost `SIGCHLD`, `SIGTERM`, timer signals: reaped children
/// that were never reaped, shutdowns that never arrived, attributable to
/// nothing. Measuring a program must not change what it receives.
///
/// Only a genuine signal-delivery-stop is handed back. The stops this file makes
/// itself carry a `PTRACE_EVENT_*` in the high bits — `PTRACE_INTERRUPT` reports
/// `SIGTRAP` with `PTRACE_EVENT_STOP`, and a group-stop under `SEIZE` reports the
/// stopping signal with the same event. Neither was on its way to the program,
/// and re-injecting either is how a profiler kills what it measures.
fn pending_signal(status: libc::c_int) -> libc::c_int {
    if !libc::WIFSTOPPED(status) || (status >> 16) != 0 {
        return 0;
    }
    let signal = libc::WSTOPSIG(status);
    if signal == libc::SIGTRAP { 0 } else { signal }
}

/// Lets a stopped thread run again, delivering whatever signal it stopped on.
///
/// Called on every path out of a stop, including the failing ones: a thread
/// left stopped is a program that has silently hung, and an operator who
/// attached a profiler to a running service has no reason to suspect the
/// profiler of it.
pub(crate) fn resume(tid: u32, signal: libc::c_int) {
    // SAFETY: no memory operands; a failure here means the thread already
    // exited, which needs no resuming.
    unsafe {
        libc::ptrace(libc::PTRACE_CONT, tid as libc::pid_t, 0, libc::c_long::from(signal));
    }
}

/// Stops tracing a thread, leaving it running as it was found.
///
/// The stop first is not ceremony: `PTRACE_DETACH` requires a STOPPED tracee and
/// restarts it as it detaches. Asked of a running one it fails with `ESRCH` and
/// changes nothing — the thread stays traced, invisibly, because nothing here
/// reads the result.
///
/// That is exactly how a live view died after one window. The seizes happen per
/// window; the second one was refused, because a thread that is already traced
/// cannot be seized again. An empty window is how `--attach` learns its target
/// is gone, so the view reported the program had ended while it was still
/// running — a wrong answer produced by a failure nobody was told about.
pub(crate) fn detach(tid: u32) {
    let _ = interrupt(tid);
    // The last stop is the last chance to hand a pending signal back: after the
    // detach there is no tracer left to hold it, and it is gone.
    let signal = wait_for_stop(tid).map_or(0, pending_signal);
    // SAFETY: no memory operands; ESRCH for a thread that has gone is fine, and
    // a tracer that exits detaches whatever it still holds.
    unsafe {
        libc::ptrace(libc::PTRACE_DETACH, tid as libc::pid_t, 0, libc::c_long::from(signal));
    }
}

/// How many 64-bit words the kernel's register file is on this architecture,
/// and where the two registers a walk needs sit inside it.
///
/// Naming the indices rather than the struct keeps this to one shape and one
/// `iovec`. The layout is ABI — `user_regs_struct` on x86_64, `user_pt_regs` on
/// aarch64 — not a detail that drifts.
#[cfg(target_arch = "x86_64")]
const REGISTER_WORDS: usize = 27;
/// rbp, at index 4 of `user_regs_struct`.
#[cfg(target_arch = "x86_64")]
const FRAME_POINTER_INDEX: usize = 4;
/// rip, at index 16.
#[cfg(target_arch = "x86_64")]
const PROGRAM_COUNTER_INDEX: usize = 16;

/// x0..x30, then sp, pc, pstate.
#[cfg(target_arch = "aarch64")]
const REGISTER_WORDS: usize = 34;
/// x29 is the frame pointer by the AAPCS.
#[cfg(target_arch = "aarch64")]
const FRAME_POINTER_INDEX: usize = 29;
/// pc follows the 31 general registers and sp.
#[cfg(target_arch = "aarch64")]
const PROGRAM_COUNTER_INDEX: usize = 32;

/// Reads the two registers a frame walk starts from.
///
/// `GETREGSET` rather than the older `GETREGS`, because `GETREGS` does not
/// exist on aarch64 — the register file is architecture-shaped and only the
/// regset interface is common to both.
pub(crate) fn registers(tid: u32) -> io::Result<Registers> {
    let mut regs = [0u64; REGISTER_WORDS];
    let mut iov = libc::iovec {
        iov_base: regs.as_mut_ptr().cast::<libc::c_void>(),
        iov_len: std::mem::size_of_val(&regs),
    };
    // SAFETY: `iov` describes `regs`, which outlives the call; NT_PRSTATUS is
    // the regset that layout belongs to, and the kernel writes at most
    // `iov_len` bytes.
    let result = unsafe {
        libc::ptrace(
            libc::PTRACE_GETREGSET,
            tid as libc::pid_t,
            libc::NT_PRSTATUS,
            &mut iov as *mut libc::iovec,
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(Registers { pc: regs[PROGRAM_COUNTER_INDEX], fp: regs[FRAME_POINTER_INDEX] })
}

/// Reads a run of bytes out of the target's address space.
///
/// `process_vm_readv` rather than word-at-a-time `PEEKDATA`: a frame is two
/// words, and a `PEEKDATA` pair is two syscalls for the same bytes — which is
/// the target held stopped twice as long, every frame of every sample.
pub(crate) fn read_memory(pid: u32, address: u64, into: &mut [u8]) -> io::Result<()> {
    let local = libc::iovec {
        iov_base: into.as_mut_ptr().cast::<libc::c_void>(),
        iov_len: into.len(),
    };
    let remote = libc::iovec {
        iov_base: address as usize as *mut libc::c_void,
        iov_len: into.len(),
    };
    // SAFETY: `local` describes `into`, which outlives the call. `remote` is an
    // address in ANOTHER process and is never dereferenced here; an unmapped one
    // comes back as EFAULT rather than faulting this process.
    let read = unsafe { libc::process_vm_readv(pid as libc::pid_t, &local, 1, &remote, 1, 0) };
    if read == -1 {
        return Err(io::Error::last_os_error());
    }
    if read as usize != into.len() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "short read from the target"));
    }
    Ok(())
}

/// Reads one frame off the chain: the caller's frame pointer, and the address
/// to return to.
///
/// Split from the walk so the walk's stopping rules can be read — and tested —
/// without a process to read from.
pub(crate) fn decode_frame(bytes: [u8; 16]) -> (u64, u64) {
    let next = u64::from_le_bytes(bytes[0..8].try_into().unwrap_or([0; 8]));
    let return_address = u64::from_le_bytes(bytes[8..16].try_into().unwrap_or([0; 8]));
    (next, return_address)
}

/// Whether a frame pointer can be followed at all.
///
/// Three ways it cannot: null, unaligned — no ABI here produces one, so it is a
/// register holding something that is not a pointer — or not climbing, which is
/// a cycle. Following any of them invents frames, and an invented frame does not
/// announce itself: it just names the wrong function as the expensive one. A
/// short true stack beats a long false one.
pub(crate) fn can_follow(fp: u64, previous: u64) -> bool {
    fp != 0 && fp % 8 == 0 && fp > previous
}

/// Walks the frame chain from an interrupted thread, innermost first.
///
/// One frame is two words: at `[fp]` the caller's frame pointer and at `[fp+8]`
/// the address to return to. The walk stops at the first read that fails, at a
/// pointer `can_follow` rejects, and at `MAX_DEPTH`.
pub(crate) fn walk(pid: u32, regs: &Registers) -> Vec<u64> {
    let mut chain = vec![regs.pc];
    let mut fp = regs.fp;
    for _ in 0..MAX_DEPTH {
        if !can_follow(fp, 0) {
            break;
        }
        let mut frame = [0u8; 16];
        if read_memory(pid, fp, &mut frame).is_err() {
            break;
        }
        let (next, return_address) = decode_frame(frame);
        if return_address == 0 {
            break;
        }
        chain.push(return_address);
        if !can_follow(next, fp) {
            break;
        }
        fp = next;
    }
    chain
}

/// Stops one thread, reads its stack, and lets it go again.
///
/// The resume is unconditional past the interrupt, which is the point of the
/// wrapper: every failure between the stop and the read still has to give the
/// thread back. It was not always so: the interrupt and the wait were one call,
/// so a wait that failed returned before any resume and left the thread stopped
/// until the window ended and the detach released it. A single `EINTR` was
/// enough, and the symptom would have been a program that stalls for as long as
/// it is being profiled — which reads as the profiler being slow, not as the
/// profiler having stopped it.
pub(crate) fn sample_thread(pid: u32, tid: u32) -> Option<Vec<u64>> {
    // Below this line the thread is stopped, or on its way to being; above it,
    // nothing has happened to it.
    if interrupt(tid).is_err() {
        return None;
    }
    let stopped = wait_for_stop(tid);
    let signal = stopped.as_ref().map_or(0, |status| pending_signal(*status));
    // The walk happens HERE, between the stop and the resume, because it READS
    // THE TARGET'S MEMORY: up to `MAX_DEPTH` frames, a `PEEKDATA` pair each. The
    // registers are one instant; resuming before the walk made every frame after
    // them a different one. `can_follow` rejects the gross tears, not the
    // plausible ones — a stale record sitting above a stack that shrank between
    // two reads is aligned and still climbing, so it passes both tests and puts
    // a function on the stack that was not on it. That is the one failure a
    // profiler cannot afford, because nothing downstream can tell an invented
    // frame from a real one.
    let walked = stopped.and_then(|_| registers(tid)).map(|regs| walk(pid, &regs));
    resume(tid, signal);
    walked.ok()
}

/// How often a thread is stopped and read, per second.
///
/// Deliberately not a round one: a program doing something on a 100 Hz timer,
/// sampled at 100 Hz, is caught in the same place every time and reports one
/// function as the whole profile. 99 is coprime with 100 — and with 50, 20 and
/// 10 — so the sampler and the program cannot march in step.
///
/// It is also a budget. Every sample stops every thread, so the rate is what
/// the profiled program pays; this is the rate `py-spy` and `perf` default to
/// for the same reason.
const SAMPLE_HZ: u64 = 99;

/// Attaches to every thread of every process given, samples them for a window,
/// and detaches — returning the folded display stacks the rest of `monitor`
/// renders.
///
/// A tree, not one process, because `--attach` is documented to measure a
/// prefork server across all its workers and the macOS path does. The threads
/// of every process are read on every tick so each contributes samples in
/// proportion to the time it actually spent running.
///
/// The seizes happen once for the window rather than per sample: seizing is the
/// expensive half and the threads keep running between samples either way.
/// Threads and workers that appear mid-window are missed until the next one,
/// which is the price of not re-reading `/proc` at 99 Hz.
///
/// Failure DURING a window is silence, not an error. A thread that exits
/// mid-window, a stack that cannot be walked, a program that ends early — none
/// of them are worth refusing a window over, because the samples that DID land
/// are still true, and the caller learns the target is gone the way it always
/// has: an empty window.
///
/// A REFUSAL is an error, and that distinction is the whole reason this returns
/// a `Result`. The kernel saying no is not a quiet window: the operator has
/// something to fix, and both used to arrive as the same empty vector, so the
/// commonest refusal there is — `yama/ptrace_scope=1`, the default — was
/// reported as "the program may have exited" about a program still running.
///
/// Seizing NOTHING is not by itself a refusal, and reading it as one is the
/// mistake the first version of this made in the other direction: a live view
/// over a program that ends closed on "cannot attach to the target", because a
/// reaped process has an empty `/proc/<pid>/task` and a zombie has no `maps`.
/// `window_refusal` is what separates the two.
pub(crate) fn attach_window(
    pids: &[u32],
    duration_secs: u32,
    image: &super::attach::Image,
) -> Result<Vec<(Vec<(String, super::Kind)>, u64)>, String> {
    // Each process brings its own bias and its own threads; they share the
    // symbol table, because a prefork server's workers are forks of one image.
    // A process whose bias cannot be read is dropped rather than resolved
    // against a neighbour's, which would not fail — it would name the wrong
    // functions, and a table that is confidently wrong is worse than a short one.
    let mut targets: Vec<(u32, u64, Vec<u32>)> = Vec::new();
    // Kept, because a seize that FAILED is the thing the caller most needs told
    // apart from a window that sampled nothing, and the two used to arrive as
    // the same empty vector.
    let mut refusal: Option<io::Error> = None;
    for pid in pids {
        let Some(bias) = super::attach::bias_of(image, *pid) else { continue };
        let mut seized = Vec::new();
        for tid in thread_ids(*pid) {
            match seize(tid) {
                Ok(()) => seized.push(tid),
                // An `EPERM` outranks whatever else is held: a thread that
                // exited between the `/proc` read and the seize is ordinary and
                // says nothing, a refusal is the whole diagnosis.
                Err(error) => {
                    let held_is_refusal = matches!(
                        &refusal,
                        Some(held) if held.kind() == io::ErrorKind::PermissionDenied
                    );
                    if !held_is_refusal {
                        refusal = Some(error);
                    }
                }
            }
        }
        if !seized.is_empty() {
            targets.push((*pid, bias, seized));
        }
    }
    if targets.is_empty() {
        return match window_refusal(refusal) {
            Some(reason) => Err(reason),
            // Nothing was REFUSED — the target simply is not there any more, and
            // that is an empty window, which is how attach has always learnt its
            // target has gone.
            None => Ok(Vec::new()),
        };
    }
    let interval = std::time::Duration::from_nanos(1_000_000_000 / SAMPLE_HZ);
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(u64::from(duration_secs.max(1)));
    let mut stacks = Vec::new();
    while std::time::Instant::now() < deadline {
        let started = std::time::Instant::now();
        // Every process every tick, rather than one process for the whole
        // window each: a worker sampled for a third of the window contributes a
        // third of the samples, and its share of the table would be a third of
        // the truth.
        for (pid, bias, seized) in &targets {
            for tid in seized {
                let Some(chain) = sample_thread(*pid, *tid) else { continue };
                let stack = super::attach::display_stack(&chain, &image.symbols, *bias);
                if !stack.is_empty() {
                    stacks.push(stack);
                }
            }
        }
        // Sleep the remainder of the interval, not the whole of it: the samples
        // themselves take time, and sleeping a full interval on top of them
        // would drift the rate below the one documented above.
        if let Some(rest) = interval.checked_sub(started.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    for (_, _, seized) in &targets {
        for tid in seized {
            detach(*tid);
        }
    }
    Ok(super::attach::fold(stacks))
}

/// Whether nothing-seized is a refusal worth reporting, and what to say.
///
/// `None` means it is not: the target is gone. That distinction is finer than
/// the first version of this made it, which returned an error whenever nothing
/// was seized and so turned every ordinary ending into "cannot attach to the
/// target: it has no threads this tool can read" — a live view over a program
/// that exits closed on a refusal that never happened. Reaching this with
/// nothing held happens on the ordinary path: `thread_ids` finds an empty
/// `/proc/<pid>/task` for a process already reaped, and `bias_of` finds no
/// `maps` for a zombie, and neither of those is the kernel saying no.
///
/// `ESRCH` is the same answer arriving from the seize itself — the thread went
/// between the `/proc` read and the syscall — so it is a gone target too. What
/// is left is `EPERM` and the unexpected, and those the operator wants said.
fn window_refusal(error: Option<io::Error>) -> Option<String> {
    match error {
        None => None,
        Some(error) if error.raw_os_error() == Some(libc::ESRCH) => None,
        Some(error) => Some(seize_refusal(Some(error))),
    }
}

/// Says why nothing could be seized, in the operator's terms.
///
/// The default on most distributions is `yama/ptrace_scope=1`, which allows
/// attaching to descendants only — and `--attach` is handed a pid it did not
/// launch, so that is exactly the common case. Everything BEFORE the seize
/// succeeds under it: `/proc/<pid>/maps` and the executable are readable, so the
/// image is built and the symbols are loaded, and only then does every
/// `PTRACE_SEIZE` come back `EPERM`.
///
/// That refusal used to leave through the same door as an empty window, and the
/// empty window is how attach learns its target is gone — so the operator was
/// told "the program may have exited" about a program still running, and the
/// hint naming the one setting they had to change was never printed, because it
/// was only ever appended to an image error that had not occurred.
fn seize_refusal(error: Option<io::Error>) -> String {
    match error {
        Some(error) if error.kind() == io::ErrorKind::PermissionDenied => match attach_refusal_hint()
        {
            Some(hint) => format!("cannot attach to the target: {hint}"),
            // No yama file to read, so the refusal came from somewhere else —
            // a container without `CAP_SYS_PTRACE`, LSM policy, a setuid target.
            None => "cannot attach to the target: the kernel refused to trace it. Run as root, \
                     grant CAP_SYS_PTRACE, or start the program under `elephc monitor` instead"
                .to_string(),
        },
        Some(error) => format!("cannot attach to the target: {error}"),
        None => "cannot attach to the target: it has no threads this tool can read".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds the `waitpid` status word for a stopped tracee, the way the kernel
    /// encodes one: the signal in the second byte, `0x7f` in the first, and a
    /// `PTRACE_EVENT_*` — if the stop is one this tracer asked for — above them.
    fn stopped(signal: libc::c_int, event: libc::c_int) -> libc::c_int {
        ((signal | (event << 8)) << 8) | 0x7f
    }

    /// The stop `PTRACE_INTERRUPT` produces is `SIGTRAP` with `PTRACE_EVENT_STOP`
    /// over it, and every sample makes one. Handing that `SIGTRAP` back is not a
    /// harmless mistake: an untraced `SIGTRAP` terminates the process, so a
    /// profiler that re-injected it would kill its target on the first tick.
    #[test]
    fn the_stop_the_sampler_asked_for_is_not_a_signal_to_deliver() {
        assert_eq!(pending_signal(stopped(libc::SIGTRAP, 128)), 0);
    }

    /// A group-stop under `SEIZE` reports the stopping signal with the same
    /// event, so the signal in the status is not evidence on its own — the event
    /// bits are what separate a stop this tracer caused from one the program was
    /// about to receive.
    #[test]
    fn a_group_stop_is_not_a_signal_to_deliver_either() {
        assert_eq!(pending_signal(stopped(libc::SIGSTOP, 128)), 0);
    }

    /// The case the zero used to swallow. A seized tracee stops on EVERY signal
    /// and the tracer decides whether it arrives; passing zero to `PTRACE_CONT`
    /// throws it away, so a program being profiled lost the children it meant to
    /// reap and the shutdown it was asked for, with nothing to attribute it to.
    #[test]
    fn a_signal_the_program_was_about_to_receive_is_handed_back() {
        assert_eq!(pending_signal(stopped(libc::SIGCHLD, 0)), libc::SIGCHLD);
        assert_eq!(pending_signal(stopped(libc::SIGTERM, 0)), libc::SIGTERM);
    }

    /// A thread that exited between the interrupt and the wait reports an exit,
    /// not a stop, and there is nothing to deliver to it.
    #[test]
    fn an_exit_carries_no_signal() {
        assert_eq!(pending_signal(0), 0);
        assert_eq!(pending_signal(1 << 8), 0);
    }

    /// The wording was split out from reading `/proc` precisely so it could be
    /// tested, and then was not. Each setting has to name itself and say what to
    /// do, because a bare `EPERM` reads as a bug in this tool rather than as one
    /// line of configuration.
    #[test]
    fn each_ptrace_scope_names_itself_and_what_to_do() {
        assert_eq!(
            explain_ptrace_scope("0"),
            None,
            "an unrestricted kernel has nothing to explain, and saying something would send \
             an operator to change a setting that is already right"
        );
        let one = explain_ptrace_scope("1").expect("the default setting must be explained");
        assert!(one.contains("ptrace_scope is 1"), "{one}");
        assert!(one.contains("descendants only"), "{one}");
        assert!(one.contains("CAP_SYS_PTRACE"), "{one}");
        let two = explain_ptrace_scope("2").expect("the strict setting must be explained");
        assert!(two.contains("ptrace_scope is 2"), "{two}");
        assert!(two.contains("CAP_SYS_PTRACE"), "{two}");
        // 3 is "no attaching, ever, until reboot", and any future value this
        // tool has not heard of must still produce a refusal rather than silence.
        assert!(explain_ptrace_scope("3").is_some());
        assert!(explain_ptrace_scope("").is_some());
    }

    /// The message an operator gets when NOTHING could be seized. It has to tell
    /// a refusal apart from a target that went away, because attach reads an
    /// empty window as proof the program has ended — and reporting a refusal
    /// that way is how `yama/ptrace_scope=1`, the default nearly everywhere,
    /// came out as "the program may have exited" about a running program.
    #[test]
    fn a_refusal_reads_differently_from_a_target_that_went_away() {
        let refused = seize_refusal(Some(io::Error::from_raw_os_error(libc::EPERM)));
        assert!(refused.contains("cannot attach"), "{refused}");
        assert!(
            refused.contains("CAP_SYS_PTRACE") || refused.contains("ptrace_scope"),
            "a refusal must name something the operator can change: {refused}"
        );

        let gone = seize_refusal(Some(io::Error::from_raw_os_error(libc::ESRCH)));
        assert!(
            !gone.contains("CAP_SYS_PTRACE") && !gone.contains("ptrace_scope"),
            "a thread that exited is not a permission problem: {gone}"
        );

        let nothing = seize_refusal(None);
        assert!(nothing.contains("no threads"), "{nothing}");
    }

    /// Seizing nothing is not by itself a refusal.
    ///
    /// The first version of this returned an error whenever `targets` came back
    /// empty, which reads every ordinary ending as the kernel saying no: a
    /// reaped process has an empty `/proc/<pid>/task` and a zombie has no
    /// `maps`, so a live view over a program that exits closed on "cannot
    /// attach to the target" instead of on the empty window it has always used.
    /// `ESRCH` from the seize itself is the same target, gone between the
    /// `/proc` read and the syscall.
    #[test]
    fn a_target_that_went_away_is_an_empty_window_not_a_refusal() {
        assert!(window_refusal(None).is_none(), "nothing held is a gone target");
        assert!(
            window_refusal(Some(io::Error::from_raw_os_error(libc::ESRCH))).is_none(),
            "a thread that exited between the /proc read and the seize is a gone target"
        );

        let refused = window_refusal(Some(io::Error::from_raw_os_error(libc::EPERM)))
            .expect("the kernel saying no is worth reporting");
        assert!(refused.contains("cannot attach"), "{refused}");

        // Neither gone nor a permission problem: unexpected, and the operator
        // still wants it said rather than shown an empty table.
        assert!(
            window_refusal(Some(io::Error::from_raw_os_error(libc::EIO))).is_some(),
            "an unexpected failure must not be reported as a target that ended"
        );
    }

    /// A target that cannot be seized is an ERROR, not an empty window.
    ///
    /// This pins the regression itself rather than its wording. Attach reads an
    /// empty window as proof the target has gone, so a refusal and a quiet
    /// window must not share a value — they did, and that is how
    /// `yama/ptrace_scope=1` came out as "the program may have exited".
    ///
    /// Driven against a REAL refused seize, deterministically and without any
    /// capability, by pointing the sampler at THIS process. A task cannot trace
    /// its own thread group, so everything BEFORE the seize succeeds — the image
    /// is ours, `/proc/self/maps` is ours, so `bias_of` returns a bias and the
    /// loop reaches the seize — and only then is every `PTRACE_SEIZE` refused.
    /// That is the same shape scope 1 produces for a process this tool did not
    /// launch, which is the case no container flag can stage: `--cap-add=
    /// SYS_PTRACE` grants exactly what stops it happening.
    #[test]
    fn a_target_that_cannot_be_seized_is_an_error_not_an_empty_window() {
        let me = std::process::id();
        let Ok(image) = super::super::attach::image_for(me) else {
            // A host whose own `/proc` this cannot read says nothing about the
            // branch under test, and failing here would report the wrong thing.
            return;
        };
        match attach_window(&[me], 1, &image) {
            Err(reason) => assert!(
                reason.starts_with("cannot attach to the target"),
                "a refusal has to say it could not attach: {reason}"
            ),
            Ok(window) => panic!(
                "a refused seize came back as a window of {} stacks, which every caller reads \
                 as a target that has exited",
                window.len()
            ),
        }
    }
}
