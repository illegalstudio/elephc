//! Purpose:
//! Master fd-dispatch backend for `--dispatch master`: the raw-libc SCM_RIGHTS
//! fd-passing primitives (`socketpair_cloexec`, `send_fd`, `recv_fd`,
//! `send_ready`/`recv_ready`) plus the synchronous `master_loop` that accepts on
//! a single listener and hands each connection to an idle worker over a per-worker
//! Unix socketpair. This is a strictly parallel alternative to the kernel
//! (SO_REUSEPORT) dispatch path; the kernel path is never routed through here.
//!
//! Called from:
//! - `crate::server::run_master` selects `master_loop` (instead of `supervise`)
//!   when `--dispatch master` is set, in all three web modes.
//! - `crate::worker::serve` / `crate::worker_mode::enter_worker_loop` read the
//!   child socketpair end via `take_child_dispatch_chan` and call `recv_fd` /
//!   `send_ready` from the per-worker serve loop.
//!
//! Key details:
//! - ALL OS divergence for the campaign lives here, behind `#[cfg(target_os)]`:
//!   Linux uses `SOCK_CLOEXEC` / `accept4` / `MSG_NOSIGNAL` / `MSG_CMSG_CLOEXEC`;
//!   macOS uses post-hoc `fcntl(F_SETFD, FD_CLOEXEC)` / `accept` / `SO_NOSIGPIPE`.
//! - cmsg buffers are ALWAYS sized/walked with the libc `CMSG_SPACE` / `CMSG_LEN`
//!   / `CMSG_DATA` / `CMSG_FIRSTHDR` / `CMSG_NXTHDR` macros; alignment is never
//!   hand-computed (Linux aligns on `size_t`, macOS on `u32`).
//! - Slot = 1: a worker sends one READY byte (`b'R'`) at startup and again only
//!   when its assigned connection's `serve_connection` fully completes, so a slow
//!   request never blocks connections pre-assigned to other workers.
//! - Queue-full is SYN backpressure (the master stops polling the listener), never
//!   an HTTP 503 — the master writes no HTTP bytes and drops no accepted fd.

use std::collections::VecDeque;
use std::ffi::{c_int, c_uint, c_void};
use std::io;
use std::mem::size_of;
use std::os::fd::{AsRawFd, RawFd};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::server::{classify_crash, spawn_worker, WorkerKind, MAX_FAST_DEATHS};
use crate::worker::WorkerConfig;

/// Which connection-dispatch backend the server uses. `Kernel` is the default,
/// behaviorally-identical SO_REUSEPORT path; `Master` selects `master_loop`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DispatchMode {
    /// Per-worker SO_REUSEPORT listeners; the kernel picks a worker at the SYN.
    Kernel,
    /// The master accepts on one listener and passes each fd to an idle worker.
    Master,
}

/// Master-dispatch configuration, kept SEPARATE from the `Copy` `WorkerConfig`
/// (which threads per-request limits into the workers) so the worker config does
/// not grow a master-only field. `backlog` bounds the master's internal queue of
/// accepted-but-undispatched fds.
#[derive(Clone, Copy)]
pub(crate) struct DispatchConfig {
    /// Which dispatch backend is active.
    pub mode: DispatchMode,
    /// Master mode only: max accepted connections queued while all workers are
    /// busy before the master stops polling the listener (SYN backpressure).
    pub backlog: usize,
}

// ---------------------------------------------------------------------------
// Child socketpair-end slot (installed by `spawn_worker` before boot/serve).
// ---------------------------------------------------------------------------

/// Process-static child end of the master↔worker socketpair, installed by
/// `spawn_worker` in the forked child (master mode only) before the serve loop /
/// boot runs — the same pattern as `worker_mode::set_worker_listen`. Its presence
/// is what tells `worker::serve` / `enter_worker_loop` to use `ConnSource::Master`
/// (and NOT bind a listener). Accessed only through `addr_of_mut!`.
static mut CHILD_DISPATCH_CHAN: Option<RawFd> = None;

/// Stores the child socketpair-end fd in the process-static slot. Called by
/// `spawn_worker` in the forked child after fork and before boot/serve, so the
/// serve loop can retrieve it. Writes through `addr_of_mut!`; never borrows the
/// `static mut`.
pub(crate) fn set_child_dispatch_chan(fd: RawFd) {
    // SAFETY: single-threaded per worker; written through a raw pointer.
    unsafe {
        ptr::write(ptr::addr_of_mut!(CHILD_DISPATCH_CHAN), Some(fd));
    }
}

/// Takes the stored child socketpair-end fd, returning `None` in kernel mode
/// (never set) so the serve loop binds a SO_REUSEPORT listener as before. Reads
/// and clears the slot through `addr_of_mut!`.
pub(crate) fn take_child_dispatch_chan() -> Option<RawFd> {
    // SAFETY: single-threaded per worker; taken through a raw pointer.
    unsafe { (*ptr::addr_of_mut!(CHILD_DISPATCH_CHAN)).take() }
}

