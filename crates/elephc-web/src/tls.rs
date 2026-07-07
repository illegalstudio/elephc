//! Purpose:
//! Server-side TLS termination for the `--web` prefork HTTP server. Loads the
//! operator-supplied PEM cert chain + key into a rustls `ServerConfig`, exposes a
//! process-wide `TlsAcceptor` (built once by the master before fork, inherited by
//! every worker), and wraps an already-accepted `TcpStream` in a TLS (or
//! plaintext) transport for hyper.
//!
//! Called from:
//! - `crate::server::{elephc_web_run, elephc_web_run_worker, elephc_web_run_script}`
//!   call `load_acceptor` + `set_tls_acceptor` in the master, before `spawn_worker`.
//! - `crate::worker::serve` and `crate::worker_mode::enter_worker_loop` call
//!   `tls_acceptor()` + `wrap_accepted(...)` inside each connection's task.
//!
//! Key details:
//! - The acceptor lives in an `OnceLock<TlsAcceptor>`, NOT a `static mut`: the
//!   master writes it once before forking and workers only ever read it, so a
//!   write-once cell is the UB-free fit (unlike `WORKER_LISTEN`, which is written
//!   in the child after fork). `TlsAcceptor` is `Arc<ServerConfig>`, so the fork'd
//!   config pages stay physically shared (read-only after load).
//! - `wrap_accepted` takes an ALREADY-ACCEPTED `TcpStream`; the `accept()` call
//!   stays in the serve loop OUTSIDE this function. This keeps the handshake
//!   decoupled from `accept()` so a future fd-dispatch path (SCM_RIGHTS) can run
//!   the handshake on a stream reconstructed from a received fd.
//! - `MaybeTls` is `Unpin` (both variants wrap `Unpin` transports), so
//!   `TokioIo::new(maybe_tls)` satisfies hyper's `Unpin` connection bound.
//! - ALPN advertises `h2` ahead of `http/1.1` when `--http2` is on (the `http2`
//!   flag is passed through from `install_tls_if_configured`), so a TLS client
//!   that supports h2 negotiates HTTP/2-over-TLS; when `--http2` is off the list
//!   is `http/1.1` only (the prior behavior). Plaintext h2c prior-knowledge is
//!   unaffected — ALPN only applies to the TLS path.
//! - Ticketer caveat: the ring `Ticketer` rotates its keys per process over time.
//!   Built pre-fork, workers start with shared keys (inter-worker TLS resumption
//!   works), but after the first per-process rotation the workers diverge and
//!   resumption across workers degrades to a full handshake. Acceptable in v1.

use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

/// Maximum time allowed for a TLS handshake before the connection is dropped.
/// hyper's `header_read_timeout(30s)` only arms AFTER the handshake, so without a
/// dedicated bound a client that opens the TCP connection and stays silent would
/// pin a connection task forever (a pre-HTTP slowloris). Symmetric to the existing
/// header-read protection.
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Process-wide TLS acceptor, set once by the master (`set_tls_acceptor`) before
/// the `spawn_worker` fork loop and read by every worker (`tls_acceptor`). A
/// write-once `OnceLock` — not a `static mut` — because the value is produced by
/// the master before fork and only ever read afterwards; the forked workers
/// inherit the initialized cell. `None` (never set) means plaintext HTTP.
static TLS_ACCEPTOR: OnceLock<TlsAcceptor> = OnceLock::new();

/// A per-connection transport that is either a plaintext TCP stream (`--tls-*`
/// off) or a completed TLS server stream. Delegates `AsyncRead`/`AsyncWrite` to
/// the active variant so both serve loops pass a single concrete type to
/// `TokioIo`/hyper. Both variants wrap `Unpin` transports, so `MaybeTls` is
/// `Unpin`.
pub(crate) enum MaybeTls {
    /// Plaintext HTTP over a raw TCP stream (TLS not configured).
    Plain(TcpStream),
    /// HTTP over a completed rustls server-side TLS stream.
    Tls(TlsStream<TcpStream>),
}

