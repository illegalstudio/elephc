//! Purpose:
//! Reads a service already serving traffic, over its endpoint. Answers from the
//! sample ring by default; `--exact` arms the instrumentation and returns the
//! measured table for the next request that completes.
//!
//! Called from:
//! - `monitor::main`, when the target parses as an address or a URL.
//!
//! Key details:
//! - The mutual handshake proves both sides hold the build key before a byte of
//!   profile crosses, and the payload is sealed under keys derived from it.
//! - Over `https://` the certificate must validate against the system roots
//!   first; the listener itself speaks the framed protocol, not TLS.

use super::*;

/// A `monitor` target that is a running service rather than a file.
///
/// `host:port`, or the same behind an `http://` scheme — the two spellings people
/// actually type. A filesystem path stays a path, so a socket at `/tmp/p.sock` is
/// still a socket and never mistaken for a host.
pub(crate) fn remote_target(spec: &str) -> Option<RemoteTarget> {
    let (tls, rest, default_port) = if let Some(rest) = spec.strip_prefix("https://") {
        (true, rest, 443)
    } else if let Some(rest) = spec.strip_prefix("http://") {
        (false, rest, 80)
    } else {
        (false, spec, 0)
    };
    // Anything after the authority is a path, not part of the address.
    let authority = rest.split('/').next().unwrap_or(rest);
    if authority.starts_with('/') || authority.starts_with('.') || authority.is_empty() {
        return None;
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => {
            if host.is_empty() || port.parse::<u16>().is_err() {
                return None;
            }
            (host.to_string(), port.parse::<u16>().ok()?)
        }
        // A scheme implies its port; a bare name without one is a file, not a host.
        None if default_port != 0 => (authority.to_string(), default_port),
        None => return None,
    };
    Some(RemoteTarget { host, port, tls })
}

/// How long the client waits on a socket read, by mode.
///
/// `--exact` asks the server to hold the connection until the next request
/// completes, so the client must outlast the server's own `EXACT_WAIT` — derived
/// from that constant rather than written out again, because the two drifting
/// apart is the bug: a 10-second client against a 30-second server could not
/// receive the documented no-traffic answer, and lost any slice from a request
/// completing after the tenth second. The margin covers the round trip and the
/// server's own rendering after its wait expires.
///
/// The sampled answer is produced immediately, so it keeps the short timeout: a
/// dead peer should not hold the command for half a minute.
pub(crate) fn read_timeout(exact: bool) -> std::time::Duration {
    if exact {
        elephc_probe::endpoint::EXACT_WAIT + std::time::Duration::from_secs(10)
    } else {
        std::time::Duration::from_secs(10)
    }
}

/// A running service `monitor` can read, and how to reach it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RemoteTarget {
    pub host: String,
    pub port: u16,
    /// Whether the connection is wrapped in TLS, with the certificate verified
    /// against the platform root store before a single protocol byte is sent.
    pub tls: bool,
}

impl RemoteTarget {
    /// `host:port`, the form the connect call and the diagnostics both want.
    fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Opens the connection, verifying the certificate first when the target is
    /// https.
    ///
    /// Verification is the entire difference between https and a plaintext port:
    /// without it, an attacker in the path answers the handshake and receives a
    /// profile — the shape of the code and the URLs it serves. Refusing an
    /// unverifiable certificate is therefore a hard failure, never a prompt.
    fn connect(&self, exact: bool) -> Result<Box<dyn ReadWrite>, String> {
        let timeout = read_timeout(exact);
        let tcp = std::net::TcpStream::connect(self.authority())
            .map_err(|error| format!("cannot reach {}: {error}", self.authority()))?;
        let _ = tcp.set_read_timeout(Some(timeout));
        let _ = tcp.set_write_timeout(Some(timeout));
        if !self.tls {
            return Ok(Box::new(tcp));
        }

        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let server = rustls::pki_types::ServerName::try_from(self.host.clone())
            .map_err(|_| format!("{} is not a valid server name", self.host))?;
        let connection = rustls::ClientConnection::new(std::sync::Arc::new(config), server)
            .map_err(|error| format!("cannot start TLS to {}: {error}", self.host))?;
        Ok(Box::new(rustls::StreamOwned::new(connection, tcp)))
    }
}

/// Read + Write in one object, so the remote path is written once for both
/// transports rather than duplicated per socket type.
pub(crate) trait ReadWrite: std::io::Read + std::io::Write {}
impl<T: std::io::Read + std::io::Write> ReadWrite for T {}