// ---------------------------------------------------------------------------
// Target-specific primitives (the ONLY place `#[cfg(target_os)]` appears).
// ---------------------------------------------------------------------------

/// Returns the `socketpair(2)` type argument: `SOCK_STREAM | SOCK_CLOEXEC` on
/// Linux (atomic close-on-exec), plain `SOCK_STREAM` on macOS (which has no
/// `SOCK_CLOEXEC`; `post_socketpair` sets `FD_CLOEXEC` afterwards).
#[cfg(target_os = "linux")]
fn socket_stream_type() -> c_int {
    libc::SOCK_STREAM | libc::SOCK_CLOEXEC
}

/// See the Linux variant. macOS lacks `SOCK_CLOEXEC` for `socketpair`.
#[cfg(target_os = "macos")]
fn socket_stream_type() -> c_int {
    libc::SOCK_STREAM
}

/// Post-creation setup for a fresh socketpair. Linux: nothing (CLOEXEC came from
/// `SOCK_CLOEXEC`; SIGPIPE is suppressed per-send via `MSG_NOSIGNAL`). macOS: set
/// `FD_CLOEXEC` on both ends and `SO_NOSIGPIPE` on both ends so a write to a dead
/// peer returns `EPIPE` instead of raising `SIGPIPE` (macOS has no `MSG_NOSIGNAL`).
#[cfg(target_os = "linux")]
fn post_socketpair(_a: RawFd, _b: RawFd) {}

/// See the Linux variant. macOS needs explicit `FD_CLOEXEC` + `SO_NOSIGPIPE`.
#[cfg(target_os = "macos")]
fn post_socketpair(a: RawFd, b: RawFd) {
    for fd in [a, b] {
        // SAFETY: fd is a freshly created, valid socket end.
        unsafe {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
            let on: c_int = 1;
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_NOSIGPIPE,
                &on as *const c_int as *const c_void,
                size_of::<c_int>() as libc::socklen_t,
            );
        }
    }
}

/// Flags for `sendmsg`/`send` on the socketpair: `MSG_NOSIGNAL` on Linux so a
/// write to a dead worker/master yields `EPIPE` rather than `SIGPIPE`; `0` on
/// macOS (SIGPIPE already suppressed per-socket by `SO_NOSIGPIPE`).
#[cfg(target_os = "linux")]
fn send_flags() -> c_int {
    libc::MSG_NOSIGNAL
}

/// See the Linux variant. macOS relies on `SO_NOSIGPIPE`.
#[cfg(target_os = "macos")]
fn send_flags() -> c_int {
    0
}

/// Flags for `recvmsg`: `MSG_CMSG_CLOEXEC` on Linux so the received fd is atomic
/// close-on-exec; `0` on macOS (which has no such flag — `recv_fd` sets
/// `FD_CLOEXEC` explicitly afterwards via `set_cloexec`).
#[cfg(target_os = "linux")]
fn recv_flags() -> c_int {
    libc::MSG_CMSG_CLOEXEC
}

/// See the Linux variant. macOS lacks `MSG_CMSG_CLOEXEC`.
#[cfg(target_os = "macos")]
fn recv_flags() -> c_int {
    0
}

/// Sets `FD_CLOEXEC` on a received fd. No-op on Linux (`MSG_CMSG_CLOEXEC` already
/// applied it during `recvmsg`); on macOS this is the explicit close-on-exec step.
#[cfg(target_os = "linux")]
fn set_cloexec(_fd: RawFd) {}

/// See the Linux variant. macOS must set `FD_CLOEXEC` by hand.
#[cfg(target_os = "macos")]
fn set_cloexec(fd: RawFd) {
    // SAFETY: fd is a valid, just-received descriptor.
    unsafe {
        libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
    }
}

/// Accepts one connection off the master's listener with close-on-exec set.
/// Linux uses `accept4(SOCK_CLOEXEC)` (atomic); macOS uses `accept` then
/// `fcntl(F_SETFD, FD_CLOEXEC)`. Returns the raw fd or the underlying `io::Error`
/// (including `EWOULDBLOCK` when the nonblocking listener has no pending
/// connection), which `accept_conn` classifies.
#[cfg(target_os = "linux")]
fn accept_raw(listener: RawFd) -> io::Result<RawFd> {
    // SAFETY: listener is a valid, bound, listening socket fd.
    let fd = unsafe {
        libc::accept4(listener, ptr::null_mut(), ptr::null_mut(), libc::SOCK_CLOEXEC)
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fd)
}

