//! Purpose:
//! The probe's remote endpoint: a Unix-socket listener that authenticates a
//! `--probe-host` client with the build-key HMAC handshake, then serves the
//! folded profile. Both sides drive `wire::*`, so the client (in the compiler)
//! and this server cannot disagree on the protocol.
//!
//! Called from:
//! - `elephc_probe_init` spawns `serve` on a background thread when
//!   `ELEPHC_PROBE_ADDR` names a socket path.
//!
//! Key details:
//! - No secret crosses the socket; only nonces and HMAC tags (see `handshake`).
//! - The server reads randomness from `/dev/urandom`; a client that cannot
//!   prove the key is disconnected before any profile bytes are sent.

use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::time::Duration;

use crate::handshake::{self, KEY_LEN, NONCE_LEN, TAG_LEN};

/// Per-connection I/O timeout: a client that stalls mid-handshake must not pin
/// the endpoint thread and starve every other operator.
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// Upper bound on a served profile, enforced by the client so a buggy or hostile
/// server cannot make it allocate gigabytes from a 4-byte length.
pub const MAX_PROFILE_BYTES: usize = 64 * 1024 * 1024;

/// Wire protocol shared by the endpoint server and the `--probe-host` client.
///
/// After a successful mutual handshake the server frames the folded profile as
/// a 4-byte big-endian length followed by that many UTF-8 bytes.
pub mod wire {
    use super::*;

    /// Reads exactly `n` bytes or returns an error on short read.
    pub fn read_exact_vec(stream: &mut impl Read, n: usize) -> std::io::Result<Vec<u8>> {
        let mut buf = vec![0u8; n];
        stream.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Client side of the handshake over an established stream: proves authority
    /// with the key and, on success, returns the served folded profile text.
    pub fn client_handshake_and_fetch(
        stream: &mut (impl Read + Write),
        key: &[u8; KEY_LEN],
        nonce_c: &[u8; NONCE_LEN],
    ) -> std::io::Result<String> {
        stream.write_all(nonce_c)?;
        stream.flush()?;
        let nonce_s = read_exact_vec(stream, NONCE_LEN)?;
        let server_tag = read_exact_vec(stream, TAG_LEN)?;
        let expected = handshake::server_tag(key, nonce_c, &nonce_s);
        if !handshake::tags_equal(&server_tag, &expected) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "probe endpoint failed to prove the build key (wrong binary or key)",
            ));
        }
        let client_tag = handshake::client_tag(key, &nonce_s, nonce_c);
        stream.write_all(&client_tag)?;
        stream.flush()?;
        let mut len_bytes = [0u8; 4];
        stream.read_exact(&mut len_bytes)?;
        let len = u32::from_be_bytes(len_bytes) as usize;
        if len > MAX_PROFILE_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "probe profile exceeds the size cap (buggy or hostile server?)",
            ));
        }
        let payload = read_exact_vec(stream, len)?;
        String::from_utf8(payload)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "non-UTF-8 profile"))
    }
}

/// Spawns the endpoint listener on a background thread. Silent on bind failure —
/// a diagnostic must never take down the profiled process.
pub fn spawn(path: String) {
    std::thread::Builder::new()
        .name("elephc-probe-endpoint".to_string())
        .spawn(move || serve(&path))
        .ok();
}