impl AsyncRead for MaybeTls {
    /// Delegates the read poll to the active transport. `get_mut()` is valid
    /// because both variants are `Unpin`, so re-pinning the inner stream is sound.
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_read(cx, buf),
            MaybeTls::Tls(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MaybeTls {
    /// Delegates the write poll to the active transport.
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_write(cx, buf),
            MaybeTls::Tls(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    /// Delegates the flush poll to the active transport.
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_flush(cx),
            MaybeTls::Tls(s) => Pin::new(s).poll_flush(cx),
        }
    }

    /// Delegates the shutdown poll to the active transport (a TLS shutdown sends
    /// `close_notify` before closing the underlying socket).
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_shutdown(cx),
            MaybeTls::Tls(s) => Pin::new(s).poll_shutdown(cx),
        }
    }

    /// Delegates vectored writes so hyper can coalesce header/body buffers on the
    /// plaintext path; the TLS path forwards to rustls' own vectored handling.
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_write_vectored(cx, bufs),
            MaybeTls::Tls(s) => Pin::new(s).poll_write_vectored(cx, bufs),
        }
    }

    /// Reports whether the active transport benefits from vectored writes.
    fn is_write_vectored(&self) -> bool {
        match self {
            MaybeTls::Plain(s) => s.is_write_vectored(),
            MaybeTls::Tls(s) => s.is_write_vectored(),
        }
    }
}

/// Loads a TLS acceptor from PEM `cert_path` (certificate chain) and `key_path`
/// (private key, PKCS#8/PKCS#1/SEC1). Installs the ring crypto provider (idempotent
/// — `elephc-tls` may have installed it too), builds a `ServerConfig` with no
/// client auth, advertises `h2` ahead of `http/1.1` via ALPN when `http2` is true
/// (so a TLS client that supports h2 negotiates HTTP/2-over-TLS) or `http/1.1`
/// only when `http2` is false (the prior behavior), and attaches a stateless ring
/// `Ticketer` for TLS session resumption. Returns the acceptor or a human-readable
/// error string (unreadable file, malformed PEM, encrypted/absent key, or a
/// cert/key mismatch) so the master can fail-fast before forking.
pub(crate) fn load_acceptor(
    cert_path: &str,
    key_path: &str,
    http2: bool,
) -> Result<TlsAcceptor, String> {
    Ok(TlsAcceptor::from(load_acceptor_config(cert_path, key_path, http2)?))
}

/// Builds the rustls `ServerConfig` (wrapped in an `Arc`) for `cert_path` /
/// `key_path`, applying the ALPN list selected by `http2` and the stateless ring
/// `Ticketer`. Factored out of `load_acceptor` so the unit tests can read
/// `config.alpn_protocols` back without going through the `TlsAcceptor` wrapper
/// (`TlsAcceptor` is `Arc<ServerConfig>` but does not expose the inner config
/// by reference). Returns the config or a human-readable error string.
fn load_acceptor_config(
    cert_path: &str,
    key_path: &str,
    http2: bool,
) -> Result<Arc<rustls::ServerConfig>, String> {
    // Install the ring provider explicitly so provider selection never depends on
    // default features. Ignoring the result is correct: an "already installed"
    // error just means elephc-tls (or a prior call) installed the same provider.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cert_pem = std::fs::read(cert_path)
        .map_err(|e| format!("cannot read certificate file {}: {}", cert_path, e))?;
    let mut cert_reader: &[u8] = &cert_pem;
    let certs: Vec<rustls::pki_types::CertificateDer<'static>> =
        rustls_pemfile::certs(&mut cert_reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("invalid PEM in certificate file {}: {}", cert_path, e))?;
    if certs.is_empty() {
        return Err(format!("no certificates found in {}", cert_path));
    }

    let key_pem = std::fs::read(key_path)
        .map_err(|e| format!("cannot read key file {}: {}", key_path, e))?;
    let mut key_reader: &[u8] = &key_pem;
    let key = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|e| format!("invalid PEM in key file {}: {}", key_path, e))?
        .ok_or_else(|| {
            format!(
                "no usable private key in {} (an encrypted PEM key is not supported)",
                key_path
            )
        })?;

    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("certificate/key rejected: {}", e))?;
    // ALPN: advertise h2 ahead of http/1.1 when the operator opted into HTTP/2, so
    // a TLS client that supports h2 negotiates HTTP/2-over-TLS; otherwise advertise
    // http/1.1 only (the prior behavior). The order matters: h2 first lets a
    // capable client pick h2, while a client that only knows http/1.1 falls back.
    config.alpn_protocols = if http2 {
        vec![b"h2".to_vec(), b"http/1.1".to_vec()]
    } else {
        vec![b"http/1.1".to_vec()]
    };
    // Stateless ring Ticketer for TLS session resumption. Built in the master
    // before fork so the ticket keys live in the inherited (CoW) memory and every
    // worker starts with the same keys; a ticket minted by worker A is then
    // resumable by worker B despite SO_REUSEPORT spreading connections. Caveat:
    // the ticketer rotates keys per process over time, so after the first
    // post-fork rotation inter-worker resumption degrades to a full handshake
    // (acceptable v1 — see the module preamble). A build failure here is
    // non-fatal: resumption is a performance optimization, not correctness.
    if let Ok(ticketer) = rustls::crypto::ring::Ticketer::new() {
        config.ticketer = ticketer;
    }
    Ok(Arc::new(config))
}