/// See the Linux variant. macOS has no `accept4`, so set `FD_CLOEXEC` afterwards.
#[cfg(target_os = "macos")]
fn accept_raw(listener: RawFd) -> io::Result<RawFd> {
    // SAFETY: listener is a valid, bound, listening socket fd.
    let fd = unsafe { libc::accept(listener, ptr::null_mut(), ptr::null_mut()) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    set_cloexec(fd);
    Ok(fd)
}

/// Sets `SO_NOSIGPIPE` on a received TCP stream fd (macOS only) so hyper's writes
/// to a client that vanished return `EPIPE` instead of raising `SIGPIPE`. The fd
/// was created by the master's `accept` (not by tokio, which would have set the
/// option), so the worker must set it. No-op on Linux.
#[cfg(target_os = "linux")]
fn set_stream_nosigpipe(_fd: RawFd) {}

/// See the Linux variant. macOS TCP writes SIGPIPE without `SO_NOSIGPIPE`.
#[cfg(target_os = "macos")]
fn set_stream_nosigpipe(fd: RawFd) {
    // SAFETY: fd is a valid, connected TCP socket received over SCM_RIGHTS.
    unsafe {
        let on: c_int = 1;
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_NOSIGPIPE,
            &on as *const c_int as *const c_void,
            size_of::<c_int>() as libc::socklen_t,
        );
    }
}

// ---------------------------------------------------------------------------
// Portable fd helpers.
// ---------------------------------------------------------------------------

/// Sets `O_NONBLOCK` on `fd`. Used on the worker's socketpair end (required by
/// `tokio::io::unix::AsyncFd`) and on each received TCP stream fd (accept does not
/// guarantee the nonblocking flag is inherited across SCM_RIGHTS).
pub(crate) fn set_nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: fd is a valid descriptor owned by the caller.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: as above; setting the nonblocking bit.
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Prepares a TCP stream fd received over SCM_RIGHTS for tokio: makes it
/// nonblocking (required by `TcpStream::from_std`) and, on macOS, sets
/// `SO_NOSIGPIPE`. Called by the worker before wrapping the fd in a tokio stream.
pub(crate) fn prepare_received_fd(fd: RawFd) -> io::Result<()> {
    set_nonblocking(fd)?;
    set_stream_nosigpipe(fd);
    Ok(())
}

// ---------------------------------------------------------------------------
// fd-passing primitives (WI-1).
// ---------------------------------------------------------------------------

/// Creates an `AF_UNIX`/`SOCK_STREAM` socketpair with close-on-exec (and, on
/// macOS, `SO_NOSIGPIPE`) set on both ends. One end stays with the master, the
/// other is installed in the forked child. Returns `(a, b)` raw fds; the caller
/// owns and closes them.
pub(crate) fn socketpair_cloexec() -> io::Result<(RawFd, RawFd)> {
    let mut fds = [0 as RawFd; 2];
    // SAFETY: socketpair(2) fills a 2-element array; return checked below.
    let rc = unsafe { libc::socketpair(libc::AF_UNIX, socket_stream_type(), 0, fds.as_mut_ptr()) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    post_socketpair(fds[0], fds[1]);
    Ok((fds[0], fds[1]))
}

/// Sends `fd` to the peer over `chan` via `sendmsg` + a single `SCM_RIGHTS`
/// control message (1 data byte `b'F'`). The ancillary buffer is sized with
/// `CMSG_SPACE(sizeof(c_int))` and populated through `CMSG_FIRSTHDR` / `CMSG_LEN`
/// / `CMSG_DATA` — never hand-aligned. Returns `Err(EPIPE/ECONNRESET)` when the
/// worker died (SIGPIPE suppressed via `send_flags`/`SO_NOSIGPIPE`); the caller
/// keeps the fd and re-dispatches it to the next idle worker.
pub(crate) fn send_fd(chan: RawFd, fd: RawFd) -> io::Result<()> {
    let mut byte: [u8; 1] = [b'F'];
    let mut iov = libc::iovec {
        iov_base: byte.as_mut_ptr() as *mut c_void,
        iov_len: 1,
    };
    // SAFETY: CMSG_SPACE is a pure size computation over a constant length.
    let cmsg_space = unsafe { libc::CMSG_SPACE(size_of::<c_int>() as c_uint) } as usize;
    let mut cmsg_buf = vec![0u8; cmsg_space];
    // SAFETY: msghdr is a plain-old-data struct; zeroing is a valid init.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = cmsg_buf.as_mut_ptr() as *mut c_void;
    msg.msg_controllen = cmsg_space as _;
    // SAFETY: msg_control points at a buffer >= CMSG_SPACE(4), so the first header
    // is in-bounds; the fd is copied into CMSG_DATA using the libc macros.
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        if cmsg.is_null() {
            return Err(io::Error::new(io::ErrorKind::Other, "CMSG_FIRSTHDR returned null"));
        }
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(size_of::<c_int>() as c_uint) as _;
        ptr::copy_nonoverlapping(
            &fd as *const RawFd as *const u8,
            libc::CMSG_DATA(cmsg),
            size_of::<c_int>(),
        );
    }
    // SAFETY: msg is fully initialized above; sendmsg reads it.
    let n = unsafe { libc::sendmsg(chan, &msg, send_flags()) };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Receives one fd from `chan` via `recvmsg`. Returns `Ok(Some(fd))` on success,
/// `Ok(None)` on EOF (the master closed its end → the worker exits cleanly), and
/// `Err(WouldBlock)` when the nonblocking socket has nothing pending (so the
/// tokio `AsyncFd` guard retries). Validates `SOL_SOCKET` + `SCM_RIGHTS`, applies
/// close-on-exec (`set_cloexec` on macOS; `MSG_CMSG_CLOEXEC` on Linux), closes any
/// surplus fds to avoid leaks, and errors on a truncated control message.
pub(crate) fn recv_fd(chan: RawFd) -> io::Result<Option<RawFd>> {
    let mut byte = [0u8; 1];
    let mut iov = libc::iovec {
        iov_base: byte.as_mut_ptr() as *mut c_void,
        iov_len: 1,
    };
    // SAFETY: CMSG_SPACE is a pure size computation over a constant length.
    let cmsg_space = unsafe { libc::CMSG_SPACE(size_of::<c_int>() as c_uint) } as usize;
    let mut cmsg_buf = vec![0u8; cmsg_space];
    // SAFETY: msghdr is plain-old-data; zeroing is a valid init.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = cmsg_buf.as_mut_ptr() as *mut c_void;
    msg.msg_controllen = cmsg_space as _;
    // SAFETY: msg is initialized; recvmsg writes the byte, control data, and flags.
    let n = unsafe { libc::recvmsg(chan, &mut msg, recv_flags()) };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    if n == 0 {
        // Peer closed the socketpair: the master is gone.
        return Ok(None);
    }
    let mut received: Option<RawFd> = None;
    // SAFETY: walk the control buffer with the libc macros only; each header the
    // macros return is in-bounds for the buffer we supplied.
    unsafe {
        let mut cmsg = libc::CMSG_FIRSTHDR(&msg);
        while !cmsg.is_null() {
            if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                let header_len = libc::CMSG_LEN(0) as usize;
                let payload_len = (*cmsg).cmsg_len as usize - header_len;
                let count = payload_len / size_of::<c_int>();
                let data = libc::CMSG_DATA(cmsg);
                for k in 0..count {
                    let mut got: c_int = 0;
                    ptr::copy_nonoverlapping(
                        data.add(k * size_of::<c_int>()),
                        &mut got as *mut c_int as *mut u8,
                        size_of::<c_int>(),
                    );
                    if received.is_none() {
                        received = Some(got);
                        set_cloexec(got);
                    } else {
                        // A well-behaved sender passes exactly one fd; close any
                        // surplus so it does not leak into the worker.
                        libc::close(got);
                    }
                }
            }
            cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
        }
    }
    if (msg.msg_flags & libc::MSG_CTRUNC) != 0 {
        // The kernel truncated the ancillary data: an fd may have been dropped.
        if let Some(fd) = received {
            // SAFETY: fd is a valid received descriptor we own.
            unsafe { libc::close(fd); }
        }
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "received truncated SCM_RIGHTS control message",
        ));
    }
    match received {
        Some(fd) => Ok(Some(fd)),
        None => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message carried no SCM_RIGHTS fd",
        )),
    }
}

