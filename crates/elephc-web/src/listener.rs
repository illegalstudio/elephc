//! Purpose:
//! Owns the `--listen` address model and the shared TCP/Unix listening sockets
//! used by the prefork web workers.
//!
//! Called from:
//! - `crate::server` for the master-side TCP/Unix binds and the per-worker handoff.
//! - `crate::worker` and `crate::isolated_worker` for their accept loops.
//!
//! Key details:
//! - Linux keeps one `SO_REUSEPORT` TCP listener per worker: the kernel hashes
//!   the connection 4-tuple across the group, which scales and avoids a shared
//!   accept queue.
//! - Darwin does NOT distribute TCP connections by the 4-tuple, and a shared
//!   accept queue lets one worker pin the keep-alive connections it wins. On
//!   Apple targets the master binds the only TCP listener, accepts every
//!   connection itself, and passes each descriptor to a worker over a Unix
//!   datagram channel with `SCM_RIGHTS`. Round-robin starts at the next worker
//!   and skips channels that are not writable, so a busy worker cannot stall
//!   the acceptor. The master holds each descriptor until the worker acks. A
//!   worker blocked in PHP still receives its round-robin share; only the
//!   acceptor thread is guaranteed not to stall.
//! - UDS is bound once in the master on every target and inherited by workers
//!   because `SO_REUSEPORT` does not balance `accept()` across AF_UNIX listeners.
//! - The master marks shared descriptors `FD_CLOEXEC`, so exec'd handler and user
//!   subprocesses cannot retain them; `fork` still copies them into workers.
//!   Worker clones are marked close-on-exec again after duplication.
//! - The socket file is created group-readable/writable only (0660); the edge
//!   must run under the same group as the origin.
//! - A UDS peer has no IP address: `REMOTE_ADDR`/`SERVER_ADDR` are empty and
//!   `REMOTE_PORT`/`SERVER_PORT` are zero.

use std::io;
#[cfg(target_vendor = "apple")]
use std::collections::HashMap;
use std::net::SocketAddr;
#[cfg(target_vendor = "apple")]
use std::net::TcpStream;
use std::os::fd::{AsRawFd, RawFd};
#[cfg(target_vendor = "apple")]
use std::os::fd::{FromRawFd, IntoRawFd};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
#[cfg(target_vendor = "apple")]
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::pin::Pin;
#[cfg(target_vendor = "apple")]
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
#[cfg(target_vendor = "apple")]
use std::sync::Mutex;
use std::task::{Context, Poll};
#[cfg(target_vendor = "apple")]
use std::time::Duration;

use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, UnixListener};

/// Pending-connection backlog for each listening socket.
const LISTEN_BACKLOG: i32 = 1024;

/// Master-owned TCP listener and per-worker descriptor channels.
///
/// On Apple targets a single acceptor drains the shared listener and passes
/// accepted descriptors to workers. That avoids both the Darwin
/// `SO_REUSEPORT` single-client-IP hash and a thundering-herd accept queue that
/// lets one worker pin every keep-alive connection.
///
/// Each descriptor carries a `u64` id and is held in `pending` until the worker
/// acknowledges that it installed its own copy. Closing the master copy early
/// can turn queued request bytes into an RST on Darwin, so the ack is required.
/// Acks are asynchronous: the acceptor drains them between accepts and never
/// blocks on a worker that is busy or recycling.
///
/// The listener and channel ends are stored as raw descriptors so a forked
/// worker can close the master-only copies without racing a Rust destructor.
pub(crate) struct SharedTcp {
    #[cfg(target_vendor = "apple")]
    listener: RawFd,
    #[cfg(target_vendor = "apple")]
    master_ends: Vec<RawFd>,
    #[cfg(target_vendor = "apple")]
    worker_ends: Vec<RawFd>,
    #[cfg(target_vendor = "apple")]
    next_worker: AtomicUsize,
    #[cfg(target_vendor = "apple")]
    next_id: AtomicU64,
    #[cfg(target_vendor = "apple")]
    pending: Mutex<HashMap<u64, (usize, std::net::TcpStream)>>,
    #[cfg(target_vendor = "apple")]
    stop: AtomicBool,
    /// False in a forked worker after it closed the master-only descriptors.
    #[cfg(target_vendor = "apple")]
    owns_descriptors: AtomicBool,
}

/// Binds the master-owned TCP listener and creates one descriptor channel per
/// worker. Returns `None` on targets that keep per-worker `SO_REUSEPORT`
/// sockets.
#[cfg(target_vendor = "apple")]
pub(crate) fn install_shared(
    addr: SocketAddr,
    workers: usize,
) -> io::Result<Option<Arc<SharedTcp>>> {
    let domain = if addr.is_ipv6() {
        Domain::IPV6
    } else {
        Domain::IPV4
    };
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(LISTEN_BACKLOG)?;
    set_close_on_exec(socket.as_raw_fd())?;
    let listener: std::net::TcpListener = socket.into();
    let listener = listener.into_raw_fd();

    let workers = workers.max(1);
    let mut master_ends = Vec::with_capacity(workers);
    let mut worker_ends = Vec::with_capacity(workers);
    for _ in 0..workers {
        let (master, worker) = UnixDatagram::pair()?;
        // A full channel is reported to the acceptor as `WouldBlock`, which
        // makes it try the next worker. The worker side stays blocking so its
        // receiver thread can sleep in `recvmsg`.
        master.set_nonblocking(true)?;
        set_no_sigpipe(master.as_raw_fd())?;
        set_no_sigpipe(worker.as_raw_fd())?;
        set_close_on_exec(master.as_raw_fd())?;
        set_close_on_exec(worker.as_raw_fd())?;
        master_ends.push(master.into_raw_fd());
        worker_ends.push(worker.into_raw_fd());
    }

    Ok(Some(Arc::new(SharedTcp {
        listener,
        master_ends,
        worker_ends,
        next_worker: AtomicUsize::new(0),
        next_id: AtomicU64::new(0),
        pending: Mutex::new(HashMap::with_capacity(workers)),
        stop: AtomicBool::new(false),
        owns_descriptors: AtomicBool::new(true),
    })))
}

