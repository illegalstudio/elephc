//! Purpose:
//! Spawns a local `openssl s_server` TLS 1.2 fixture whose session cache supports
//! ID-based resumption, and seeds it with an `openssl s_client` session export.
//!
//! Called from:
//! - `tests/codegen/openssl_php86.rs` for the end-to-end `ssl.session_data` fixture.
//!
//! Key details:
//! - The server runs in one process for both connections so its TLS 1.2 session
//!   cache can resume the ID captured from the first `s_client` handshake.
//! - `-state` records the handshake transcript: a resumed connection has no
//!   `write certificate` transition, which is the assertion the fixture uses.
//! - Missing `openssl` CLI tools skip visibly; `ELEPHC_TEST_REQUIRE_OPENSSL_CLI`
//!   turns that into a panic in jobs that install the client/server tools.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use super::TEST_ID;
use std::sync::atomic::Ordering;

/// Environment variable that turns a missing `openssl` CLI into a panic.
pub(crate) const OPENSSL_CLI_REQUIRE_ENV: &str = "ELEPHC_TEST_REQUIRE_OPENSSL_CLI";

/// Stable token a log gate can grep for when the CLI is unavailable.
pub(crate) const OPENSSL_CLI_SKIP_MARKER: &str = "ELEPHC_OPENSSL_CLI_SKIP_GATE";

/// Reports whether an `openssl` binary with the s_server/s_client tools is available.
fn openssl_cli() -> Option<&'static Path> {
    static BINARY: OnceLock<Option<PathBuf>> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            if let Some(explicit) = std::env::var_os("ELEPHC_TEST_OPENSSL_BIN") {
                let path = PathBuf::from(explicit);
                return cli_supports_server(&path).then_some(path);
            }
            let mut candidates = vec![PathBuf::from("openssl")];
            for prefix in ["/opt/homebrew/opt/openssl@3", "/usr/local/opt/openssl@3"] {
                candidates.push(PathBuf::from(prefix).join("bin/openssl"));
            }
            candidates.into_iter().find(|path| cli_supports_server(path))
        })
        .as_deref()
}

/// Verifies the candidate binary runs and advertises `s_server`.
fn cli_supports_server(path: &Path) -> bool {
    let Ok(output) = Command::new(path).arg("s_server").arg("-help").output() else {
        return false;
    };
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text.contains("-accept") && text.contains("-www")
}

/// Reports a skip for `test_name` and returns whether the caller should return early.
pub(crate) fn skip_without_openssl_cli(test_name: &str) -> bool {
    if openssl_cli().is_some() {
        return false;
    }
    if std::env::var_os(OPENSSL_CLI_REQUIRE_ENV).is_some_and(|value| !value.is_empty()) {
        panic!(
            "{test_name}: {OPENSSL_CLI_REQUIRE_ENV} is set but no openssl s_server/s_client \
             binary was found (set ELEPHC_TEST_OPENSSL_BIN to its path)"
        );
    }
    eprintln!(
        "{OPENSSL_CLI_SKIP_MARKER} skipping {test_name}: no openssl s_server/s_client binary \
         was found (set ELEPHC_TEST_OPENSSL_BIN to its path)"
    );
    true
}

/// A local certificate-less TLS 1.3 `openssl s_server` that only accepts one
/// external PSK identity.
pub(crate) struct OpensslPskServer {
    child: Child,
    directory: PathBuf,
    port: u16,
    log_path: PathBuf,
}