/// Sends the 1-byte READY signal (`b'R'`) to the master. SIGPIPE is suppressed
/// via `send_flags`/`SO_NOSIGPIPE`, so a dead master yields `Err(EPIPE)` (the
/// worker then hits EOF on `recv_fd` and exits) rather than a signal.
pub(crate) fn send_ready(chan: RawFd) -> io::Result<()> {
    let byte: [u8; 1] = [b'R'];
    // SAFETY: chan is a valid socketpair end; sending one byte, no ancillary.
    let n = unsafe { libc::send(chan, byte.as_ptr() as *const c_void, 1, send_flags()) };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Reads the master's side of the idle protocol: `Ok(Some(byte))` for a READY
/// signal, `Ok(None)` on EOF (the worker closed its end → it is gone). Called by
/// `master_loop` after `poll` reports the socketpair readable.
fn recv_ready(chan: RawFd) -> io::Result<Option<u8>> {
    let mut byte = [0u8; 1];
    // SAFETY: chan is a valid socketpair end; reading one byte.
    let n = unsafe { libc::recv(chan, byte.as_mut_ptr() as *mut c_void, 1, 0) };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    if n == 0 {
        return Ok(None);
    }
    Ok(Some(byte[0]))
}

/// Accepts one pending connection off the master's nonblocking listener. Returns
/// `Ok(Some(fd))` for a connection, `Ok(None)` when there is nothing to accept
/// right now (`EWOULDBLOCK`/`EAGAIN`) or the accept was interrupted/aborted
/// (`EINTR`/`ECONNABORTED`, retryable next poll), and `Err` on a real failure.
pub(crate) fn accept_conn(listener: RawFd) -> io::Result<Option<RawFd>> {
    match accept_raw(listener) {
        Ok(fd) => Ok(Some(fd)),
        Err(e) => match e.raw_os_error() {
            Some(code)
                if code == libc::EWOULDBLOCK
                    || code == libc::EAGAIN
                    || code == libc::EINTR
                    || code == libc::ECONNABORTED =>
            {
                Ok(None)
            }
            _ => Err(e),
        },
    }
}

// ---------------------------------------------------------------------------
// Master supervision loop (WI-4 + WI-5).
// ---------------------------------------------------------------------------

/// A worker tracked by the master in `--dispatch master` mode: its pid, the
/// master end of its socketpair, and (in worker/script modes) the boot-signal
/// pipe read end used by `classify_crash`. `spawned_at` feeds the classic-mode
/// `FAST_DEATH` timing heuristic.
pub(crate) struct MasterWorker {
    /// The worker process pid.
    pid: libc::pid_t,
    /// The master's end of the master↔worker socketpair.
    chan: RawFd,
    /// Boot-signal pipe read end (worker/script modes), else `None` (classic).
    boot_pipe_rd: Option<i32>,
    /// When the worker was spawned, for the classic-mode crash-loop heuristic.
    spawned_at: Instant,
}

impl MasterWorker {
    /// Builds a tracked worker record, stamping the spawn time as now.
    pub(crate) fn new(pid: libc::pid_t, chan: RawFd, boot_pipe_rd: Option<i32>) -> Self {
        MasterWorker {
            pid,
            chan,
            boot_pipe_rd,
            spawned_at: Instant::now(),
        }
    }

    /// Returns the master end of this worker's socketpair (for sibling-close on a
    /// later fork, so a child never inherits another worker's master end).
    pub(crate) fn chan(&self) -> RawFd {
        self.chan
    }

    /// Returns the boot-signal pipe read end, if any (worker/script modes). Used
    /// so a later-forked child closes its inherited copies of sibling boot pipes
    /// (an analogous inherited-fd leak to the socketpair master ends).
    pub(crate) fn boot_pipe_rd(&self) -> Option<i32> {
        self.boot_pipe_rd
    }
}

/// Set by the master's SIGCHLD handler so the poll loop reaps and respawns dead
/// workers. Async-signal-safe: the handler only stores to it.
static MASTER_SIGCHLD: AtomicBool = AtomicBool::new(false);

/// Async-signal-safe SIGCHLD handler: records that a child changed state so the
/// master's poll loop performs a `waitpid(WNOHANG)` sweep.
extern "C" fn handle_sigchld(_sig: c_int) {
    MASTER_SIGCHLD.store(true, Ordering::SeqCst);
}

/// Installs `handle_sigchld` for SIGCHLD WITHOUT `SA_RESTART` (so a pending
/// `poll` returns `EINTR` and the loop reaps) and with `SA_NOCLDSTOP` (ignore
/// child stop/continue). Called once at the top of `master_loop`.
fn install_sigchld_handler() {
    // SAFETY: installs a static async-signal-safe handler; matches the pattern in
    // `server::install_signal_handlers`.
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handle_sigchld as extern "C" fn(c_int) as libc::sighandler_t;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = libc::SA_NOCLDSTOP;
        libc::sigaction(libc::SIGCHLD, &sa, ptr::null_mut());
    }
}