/// Leaves the per-worker `SO_REUSEPORT` strategy in place on non-Apple targets.
#[cfg(not(target_vendor = "apple"))]
pub(crate) fn install_shared(
    _addr: SocketAddr,
    _workers: usize,
) -> io::Result<Option<Arc<SharedTcp>>> {
    Ok(None)
}

impl SharedTcp {
    /// Returns a worker-owned descriptor channel for `index`.
    #[cfg(target_vendor = "apple")]
    pub(crate) fn worker_receiver(&self, index: usize) -> io::Result<UnixDatagram> {
        let fd = *self.worker_ends.get(index).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "worker index out of range")
        })?;
        let duplicate = unsafe { libc::dup(fd) };
        if duplicate < 0 {
            return Err(io::Error::last_os_error());
        }
        if let Err(error) = set_close_on_exec(duplicate) {
            unsafe { libc::close(duplicate) };
            return Err(error);
        }
        Ok(unsafe { UnixDatagram::from_raw_fd(duplicate) })
    }

    /// Starts the master acceptor thread.
    #[cfg(target_vendor = "apple")]
    pub(crate) fn start_accepting(self: &Arc<Self>) -> std::thread::JoinHandle<()> {
        let shared = Arc::clone(self);
        std::thread::Builder::new()
            .name("elephc-tcp-accept".to_string())
            .spawn(move || accept_loop(shared))
            .expect("failed to spawn the TCP accept thread")
    }

    /// Requests acceptor shutdown. The thread exits after its poll timeout.
    pub(crate) fn stop(&self) {
        #[cfg(target_vendor = "apple")]
        self.stop.store(true, Ordering::SeqCst);
    }

    /// Closes the descriptors a forked worker inherited from the master.
    ///
    /// The worker keeps only the duplicate returned by `worker_receiver`. The
    /// listener, every channel end, and every pending accepted socket belong to
    /// the master; an orphaned worker holding them keeps the port in LISTEN and
    /// keeps clients alive forever.
    #[cfg(target_vendor = "apple")]
    pub(crate) fn close_in_worker(&self) {
        if !self.owns_descriptors.swap(false, Ordering::SeqCst) {
            return;
        }
        self.close_raw_descriptors();
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    /// Closes the master-owned raw descriptors exactly once.
    #[cfg(target_vendor = "apple")]
    fn close_raw_descriptors(&self) {
        unsafe { libc::close(self.listener) };
        for &fd in &self.master_ends {
            unsafe { libc::close(fd) };
        }
        for &fd in &self.worker_ends {
            unsafe { libc::close(fd) };
        }
    }

    /// Drops every descriptor still waiting for `index` and clears its stale
    /// acks. Called after the supervisor reaps a worker and before the
    /// replacement is forked, so a lost handoff cannot pin a client forever.
    #[cfg(target_vendor = "apple")]
    pub(crate) fn release_worker(&self, index: usize) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pending.retain(|_, (owner, _)| *owner != index);
        if let Some(&socket) = self.master_ends.get(index) {
            let mut id = [0u8; 8];
            loop {
                let read = unsafe {
                    libc::recv(socket, id.as_mut_ptr().cast(), id.len(), 0)
                };
                if read == 8 {
                    continue;
                }
                if read < 0
                    && io::Error::last_os_error().raw_os_error() == Some(libc::EINTR)
                {
                    continue;
                }
                break;
            }
        }
    }
}

#[cfg(target_vendor = "apple")]
impl Drop for SharedTcp {
    /// Closes the master's raw descriptors when the last reference is dropped.
    fn drop(&mut self) {
        if self.owns_descriptors.swap(false, Ordering::SeqCst) {
            self.close_raw_descriptors();
        }
    }
}