/// Stores the TLS acceptor in the process-wide `OnceLock`. Called by the master
/// once, before the `spawn_worker` fork loop, so every worker inherits the
/// initialized cell. A second call (should not happen) is ignored via the
/// `OnceLock::set` `Err` path.
pub(crate) fn set_tls_acceptor(acceptor: TlsAcceptor) {
    let _ = TLS_ACCEPTOR.set(acceptor);
}

/// Returns the process-wide TLS acceptor, or `None` when TLS is not configured
/// (plaintext HTTP). The reference is `'static` (the `OnceLock` outlives the
/// process's serving loop), so a worker can carry it into a connection task.
pub(crate) fn tls_acceptor() -> Option<&'static TlsAcceptor> {
    TLS_ACCEPTOR.get()
}

/// Wraps an ALREADY-ACCEPTED `TcpStream` for hyper. With `acceptor` `None`
/// (plaintext), returns `Some(MaybeTls::Plain(stream))` immediately. With
/// `Some(acceptor)`, runs the TLS handshake bounded by `TLS_HANDSHAKE_TIMEOUT` and
/// returns `Some(MaybeTls::Tls(..))` on success or `None` on handshake failure or
/// timeout (the caller drops the connection). The `accept()` call stays OUTSIDE
/// this function so the handshake is not entangled with connection acceptance
/// (the fd-dispatch composition contract).
pub(crate) async fn wrap_accepted(
    stream: TcpStream,
    acceptor: Option<&TlsAcceptor>,
) -> Option<MaybeTls> {
    match acceptor {
        None => Some(MaybeTls::Plain(stream)),
        Some(acceptor) => {
            match tokio::time::timeout(TLS_HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await {
                Ok(Ok(tls)) => Some(MaybeTls::Tls(tls)),
                // Handshake error (plaintext client, scanner, unusable cert) or the
                // 10s timeout elapsed: drop the connection, the worker continues.
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for TLS acceptor loading and the `OnceLock` accessor.
    //!
    //! Called from:
    //! - `cargo test -p elephc-web` through Rust's test harness.
    //!
    //! Key details:
    //! - A self-signed cert+key is generated at RUNTIME with `rcgen` and written to
    //!   temp PEM files, so no certificate/key material is ever committed.
    //! - `TLS_ACCEPTOR` is a process-global write-once cell, so exactly ONE test
    //!   (`once_lock_accessor_round_trip`) sets it, to avoid cross-test ordering
    //!   assumptions.

    use super::*;

    /// Writes a runtime-generated self-signed cert+key to unique temp PEM files and
    /// returns their paths. Nothing is committed: the files live under the system
    /// temp dir. Panics on any generation/IO failure (a broken test environment).
    fn write_temp_cert_key() -> (std::path::PathBuf, std::path::PathBuf) {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .expect("rcgen must generate a self-signed cert");
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir();
        let cert_path = dir.join(format!("elephc_web_tls_test_cert_{}.pem", id));
        let key_path = dir.join(format!("elephc_web_tls_test_key_{}.pem", id));
        std::fs::write(&cert_path, cert.cert.pem()).expect("write cert pem");
        std::fs::write(&key_path, cert.key_pair.serialize_pem()).expect("write key pem");
        (cert_path, key_path)
    }

    /// A valid cert/key pair loads into a `TlsAcceptor` (the happy path the master
    /// takes before fork). Cleans up the temp PEM files afterwards. `http2=false`
    /// because this test is about PEM loading, not ALPN content.
    #[test]
    fn load_acceptor_accepts_valid_pem() {
        let (cert_path, key_path) = write_temp_cert_key();
        let result = load_acceptor(
            cert_path.to_str().unwrap(),
            key_path.to_str().unwrap(),
            false,
        );
        let _ = std::fs::remove_file(&cert_path);
        let _ = std::fs::remove_file(&key_path);
        assert!(result.is_ok(), "valid PEM must load: {:?}", result.err());
    }

    /// Garbage (non-PEM) cert content is rejected with an error, not a panic, so
    /// the master can fail-fast with a diagnostic. `http2=false` (ALPN-agnostic).
    #[test]
    fn load_acceptor_rejects_garbage_pem() {
        let (_valid_cert, key_path) = write_temp_cert_key();
        let dir = std::env::temp_dir();
        let garbage = dir.join(format!(
            "elephc_web_tls_garbage_{}.pem",
            std::process::id()
        ));
        std::fs::write(&garbage, b"this is not a PEM certificate at all\n").unwrap();
        let result = load_acceptor(garbage.to_str().unwrap(), key_path.to_str().unwrap(), false);
        let _ = std::fs::remove_file(&garbage);
        let _ = std::fs::remove_file(&_valid_cert);
        let _ = std::fs::remove_file(&key_path);
        assert!(result.is_err(), "garbage cert must be rejected");
    }

    /// A missing certificate file is reported as an error (unreadable file), not a
    /// panic. `http2=false` (ALPN-agnostic — the read fails before ALPN runs).
    #[test]
    fn load_acceptor_rejects_missing_file() {
        let result = load_acceptor(
            "/nonexistent/elephc-web/does-not-exist-cert.pem",
            "/nonexistent/elephc-web/does-not-exist-key.pem",
            false,
        );
        // `TlsAcceptor` is not `Debug`, so match rather than `unwrap_err`.
        match result {
            Ok(_) => panic!("missing files must be rejected"),
            Err(cause) => assert!(
                cause.contains("cannot read certificate file"),
                "error should name the unreadable certificate file: {}",
                cause
            ),
        }
    }

    /// `set_tls_acceptor` then `tls_acceptor` round-trips through the process-wide
    /// `OnceLock`. This is the ONLY test that writes `TLS_ACCEPTOR`, so no other
    /// test observes an ordering-dependent value. `http2=false` (ALPN-agnostic).
    #[test]
    fn once_lock_accessor_round_trip() {
        let (cert_path, key_path) = write_temp_cert_key();
        let acceptor = load_acceptor(
            cert_path.to_str().unwrap(),
            key_path.to_str().unwrap(),
            false,
        )
        .expect("valid PEM must load");
        let _ = std::fs::remove_file(&cert_path);
        let _ = std::fs::remove_file(&key_path);
        set_tls_acceptor(acceptor);
        assert!(
            tls_acceptor().is_some(),
            "acceptor must be readable after being set"
        );
    }

    /// The built `ServerConfig`'s ALPN list reflects the `http2` flag: `false`
    /// advertises `["http/1.1"]` only (the prior behavior, byte-for-byte), and
    /// `true` advertises `["h2", "http/1.1"]` with h2 first so a capable client
    /// picks h2 while an h1-only client falls back. Reads the list back through
    /// `load_acceptor_config` (which exposes the `Arc<ServerConfig>` directly,
    /// unlike `load_acceptor` which wraps it in a `TlsAcceptor`).
    #[test]
    fn load_acceptor_alpn_reflects_http2_flag() {
        let (cert_path, key_path) = write_temp_cert_key();
        let cert = cert_path.to_str().unwrap();
        let key = key_path.to_str().unwrap();

        let config_off = load_acceptor_config(cert, key, false)
            .expect("http2=false must build a config");
        assert_eq!(
            config_off.alpn_protocols,
            vec![b"http/1.1".to_vec()],
            "http2=false must advertise http/1.1 only"
        );

        let config_on = load_acceptor_config(cert, key, true)
            .expect("http2=true must build a config");
        assert_eq!(
            config_on.alpn_protocols,
            vec![b"h2".to_vec(), b"http/1.1".to_vec()],
            "http2=true must advertise h2 ahead of http/1.1"
        );

        let _ = std::fs::remove_file(&cert_path);
        let _ = std::fs::remove_file(&key_path);
    }

    /// Compile-time assertion that `MaybeTls` is `Unpin`, so `TokioIo::new` and
    /// hyper's `serve_connection` (which require `Unpin`) accept it.
    #[test]
    fn maybe_tls_is_unpin() {
        fn assert_unpin<T: Unpin>() {}
        assert_unpin::<MaybeTls>();
    }
}