/// Collects every fd a forked/respawned master-mode child must close so it does
/// not inherit a live copy: each still-live sibling's socketpair master end and
/// boot-pipe read end, the listener fd, and every accepted-but-undispatched
/// connection fd currently queued in `backlog`. Inheriting a sibling master end
/// would break master-crash EOF detection; inheriting the listener or queued
/// connection fds (which the child never serves) would leak them and defer client
/// teardown under saturation + respawn. `FD_CLOEXEC` does not help — this is fork
/// without exec.
fn child_close_fds(
    slots: &[Option<MasterWorker>],
    listener_fd: RawFd,
    backlog: &VecDeque<RawFd>,
) -> Vec<i32> {
    let mut fds: Vec<i32> = Vec::new();
    for w in slots.iter().flatten() {
        fds.push(w.chan);
        if let Some(rd) = w.boot_pipe_rd {
            fds.push(rd);
        }
    }
    fds.push(listener_fd);
    fds.extend(backlog.iter().copied());
    fds
}

/// Attempts to hand `fd` to the worker in `slot`. Returns `true` on success (the
/// fd was sent and the master's copy closed — the worker now owns it); `false` if
/// the slot is empty or the send failed (the worker died between poll and send).
/// Does NOT touch the backlog: the caller decides whether to try another idle
/// worker or queue the fd, so a stranded connection is never left behind a single
/// just-crashed worker.
fn try_send_fd_to(slots: &[Option<MasterWorker>], slot: usize, fd: RawFd) -> bool {
    let chan = match slots[slot].as_ref() {
        Some(w) => w.chan,
        None => return false,
    };
    match send_fd(chan, fd) {
        Ok(()) => {
            // SAFETY: fd is owned by the master until handed off; the worker now
            // owns the passed copy, so close the master's.
            unsafe { libc::close(fd); }
            true
        }
        Err(_) => false,
    }
}