/// Drains completed handoffs, accepts pending connections, and passes each
/// descriptor to a worker.
#[cfg(target_vendor = "apple")]
fn accept_loop(shared: Arc<SharedTcp>) {
    while !shared.stop.load(Ordering::SeqCst) {
        shared.drain_acks();
        // Only accept when a connection is pending AND at least one worker can
        // receive a descriptor. Connections that cannot be handed off stay in
        // the kernel accept queue, so the backlog applies backpressure instead
        // of the acceptor blocking on a channel owned by a busy worker.
        if !poll_fd(shared.listener, libc::POLLIN, 100) {
            continue;
        }
        if !shared.wait_for_writable_worker(100) {
            continue;
        }
        let accepted = unsafe {
            libc::accept(shared.listener, std::ptr::null_mut(), std::ptr::null_mut())
        };
        if accepted < 0 {
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted => {}
                _ => {
                    // EMFILE/ENFILE/ENOBUFS and transient peer errors must not
                    // kill the only acceptor. Back off briefly and retry.
                    eprintln!("elephc-web: accept failed: {error}");
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            continue;
        }
        let stream = unsafe { TcpStream::from_raw_fd(accepted) };
        if !shared.pass_descriptor(stream) {
            // Every channel filled between the writability poll and the
            // handoff. Dropping this connection is bounded: the client
            // observes a reset and retries, while the acceptor stays
            // responsive for workers that are draining.
            continue;
        }
    }
}

impl SharedTcp {
    /// Waits until at least one worker descriptor channel accepts a write.
    #[cfg(target_vendor = "apple")]
    fn wait_for_writable_worker(&self, timeout_ms: libc::c_int) -> bool {
        let mut fds: Vec<libc::pollfd> = self
            .master_ends
            .iter()
            .map(|&socket| libc::pollfd {
                fd: socket,
                events: libc::POLLOUT,
                revents: 0,
            })
            .collect();
        let result = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, timeout_ms) };
        if result <= 0 {
            return false;
        }
        fds.iter()
            .any(|entry| entry.revents & libc::POLLOUT != 0)
    }

    /// Retires every acked descriptor whose worker installed its own copy.
    #[cfg(target_vendor = "apple")]
    fn drain_acks(&self) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for &socket in &self.master_ends {
            let mut id = [0u8; 8];
            loop {
                let read = unsafe {
                    libc::recv(socket, id.as_mut_ptr().cast(), id.len(), 0)
                };
                if read == 8 {
                    pending.remove(&u64::from_ne_bytes(id));
                    continue;
                }
                if read < 0
                    && io::Error::last_os_error().raw_os_error() == Some(libc::EINTR)
                {
                    continue;
                }
                break;
            }
        }
    }

    /// Passes one accepted socket to a writable worker, starting at the next
    /// round-robin slot, and keeps the master's copy until the worker acks.
    #[cfg(target_vendor = "apple")]
    fn pass_descriptor(&self, stream: std::net::TcpStream) -> bool {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let fd = stream.as_raw_fd();
        let count = self.master_ends.len();
        let start = self.next_worker.fetch_add(1, Ordering::Relaxed);
        for offset in 0..count {
            let index = (start + offset) % count;
            if send_fd(self.master_ends[index], fd, id).is_ok() {
                self.pending
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(id, (index, stream));
                return true;
            }
        }
        false
    }
}

/// Polls one descriptor for `events` and reports whether it is ready.
#[cfg(target_vendor = "apple")]
fn poll_fd(fd: RawFd, events: libc::c_short, timeout_ms: libc::c_int) -> bool {
    let mut entry = libc::pollfd {
        fd,
        events,
        revents: 0,
    };
    let result = unsafe { libc::poll(&mut entry, 1, timeout_ms) };
    result > 0 && entry.revents & events != 0
}

/// Aligned storage for an `SCM_RIGHTS` control message.
#[cfg(target_vendor = "apple")]
#[repr(C, align(8))]
struct ControlBuffer([u8; 64]);

/// Transfers one descriptor with its id over a Unix datagram channel.
#[cfg(target_vendor = "apple")]
fn send_fd(socket: RawFd, fd: RawFd, id: u64) -> io::Result<()> {
    let mut payload = id.to_ne_bytes();
    let mut iov = libc::iovec {
        iov_base: payload.as_mut_ptr().cast(),
        iov_len: payload.len(),
    };
    let mut control = ControlBuffer([0u8; 64]);
    let space = unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) } as usize;
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.0.as_mut_ptr().cast();
    message.msg_controllen = space as _;
    let header = unsafe { libc::CMSG_FIRSTHDR(&message) };
    if header.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "failed to build SCM_RIGHTS header",
        ));
    }
    unsafe {
        (*header).cmsg_level = libc::SOL_SOCKET;
        (*header).cmsg_type = libc::SCM_RIGHTS;
        (*header).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as _;
        std::ptr::copy_nonoverlapping(
            &fd,
            libc::CMSG_DATA(header).cast::<RawFd>(),
            1,
        );
    }
    let result = unsafe { libc::sendmsg(socket, &message, 0) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else if result as usize == payload.len() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "short descriptor datagram",
        ))
    }
}

