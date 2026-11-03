//! Purpose:
//! Owns the `--listen` address model and the shared TCP/Unix listening sockets
//! used by the prefork web workers.
//!
//! Called from:
//! - `crate::server` for the master-side Unix bind and the per-worker handoff.
//! - `crate::worker` and `crate::isolated_worker` for their accept loops.
//!
//! Key details:
//! - TCP keeps the per-worker `SO_REUSEPORT` bind; UDS is bound once in the
//!   master and inherited by every worker because `SO_REUSEPORT` does not
//!   balance `accept()` across AF_UNIX listeners.
//! - The master marks the UDS descriptor `FD_CLOEXEC`, so exec'd handler and
//!   user subprocesses cannot retain it; `fork` still copies it into workers.
//! - A UDS peer has no IP address: `REMOTE_ADDR`/`SERVER_ADDR` are empty and
//!   `REMOTE_PORT`/`SERVER_PORT` are zero.

use std::io;
use std::net::SocketAddr;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use socket2::{Domain, Protocol, Socket, Type};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, UnixListener};

/// Pending-connection backlog for each listening socket.
const LISTEN_BACKLOG: i32 = 1024;

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

/// A worker's listener source: a TCP address to bind locally, or the
/// master-bound Unix listener inherited across `fork`.
pub(crate) enum ListenSource {
    /// Bind a fresh `SO_REUSEPORT` TCP socket in this worker.
    Tcp(SocketAddr),
    /// Wrap the inherited master Unix listener in this worker.
    Unix(std::os::unix::net::UnixListener),
}

impl ListenSource {
    /// Builds the worker-side source from parsed args and the optional master listener.
    pub(crate) fn for_worker(
        addr: &ListenAddr,
        master_unix: Option<&std::os::unix::net::UnixListener>,
    ) -> io::Result<Self> {
        match addr {
            ListenAddr::Tcp(addr) => Ok(Self::Tcp(*addr)),
            ListenAddr::Unix(_) => {
                let listener = master_unix.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "the master did not create a Unix listener",
                    )
                })?;
                Ok(Self::Unix(listener.try_clone()?))
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
}

impl Listener {
    /// Registers the worker listener with Tokio.
    ///
    /// TCP binds a fresh `SO_REUSEPORT` socket; Unix wraps the inherited
    /// master listener. Must run inside the worker's Tokio runtime.
    pub(crate) fn bind(source: ListenSource) -> io::Result<Self> {
        match source {
            ListenSource::Tcp(addr) => {
                let listener = TcpListener::from_std(reuseport_listener(addr)?)?;
                Ok(Self::Tcp {
                    listener,
                    server: addr,
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
    pub(crate) async fn accept(&self) -> io::Result<(Stream, PeerInfo)> {
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
/// removed, so a typo cannot delete arbitrary files.
pub(crate) fn bind_unix_master(path: &Path) -> io::Result<std::os::unix::net::UnixListener> {
    remove_stale_socket(path)?;
    let listener = std::os::unix::net::UnixListener::bind(path)?;
    if let Err(error) = listener.set_nonblocking(true) {
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    if let Err(error) = set_close_on_exec(listener.as_raw_fd()) {
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

/// Removes a previous socket file, refusing a non-socket path.
fn remove_stale_socket(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => std::fs::remove_file(path),
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

/// Builds a listening `std::net::TcpListener` with `SO_REUSEPORT` set.
fn reuseport_listener(addr: SocketAddr) -> io::Result<std::net::TcpListener> {
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
}