/// Accept loop: bind the Unix socket (replacing a stale one), restrict it to the
/// owner, then handle each connection with the handshake and, on success, the
/// folded profile.
fn serve(path: &str) {
    // A broken client that disconnects mid-write must not kill the profiled
    // process: a write to a closed socket would otherwise raise SIGPIPE, whose
    // default action terminates. Rather than change the process-wide disposition
    // (which would alter the host program's own pipe semantics), block SIGPIPE
    // on THIS endpoint thread only. SIGPIPE is generated synchronously by the
    // faulting write, so with it blocked here the write returns EPIPE instead —
    // and the host's other threads keep their original SIGPIPE behavior.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGPIPE);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
    // `host:port` listens on TCP so a service can be read from another machine;
    // anything else is a filesystem path and stays a Unix socket, which is both
    // faster and unreachable from the network. The handshake is the same either
    // way — the transport changes who can *attempt* it, never who succeeds.
    if let Some(addr) = tcp_address(path) {
        serve_tcp(&addr);
        return;
    }
    let _ = std::fs::remove_file(path);
    let listener = match UnixListener::bind(path) {
        Ok(listener) => listener,
        Err(error) => {
            // Say so. Failing silently here means the operator set
            // ELEPHC_PROBE_ADDR, watched nothing happen, and had no way to tell
            // a refused bind from a program that ignores the variable — and the
            // commonest cause is invisible: a `sockaddr_un` path holds about 104
            // bytes, and a path longer than that fails with nothing to read.
            eprintln!(
                "elephc-probe: cannot serve on {path}: {error}{}",
                if path.len() > 100 {
                    format!(
                        " (the path is {} bytes; a Unix socket address holds about 104, \
                         so keep it in /tmp or /run)",
                        path.len()
                    )
                } else {
                    String::new()
                }
            );
            return;
        }
    };
    // Restrict the socket to the owner: the handshake authenticates, but there
    // is no reason to let other local users even reach it.
    unsafe {
        if let Ok(c_path) = std::ffi::CString::new(path) {
            libc::chmod(c_path.as_ptr(), 0o600);
        }
    }
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
                let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
                // One misbehaving client must not stop the endpoint.
                let _ = handle(stream);
            }
            Err(error) => match error.kind() {
                // Transient: a signal or a client that aborted before accept.
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::ConnectionAborted => continue,
                // fd exhaustion under load: back off instead of busy-looping a core.
                _ if error.raw_os_error() == Some(libc::EMFILE)
                    || error.raw_os_error() == Some(libc::ENFILE) =>
                {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
                // Anything else means the listener is unusable; stop cleanly.
                _ => return,
            },
        }
    }
}

/// Interprets `spec` as a TCP address, or `None` when it is a filesystem path.
///
/// A path is the common case and must not be mistaken for a host: `/tmp/p.sock`
/// contains no colon, and a Windows-style path never reaches here. Requiring a
/// port keeps `host:port` unambiguous.
fn tcp_address(spec: &str) -> Option<String> {
    if spec.starts_with('/') || spec.starts_with('.') {
        return None;
    }
    let (host, port) = spec.rsplit_once(':')?;
    if host.is_empty() || port.parse::<u16>().is_err() {
        return None;
    }
    Some(spec.to_string())
}

/// Accept loop over TCP. Same handshake, same failure handling as the Unix path.
///
/// Binding a port makes the endpoint reachable from the network, where the
/// handshake is the only thing standing between a stranger and a profile. That is
/// what it was built for — no secret crosses the wire and a client who cannot
/// prove the build key is disconnected before any bytes are served — but binding
/// a wildcard address is still a deployment decision, not a default: prefer
/// 127.0.0.1 and a tunnel, or a reverse proxy.
fn serve_tcp(addr: &str) {
    let Ok(listener) = std::net::TcpListener::bind(addr) else {
        return;
    };
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
                let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
                let _ = handle(stream);
            }
            Err(error) => match error.kind() {
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::ConnectionAborted => continue,
                _ if error.raw_os_error() == Some(libc::EMFILE)
                    || error.raw_os_error() == Some(libc::ENFILE) =>
                {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
                _ => return,
            },
        }
    }
}