/// Receives one descriptor and its id from a Unix datagram channel.
#[cfg(target_vendor = "apple")]
fn recv_fd(socket: &UnixDatagram) -> io::Result<(RawFd, u64)> {
    let mut payload = [0u8; 8];
    let mut iov = libc::iovec {
        iov_base: payload.as_mut_ptr().cast(),
        iov_len: payload.len(),
    };
    let mut control = ControlBuffer([0u8; 64]);
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.0.as_mut_ptr().cast();
    message.msg_controllen = control.0.len() as _;
    let result = loop {
        let result = unsafe { libc::recvmsg(socket.as_raw_fd(), &mut message, 0) };
        if result < 0
            && io::Error::last_os_error().raw_os_error() == Some(libc::EINTR)
        {
            continue;
        }
        break result;
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    if result == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "descriptor channel closed",
        ));
    }
    if result as usize != payload.len() || message.msg_flags & libc::MSG_CTRUNC != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "truncated descriptor datagram",
        ));
    }
    let header = unsafe { libc::CMSG_FIRSTHDR(&message) };
    if header.is_null()
        || unsafe { (*header).cmsg_level } != libc::SOL_SOCKET
        || unsafe { (*header).cmsg_type } != libc::SCM_RIGHTS
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing SCM_RIGHTS descriptor",
        ));
    }
    let fd = unsafe { std::ptr::read_unaligned(libc::CMSG_DATA(header).cast::<RawFd>()) };
    if let Err(error) = set_close_on_exec(fd) {
        unsafe { libc::close(fd) };
        return Err(error);
    }
    Ok((fd, u64::from_ne_bytes(payload)))
}

/// Acknowledges one installed descriptor to the master.
#[cfg(target_vendor = "apple")]
fn send_ack(socket: &UnixDatagram, id: u64) -> io::Result<()> {
    let payload = id.to_ne_bytes();
    loop {
        match socket.send(&payload) {
            Ok(8) => return Ok(()),
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "short acknowledgement datagram",
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

/// Ignores `SIGPIPE` for a descriptor channel; a dead worker is an I/O error,
/// not a reason to terminate the master.
#[cfg(target_vendor = "apple")]
fn set_no_sigpipe(fd: RawFd) -> io::Result<()> {
    let value: libc::c_int = 1;
    let result = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_NOSIGPIPE,
            &value as *const libc::c_int as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Drains descriptors from the master channel, acknowledges each installed
/// socket, and hands it to the worker's accept loop.
#[cfg(target_vendor = "apple")]
fn receive_tcp_connections(
    socket: UnixDatagram,
    sender: tokio::sync::mpsc::UnboundedSender<std::net::TcpStream>,
) {
    loop {
        let (fd, id) = match recv_fd(&socket) {
            Ok(descriptor) => descriptor,
            Err(_) => break,
        };
        let stream = unsafe { TcpStream::from_raw_fd(fd) };
        if stream.set_nonblocking(true).is_err() {
            break;
        }
        if send_ack(&socket, id).is_err() || sender.send(stream).is_err() {
            break;
        }
    }
}

/// Conservative `sun_path` limit shared by Linux (108 bytes) and macOS
/// (104 bytes), leaving room for the terminating NUL.
pub(crate) const MAX_UNIX_PATH_BYTES: usize = 100;

/// Parsed `--listen` target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ListenAddr {
    /// A TCP address bound independently by each worker with `SO_REUSEPORT`.
    Tcp(SocketAddr),
    /// A Unix-domain socket bound once by the master and inherited by workers.
    Unix(PathBuf),
}

impl ListenAddr {
    /// Parses `HOST:PORT` or `unix:/PATH`.
    ///
    /// Returns a user-facing message on usage errors so the caller can exit 2
    /// without panicking.
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        if let Some(path) = value.strip_prefix("unix:") {
            if path.is_empty() {
                return Err("--listen unix: requires a non-empty socket path".to_string());
            }
            let path_len = path.len();
            if path_len > MAX_UNIX_PATH_BYTES {
                return Err(format!(
                    "--listen unix: path is {path_len} bytes; the maximum is {MAX_UNIX_PATH_BYTES}"
                ));
            }
            return Ok(Self::Unix(PathBuf::from(path)));
        }
        value.parse::<SocketAddr>().map(Self::Tcp).map_err(|_| {
            format!("invalid --listen value {value:?}; expected HOST:PORT or unix:/PATH")
        })
    }

    /// Returns the TCP address, if this is a TCP target.
    #[cfg(test)]
    pub(crate) fn tcp(&self) -> Option<SocketAddr> {
        match self {
            Self::Tcp(addr) => Some(*addr),
            Self::Unix(_) => None,
        }
    }

    /// Returns the Unix socket path, if this is a Unix target.
    #[cfg(test)]
    pub(crate) fn unix_path(&self) -> Option<&Path> {
        match self {
            Self::Tcp(_) => None,
            Self::Unix(path) => Some(path),
        }
    }

    /// Formats the target for startup logs and diagnostics.
    pub(crate) fn display(&self) -> String {
        match self {
            Self::Tcp(addr) => format!("http://{addr}"),
            Self::Unix(path) => format!("unix:{}", path.display()),
        }
    }
}

/// A worker's listener source: a TCP address to bind locally, master-accepted
/// TCP descriptors on Apple targets, or the master-bound Unix listener inherited
/// across `fork`.
pub(crate) enum ListenSource {
    /// Bind a fresh `SO_REUSEPORT` TCP socket in this worker.
    Tcp(SocketAddr),
    /// Receive master-accepted TCP descriptors on Apple targets.
    #[cfg(target_vendor = "apple")]
    TcpPassed(UnixDatagram, SocketAddr),
    /// Wrap the inherited master Unix listener in this worker.
    Unix(std::os::unix::net::UnixListener),
}

impl ListenSource {
    /// Builds the worker-side source from parsed args and the optional master listener.
    pub(crate) fn for_worker(
        addr: &ListenAddr,
        master_unix: Option<&std::os::unix::net::UnixListener>,
        shared_tcp: Option<&SharedTcp>,
        worker_index: usize,
    ) -> io::Result<Self> {
        match addr {
            ListenAddr::Tcp(addr) => {
                #[cfg(target_vendor = "apple")]
                if let Some(shared) = shared_tcp {
                    return Ok(Self::TcpPassed(
                        shared.worker_receiver(worker_index)?,
                        *addr,
                    ));
                }
                let _ = (shared_tcp, worker_index);
                Ok(Self::Tcp(*addr))
            }
            ListenAddr::Unix(_) => {
                let listener = master_unix.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "the master did not create a Unix listener",
                    )
                })?;
                let clone = listener.try_clone()?;
                // `try_clone` must not leave an exec-inheritable listener in a
                // worker that may spawn PHP subprocesses.
                set_close_on_exec(clone.as_raw_fd())?;
                Ok(Self::Unix(clone))
            }
        }
    }
}

