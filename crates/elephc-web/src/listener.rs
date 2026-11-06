//! Purpose:
//! Owns the listening-socket strategy shared by every `--web` worker.
//!
//! Called from:
//! - `crate::server::run_server`, which installs the shared socket before forking.
//! - `crate::worker::serve_inner` and `crate::isolated_worker::serve_inner`, which
//!   take a worker-owned duplicate of it.
//!
//! Key details:
//! - Linux keeps one `SO_REUSEPORT` listener per worker: the kernel hashes the
//!   connection 4-tuple across the group, which scales and avoids a shared accept
//!   queue.
//! - Darwin does NOT distribute TCP connections by the 4-tuple. Every connection
//!   from one client IP lands on the same `SO_REUSEPORT` socket, so a prefork
//!   server with N workers serves from one core. On Apple targets the master
//!   binds once and workers accept on inherited duplicates of that descriptor, so
//!   the kernel hands each new connection to whichever worker calls `accept`
//!   first.
//! - The shared descriptor is process-wide and fixed at startup; the master keeps
//!   it open for the server's lifetime, including across worker respawns.

use std::io;
use std::net::SocketAddr;

#[cfg(target_vendor = "apple")]
use std::os::fd::{FromRawFd, IntoRawFd, RawFd};
#[cfg(target_vendor = "apple")]
use std::sync::atomic::{AtomicI32, Ordering};
use socket2::{Domain, Protocol, Socket, Type};

/// Pending-connection backlog for the listening socket.
pub(crate) const LISTEN_BACKLOG: i32 = 1024;

/// Raw descriptor of the master-owned listener on Apple targets (`-1` elsewhere).
#[cfg(target_vendor = "apple")]
static SHARED_LISTENER_FD: AtomicI32 = AtomicI32::new(-1);

/// Binds the process-wide listener every worker accepts on.
///
/// Linux and other targets keep per-worker `SO_REUSEPORT` sockets, so this is a
/// no-op there. On Darwin an invalid address or bind failure is fatal before any
/// worker is forked, instead of turning into a worker crash loop.
#[cfg(target_vendor = "apple")]
pub(crate) fn install_shared(addr: SocketAddr) -> io::Result<()> {
    let domain = if addr.is_ipv6() { Domain::IPV6 } else { Domain::IPV4 };
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(LISTEN_BACKLOG)?;
    let listener: std::net::TcpListener = socket.into();
    let fd = listener.into_raw_fd();
    SHARED_LISTENER_FD.store(fd, Ordering::SeqCst);
    Ok(())
}

/// Leaves the per-worker `SO_REUSEPORT` strategy in place on non-Apple targets.
#[cfg(not(target_vendor = "apple"))]
pub(crate) fn install_shared(_addr: SocketAddr) -> io::Result<()> {
    Ok(())
}

/// Returns a worker-owned duplicate of the shared listener on Apple targets.
///
/// `None` means the platform uses per-worker `SO_REUSEPORT` sockets and the
/// caller must bind its own listener. The duplicate shares the master's accept
/// queue but has an independent descriptor, so a worker that exits closes only
/// its own handle.
#[cfg(target_vendor = "apple")]
pub(crate) fn worker_listener() -> Option<io::Result<std::net::TcpListener>> {
    let fd: RawFd = SHARED_LISTENER_FD.load(Ordering::SeqCst);
    if fd < 0 {
        return None;
    }
    let duplicate = unsafe { libc::dup(fd) };
    if duplicate < 0 {
        return Some(Err(io::Error::last_os_error()));
    }
    // `dup` clears FD_CLOEXEC on POSIX; restore it so an exec'd helper started
    // by the PHP handler cannot inherit the listener.
    unsafe { libc::fcntl(duplicate, libc::F_SETFD, libc::FD_CLOEXEC) };
    // SAFETY: `duplicate` is a fresh descriptor returned by `dup`; ownership is
    // transferred to the listener exactly once.
    Some(Ok(unsafe { std::net::TcpListener::from_raw_fd(duplicate) }))
}

/// Returns `None` on targets that bind one listener per worker.
#[cfg(not(target_vendor = "apple"))]
pub(crate) fn worker_listener() -> Option<io::Result<std::net::TcpListener>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A per-worker socket can bind an ephemeral port and reports it back.
    #[test]
    fn reuseport_listener_binds_an_ephemeral_port() {
        let listener = reuseport("127.0.0.1:0".parse().unwrap()).unwrap();
        assert_ne!(listener.local_addr().unwrap().port(), 0);
    }

    /// Apple workers inherit a duplicate of the master's bound socket.
    #[cfg(target_vendor = "apple")]
    #[test]
    fn shared_listener_is_inherited_by_workers() {
        install_shared("127.0.0.1:0".parse().unwrap()).unwrap();
        let worker = worker_listener().expect("Apple targets share one listener").unwrap();
        let port = worker.local_addr().unwrap().port();
        assert_ne!(port, 0);
    }

    /// Non-Apple targets keep the per-worker `SO_REUSEPORT` strategy.
    #[cfg(not(target_vendor = "apple"))]
    #[test]
    fn non_apple_targets_have_no_shared_listener() {
        install_shared("127.0.0.1:0".parse().unwrap()).unwrap();
        assert!(worker_listener().is_none());
    }
}

/// Binds a per-worker `SO_REUSEPORT` listening socket.
///
/// Used directly on Linux, and on Apple targets as the fallback when no shared
/// listener was installed (for example a unit test calling `worker::serve`
/// directly).
pub(crate) fn reuseport(addr: SocketAddr) -> io::Result<std::net::TcpListener> {
    let domain = if addr.is_ipv6() { Domain::IPV6 } else { Domain::IPV4 };
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(LISTEN_BACKLOG)?;
    Ok(socket.into())
}