/// Runs the server side of the handshake and serves the folded profile on
/// success. Returns early (dropping the connection) on any failure.
fn handle<S: std::io::Read + std::io::Write>(mut stream: S) -> std::io::Result<()> {
    let Some(key) = crate::build_key() else {
        return Ok(());
    };
    let nonce_c = wire::read_exact_vec(&mut stream, NONCE_LEN)?;
    let nonce_s = os_random::<NONCE_LEN>();
    let server_tag = handshake::server_tag(&key, &nonce_c, &nonce_s);
    stream.write_all(&nonce_s)?;
    stream.write_all(&server_tag)?;
    stream.flush()?;
    let client_tag = wire::read_exact_vec(&mut stream, TAG_LEN)?;
    let expected = handshake::client_tag(&key, &nonce_s, &nonce_c);
    if !handshake::tags_equal(&client_tag, &expected) {
        // Authority not proven: disconnect without serving anything.
        return Ok(());
    }
    let profile = crate::current_folded_profile().unwrap_or_default();
    let bytes = profile.as_bytes();
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(bytes)?;
    stream.flush()?;
    Ok(())
}

/// Fills `N` bytes from the OS entropy source, falling back to a time seed if
/// `/dev/urandom` is unavailable — the nonce only needs to be non-repeating.
fn os_random<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        if file.read_exact(&mut out).is_ok() {
            return out;
        }
    }
    let mut state = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e3779b97f4a7c15)
        | 1;
    for byte in out.iter_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = (state >> 24) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drives both sides of the wire protocol over an in-memory duplex to prove
    /// the handshake serves the profile to a key-holder and rejects others.
    #[test]
    fn handshake_serves_the_profile_to_a_key_holder() {
        let key = [42u8; KEY_LEN];
        let nonce_c = [1u8; NONCE_LEN];
        let nonce_s = [2u8; NONCE_LEN];
        let profile = "elephc-probe: {main};hot 10\nelephc-probe-samples: 10\n";

        // Server frames: nonce_s, server_tag, then len+profile after verifying client_tag.
        let server_tag = handshake::server_tag(&key, &nonce_c, &nonce_s);
        let mut server_to_client = Vec::new();
        server_to_client.extend_from_slice(&nonce_s);
        server_to_client.extend_from_slice(&server_tag);
        server_to_client.extend_from_slice(&(profile.len() as u32).to_be_bytes());
        server_to_client.extend_from_slice(profile.as_bytes());

        let mut duplex = MockStream {
            to_read: server_to_client,
            read_pos: 0,
            written: Vec::new(),
        };
        let got = wire::client_handshake_and_fetch(&mut duplex, &key, &nonce_c).unwrap();
        assert_eq!(got, profile);
        // The client wrote nonce_c then the correct client_tag.
        assert_eq!(&duplex.written[..NONCE_LEN], &nonce_c);
        let expected_client_tag = handshake::client_tag(&key, &nonce_s, &nonce_c);
        assert_eq!(&duplex.written[NONCE_LEN..NONCE_LEN + TAG_LEN], &expected_client_tag);
    }

    #[test]
    fn a_wrong_key_is_rejected_before_any_profile() {
        let real = [42u8; KEY_LEN];
        let wrong = [7u8; KEY_LEN];
        let nonce_c = [1u8; NONCE_LEN];
        let nonce_s = [2u8; NONCE_LEN];
        // Server proved identity with the REAL key.
        let server_tag = handshake::server_tag(&real, &nonce_c, &nonce_s);
        let mut server_to_client = Vec::new();
        server_to_client.extend_from_slice(&nonce_s);
        server_to_client.extend_from_slice(&server_tag);
        let mut duplex = MockStream {
            to_read: server_to_client,
            read_pos: 0,
            written: Vec::new(),
        };
        // Client holds the WRONG key: it must reject the server tag and not read a profile.
        let err = wire::client_handshake_and_fetch(&mut duplex, &wrong, &nonce_c).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }

    struct MockStream {
        to_read: Vec<u8>,
        read_pos: usize,
        written: Vec<u8>,
    }

    impl Read for MockStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let remaining = &self.to_read[self.read_pos..];
            let n = remaining.len().min(buf.len());
            buf[..n].copy_from_slice(&remaining[..n]);
            self.read_pos += n;
            Ok(n)
        }
    }

    impl Write for MockStream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.written.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}
