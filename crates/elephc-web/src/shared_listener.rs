//! Purpose:
//! Creates ONE listening socket in the master before forking, so every prefork worker accepts
//! from the same queue.
//!
//! Called from:
//! - `crate::server` before the fork loop, and `crate::worker` / `crate::isolated_worker` when a
//!   worker adopts the inherited descriptor.
//!
//! Key details:
//! - `SO_REUSEPORT` means two different things. On Linux the kernel hash-balances new connections
//!   across every socket bound to the port, which is what the per-worker listener relies on. On
//!   Darwin it only permits the duplicate bind: connections are NOT distributed, and one socket
//!   takes essentially all of them.
//! - MEASURED on macOS with 8 workers and `wrk -t4 -c100`: seven workers accumulated 0:00.00 of
//!   CPU while the eighth took 0:05.07 — one worker served the entire load. Throughput went from
//!   6609 req/s at one worker to 7340 at eighteen, a 1.1x return on 18x the CPU, because 17 of
//!   them were idle.
//! - A single listener inherited across `fork()` is the classic prefork model and distributes on
//!   both platforms: the kernel hands each accepted connection to one of the blocked accepters.
//!   Linux keeps its `SO_REUSEPORT` path, which is at least as good there and already proven.

use socket2::{Domain, Protocol, Socket, Type};
use std::net::SocketAddr;

/// Pending-connection backlog for the shared listening socket.
const LISTEN_BACKLOG: i32 = 1024;

/// Whether workers should adopt a master-created listener rather than each binding their own.
///
/// True exactly where `SO_REUSEPORT` does not load-balance.
pub(crate) const fn workers_share_one_listener() -> bool {
    cfg!(target_vendor = "apple")
}

/// Builds the listening socket the master hands down to every worker.
///
/// Left blocking and WITHOUT `FD_CLOEXEC`: the workers arrive by `fork()` with no `exec()`, so
/// the descriptor has to survive the fork, and each worker re-marks it non-blocking when it
/// adopts it into its own reactor.
pub(crate) fn bind_shared(addr: SocketAddr) -> std::io::Result<std::net::TcpListener> {
    let domain = if addr.is_ipv6() { Domain::IPV6 } else { Domain::IPV4 };
    let sock = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    sock.set_reuse_address(true)?;
    sock.bind(&addr.into())?;
    sock.listen(LISTEN_BACKLOG)?;
    Ok(sock.into())
}

/// Adopts the inherited listening descriptor inside a forked worker.
///
/// # Safety
/// `fd` must be a listening socket descriptor this process owns, which is the case for a
/// descriptor inherited across `fork()` from the master that created it and keeps it open.
pub(crate) unsafe fn adopt(fd: std::os::fd::RawFd) -> std::io::Result<std::net::TcpListener> {
    use std::os::fd::FromRawFd;

    // Each worker drives the descriptor from its own tokio reactor, which requires non-blocking
    // mode. The flag lives on the shared open file description, so every worker setting it is
    // idempotent rather than conflicting.
    let listener = unsafe { std::net::TcpListener::from_raw_fd(fd) };
    listener.set_nonblocking(true)?;
    Ok(listener)
}