/// Reaps every dead worker (`waitpid(WNOHANG)` sweep), classifies each death as
/// startup vs runtime via `classify_crash`, removes it from the ready queue, and
/// respawns it IN PLACE with a fresh socketpair re-registered in the poll set.
/// Bumps `fast_deaths` on startup crashes and sets `give_up` past
/// `MAX_FAST_DEATHS`; resets the counter on a runtime crash. Mirrors
/// `server::supervise`'s crash-loop policy.
fn reap_and_respawn(
    slots: &mut Vec<Option<MasterWorker>>,
    ready: &mut VecDeque<usize>,
    backlog: &VecDeque<RawFd>,
    listen: &str,
    kind: &WorkerKind,
    cfg: WorkerConfig,
    listener_fd: RawFd,
    fast_deaths: &mut u32,
    give_up: &mut bool,
) {
    loop {
        let mut status = 0;
        // SAFETY: reap any exited child without blocking.
        let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
        if pid <= 0 {
            // 0 = no more zombies; -1 = ECHILD/EINTR.
            break;
        }
        let slot = match slots
            .iter()
            .position(|w| w.as_ref().map(|x| x.pid) == Some(pid))
        {
            Some(i) => i,
            None => continue,
        };
        let dead = slots[slot].take().expect("slot matched a live worker");
        ready.retain(|&r| r != slot);
        let is_startup = classify_crash(dead.boot_pipe_rd, dead.spawned_at);
        // SAFETY: the master owns these fds; close them now that the worker is gone.
        unsafe {
            if let Some(rd) = dead.boot_pipe_rd {
                libc::close(rd);
            }
            libc::close(dead.chan);
        }
        if is_startup {
            *fast_deaths += 1;
            if *fast_deaths >= MAX_FAST_DEATHS {
                eprintln!(
                    "elephc-web: {} workers died on startup (likely a bad --listen or a \
                     handler crashing every request); giving up",
                    *fast_deaths
                );
                *give_up = true;
                return;
            }
        } else {
            *fast_deaths = 0;
        }
        let close_fds = child_close_fds(slots, listener_fd, backlog);
        let (new_pid, new_rd, new_chan) = spawn_worker(listen, kind.clone(), cfg, true, &close_fds);
        let new_chan = new_chan.expect("master respawn must return a socketpair master end");
        slots[slot] = Some(MasterWorker::new(new_pid, new_chan, new_rd));
    }
}

/// SIGTERMs, reaps, and closes the fds of every worker in `workers`. Used both by
/// the master's shutdown `teardown` and by `server::run_master` when the listener
/// fails to bind after the workers were already forked.
pub(crate) fn reap_workers(workers: &[MasterWorker]) {
    for w in workers {
        // SAFETY: signal each tracked worker to terminate.
        unsafe { libc::kill(w.pid, libc::SIGTERM); }
    }
    for w in workers {
        let mut status = 0;
        // SAFETY: reap the worker, then close its master-owned fds.
        unsafe {
            libc::waitpid(w.pid, &mut status, 0);
            libc::close(w.chan);
            if let Some(rd) = w.boot_pipe_rd {
                libc::close(rd);
            }
        }
    }
}

/// Closes the listener, RSTs any queued (accepted-but-undispatched) connections,
/// then SIGTERMs and reaps every tracked worker. Called on shutdown (SIGINT/
/// SIGTERM) or crash-loop give-up. Mirrors `server::supervise`'s teardown.
fn teardown(listener: std::net::TcpListener, slots: &[Option<MasterWorker>], backlog: &mut VecDeque<RawFd>) {
    drop(listener); // closes the listening socket
    for fd in backlog.drain(..) {
        // SAFETY: each queued fd is a master-owned accepted connection.
        unsafe { libc::close(fd); }
    }
    let live: Vec<&MasterWorker> = slots.iter().flatten().collect();
    for w in &live {
        // SAFETY: signal each tracked worker to terminate.
        unsafe { libc::kill(w.pid, libc::SIGTERM); }
    }
    for w in &live {
        let mut status = 0;
        // SAFETY: reap the worker, then close its master-owned fds.
        unsafe {
            libc::waitpid(w.pid, &mut status, 0);
            libc::close(w.chan);
            if let Some(rd) = w.boot_pipe_rd {
                libc::close(rd);
            }
        }
    }
}