/// Profiles a running `--probe` binary through its endpoint socket: reads the
/// build key from its `.key` file (or `ELEPHC_PROBE_KEY`), runs the
/// mutual HMAC handshake, receives the folded profile, and renders the same
/// table (plus optional Speedscope/pprof). Needs no macOS sampler.
pub(crate) fn run_probe_host(cmd: &MonitorCommand, socket: &str) -> i32 {
    let key = match resolve_probe_key(cmd, socket) {
        Ok(key) => key,
        Err(error) => {
            eprintln!("elephc monitor: {error}");
            return 1;
        }
    };
    // A path is a local socket; `host:port` is a service somewhere else. The
    // handshake is identical, so the transport is the only thing that differs —
    // which is what lets one command read a binary on this machine and a service
    // on another.
    let mut stream: Box<dyn ReadWrite> = match remote_target(socket) {
        Some(target) => match target.connect(cmd.exact) {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("elephc monitor: {error}");
                return 1;
            }
        },
        None => {
            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::net::UnixStream;

                match UnixStream::connect(socket) {
                    Ok(stream) => {
                        // The same deadline the TCP path uses. Left unset, a local socket
                        // waited forever where a remote one gave up, so the same command
                        // against the same server had a different contract depending on
                        // the transport — and a wedged peer hung the command with no way
                        // to tell that from a quiet service.
                        let timeout = read_timeout(cmd.exact);
                        let _ = stream.set_read_timeout(Some(timeout));
                        let _ = stream.set_write_timeout(Some(timeout));
                        Box::new(stream)
                    }
                    Err(error) => {
                        eprintln!("elephc monitor: cannot connect to probe at {socket}: {error}");
                        return 1;
                    }
                }
            }
            #[cfg(target_os = "windows")]
            {
                eprintln!(
                    "elephc monitor: local Unix-socket probe endpoints are unavailable on Windows; \
                     use host:port or an http(s) URL"
                );
                return 2;
            }
        }
    };
    let nonce_c = probe_nonce();
    let want = if cmd.exact {
        elephc_probe::endpoint::WANT_EXACT
    } else {
        elephc_probe::endpoint::WANT_SAMPLED
    };
    let folded = match elephc_probe::endpoint::wire::client_handshake_and_fetch(
        &mut stream,
        &key,
        &nonce_c,
        want,
    ) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("elephc monitor: {error}");
            return 1;
        }
    };
    // Announced only once the peer has PROVEN it holds the same build key.
    // Printing it on connect claimed a relationship that had not been
    // established — and still printed when the TLS certificate was then refused,
    // which reads as "connected, then something odd happened" rather than
    // "never connected to anything trustworthy".
    println!(
        "connected to probe build {}",
        elephc_probe::handshake::fingerprint(&key)
    );
    if cmd.exact {
        // An exact slice is `elephc-instr:` rows, not folded stacks: handing it
        // to the sampled parser produced an empty display and a message saying
        // nothing had arrived, while the server had in fact just sent 290 bytes
        // of profile. Same rendering as a locally captured exact run, because it
        // is the same data.
        let graph = parse_instrument_dump(&folded);
        if graph.nodes.is_empty() {
            // The server distinguishes its reasons — an exact slot already held
            // by another operator is not the same as a service with no traffic —
            // and the parser drops `note:` lines because they carry no metrics.
            // Printing the server's own words keeps that distinction instead of
            // answering every case with the most common one.
            match folded
                .lines()
                .find_map(|line| line.strip_prefix("elephc-instr: note: "))
            {
                Some(note) => eprintln!("elephc monitor: {note}"),
                None => eprintln!(
                    "elephc monitor: no exact slice arrived within the wait. A slice is \
                     rendered when a profiled request completes, so a service with no \
                     traffic has none to give yet; drop `--exact` for the sampled view, \
                     which does not need a request to finish."
                ),
            }
            return 1;
        }
        // No export handling here: `--exact` with an export flag is refused
        // during argument validation, because warning and exiting 0 told a
        // pipeline it had a file when it had none.
        print!("{}", instrument_table(&graph));
        return 0;
    }
    let display = folded_text_to_display(&folded);
    if display.is_empty() {
        // "Collection has started" is a state of its own, not a synonym for
        // "the process looks idle". Asking is what starts sampling, so the first
        // command necessarily arrives before any sample exists; the server now
        // waits briefly for one, and reaching here after that wait means either
        // a genuinely quiet process or one that has only just begun. Saying so
        // is the difference between a user retrying and a user concluding the
        // profiler does not work.
        eprintln!(
            "elephc monitor: no samples yet. Collection starts when you first ask, so \
             run the same command again — if it stays empty, the process is idle."
        );
        return 1;
    }
    if let Some(out_path) = &cmd.out {
        if let Err(error) = write_speedscope(&display, out_path) {
            eprintln!("elephc monitor: {error}");
            return 1;
        }
        println!("wrote {out_path}");
    }
    if let Some(pprof_path) = &cmd.pprof_out {
        let stacks = php_folded_stacks(&display);
        if let Err(error) = std::fs::write(pprof_path, crate::pprof_encode::encode_folded_profile(&stacks)) {
            eprintln!("elephc monitor: cannot write {pprof_path}: {error}");
            return 1;
        }
        println!("wrote {pprof_path}");
    }
    // A remote probe capture has no local dSYM/source, so no line attribution.
    if let Err(error) = write_graph_exports(cmd, &display, "elephc probe (remote)", None) {
        eprintln!("elephc monitor: {error}");
        return 1;
    }
    print!("{}", why_table(&display, 1));
    print!("{}", probe_io_summary(&folded));
    0
}

/// A per-connection client nonce from the OS RNG (time-seeded fallback).
pub(crate) fn probe_nonce() -> [u8; 32] {
    use std::io::Read as _;
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let mut nonce = [0u8; 32];
        if file.read_exact(&mut nonce).is_ok() {
            return nonce;
        }
    }
    let mut state = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
        | 1;
    let mut nonce = [0u8; 32];
    for byte in nonce.iter_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = (state >> 24) as u8;
    }
    nonce
}