impl OpensslPskServer {
    /// Starts a TLS 1.3 PSK `s_server -www -state -nocert` listener.
    pub(crate) fn start(psk_hex: &str, identity: &str) -> Option<Self> {
        let binary = psk_cli()?;
        let mut last_log = String::new();
        for _ in 0..5 {
            let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
            let directory = std::env::temp_dir().join(format!(
                "elephc_openssl_psk_server_{}_{}",
                std::process::id(),
                id
            ));
            std::fs::create_dir_all(&directory)
                .expect("openssl PSK server: create fixture directory");
            let log_path = directory.join("server.log");
            let port = reserve_port();
            let log = std::fs::File::create(&log_path).expect("openssl PSK server: create log");
            let stderr = log.try_clone().expect("openssl PSK server: clone log");
            let child = Command::new(binary)
                .args([
                    "s_server",
                    "-accept",
                    &port.to_string(),
                    "-tls1_3",
                    "-psk",
                    psk_hex,
                    "-psk_identity",
                    identity,
                    "-nocert",
                    "-www",
                    "-state",
                ])
                .current_dir(&directory)
                .stdin(Stdio::null())
                .stdout(Stdio::from(log))
                .stderr(Stdio::from(stderr))
                .spawn()
                .expect("openssl PSK server: spawn s_server");
            let mut server = Self {
                child,
                directory,
                port,
                log_path,
            };
            if server.wait_for_listener(Duration::from_secs(5)) {
                return Some(server);
            }
            last_log = server.state_log();
            server.stop();
            drop(server);
        }
        panic!("openssl PSK server: s_server did not listen on five port attempts: {last_log}");
    }

    /// Returns the loopback port the fixture server listens on.
    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// Returns the accumulated `-state` transcript.
    pub(crate) fn state_log(&self) -> String {
        std::fs::read_to_string(&self.log_path).unwrap_or_default()
    }