/// The synchronous master dispatch loop for `--dispatch master`. Owns the single
/// listener; `poll(2)`s over { listener (dropped from the set while the backlog is
/// full, for SYN backpressure), every worker socketpair }; hands each accepted fd
/// to an idle worker or queues it; and reaps/respawns dead workers (SIGCHLD).
/// Runs INSTEAD of `server::supervise` in master mode; the kernel path is never
/// routed here. Returns the master exit code (0 clean, 1 crash-loop give-up).
///
/// `listen`/`kind`/`cfg` are threaded through for respawn (each respawn creates a
/// fresh socketpair); `dispatch.backlog` bounds the internal fd queue.
pub(crate) fn master_loop(
    listener: std::net::TcpListener,
    listen: &str,
    kind: WorkerKind,
    cfg: WorkerConfig,
    dispatch: DispatchConfig,
    workers: Vec<MasterWorker>,
) -> i32 {
    install_sigchld_handler();
    let listener_fd = listener.as_raw_fd();
    // Stable slots: a dead worker's slot becomes `None` and is respawned in place,
    // so ready-queue indices stay valid across respawns.
    let mut slots: Vec<Option<MasterWorker>> = workers.into_iter().map(Some).collect();
    let mut ready: VecDeque<usize> = VecDeque::new();
    let mut backlog: VecDeque<RawFd> = VecDeque::new();
    let mut fast_deaths: u32 = 0;
    let mut give_up = false;

    loop {
        if crate::server::shutdown_requested() {
            break;
        }
        if MASTER_SIGCHLD.swap(false, Ordering::SeqCst) {
            reap_and_respawn(
                &mut slots,
                &mut ready,
                &backlog,
                listen,
                &kind,
                cfg,
                listener_fd,
                &mut fast_deaths,
                &mut give_up,
            );
            if give_up {
                break;
            }
        }
        // Build the poll set: listener first (only when the backlog has room), then
        // every live worker's socketpair. `idx_map[k]` is the slot index of the
        // (base + k)-th pollfd.
        let poll_listener = backlog.len() < dispatch.backlog;
        let mut pfds: Vec<libc::pollfd> = Vec::with_capacity(slots.len() + 1);
        if poll_listener {
            pfds.push(libc::pollfd {
                fd: listener_fd,
                events: libc::POLLIN,
                revents: 0,
            });
        }
        let mut idx_map: Vec<usize> = Vec::with_capacity(slots.len());
        for (i, w) in slots.iter().enumerate() {
            if let Some(w) = w {
                pfds.push(libc::pollfd {
                    fd: w.chan,
                    events: libc::POLLIN,
                    revents: 0,
                });
                idx_map.push(i);
            }
        }
        // SAFETY: pfds is a valid, correctly-sized array; -1 = block until an fd is
        // ready or a signal (no SA_RESTART) interrupts with EINTR.
        let n = unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, -1) };
        if crate::server::shutdown_requested() {
            break;
        }
        if n < 0 {
            // EINTR (signal) or a transient error: re-check flags at the top.
            continue;
        }
        let base = if poll_listener { 1 } else { 0 };
        // Worker socketpairs: a READY byte frees the worker; EOF/HUP means it died.
        for (k, &slot) in idx_map.iter().enumerate() {
            let re = pfds[base + k].revents;
            if re == 0 {
                continue;
            }
            if re & libc::POLLIN != 0 {
                let chan = match slots[slot].as_ref() {
                    Some(w) => w.chan,
                    None => continue,
                };
                match recv_ready(chan) {
                    Ok(Some(b'R')) => {
                        if let Some(fd) = backlog.pop_front() {
                            // The freeing worker takes the oldest queued connection.
                            // If the send fails (it died in the same tick) keep the
                            // fd at the front for the next idle worker / respawn and
                            // request a reap; `ready` stays empty so the queue is
                            // still drained by a future READY.
                            if !try_send_fd_to(&slots, slot, fd) {
                                backlog.push_front(fd);
                                MASTER_SIGCHLD.store(true, Ordering::SeqCst);
                            }
                        } else {
                            ready.push_back(slot);
                        }
                    }
                    Ok(Some(_)) => { /* protocol only sends b'R'; ignore stray byte */ }
                    Ok(None) => {
                        // EOF: the worker closed its end (it is gone). Drop it from
                        // the ready queue NOW so the accept branch below never hands
                        // a connection to a dead worker; the reap sweep respawns it.
                        ready.retain(|&r| r != slot);
                        MASTER_SIGCHLD.store(true, Ordering::SeqCst);
                    }
                    Err(_) => { /* transient recv error: worker retries or dies */ }
                }
            } else if re & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
                // Worker died while (possibly) idle: remove it from the ready queue
                // immediately, then let the reap sweep respawn it.
                ready.retain(|&r| r != slot);
                MASTER_SIGCHLD.store(true, Ordering::SeqCst);
            }
        }
        // Listener: drain all pending connections, dispatching to idle workers or
        // queuing; stop early (leaving the rest in the SYN backlog) when the queue
        // fills — that connection stays in the kernel until a worker frees.
        if poll_listener && (pfds[0].revents & libc::POLLIN) != 0 {
            loop {
                match accept_conn(listener_fd) {
                    Ok(Some(fd)) => {
                        // Try idle workers in order; a worker that died between poll
                        // and send (send fails) is already popped from `ready`, so we
                        // just request a reap and try the NEXT idle worker rather than
                        // stranding the connection behind one just-crashed worker.
                        let mut dispatched = false;
                        while let Some(slot) = ready.pop_front() {
                            if slots[slot].is_none() {
                                continue; // stale index for a reaped/respawning worker
                            }
                            if try_send_fd_to(&slots, slot, fd) {
                                dispatched = true;
                                break;
                            }
                            MASTER_SIGCHLD.store(true, Ordering::SeqCst);
                        }
                        if !dispatched {
                            // No live idle worker: queue the connection (SYN
                            // backpressure kicks in once the queue is full).
                            backlog.push_back(fd);
                        }
                        if backlog.len() >= dispatch.backlog {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
    }
    teardown(listener, &slots, &mut backlog);
    if give_up {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the raw-libc fd-passing primitives over a real socketpair:
    //! round-trip, EOF, close-on-exec, and SIGPIPE-free send to a dead peer.
    //!
    //! Called from:
    //! - `cargo test -p elephc-web` through Rust's test harness.
    //!
    //! Key details:
    //! - Each test creates its own `socketpair_cloexec` pair and a throwaway pipe
    //!   as the payload fd, so the tests are self-contained and leak no fds.
    //! - The pair is blocking (default), so `send_fd`/`recv_fd` run synchronously
    //!   without an async runtime.

    use super::*;

    /// Creates a pipe and returns `(read_fd, write_fd)`, panicking on failure.
    fn make_pipe() -> (RawFd, RawFd) {
        let mut fds = [0 as RawFd; 2];
        // SAFETY: pipe(2) fills a 2-element array; return checked.
        let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(rc, 0, "pipe creation must succeed");
        (fds[0], fds[1])
    }

    /// Sends a pipe read-end fd across the socketpair, receives it on the other
    /// end, then proves the received fd names the SAME pipe: a byte written to the
    /// original write end is readable through the received fd.
    #[test]
    fn send_recv_fd_roundtrip() {
        let (a, b) = socketpair_cloexec().expect("socketpair");
        let (pipe_r, pipe_w) = make_pipe();
        send_fd(a, pipe_r).expect("send_fd");
        let got = recv_fd(b).expect("recv_fd").expect("some fd");
        // Write through the original write end; read through the received fd.
        let out: [u8; 1] = [0x5A];
        // SAFETY: pipe_w is a valid write end; got is the received read end.
        let wn = unsafe { libc::write(pipe_w, out.as_ptr() as *const c_void, 1) };
        assert_eq!(wn, 1, "write to pipe");
        let mut inb = [0u8; 1];
        let rn = unsafe { libc::read(got, inb.as_mut_ptr() as *mut c_void, 1) };
        assert_eq!(rn, 1, "read through received fd");
        assert_eq!(inb[0], 0x5A, "received fd must name the same pipe");
        // SAFETY: close every fd this test owns.
        unsafe {
            libc::close(a);
            libc::close(b);
            libc::close(pipe_r);
            libc::close(pipe_w);
            libc::close(got);
        }
    }

    /// Closing the sending end makes `recv_fd` on the other end return `Ok(None)`
    /// (EOF), which the worker treats as "master gone → exit cleanly".
    #[test]
    fn recv_fd_eof_returns_none() {
        let (a, b) = socketpair_cloexec().expect("socketpair");
        // SAFETY: close the sending end so the receiver observes EOF.
        unsafe { libc::close(a); }
        let got = recv_fd(b).expect("recv_fd must not error on EOF");
        assert!(got.is_none(), "closed peer must yield Ok(None)");
        // SAFETY: close the receiver.
        unsafe { libc::close(b); }
    }

    /// The received fd carries `FD_CLOEXEC` (Linux via `MSG_CMSG_CLOEXEC`, macOS
    /// via the explicit `fcntl` in `recv_fd`), so a received connection is not
    /// leaked across any later `exec`.
    #[test]
    fn recv_fd_sets_cloexec() {
        let (a, b) = socketpair_cloexec().expect("socketpair");
        let (pipe_r, pipe_w) = make_pipe();
        send_fd(a, pipe_r).expect("send_fd");
        let got = recv_fd(b).expect("recv_fd").expect("some fd");
        // SAFETY: query the descriptor flags of the received fd.
        let flags = unsafe { libc::fcntl(got, libc::F_GETFD) };
        assert!(flags >= 0, "F_GETFD must succeed");
        assert!(
            flags & libc::FD_CLOEXEC != 0,
            "received fd must have FD_CLOEXEC set"
        );
        // SAFETY: close every fd this test owns.
        unsafe {
            libc::close(a);
            libc::close(b);
            libc::close(pipe_r);
            libc::close(pipe_w);
            libc::close(got);
        }
    }

    /// Sending to a socketpair whose peer end is closed returns `Err` (EPIPE)
    /// WITHOUT raising SIGPIPE — reaching the assertion proves the test process
    /// survived (`MSG_NOSIGNAL` on Linux, `SO_NOSIGPIPE` on macOS).
    #[test]
    fn send_fd_to_closed_peer_is_error_not_signal() {
        let (a, b) = socketpair_cloexec().expect("socketpair");
        let (pipe_r, pipe_w) = make_pipe();
        // SAFETY: close the receiving end so sends fail with EPIPE.
        unsafe { libc::close(b); }
        // A stream write to a closed peer fails with EPIPE; try a few times so a
        // first buffered write cannot mask the error.
        let mut last = Ok(());
        for _ in 0..4 {
            last = send_fd(a, pipe_r);
            if last.is_err() {
                break;
            }
        }
        assert!(
            last.is_err(),
            "send to a closed peer must return Err, not raise SIGPIPE"
        );
        // SAFETY: close the remaining fds this test owns.
        unsafe {
            libc::close(a);
            libc::close(pipe_r);
            libc::close(pipe_w);
        }
    }
}