/// A bound listener ready for the worker accept loop.
pub(crate) enum Listener {
    /// A per-worker TCP listener; carries the bound server address.
    Tcp {
        /// The Tokio TCP listener.
        listener: TcpListener,
        /// The address every worker bound with `SO_REUSEPORT`.
        server: SocketAddr,
    },
    /// The inherited Unix listener.
    Unix {
        /// The Tokio Unix listener.
        listener: UnixListener,
    },
    /// Master-accepted TCP descriptors received over a Unix channel.
    #[cfg(target_vendor = "apple")]
    TcpPassed {
        /// Descriptor receiver filled by the channel thread.
        receiver: tokio::sync::mpsc::UnboundedReceiver<std::net::TcpStream>,
        /// The configured server address.
        server: SocketAddr,
    },
}

impl Listener {
    /// Registers the worker listener with Tokio.
    ///
    /// TCP binds a fresh `SO_REUSEPORT` socket or starts the descriptor receiver
    /// on Apple targets; Unix wraps the inherited master listener. Must run
    /// inside the worker's Tokio runtime.
    pub(crate) fn bind(source: ListenSource) -> io::Result<Self> {
        match source {
            ListenSource::Tcp(addr) => {
                let listener = TcpListener::from_std(reuseport(addr)?)?;
                Ok(Self::Tcp {
                    listener,
                    server: addr,
                })
            }
            #[cfg(target_vendor = "apple")]
            ListenSource::TcpPassed(receiver, server) => {
                let (sender, receiver_channel) =
                    tokio::sync::mpsc::unbounded_channel::<std::net::TcpStream>();
                std::thread::Builder::new()
                    .name("elephc-tcp-receive".to_string())
                    .spawn(move || receive_tcp_connections(receiver, sender))
                    .expect("failed to spawn the TCP descriptor receiver");
                Ok(Self::TcpPassed {
                    receiver: receiver_channel,
                    server,
                })
            }
            ListenSource::Unix(listener) => {
                listener.set_nonblocking(true)?;
                let listener = UnixListener::from_std(listener)?;
                Ok(Self::Unix { listener })
            }
        }
    }

    /// Accepts one connection and returns its stream plus peer/server addresses.
    pub(crate) async fn accept(&mut self) -> io::Result<(Stream, PeerInfo)> {
        match self {
            Self::Tcp { listener, server } => {
                let (stream, peer) = listener.accept().await?;
                Ok((
                    Stream::Tcp(stream),
                    PeerInfo {
                        remote: Some(peer),
                        server: Some(*server),
                    },
                ))
            }
            Self::Unix { listener } => {
                let (stream, _) = listener.accept().await?;
                Ok((Stream::Unix(stream), PeerInfo::unix()))
            }
            #[cfg(target_vendor = "apple")]
            Self::TcpPassed { receiver, server } => {
                let stream = receiver.recv().await.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        "TCP descriptor channel closed",
                    )
                })?;
                let peer = stream.peer_addr()?;
                let local = stream.local_addr().unwrap_or(*server);
                let stream = tokio::net::TcpStream::from_std(stream)?;
                Ok((
                    Stream::Tcp(stream),
                    PeerInfo {
                        remote: Some(peer),
                        server: Some(local),
                    },
                ))
            }
        }
    }
}

/// One accepted connection, either TCP or Unix.
pub(crate) enum Stream {
    /// A Tokio TCP stream.
    Tcp(tokio::net::TcpStream),
    /// A Tokio Unix stream.
    Unix(tokio::net::UnixStream),
}

impl Stream {
    /// Sets `TCP_NODELAY` for TCP connections; Unix streams are a no-op.
    pub(crate) fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.set_nodelay(nodelay),
            Self::Unix(_) => Ok(()),
        }
    }
}

impl AsyncRead for Stream {
    /// Delegates the read to the wrapped TCP or Unix stream.
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_read(context, buffer),
            Self::Unix(stream) => Pin::new(stream).poll_read(context, buffer),
        }
    }
}

impl AsyncWrite for Stream {
    /// Delegates the write to the wrapped TCP or Unix stream.
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_write(context, buffer),
            Self::Unix(stream) => Pin::new(stream).poll_write(context, buffer),
        }
    }

    /// Delegates the flush to the wrapped TCP or Unix stream.
    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_flush(context),
            Self::Unix(stream) => Pin::new(stream).poll_flush(context),
        }
    }

    /// Delegates the shutdown to the wrapped TCP or Unix stream.
    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(stream) => Pin::new(stream).poll_shutdown(context),
            Self::Unix(stream) => Pin::new(stream).poll_shutdown(context),
        }
    }
}