    /// Waits until the transcript contains `needle` at least `count` times.
    pub(crate) fn wait_for_state(&self, needle: &str, count: usize, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.state_log().matches(needle).count() >= count {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    /// Waits for the `ACCEPT` banner that means the listener is bound.
    fn wait_for_listener(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return false;
            }
            if self.state_log().contains("ACCEPT") {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    /// Terminates the fixture server and reaps its process.
    pub(crate) fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for OpensslPskServer {
    /// Stops the fixture server and removes its scratch directory.
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// Reports whether an `openssl` binary supports the TLS 1.3 PSK server options.
fn psk_cli() -> Option<&'static Path> {
    static BINARY: OnceLock<Option<PathBuf>> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            if let Some(explicit) = std::env::var_os("ELEPHC_TEST_OPENSSL_BIN") {
                let path = PathBuf::from(explicit);
                return cli_supports_psk_server(&path).then_some(path);
            }
            let mut candidates = vec![PathBuf::from("openssl")];
            for prefix in ["/opt/homebrew/opt/openssl@3", "/usr/local/opt/openssl@3"] {
                candidates.push(PathBuf::from(prefix).join("bin/openssl"));
            }
            candidates.into_iter().find(|path| cli_supports_psk_server(path))
        })
        .as_deref()
}

/// Verifies the candidate binary advertises the TLS 1.3 PSK server flags.
fn cli_supports_psk_server(path: &Path) -> bool {
    let Ok(output) = Command::new(path).arg("s_server").arg("-help").output() else {
        return false;
    };
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text.contains("-psk") && text.contains("-nocert") && text.contains("-tls1_3")
}

/// Reports a skip for `test_name` when no TLS 1.3 PSK-capable CLI is available.
pub(crate) fn skip_without_openssl_psk_cli(test_name: &str) -> bool {
    if psk_cli().is_some() {
        return false;
    }
    if std::env::var_os(OPENSSL_CLI_REQUIRE_ENV).is_some_and(|value| !value.is_empty()) {
        panic!(
            "{test_name}: {OPENSSL_CLI_REQUIRE_ENV} is set but no openssl s_server with \
             TLS 1.3 PSK support was found (set ELEPHC_TEST_OPENSSL_BIN to its path)"
        );
    }
    eprintln!(
        "{OPENSSL_CLI_SKIP_MARKER} skipping {test_name}: no openssl s_server with TLS 1.3 \
         PSK support was found (set ELEPHC_TEST_OPENSSL_BIN to its path)"
    );
    true
}

/// A local certificate-less TLS 1.3 `openssl s_server` that only accepts one
/// external PSK identity.
pub(crate) struct OpensslSessionServer {
    child: Child,
    directory: PathBuf,
    port: u16,
    log_path: PathBuf,
    session_path: PathBuf,
}

impl OpensslSessionServer {
    /// Starts a TLS 1.2 `s_server -www -state -no_ticket` listener for the given
    /// self-signed certificate and key PEM contents.
    pub(crate) fn start(cert_pem: &str, key_pem: &str) -> Option<Self> {
        let binary = openssl_cli()?;
        let mut last_log = String::new();
        for _ in 0..5 {
            let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
            let directory = std::env::temp_dir().join(format!(
                "elephc_openssl_tls_server_{}_{}",
                std::process::id(),
                id
            ));
            std::fs::create_dir_all(&directory)
                .expect("openssl TLS server: create fixture directory");
            let cert_path = directory.join("cert.pem");
            let key_path = directory.join("key.pem");
            let log_path = directory.join("server.log");
            let session_path = directory.join("session.pem");
            std::fs::write(&cert_path, cert_pem).expect("openssl TLS server: write certificate");
            std::fs::write(&key_path, key_pem).expect("openssl TLS server: write private key");
            let port = reserve_port();
            let log = std::fs::File::create(&log_path).expect("openssl TLS server: create log");
            let stderr = log.try_clone().expect("openssl TLS server: clone log");
            let child = Command::new(binary)
                .args([
                    "s_server",
                    "-accept",
                    &port.to_string(),
                    "-cert",
                    cert_path.to_str().expect("openssl TLS server: UTF-8 cert path"),
                    "-key",
                    key_path.to_str().expect("openssl TLS server: UTF-8 key path"),
                    "-www",
                    "-tls1_2",
                    "-no_ticket",
                    "-state",
                ])
                .current_dir(&directory)
                .stdin(Stdio::null())
                .stdout(Stdio::from(log))
                .stderr(Stdio::from(stderr))
                .spawn()
                .expect("openssl TLS server: spawn s_server");
            let mut server = Self {
                child,
                directory,
                port,
                log_path,
                session_path,
            };
            if server.wait_for_listener(Duration::from_secs(5)) {
                return Some(server);
            }
            last_log = server.state_log();
            server.stop();
            drop(server);
        }
        panic!("openssl TLS server: s_server did not listen on five port attempts: {last_log}");
    }

    /// Returns the loopback port the fixture server listens on.
    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// Performs the full first handshake with `openssl s_client -sess_out` and
    /// returns the exported session PEM.
    pub(crate) fn seed_session(&self) -> String {
        let binary = openssl_cli().expect("openssl TLS session fixture requires the CLI");
        let mut child = Command::new(binary)
            .args([
                "s_client",
                "-connect",
                &format!("127.0.0.1:{}", self.port),
                "-tls1_2",
                "-sess_out",
                self.session_path
                    .to_str()
                    .expect("openssl TLS session fixture: UTF-8 session path"),
            ])
            .current_dir(&self.directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("openssl TLS session fixture: spawn s_client");
        {
            use std::io::Write;
            let stdin = child.stdin.as_mut().expect("openssl TLS session fixture: s_client stdin");
            stdin
                .write_all(b"GET / HTTP/1.0\r\n\r\n")
                .expect("openssl TLS session fixture: write request");
        }
        let output = child
            .wait_with_output()
            .expect("openssl TLS session fixture: wait for s_client");
        assert!(
            output.status.success(),
            "openssl TLS session fixture: s_client failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::read_to_string(&self.session_path)
            .expect("openssl TLS session fixture: read exported session")
    }

    /// Returns the accumulated `-state` transcript.
    pub(crate) fn state_log(&self) -> String {
        std::fs::read_to_string(&self.log_path).unwrap_or_default()
    }

    /// Waits until the transcript contains `needle` at least `count` times.
    pub(crate) fn wait_for_state(&self, needle: &str, count: usize, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.state_log().matches(needle).count() >= count {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    /// Waits for the `ACCEPT` banner that means the listener is bound.
    fn wait_for_listener(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return false;
            }
            if self.state_log().contains("ACCEPT") {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }

    /// Terminates the fixture server and reaps its process.
    pub(crate) fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for OpensslSessionServer {
    /// Stops the fixture server and removes its scratch directory.
    fn drop(&mut self) {
        self.stop();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// Binds and immediately releases a loopback port for the fixture server.
fn reserve_port() -> u16 {
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("openssl TLS server: reserve port");
    listener
        .local_addr()
        .expect("openssl TLS server: local address")
        .port()
}