/// Socket addresses for one accepted connection.
///
/// A Unix-domain peer has no IP address, so both fields are `None` and the
/// PHP-facing `REMOTE_ADDR`/`SERVER_ADDR` stay empty.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PeerInfo {
    /// The remote peer address, or `None` for a Unix socket.
    remote: Option<SocketAddr>,
    /// The local server address, or `None` for a Unix socket.
    server: Option<SocketAddr>,
}

impl PeerInfo {
    /// Returns the fallback for a Unix-domain peer.
    const fn unix() -> Self {
        Self {
            remote: None,
            server: None,
        }
    }

    /// Builds peer facts for forwarded-header unit tests.
    #[cfg(test)]
    pub(crate) fn for_test(remote: Option<SocketAddr>, server: Option<SocketAddr>) -> Self {
        Self { remote, server }
    }

    /// Returns the peer IP, or `None` for a Unix-domain socket.
    pub(crate) fn remote_ip(&self) -> Option<std::net::IpAddr> {
        self.remote.map(|address| address.ip())
    }

    /// Returns `REMOTE_ADDR`; empty for a Unix socket.
    pub(crate) fn remote_addr(&self) -> String {
        self.remote
            .map(|address| address.ip().to_string())
            .unwrap_or_default()
    }

    /// Returns `REMOTE_PORT`; zero for a Unix socket.
    pub(crate) fn remote_port(&self) -> u16 {
        self.remote.map(|address| address.port()).unwrap_or(0)
    }

    /// Returns `SERVER_ADDR`; empty for a Unix socket.
    pub(crate) fn server_addr(&self) -> String {
        self.server
            .map(|address| address.ip().to_string())
            .unwrap_or_default()
    }

    /// Returns `SERVER_PORT`; zero for a Unix socket.
    pub(crate) fn server_port(&self) -> u16 {
        self.server.map(|address| address.port()).unwrap_or(0)
    }

    /// Returns the peer field for the access log; `-` for a Unix socket.
    pub(crate) fn log_peer(&self) -> String {
        self.remote
            .map(|address| address.ip().to_string())
            .unwrap_or_else(|| "-".to_string())
    }
}

/// Binds the master's Unix listener, replacing a stale socket file.
///
/// A regular file or directory at `path` is refused: only a previous socket is
/// removed, so a typo cannot delete arbitrary files. A socket that still
/// accepts connections is not stale and is refused instead of stolen. Called
/// before any worker fork or Tokio runtime, so the temporary umask change
/// cannot race another thread.
pub(crate) fn bind_unix_master(path: &Path) -> io::Result<std::os::unix::net::UnixListener> {
    remove_stale_socket(path)?;
    // Restrict the create mode before bind so the socket is never exposed with
    // the process umask between bind and chmod. The master is single-threaded
    // at this point, so changing and restoring the process umask is safe.
    let previous_umask = unsafe { libc::umask(0o117) };
    let bound = std::os::unix::net::UnixListener::bind(path);
    unsafe { libc::umask(previous_umask) };
    let listener = bound?;
    if let Err(error) = listener.set_nonblocking(true) {
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    if let Err(error) = set_close_on_exec(listener.as_raw_fd()) {
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    // The socket is the origin's trust boundary for `--trusted-proxy unix`.
    // Keep it owner/group read-write, never world-accessible.
    if let Err(error) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660)) {
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    Ok(listener)
}

/// Removes the socket file on a clean master shutdown.
pub(crate) fn remove_unix_socket(path: &Path) {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if metadata.file_type().is_socket() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Removes a previous socket file, refusing a non-socket or live-socket path.
fn remove_stale_socket(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => match unix_socket_is_live(path) {
            Ok(true) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("socket is already in use: {}", path.display()),
            )),
            Ok(false) => remove_socket_file(path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        },
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "path exists and is not a Unix socket: {}",
                path.display()
            ),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Probes a Unix socket without blocking on a full listener backlog.
///
/// A successful or in-progress non-blocking `connect` means a listener exists;
/// `ECONNREFUSED` and `ENOENT` mean the socket file is stale.
fn unix_socket_is_live(path: &Path) -> io::Result<bool> {
    let socket = Socket::new(Domain::UNIX, Type::STREAM, None)?;
    socket.set_nonblocking(true)?;
    set_close_on_exec(socket.as_raw_fd())?;
    let address = SockAddr::unix(path)?;
    match socket.connect(&address) {
        Ok(()) => Ok(true),
        Err(error)
            if matches!(
                error.raw_os_error(),
                Some(libc::EINPROGRESS) | Some(libc::EAGAIN)
            ) =>
        {
            Ok(true)
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

/// Removes a stale socket file, treating an already-unlinked path as success.
fn remove_socket_file(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Builds a listening `std::net::TcpListener` with `SO_REUSEPORT` set.
pub(crate) fn reuseport(addr: SocketAddr) -> io::Result<std::net::TcpListener> {
    let domain = if addr.is_ipv6() {
        Domain::IPV6
    } else {
        Domain::IPV4
    };
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(LISTEN_BACKLOG)?;
    Ok(socket.into())
}

/// Marks the listening descriptor close-on-exec so exec'd children cannot retain it.
fn set_close_on_exec(fd: RawFd) -> io::Result<()> {
    let flags = loop {
        let result = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        if result >= 0 {
            break result;
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINTR) {
            return Err(error);
        }
    };
    loop {
        if unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) } >= 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINTR) {
            return Err(error);
        }
    }
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

    /// Apple targets create one descriptor channel per configured worker.
    #[cfg(target_vendor = "apple")]
    #[test]
    fn shared_listener_creates_worker_channels() {
        let shared = install_shared("127.0.0.1:0".parse().unwrap(), 2)
            .unwrap()
            .expect("Apple targets install a shared listener");
        assert!(shared.worker_receiver(0).is_ok());
        assert!(shared.worker_receiver(1).is_ok());
        assert!(shared.worker_receiver(2).is_err());
    }

    /// A forked worker releases the master descriptors exactly once and keeps
    /// its receiver duplicate open.
    #[cfg(target_vendor = "apple")]
    #[test]
    fn close_in_worker_is_idempotent_and_keeps_the_receiver() {
        let shared = install_shared("127.0.0.1:0".parse().unwrap(), 1)
            .unwrap()
            .expect("Apple targets install a shared listener");
        let receiver = shared.worker_receiver(0).unwrap();
        shared.close_in_worker();
        assert!(!shared.owns_descriptors.load(Ordering::SeqCst));
        // A second call must not close a reused descriptor.
        shared.close_in_worker();
        let receiver_flags = unsafe { libc::fcntl(receiver.as_raw_fd(), libc::F_GETFD) };
        assert!(
            receiver_flags >= 0,
            "the worker receiver duplicate must stay open"
        );
    }

    /// SCM_RIGHTS transfers a live descriptor and its id between the two ends.
    #[cfg(target_vendor = "apple")]
    #[test]
    fn descriptor_channel_round_trips() {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixStream;

        let (master, worker) = UnixDatagram::pair().unwrap();
        let (mut writer, reader) = UnixStream::pair().unwrap();
        send_fd(master.as_raw_fd(), reader.as_raw_fd(), 42).unwrap();
        let (fd, id) = recv_fd(&worker).unwrap();
        assert_eq!(id, 42);
        let mut received = unsafe { UnixStream::from_raw_fd(fd) };
        writer.write_all(b"x").unwrap();
        let mut byte = [0u8; 1];
        received.read_exact(&mut byte).unwrap();
        assert_eq!(&byte, b"x");
    }

    /// A worker acknowledgement carries the descriptor id back to the master.
    #[cfg(target_vendor = "apple")]
    #[test]
    fn descriptor_ack_round_trips() {
        let (master, worker) = UnixDatagram::pair().unwrap();
        send_ack(&worker, 99).unwrap();
        let mut ack = [0u8; 8];
        assert_eq!(master.recv(&mut ack).unwrap(), 8);
        assert_eq!(u64::from_ne_bytes(ack), 99);
    }

    /// Consecutive connections are handed to workers in round-robin order.
    #[cfg(target_vendor = "apple")]
    #[test]
    fn descriptors_are_passed_round_robin() {
        let shared = install_shared("127.0.0.1:0".parse().unwrap(), 3)
            .unwrap()
            .expect("Apple targets install a shared listener");
        let receivers = (0..3)
            .map(|index| shared.worker_receiver(index).unwrap())
            .collect::<Vec<_>>();
        let handles = receivers
            .into_iter()
            .map(|receiver| {
                std::thread::spawn(move || {
                    let mut received = 0usize;
                    while received < 2 {
                        let (fd, id) = recv_fd(&receiver).expect("descriptor must arrive");
                        send_ack(&receiver, id).expect("ack must reach the master");
                        unsafe { libc::close(fd) };
                        received += 1;
                    }
                    received
                })
            })
            .collect::<Vec<_>>();

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut clients = Vec::new();
        for _ in 0..6 {
            clients.push(TcpStream::connect(addr).unwrap());
            let (server, _) = listener.accept().unwrap();
            assert!(shared.pass_descriptor(server));
        }
        let counts = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        shared.drain_acks();
        assert!(
            shared.pending.lock().unwrap().is_empty(),
            "every acked descriptor must be released"
        );
        assert_eq!(counts, vec![2, 2, 2]);
    }

    /// The worker's acknowledgement lets the master close its copy without
    /// discarding request bytes already queued on the transferred connection.
    #[cfg(target_vendor = "apple")]
    #[test]
    fn descriptor_ack_preserves_queued_request_bytes() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let mut client = TcpStream::connect(addr).unwrap();
        let (server, _) = listener.accept().unwrap();
        client.write_all(b"hello").unwrap();

        let (master, worker) = UnixDatagram::pair().unwrap();
        send_fd(master.as_raw_fd(), server.as_raw_fd(), 7).unwrap();
        let (fd, id) = recv_fd(&worker).unwrap();
        assert_eq!(id, 7);
        let mut received = unsafe { TcpStream::from_raw_fd(fd) };
        send_ack(&worker, id).unwrap();
        let mut ack = [0u8; 8];
        assert_eq!(master.recv(&mut ack).unwrap(), 8);
        assert_eq!(u64::from_ne_bytes(ack), 7);
        // Closing the master's copy after the ack must not reset the worker's
        // connection or drop the bytes the client already sent.
        drop(server);

        let mut bytes = [0u8; 5];
        received.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"hello");
    }

    /// Reaping a worker releases descriptors that never reached a live worker.
    #[cfg(target_vendor = "apple")]
    #[test]
    fn release_worker_drops_pending_descriptors() {
        let shared = install_shared("127.0.0.1:0".parse().unwrap(), 1)
            .unwrap()
            .expect("Apple targets install a shared listener");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let _client = TcpStream::connect(addr).unwrap();
        let (server, _) = listener.accept().unwrap();
        assert!(shared.pass_descriptor(server));
        assert_eq!(shared.pending.lock().unwrap().len(), 1);
        shared.release_worker(0);
        assert!(shared.pending.lock().unwrap().is_empty());
    }

    /// Non-Apple targets keep the per-worker `SO_REUSEPORT` strategy.
    #[cfg(not(target_vendor = "apple"))]
    #[test]
    fn non_apple_targets_have_no_shared_listener() {
        assert!(install_shared("127.0.0.1:0".parse().unwrap(), 2)
            .unwrap()
            .is_none());
    }

    /// Returns a unique short Unix socket path for one unit test.
    fn test_socket_path(stem: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "elephc-{stem}-{}-{}.sock",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    /// Parses the TCP form used by the existing standalone deployment path.
    #[test]
    fn listen_parser_accepts_tcp() {
        let parsed = ListenAddr::parse("127.0.0.1:8080").expect("TCP address must parse");
        assert_eq!(
            parsed,
            ListenAddr::Tcp("127.0.0.1:8080".parse().unwrap())
        );
        assert_eq!(parsed.tcp(), Some("127.0.0.1:8080".parse().unwrap()));
        assert!(parsed.unix_path().is_none());
        assert_eq!(parsed.display(), "http://127.0.0.1:8080");
    }

    /// Parses the Unix form and preserves the path verbatim.
    #[test]
    fn listen_parser_accepts_unix() {
        let parsed = ListenAddr::parse("unix:/tmp/elephc.sock").expect("Unix path must parse");
        assert_eq!(parsed, ListenAddr::Unix(PathBuf::from("/tmp/elephc.sock")));
        assert!(parsed.tcp().is_none());
        assert_eq!(parsed.unix_path(), Some(Path::new("/tmp/elephc.sock")));
        assert_eq!(parsed.display(), "unix:/tmp/elephc.sock");
    }

    /// Rejects an empty Unix path instead of binding the current directory.
    #[test]
    fn listen_parser_rejects_empty_unix_path() {
        let error = ListenAddr::parse("unix:").expect_err("empty path must fail");
        assert!(error.contains("non-empty"), "{error}");
    }

    /// Rejects a Unix path that cannot fit in `sun_path` on the supported hosts.
    #[test]
    fn listen_parser_rejects_long_unix_path() {
        let path = format!("unix:/{}", "x".repeat(MAX_UNIX_PATH_BYTES + 1));
        let error = ListenAddr::parse(&path).expect_err("oversized path must fail");
        assert!(error.contains("maximum"), "{error}");
    }

    /// Rejects a malformed value with both accepted forms named.
    #[test]
    fn listen_parser_rejects_malformed_value() {
        let error = ListenAddr::parse("not-a-listener").expect_err("garbage must fail");
        assert!(error.contains("HOST:PORT"), "{error}");
        assert!(error.contains("unix:/PATH"), "{error}");
    }

    /// The worker clone keeps `FD_CLOEXEC` after `try_clone`.
    #[test]
    fn worker_clone_stays_close_on_exec() {
        let path = test_socket_path("cloexec");
        let _ = std::fs::remove_file(&path);
        let master = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let source = ListenSource::for_worker(
            &ListenAddr::Unix(path.clone()),
            Some(&master),
            None,
            0,
        )
        .unwrap();
        let listener = match source {
            ListenSource::Unix(listener) => listener,
            ListenSource::Tcp(_) => panic!("a Unix listen target must produce a Unix source"),
            #[cfg(target_vendor = "apple")]
            ListenSource::TcpPassed(_, _) => {
                panic!("a Unix listen target must produce a Unix source")
            }
        };
        let flags = unsafe { libc::fcntl(listener.as_raw_fd(), libc::F_GETFD) };
        assert!(flags >= 0, "F_GETFD failed for the worker listener clone");
        assert_ne!(flags & libc::FD_CLOEXEC, 0);
        drop(listener);
        drop(master);
        let _ = std::fs::remove_file(&path);
    }

    /// A live socket is refused, then its stale file is replaced after close.
    #[test]
    fn unix_probe_refuses_live_and_replaces_stale() {
        let path = test_socket_path("probe");
        let _ = std::fs::remove_file(&path);
        let live = bind_unix_master(&path).unwrap();
        let error = match bind_unix_master(&path) {
            Ok(_) => panic!("a live socket must not be replaced"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert!(path.exists(), "a refused live socket must stay in place");

        drop(live);
        let stale = bind_unix_master(&path).unwrap();
        drop(stale);
        let _ = std::fs::remove_file(&path);
    }
}
